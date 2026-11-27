use super::*;
use crate::connection::target::{Endpoint, Host, SslMode, TargetSessionAttrs};

fn target(sslmode: SslMode) -> ConnectionTarget {
    let host = Host::Tcp("db.example.com".into());
    ConnectionTarget {
        host: host.clone(),
        port: 5432,
        endpoints: vec![Endpoint { host, port: 5432 }],
        database: "orders".into(),
        user: "someone@example.com".into(),
        password: None,
        sslmode,
        target_session_attrs: TargetSessionAttrs::Any,
        root_cert: None,
        client_cert: None,
        client_key: None,
        application_name: "ignatius".into(),
        connect_timeout: Duration::from_secs(5),
        statement_timeout: None,
        lock_timeout: None,
        environment: crate::connection::Environment::Local,
        read_only: false,
        notes: Vec::new(),
        auth: Some("entra".into()),
        passfile: None,
        explicit_password: false,
    }
}

/// Builds a command that echoes raw text to stdout across Unix and Windows.
fn echo_raw_command(token: &str) -> Vec<String> {
    if cfg!(windows) {
        let escaped = token.replace('\'', "''");
        vec![
            "powershell.exe".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            format!("[Console]::WriteLine('{escaped}')"),
        ]
    } else {
        vec!["/bin/echo".into(), token.into()]
    }
}

/// Builds a command that echoes JSON or formatted text to stdout across Unix and Windows.
fn echo_json_command(json: &str) -> Vec<String> {
    if cfg!(windows) {
        let escaped = json.replace('\'', "''");
        vec![
            "powershell.exe".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            format!("[Console]::WriteLine('{escaped}')"),
        ]
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
    let presentations = presentation_registry(&BTreeMap::new());
    let view = &presentations["entra"];
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
    let presentations = presentation_registry(&BTreeMap::new());
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
        let view = &presentations[name];
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
fn presentation_registry_tracks_configured_origin_by_exact_key() {
    use crate::config::schema::ProviderConfig;

    let stock = registry(&BTreeMap::new())
        .remove("entra")
        .expect("stock Entra");
    let definition = ProviderConfig {
        command: stock.command.clone(),
        json_field: Some("accessToken".to_owned()),
        timeout_seconds: Some(stock.timeout.as_secs()),
        remedy: Some(stock.remedy.clone()),
    };
    let mut config = crate::config::Config::default();
    config
        .auth
        .providers
        .insert("Entra".to_owned(), definition.clone());

    let configured = crate::cli::auth_providers(&config).expect("validated alias");
    let views = presentation_registry(&configured);
    assert_eq!(views["entra"].display_name, "Microsoft Entra ID");
    assert_eq!(views["Entra"].name, "Entra");
    assert_eq!(
        views["Entra"].display_name,
        r#"Configured provider "Entra""#
    );

    config.auth.providers.insert("entra".to_owned(), definition);
    let configured = crate::cli::auth_providers(&config).expect("validated override and alias");
    assert_eq!(
        configured["entra"], stock,
        "the override copies the stock definition"
    );
    let views = presentation_registry(&configured);
    let overridden = &views["entra"];
    assert_eq!(overridden.name, "entra");
    assert_eq!(overridden.display_name, r#"Configured provider "entra""#);
    assert_eq!(overridden.command_name, "az");
    assert_eq!(
        overridden.invocation,
        "configured provider command (arguments hidden)"
    );
    assert_eq!(overridden.extraction, r#"JSON field "accessToken""#);
    assert_eq!(
        overridden.lifetime,
        "short-lived credential, lifetime defined by the provider"
    );
    assert_eq!(overridden.remedy, stock.remedy);
    assert_eq!(stock.presentation(), *overridden);

    let aws = &views["aws"];
    assert_eq!(aws.display_name, "AWS IAM database authentication");
    assert_eq!(aws.extraction, "trimmed standard output");
    assert!(aws.remedy.contains("aws sts get-caller-identity"));
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
async fn a_provider_stream_is_retained_only_if_complete_and_drained_through_eof() {
    assert_eq!((STDOUT_LIMIT, STDERR_LIMIT), (1_048_576, 65_536));
    for text in ["", "abc", "abcd", "abcde"] {
        let retained = read_complete(&mut text.as_bytes(), 4).await.unwrap();
        assert_eq!(
            retained.as_deref(),
            (text.len() <= 4).then_some(text.as_bytes())
        );
    }

    let mut tail = b"more text".as_slice();
    let mut reader = b"abcd".as_slice().chain(b"e".as_slice()).chain(&mut tail);
    assert!(read_complete(&mut reader, 4).await.unwrap().is_none());
    assert!(
        tail.is_empty(),
        "bytes after the overflowing read must also be consumed"
    );
}

#[tokio::test]
async fn an_interrupted_provider_read_resumes_and_retains_the_complete_stream() {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::ReadBuf;

    struct InterruptedOnce {
        interrupted: bool,
        bytes: &'static [u8],
    }

    impl AsyncRead for InterruptedOnce {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if !self.interrupted {
                self.interrupted = true;
                return Poll::Ready(Err(io::ErrorKind::Interrupted.into()));
            }
            Pin::new(&mut self.bytes).poll_read(cx, buf)
        }
    }

    let mut reader = InterruptedOnce {
        interrupted: false,
        bytes: b"sample",
    };
    let retained = read_complete(&mut reader, 6).await.unwrap();
    assert_eq!(retained.as_deref(), Some(b"sample".as_slice()));
}

#[test]
fn complete_stdout_can_supply_a_credential_after_stderr_overflows() {
    for provider in built_in() {
        let stdout = match &provider.extract {
            Extract::Raw => b"sample".to_vec(),
            Extract::JsonField(field) => format!(r#"{{"{field}":"sample"}}"#).into_bytes(),
        };
        let result = credential_from_output(
            &provider,
            "fixture",
            ProviderOutput {
                status: ExitStatus::default(),
                stdout: Some(stdout),
                stderr: None,
            },
        )
        .unwrap();
        assert_eq!(secrecy::ExposeSecret::expose_secret(&result), "sample");

        let error = credential_from_output(
            &provider,
            "fixture",
            ProviderOutput {
                status: ExitStatus::default(),
                stdout: None,
                stderr: Some(b"sample warning".to_vec()),
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Authentication);
        assert!(error.headline.contains("1 MiB"));
        assert!(!format!("{error:?}").contains("sample warning"));
    }
}

#[test]
fn a_failed_provider_keeps_its_exit_status_and_remedy_after_output_overflow() {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    let status = ExitStatus::from_raw(256);
    let provider = built_in().remove(0);
    let error = credential_from_output(
        &provider,
        "fixture",
        ProviderOutput {
            status,
            stdout: None,
            stderr: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, DiagnosticKind::Authentication);
    assert_eq!(error.headline, "fixture could not provide a credential");
    assert_eq!(error.next_action.as_ref(), Some(&provider.remedy));
    assert_eq!(
        error.likely_cause.as_deref(),
        Some("provider stderr exceeded 64 KiB and was discarded")
    );
    assert!(
        error
            .technical
            .iter()
            .any(|f| f.label == "Exit status" && f.value == status.to_string())
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

#[test]
fn only_the_registry_supplied_entra_definition_has_an_msi_launch_route() {
    let configured = BTreeMap::new();
    let (stock, launch) = resolve_provider("entra", &configured).unwrap();
    assert_eq!(launch, Launch::BuiltInEntra);
    for name in ["aws", "gcp"] {
        assert_eq!(
            resolve_provider(name, &configured).unwrap().1,
            Launch::Direct
        );
    }
    let configured = BTreeMap::from([("entra".into(), stock.clone()), ("Entra".into(), stock)]);
    for name in ["entra", "Entra"] {
        assert_eq!(
            resolve_provider(name, &configured).unwrap().1,
            Launch::Direct
        );
    }
}

#[tokio::test]
async fn a_missing_configured_program_keeps_its_diagnostic_at_zero_timeout() {
    let directory = tempfile::tempdir().expect("test directory");
    let program = directory.path().join("absent-provider.exe");
    let provider = Provider {
        name: "configured".into(),
        command: vec![program.to_str().expect("fixture path").to_owned()],
        extract: Extract::Raw,
        timeout: Duration::ZERO,
        remedy: String::new(),
    };

    let error = fetch(&provider, &target(SslMode::Require))
        .await
        .unwrap_err();
    assert_eq!(error.kind, DiagnosticKind::Config);
    assert!(
        error
            .headline
            .ends_with("is not installed, or is not on the path")
    );
}

#[test]
fn package_command_prefix_and_child_environment_preserve_the_argument_elements() {
    use std::ffi::OsStr;

    let python = PathBuf::from("fixture directory").join("python.exe");
    let arguments = ["version", "--output", "one & literal % argument"].map(str::to_owned);
    let command = package_command(python.clone(), &arguments);
    let command = command.as_std();
    assert_eq!(command.get_program(), python.as_os_str());
    assert!(
        command.get_args().eq([
            "-IBm",
            "azure.cli",
            "version",
            "--output",
            "one & literal % argument"
        ]
        .map(OsStr::new))
    );
    assert!(
        command
            .get_envs()
            .eq([(OsStr::new("AZ_INSTALLER"), Some(OsStr::new("MSI")))])
    );
}

#[tokio::test]
async fn missing_native_builtin_can_start_exactly_one_package_child() {
    let directory = tempfile::tempdir().expect("test directory");
    let missing = directory.path().join("absent-native.exe");
    let mut lookups = 0;
    let child = spawn_provider(
        missing.to_str().expect("fixture path"),
        &[],
        Launch::BuiltInEntra,
        || false,
        || {
            lookups += 1;
            std::env::current_exe().map(Some)
        },
    )
    .expect("the built-in should start the supplied package executable");
    // The substitute interpreter is this test binary; libtest rejects Python's
    // flags. Its nonzero exit is the final outcome of a successfully started child.
    assert!(
        !collect_provider_output(child)
            .await
            .expect("child output")
            .status
            .success()
    );
    assert_eq!(lookups, 1);
}

#[tokio::test]
async fn an_expired_budget_never_starts_the_package() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("absent-native.exe");
    let missing = missing.to_str().unwrap();
    let expired = std::cell::Cell::new(false);
    let result = spawn_provider(
        missing,
        &[],
        Launch::BuiltInEntra,
        || expired.get(),
        || {
            expired.set(true);
            std::env::current_exe().map(Some)
        },
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert!(expired.get(), "the simulated lookup must have run");
    let result = spawn_provider(
        missing,
        &[],
        Launch::BuiltInEntra,
        || true,
        || panic!("a budget already exhausted at native spawn needs no lookup"),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
}

#[tokio::test]
async fn started_denied_and_configured_commands_never_consult_the_msi() {
    let never = || panic!("the package lookup must not run");
    let executable = std::env::current_exe().unwrap();
    for (args, success) in [("--list", true), ("--invalid-libtest-option", false)] {
        let child = spawn_provider(
            executable.to_str().unwrap(),
            &[args.to_owned()],
            Launch::BuiltInEntra,
            || false,
            never,
        )
        .unwrap();
        assert_eq!(
            child.wait_with_output().await.unwrap().status.success(),
            success
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid-native.exe");
    let absent =
        spawn_provider(path.to_str().unwrap(), &[], Launch::Direct, || false, never).unwrap_err();
    assert_eq!(absent.kind(), io::ErrorKind::NotFound);
    std::fs::write(&path, b"not an executable").unwrap();
    let denied = spawn_provider(
        path.to_str().unwrap(),
        &[],
        Launch::BuiltInEntra,
        || false,
        never,
    )
    .unwrap_err();
    assert_ne!(denied.kind(), io::ErrorKind::NotFound);
}

/// The CI helper starts this test once per route with a private Azure CLI
/// profile. Only the installed CLI's version is requested or compared.
#[cfg(all(windows, target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "requires scripts/check-windows-azure-msi.ps1 and the x64 MSI"]
async fn installed_windows_azure_msi_version() {
    use secrecy::ExposeSecret;

    let route = std::env::var("IGNATIUS_TEST_AZ_ROUTE").expect("MSI probe route");
    let expected = std::env::var("IGNATIUS_TEST_AZ_VERSION").expect("registered MSI version");
    let (mut command, launch) = match route.as_str() {
        "bare" => (vec!["az".to_owned()], Launch::Direct),
        "package" => (
            vec![
                std::env::var("IGNATIUS_TEST_AZ_PYTHON").expect("MSI package interpreter"),
                "-IBm".to_owned(),
                "azure.cli".to_owned(),
            ],
            Launch::Direct,
        ),
        "resolved" => {
            let (mut entra, launch) =
                resolve_provider("entra", &BTreeMap::new()).expect("built-in Entra");
            // Retain the shipping executable and launch policy. Substitute
            // only the account-free command tail and output field.
            entra.command.truncate(1);
            (entra.command, launch)
        }
        _ => panic!("unsupported MSI probe route"),
    };
    command.extend(["version", "--output", "json"].map(str::to_owned));
    let probe = Provider {
        name: "msi-version-probe".to_owned(),
        command,
        extract: Extract::JsonField("azure-cli".to_owned()),
        timeout: DEFAULT_TIMEOUT,
        remedy: String::new(),
    };
    let outcome = match fetch_selected(&probe, &target(SslMode::VerifyFull), launch).await {
        Ok(version) => {
            assert!(
                version.expose_secret() == expected,
                "Azure CLI version differs from the registered MSI"
            );
            "version-matched"
        }
        Err(error)
            if route == "bare"
                && error.kind == DiagnosticKind::Config
                && error.headline == "az is not installed, or is not on the path" =>
        {
            "missing-program"
        }
        Err(error) => panic!(
            "unexpected {route} version command diagnostic: {:?}",
            error.kind
        ),
    };
    // An explicit receipt prevents a misspelled libtest filter from passing CI.
    std::fs::write("probe-result", outcome).expect("write the version-probe result");
}

/// The opt-in helper identifies the registered machine MSI and pins the
/// worker's PATH to its directory. The provider requests only its version.
#[cfg(all(windows, target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "requires scripts/check-windows-aws-cli.ps1 and the AWS CLI v2 x64 MSI"]
async fn installed_windows_aws_cli_version() {
    use secrecy::ExposeSecret;

    let expected = std::env::var("IGNATIUS_TEST_AWS_VERSION").expect("registered AWS CLI version");
    let mut target = target(SslMode::VerifyFull);
    target.auth = Some("aws".to_owned());
    let (mut provider, launch) =
        resolve_provider("aws", &BTreeMap::new()).expect("built-in AWS provider");
    assert_eq!(launch, Launch::Direct);
    assert_eq!(provider.command.first().map(String::as_str), Some("aws"));
    assert_eq!(provider.extract, Extract::Raw);
    provider.command.truncate(1);
    provider.command.push("--version".to_owned());

    let version = fetch_selected(&provider, &target, launch)
        .await
        .unwrap_or_else(|error| panic!("AWS CLI version probe failed ({:?})", error.kind));
    let cli_version = version
        .expose_secret()
        .split_ascii_whitespace()
        .next()
        .and_then(|word| word.strip_prefix("aws-cli/"));
    assert!(
        cli_version == Some(expected.as_str()),
        "AWS CLI version differs from the registered MSI"
    );
    std::fs::write("probe-result", "version-matched").expect("write the AWS version receipt");
}

/// The opt-in Windows archive probe measures the stock bare-name launch with
/// a harmless version command. The separate vendor control runs in its harness.
#[cfg(all(windows, target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "requires scripts/check-windows-gcloud-archive.ps1 and the pinned x64 archive"]
async fn windows_gcloud_archive_version() {
    use secrecy::ExposeSecret;

    let expected = std::env::var("IGNATIUS_TEST_GCLOUD_VERSION").expect("pinned archive version");
    let mut target = target(SslMode::VerifyFull);
    target.auth = Some("gcp".to_owned());
    let (mut provider, launch) =
        resolve_provider("gcp", &BTreeMap::new()).expect("built-in GCP provider");
    assert_eq!(launch, Launch::Direct);
    assert_eq!(provider.command.first().map(String::as_str), Some("gcloud"));
    assert_eq!(provider.extract, Extract::Raw);
    provider.command.truncate(1);
    provider
        .command
        .extend(["version", "--format=json"].map(str::to_owned));
    provider.extract = Extract::JsonField("Google Cloud SDK".to_owned());

    let receipt = match fetch_selected(&provider, &target, launch).await {
        Ok(version) => {
            assert!(
                version.expose_secret() == expected,
                "GCP version differs from the pinned archive"
            );
            "version-matched"
        }
        Err(error)
            if error.kind == DiagnosticKind::Config
                && error.headline == "gcloud is not installed, or is not on the path" =>
        {
            "missing-program"
        }
        Err(error) => panic!(
            "unexpected GCP version command diagnostic ({:?})",
            error.kind
        ),
    };
    std::fs::write("probe-result", receipt).expect("write the GCP version receipt");
}
