//! Central redaction.
//!
//! Every path that renders text which might have come from a connection string,
//! an environment variable, a log line or a process argument goes through here.
//! There is deliberately one implementation: a second one would eventually differ
//! from this one, and the difference would be a credential leak.
//!
//! What this can and cannot do is stated plainly in `docs/security/data-handling.md`.
//! It recognises the shapes PostgreSQL clients and cloud provider tools actually
//! use - URI userinfo, libpq keyword/value pairs, HTTP authorization header values
//! and bare `Bearer` tokens. It cannot recognise a password that appears as bare
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
    collect_authorization_spans(input, &mut spans);
    collect_bearer_spans(input, &mut spans);
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

/// Header names whose value carries or contains a credential.
const AUTHORIZATION_HEADERS: &[&str] = &["authorization", "proxy-authorization"];

/// Schemes whose own name is not secret, so only what follows is removed.
const AUTHORIZATION_SCHEMES: &[&str] = &["bearer", "basic", "digest", "negotiate", "ntlm"];

/// Shortest bare `Bearer` value treated as a credential, and the floor for an
/// unrecognised header value. Real tokens are long; the floor keeps ordinary
/// prose such as "the bearer of good news" untouched.
const MIN_BARE_BEARER_TOKEN: usize = 16;

/// Finds the credential in an inline HTTP-style authorization header, which a
/// provider tool may print when a request fails. Quoted and repr-style keys and
/// values are unwrapped first. A known scheme stays visible; anything else that
/// looks like a credential has its whole value removed.
fn collect_authorization_spans(input: &str, spans: &mut Vec<(usize, usize)>) {
    let bytes = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    for header in AUTHORIZATION_HEADERS {
        let mut search_from = 0usize;
        while let Some(relative) = lower[search_from..].find(header) {
            let header_start = search_from + relative;
            let header_end = header_start + header.len();
            let boundary = header_start == 0
                || !(bytes[header_start - 1].is_ascii_alphanumeric()
                    || bytes[header_start - 1] == b'_'
                    || bytes[header_start - 1] == b'-');
            if !boundary {
                search_from = header_end;
                continue;
            }
            let mut cursor = header_end;
            skip_spaces(bytes, &mut cursor);
            // A JSON or repr key may close its quote before the colon.
            if bytes.get(cursor) == Some(&b'"') || bytes.get(cursor) == Some(&b'\'') {
                cursor += 1;
                skip_spaces(bytes, &mut cursor);
            }
            if bytes.get(cursor) != Some(&b':') {
                search_from = header_end;
                continue;
            }
            cursor += 1;
            skip_spaces(bytes, &mut cursor);
            let (value_start, value_end) =
                if bytes.get(cursor) == Some(&b'"') || bytes.get(cursor) == Some(&b'\'') {
                    let quote = bytes[cursor];
                    let start = cursor + 1;
                    let end = bytes[start..]
                        .iter()
                        .position(|b| *b == quote)
                        .map_or_else(|| line_end(bytes, start), |p| start + p);
                    (start, end)
                } else if cursor < bytes.len() {
                    (cursor, line_end(bytes, cursor))
                } else {
                    search_from = header_end;
                    continue;
                };
            let first_end = credential_run_end(bytes, value_start).min(value_end);
            let known_scheme = AUTHORIZATION_SCHEMES
                .iter()
                .any(|scheme| lower[value_start..first_end].eq_ignore_ascii_case(scheme));
            if known_scheme {
                cursor = first_end;
                skip_spaces(bytes, &mut cursor);
                if cursor < value_end {
                    spans.push((cursor, value_end));
                }
            } else if first_end > value_start && credential_shaped(bytes, value_start, value_end) {
                spans.push((value_start, value_end));
            }
            search_from = value_end.max(header_end);
        }
    }
}

/// Whether an unrecognised header value looks like a credential rather than a
/// word of prose: one token at least as long as a bearer token, or a string
/// carrying the `=` an AWS-style credential has.
fn credential_shaped(bytes: &[u8], start: usize, end: usize) -> bool {
    if bytes[start..end].contains(&b'=') {
        return true;
    }
    let first_end = credential_run_end(bytes, start).min(end);
    first_end == end && end.saturating_sub(start) >= MIN_BARE_BEARER_TOKEN
}

/// Finds a bare `Bearer` credential, which a provider may print without the
/// header name when it reformats or truncates its output.
fn collect_bearer_spans(input: &str, spans: &mut Vec<(usize, usize)>) {
    const KEYWORD: &str = "bearer";
    let bytes = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(relative) = lower[search_from..].find(KEYWORD) {
        let keyword_start = search_from + relative;
        let keyword_end = keyword_start + KEYWORD.len();
        let boundary = keyword_start == 0 || !bytes[keyword_start - 1].is_ascii_alphanumeric();
        let mut cursor = keyword_end;
        skip_spaces(bytes, &mut cursor);
        let credential_start = cursor;
        let credential_end = credential_run_end(bytes, credential_start);
        if boundary && credential_end.saturating_sub(credential_start) >= MIN_BARE_BEARER_TOKEN {
            spans.push((credential_start, credential_end));
        }
        search_from = credential_end.max(keyword_end);
    }
}

/// Advances past spaces and tabs.
fn skip_spaces(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && matches!(bytes[*cursor], b' ' | b'\t') {
        *cursor += 1;
    }
}

/// The end of one credential-shaped run. A token ends at whitespace, quotes,
/// commas, semicolons or brackets; dots and `=` stay inside, as JWTs and base64
/// padding use them.
fn credential_run_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|b| {
            matches!(
                b,
                b' ' | b'\t'
                    | b'\n'
                    | b'\r'
                    | b'"'
                    | b'\''
                    | b','
                    | b';'
                    | b'<'
                    | b'>'
                    | b'('
                    | b')'
            )
        })
        .map_or(bytes.len(), |p| start + p)
}

/// The end of the header value: the line break, or the end of the input.
fn line_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|b| matches!(b, b'\n' | b'\r'))
        .map_or(bytes.len(), |p| start + p)
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

/// Merges overlapping spans and replaces each region with [`REDACTED`], from
/// left to right. Merging matters because one credential can match two shapes,
/// and replacing only the shorter match would leave its tail behind.
fn apply(input: &str, spans: Vec<(usize, usize)>) -> String {
    let mut sorted: Vec<(usize, usize)> = spans
        .into_iter()
        .filter(|(start, end)| start < end && *end <= input.len())
        .collect();
    if sorted.is_empty() {
        return input.to_owned();
    }
    sorted.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(sorted.len());
    for (start, end) in sorted {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;
    for (start, end) in merged {
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
    fn redacts_authorization_headers_and_keeps_the_scheme() {
        let out = redact_text(&format!("Authorization: Bearer {SECRET}"));
        assert_eq!(out, format!("Authorization: Bearer {REDACTED}"));

        for input in [
            format!("authorization: bearer {SECRET}"),
            format!("Proxy-Authorization: Basic {SECRET}="),
            format!("Authorization: {SECRET}"),
            format!("request failed: Authorization: Bearer {SECRET} (expired)"),
            format!("Authorization: Digest username=\"x\", response=\"{SECRET}\""),
        ] {
            let out = redact_text(&input);
            assert!(
                !out.contains(SECRET),
                "credential survived in {input:?}: {out}"
            );
        }
    }

    #[test]
    fn redacts_a_bare_bearer_token_but_not_the_word() {
        let out = redact_text(&format!("the tool said Bearer {SECRET} expired"));
        assert!(!out.contains(SECRET), "{out}");

        // The shape needs a long credential; ordinary prose stays readable.
        for prose in [
            "the bearer of good news",
            "Bearer",
            "Basic configuration is fine",
            "the authorization: process returned an error",
            "authorization: failed",
        ] {
            assert_eq!(redact_text(prose), prose);
        }
    }

    #[test]
    fn redacts_quoted_and_repr_authorization_values() {
        for input in [
            format!("Authorization: \"Basic {SECRET}\""),
            format!(
                "'Authorization': 'AWS4-HMAC-SHA256 Credential={SECRET}/20260911, SignedHeaders=host'"
            ),
            format!("{{'Authorization': 'Basic {SECRET}'}}"),
        ] {
            let out = redact_text(&input);
            assert!(
                !out.contains(SECRET),
                "credential survived in {input:?}: {out}"
            );
        }
    }

    #[test]
    fn overlapping_authorization_and_bearer_spans_redact_the_whole_value() {
        let tail = "bbbbbbbbbbbbbbbb";
        let out = redact_text(&format!("Authorization: Bearer aaaaaaaaaaaaaaaa,{tail}"));
        assert_eq!(out, format!("Authorization: Bearer {REDACTED}"), "{out}");
        assert!(!out.contains(tail), "{out}");
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
