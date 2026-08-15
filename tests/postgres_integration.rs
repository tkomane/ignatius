//! Integration tests against a real PostgreSQL server.
//!
//! Protocol claims are only worth making with a real server behind them, so
//! everything here talks to a disposable container started by
//! `docker/compose.yaml`. All data is synthetic; no test may ever require a
//! personal or production database.
//!
//! Set `IGNATIUS_TEST_PG_URI` to run these. Without it they skip, because a
//! missing container is an environment limitation, not a product regression.
//! CI sets the variable, so there the tests must run.

// Tests report skips to the developer running them; that is what stderr is for.
#![allow(clippy::print_stderr)]

use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, EnvSnapshot, SslMode, resolve};
use ignatius::postgres::{TlsState, session};
use ignatius::query::result::{ExecutionStatus, JobId};
use ignatius::query::value::Cell;
use std::time::{Duration, Instant};

/// Reads the target from the environment, or skips the test.
macro_rules! target_or_skip {
    () => {
        match std::env::var("IGNATIUS_TEST_PG_URI") {
            Ok(uri) if !uri.is_empty() => uri,
            _ => {
                eprintln!(
                    "skipping: IGNATIUS_TEST_PG_URI is not set. Start the container with \
                     `docker compose -f docker/compose.yaml up -d` and export the URI."
                );
                return;
            }
        }
    };
}

/// A live session together with the runtime that owns its connection task.
///
/// The two are kept together deliberately. `tokio-postgres` drives a connection
/// on a spawned task, so dropping the runtime closes the connection underneath
/// the client. Holding both in one value makes that impossible to get wrong.
struct Fixture {
    runtime: tokio::runtime::Runtime,
    session: session::Session,
}

impl Fixture {
    fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.runtime.block_on(future)
    }
}

fn fixture(uri: &str) -> Fixture {
    let config = Config::default();
    let target = resolve(
        Some(uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let session = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("connects");
    Fixture { runtime, session }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("runtime")
}

#[test]
fn select_one_returns_one_row_with_the_expected_value() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute("SELECT 1 AS one", 100, JobId(1)));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
    let set = execution.statements[0].result_set.as_ref().expect("rows");
    assert_eq!(set.columns, vec!["one".to_owned()]);
    assert_eq!(set.rows, vec![vec![Cell::Text("1".into())]]);
    assert_eq!(set.rows_seen, 1);
    assert!(!set.is_truncated());
}

#[test]
fn the_session_reports_what_the_server_says_about_itself() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let info = fx.session.info();

    assert!(!info.server_version.is_empty());
    assert!(
        info.backend_pid.parse::<i32>().is_ok(),
        "backend pid is a number"
    );
    assert!(!info.search_path.is_empty());
    // The container speaks plain TCP, and the client must say so rather than
    // implying the connection is protected.
    assert!(
        matches!(info.tls, TlsState::NotNegotiated | TlsState::Active { .. }),
        "unexpected TLS state: {:?}",
        info.tls
    );
    if info.tls == TlsState::NotNegotiated {
        assert!(!info.tls.is_encrypted());
        assert!(info.tls.description().contains("not encrypted"));
    }
}

#[test]
fn a_server_error_carries_sqlstate_and_a_position() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT * FROM definitely_not_a_table",
        100,
        JobId(1),
    ));

    assert_eq!(execution.status, ExecutionStatus::Failed);
    let error = execution.error.expect("diagnostic");
    assert!(
        error.headline.contains("definitely_not_a_table"),
        "{}",
        error.headline
    );
    let sqlstate = error
        .technical
        .iter()
        .find(|f| f.label == "SQLSTATE")
        .expect("SQLSTATE reported");
    assert_eq!(sqlstate.value, "42P01");
    assert!(
        error.next_action.is_some(),
        "an error must say what to do next"
    );
    assert!(error.position.is_some(), "the server reported a position");
}

#[test]
fn a_long_statement_can_be_cancelled_and_the_server_confirms_it() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let cancel = fx.session.cancel_handle();

    let started = Instant::now();
    let execution = fx.block_on(async {
        let running = fx.session.execute("SELECT pg_sleep(30)", 100, JobId(7));
        let stopper = async {
            // Give the statement time to reach the server, then ask it to stop.
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel
                .cancel()
                .await
                .expect("cancellation request is delivered");
        };
        let (execution, ()) = tokio::join!(running, stopper);
        execution
    });

    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancellation should not wait out the full sleep, took {:?}",
        started.elapsed()
    );
    assert_eq!(
        execution.status,
        ExecutionStatus::Cancelled,
        "the server should confirm cancellation: {:?}",
        execution.error
    );
    let error = execution.error.expect("diagnostic");
    assert_eq!(
        error
            .technical
            .iter()
            .find(|f| f.label == "SQLSTATE")
            .expect("SQLSTATE")
            .value,
        "57014",
        "PostgreSQL reports query_canceled"
    );
    assert_eq!(error.exit_code(), ignatius::ExitCode::Cancelled);
}

#[test]
fn results_are_bounded_by_the_row_cap_and_report_the_true_count() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT generate_series(1, 5000) AS n",
        10,
        JobId(1),
    ));

    let set = execution.statements[0].result_set.as_ref().expect("rows");
    assert_eq!(set.rows.len(), 10, "memory stays bounded at the cap");
    assert_eq!(set.rows_seen, 5000, "the true count is still known");
    assert!(set.is_truncated());
    assert!(
        set.window_label().contains("5000"),
        "{}",
        set.window_label()
    );
}

#[test]
fn null_is_distinguishable_from_an_empty_string_over_the_wire() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT NULL::text AS a, ''::text AS b, 'NULL'::text AS c",
        100,
        JobId(1),
    ));

    let row = &execution.statements[0]
        .result_set
        .as_ref()
        .expect("rows")
        .rows[0];
    assert_eq!(row[0], Cell::Null);
    assert_eq!(row[1], Cell::Text(String::new()));
    assert_eq!(row[2], Cell::Text("NULL".into()));
    assert_ne!(row[0].display(), row[1].display());
    assert_ne!(row[0].display(), row[2].display());
}

#[test]
fn values_keep_the_precision_and_meaning_the_server_gave_them() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SET TIME ZONE 'Africa/Johannesburg'; \
         SELECT 123456789.12345678::numeric(20,8) AS exact, \
                1.100::numeric AS trailing, \
                '2026-08-15 12:00:00+02'::timestamptz AS moment",
        100,
        JobId(1),
    ));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
    let set = execution
        .statements
        .iter()
        .find_map(|s| s.result_set.as_ref())
        .expect("rows");
    let row = &set.rows[0];
    assert_eq!(
        row[0],
        Cell::Text("123456789.12345678".into()),
        "no float rounding"
    );
    assert_eq!(
        row[1],
        Cell::Text("1.100".into()),
        "trailing zeros are the server's"
    );
    let moment = row[2].raw().expect("timestamp");
    assert!(moment.contains("+02"), "the offset survives: {moment}");
}

#[test]
fn every_common_type_including_unknown_ones_renders_safely() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT as_int, as_bigint, as_numeric, as_bool, as_uuid, as_json, as_array, \
                as_bytea, as_interval, as_inet, as_range, as_ts FROM type_coverage",
        100,
        JobId(1),
    ));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
    let set = execution.statements[0].result_set.as_ref().expect("rows");
    assert_eq!(set.columns.len(), 12);
    for (column, cell) in set.columns.iter().zip(&set.rows[0]) {
        let rendered = cell.display();
        assert!(!rendered.is_empty(), "{column} rendered as nothing");
        assert!(!rendered.contains('\u{1b}'), "{column} emitted an escape");
    }
    let row = &set.rows[0];
    assert_eq!(
        row[1],
        Cell::Text("9223372036854775807".into()),
        "bigint is exact"
    );
    assert_eq!(
        row[7],
        Cell::Text("\\xdeadbeef".into()),
        "bytea keeps its hex form"
    );
    assert_eq!(
        row[10],
        Cell::Text("[1,10)".into()),
        "range keeps its bounds"
    );
}

#[test]
fn hostile_values_from_the_database_cannot_drive_the_terminal() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT value FROM rendering_cases WHERE label = 'escape_attack'",
        100,
        JobId(1),
    ));

    let cell = &execution.statements[0]
        .result_set
        .as_ref()
        .expect("rows")
        .rows[0][0];
    assert!(
        cell.raw().expect("text").contains('\u{1b}'),
        "the raw value really does contain an escape"
    );
    assert!(
        !cell.display().contains('\u{1b}'),
        "the displayed value must not: {}",
        cell.display()
    );
}

#[test]
fn multiple_statements_produce_multiple_results_in_order() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT 1 AS first; SELECT 2 AS second; SELECT 3 AS third;",
        100,
        JobId(1),
    ));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
    assert_eq!(execution.statements.len(), 3);
    for (index, expected) in ["first", "second", "third"].iter().enumerate() {
        let set = execution.statements[index]
            .result_set
            .as_ref()
            .expect("rows");
        assert_eq!(set.columns, vec![(*expected).to_owned()]);
    }
}

#[test]
fn a_statement_after_a_failure_does_not_run() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT 1; SELECT * FROM no_such_table; SELECT 2;",
        100,
        JobId(1),
    ));

    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert_eq!(
        execution.statements.len(),
        1,
        "the first statement ran and the third did not"
    );
    assert!(execution.error.is_some());
}

#[test]
fn server_notices_reach_the_client() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "DO $$ BEGIN RAISE NOTICE 'synthetic notice for the test suite'; END $$;",
        100,
        JobId(1),
    ));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
    let notices: Vec<_> = execution
        .statements
        .iter()
        .flat_map(|s| s.notices.clone())
        .collect();
    assert!(
        notices
            .iter()
            .any(|n| n.message.contains("synthetic notice")),
        "notices were dropped: {notices:?}"
    );
}

#[test]
fn a_failed_transaction_is_reported_and_rollback_recovers_it() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    fx.block_on(fx.session.execute("BEGIN", 100, JobId(1)));
    let failed = fx.block_on(
        fx.session
            .execute("SELECT * FROM no_such_table", 100, JobId(2)),
    );
    assert_eq!(failed.status, ExecutionStatus::Failed);

    // Inside a failed transaction the server refuses everything until it ends.
    let blocked = fx.block_on(fx.session.execute("SELECT 1", 100, JobId(3)));
    assert_eq!(blocked.status, ExecutionStatus::Failed);
    let error = blocked.error.expect("diagnostic");
    assert_eq!(
        error
            .technical
            .iter()
            .find(|f| f.label == "SQLSTATE")
            .expect("SQLSTATE")
            .value,
        "25P02"
    );
    assert_eq!(
        error.next_action.as_deref(),
        Some("run ROLLBACK to end the failed transaction"),
        "the client must name the recovery action"
    );

    let recovered = fx.block_on(fx.session.execute("ROLLBACK; SELECT 1", 100, JobId(4)));
    assert_eq!(recovered.status, ExecutionStatus::Succeeded);
}

#[test]
fn the_drivers_tls_error_wording_is_still_what_the_mapping_expects() {
    let uri = target_or_skip!();
    let config = Config::default();
    let target = resolve(
        Some(&format!("{uri}?sslmode=require")),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");

    // The demo server speaks no TLS, so this is a real TLS-class failure from
    // the driver. If the driver ever rewords it, the mapping in
    // `postgres::error` would silently start calling TLS failures network
    // failures, so the string is pinned here against the live driver.
    let error = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("a server without TLS must refuse sslmode=require");
    assert_eq!(
        error.exit_code(),
        ignatius::ExitCode::Tls,
        "a refusal to encrypt is not a network problem: {error:?}"
    );
}

#[test]
fn requiring_tls_against_a_server_without_it_fails_rather_than_downgrading() {
    let uri = target_or_skip!();
    let config = Config::default();
    let target = resolve(
        Some(&format!("{uri}?sslmode=require")),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");
    assert_eq!(target.sslmode, SslMode::Require);

    let result = runtime().block_on(session::connect(&target, Duration::ZERO));
    match result {
        Err(diagnostic) => {
            // The connection failed. What must never happen is a silent fallback
            // to an unencrypted session.
            assert!(
                diagnostic.next_action.is_some(),
                "a refusal must explain the next step"
            );
        }
        Ok(session) => {
            // If the server does offer TLS, the session must actually be encrypted.
            assert!(
                session.info().tls.is_encrypted(),
                "sslmode=require produced an unencrypted session: {:?}",
                session.info().tls
            );
        }
    }
}

#[test]
fn a_restricted_role_can_still_connect_and_read_what_it_is_granted() {
    let uri = target_or_skip!();
    // Rebuild the URI for the restricted role defined in docker/init/01-demo.sql.
    let Some((prefix, rest)) = uri.split_once("://") else {
        panic!("expected a URI");
    };
    let Some((_, host_and_db)) = rest.split_once('@') else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };
    let restricted =
        format!("{prefix}://restricted_reader:not-a-real-password-restricted@{host_and_db}");

    let fx = fixture(&restricted);

    let allowed = fx.block_on(
        fx.session
            .execute("SELECT count(*) FROM orders", 100, JobId(1)),
    );
    assert_eq!(
        allowed.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        allowed.error
    );

    let denied = fx.block_on(
        fx.session
            .execute("SELECT * FROM type_coverage", 100, JobId(2)),
    );
    assert_eq!(denied.status, ExecutionStatus::Failed);
    let error = denied.error.expect("diagnostic");
    assert_eq!(
        error
            .technical
            .iter()
            .find(|f| f.label == "SQLSTATE")
            .expect("SQLSTATE")
            .value,
        "42501",
        "permission denied is reported as itself"
    );
    assert!(
        error.likely_cause.is_some(),
        "a permission error explains itself"
    );
}

#[test]
fn a_wrong_password_is_reported_as_authentication_not_as_a_generic_failure() {
    let uri = target_or_skip!();
    let Some((prefix, rest)) = uri.split_once("://") else {
        panic!("expected a URI");
    };
    let Some((_, host_and_db)) = rest.split_once('@') else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };
    let wrong = format!("{prefix}://ignatius_test:definitely-the-wrong-password@{host_and_db}");

    let config = Config::default();
    let target = resolve(
        Some(&wrong),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("target resolves");

    let error = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("wrong password must fail");

    assert_eq!(error.exit_code(), ignatius::ExitCode::Authentication);
    assert!(
        !error
            .render_plain(true)
            .contains("definitely-the-wrong-password"),
        "the diagnostic must never echo the password"
    );
}

// ---------------------------------------------------------------- catalogue

#[test]
fn schemas_are_listed_with_counts_in_a_constant_number_of_round_trips() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let schemas = fx.block_on(fx.session.schemas()).expect("schemas");

    let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"public"), "{names:?}");
    assert!(names.contains(&"reporting"), "{names:?}");
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("pg_") || *n == "information_schema"),
        "system schemas are noise and are excluded: {names:?}"
    );

    let public = schemas.iter().find(|s| s.name == "public").expect("public");
    assert!(public.usable);
    assert!(
        public.count(ignatius::postgres::ObjectKind::Table) >= 4,
        "counts are populated: {:?}",
        public.counts
    );

    let reporting = schemas
        .iter()
        .find(|s| s.name == "reporting")
        .expect("reporting");
    assert_eq!(reporting.count(ignatius::postgres::ObjectKind::View), 1);
    assert_eq!(reporting.count(ignatius::postgres::ObjectKind::Function), 1);
    assert_eq!(
        reporting.count(ignatius::postgres::ObjectKind::Table),
        0,
        "a kind with none present reports zero rather than being absent"
    );
}

#[test]
fn objects_are_listed_by_kind_within_a_schema() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    use ignatius::postgres::ObjectKind;

    let tables = fx
        .block_on(fx.session.objects("public", ObjectKind::Table))
        .expect("tables");
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"orders"), "{names:?}");
    assert!(names.contains(&"type_coverage"), "{names:?}");
    for table in &tables {
        assert_eq!(table.kind, ObjectKind::Table);
        assert_eq!(table.schema, "public");
    }

    let views = fx
        .block_on(fx.session.objects("reporting", ObjectKind::View))
        .expect("views");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "order_totals");

    let functions = fx
        .block_on(fx.session.objects("reporting", ObjectKind::Function))
        .expect("functions");
    assert_eq!(functions.len(), 1);
    assert!(
        functions[0]
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("bigint"),
        "a function shows what it returns: {:?}",
        functions[0].detail
    );
}

#[test]
fn a_hostile_object_name_is_listed_and_quoted_rather_than_executed() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    use ignatius::postgres::ObjectKind;

    let tables = fx
        .block_on(fx.session.objects("public", ObjectKind::Table))
        .expect("tables");
    let hostile = tables
        .iter()
        .find(|t| t.name.contains("DROP TABLE"))
        .expect("the hostile fixture is listed like any other table");

    // The name round-trips into SQL that selects from it, rather than running it.
    let sql = format!("SELECT note FROM {} WHERE id = 1", hostile.qualified_sql());
    let execution = fx.block_on(fx.session.execute(&sql, 10, JobId(1)));
    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "quoting failed: {:?}",
        execution.error
    );
    let row = &execution.statements[0]
        .result_set
        .as_ref()
        .expect("rows")
        .rows[0];
    assert_eq!(row[0], Cell::Text("still here".into()));

    // And the table it tried to name in its own text is untouched.
    let orders = fx.block_on(
        fx.session
            .execute("SELECT count(*) FROM orders", 10, JobId(2)),
    );
    assert_eq!(orders.status, ExecutionStatus::Succeeded);
}

#[test]
fn columns_carry_type_nullability_and_key_membership() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let columns = fx
        .block_on(fx.session.columns("public", "orders"))
        .expect("columns");

    let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "order_id",
            "customer_id",
            "total",
            "currency",
            "note",
            "created_at"
        ],
        "columns come back in catalogue order"
    );

    let id = &columns[0];
    assert!(id.primary_key, "order_id is the primary key");
    assert!(!id.nullable);

    let note = columns.iter().find(|c| c.name == "note").expect("note");
    assert!(note.nullable, "note has no NOT NULL constraint");
    assert!(!note.primary_key);

    let total = columns.iter().find(|c| c.name == "total").expect("total");
    assert_eq!(
        total.data_type, "numeric(12,2)",
        "the declared type is preserved"
    );

    let currency = columns
        .iter()
        .find(|c| c.name == "currency")
        .expect("currency");
    assert!(
        currency
            .default
            .as_deref()
            .unwrap_or_default()
            .contains("ZAR"),
        "a default is reported: {:?}",
        currency.default
    );
}

#[test]
fn indexes_and_extensions_are_listed() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let indexes = fx
        .block_on(fx.session.indexes("public", "orders"))
        .expect("indexes");
    assert!(!indexes.is_empty(), "the primary key has an index");
    assert!(
        indexes
            .iter()
            .any(|i| i.detail.as_deref() == Some("primary key")),
        "{indexes:?}"
    );

    let extensions = fx.block_on(fx.session.extensions()).expect("extensions");
    assert!(
        extensions.iter().any(|e| e.name == "plpgsql"),
        "plpgsql is installed by default: {extensions:?}"
    );
    assert!(
        extensions[0]
            .detail
            .as_deref()
            .unwrap_or_default()
            .starts_with("version ")
    );
}

#[test]
fn a_restricted_role_sees_objects_it_cannot_read_and_they_are_marked_as_such() {
    let uri = target_or_skip!();
    let Some((prefix, rest)) = uri.split_once("://") else {
        panic!("expected a URI");
    };
    let Some((_, host_and_db)) = rest.split_once('@') else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };
    let restricted =
        format!("{prefix}://restricted_reader:not-a-real-password-restricted@{host_and_db}");
    let fx = fixture(&restricted);
    use ignatius::postgres::ObjectKind;

    // The catalogue is readable by everyone, so the tree can still be drawn.
    let schemas = fx.block_on(fx.session.schemas()).expect("schemas");
    assert!(schemas.iter().any(|s| s.name == "public"));

    let tables = fx
        .block_on(fx.session.objects("public", ObjectKind::Table))
        .expect("tables");
    let granted = tables.iter().find(|t| t.name == "orders").expect("orders");
    assert!(granted.readable, "the role was granted SELECT on orders");

    let denied = tables
        .iter()
        .find(|t| t.name == "secrets_of_the_realm")
        .expect("listed even though it cannot be read");
    assert!(
        !denied.readable,
        "an object the role cannot read must be marked, not hidden or fatal"
    );
}
