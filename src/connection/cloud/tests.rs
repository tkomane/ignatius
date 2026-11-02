use super::*;
use crate::connection::target::{Host, SslMode};

fn target(sslmode: SslMode) -> ConnectionTarget {
    ConnectionTarget {
        host: Host::Tcp("db.example.com".into()),
        port: 5432,
        database: "orders".into(),
        user: "someone@example.com".into(),
        password: None,
        sslmode,
        root_cert: None,
        client_cert: None,
        client_key: None,
        application_name: "ignatius".into(),
        connect_timeout: Duration::from_secs(5),
        environment: crate::connection::Environment::Local,
        read_only: false,
        notes: Vec::new(),
        auth: Some("entra".into()),
    }
}

/// Builds a command that echoes raw text to stdout across Unix and Windows.
fn echo_raw_command(token: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd.exe".into(), "/C".into(), format!("echo {token}")]
    } else {
        vec!["/bin/echo".into(), token.into()]
    }
}

/// Builds a command that echoes JSON or formatted text to stdout across Unix and Windows.
fn echo_json_command(json: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd.exe".into(), "/C".into(), format!("echo {json}")]
    } else {
        vec!["/bin/echo".into(), json.into()]
    }
}

/// Builds a command that writes text to stderr and exits with a non-zero code.
fn failing_stderr_command(stderr: &str, exit_code: i32) -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd.exe".into(),
            "/C".into(),
            format!("echo {stderr}>&2 & exit {exit_code}"),
        ]
    } else {
        vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("echo '{stderr}' >&2; exit {exit_code}"),
        ]
    }
}

/// Builds a command that outputs credential-shaped stderr and exits 1.
fn credential_leak_command(sentinel: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd.exe".into(),
            "/C".into(),
            format!(
                "echo password={sentinel}>&2 & echo Authorization: Bearer {sentinel}>&2 & exit 1"
            ),
        ]
    } else {
        vec![
            "/bin/sh".into(),
            "-c".into(),
            format!(
                "echo 'password={sentinel}' >&2; echo 'Authorization: Bearer {sentinel}' >&2; exit 1"
            ),
        ]
    }
}

/// Builds a command that exits successfully with empty stdout.
fn empty_stdout_command() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd.exe".into(), "/C".into(), "rem".into()]
    } else {
        vec!["/bin/sh".into(), "-c".into(), ":".into()]
    }
}

/// Builds a command that exits successfully with whitespace-only stdout.
fn whitespace_stdout_command() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd.exe".into(), "/C".into(), "echo.   ".into()]
    } else {
        vec!["/bin/echo".into(), "   \n\t  ".into()]
    }
}

/// Builds a command that sleeps long enough to exceed test timeouts.
fn hanging_command() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd.exe".into(),
            "/C".into(),
            "ping 127.0.0.1 -n 4 > nul".into(),
        ]
    } else {
        vec!["/bin/sleep".into(), "3".into()]
    }
}

/// Builds a command that creates a file to witness execution.
fn touch_command(path: &std::path::Path) -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd.exe".into(),
            "/C".into(),
            format!("type nul > \"{}\"", path.display()),
        ]
    } else {
        vec!["/usr/bin/touch".into(), path.display().to_string()]
    }
}

#[test]
fn the_built_in_providers_are_the_three_clouds_and_each_has_a_command() {
    let names: Vec<String> = built_in().into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["entra", "aws", "gcp"]);
    for provider in built_in() {
        assert!(
            !provider.command.is_empty(),
            "{} has no command",
            provider.name
        );
        assert!(
            !provider.remedy.is_empty(),
            "{} has no remedy to suggest",
            provider.name
        );
    }
}

#[test]
fn provider_presentation_names_the_cloud_and_never_copies_arguments() {
    let entra = built_in()
        .into_iter()
        .find(|provider| provider.name == "entra")
        .expect("built-in Entra provider");
    let view = entra.presentation();
    assert_eq!(view.display_name, "Microsoft Entra ID");
    assert_eq!(view.command_name, "az");
    assert!(view.invocation.contains("OSS RDBMS"));
    assert!(view.lifetime.contains("5-60 minutes"));
    assert!(view.transport.contains("TLS required"));
    assert!(view.extraction.contains("accessToken"));
    assert!(view.remedy.contains("az login"));
    assert!(
        !view.invocation.contains("{host}"),
        "display metadata must not expose substituted arguments"
    );
}

#[test]
fn custom_provider_presentation_hides_arbitrary_arguments() {
    let provider = Provider {
        name: "corp".into(),
        command: vec!["corp-token".into(), "--secret-value".into()],
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "run corp login".into(),
    };
    let view = provider.presentation();
    assert!(view.display_name.contains("corp"));
    assert_eq!(view.command_name, "corp-token");
    assert!(view.invocation.contains("arguments hidden"));
    assert!(!view.invocation.contains("secret-value"));
    assert_eq!(view.remedy, "run corp login");
}

#[test]
fn every_built_in_cloud_has_a_named_route_and_lifetime() {
    for (name, display, command, lifetime) in [
        (
            "entra",
            "Microsoft Entra ID",
            "az account get-access-token",
            "5-60 minutes",
        ),
        (
            "aws",
            "AWS IAM database authentication",
            "aws rds generate-db-auth-token",
            "15 minutes",
        ),
        (
            "gcp",
            "Google Cloud SQL IAM",
            "gcloud sql generate-login-token",
            "about 1 hour",
        ),
    ] {
        let provider = built_in()
            .into_iter()
            .find(|provider| provider.name == name)
            .expect("built-in provider");
        let view = provider.presentation();
        assert_eq!(view.display_name, display);
        assert!(view.invocation.contains(command), "{name}: {view:?}");
        assert!(view.lifetime.contains(lifetime), "{name}: {view:?}");
        assert!(view.transport.contains("TLS required"), "{name}: {view:?}");
    }
}

#[test]
fn configuration_replaces_a_built_in_rather_than_being_ignored() {
    let mut configured = BTreeMap::new();
    configured.insert(
        "entra".to_owned(),
        Provider {
            name: "entra".into(),
            command: vec!["mine".into()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "mine".into(),
        },
    );
    let all = registry(&configured);
    assert_eq!(all["entra"].command, vec!["mine".to_owned()]);
    assert!(all.contains_key("aws"), "the others are still there");
}

#[test]
fn an_unknown_provider_is_refused_with_the_names_that_exist() {
    let error = find(&registry(&BTreeMap::new()), "azure").expect_err("unknown");
    let text = format!("{error:?}");
    assert!(text.contains("entra"), "it lists what exists: {text}");
    assert_eq!(error.kind, DiagnosticKind::Config);
}

#[test]
fn a_transport_that_might_be_unencrypted_is_refused_before_anything_runs() {
    for mode in [SslMode::Disable, SslMode::Prefer] {
        let error = require_encryption(&target(mode), "entra").expect_err("must refuse {mode:?}");
        assert_eq!(error.kind, DiagnosticKind::Config);
    }
    for mode in [SslMode::Require, SslMode::VerifyCa, SslMode::VerifyFull] {
        assert!(
            require_encryption(&target(mode), "entra").is_ok(),
            "{mode:?}"
        );
    }
}

#[test]
fn the_targets_own_facts_are_substituted_as_whole_arguments() {
    let aws = built_in().into_iter().find(|p| p.name == "aws").unwrap();
    let (program, arguments) = command_for(&aws, &target(SslMode::VerifyFull)).expect("a command");
    assert_eq!(program, "aws");
    assert!(arguments.contains(&"db.example.com".to_owned()));
    assert!(arguments.contains(&"5432".to_owned()));
    assert!(arguments.contains(&"someone@example.com".to_owned()));
    assert!(
        !arguments.iter().any(|a| a.contains('{')),
        "nothing is left unsubstituted: {arguments:?}"
    );
}

#[test]
fn a_hostile_database_name_is_one_argument_and_not_a_command() {
    let provider = Provider {
        name: "test".into(),
        command: vec!["tool".into(), "--db".into(), "{database}".into()],
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: String::new(),
    };
    let mut hostile = target(SslMode::VerifyFull);
    hostile.database = "x; rm -rf /".into();
    let (_, arguments) = command_for(&provider, &hostile).expect("a command");
    assert_eq!(
        arguments,
        vec!["--db".to_owned(), "x; rm -rf /".to_owned()],
        "no shell means no splitting, so it stays one argument"
    );
}

#[tokio::test]
async fn a_program_that_does_not_exist_is_a_missing_prerequisite_not_a_refusal() {
    let provider = Provider {
        name: "test".into(),
        command: vec!["ignatius-no-such-program-exists".into()],
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "install it".into(),
    };
    let error = fetch(&provider, &target(SslMode::VerifyFull))
        .await
        .expect_err("no such program");
    assert_eq!(
        error.kind,
        DiagnosticKind::Config,
        "a missing tool is configuration, not a rejected credential"
    );
}

#[tokio::test]
async fn a_raw_token_is_read_from_standard_output() {
    let provider = Provider {
        name: "test".into(),
        command: echo_raw_command("  a-token  "),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: String::new(),
    };
    let secret = fetch(&provider, &target(SslMode::Require))
        .await
        .expect("a token");
    assert_eq!(secrecy::ExposeSecret::expose_secret(&secret), "a-token");
}

#[tokio::test]
async fn a_json_token_is_read_from_its_field() {
    let provider = Provider {
        name: "test".into(),
        command: echo_json_command(r#"{"accessToken":"from-json","expiresOn":"later"}"#),
        extract: Extract::JsonField("accessToken".into()),
        timeout: DEFAULT_TIMEOUT,
        remedy: String::new(),
    };
    let secret = fetch(&provider, &target(SslMode::Require))
        .await
        .expect("a token");
    assert_eq!(secrecy::ExposeSecret::expose_secret(&secret), "from-json");
}

#[tokio::test]
async fn a_failing_program_is_an_authentication_failure_carrying_its_own_reason() {
    let provider = Provider {
        name: "test".into(),
        command: failing_stderr_command("please run: log in", 1),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "sign in".into(),
    };
    let error = fetch(&provider, &target(SslMode::Require))
        .await
        .expect_err("it failed");
    assert_eq!(error.kind, DiagnosticKind::Authentication);
    let text = format!("{error:?}");
    assert!(
        text.contains("log in"),
        "the tool's reason survives: {text}"
    );
}

#[tokio::test]
async fn a_provider_that_echoes_a_credential_shape_never_leaks_it_into_the_diagnostic() {
    const SENTINEL: &str = "w04-sentinel-4f2a";
    let provider = Provider {
        name: "test".into(),
        command: credential_leak_command(SENTINEL),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "sign in".into(),
    };
    let error = fetch(&provider, &target(SslMode::Require))
        .await
        .expect_err("it failed");
    assert_eq!(error.kind, DiagnosticKind::Authentication);

    let mut displayable = vec![error.headline.clone(), error.attempted.clone()];
    if let Some(cause) = &error.likely_cause {
        displayable.push(cause.clone());
    }
    if let Some(action) = &error.next_action {
        displayable.push(action.clone());
    }
    for field in &error.technical {
        displayable.push(field.label.clone());
        displayable.push(field.value.clone());
    }
    for text in displayable {
        assert!(
            !text.contains(SENTINEL),
            "provider stderr leaked the sentinel into {text:?}"
        );
    }
    let cause = error.likely_cause.as_deref().unwrap_or_default();
    assert!(
        cause.contains("[redacted]"),
        "the provider's reason should survive redaction rather than be dropped: {cause:?}"
    );
}

#[tokio::test]
async fn a_provider_that_never_answers_times_out_without_reading_its_output() {
    let provider = Provider {
        name: "test".into(),
        command: hanging_command(),
        extract: Extract::Raw,
        timeout: Duration::from_secs(1),
        remedy: "sign in".into(),
    };
    let error = fetch(&provider, &target(SslMode::Require))
        .await
        .expect_err("it hung");
    assert_eq!(
        error.kind,
        DiagnosticKind::Connection,
        "a timeout is a connection failure, not a rejected credential"
    );
    assert!(
        error.headline.contains("did not answer within 1 seconds"),
        "the provider's own timeout drives the path: {}",
        error.headline
    );
}

#[tokio::test]
async fn nothing_is_run_at_all_when_the_transport_would_allow_plain_text() {
    let witness = std::env::temp_dir().join(format!(
        "ignatius-provider-witness-{}-{}",
        std::process::id(),
        "plain"
    ));
    let _ = std::fs::remove_file(&witness);
    let provider = Provider {
        name: "test".into(),
        command: touch_command(&witness),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: String::new(),
    };
    let error = fetch(&provider, &target(SslMode::Prefer))
        .await
        .expect_err("it must refuse");
    assert_eq!(error.kind, DiagnosticKind::Config);
    assert!(
        !witness.exists(),
        "the provider's command ran despite the transport being refused"
    );
}

#[tokio::test]
async fn w06_malformed_json_and_html_error_pages_on_stdout_are_rejected_without_leaking_payload() {
    const HTML_SENTINEL: &str = "w06-html-secret-sentinel-8472";
    const JSON_SENTINEL: &str = "w06-json-secret-sentinel-1934";

    // Scenario A: Provider expecting JSON receives an HTML 502 Bad Gateway error page on stdout
    let html_provider = Provider {
        name: "entra".into(),
        command: echo_json_command(&format!(
            "<html><body>502 Bad Gateway: Bearer {HTML_SENTINEL}</body></html>"
        )),
        extract: Extract::JsonField("accessToken".into()),
        timeout: DEFAULT_TIMEOUT,
        remedy: "check cloud sign-in".into(),
    };
    let html_err = fetch(&html_provider, &target(SslMode::Require))
        .await
        .expect_err("HTML error page must be rejected when JSON expected");
    assert_eq!(html_err.kind, DiagnosticKind::Authentication);
    let html_debug = format!("{html_err:?}");
    assert!(
        !html_debug.contains(HTML_SENTINEL),
        "sentinel leaked into diagnostic: {html_debug}"
    );

    // Scenario B: Provider expecting JSON receives a multiline JSON error object missing accessToken
    let json_err_provider = Provider {
        name: "entra".into(),
        command: echo_json_command(&format!(
            "{{\"error\":\"gateway_error\",\"details\":\"Bearer {JSON_SENTINEL}\"}}"
        )),
        extract: Extract::JsonField("accessToken".into()),
        timeout: DEFAULT_TIMEOUT,
        remedy: "check cloud sign-in".into(),
    };
    let json_err = fetch(&json_err_provider, &target(SslMode::Require))
        .await
        .expect_err("JSON error object without target field must be rejected");
    assert_eq!(json_err.kind, DiagnosticKind::Authentication);
    let json_debug = format!("{json_err:?}");
    assert!(
        !json_debug.contains(JSON_SENTINEL),
        "sentinel leaked into diagnostic: {json_debug}"
    );

    // Scenario C: Provider with Extract::Raw receives an HTML error page on stdout
    let raw_html_provider = Provider {
        name: "aws".into(),
        command: echo_raw_command(&format!(
            "<html><body>502 Bad Gateway: Bearer {HTML_SENTINEL}</body></html>"
        )),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "check cloud sign-in".into(),
    };
    let raw_html_err = fetch(&raw_html_provider, &target(SslMode::Require))
        .await
        .expect_err("HTML payload must be rejected as invalid credential");
    assert_eq!(raw_html_err.kind, DiagnosticKind::Authentication);
    let raw_debug = format!("{raw_html_err:?}");
    assert!(
        !raw_debug.contains(HTML_SENTINEL),
        "sentinel leaked into diagnostic: {raw_debug}"
    );
}

#[tokio::test]
async fn w06_json_and_url_credentials_in_stderr_are_redacted_while_preserving_exit_code() {
    const URL_SENTINEL: &str = "w06-url-secret-sentinel-5821";
    const BEARER_SENTINEL: &str = "w06-bearer-secret-sentinel-9314";

    let stderr = format!(
        "{{\"error\":\"forbidden\",\"url\":\"https://client:{URL_SENTINEL}@auth.cloud.example.com/v1/token\",\"details\":\"Authorization: Bearer {BEARER_SENTINEL}\"}}"
    );
    let provider = Provider {
        name: "custom-auth".into(),
        command: failing_stderr_command(&stderr, 1),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "re-authenticate with identity provider".into(),
    };
    let error = fetch(&provider, &target(SslMode::Require))
        .await
        .expect_err("failing command must produce error");
    assert_eq!(error.kind, DiagnosticKind::Authentication);

    // Verify secret sentinels are completely redacted
    let debug_text = format!("{error:?}");
    assert!(
        !debug_text.contains(URL_SENTINEL),
        "url credential sentinel leaked: {debug_text}"
    );
    assert!(
        !debug_text.contains(BEARER_SENTINEL),
        "bearer token sentinel leaked: {debug_text}"
    );

    // Verify redacted placeholder exists
    let cause = error.likely_cause.as_deref().unwrap_or_default();
    assert!(
        cause.contains("[redacted]"),
        "stderr reason should survive with redaction: {cause}"
    );

    // Verify command program name and exit status are preserved
    let program = &provider.command[0];
    assert!(
        error.headline.contains(program),
        "program name must be preserved in headline: {}",
        error.headline
    );
    assert!(
        error
            .technical
            .iter()
            .any(|field| field.label == "Program" && &field.value == program),
        "program technical field must be present"
    );
    assert!(
        error.technical.iter().any(|field| {
            field.label == "Exit status"
                && (field.value.contains('1') || field.value.contains("exit"))
        }),
        "exit status must be preserved in technical fields: {:?}",
        error.technical
    );
}

#[tokio::test]
async fn w06_cross_platform_test_binary_invocation_rejects_multiline_output_cleanly() {
    let current_exe = std::env::current_exe().expect("current test executable path");
    let provider = Provider {
        name: "test-multiline-exe".into(),
        command: vec![current_exe.display().to_string(), "--list".into()],
        extract: Extract::Raw,
        timeout: Duration::from_secs(10),
        remedy: "provide a single-line bearer token".into(),
    };

    let error = fetch(&provider, &target(SslMode::Require))
        .await
        .expect_err("multiline output from --list must be rejected");
    assert_eq!(error.kind, DiagnosticKind::Authentication);
    assert!(
        error.headline.contains("invalid multiline credential"),
        "headline must indicate multiline rejection: {}",
        error.headline
    );
    let cause = error.likely_cause.as_deref().unwrap_or_default();
    assert!(
        cause.contains("multiline output"),
        "cause must explain multiline rejection: {cause}"
    );
}

#[tokio::test]
async fn w06_empty_and_whitespace_only_stdout_are_rejected_without_panics() {
    // Empty stdout rejection
    let empty_provider = Provider {
        name: "empty-test".into(),
        command: empty_stdout_command(),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "check token command".into(),
    };
    let empty_err = fetch(&empty_provider, &target(SslMode::Require))
        .await
        .expect_err("empty output must be rejected");
    assert_eq!(empty_err.kind, DiagnosticKind::Authentication);
    assert!(
        empty_err.headline.contains("produced no credential"),
        "headline must report missing credential: {}",
        empty_err.headline
    );

    // Whitespace-only stdout rejection
    let ws_provider = Provider {
        name: "ws-test".into(),
        command: whitespace_stdout_command(),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: "check token command".into(),
    };
    let ws_err = fetch(&ws_provider, &target(SslMode::Require))
        .await
        .expect_err("whitespace output must be rejected");
    assert_eq!(ws_err.kind, DiagnosticKind::Authentication);
    assert!(
        ws_err.headline.contains("produced no credential"),
        "headline must report missing credential: {}",
        ws_err.headline
    );
}

#[tokio::test]
async fn w06_crlf_line_endings_are_trimmed_cleanly_from_token() {
    let provider = Provider {
        name: "crlf-test".into(),
        command: echo_raw_command("synthetic-token-from-command"),
        extract: Extract::Raw,
        timeout: DEFAULT_TIMEOUT,
        remedy: String::new(),
    };
    let secret = fetch(&provider, &target(SslMode::Require))
        .await
        .expect("token with line ending must be cleanly trimmed and accepted");
    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&secret),
        "synthetic-token-from-command"
    );
}
