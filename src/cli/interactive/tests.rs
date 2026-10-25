use super::*;

#[test]
fn w10_actual_streaming_sink_does_not_count_rejected_insert_rows() {
    use crate::cli::output::{Format, OutputOptions, StreamWriter};
    use crate::postgres::StreamEvent;
    use crate::query::value::Cell;
    let mut wrong_counts = Vec::new();
    for prefix in [0, 1] {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("runtime.sql");
        let mut export = crate::query::export::Export::create(&destination, false).unwrap();
        let mut writer = StreamWriter::new(OutputOptions {
            format: Format::Insert,
            insert_table: Some("snapshot".into()),
            ..OutputOptions::default()
        })
        .unwrap();
        write_export_event(
            &mut export,
            &mut writer,
            StreamEvent::Columns(vec!["value".into()]),
        )
        .unwrap();
        if prefix == 1 {
            write_export_event(
                &mut export,
                &mut writer,
                StreamEvent::Row(vec![Cell::Text("synthetic-prefix".into())]),
            )
            .unwrap();
        }
        assert_eq!(writer.rows(), prefix);
        assert_eq!(export.rows(), prefix);
        let error = write_export_event(
            &mut export,
            &mut writer,
            StreamEvent::Row(vec![Cell::Text("synthetic\0rejected".into())]),
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(writer.rows(), prefix);
        if export.rows() != prefix {
            wrong_counts.push((prefix, export.rows()));
        }
        let abandoned = export.abandon();
        let expected = "INSERT INTO \"snapshot\" (\"value\") VALUES (E'synthetic-prefix');\n"
            .repeat(usize::try_from(prefix).unwrap());
        assert_eq!(
            std::fs::read_to_string(&abandoned.partial).unwrap(),
            expected
        );
        assert!(!destination.exists());
    }
    assert!(
        wrong_counts.is_empty(),
        "rejected runtime rows counted after (accepted-prefix, observed-count): {wrong_counts:?}"
    );
}

#[test]
fn w10_actual_streaming_sink_column_validation_failure_keeps_zero_counts() {
    use crate::cli::output::{Format, OutputOptions, StreamWriter};
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("columns.sql");
    let mut export = crate::query::export::Export::create(&destination, false).unwrap();
    let mut writer = StreamWriter::new(OutputOptions {
        format: Format::Insert,
        insert_table: Some("snapshot".into()),
        ..OutputOptions::default()
    })
    .unwrap();
    let result = write_export_event(
        &mut export,
        &mut writer,
        crate::postgres::StreamEvent::Columns(vec!["same".into(), "same".into()]),
    );
    assert!(result.is_err());
    assert_eq!(writer.rows(), 0);
    assert_eq!(export.rows(), 0);
    let abandoned = export.abandon();
    assert!(std::fs::read(&abandoned.partial).unwrap().is_empty());
    assert!(!destination.exists());
}

#[test]
fn runtime_geometry_first_frame_synchronizes_model_with_the_actual_buffer() {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8))
        .expect("owned in-memory terminal");
    let mut model = Model::new(100);
    assert_eq!(model.size, (80, 24), "reproduce the initial model default");
    draw(&mut terminal, &mut model, &Keymap::new()).expect("first actual frame");
    assert_eq!(model.size, (40, 8));
    assert_eq!(
        terminal.backend().buffer().area,
        ratatui::layout::Rect::new(0, 0, 40, 8)
    );
}

#[test]
fn runtime_geometry_input_observes_backend_changes_without_a_resize_message() {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8))
        .expect("owned in-memory terminal");
    let mut model = Model::new(100);
    assert!(
        reduce_terminal_input(
            &terminal,
            &mut model,
            Message::Action(crate::app::Action::FocusNext),
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(model.size, (40, 8));
    terminal.backend_mut().resize(60, 20);
    assert!(
        reduce_terminal_input(
            &terminal,
            &mut model,
            Message::Action(crate::app::Action::FocusNext),
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(model.size, (60, 20));
}

#[test]
fn runtime_geometry_stale_queued_resize_cannot_replace_current_backend_size() {
    let terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 8))
        .expect("owned in-memory terminal");
    let mut model = Model::new(100);
    assert!(
        reduce_terminal_input(&terminal, &mut model, Message::Resized(140, 50))
            .unwrap()
            .is_empty()
    );
    assert_eq!(model.size, (40, 8));
}

#[test]
fn runtime_geometry_size_failure_keeps_the_pending_statement_unsent() {
    let mut model = Model::new(100);
    model.pending_run = Some(crate::app::model::PendingRun {
        sql: "UPDATE synthetic_table SET value = 1;".into(),
        impact: crate::query::Impact::Write,
        typed: String::new(),
        required: String::new(),
        source: None,
    });
    let result = reduce_input_with_size(
        &mut model,
        Message::Action(crate::app::Action::Activate),
        || {
            Err(Diagnostic::new(
                DiagnosticKind::Internal,
                "size unavailable",
                "synthetic terminal",
            ))
        },
    );
    assert!(result.is_err());
    assert!(
        model.pending_run.is_some(),
        "admission must precede candidate mutation"
    );
    assert!(!model.phase.is_busy());
}

#[test]
fn runtime_geometry_recovery_actions_do_not_require_a_working_size_query() {
    for action in [
        crate::app::Action::Quit,
        crate::app::Action::Cancel,
        crate::app::Action::Dismiss,
    ] {
        let mut model = Model::new(100);
        model.pending_run = Some(crate::app::model::PendingRun {
            sql: "UPDATE synthetic_table SET value = 1;".into(),
            impact: crate::query::Impact::Write,
            typed: String::new(),
            required: String::new(),
            source: None,
        });
        let queried = std::cell::Cell::new(false);
        let effects = reduce_input_with_size(&mut model, Message::Action(action.clone()), || {
            queried.set(true);
            Err(Diagnostic::new(
                DiagnosticKind::Internal,
                "size unavailable",
                "synthetic terminal",
            ))
        })
        .expect("recovery must remain available");
        assert!(!queried.get());
        if action == crate::app::Action::Quit {
            assert!(matches!(effects.as_slice(), [Effect::Quit]));
            assert!(model.should_quit);
        } else {
            assert!(effects.is_empty());
            assert!(model.pending_run.is_none());
        }
    }
}

#[test]
fn runtime_geometry_changed_input_cannot_accept_before_the_new_frame_is_painted() {
    for action in [crate::app::Action::Activate, crate::app::Action::RunBuffer] {
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 50))
            .expect("owned in-memory terminal");
        let mut model = Model::new(100);
        model.size = (40, 8);
        model.connection =
            crate::app::model::ConnectionState::Connected(Box::new(crate::postgres::SessionInfo {
                target: "sample@localhost:55432/example".into(),
                database: "example".into(),
                user: "sample".into(),
                server_version: "18.6".into(),
                backend_pid: "123".into(),
                search_path: "public".into(),
                read_only: false,
                tls: crate::postgres::TlsState::Disabled,
                environment: crate::connection::Environment::Local,
            }));
        model.pending_plan = Some(crate::app::model::PendingPlan {
            sql: "SELECT 1;".into(),
            impact: crate::query::Impact::Read,
            typed: String::new(),
            required: String::new(),
        });
        let effects = reduce_terminal_input(&terminal, &mut model, Message::Action(action.clone()))
            .expect("observe fresh dimensions");
        assert!(
            effects.is_empty(),
            "a newly fitting review has not been painted yet"
        );
        assert!(model.pending_plan.is_some());
        assert_eq!(model.size, (140, 50));
        draw(&mut terminal, &mut model, &Keymap::new()).expect("paint fresh review");
        let effects = reduce_terminal_input(&terminal, &mut model, Message::Action(action))
            .expect("explicit confirmation after painting");
        assert!(matches!(effects.as_slice(), [Effect::Explain { .. }]));
    }
}

#[cfg(unix)]
fn wait_for_owned_signal_child(
    child: &mut std::process::Child,
) -> (std::process::ExitStatus, bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("poll only the owned signal child") {
            return (status, false);
        }
        if std::time::Instant::now() >= deadline {
            // It may have exited since try_wait; the unreaped owned child
            // cannot have its PID reused before the following wait.
            let _ = child.kill();
            let status = child.wait().expect("reap the owned signal child");
            return (status, true);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
#[test]
fn sigterm_is_registered_before_the_runtime_is_polled() {
    use std::io::{BufRead, Read, Write};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};

    const CHILD: &str = "IGNATIUS_TEST_STARTUP_SIGNAL_CHILD";
    const READY: &str = "IGNATIUS_STARTUP_SIGNAL_READY";
    if std::env::var_os(CHILD).is_some() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("unpolled current-thread runtime");
        let (tx, mut rx) = mpsc::unbounded_channel();
        {
            let _entered = runtime.enter();
            let ((), termination) = prepare_terminal(&runtime, || Ok(()))
                .expect("synchronous production signal preparation");
            spawn_signal_watcher(tx, termination);
        }
        // Neither the watcher task nor the current-thread driver has run.
        writeln!(std::io::stdout(), "{READY}").expect("signal readiness marker");
        std::io::stdout()
            .flush()
            .expect("flush readiness before blocking");
        std::io::stdin()
            .read_exact(&mut [0_u8])
            .expect("parent permits the first driver poll");
        let message = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("queued termination reaches the watcher")
                .expect("watcher emits a quit message")
        });
        assert!(matches!(message, Message::Action(crate::app::Action::Quit)));
        return;
    }

    let mut child = Command::new(std::env::current_exe().expect("current libtest executable"))
        .args([
            "--exact",
            "cli::interactive::tests::sigterm_is_registered_before_the_runtime_is_polled",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the one owned startup signal probe");
    let stdout = child.stdout.take().expect("child stdout");
    let stderr = child.stderr.take().expect("child stderr");
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let stdout_reader = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stdout);
        let mut transcript = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).expect("read child stdout") == 0 {
                break;
            }
            transcript.push_str(&line);
            if line.contains(READY) {
                let _ = ready_tx.send(());
            }
        }
        transcript
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut transcript = String::new();
        std::io::BufReader::new(stderr)
            .read_to_string(&mut transcript)
            .expect("read child stderr");
        transcript
    });
    if let Err(error) = ready_rx.recv_timeout(Duration::from_secs(10)) {
        if child.try_wait().expect("poll owned child").is_none() {
            let _ = child.kill();
        }
        child.wait().expect("reap unready child");
        let stdout = stdout_reader.join().expect("join stdout reader");
        let stderr = stderr_reader.join().expect("join stderr reader");
        panic!("startup signal readiness failed: {error}; {stdout}; {stderr}");
    }
    let was_blocked = child
        .try_wait()
        .expect("poll owned child before signal")
        .is_none();
    let sent = was_blocked.then(|| {
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .output()
    });
    // The old implementation can already be dead, so retain its exit evidence.
    let _ = child.stdin.take().expect("child stdin").write_all(b"p");
    let (status, timed_out) = wait_for_owned_signal_child(&mut child);
    let stdout = stdout_reader.join().expect("join stdout reader");
    let stderr = stderr_reader.join().expect("join stderr reader");
    assert!(was_blocked, "child must wait for the first driver poll");
    assert!(
        sent.is_some_and(|result| result.is_ok_and(|output| output.status.success())),
        "SIGTERM must be sent successfully only to the owned child"
    );
    assert!(
        !timed_out,
        "owned child exceeded completion deadline; {stdout}; {stderr}"
    );
    assert!(
        status.success(),
        "SIGTERM before the first runtime poll must queue quit; status={status}, signal={:?}; {stdout}; {stderr}",
        status.signal()
    );
}

#[cfg(unix)]
#[test]
fn a_signal_registration_failure_never_acquires_the_terminal() {
    let acquired = std::cell::Cell::new(false);
    let error = register_then_acquire(
        || {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "synthetic signal registration denied",
            ))
        },
        || {
            acquired.set(true);
            Ok(())
        },
    )
    .expect_err("registration failure must refuse terminal acquisition");
    assert!(!acquired.get(), "no terminal mode may be taken");
    assert_eq!(error.kind, DiagnosticKind::Internal);
    assert_eq!(error.kind.exit_code(), ExitCode::Internal);
    assert_eq!(error.headline, "could not prepare SIGTERM handling");
    assert_eq!(error.attempted, "opening the interactive client");
    assert!(
        error
            .next_action
            .as_deref()
            .is_some_and(|action| action.contains("--plain"))
    );
}

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
fn queued_connection_failures_and_reloads_keep_their_source_generation() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    send_connection_message(
        &tx,
        7,
        Message::ConnectionFailed(Box::new(Diagnostic::new(
            DiagnosticKind::Config,
            "the selected profile does not exist",
            "resolving the selected connection",
        ))),
    );
    send_connection_message(&tx, 7, Message::Action(crate::app::Action::ReloadObjects));

    let failure = rx.try_recv().expect("queued profile failure");
    assert!(matches!(
        failure,
        Message::ForConnection { generation: 7, message }
            if matches!(*message, Message::ConnectionFailed(_))
    ));
    let reload = rx.try_recv().expect("queued initial catalogue reload");
    assert!(matches!(
        reload,
        Message::ForConnection { generation: 7, message }
            if matches!(*message, Message::Action(crate::app::Action::ReloadObjects))
    ));
    assert!(rx.try_recv().is_err(), "each outcome is queued once");
}

fn resolved_test_target(database: &str) -> Arc<ConnectionTarget> {
    Arc::new(
        crate::connection::resolve(
            Some(&format!("postgres://app@127.0.0.1/{database}")),
            &crate::connection::ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &Config::default().connection,
        )
        .expect("synthetic target resolves without connecting"),
    )
}

#[tokio::test]
async fn a_profile_resolver_waiting_for_the_target_lock_cannot_replace_a_new_target() {
    let generation = std::sync::atomic::AtomicU64::new(7);
    let slot = tokio::sync::RwLock::new(None);
    let mut held = slot.write().await;
    let mut installation = Box::pin(install_connection_target(
        &slot,
        resolved_test_target("obsolete"),
        &generation,
        7,
    ));

    // Poll the old resolver into the held lock before advancing the connection.
    // This makes the interleaving deterministic without sleeping or a database.
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(installation.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    generation.store(8, std::sync::atomic::Ordering::SeqCst);
    let current = resolved_test_target("current");
    *held = Some(Arc::clone(&current));
    drop(held);

    assert!(!installation.await, "the old resolver is superseded");
    let installed = slot.read().await;
    assert!(
        Arc::ptr_eq(installed.as_ref().expect("current target"), &current),
        "a superseded resolver must not overwrite the current reconnect target"
    );
}

#[tokio::test]
async fn a_current_profile_resolver_installs_its_reconnect_target() {
    let generation = std::sync::atomic::AtomicU64::new(8);
    let slot = tokio::sync::RwLock::new(None);
    let target = resolved_test_target("current");

    assert!(install_connection_target(&slot, Arc::clone(&target), &generation, 8).await);
    let installed = slot.read().await;
    assert!(Arc::ptr_eq(
        installed.as_ref().expect("resolved target"),
        &target
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
        notice_report: crate::query::result::NoticeReport::default(),
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

fn notice_adapter_report() -> NoticeReport {
    use crate::query::result::{NoticeCount, NoticeCounts, NoticeScope};
    NoticeReport {
        scope: NoticeScope::Export,
        retained: Some(256),
        counts: NoticeCounts {
            omitted: NoticeCount::Exact(1),
            ..NoticeCounts::default()
        },
        ..NoticeReport::default()
    }
}

#[test]
fn notice_stream_report_survives_real_owned_rename_failure_and_each_stop() {
    for case in ["complete", "rename", "server", "sink"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rows.csv");
        let mut export = crate::query::export::Export::create(&path, false).unwrap();
        let mut writer = crate::cli::output::StreamWriter::new(crate::cli::output::OutputOptions {
            format: crate::cli::output::Format::Csv,
            ..crate::cli::output::OutputOptions::default()
        })
        .unwrap();
        write_export_event(
            &mut export,
            &mut writer,
            crate::postgres::StreamEvent::Columns(vec!["value".into()]),
        )
        .unwrap();
        write_export_event(
            &mut export,
            &mut writer,
            crate::postgres::StreamEvent::Row(vec![crate::query::Cell::Text("42".into())]),
        )
        .unwrap();
        let partial = export.partial_path().to_owned();
        if case == "rename" {
            std::fs::create_dir(&path).unwrap();
        }
        let value = match case {
            "server" => Err(crate::postgres::StreamStop::Server(Box::new(
                Diagnostic::new(
                    DiagnosticKind::Query,
                    "synthetic server failure",
                    "streaming",
                ),
            ))),
            "sink" => Err(crate::postgres::StreamStop::Sink(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied,
            ))),
            _ => Ok(1),
        };
        let result = finish_stream_export(
            Reported {
                value,
                notice_report: notice_adapter_report(),
            },
            export,
            crate::query::export::Export::finish,
        );
        assert_eq!(
            result.notice_report,
            notice_adapter_report(),
            "{case} cannot discard stream accounting"
        );
        match (case, result.value) {
            ("complete", Ok(ExportOutcome::Completed(finished))) => {
                assert_eq!(finished.rows, 1);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "value\n42\n");
            }
            ("rename", Err(error)) => {
                assert_eq!(error.exit_code(), ExitCode::ExportInterrupted);
                assert_eq!(std::fs::read_to_string(&partial).unwrap(), "value\n42\n");
            }
            ("server" | "sink", Ok(ExportOutcome::Interrupted { abandoned, .. })) => {
                assert_eq!(abandoned.rows, 1);
                assert_eq!(
                    std::fs::read_to_string(&abandoned.partial).unwrap(),
                    "value\n42\n"
                );
            }
            _ => panic!("unexpected {case} outcome"),
        }
    }
}

#[test]
fn notice_doctor_warning_is_inside_existing_report_and_contains_only_client_facts() {
    let mut checks = vec![Check::ok("postgres", "synthetic positive connection")];
    add_notice_check(&mut checks, &notice_adapter_report());
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0].status, CheckStatus::Ok);
    assert_eq!(checks[1].name, "server notice retention");
    assert_eq!(checks[1].status, CheckStatus::Warn);
    assert_eq!(
        checks[1].next_action.as_deref(),
        Some("Review server logs separately if complete notice history is required.")
    );
    assert_eq!(
        checks[1].detail,
        notice_adapter_report().counts.lines().join(" ")
    );
    add_notice_check(&mut checks, &NoticeReport::default());
    assert_eq!(checks.len(), 2);
}

#[allow(clippy::print_stderr)]
fn notice_runtime_fixture() -> Option<(tokio::runtime::Runtime, Session)> {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return None;
    };
    if uri.is_empty() {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is empty");
        return None;
    }
    let mut target = crate::connection::resolve(
        Some(&uri),
        &crate::connection::ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &Config::default().connection,
    )
    .unwrap();
    target.read_only = true;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let opened = runtime
        .block_on(session::connect_reported(&target, Duration::from_secs(5)))
        .value
        .unwrap();
    let major = opened
        .info()
        .server_version
        .split('.')
        .next()
        .unwrap()
        .parse::<u16>()
        .expect("PostgreSQL version begins with a numeric major");
    assert!(
        (14..=18).contains(&major),
        "supported PostgreSQL fixture major"
    );
    if let Ok(expected) = std::env::var("IGNATIUS_PG_VERSION") {
        assert_eq!(
            major,
            expected
                .parse::<u16>()
                .expect("configured fixture major is numeric")
        );
    }
    notice_runtime_health(&runtime, &opened);
    Some((runtime, opened))
}
fn notice_runtime_health(runtime: &tokio::runtime::Runtime, session: &Session) {
    let result =
        runtime.block_on(session.execute("SELECT count(*) FROM public.orders", 1, JobId(9_900)));
    assert_eq!(result.status, ExecutionStatus::Succeeded);
    assert_eq!(
        result.statements[0].result_set.as_ref().unwrap().rows[0],
        vec![crate::query::Cell::Text("3".into())]
    );
}
fn notice_installed(session: Session) -> Arc<InstalledSession> {
    Arc::new(InstalledSession {
        session: Arc::new(session),
        session_id: crate::app::model::NoticeSessionId::new(),
        generation: 1,
        source: crate::app::model::NoticeSource::User,
        retired: std::sync::atomic::AtomicBool::new(false),
    })
}

#[test]
fn notice_runtime_held_reader_defers_install_while_input_quit_and_ack_progress() {
    let Some((runtime, opened)) = notice_runtime_fixture() else {
        return;
    };
    let installed = notice_installed(opened);
    runtime.block_on(async {
        let slot: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let held = slot.read().await;
        let (acknowledged, mut ack) = tokio::sync::oneshot::channel();
        let mut model = Model::new(10);
        let install = SessionInstall {
            opened: Arc::clone(&installed),
            acknowledged,
        };
        let pending = try_session_install(install, &slot, &mut model, 1)
            .expect("held reader defers without awaiting");
        assert!(matches!(
            ack.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        let effects = update(&mut model, Message::Action(crate::app::Action::Quit));
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
        assert!(model.should_quit);
        drop(pending);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), &mut ack)
                .await
                .unwrap()
                .is_err(),
            "dropping pending install wakes owned ack waiter"
        );
        drop(held);
        let (acknowledged, ack) = tokio::sync::oneshot::channel();
        let mut model = Model::new(10);
        assert!(
            try_session_install(
                SessionInstall {
                    opened: Arc::clone(&installed),
                    acknowledged
                },
                &slot,
                &mut model,
                2
            )
            .is_none()
        );
        assert!(
            !ack.await.unwrap(),
            "stale install rejected before slot mutation"
        );
        assert!(slot.read().await.is_none());
        let (tx, mut rx) = mpsc::channel::<SessionInstall>(2);
        let (acknowledged, _) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::clone(&installed),
            acknowledged,
        })
        .await
        .unwrap();
        let (acknowledged, _) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::clone(&installed),
            acknowledged,
        })
        .await
        .unwrap();
        let (acknowledged, _) = tokio::sync::oneshot::channel();
        let send = tx.send(SessionInstall {
            opened: Arc::clone(&installed),
            acknowledged,
        });
        tokio::pin!(send);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut send)
                .await
                .is_err(),
            "capacity stays bounded"
        );
        rx.close();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), &mut send)
                .await
                .unwrap()
                .is_err(),
            "quit closes blocked sender"
        );
    });
    notice_runtime_health(&runtime, &installed.session);
}

#[test]
fn notice_runtime_physical_install_ahead_of_model_quit_and_profile_retirement_is_not_lost() {
    use crate::query::result::NoticeCount;
    for profile_switch in [false, true] {
        let Some((runtime, opened)) = notice_runtime_fixture() else {
            return;
        };
        let installed = notice_installed(opened);
        let result=runtime.block_on(installed.execute("DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic retirement %',i; END LOOP; END $$; SELECT 42",1,JobId(9_901)));
        assert_eq!(result.status, ExecutionStatus::Succeeded);
        assert_eq!(result.notice_report.counts.omitted, NoticeCount::Exact(1));
        assert_eq!(
            result.statements[1].result_set.as_ref().unwrap().rows[0],
            vec![crate::query::Cell::Text("42".into())]
        );
        let slot: SessionSlot = Arc::new(tokio::sync::RwLock::new(Some(Arc::clone(&installed))));
        let metadata: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let mut model = Model::new(10);
        // The physical slot exists, but neither its install nor Connected fact
        // has been reduced yet. Retirement must synchronize it explicitly.
        if profile_switch {
            model.connection_profiles = crate::app::connection_picker::summaries(
                &std::collections::BTreeMap::from([(
                    "synthetic".into(),
                    crate::config::schema::Profile::default(),
                )]),
                &std::collections::BTreeMap::new(),
            );
            update(
                &mut model,
                Message::Action(crate::app::Action::OpenConnectionPicker),
            );
            let effects = update(&mut model, Message::Action(crate::app::Action::Activate));
            assert!(matches!(
                effects.as_slice(),
                [Effect::ConnectProfile { .. }]
            ));
            assert_eq!(model.connection_generation, 2);
        } else {
            assert!(matches!(
                update(&mut model, Message::Action(crate::app::Action::Quit)).as_slice(),
                [Effect::Quit]
            ));
        }
        runtime.block_on(retire_notice_sources(&mut model, &slot, &metadata));
        assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
        assert_eq!(model.notice_state.previous.omitted, NoticeCount::Exact(1));
        assert!(model.notice_state.sources[0].is_none());
        runtime.block_on(retire_notice_sources(&mut model, &slot, &metadata));
        assert_eq!(
            model.notice_state.pending.omitted,
            NoticeCount::Exact(1),
            "actual retirement is once per physical token"
        );
        update(
            &mut model,
            connection_message(
                1,
                Message::NoticeStatus {
                    source: installed.source,
                    session_id: installed.session_id.clone(),
                    checkpoint: installed.notice_checkpoint(),
                    latest: None,
                },
            ),
        );
        assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
        assert!(model.notice_state.sources[0].is_none());
        notice_runtime_health(&runtime, &installed.session);
    }
}

#[test]
fn notice_runtime_shared_physical_fallback_is_polled_once_and_old_generation_is_ignored() {
    use crate::query::result::NoticeCount;
    let Some((runtime, opened)) = notice_runtime_fixture() else {
        return;
    };
    let installed = notice_installed(opened);
    let result = runtime.block_on(installed.execute(
        "DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic fallback %',i; END LOOP; END $$",
        1,
        JobId(9_902),
    ));
    assert_eq!(result.status, ExecutionStatus::Succeeded);
    let slot: SessionSlot = Arc::new(tokio::sync::RwLock::new(Some(Arc::clone(&installed))));
    let mut model = Model::new(10);
    runtime.block_on(poll_notice_sources(&mut model, &slot, &slot));
    assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
    runtime.block_on(poll_notice_sources(&mut model, &slot, &slot));
    assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
    model.connection_generation = 2;
    runtime.block_on(poll_notice_sources(&mut model, &slot, &slot));
    assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
    notice_runtime_health(&runtime, &installed.session);
}

#[test]
fn notice_runtime_pending_uninstalled_shutdown_keeps_known_loss_once_without_waiting() {
    use crate::query::result::NoticeCount;
    let Some((runtime, opened)) = notice_runtime_fixture() else {
        return;
    };
    let installed = notice_installed(opened);
    let result=runtime.block_on(installed.execute("DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic pending %',i; END LOOP; END $$; SELECT 42",1,JobId(9_903)));
    assert_eq!(result.status, ExecutionStatus::Succeeded);
    assert_eq!(result.notice_report.counts.omitted, NoticeCount::Exact(1));
    runtime.block_on(async {
        let slot: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let held = slot.read().await;
        let (acknowledged, ack) = tokio::sync::oneshot::channel();
        let mut model = Model::new(10);
        let pending = try_session_install(
            SessionInstall {
                opened: Arc::clone(&installed),
                acknowledged,
            },
            &slot,
            &mut model,
            1,
        )
        .expect("held reader defers install");
        let (tx, mut receiver) = mpsc::channel(2);
        let (alias_acknowledged, alias_ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::clone(&installed),
            acknowledged: alias_acknowledged,
        })
        .await
        .unwrap();
        assert!(matches!(
            update(&mut model, Message::Action(crate::app::Action::Quit)).as_slice(),
            [Effect::Quit]
        ));
        shutdown_pending_installs(
            &mut model,
            &mut receiver,
            [Some(pending), None],
            [None, None],
            [None, None],
        );
        assert_eq!(
            model.notice_state.pending.omitted,
            NoticeCount::Exact(1),
            "locally observed uninstalled loss survives Quit"
        );
        assert_eq!(
            model.notice_state.previous.omitted,
            NoticeCount::Exact(1),
            "no current connection facts restored"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), ack)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), alias_ack)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(model.should_quit);
        assert!(slot.read().await.is_none());
        drop(held);
    });
    runtime.block_on(async {
        // A differently wrapped alias must not re-add an already installed
        // physical Session, even if its opaque token is different.
        let alias = Arc::new(InstalledSession {
            session: Arc::clone(&installed.session),
            session_id: crate::app::model::NoticeSessionId::new(),
            generation: 1,
            source: crate::app::model::NoticeSource::Metadata,
            retired: std::sync::atomic::AtomicBool::new(false),
        });
        let (tx, mut receiver) = mpsc::channel(2);
        let (acknowledged, ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: alias,
            acknowledged,
        })
        .await
        .unwrap();
        let mut model = Model::new(10);
        model.notice_state.pending.omitted = NoticeCount::Exact(1);
        model.notice_state.previous.omitted = NoticeCount::Exact(1);
        shutdown_pending_installs(
            &mut model,
            &mut receiver,
            [None, None],
            [Some(Arc::clone(&installed)), None],
            [None, None],
        );
        assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), ack)
                .await
                .unwrap()
                .unwrap()
        );
        let stale = Arc::new(InstalledSession {
            session: Arc::clone(&installed.session),
            session_id: crate::app::model::NoticeSessionId::new(),
            generation: 1,
            source: crate::app::model::NoticeSource::User,
            retired: std::sync::atomic::AtomicBool::new(false),
        });
        let (tx, mut receiver) = mpsc::channel(2);
        let (acknowledged, ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: stale,
            acknowledged,
        })
        .await
        .unwrap();
        let mut model = Model::new(10);
        model.connection_generation = 2;
        shutdown_pending_installs(
            &mut model,
            &mut receiver,
            [None, None],
            [None, None],
            [None, None],
        );
        assert!(
            !model.notice_state.pending.needs_report(),
            "stale uninstalled work cannot become current loss"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), ack)
                .await
                .unwrap()
                .unwrap()
        );
    });
    notice_runtime_health(&runtime, &installed.session);
}

#[test]
fn notice_runtime_replacing_deferred_open_transfers_observed_loss_once() {
    use crate::query::result::NoticeCount;
    let Some((runtime, opened)) = notice_runtime_fixture() else {
        return;
    };
    let old = notice_installed(opened);
    let Some((new_runtime, new_opened)) = notice_runtime_fixture() else {
        return;
    };
    let replacement = notice_installed(new_opened);
    let result=runtime.block_on(old.execute("DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic replaced %',i; END LOOP; END $$; SELECT 42",1,JobId(9_910)));
    assert_eq!(result.status, ExecutionStatus::Succeeded);
    assert_eq!(result.notice_report.counts.omitted, NoticeCount::Exact(1));
    assert_eq!(
        result.statements[1].result_set.as_ref().unwrap().rows[0],
        vec![crate::query::Cell::Text("42".into())]
    );
    runtime.block_on(async {
        let slot: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let held = slot.read().await;
        let mut model = Model::new(10);
        let (acknowledged, old_ack) = tokio::sync::oneshot::channel();
        let old_pending = try_session_install(
            SessionInstall {
                opened: Arc::clone(&old),
                acknowledged,
            },
            &slot,
            &mut model,
            1,
        )
        .unwrap();
        let mut pending = [Some(old_pending), None];
        let mut remembered = [None, None];
        for (label, candidate) in [
            (
                "stale",
                Arc::new(InstalledSession {
                    session: Arc::clone(&replacement.session),
                    session_id: crate::app::model::NoticeSessionId::new(),
                    generation: 0,
                    source: old.source,
                    retired: std::sync::atomic::AtomicBool::new(false),
                }),
            ),
            (
                "opaque alias",
                Arc::new(InstalledSession {
                    session: Arc::clone(&replacement.session),
                    session_id: old.session_id.clone(),
                    generation: 1,
                    source: old.source,
                    retired: std::sync::atomic::AtomicBool::new(false),
                }),
            ),
            (
                "physical alias",
                Arc::new(InstalledSession {
                    session: Arc::clone(&old.session),
                    session_id: crate::app::model::NoticeSessionId::new(),
                    generation: 1,
                    source: old.source,
                    retired: std::sync::atomic::AtomicBool::new(false),
                }),
            ),
        ] {
            let (acknowledged, rejected) = tokio::sync::oneshot::channel();
            admit_pending_install(
                &mut pending,
                SessionInstall {
                    opened: candidate,
                    acknowledged,
                },
                &mut model,
                &[None, None],
                &mut remembered,
                1,
            );
            assert_eq!(
                pending[0].as_ref().unwrap().opened.session_id,
                old.session_id,
                "{label} preserves the existing owner"
            );
            assert!(
                !tokio::time::timeout(Duration::from_secs(1), rejected)
                    .await
                    .unwrap()
                    .unwrap(),
                "{label} is false-acknowledged"
            );
            assert!(
                !model.notice_state.pending.needs_report(),
                "{label} cannot retire or recount the original attempt"
            );
        }
        let (acknowledged, new_ack) = tokio::sync::oneshot::channel();
        admit_pending_install(
            &mut pending,
            SessionInstall {
                opened: Arc::clone(&replacement),
                acknowledged,
            },
            &mut model,
            &[None, None],
            &mut remembered,
            1,
        );
        assert_eq!(
            model.notice_state.pending.omitted,
            NoticeCount::Exact(1),
            "replaced current-generation opened session cannot lose known counts"
        );
        assert_eq!(model.notice_state.previous.omitted, NoticeCount::Exact(1));
        assert!(model.notice_state.sources.iter().all(Option::is_none));
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), old_ack)
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            pending[0].as_ref().unwrap().opened.session_id,
            replacement.session_id
        );
        // A newly wrapped alias of the just-replaced physical session must not
        // displace the new owner or re-add the same previous debt.
        let alias = Arc::new(InstalledSession {
            session: Arc::clone(&old.session),
            session_id: crate::app::model::NoticeSessionId::new(),
            generation: 1,
            source: old.source,
            retired: std::sync::atomic::AtomicBool::new(false),
        });
        let (acknowledged, alias_ack) = tokio::sync::oneshot::channel();
        admit_pending_install(
            &mut pending,
            SessionInstall {
                opened: alias,
                acknowledged,
            },
            &mut model,
            &[None, None],
            &mut remembered,
            1,
        );
        assert_eq!(
            pending[0].as_ref().unwrap().opened.session_id,
            replacement.session_id,
            "late physical alias cannot replace the distinct pending owner"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), alias_ack)
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(model.notice_state.previous.omitted, NoticeCount::Exact(1));
        let (tx, mut receiver) = mpsc::channel(2);
        let (acknowledged, queued_alias_ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::new(InstalledSession {
                session: Arc::clone(&old.session),
                session_id: crate::app::model::NoticeSessionId::new(),
                generation: 1,
                source: old.source,
                retired: std::sync::atomic::AtomicBool::new(false),
            }),
            acknowledged,
        })
        .await
        .unwrap();
        assert!(matches!(
            update(&mut model, Message::Action(crate::app::Action::Quit)).as_slice(),
            [Effect::Quit]
        ));
        shutdown_pending_installs(&mut model, &mut receiver, pending, [None, None], remembered);
        assert_eq!(
            model.notice_state.previous.omitted,
            NoticeCount::Exact(1),
            "queued alias of replaced attempt is not counted again at Quit"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), new_ack)
                .await
                .unwrap()
                .unwrap()
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), queued_alias_ack)
                .await
                .unwrap()
                .unwrap()
        );
        drop(held);
    });
    notice_runtime_health(&runtime, &old.session);
    notice_runtime_health(&new_runtime, &replacement.session);
}

#[test]
fn notice_runtime_profile_retirement_preserves_pending_loss_and_keeps_install_port_open() {
    use crate::query::result::NoticeCount;
    let Some((runtime, opened)) = notice_runtime_fixture() else {
        return;
    };
    let old = notice_installed(opened);
    let Some((new_runtime, new_opened)) = notice_runtime_fixture() else {
        return;
    };
    let new = Arc::new(InstalledSession {
        session: Arc::new(new_opened),
        session_id: crate::app::model::NoticeSessionId::new(),
        generation: 2,
        source: crate::app::model::NoticeSource::User,
        retired: std::sync::atomic::AtomicBool::new(false),
    });
    let result=runtime.block_on(old.execute("DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic profile pending %',i; END LOOP; END $$; SELECT 42",1,JobId(9_911)));
    assert_eq!(result.status, ExecutionStatus::Succeeded);
    assert_eq!(result.notice_report.counts.omitted, NoticeCount::Exact(1));
    assert_eq!(
        result.statements[1].result_set.as_ref().unwrap().rows[0],
        vec![crate::query::Cell::Text("42".into())]
    );
    runtime.block_on(async {
        let slot: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let metadata: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let held = slot.read().await;
        let mut model = Model::new(10);
        let (acknowledged, old_ack) = tokio::sync::oneshot::channel();
        let deferred = try_session_install(
            SessionInstall {
                opened: Arc::clone(&old),
                acknowledged,
            },
            &slot,
            &mut model,
            1,
        )
        .unwrap();
        let mut pending = [Some(deferred), None];
        let mut remembered = [None, None];
        let (tx, mut receiver) = mpsc::channel(2);
        let (acknowledged, queued_alias_ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::new(InstalledSession {
                session: Arc::clone(&old.session),
                session_id: crate::app::model::NoticeSessionId::new(),
                generation: 1,
                source: crate::app::model::NoticeSource::Metadata,
                retired: std::sync::atomic::AtomicBool::new(false),
            }),
            acknowledged,
        })
        .await
        .unwrap();
        let (acknowledged, stale_ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::new(InstalledSession {
                session: Arc::clone(&new.session),
                session_id: crate::app::model::NoticeSessionId::new(),
                generation: 0,
                source: new.source,
                retired: std::sync::atomic::AtomicBool::new(false),
            }),
            acknowledged,
        })
        .await
        .unwrap();
        model.connection_profiles = crate::app::connection_picker::summaries(
            &std::collections::BTreeMap::from([(
                "synthetic".into(),
                crate::config::schema::Profile::default(),
            )]),
            &std::collections::BTreeMap::new(),
        );
        update(
            &mut model,
            Message::Action(crate::app::Action::OpenConnectionPicker),
        );
        assert!(matches!(
            update(&mut model, Message::Action(crate::app::Action::Activate)).as_slice(),
            [Effect::ConnectProfile { .. }]
        ));
        assert_eq!(model.connection_generation, 2);
        retire_profile_notice_sources(
            &mut model,
            &slot,
            &metadata,
            &mut receiver,
            &mut pending,
            &mut remembered,
            1,
        )
        .await;
        assert_eq!(
            model.notice_state.pending.omitted,
            NoticeCount::Exact(1),
            "profile switch transfers locally held old-attempt counts before authority is retired"
        );
        assert_eq!(model.notice_state.previous.omitted, NoticeCount::Exact(1));
        assert!(pending.iter().all(Option::is_none));
        assert!(remembered.iter().all(Option::is_none));
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), queued_alias_ack)
                .await
                .unwrap()
                .unwrap(),
            "physical alias projected once"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), stale_ack)
                .await
                .unwrap()
                .unwrap(),
            "stale attempt rejected"
        );
        assert_eq!(
            model.notice_state.retiring_generation, None,
            "old retirement authority ended"
        );
        assert!(
            !tokio::time::timeout(Duration::from_secs(1), old_ack)
                .await
                .unwrap()
                .unwrap()
        );
        update(
            &mut model,
            connection_message(
                1,
                Message::NoticeStatus {
                    source: old.source,
                    session_id: old.session_id.clone(),
                    checkpoint: old.notice_checkpoint(),
                    latest: None,
                },
            ),
        );
        assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
        update(
            &mut model,
            Message::NoticeUninstalledRetired {
                generation: 1,
                session_id: crate::app::model::NoticeSessionId::new(),
                counts: result.notice_report.counts,
            },
        );
        assert_eq!(
            model.notice_state.pending.omitted,
            NoticeCount::Exact(1),
            "finished authority rejects late old special reports too"
        );
        let (acknowledged, new_ack) = tokio::sync::oneshot::channel();
        tx.send(SessionInstall {
            opened: Arc::clone(&new),
            acknowledged,
        })
        .await
        .expect("profile retirement keeps bounded port open");
        let incoming = receiver.try_recv().unwrap();
        admit_pending_install(
            &mut pending,
            incoming,
            &mut model,
            &[None, None],
            &mut remembered,
            2,
        );
        drop(held);
        assert!(try_session_install(pending[0].take().unwrap(), &slot, &mut model, 2).is_none());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), new_ack)
                .await
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            slot.read().await.as_ref().unwrap().session_id,
            new.session_id
        );
        assert_eq!(model.notice_state.pending.omitted, NoticeCount::Exact(1));
    });
    notice_runtime_health(&runtime, &old.session);
    notice_runtime_health(&new_runtime, &new.session);
}
