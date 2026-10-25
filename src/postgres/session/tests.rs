use super::*;
use crate::connection::{ConnectionArgs, EnvSnapshot, resolve};

fn target(uri: &str) -> ConnectionTarget {
    resolve(
        Some(uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &crate::config::ConnectionConfig::default(),
    )
    .expect("resolve")
}

#[test]
fn the_driver_configuration_mirrors_the_resolved_target() {
    let target = target("postgres://app@db.example.net:6432/orders?sslmode=require");
    let config = build_config(&target);
    assert_eq!(config.get_ports(), [6432]);
    assert_eq!(config.get_user(), Some("app"));
    assert_eq!(config.get_dbname(), Some("orders"));
    assert_eq!(config.get_ssl_mode(), PgSslMode::Require);
    assert_eq!(
        config.get_application_name(),
        Some(crate::branding::DEFAULT_APPLICATION_NAME)
    );
}

#[test]
fn verifying_modes_still_require_tls_at_the_protocol_level() {
    for uri in [
        "postgres://app@db.example.net/orders?sslmode=verify-full",
        "postgres://app@db.example.net/orders?sslmode=require",
    ] {
        assert_eq!(
            build_config(&target(uri)).get_ssl_mode(),
            PgSslMode::Require
        );
    }
    assert_eq!(
        build_config(&target("postgres://app@localhost/orders?sslmode=disable")).get_ssl_mode(),
        PgSslMode::Disable
    );
    assert_eq!(
        build_config(&target("postgres://app@localhost/orders")).get_ssl_mode(),
        PgSslMode::Prefer
    );
}

#[test]
fn posture_is_stated_in_words() {
    let mut info = SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "123".into(),
        search_path: "\"$user\", public".into(),
        read_only: false,
        tls: TlsState::Disabled,
        environment: crate::connection::Environment::Local,
    };
    assert_eq!(info.posture(), "read-write");
    info.read_only = true;
    assert_eq!(info.posture(), "read-only");
}

#[test]
fn notice_privacy_scrubs_the_backlog_and_stays_active_for_later_notices() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    retain_notice(
        &notices,
        &privacy,
        "NOTICE",
        "ordinary visible notice",
        "00000",
    );
    assert_eq!(
        notices.state_for_tests()[0].message,
        "ordinary visible notice"
    );

    activate_notice_privacy(&notices, &privacy);
    assert!(privacy.load(Ordering::SeqCst));
    retain_notice(
        &notices,
        &privacy,
        "secret severity",
        "secret-derived notice",
        "bad-code",
    );
    let held = notices.state_for_tests();
    assert_eq!(held.len(), 2);
    for notice in held.iter() {
        assert_eq!(notice.severity, "NOTICE");
        assert!(notice.message.contains("omitted"));
        assert!(!notice.message.contains("secret"));
    }
    assert_eq!(held[0].code.as_deref(), Some("00000"));
    assert!(held[1].code.is_none());
}

#[test]
fn parameter_semantics_refuses_unknown_or_non_exact_settings_without_echoing_them() {
    assert!(validate_parameter_semantics(Some("on"), 3).is_ok());
    for setting in [
        None,
        Some("off"),
        Some("ON"),
        Some(" on"),
        Some("on\n"),
        Some("private-setting-value"),
    ] {
        let diagnostic = validate_parameter_semantics(setting, 3).expect_err("exact on required");
        assert_eq!(diagnostic.kind, DiagnosticKind::Query);
        assert_eq!(diagnostic.statement_number, Some(3));
        assert!(
            diagnostic
                .next_action
                .as_deref()
                .is_some_and(|action| action.contains("separately"))
        );
        assert!(!format!("{diagnostic:?}").contains("private-setting-value"));
    }
}

#[allow(clippy::print_stderr)]
async fn live_session(startup_setting: Option<&str>) -> Option<Session> {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return None;
    };
    if uri.is_empty() {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is empty");
        return None;
    }
    let target = target(&uri);
    let mut config = build_config(&target);
    if let Some(setting) = startup_setting {
        // The public connection contract deliberately has no PGOPTIONS route.
        // Exercise the production driver/notice/bootstrap path with a startup
        // setting instead of changing a shared role or database's defaults.
        config.options(format!("-c standard_conforming_strings={setting}"));
    }
    let tls = crate::postgres::tls::TlsOptions {
        mode: target.sslmode,
        root_cert: target.root_cert.clone(),
        client_cert: target.client_cert.clone(),
        client_key: target.client_key.clone(),
    };
    let notice_privacy = Arc::new(AtomicBool::new(false));
    let (client, notices, connection_lost) = match client_config(&tls).expect("fixture TLS") {
        None => {
            establish(
                &config,
                tokio_postgres::NoTls,
                &target,
                Arc::clone(&notice_privacy),
            )
            .await
        }
        Some(tls_config) => {
            establish(
                &config,
                tokio_postgres_rustls::MakeRustlsConnect::new(tls_config),
                &target,
                Arc::clone(&notice_privacy),
            )
            .await
        }
    }
    .expect("synthetic fixture connection");
    let info = bootstrap(&client, &target).await.expect("session facts");
    Some(Session {
        client,
        user_sql_gate: tokio::sync::Mutex::new(()),
        info,
        tls,
        notices,
        notice_privacy,
        connection_lost,
    })
}

fn semantics_bindings(value: &str) -> crate::query::ParameterBindings {
    crate::query::ParameterBindings::from_secrets(
        vec!["value".to_owned()],
        vec![secrecy::SecretString::from(value.to_owned())],
    )
    .expect("synthetic binding")
}

#[tokio::test]
async fn parameter_semantics_off_at_start_is_refused_before_the_first_parameterized_send() {
    let Some(session) = live_session(Some("off")).await else {
        return;
    };
    let supplied = semantics_bindings("synthetic-startup-value");
    let execution = session
        .execute_with_parameters("SELECT :value", &supplied, 10, JobId(820))
        .await;
    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert!(execution.statements.is_empty());
    assert!(
        execution
            .error
            .expect("setting refusal")
            .headline
            .contains("standard_conforming_strings")
    );
    let mut events = Vec::new();
    assert!(matches!(
        session
            .stream_with_parameters("SELECT :value", &supplied, &mut |event| {
                events.push(event);
                Ok(())
            })
            .await,
        Err(StreamStop::Server(_))
    ));
    assert!(events.is_empty());
}

#[tokio::test]
async fn parameter_semantics_gate_covers_every_user_sql_entry_point() {
    let Some(session) = live_session(None).await else {
        return;
    };
    session
        .client
        .batch_execute("CREATE TEMP TABLE semantics_gate_marker (n integer)")
        .await
        .expect("synthetic gate marker");

    macro_rules! prove_gate {
        ($operation:expr, $check:expr) => {{
            let held = session.user_sql_gate.lock().await;
            let mut operation = Box::pin($operation);
            for _ in 0..4 {
                std::future::poll_fn(|cx| {
                    assert!(
                        std::future::Future::poll(operation.as_mut(), cx).is_pending(),
                        "the user operation completed while another operation held the gate"
                    );
                    std::task::Poll::Ready(())
                })
                .await;
                // This private, fixed SQL is a protocol barrier, not a sleep.
                // Any request sent by the polled operation has completed by
                // this response, including a fresh setting probe.
                let row = session
                    .client
                    .query_one("SELECT count(*) FROM semantics_gate_marker", &[])
                    .await
                    .expect("protocol barrier");
                assert_eq!(
                    row.get::<_, i64>(0),
                    0,
                    "user SQL bypassed the held session gate"
                );
            }
            drop(held);
            let result = tokio::time::timeout(Duration::from_secs(5), operation)
                .await
                .expect("the released operation must complete");
            ($check)(result);
            let row = session
                .client
                .query_one("SELECT count(*) FROM semantics_gate_marker", &[])
                .await
                .expect("post-operation barrier");
            assert_eq!(
                row.get::<_, i64>(0),
                1,
                "the operation executes once after gate release"
            );
            session
                .client
                .batch_execute("TRUNCATE semantics_gate_marker")
                .await
                .expect("clear marker");
        }};
    }

    prove_gate!(
        session.execute(
            "INSERT INTO semantics_gate_marker VALUES (1)",
            10,
            JobId(821)
        ),
        |execution: Execution| assert_eq!(execution.status, ExecutionStatus::Succeeded)
    );
    let supplied = semantics_bindings("2");
    prove_gate!(
        session.execute_with_parameters(
            "INSERT INTO semantics_gate_marker VALUES (:value::integer)",
            &supplied,
            10,
            JobId(822)
        ),
        |execution: Execution| assert_eq!(execution.status, ExecutionStatus::Succeeded)
    );
    let mut sink = |_| Ok(());
    prove_gate!(
        session.stream("INSERT INTO semantics_gate_marker VALUES (3)", &mut sink),
        |result: Result<u64, StreamStop>| assert_eq!(result.expect("ordinary stream"), 0)
    );
    prove_gate!(
        session.stream_with_parameters(
            "INSERT INTO semantics_gate_marker VALUES (:value::integer)",
            &supplied,
            &mut sink
        ),
        |result: Result<u64, StreamStop>| assert_eq!(result.expect("parameterized stream"), 0)
    );
    prove_gate!(
        session.explain("INSERT INTO semantics_gate_marker VALUES (5)", true),
        |result: Result<String, Diagnostic>| assert!(
            result.expect("analyzed statement").contains("Plan")
        )
    );
}

#[test]
fn notice_retention_count_boundary_is_an_ordered_prefix() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    for index in 0..257 {
        retain_notice(
            &notices,
            &privacy,
            "NOTICE",
            &format!("synthetic-{index}"),
            "00000",
        );
    }
    let held = notices.state_for_tests();
    assert_eq!(
        held.len(),
        256,
        "whole inbox must retain at most 256 entries"
    );
    assert_eq!(held.last().unwrap().message, "synthetic-255");
}

#[test]
fn notice_retention_aggregate_fields_boundary_precedes_cloning() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let message = "x".repeat(crate::query::result::MAX_NOTICE_MESSAGE_BYTES);
    for _ in 0..17 {
        retain_notice(&notices, &privacy, "NOTICE", &message, "00000");
    }
    let held = notices.state_for_tests();
    let retained_bytes: usize = held
        .iter()
        .map(|notice| {
            notice.severity.len()
                + notice.message.len()
                + notice.code.as_deref().map_or(0, str::len)
        })
        .sum();
    assert!(
        retained_bytes <= 1024 * 1024,
        "all retained fields must fit 1 MiB, got {retained_bytes}"
    );
    assert_eq!(held.len(), 15);
}

#[test]
fn retained_data_ingestion_header_refusal_keeps_no_partial_description_and_closes() {
    let mut budget = RetentionBudget::for_tests(1);
    let set = retain_description(["one", "two"].into_iter(), 2, 10, &mut budget);
    assert!(
        !set.is_available(),
        "essential description exceeded the complete occupied-data allowance"
    );
    assert!(set.columns.is_empty() && set.column_types.is_empty() && set.rows.is_empty());
    assert_eq!(set.source_column_count(), 2);
    assert_eq!(budget.charged_bytes(), 0);
    assert!(budget.data_is_closed());
}

#[test]
fn retained_data_ingestion_row_refusal_counts_without_owned_placeholder() {
    let mut budget = RetentionBudget::for_tests(1);
    let mut set = ResultSet::new(vec!["n".into()], 10);
    retain_buffered_row(
        &mut set,
        [Some("too much for the remaining budget")].into_iter(),
        &mut budget,
    )
    .unwrap();
    assert_eq!(set.rows_seen, 1);
    assert!(
        set.rows.is_empty(),
        "aggregate refusal must not retain a complete or empty invented row"
    );
    assert_eq!(
        set.retention_reason(),
        Some(crate::query::result::RetentionReason::RetainedResultLimit)
    );
}

#[test]
fn retained_data_ingestion_existing_percell_failure_precedes_aggregate_limit() {
    let mut budget = RetentionBudget::for_tests(0);
    let mut set = ResultSet::new(vec!["n".into()], 10);
    let oversized = "x".repeat(crate::query::value::MAX_RETAINED_VALUE_BYTES + 1);
    let error = retain_buffered_row(
        &mut set,
        [Some(oversized.as_str())].into_iter(),
        &mut budget,
    )
    .unwrap_err();
    assert_eq!(error, oversized.len());
    assert_eq!(set.rows_seen, 0);
    assert!(set.rows.is_empty());
}

#[test]
fn retained_data_ingestion_existing_rowcap_zero_does_not_read_discarded_values() {
    let mut budget = RetentionBudget::for_tests(0);
    let mut set = ResultSet::new(vec!["n".into()], 0);
    let cells = std::iter::once_with(|| -> Option<&str> {
        panic!("row-cap-only discarded value was inspected")
    });
    retain_buffered_row(&mut set, cells, &mut budget).unwrap();
    assert_eq!(set.rows_seen, 1);
    assert!(set.rows.is_empty());
    assert!(!budget.data_is_closed());
}

#[test]
fn retained_data_ingestion_cumulative_closure_keeps_later_received_counts() {
    let mut budget = RetentionBudget::for_tests(0);
    let mut first = retain_description(["first"].into_iter(), 1, 100, &mut budget);
    retain_buffered_row(
        &mut first,
        std::iter::once_with(|| -> Option<&str> { panic!("unavailable row inspected") }),
        &mut budget,
    )
    .unwrap();
    let mut later = retain_description(["small"].into_iter(), 1, 100, &mut budget);
    retain_buffered_row(&mut later, [Some("42")].into_iter(), &mut budget).unwrap();
    assert!(!first.is_available() && !later.is_available());
    assert_eq!(first.rows_seen, 1);
    assert_eq!(later.rows_seen, 1);
    assert_eq!(budget.charged_bytes(), 0);
}

#[test]
fn retained_data_ingestion_rowcap_only_leaves_later_set_budget_open() {
    let mut budget = RetentionBudget::for_tests(1024);
    let mut capped = retain_description(["capped"].into_iter(), 1, 0, &mut budget);
    retain_buffered_row(
        &mut capped,
        std::iter::once_with(|| -> Option<&str> { panic!("capped value inspected") }),
        &mut budget,
    )
    .unwrap();
    let mut later = retain_description(["later"].into_iter(), 1, 1, &mut budget);
    retain_buffered_row(&mut later, [Some("42")].into_iter(), &mut budget).unwrap();
    assert_eq!(capped.retention_reason(), Some(RetentionReason::RowCap));
    assert!(later.is_available());
    assert_eq!(later.rows, vec![vec![Cell::Text("42".into())]]);
    assert!(!budget.data_is_closed());
}

/// The original iterator is the owned-copy pass; only its clone may be visited
/// for preflight when admission fails. This proves actual ingestion ordering.
struct PreflightOnly<T: Copy + 'static> {
    values: &'static [T],
    index: usize,
    preflight: bool,
}
impl<T: Copy> Clone for PreflightOnly<T> {
    fn clone(&self) -> Self {
        Self {
            values: self.values,
            index: self.index,
            preflight: true,
        }
    }
}
impl<T: Copy> Iterator for PreflightOnly<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        assert!(
            self.preflight,
            "owned-copy pass started after complete admission refusal"
        );
        let value = self.values.get(self.index).copied();
        self.index += usize::from(value.is_some());
        value
    }
}

#[test]
fn retained_data_ingestion_refusal_never_starts_owned_header_or_cell_pass() {
    let mut budget = RetentionBudget::for_tests(0);
    let names = PreflightOnly {
        values: &["one", "two"],
        index: 0,
        preflight: false,
    };
    let set = retain_description(names, 2, 100, &mut budget);
    assert!(!set.is_available());
    let mut budget = RetentionBudget::for_tests(0);
    let mut set = ResultSet::new(vec!["value".into()], 100);
    let cells = PreflightOnly {
        values: &[Some("one"), Some("two")],
        index: 0,
        preflight: false,
    };
    retain_buffered_row(&mut set, cells, &mut budget).unwrap();
    assert_eq!(set.rows_seen, 1);
    assert!(set.rows.is_empty());
}
