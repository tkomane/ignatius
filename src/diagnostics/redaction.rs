//! Central redaction.
//!
//! Every path that renders text which might have come from a connection string,
//! an environment variable, a log line or a process argument goes through here.
//! There is deliberately one implementation: a second one would eventually differ
//! from this one, and the difference would be a credential leak.
//!
//! What this can and cannot do is stated plainly in `docs/security/data-handling.md`.
//! It recognises the shapes PostgreSQL clients actually use - URI userinfo and
//! libpq keyword/value pairs. It cannot recognise a password that appears as bare
//! prose, so callers must never put a bare secret into a message in the first place.

/// Replacement text substituted for every redacted secret.
pub const REDACTED: &str = "[redacted]";

/// Keyword names whose values are treated as secret in keyword/value strings.
const SECRET_KEYWORDS: &[&str] = &[
    "password",
    "pgpassword",
    "sslpassword",
    "sslkeylogfile",
    "passfile",
];

/// Redacts secrets in arbitrary text: connection URIs, keyword/value connection
/// strings, and `KEY=value` pairs that name a secret.
///
/// The result is safe to display, log, copy to the clipboard, or place in a
/// diagnostic bundle.
#[must_use]
pub fn redact_text(input: &str) -> String {
    let mut spans = Vec::new();
    collect_uri_password_spans(input, &mut spans);
    collect_keyword_value_spans(input, &mut spans);
    apply(input, spans)
}

/// Redacts a connection string for display. Alias of [`redact_text`] with a name
/// that documents intent at the call site.
#[must_use]
pub fn redact_connection_string(input: &str) -> String {
    redact_text(input)
}

/// Redacts a list of process arguments, joining them for display.
///
/// Process arguments are visible to every other process on the machine, so this
/// makes a log line safe but does not make the original invocation safe. The
/// guidance for that is in `docs/security/data-handling.md`.
#[must_use]
pub fn redact_arguments<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let joined = args
        .into_iter()
        .map(|a| a.as_ref().to_owned())
        .collect::<Vec<_>>()
        .join(" ");
    redact_text(&joined)
}

/// Finds the password portion of any `scheme://user:password@host` authority.
fn collect_uri_password_spans(input: &str, spans: &mut Vec<(usize, usize)>) {
    let bytes = input.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = input[search_from..].find("://") {
        let authority_start = search_from + rel + 3;
        let authority_end = bytes[authority_start..]
            .iter()
            .position(|b| matches!(b, b'/' | b'?' | b'#' | b' ' | b'\t' | b'\n'))
            .map_or(input.len(), |p| authority_start + p);

        let authority = &input[authority_start..authority_end];
        // Userinfo ends at the last '@' in the authority: a password may itself
        // contain a percent-encoded '@', and hosts never do.
        if let Some(at) = authority.rfind('@') {
            let userinfo = &authority[..at];
            if let Some(colon) = userinfo.find(':') {
                let password_start = authority_start + colon + 1;
                let password_end = authority_start + at;
                spans.push((password_start, password_end));
            }
        }
        search_from = authority_end.max(authority_start);
        if search_from >= input.len() {
            break;
        }
    }
}

/// Finds values of secret-bearing `key=value` pairs, honouring libpq single-quoting.
fn collect_keyword_value_spans(input: &str, spans: &mut Vec<(usize, usize)>) {
    let bytes = input.as_bytes();
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte != b'=' {
            continue;
        }
        let Some(key) = keyword_before(input, idx) else {
            continue;
        };
        if !SECRET_KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(&key)) {
            continue;
        }
        let mut value_start = idx + 1;
        while value_start < bytes.len() && matches!(bytes[value_start], b' ' | b'\t') {
            value_start += 1;
        }
        let value_end = if bytes.get(value_start) == Some(&b'\'') {
            // Single-quoted libpq value: a backslash escapes the next character.
            let mut i = value_start + 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'\'' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            i.min(bytes.len())
        } else {
            bytes[value_start..]
                .iter()
                .position(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'&' | b'"'))
                .map_or(bytes.len(), |p| value_start + p)
        };
        spans.push((value_start, value_end));
    }
}

/// Reads the identifier immediately to the left of an `=`, skipping spaces.
fn keyword_before(input: &str, equals_idx: usize) -> Option<String> {
    let bytes = input.as_bytes();
    let mut end = equals_idx;
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(input[start..end].to_owned())
}

/// Replaces the collected spans with [`REDACTED`], innermost-first and non-overlapping.
fn apply(input: &str, mut spans: Vec<(usize, usize)>) -> String {
    if spans.is_empty() {
        return input.to_owned();
    }
    spans.sort_unstable();
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    for (start, end) in spans {
        if start < cursor || end > input.len() || start > end {
            continue;
        }
        out.push_str(&input[cursor..start]);
        out.push_str(REDACTED);
        cursor = end;
    }
    out.push_str(&input[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake credentials only. Never a real secret, even in a test fixture.
    const SECRET: &str = "hunter2-not-a-real-password";

    #[test]
    fn redacts_uri_password_and_keeps_everything_else() {
        let input =
            format!("postgres://app_user:{SECRET}@db.example.net:5432/orders?sslmode=verify-full");
        let out = redact_text(&input);
        assert!(!out.contains(SECRET), "secret survived: {out}");
        assert!(out.contains("app_user"), "user should stay visible: {out}");
        assert!(out.contains("db.example.net:5432/orders"));
        assert!(out.contains("sslmode=verify-full"));
        assert_eq!(
            out,
            "postgres://app_user:[redacted]@db.example.net:5432/orders?sslmode=verify-full"
        );
    }

    #[test]
    fn redacts_keyword_value_password_forms() {
        for input in [
            format!("host=db user=app password={SECRET} dbname=orders"),
            format!("host=db password = {SECRET}"),
            format!("password='{SECRET}' host=db"),
            format!("PGPASSWORD={SECRET}"),
        ] {
            let out = redact_text(&input);
            assert!(!out.contains(SECRET), "secret survived in {input:?}: {out}");
        }
    }

    #[test]
    fn redacts_quoted_value_containing_spaces_and_escapes() {
        let input = r"host=db password='pa ss\'word here' dbname=orders";
        let out = redact_text(input);
        assert!(!out.contains("pa ss"), "{out}");
        assert!(
            out.contains("dbname=orders"),
            "trailing keywords kept: {out}"
        );
    }

    #[test]
    fn leaves_non_secret_text_untouched() {
        let input = "host=db.example.net port=5432 sslmode=verify-full application_name=ignatius";
        assert_eq!(redact_text(input), input);
    }

    #[test]
    fn handles_uri_without_password() {
        let input = "postgresql://app_user@localhost/orders";
        assert_eq!(redact_text(input), input);
        let input = "postgresql:///orders?host=/var/run/postgresql";
        assert_eq!(redact_text(input), input);
    }

    #[test]
    fn redacts_every_occurrence_in_multiline_text() {
        let input = format!(
            "first postgres://a:{SECRET}@h1/db\nsecond postgres://b:{SECRET}@h2/db\npassword={SECRET}"
        );
        let out = redact_text(&input);
        assert!(!out.contains(SECRET), "{out}");
        assert_eq!(out.matches(REDACTED).count(), 3, "{out}");
    }

    #[test]
    fn redacts_arguments_for_logging() {
        let out = redact_arguments(["ignatius", "query", &format!("postgres://u:{SECRET}@h/db")]);
        assert!(!out.contains(SECRET));
        assert!(out.starts_with("ignatius query "));
    }

    #[test]
    fn adversarial_inputs_never_leak_and_never_panic() {
        // Shapes chosen to break naive parsers: multiple '@', percent-encoding,
        // unterminated quotes, and a nested connection string in a parameter.
        let must_redact = [
            format!("postgres://u:{SECRET}@@host/db"),
            format!("postgres://u:{SECRET}%40x@host/db"),
            format!("postgres://u:{SECRET}@host/db?options=postgres://x:{SECRET}@y/z"),
            format!("password='{SECRET}"),
            format!("password={SECRET}&host=db"),
            format!("  password   =   {SECRET}  "),
            format!("PGPASSWORD={SECRET}"),
        ];
        for input in must_redact {
            let out = redact_text(&input);
            assert!(!out.contains(SECRET), "leaked for {input:?}: {out}");
        }
    }

    #[test]
    fn only_recognised_secret_keywords_are_redacted() {
        // This is the documented boundary of what redaction can promise: it
        // redacts the values of keywords PostgreSQL actually defines as secret,
        // and URI userinfo passwords. A value sitting after some other key is
        // not a password, and blanking it would hide legitimate diagnostics.
        let out = redact_text(&format!("notapassword={SECRET} password={SECRET}"));
        assert_eq!(out, format!("notapassword={SECRET} password={REDACTED}"));

        // A bare userinfo with no colon is a username, not a password.
        let user_only = format!("postgres://{SECRET}@host/db");
        assert_eq!(redact_text(&user_only), user_only);
    }

    #[test]
    fn output_is_always_valid_utf8_for_multibyte_input() {
        let input = format!("postgres://用户:{SECRET}@主机/数据库");
        let out = redact_text(&input);
        assert!(!out.contains(SECRET));
        assert!(out.contains("用户"));
        assert!(out.contains("主机/数据库"));
    }

    #[test]
    fn redaction_is_idempotent() {
        let input = format!("postgres://u:{SECRET}@h/db password={SECRET}");
        let once = redact_text(&input);
        assert_eq!(redact_text(&once), once);
    }
}
