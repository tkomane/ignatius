//! W04 privacy evidence through the production planner and PostgreSQL adapter.
//!
//! Each scenario runs in a separate copy of this test process so the production
//! global tracing subscriber writes only to an isolated profile. These checks
//! exercise the adapter, generated templates, history and diagnostic formats;
//! they do not drive the full-screen review or a packaged release candidate.

#![allow(clippy::print_stderr)]

use ignatius::app::Effect;
use ignatius::config::{Config, Paths};
use ignatius::connection::{ConnectionArgs, EnvSnapshot, resolve};
use ignatius::diagnostics::logging::{self, LoggingState};
use ignatius::history::{Entry, History, Outcome, Recorded};
use ignatius::postgres::session::{self, Session};
use ignatius::query::{Cell, ExecutionStatus, JobId, ParameterBindings, UpdateColumn, UpdatePlan};
use secrecy::SecretString;
use std::process::Command;
use std::time::Duration;

const CHILD_SCENARIO: &str = "IGNATIUS_RUNTIME_PRIVACY_CHILD";
const KEY: &str = "w04-update-key-2a9e";
const REPLACEMENT: &str = "w04-update-replacement-7f1c ' \\ unicode ✓";
const TABLE: &str = "runtime_privacy_probe";

fn is_child(scenario: &str) -> bool {
    std::env::var(CHILD_SCENARIO).as_deref() == Ok(scenario)
}

fn isolated_child(test_name: &str, scenario: &str) -> Option<tempfile::TempDir> {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
        return None;
    };
    if uri.is_empty() {
        eprintln!("skipping: IGNATIUS_TEST_PG_URI is empty");
        return None;
    }

    let evidence = tempfile::tempdir().expect("isolated privacy profile");
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", test_name, "--nocapture"])
        .env(CHILD_SCENARIO, scenario)
        .env("IGNATIUS_TEST_PG_URI", uri)
        .env("IGNATIUS_CONFIG_DIR", evidence.path())
        .env("IGNATIUS_DATA_DIR", evidence.path().join("data"))
        .env("IGNATIUS_LOG", "debug")
        .output()
        .expect("run isolated production-adapter privacy scenario");
    assert!(
        output.status.success(),
        "the isolated {scenario} scenario failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for stream in [&output.stdout, &output.stderr] {
        assert_no_values(&String::from_utf8_lossy(stream), "child process output");
    }
    Some(evidence)
}

struct Fixture {
    paths: Paths,
    runtime: tokio::runtime::Runtime,
    session: Session,
}

impl Fixture {
    fn open() -> Self {
        let paths = Paths::resolve();
        assert!(matches!(
            logging::init(&paths.log_dir),
            LoggingState::Enabled { .. }
        ));
        let uri = std::env::var("IGNATIUS_TEST_PG_URI").expect("injected fixture URI");
        let target = resolve(
            Some(&uri),
            &ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &Config::default().connection,
        )
        .expect("synthetic target resolves without reading the owner's environment or files");
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let session = runtime
            .block_on(session::connect(&target, Duration::from_secs(5)))
            .expect("disposable PostgreSQL session");
        Self {
            paths,
            runtime,
            session,
        }
    }

    fn plan(&self, replacement: &str, column_definition: &str, initial: &str) -> UpdatePlan {
        let setup = format!(
            "CREATE TEMP TABLE {TABLE} (id text PRIMARY KEY, note {column_definition}); \
             INSERT INTO {TABLE} VALUES ('{KEY}', '{initial}'); \
             SET search_path TO pg_temp"
        );
        let created = self
            .runtime
            .block_on(self.session.execute(&setup, 10, JobId(701)));
        assert_eq!(created.status, ExecutionStatus::Succeeded);

        let source_sql = format!("SELECT id, note FROM {TABLE}");
        let selected = self
            .runtime
            .block_on(self.session.execute(&source_sql, 10, JobId(702)));
        assert_eq!(selected.status, ExecutionStatus::Succeeded);
        let set = selected.statements[0]
            .result_set
            .as_ref()
            .expect("selected retained row");
        assert_eq!(set.rows.len(), 1);
        let source = ignatius::query::parse_update_source(&source_sql, &set.columns, 1)
            .expect("supported direct source");
        let relation = self
            .runtime
            .block_on(self.session.update_relation(None, TABLE))
            .expect("live relation metadata");
        assert!(relation.readable && relation.writable);
        let columns = relation
            .columns
            .iter()
            .map(|column| UpdateColumn {
                name: column.name.clone(),
                primary_key: column.primary_key,
            })
            .collect::<Vec<_>>();
        ignatius::query::plan_update(
            &source,
            &relation.schema,
            &relation.relation,
            &set.columns,
            &set.rows[0],
            &columns,
            replacement,
        )
        .expect("generated update plan")
    }
}

fn assert_no_values(text: &str, surface: &str) {
    for value in [
        KEY.to_owned(),
        "w04-update-replacement-7f1c".to_owned(),
        KEY.to_uppercase(),
        "w04-update-replacement-7f1c".to_uppercase(),
        KEY.chars().rev().collect(),
        REPLACEMENT.chars().rev().collect(),
        REPLACEMENT.chars().take(12).collect(),
        REPLACEMENT
            .chars()
            .take(12)
            .collect::<String>()
            .to_uppercase(),
    ] {
        assert!(
            !text.contains(&value),
            "a bound generated-update value reached {surface}"
        );
    }
}

fn capture_diagnostic(paths: &Paths, diagnostic: &ignatius::diagnostics::Diagnostic, state: &str) {
    assert!(
        diagnostic
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == state),
        "the scenario did not produce its expected server SQLSTATE"
    );
    std::fs::write(
        paths.data_dir.join("diagnostic.txt"),
        format!(
            "{}\n{}\n{diagnostic:?}",
            diagnostic.render_plain(true),
            diagnostic.to_json()
        ),
    )
    .expect("capture actual diagnostic surfaces");
}

fn value_bindings() -> ParameterBindings {
    ParameterBindings::from_secrets(
        vec!["value".into()],
        vec![SecretString::from(REPLACEMENT.to_owned())],
    )
    .expect("synthetic value binding")
}

fn assert_update_log(paths: &Paths) {
    let log = std::fs::read_to_string(paths.log_dir.join("ignatius.log"))
        .expect("production debug log exists");
    assert!(
        log.contains("job=703 statement_chars="),
        "the generated update produced no statement descriptor"
    );
    assert_no_values(&log, "debug log");
    assert!(!log.contains(TABLE), "SQL text reached the debug log");
    assert!(
        !log.contains("tokio_postgres"),
        "dependency tracing reached the debug log"
    );
}

#[test]
fn a_successful_generated_update_keeps_bound_values_out_of_runtime_storage() {
    if is_child("success") {
        let fixture = Fixture::open();
        let plan = fixture.plan(REPLACEMENT, "text", "before");
        let reviewed = plan.bound_sql().expect("bound review statement");
        assert!(reviewed.contains(KEY));
        assert!(reviewed.contains("w04-update-replacement-7f1c"));
        let applied = fixture
            .runtime
            .block_on(fixture.session.execute_with_parameters(
                &plan.sql_template,
                &plan.parameters,
                10,
                JobId(703),
            ));
        assert_eq!(applied.status, ExecutionStatus::Succeeded);
        assert_eq!(applied.statements[0].rows_affected, Some(1));
        assert!(applied.error.is_none());
        let after = fixture.runtime.block_on(fixture.session.execute(
            &format!("SELECT note FROM {TABLE}"),
            10,
            JobId(704),
        ));
        assert_eq!(after.status, ExecutionStatus::Succeeded);
        let rows = &after.statements[0]
            .result_set
            .as_ref()
            .expect("after row")
            .rows;
        assert_eq!(rows, &[vec![Cell::Text(REPLACEMENT.into())]]);

        let entry = Entry::now(
            &fixture.session.info().target,
            &fixture.session.info().database,
            "test",
            &plan.sql_template,
            Outcome::from_status(&applied.status),
            applied.elapsed,
        );
        let history = History::open(&fixture.paths, &Config::default().history, false);
        assert_eq!(
            history.record(&entry).expect("record template"),
            Recorded::Written
        );
        assert_eq!(history.entries().expect("read history"), vec![entry]);
        let effect = Effect::ExecuteParameterized {
            job: JobId(703),
            sql: plan.sql_template,
            parameters: plan.parameters,
        };
        std::fs::write(
            fixture.paths.data_dir.join("effect-debug.txt"),
            format!("{effect:?}"),
        )
        .expect("capture effect Debug surface");
        return;
    }

    let Some(evidence) = isolated_child(
        "a_successful_generated_update_keeps_bound_values_out_of_runtime_storage",
        "success",
    ) else {
        return;
    };
    let paths = Paths::rooted_at(evidence.path());
    assert_update_log(&paths);
    let history = std::fs::read_to_string(&paths.history_file).expect("recorded history file");
    assert!(history.contains(":__ignatius_new_value"));
    assert!(history.contains(":__ignatius_key_0"));
    assert_no_values(&history, "history file");
    let debug = std::fs::read_to_string(paths.data_dir.join("effect-debug.txt"))
        .expect("captured effect Debug surface");
    assert!(debug.contains("ExecuteParameterized"));
    assert_no_values(&debug, "effect Debug output");
}

#[test]
fn a_rejected_generated_update_keeps_bound_values_out_of_server_diagnostics() {
    if is_child("constraint-failure") {
        let fixture = Fixture::open();
        let plan = fixture.plan(REPLACEMENT, "text CHECK (note = 'before')", "before");
        let rejected = fixture
            .runtime
            .block_on(fixture.session.execute_with_parameters(
                &plan.sql_template,
                &plan.parameters,
                10,
                JobId(703),
            ));
        assert_eq!(rejected.status, ExecutionStatus::Failed);
        let diagnostic = rejected.error.expect("server constraint diagnostic");
        capture_diagnostic(&fixture.paths, &diagnostic, "23514");
        return;
    }

    let Some(evidence) = isolated_child(
        "a_rejected_generated_update_keeps_bound_values_out_of_server_diagnostics",
        "constraint-failure",
    ) else {
        return;
    };
    let paths = Paths::rooted_at(evidence.path());
    assert_update_log(&paths);
    let diagnostic = std::fs::read_to_string(paths.data_dir.join("diagnostic.txt"))
        .expect("actual server diagnostic evidence");
    assert!(
        diagnostic.contains("23514"),
        "no server constraint failure was exercised"
    );
    assert_no_values(&diagnostic, "plain/JSON/Debug server diagnostics");
}

#[test]
fn a_parameterized_type_error_keeps_its_successful_prefix_without_replay_or_value_disclosure() {
    if is_child("type-error") {
        let fixture = Fixture::open();
        let setup = fixture.runtime.block_on(fixture.session.execute(
            "CREATE TEMP TABLE runtime_privacy_count (n integer)",
            10,
            JobId(700),
        ));
        assert_eq!(setup.status, ExecutionStatus::Succeeded);
        let failed = fixture
            .runtime
            .block_on(fixture.session.execute_with_parameters(
                "INSERT INTO runtime_privacy_count VALUES (1); \
             SELECT :value::integer; \
             INSERT INTO runtime_privacy_count VALUES (2)",
                &value_bindings(),
                10,
                JobId(703),
            ));
        assert_eq!(failed.status, ExecutionStatus::Failed);
        assert_eq!(
            failed.statements.len(),
            1,
            "only the successful prefix is retained"
        );
        assert_eq!(failed.statements[0].rows_affected, Some(1));
        assert_eq!(
            failed.transaction,
            ignatius::query::result::TransactionState::Autocommit
        );
        let diagnostic = failed.error.expect("server invalid-input diagnostic");
        assert_eq!(diagnostic.statement_number, Some(2));
        capture_diagnostic(&fixture.paths, &diagnostic, "22P02");
        let count = fixture.runtime.block_on(fixture.session.execute(
            "SELECT count(*) FROM runtime_privacy_count",
            10,
            JobId(704),
        ));
        assert_eq!(count.status, ExecutionStatus::Succeeded);
        assert_eq!(
            count.statements[0]
                .result_set
                .as_ref()
                .expect("prefix count")
                .rows,
            vec![vec![Cell::Text("1".into())]],
            "the prefix must execute once and the suffix must never run"
        );
        return;
    }
    let Some(evidence) = isolated_child(
        "a_parameterized_type_error_keeps_its_successful_prefix_without_replay_or_value_disclosure",
        "type-error",
    ) else {
        return;
    };
    let paths = Paths::rooted_at(evidence.path());
    assert_update_log(&paths);
    let diagnostic = std::fs::read_to_string(paths.data_dir.join("diagnostic.txt"))
        .expect("actual invalid-input diagnostic");
    assert!(diagnostic.contains("22P02"));
    assert_no_values(&diagnostic, "invalid-input diagnostic formats");
}

#[test]
fn a_parameterized_stream_omits_transformed_values_from_server_errors() {
    if is_child("stream-error") {
        let fixture = Fixture::open();
        let result = fixture
            .runtime
            .block_on(fixture.session.stream_with_parameters(
                "SELECT upper(left(:value, 12))::integer",
                &value_bindings(),
                &mut |_| Ok(()),
            ));
        let Err(ignatius::postgres::session::StreamStop::Server(diagnostic)) = result else {
            panic!("the export must fail with a server invalid-input diagnostic");
        };
        assert_eq!(diagnostic.statement_number, Some(1));
        capture_diagnostic(&fixture.paths, &diagnostic, "22P02");
        return;
    }
    let Some(evidence) = isolated_child(
        "a_parameterized_stream_omits_transformed_values_from_server_errors",
        "stream-error",
    ) else {
        return;
    };
    let paths = Paths::rooted_at(evidence.path());
    let diagnostic = std::fs::read_to_string(paths.data_dir.join("diagnostic.txt"))
        .expect("actual stream diagnostic");
    assert!(diagnostic.contains("22P02"));
    assert_no_values(&diagnostic, "parameterized stream diagnostic formats");
}

#[test]
fn parameterized_notice_privacy_covers_the_backlog_and_late_messages_after_stream_failure() {
    if is_child("late-notices") {
        let fixture = Fixture::open();
        let ordinary = fixture.runtime.block_on(fixture.session.execute(
            "DO $$ BEGIN RAISE NOTICE 'ordinary visible notice'; END $$",
            10,
            JobId(700),
        ));
        assert_eq!(ordinary.status, ExecutionStatus::Succeeded);
        assert!(
            ordinary.statements[0]
                .notices
                .iter()
                .any(|notice| { notice.message == "ordinary visible notice" })
        );

        let setup = fixture.runtime.block_on(fixture.session.execute(
            "CREATE FUNCTION pg_temp.runtime_privacy_emit(value text) RETURNS integer \
             LANGUAGE plpgsql AS $$ BEGIN \
             PERFORM pg_sleep(0.05); \
             RAISE NOTICE 'upper:% reverse:% truncated:%', upper(value), reverse(value), left(value, 12); \
             RETURN 1; END $$",
            10,
            JobId(701),
        ));
        assert_eq!(setup.status, ExecutionStatus::Succeeded);
        fixture
            .runtime
            .block_on(fixture.session.stream(
                "DO $$ BEGIN RAISE NOTICE 'backlog notice'; END $$",
                &mut |_| Ok(()),
            ))
            .expect("leave one ordinary notice in the backlog");

        let stopped = fixture
            .runtime
            .block_on(fixture.session.stream_with_parameters(
                "SELECT :value AS intentional_result; SELECT pg_temp.runtime_privacy_emit(:value)",
                &value_bindings(),
                &mut |_| Err(std::io::Error::other("synthetic export sink stopped")),
            ));
        assert!(matches!(
            stopped,
            Err(ignatius::postgres::session::StreamStop::Sink(_))
        ));
        // This round trip is a protocol barrier: the abandoned stream's later
        // statement and notice must have arrived before its response can do so.
        assert_eq!(
            fixture
                .runtime
                .block_on(fixture.session.stream("SELECT 1", &mut |_| Ok(())))
                .expect("complete the notice protocol barrier"),
            1
        );
        let notices = fixture.session.take_notices();
        assert_eq!(
            notices.len(),
            2,
            "both queued and late notices must be observed"
        );
        for notice in &notices {
            assert!(notice.message.contains("omitted"));
            assert_eq!(notice.severity, "NOTICE");
            assert_eq!(notice.code.as_deref(), Some("00000"));
        }

        let later = fixture.runtime.block_on(fixture.session.execute(
            "DO $$ BEGIN RAISE NOTICE 'later ordinary notice'; END $$",
            10,
            JobId(704),
        ));
        assert_eq!(later.status, ExecutionStatus::Succeeded);
        assert_eq!(later.statements[0].notices.len(), 1);
        assert!(later.statements[0].notices[0].message.contains("omitted"));
        std::fs::write(
            fixture.paths.data_dir.join("notices.txt"),
            format!("{notices:?}\n{:?}", later.statements[0].notices),
        )
        .expect("capture queued, late and subsequent notice surfaces");
        return;
    }

    let Some(evidence) = isolated_child(
        "parameterized_notice_privacy_covers_the_backlog_and_late_messages_after_stream_failure",
        "late-notices",
    ) else {
        return;
    };
    let paths = Paths::rooted_at(evidence.path());
    let notices = std::fs::read_to_string(paths.data_dir.join("notices.txt"))
        .expect("actual notice evidence");
    assert_eq!(notices.matches("omitted").count(), 3);
    assert_no_values(&notices, "queued and late notice representations");
    assert!(!notices.contains("backlog notice"));
    assert!(!notices.contains("later ordinary notice"));
    let log =
        std::fs::read_to_string(paths.log_dir.join("ignatius.log")).expect("production debug log");
    assert_no_values(&log, "notice scenario debug log");
}
