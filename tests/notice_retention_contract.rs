//! Bounded notice accounting through the actual CLI and PostgreSQL adapter.
#![allow(clippy::print_stderr)]

use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, ConnectionTarget, EnvSnapshot, resolve};
use ignatius::postgres::session::{self, Session};
use ignatius::query::{Cell, ExecutionStatus, JobId};
use secrecy::ExposeSecret;
use std::process::{Command, Output};
use std::time::Duration;

struct Fixture {
    runtime: tokio::runtime::Runtime,
    session: Session,
    target: ConnectionTarget,
    state: tempfile::TempDir,
}

impl Fixture {
    fn open() -> Option<Self> {
        let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
            eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
            return None;
        };
        if uri.is_empty() {
            eprintln!("skipping: IGNATIUS_TEST_PG_URI is empty");
            return None;
        }
        let mut target = resolve(
            Some(&uri),
            &ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &Config::default().connection,
        )
        .expect("synthetic target");
        target.read_only = true;
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let opened = runtime.block_on(session::connect_reported(&target, Duration::from_secs(5)));
        let session = opened.value.expect("synthetic fixture session");
        let fixture = Self {
            runtime,
            session,
            target,
            state: tempfile::tempdir().expect("isolated state"),
        };
        fixture.health();
        Some(fixture)
    }
    fn health(&self) {
        let major = self
            .session
            .info()
            .server_version
            .split('.')
            .next()
            .unwrap();
        assert!(
            major
                .parse::<u32>()
                .is_ok_and(|major| (14..=18).contains(&major)),
            "fixture must use a supported PostgreSQL major"
        );
        if let Ok(expected) = std::env::var("IGNATIUS_PG_VERSION") {
            assert_eq!(major, expected, "configured fixture major");
        }
        let result = self.runtime.block_on(self.session.execute(
            "SELECT count(*) FROM public.orders",
            1,
            JobId(9_000),
        ));
        assert_eq!(result.status, ExecutionStatus::Succeeded);
        assert_eq!(
            result.statements[0].result_set.as_ref().unwrap().rows,
            vec![vec![Cell::Text("3".into())]]
        );
    }
    fn command(&self) -> Command {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ignatius"));
        for key in [
            "PGHOST",
            "PGPORT",
            "PGDATABASE",
            "PGUSER",
            "PGPASSWORD",
            "PGAPPNAME",
            "PGSSLMODE",
            "PGCONNECT_TIMEOUT",
            "PGSERVICE",
            "PGSERVICEFILE",
            "PGPASSFILE",
            "PGSSLROOTCERT",
            "PGSSLCERT",
            "PGSSLKEY",
            "PGREQUIRESSL",
            "PGCHANNELBINDING",
            "PGGSSENCMODE",
        ] {
            child.env_remove(key);
        }
        child
            .env_remove("IGNATIUS_LOG")
            .env("IGNATIUS_CONFIG_DIR", self.state.path().join("config"))
            .env("IGNATIUS_DATA_DIR", self.state.path().join("data"))
            .env("PGPASSFILE", self.state.path().join("absent-passfile"))
            .env("PGHOST", self.target.host.display())
            .env("PGPORT", self.target.port.to_string())
            .env("PGDATABASE", &self.target.database)
            .env("PGUSER", &self.target.user)
            .env("PGSSLMODE", self.target.sslmode.as_str());
        if let Some(password) = self.target.password.as_ref() {
            child.env("PGPASSWORD", password.expose_secret());
        }
        if let Some(root) = self.target.root_cert.as_ref() {
            child.env("PGSSLROOTCERT", root);
        }
        child
    }
    fn query(&self, sql: &str, format: &str) -> Output {
        self.command()
            .args(["query", "--read-only", "-c", sql, "--format", format])
            .output()
            .expect("owned CLI child")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.health();
    }
}

const FLOOD: &str = "DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic notice %', i; END LOOP; END $$; SELECT 42 AS value;";

#[test]
fn notice_cli_csv_discloses_loss_after_positive_rows() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    let output = fx.query(FLOOD, "csv");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "value\n42\n");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "Client omitted 1 observed server notices after reaching the notice-retention limit.\n"
    );
}

#[test]
fn notice_cli_json_success_is_one_client_status_object() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    let output = fx.query(FLOOD, "json");
    assert_eq!(output.status.code(), Some(0));
    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("positive JSON rows");
    assert!(data.to_string().contains("42"));
    let status: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("one client JSON status");
    assert_eq!(status["kind"], "notice_retention");
    assert_eq!(status["source"], "client");
    assert_eq!(status["notice_report"]["retained"], 256);
    assert_eq!(status["notice_report"]["omitted"]["count"], 1);
    assert!(status.get("exit_code").is_none());
}

#[test]
fn notice_cli_json_error_keeps_single_diagnostic_and_exact_sqlstate() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    let sql = format!("{FLOOD} SELECT 1/0;");
    let output = fx.query(&sql, "json");
    assert_eq!(output.status.code(), Some(7));
    let prefix: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("existing completed-prefix data stays intact");
    assert!(prefix.to_string().contains("42"));
    let status: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("one original diagnostic object");
    assert_eq!(status["exit_code"], 7);
    assert!(
        status["technical"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field["label"] == "SQLSTATE" && field["value"] == "22012")
    );
    assert_eq!(status["notice_report"]["omitted"]["count"], 1);
}

#[test]
fn notice_describe_phase_remains_unattributed_to_successful_query() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    let sql = format!("SELECT 1 AS \"{}\"", "synthetic_alias_".repeat(8));
    let execution = fx
        .runtime
        .block_on(fx.session.execute(&sql, 10, JobId(9_100)));
    assert_eq!(execution.status, ExecutionStatus::Succeeded);
    assert_eq!(
        execution.statements[0].result_set.as_ref().unwrap().rows[0],
        vec![Cell::Text("1".into())]
    );
    assert_eq!(
        execution.statements[0].notices.len(),
        1,
        "one parse notice belongs to actual query phase"
    );
    assert_eq!(
        execution.notice_report.unattributed.len(),
        1,
        "prepare/describe parse notice is observed on connection"
    );
    assert_eq!(execution.notice_report.retained, Some(2));
}

#[test]
fn notice_operation_count_and_field_bytes_hold_across_many_statements() {
    use ignatius::query::result::NoticeCount;
    let Some(fx) = Fixture::open() else {
        return;
    };
    let sql = "DO $$ BEGIN FOR i IN 1..128 LOOP RAISE NOTICE 'synthetic first %',i; END LOOP; END $$; DO $$ BEGIN FOR i IN 1..128 LOOP RAISE NOTICE 'synthetic second %',i; END LOOP; END $$; DO $$ BEGIN RAISE NOTICE 'synthetic third'; END $$; SELECT 42";
    let execution = fx
        .runtime
        .block_on(fx.session.execute(sql, 1, JobId(9_200)));
    assert_eq!(execution.status, ExecutionStatus::Succeeded);
    assert_eq!(
        execution.statements[3].result_set.as_ref().unwrap().rows[0],
        vec![Cell::Text("42".into())]
    );
    assert_eq!(
        execution
            .statements
            .iter()
            .map(|statement| statement.notices.len())
            .collect::<Vec<_>>(),
        [128, 128, 0, 0]
    );
    assert_eq!(execution.notice_report.retained, Some(256));
    assert_eq!(
        execution.notice_report.counts.omitted,
        NoticeCount::Exact(1)
    );
    let bytes = "DO $$ BEGIN FOR i IN 1..17 LOOP RAISE NOTICE '%',repeat('x',65536); END LOOP; END $$; SELECT 42";
    let execution = fx
        .runtime
        .block_on(fx.session.execute(bytes, 1, JobId(9_201)));
    assert_eq!(execution.status, ExecutionStatus::Succeeded);
    assert_eq!(execution.notice_report.retained, Some(15));
    assert_eq!(
        execution.notice_report.counts.omitted,
        NoticeCount::Exact(2)
    );
    assert_eq!(
        execution.notice_report.counts.shortened,
        NoticeCount::Exact(0)
    );
    let retained: usize = execution
        .statements
        .iter()
        .flat_map(|statement| &statement.notices)
        .map(|n| n.message.len() + n.severity.len() + n.code.as_deref().map_or(0, str::len))
        .sum();
    assert!(retained <= 1024 * 1024);
    let execution = fx.runtime.block_on(fx.session.execute(
        "DO $$ BEGIN RAISE NOTICE '%',repeat('x',2097152); END $$; SELECT 42",
        1,
        JobId(9_202),
    ));
    assert_eq!(execution.status, ExecutionStatus::Succeeded);
    assert_eq!(execution.notice_report.retained, Some(1));
    assert_eq!(
        execution.notice_report.counts.shortened,
        NoticeCount::Exact(1)
    );
    assert!(
        execution.statements[0].notices[0]
            .message
            .ends_with("[notice truncated]")
    );
}

#[test]
fn notice_stream_all_stops_keep_report_and_successful_rows() {
    use ignatius::postgres::{StreamEvent, StreamStop};
    use ignatius::query::result::NoticeCount;
    let Some(fx) = Fixture::open() else {
        return;
    };
    let mut rows = Vec::new();
    let mut columns = 0;
    let complete = fx
        .runtime
        .block_on(fx.session.stream_reported(FLOOD, &mut |event| {
            match event {
                StreamEvent::Columns(_) => columns += 1,
                StreamEvent::Row(row) => rows.push(row),
            }
            Ok(())
        }));
    assert_eq!(complete.value.unwrap(), 1);
    assert_eq!(columns, 1);
    assert_eq!(rows, [vec![Cell::Text("42".into())]]);
    assert_eq!(complete.notice_report.counts.omitted, NoticeCount::Exact(1));
    assert_eq!(complete.notice_report.unattributed.len(), 256);
    let mut attempted = 0;
    let sink = fx
        .runtime
        .block_on(fx.session.stream_reported(FLOOD, &mut |event| {
            if matches!(event, StreamEvent::Row(_)) {
                attempted += 1;
                return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
            }
            Ok(())
        }));
    assert!(matches!(sink.value, Err(StreamStop::Sink(_))));
    assert_eq!(attempted, 1);
    assert_eq!(sink.notice_report.counts.omitted, NoticeCount::Exact(1));
    let server=fx.runtime.block_on(fx.session.stream_reported("DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic server %',i; END LOOP; END $$; SELECT 1/0",&mut |_|Ok(())));
    let Err(StreamStop::Server(error)) = server.value else {
        panic!("server stop required");
    };
    assert!(
        error
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "22012")
    );
    assert_eq!(server.notice_report.counts.omitted, NoticeCount::Exact(1));
    let shape = fx.runtime.block_on(
        fx.session
            .stream_reported(&format!("{FLOOD} SELECT 43"), &mut |_| Ok(())),
    );
    assert!(matches!(shape.value, Err(StreamStop::Server(_))));
    assert_eq!(shape.notice_report.counts.omitted, NoticeCount::Exact(1));
}

#[test]
fn notice_plan_success_and_failure_include_parse_phase_and_existing_transaction_facts() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    for analyze in [false, true] {
        let sql = if analyze {
            format!("SELECT 1/0 AS \"{}\"", "synthetic_alias_".repeat(8))
        } else {
            format!("SELECT 42 AS \"{}\"", "synthetic_alias_".repeat(8))
        };
        let reported = fx
            .runtime
            .block_on(fx.session.explain_reported(&sql, analyze));
        assert!(!reported.value.connection_lost);
        assert_eq!(
            reported.value.transaction,
            ignatius::query::result::TransactionState::Autocommit
        );
        assert_eq!(reported.notice_report.retained, Some(1));
        assert_eq!(reported.notice_report.unattributed.len(), 1);
        if analyze {
            let error = reported.value.result.unwrap_err();
            assert!(
                error
                    .technical
                    .iter()
                    .any(|field| field.label == "SQLSTATE" && field.value == "22012")
            );
        } else {
            let plan: serde_json::Value =
                serde_json::from_str(&reported.value.result.unwrap()).unwrap();
            assert!(plan.to_string().contains("Plan"));
        }
    }
}

#[test]
fn notice_cli_export_completion_and_server_interruption_preserve_file_bytes() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    for format in ["csv", "tsv", "ndjson", "insert"] {
        let path = fx.state.path().join(format!("complete.{format}"));
        let mut command = fx.command();
        command
            .args([
                "query",
                "--read-only",
                "-c",
                FLOOD,
                "--format",
                format,
                "--output",
            ])
            .arg(&path);
        if format == "insert" {
            command.args(["--insert-table", "snapshot"]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stdout.is_empty());
        let bytes = std::fs::read_to_string(&path).unwrap();
        assert!(bytes.contains("42"));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Client omitted 1 observed server notices")
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("Exported 1 row(s)"));
    }
    let path = fx.state.path().join("interrupted.csv");
    let sql = format!("{FLOOD} DO $$ BEGIN RAISE EXCEPTION 'synthetic bounded stop'; END $$;");
    let output = fx
        .command()
        .args([
            "query",
            "--read-only",
            "-c",
            &sql,
            "--format",
            "csv",
            "--output",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(9));
    assert!(!path.exists());
    assert!(output.stdout.is_empty());
    let partial = fx.state.path().join("interrupted.csv.partial");
    assert_eq!(std::fs::read_to_string(partial).unwrap(), "value\n42\n");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Client omitted 1 observed server notices")
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Export incomplete - 1 row(s) written")
    );
}

#[test]
fn notice_dropped_owned_operation_reports_prefix_loss_and_releases_gate_without_replay() {
    use ignatius::query::result::NoticeCount;
    let Some(fx) = Fixture::open() else {
        return;
    };
    fx.runtime.block_on(async {
        let monitor=session::connect_reported(&fx.target.for_object_tree(),Duration::from_secs(5)).await.value.unwrap();
        let pid=fx.session.info().backend_pid.parse::<u32>().unwrap();
        let sql="DO $$ BEGIN FOR i IN 1..257 LOOP RAISE NOTICE 'synthetic owned prefix %',i; END LOOP; END $$; SELECT pg_sleep(5)";
        let cancel=fx.session.cancel_handle();
        let mut running=Box::pin(fx.session.execute(sql,1,JobId(9_250)));
        let barrier=tokio::time::timeout(Duration::from_secs(3),async {
            loop {
                tokio::select! {
                    _=&mut running=>panic!("owned sleeper finished before positive wait barrier"),
                    _=tokio::time::sleep(Duration::from_millis(10))=>{},
                }
                let seen=monitor.execute(&format!("SELECT wait_event FROM pg_stat_activity WHERE pid={pid}"),1,JobId(9_251)).await;
                assert_eq!(seen.status,ExecutionStatus::Succeeded);
                if seen.statements[0].result_set.as_ref().is_some_and(|set|set.rows.iter().any(|row|row==&vec![Cell::Text("PgSleep".into())])) {break;}
            }
        }).await;
        drop(running);
        // This handle belongs only to the test's own session. No backend is
        // terminated and no controller is detached or left awaiting forever.
        let delivered=tokio::time::timeout(Duration::from_secs(2),cancel.cancel()).await;
        assert!(delivered.is_ok_and(|result|result.is_ok()),"owned cancellation request delivered");
        assert!(barrier.is_ok(),"actual second statement reached PgSleep");
        let report=fx.session.take_connection_notice_report().expect("abandoned prefix accounting");
        assert_eq!(report.counts.omitted,NoticeCount::Exact(1));assert_eq!(report.counts.abandoned,NoticeCount::Exact(256));
        let recovered=tokio::time::timeout(Duration::from_secs(3),fx.session.execute("SELECT 42",1,JobId(9_252))).await.unwrap();assert_eq!(recovered.status,ExecutionStatus::Succeeded);assert_eq!(recovered.statements[0].result_set.as_ref().unwrap().rows[0],vec![Cell::Text("42".into())]);assert!(!recovered.notice_report.counts.needs_report());assert_eq!(recovered.notice_report.checkpoint.totals.abandoned,NoticeCount::Exact(256));
    });
}

#[test]
fn notice_health_version_orders_and_transport() {
    let Some(fx) = Fixture::open() else {
        return;
    };
    use ignatius::connection::SslMode;
    use ignatius::postgres::TlsState;
    match fx.target.sslmode {
        SslMode::Disable => assert_eq!(fx.session.info().tls, TlsState::Disabled),
        SslMode::Prefer => assert!(matches!(
            fx.session.info().tls,
            TlsState::NotNegotiated
                | TlsState::Active {
                    mode: SslMode::Prefer,
                    ..
                }
        )),
        requested => assert!(matches!(
            fx.session.info().tls,
            TlsState::Active { mode, .. } if mode == requested
        )),
    }
    assert!(fx.session.info().read_only);
    eprintln!(
        "verified fixture: PostgreSQL {}; {}; read-only; orders=3",
        fx.session.info().server_version,
        fx.session.info().tls.description()
    );
}
