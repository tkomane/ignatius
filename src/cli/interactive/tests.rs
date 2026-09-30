use super::*;

#[test]
fn a_connection_switch_rejects_late_work_from_the_old_generation() {
    let generation = std::sync::atomic::AtomicU64::new(7);
    assert!(connection_generation_is_current(&generation, 7));

    generation.store(8, std::sync::atomic::Ordering::SeqCst);

    assert!(!connection_generation_is_current(&generation, 7));
    assert!(connection_generation_is_current(&generation, 8));
}

#[test]
fn a_worker_scope_is_current_only_for_its_own_request() {
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(7));
    let scope = ConnectionScope {
        generation: Arc::clone(&generation),
        request: 7,
    };
    assert!(scope.is_current());

    generation.store(8, std::sync::atomic::Ordering::SeqCst);
    assert!(!scope.is_current());

    let next = ConnectionScope {
        generation,
        request: 8,
    };
    assert!(next.is_current());
}

#[test]
fn server_bound_results_carry_their_generation_into_the_envelope() {
    let wrapped = connection_message(7, Message::ConnectionLost);
    assert!(matches!(
        wrapped,
        Message::ForConnection { generation: 7, .. }
    ));
}

#[test]
fn the_starter_buffer_is_useful_and_names_the_run_key() {
    let text = starter_query();
    assert!(text.contains("Ctrl+R"), "{text}");
    assert!(text.contains("SELECT"), "{text}");
    // It must be safe to run against anything, including production.
    assert!(!text.to_uppercase().contains("DROP"));
    assert!(!text.to_uppercase().contains("DELETE"));
    assert!(!text.to_uppercase().contains("UPDATE"));
    assert!(
        crate::query::split(text).len() == 1,
        "one statement, not a surprise batch"
    );
}

#[tokio::test]
async fn a_picker_profile_is_resolved_by_the_existing_connection_boundary() {
    let config: Config = toml::from_str(
        "[profiles.orders-dev]\n\
         host = \"127.0.0.1\"\n\
         port = 55432\n\
         dbname = \"ignatius_demo\"\n\
         user = \"ignatius_test\"\n\
         sslmode = \"disable\"\n\
         environment = \"development\"\n\
         read-only = true\n",
    )
    .expect("profile config");

    let target = prepare_profile_target_in(Some("orders-dev"), &config, &EnvSnapshot::default())
        .await
        .expect("resolve without opening a database");
    assert_eq!(
        target.safe_display(),
        "ignatius_test@127.0.0.1:55432/ignatius_demo"
    );
    assert_eq!(target.sslmode, crate::connection::SslMode::Disable);
    assert_eq!(
        target.environment,
        crate::connection::Environment::Development
    );
    assert!(target.read_only);
    assert!(target.password.is_none());
}

#[tokio::test]
async fn a_picker_unknown_profile_refuses_without_falling_back_to_defaults() {
    let config: Config =
        toml::from_str("[profiles.orders-dev]\nhost = \"127.0.0.1\"\n").expect("profile config");

    let error = prepare_profile_target_in(Some("orders-prod"), &config, &EnvSnapshot::default())
        .await
        .expect_err("unknown selection must not silently use defaults");
    assert_eq!(error.kind, DiagnosticKind::Config);
    assert!(
        error
            .likely_cause
            .as_deref()
            .is_some_and(|cause| cause.contains("orders-dev")),
        "known profiles should be named: {error:?}"
    );
}

#[tokio::test]
async fn a_picker_cloud_route_keeps_the_existing_encryption_gate_before_provider_work() {
    let config: Config = toml::from_str(
        "[profiles.orders-local]\n\
         host = \"127.0.0.1\"\n\
         dbname = \"orders\"\n\
         sslmode = \"disable\"\n\
         auth = \"entra\"\n",
    )
    .expect("profile config");

    let error = prepare_profile_target_in(Some("orders-local"), &config, &EnvSnapshot::default())
        .await
        .expect_err("cloud credentials may not cross an unencrypted route");
    assert!(
        error.headline.contains("encrypted") || error.headline.contains("TLS"),
        "the existing encryption gate should explain the refusal: {error:?}"
    );
    assert!(
        !format!("{error:?}").contains("az account"),
        "the provider command is not exposed by the refusal: {error:?}"
    );
}

/// A model holding two rows, as if a query had just returned them.
fn model_with_rows() -> Model {
    use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
    use crate::query::value::Cell;

    let mut set = ResultSet::new(vec!["name".into(), "note".into()], 100);
    set.push(vec![
        Cell::Text("alpha".into()),
        Cell::Text("has, a comma".into()),
    ]);
    set.push(vec![Cell::Text("beta".into()), Cell::Null]);

    let mut model = Model::new(100);
    model.last_execution = Some(Execution {
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(2),
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    model
}

#[test]
fn what_is_written_is_what_the_pane_shows_and_it_is_valid_csv() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("rows.csv");
    let model = model_with_rows();

    let message = export_visible_rows(
        &model,
        path.to_str().expect("utf-8"),
        crate::app::model::ExportFormat::Csv,
    )
    .expect("writes");
    assert!(message.contains("2 row(s)"), "{message}");

    let written = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        written, "name,note\nalpha,\"has, a comma\"\nbeta,\n",
        "the header, a quoted field, and a NULL as nothing at all"
    );

    // A filter narrows the file exactly as it narrows the pane.
    let mut filtered = model_with_rows();
    filtered.result_filter = "beta".into();
    let second = dir.path().join("filtered.csv");
    export_visible_rows(
        &filtered,
        second.to_str().expect("utf-8"),
        crate::app::model::ExportFormat::Csv,
    )
    .expect("writes");
    assert_eq!(
        std::fs::read_to_string(&second).expect("read"),
        "name,note\nbeta,\n"
    );
}

#[test]
fn json_export_keeps_the_server_count_and_truncation_boundary() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("filtered.json");
    let mut model = model_with_rows();
    let set = model
        .last_execution
        .as_mut()
        .expect("execution")
        .statements
        .first_mut()
        .and_then(|statement| statement.result_set.as_mut())
        .expect("result");
    set.cap = 1;
    set.rows_seen = 3;
    model.result_filter = "beta".into();

    export_visible_rows(
        &model,
        path.to_str().expect("utf-8"),
        crate::app::model::ExportFormat::Json,
    )
    .expect("writes");

    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("valid json");
    assert_eq!(document[0]["rows"].as_array().expect("rows").len(), 1);
    assert_eq!(document[0]["rows_seen"], 3);
    assert_eq!(document[0]["truncated"], true);
}

#[test]
fn writing_never_replaces_a_file_and_says_what_can_be_done_about_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("rows.csv");
    std::fs::write(&path, "precious").expect("write");

    let error = export_visible_rows(
        &model_with_rows(),
        path.to_str().expect("utf-8"),
        crate::app::model::ExportFormat::Csv,
    )
    .expect_err("must refuse");
    let action = error.next_action.expect("an action");
    assert!(
        !action.contains("--force"),
        "there are no flags to pass in here: {action}"
    );
    assert!(action.contains("choose another name"), "{action}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        "precious",
        "and the file that was there is still there"
    );
}

#[test]
fn a_leading_tilde_means_home_rather_than_a_directory_called_tilde() {
    // There is no shell behind this prompt, so the expansion is ours to do.
    let expanded = expand_home("~/exports/rows.csv");
    assert!(!expanded.starts_with("~"), "{}", expanded.display());
    assert!(
        expanded.ends_with("exports/rows.csv"),
        "{}",
        expanded.display()
    );
    assert_eq!(
        expand_home("rows.csv"),
        std::path::PathBuf::from("rows.csv")
    );
    assert_eq!(
        expand_home("/tmp/rows.csv"),
        std::path::PathBuf::from("/tmp/rows.csv")
    );
}

#[test]
fn there_is_nothing_to_write_when_no_query_has_run() {
    let error = export_visible_rows(
        &Model::new(100),
        "rows.csv",
        crate::app::model::ExportFormat::Csv,
    )
    .expect_err("must refuse");
    assert!(error.next_action.is_some());
    assert!(!std::path::Path::new("rows.csv").exists());
}

#[tokio::test]
async fn probing_an_unreachable_host_fails_with_actions_and_skips_the_rest() {
    let config = Config::default();
    let target = crate::connection::resolve(
        // Port 1 on loopback is not a PostgreSQL server.
        Some("postgres://app@127.0.0.1:1/orders?connect_timeout=2"),
        &crate::connection::ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("resolve");

    let checks = probe_target(target, &config).await.expect("probe");
    let tcp = checks.iter().find(|c| c.name == "tcp").expect("tcp check");
    assert_eq!(tcp.status, CheckStatus::Fail);
    assert!(tcp.next_action.is_some(), "a failure must say what to do");

    let postgres = checks
        .iter()
        .find(|c| c.name == "postgres")
        .expect("postgres check");
    assert_eq!(
        postgres.status,
        CheckStatus::Skipped,
        "the same failure must not be reported twice"
    );
}

#[tokio::test]
async fn probing_reports_the_requested_guarantee_before_connecting() {
    let config = Config::default();
    let target = crate::connection::resolve(
        Some("postgres://app@127.0.0.1:1/orders?sslmode=require&connect_timeout=2"),
        &crate::connection::ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("resolve");

    let checks = probe_target(target, &config).await.expect("probe");
    let target_check = &checks[0];
    assert!(
        target_check.detail.contains("sslmode=require"),
        "{target_check:?}"
    );
    assert!(
        target_check.detail.contains("no identity check"),
        "the report must not let require pass for verification: {target_check:?}"
    );
}

#[test]
fn clipboard_runtime_writes_one_sequence_and_flushes_before_reporting_sent() {
    let payload = crate::clipboard::ClipboardPayload::try_new("hé".to_owned()).expect("fits");
    let expected = crate::clipboard::osc52_sequence(&payload);
    let message = {
        let mut writer = RecordingWriter::default();
        let message = copy_to_terminal(&mut writer, &payload);
        assert_eq!(writer.writes, 1);
        assert_eq!(writer.flushes, 1);
        assert_eq!(writer.output, expected.as_bytes());
        message
    };
    assert_eq!(
        message,
        Message::ClipboardSent {
            bytes: 3,
            characters: 2
        }
    );
}

#[test]
fn clipboard_runtime_failure_is_safe_and_actionable() {
    let payload =
        crate::clipboard::ClipboardPayload::try_new("sensitive".to_owned()).expect("fits");
    let message = copy_to_terminal(&mut FailingWriter, &payload);
    let Message::ClipboardFailed(diagnostic) = message else {
        panic!("expected a safe failure message");
    };
    assert!(
        diagnostic.next_action.as_deref().is_some_and(|action| {
            action.contains("terminal output") && action.contains("export")
        })
    );
    assert!(!format!("{diagnostic:?}").contains("sensitive"));
}

#[derive(Default)]
struct RecordingWriter {
    output: Vec<u8>,
    writes: usize,
    flushes: usize,
}

impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.writes += 1;
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("synthetic terminal failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("synthetic terminal flush failure"))
    }
}
