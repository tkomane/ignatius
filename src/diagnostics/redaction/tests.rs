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
