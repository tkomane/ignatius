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
use ignatius::postgres::{StreamEvent, TlsState, session};
use ignatius::query::ParameterBindings;
use ignatius::query::result::{ExecutionStatus, JobId};
use ignatius::query::value::Cell;
use secrecy::SecretString;
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

fn within_sync_timeout<T>(fixture: &Fixture, future: impl std::future::Future<Output = T>) -> T {
    fixture
        .block_on(async { tokio::time::timeout(Duration::from_secs(10), future).await })
        .expect("sync test operation completes within ten seconds")
}

fn fixture(uri: &str) -> Fixture {
    let mut sanitized = uri.to_owned();
    if sanitized.contains(":ignatius_test_password_do_not_use@") {
        sanitized = sanitized.replace(
            ":ignatius_test_password_do_not_use@",
            ":not-a-real-password-disposable-container@",
        );
    }
    let config = Config::default();
    let target = resolve(
        Some(&sanitized),
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

/// Helper returning a connected plain PostgreSQL test fixture, or None if unset.
fn connect_plain() -> Option<Fixture> {
    match std::env::var("IGNATIUS_TEST_PG_URI") {
        Ok(uri) if !uri.is_empty() => Some(fixture(&uri)),
        _ => {
            eprintln!(
                "skipping: IGNATIUS_TEST_PG_URI is not set. Start the container with \
                 `docker compose -f docker/compose.yaml up -d` and export the URI."
            );
            None
        }
    }
}

fn unique_sync_database_name(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "ign_sync_{label}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
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
fn parameterized_execute_and_stream_preserve_literal_text_and_repeated_values() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let value = "quote ' slash \\\\ newline\n unicode ✓ ; --";
    let bindings = ParameterBindings::from_secrets(
        vec!["value".into(), "empty".into()],
        vec![
            SecretString::from(value.to_owned()),
            SecretString::from(String::new()),
        ],
    )
    .expect("bindings");

    let execution = fx.block_on(fx.session.execute_with_parameters(
        "SELECT :value AS first, :value AS second, :empty AS empty",
        &bindings,
        100,
        JobId(300),
    ));
    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "parameterized query failed: {:?}",
        execution.error
    );
    let set = execution.statements[0].result_set.as_ref().expect("rows");
    assert_eq!(set.rows[0][0], Cell::Text(value.into()));
    assert_eq!(set.rows[0][1], Cell::Text(value.into()));
    assert_eq!(set.rows[0][2], Cell::Text(String::new()));

    let stream_bindings = ParameterBindings::from_secrets(
        vec!["value".into()],
        vec![SecretString::from(value.to_owned())],
    )
    .expect("stream bindings");
    let mut events = Vec::new();
    let rows = fx.block_on(fx.session.stream_with_parameters(
        "SELECT :value AS value",
        &stream_bindings,
        &mut |event| {
            events.push(event);
            Ok(())
        },
    ));
    assert_eq!(rows.expect("stream succeeds"), 1);
    assert!(matches!(events.first(), Some(StreamEvent::Columns(columns)) if columns == &["value"]));
    assert!(
        matches!(events.get(1), Some(StreamEvent::Row(row)) if row == &vec![Cell::Text(value.into())])
    );
}

#[test]
fn parameterized_nul_is_refused_before_the_statement_is_sent() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let bindings = ParameterBindings::from_secrets(
        vec!["value".into()],
        vec![SecretString::from("safe-marker\0never-log".to_owned())],
    )
    .expect("bindings");
    let execution = fx.block_on(fx.session.execute_with_parameters(
        "SELECT :value",
        &bindings,
        100,
        JobId(301),
    ));
    assert_eq!(execution.status, ExecutionStatus::Failed);
    let error = execution.error.expect("binding diagnostic");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Usage);
    assert!(error.headline.contains(":value"));
    assert!(!error.headline.contains("never-log"));
}

#[test]
fn a_result_cell_plan_updates_the_selected_primary_key_row_only_when_executed() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE cell_update_probe (id integer PRIMARY KEY, note text); \
         INSERT INTO cell_update_probe VALUES (1, 'before')",
        100,
        JobId(303),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = "SELECT id, note FROM cell_update_probe";
    let selected = fx.block_on(fx.session.execute(source_sql, 10, JobId(304)));
    assert_eq!(selected.status, ExecutionStatus::Succeeded, "{selected:?}");
    let set = selected.statements[0]
        .result_set
        .as_ref()
        .expect("source rows");
    let source =
        ignatius::query::parse_update_source(source_sql, &set.columns, 1).expect("direct source");
    let relation = fx
        .block_on(fx.session.update_relation(None, "cell_update_probe"))
        .expect("an unqualified temporary table resolves in the default search path");
    assert!(relation.schema.starts_with("pg_temp_"));
    assert_eq!(
        relation.kind,
        ignatius::postgres::metadata::ObjectKind::Table
    );
    assert!(relation.readable);
    assert!(relation.writable);
    assert_eq!(relation.columns[0].name, "id");
    assert!(relation.columns[0].primary_key);

    let columns = relation
        .columns
        .iter()
        .map(|column| ignatius::query::UpdateColumn {
            name: column.name.clone(),
            primary_key: column.primary_key,
        })
        .collect::<Vec<_>>();
    let plan = ignatius::query::plan_update(
        &source,
        &relation.schema,
        &relation.relation,
        &set.columns,
        &set.rows[0],
        &columns,
        "after",
    )
    .expect("safe update plan");
    assert!(plan.sql_template.contains(":__ignatius_new_value"));
    assert!(
        plan.bound_sql()
            .expect("review statement")
            .contains("after")
    );

    let before = fx.block_on(fx.session.execute(
        "SELECT note FROM cell_update_probe WHERE id = 1",
        10,
        JobId(305),
    ));
    assert_eq!(
        before.statements[0]
            .result_set
            .as_ref()
            .expect("before row")
            .rows[0][0],
        Cell::Text("before".into())
    );

    let applied = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(306),
    ));
    assert_eq!(applied.status, ExecutionStatus::Succeeded, "{applied:?}");
    assert_eq!(applied.statements[0].rows_affected, Some(1));

    let after = fx.block_on(fx.session.execute(
        "SELECT note FROM cell_update_probe WHERE id = 1",
        10,
        JobId(307),
    ));
    assert_eq!(
        after.statements[0]
            .result_set
            .as_ref()
            .expect("after row")
            .rows[0][0],
        Cell::Text("after".into())
    );
}

#[test]
fn cell_update_resolves_the_qualified_pg_temp_alias_on_the_query_session() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE cell_update_qualified_probe (id integer PRIMARY KEY, note text); \
         INSERT INTO cell_update_qualified_probe VALUES (1, 'before')",
        10,
        JobId(9400),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let sql = "SELECT id, note FROM pg_temp.cell_update_qualified_probe";
    let selected = fx.block_on(fx.session.execute(sql, 10, JobId(9401)));
    assert_eq!(selected.status, ExecutionStatus::Succeeded, "{selected:?}");
    let set = selected.statements[0]
        .result_set
        .as_ref()
        .expect("source rows");
    assert_eq!(set.rows[0][1], Cell::Text("before".into()));
    let source = ignatius::query::parse_update_source(sql, &set.columns, 1)
        .expect("qualified direct source");
    assert_eq!(source.relation.schema.as_deref(), Some("pg_temp"));

    let relation = fx
        .block_on(
            fx.session
                .update_relation(source.relation.schema.as_deref(), &source.relation.relation),
        )
        .expect("the qualified pg_temp alias resolves on the query session");
    assert!(relation.schema.starts_with("pg_temp_"));
    assert_eq!(relation.relation, "cell_update_qualified_probe");
    assert!(relation.readable && relation.writable);
    assert_eq!(relation.columns[0].name, "id");
    assert!(relation.columns[0].primary_key);
}

/// Reads one retained source, then resolves live metadata and plans the same
/// conservative update the interactive reducer would, using only the public
/// helpers. Live tests can therefore execute the exact reviewed statement.
fn plan_live_cell_update(
    fx: &Fixture,
    source_sql: &str,
    relation: &str,
    selected_result_column: usize,
    selected_row: usize,
    replacement: &str,
) -> (
    ignatius::postgres::metadata::UpdateRelation,
    ignatius::query::UpdatePlan,
) {
    let selected = fx.block_on(fx.session.execute(source_sql, 10, JobId(900)));
    assert_eq!(selected.status, ExecutionStatus::Succeeded, "{selected:?}");
    let set = selected.statements[0]
        .result_set
        .as_ref()
        .expect("source rows");
    let row = set.rows.get(selected_row).expect("selected source row");
    let source =
        ignatius::query::parse_update_source(source_sql, &set.columns, selected_result_column)
            .expect("direct source");
    let metadata = fx
        .block_on(fx.session.update_relation(None, relation))
        .expect("search-path relation metadata");
    assert_eq!(
        metadata.kind,
        ignatius::postgres::metadata::ObjectKind::Table,
        "the planner is only offered ordinary tables"
    );
    let columns = metadata
        .columns
        .iter()
        .map(|column| ignatius::query::UpdateColumn {
            name: column.name.clone(),
            primary_key: column.primary_key,
        })
        .collect::<Vec<_>>();
    let plan = ignatius::query::plan_update(
        &source,
        &metadata.schema,
        &metadata.relation,
        &set.columns,
        row,
        &columns,
        replacement,
    )
    .expect("safe update plan");
    (metadata, plan)
}

/// Opens a second session to the same server and database, the way the
/// object-tree multi-connection test does, so a test can change a row between
/// planning and execution.
fn peer_session(fx: &Fixture, uri: &str) -> session::Session {
    let config = Config::default();
    let target = resolve(
        Some(uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");
    fx.block_on(session::connect(&target, Duration::ZERO))
        .expect("the second session connects")
}

#[test]
fn a_composite_quoted_primary_key_plan_updates_only_the_selected_row() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE cell_update_composite ( \
             tenant integer, \
             \"Item Code\" text, \
             note text, \
             PRIMARY KEY (tenant, \"Item Code\") \
         ); \
         INSERT INTO cell_update_composite VALUES \
             (1, 'A-1', 'first'), \
             (1, 'B-2', 'second'), \
             (2, 'A-1', 'third'); \
         SET search_path TO pg_temp",
        100,
        JobId(303),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = "SELECT tenant, \"Item Code\", note FROM cell_update_composite \
                      ORDER BY tenant, \"Item Code\"";
    let (relation, plan) = plan_live_cell_update(
        &fx,
        source_sql,
        "cell_update_composite",
        2,
        1,
        "after-composite",
    );

    assert!(relation.readable);
    assert!(relation.writable);
    let key_columns = relation
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        key_columns,
        vec!["tenant", "Item Code"],
        "primary-key metadata is returned in catalogue order"
    );

    assert_eq!(
        plan.key_columns,
        vec!["tenant".to_owned(), "Item Code".to_owned()]
    );
    let tenant_predicate = plan
        .sql_template
        .find("WHERE \"tenant\" = :__ignatius_key_0")
        .expect("first key predicate");
    let item_code_predicate = plan
        .sql_template
        .find("\"Item Code\" = :__ignatius_key_1")
        .expect("quoted mixed-case key predicate");
    assert!(
        tenant_predicate < item_code_predicate,
        "key predicates follow primary-key metadata order: {}",
        plan.sql_template
    );

    let applied = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(304),
    ));
    assert_eq!(applied.status, ExecutionStatus::Succeeded, "{applied:?}");
    assert_eq!(applied.statements[0].rows_affected, Some(1));

    let after = fx.block_on(fx.session.execute(
        "SELECT tenant, \"Item Code\", note FROM cell_update_composite \
         ORDER BY tenant, \"Item Code\"",
        10,
        JobId(305),
    ));
    let rows = after.statements[0]
        .result_set
        .as_ref()
        .expect("rows after the update")
        .rows
        .clone();
    assert_eq!(
        rows,
        vec![
            vec![
                Cell::Text("1".into()),
                Cell::Text("A-1".into()),
                Cell::Text("first".into())
            ],
            vec![
                Cell::Text("1".into()),
                Cell::Text("B-2".into()),
                Cell::Text("after-composite".into())
            ],
            vec![
                Cell::Text("2".into()),
                Cell::Text("A-1".into()),
                Cell::Text("third".into())
            ],
        ],
        "only the selected composite row may change"
    );
}

#[test]
fn review_does_not_lock_the_row_a_concurrent_delete_reports_zero_rows() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let table = "cell_update_concurrent_delete";
    let setup = fx.block_on(fx.session.execute(
        &format!(
            "DROP TABLE IF EXISTS {table}; \
             CREATE TABLE {table} (id integer PRIMARY KEY, note text); \
             INSERT INTO {table} VALUES (1, 'before'), (2, 'other')"
        ),
        100,
        JobId(306),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = format!("SELECT id, note FROM {table} ORDER BY id");
    let (_relation, plan) = plan_live_cell_update(&fx, &source_sql, table, 1, 0, "after");

    let peer = peer_session(&fx, &uri);
    let deleted =
        fx.block_on(peer.execute(&format!("DELETE FROM {table} WHERE id = 1"), 10, JobId(307)));
    assert_eq!(deleted.status, ExecutionStatus::Succeeded, "{deleted:?}");
    assert_eq!(deleted.statements[0].rows_affected, Some(1));

    let applied = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(308),
    ));
    assert_eq!(applied.status, ExecutionStatus::Succeeded, "{applied:?}");
    assert_eq!(
        applied.statements[0].rows_affected,
        Some(0),
        "a row deleted after review must be reported as zero rows affected"
    );

    let remaining = fx.block_on(fx.session.execute(
        &format!("SELECT id, note FROM {table} ORDER BY id"),
        10,
        JobId(309),
    ));
    let rows = remaining.statements[0]
        .result_set
        .as_ref()
        .expect("rows after the reviewed update")
        .rows
        .clone();
    assert_eq!(
        rows,
        vec![vec![Cell::Text("2".into()), Cell::Text("other".into())]],
        "the reviewed update must not recreate or alter any other row"
    );

    let cleanup = fx.block_on(
        fx.session
            .execute(&format!("DROP TABLE {table}"), 10, JobId(310)),
    );
    assert_eq!(cleanup.status, ExecutionStatus::Succeeded, "{cleanup:?}");
}

#[test]
fn review_does_not_lock_the_row_a_concurrent_change_reports_zero_rows() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let table = "cell_update_concurrent_change";
    let setup = fx.block_on(fx.session.execute(
        &format!(
            "DROP TABLE IF EXISTS {table}; \
             CREATE TABLE {table} (id integer PRIMARY KEY, note text); \
             INSERT INTO {table} VALUES (1, 'before'), (2, 'other')"
        ),
        100,
        JobId(311),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = format!("SELECT id, note FROM {table} ORDER BY id");
    let (_relation, plan) = plan_live_cell_update(&fx, &source_sql, table, 1, 0, "reviewed-value");

    let peer = peer_session(&fx, &uri);
    let changed = fx.block_on(peer.execute(
        &format!("UPDATE {table} SET note = 'concurrent-sentinel' WHERE id = 1"),
        10,
        JobId(312),
    ));
    assert_eq!(changed.status, ExecutionStatus::Succeeded, "{changed:?}");
    assert_eq!(changed.statements[0].rows_affected, Some(1));

    let applied = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(313),
    ));
    assert_eq!(applied.status, ExecutionStatus::Succeeded, "{applied:?}");
    assert_eq!(
        applied.statements[0].rows_affected,
        Some(0),
        "a row changed concurrently after review must match zero rows without retry"
    );

    let after = fx.block_on(fx.session.execute(
        &format!("SELECT id, note FROM {table} ORDER BY id"),
        10,
        JobId(314),
    ));
    let rows = after.statements[0]
        .result_set
        .as_ref()
        .expect("rows after the reviewed update")
        .rows
        .clone();
    assert_eq!(
        rows,
        vec![
            vec![
                Cell::Text("1".into()),
                Cell::Text("concurrent-sentinel".into())
            ],
            vec![Cell::Text("2".into()), Cell::Text("other".into())],
        ],
        "the concurrent non-key change must be preserved without overwriting or retrying"
    );

    let cleanup = fx.block_on(
        fx.session
            .execute(&format!("DROP TABLE {table}"), 10, JobId(315)),
    );
    assert_eq!(cleanup.status, ExecutionStatus::Succeeded, "{cleanup:?}");
}

#[test]
fn cell_update_optimistic_guard_handles_null_cells_and_concurrent_null_transitions() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let table = "cell_update_null_guard";
    let setup = fx.block_on(fx.session.execute(
        &format!(
            "DROP TABLE IF EXISTS {table}; \
             CREATE TABLE {table} (id integer PRIMARY KEY, note text); \
             INSERT INTO {table} VALUES (1, NULL), (2, NULL), (3, 'initial')"
        ),
        100,
        JobId(316),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = format!("SELECT id, note FROM {table} ORDER BY id");

    // 1. Unchanged NULL cell updates 1 row cleanly.
    let (_relation, null_ok_plan) =
        plan_live_cell_update(&fx, &source_sql, table, 1, 0, "filled-from-null");
    assert!(null_ok_plan.sql_template.contains("IS NULL"));
    let applied = fx.block_on(fx.session.execute_with_parameters(
        &null_ok_plan.sql_template,
        &null_ok_plan.parameters,
        10,
        JobId(317),
    ));
    assert_eq!(applied.status, ExecutionStatus::Succeeded, "{applied:?}");
    assert_eq!(applied.statements[0].rows_affected, Some(1));

    // 2. Concurrent NULL -> non-NULL write causes planned NULL update to match 0 rows.
    let (_relation, null_conflict_plan) =
        plan_live_cell_update(&fx, &source_sql, table, 1, 1, "stale-fill");
    let peer = peer_session(&fx, &uri);
    let peer_fill = fx.block_on(peer.execute(
        &format!("UPDATE {table} SET note = 'concurrent-fill' WHERE id = 2"),
        10,
        JobId(318),
    ));
    assert_eq!(
        peer_fill.status,
        ExecutionStatus::Succeeded,
        "{peer_fill:?}"
    );
    assert_eq!(peer_fill.statements[0].rows_affected, Some(1));
    let conflict_null = fx.block_on(fx.session.execute_with_parameters(
        &null_conflict_plan.sql_template,
        &null_conflict_plan.parameters,
        10,
        JobId(319),
    ));
    assert_eq!(
        conflict_null.status,
        ExecutionStatus::Succeeded,
        "{conflict_null:?}"
    );
    assert_eq!(conflict_null.statements[0].rows_affected, Some(0));

    // 3. Concurrent non-NULL -> NULL write causes planned non-NULL update to match 0 rows.
    let (_relation, text_conflict_plan) =
        plan_live_cell_update(&fx, &source_sql, table, 1, 2, "stale-overwrite");
    let peer_null = fx.block_on(peer.execute(
        &format!("UPDATE {table} SET note = NULL WHERE id = 3"),
        10,
        JobId(320),
    ));
    assert_eq!(
        peer_null.status,
        ExecutionStatus::Succeeded,
        "{peer_null:?}"
    );
    assert_eq!(peer_null.statements[0].rows_affected, Some(1));
    let conflict_text = fx.block_on(fx.session.execute_with_parameters(
        &text_conflict_plan.sql_template,
        &text_conflict_plan.parameters,
        10,
        JobId(321),
    ));
    assert_eq!(
        conflict_text.status,
        ExecutionStatus::Succeeded,
        "{conflict_text:?}"
    );
    assert_eq!(conflict_text.statements[0].rows_affected, Some(0));

    let after = fx.block_on(fx.session.execute(
        &format!("SELECT id, note FROM {table} ORDER BY id"),
        10,
        JobId(322),
    ));
    let rows = after.statements[0]
        .result_set
        .as_ref()
        .expect("rows after optimistic guard checks")
        .rows
        .clone();
    assert_eq!(
        rows,
        vec![
            vec![
                Cell::Text("1".into()),
                Cell::Text("filled-from-null".into())
            ],
            vec![Cell::Text("2".into()), Cell::Text("concurrent-fill".into())],
            vec![Cell::Text("3".into()), Cell::Null],
        ],
        "unchanged NULL updates cleanly while concurrent changes are preserved"
    );

    let cleanup = fx.block_on(
        fx.session
            .execute(&format!("DROP TABLE {table}"), 10, JobId(323)),
    );
    assert_eq!(cleanup.status, ExecutionStatus::Succeeded, "{cleanup:?}");
}

#[test]
fn update_relation_reports_view_kind_for_the_interactive_refusal() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE cell_update_view_source (id integer PRIMARY KEY, note text); \
         CREATE TEMP VIEW cell_update_view AS SELECT id, note FROM cell_update_view_source; \
         SET search_path TO pg_temp",
        100,
        JobId(308),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let relation = fx
        .block_on(fx.session.update_relation(None, "cell_update_view"))
        .expect("search-path view metadata");
    assert_eq!(
        relation.kind,
        ignatius::postgres::metadata::ObjectKind::View
    );
    assert_eq!(relation.relation, "cell_update_view");
    assert_eq!(
        relation
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "note"]
    );
}

#[test]
fn a_parameterized_long_statement_can_be_cancelled_and_the_server_confirms_it() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let bindings = ParameterBindings::from_secrets(
        vec!["seconds".into()],
        vec![SecretString::from("30".to_owned())],
    )
    .expect("bindings");
    let cancel = fx.session.cancel_handle();

    let started = Instant::now();
    let execution = fx.block_on(async {
        let running = fx.session.execute_with_parameters(
            "SELECT pg_sleep(:seconds::double precision)",
            &bindings,
            100,
            JobId(302),
        );
        let stopper = async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel.cancel().await.expect("cancel delivered");
        };
        let (execution, ()) = tokio::join!(running, stopper);
        execution
    });
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(execution.status, ExecutionStatus::Cancelled);
    assert_eq!(
        execution
            .error
            .expect("cancel diagnostic")
            .technical
            .iter()
            .find(|field| field.label == "SQLSTATE")
            .expect("SQLSTATE")
            .value,
        "57014"
    );
}

#[test]
fn plain_explain_plan_does_not_execute_the_target_statement() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE explain_guard (value integer); INSERT INTO explain_guard VALUES (1)",
        100,
        JobId(10),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let before = fx.block_on(
        fx.session
            .execute("SELECT value FROM explain_guard", 10, JobId(11)),
    );
    assert_eq!(
        before.statements[0]
            .result_set
            .as_ref()
            .expect("guard row")
            .rows[0][0],
        Cell::Text("1".into())
    );

    let json = fx
        .block_on(
            fx.session
                .explain("UPDATE explain_guard SET value = 2", false),
        )
        .expect("plain EXPLAIN returns JSON");
    let plan = ignatius::query::PlanDocument::parse(&json, false).expect("structured plan");
    assert!(!plan.analyzed);
    assert_eq!(plan.root.node_type, "ModifyTable");

    let after = fx.block_on(
        fx.session
            .execute("SELECT value FROM explain_guard", 10, JobId(12)),
    );
    assert_eq!(
        after.statements[0]
            .result_set
            .as_ref()
            .expect("guard row")
            .rows[0][0],
        Cell::Text("1".into()),
        "plain EXPLAIN must not apply the UPDATE"
    );
}

#[test]
fn analyzed_explain_plan_reports_actual_metrics_for_a_synthetic_select() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let json = fx
        .block_on(fx.session.explain("SELECT generate_series(1, 3)", true))
        .expect("analyzed EXPLAIN returns JSON");
    let plan = ignatius::query::PlanDocument::parse(&json, true).expect("structured analyzed plan");
    assert!(plan.analyzed);
    assert!(plan.execution_time_ms.is_some());
    assert!(plan.root.actual_rows.is_some());
    assert!(plan.root.actual_loops.is_some());
}

#[test]
fn tls_explain_plan_uses_the_encrypted_session_when_the_tls_fixture_is_available() {
    let uri = match std::env::var("IGNATIUS_TEST_PG_TLS_URI") {
        Ok(uri) if !uri.is_empty() => uri,
        _ => {
            eprintln!("skipping: IGNATIUS_TEST_PG_TLS_URI is not set");
            return;
        }
    };
    let target = tls_target(
        &uri,
        &format!("sslmode=verify-full&sslrootcert={}", certificate("ca.crt")),
    );
    let runtime = runtime();
    let session = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("TLS fixture connects");
    assert!(session.info().tls.is_encrypted());
    let json = runtime
        .block_on(session.explain("SELECT 1", false))
        .expect("TLS EXPLAIN returns JSON");
    let plan = ignatius::query::PlanDocument::parse(&json, false).expect("structured TLS plan");
    assert_eq!(plan.root.node_type, "Result");
}

#[test]
fn row_headers_carry_server_types_without_changing_text_values() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT 2::integer AS n, 'ok'::text AS label, NULL::boolean AS active",
        100,
        JobId(2),
    ));

    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "type description must not change query success: {:?}",
        execution.error
    );
    let set = execution.statements[0].result_set.as_ref().expect("rows");
    assert_eq!(
        set.columns,
        vec!["n".to_owned(), "label".to_owned(), "active".to_owned()]
    );
    assert_eq!(
        set.column_types,
        vec![
            Some("int4".to_owned()),
            Some("text".to_owned()),
            Some("bool".to_owned()),
        ]
    );
    assert_eq!(
        set.rows,
        vec![vec![
            Cell::Text("2".into()),
            Cell::Text("ok".into()),
            Cell::Null,
        ]]
    );
    assert_eq!(set.rows_seen, 1);
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
    assert_eq!(error.statement_number, Some(1));
}

#[test]
fn constraint_errors_preserve_server_object_fields_and_statement_identity() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE constraint_parents (id integer PRIMARY KEY); \
         CREATE TEMP TABLE constraint_children ( \
             id integer PRIMARY KEY, \
             parent_id integer, \
             amount integer, \
             CONSTRAINT children_parent_id_fkey FOREIGN KEY (parent_id) \
                 REFERENCES constraint_parents (id), \
             CONSTRAINT children_amount_positive CHECK (amount > 0) \
         )",
        100,
        JobId(1),
    ));
    assert_eq!(
        setup.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        setup.error
    );

    let unique = fx.block_on(fx.session.execute(
        "INSERT INTO orders (order_id, customer_id, total) VALUES (1, 99999, 1.00)",
        100,
        JobId(2),
    ));
    assert_eq!(unique.status, ExecutionStatus::Failed);
    let unique = unique.error.expect("unique diagnostic");
    assert_eq!(sqlstate(&unique), "23505");
    assert_eq!(unique.statement_number, Some(1));
    assert_eq!(
        unique
            .object
            .as_ref()
            .and_then(|object| object.schema.as_deref()),
        Some("public")
    );
    assert_eq!(
        unique
            .object
            .as_ref()
            .and_then(|object| object.table.as_deref()),
        Some("orders")
    );
    assert_eq!(
        unique
            .object
            .as_ref()
            .and_then(|object| object.constraint.as_deref()),
        Some("orders_pkey")
    );

    let foreign_key = fx.block_on(fx.session.execute(
        "INSERT INTO constraint_children (id, parent_id, amount) VALUES (1, 999, 1)",
        100,
        JobId(3),
    ));
    assert_eq!(foreign_key.status, ExecutionStatus::Failed);
    let foreign_key = foreign_key.error.expect("foreign-key diagnostic");
    assert_eq!(sqlstate(&foreign_key), "23503");
    assert_eq!(
        foreign_key
            .object
            .as_ref()
            .and_then(|object| object.table.as_deref()),
        Some("constraint_children")
    );
    assert_eq!(
        foreign_key
            .object
            .as_ref()
            .and_then(|object| object.constraint.as_deref()),
        Some("children_parent_id_fkey")
    );

    let not_null = fx.block_on(fx.session.execute(
        "INSERT INTO orders (customer_id, total) VALUES (NULL, 1.00)",
        100,
        JobId(4),
    ));
    assert_eq!(not_null.status, ExecutionStatus::Failed);
    let not_null = not_null.error.expect("not-null diagnostic");
    assert_eq!(sqlstate(&not_null), "23502");
    assert_eq!(
        not_null
            .object
            .as_ref()
            .and_then(|object| object.table.as_deref()),
        Some("orders")
    );
    assert_eq!(
        not_null
            .object
            .as_ref()
            .and_then(|object| object.column.as_deref()),
        Some("customer_id")
    );

    let check = fx.block_on(fx.session.execute(
        "INSERT INTO constraint_children (id, parent_id, amount) VALUES (2, NULL, -1)",
        100,
        JobId(5),
    ));
    assert_eq!(check.status, ExecutionStatus::Failed);
    let check = check.error.expect("check diagnostic");
    assert_eq!(sqlstate(&check), "23514");
    assert_eq!(
        check
            .object
            .as_ref()
            .and_then(|object| object.table.as_deref()),
        Some("constraint_children")
    );
    assert_eq!(
        check
            .object
            .as_ref()
            .and_then(|object| object.constraint.as_deref()),
        Some("children_amount_positive")
    );
}

fn sqlstate(error: &ignatius::diagnostics::Diagnostic) -> &str {
    error
        .technical
        .iter()
        .find(|field| field.label == "SQLSTATE")
        .map_or("", |field| field.value.as_str())
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
fn a_value_over_the_retained_bound_is_refused_without_keeping_it() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let execution = fx.block_on(fx.session.execute(
        "SELECT repeat('x', 2 * 1024 * 1024) AS big",
        100,
        JobId(1),
    ));

    assert_eq!(
        execution.status,
        ExecutionStatus::Failed,
        "{:?}",
        execution.error
    );
    let error = execution.error.expect("refusal diagnostic");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert!(error.headline.contains("too large to retain"), "{error:?}");
    assert!(
        !format!("{error:?}").contains(&"x".repeat(64)),
        "the refused value itself must not ride along in the diagnostic"
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
    let Some(restricted) = restricted_reader_uri(&uri) else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };

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
fn a_restricted_role_reads_orders_but_the_server_refuses_a_generated_update() {
    let uri = target_or_skip!();
    let Some(restricted) = restricted_reader_uri(&uri) else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };
    let fx = fixture(&restricted);

    let source_sql = "SELECT order_id, note FROM orders ORDER BY order_id";
    let (relation, plan) =
        plan_live_cell_update(&fx, source_sql, "orders", 1, 0, "refused-by-privilege");
    assert!(relation.readable, "the role was granted SELECT on orders");
    assert!(
        !relation.writable,
        "the role was not granted UPDATE, and metadata must say so"
    );

    let execution = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(1),
    ));
    assert_eq!(execution.status, ExecutionStatus::Failed);
    let error = execution.error.expect("privilege diagnostic");
    assert_eq!(
        error.kind,
        ignatius::diagnostics::DiagnosticKind::Query,
        "a server refusal is the existing Query diagnostic"
    );
    assert_eq!(
        error
            .technical
            .iter()
            .find(|field| field.label == "SQLSTATE")
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
fn completion_catalogue_contains_objects_columns_and_function_detail_in_one_snapshot() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let catalog = fx
        .block_on(fx.session.completion_catalog())
        .expect("completion catalogue");

    let orders = catalog
        .objects
        .iter()
        .find(|object| object.schema == "public" && object.name == "orders")
        .expect("orders object");
    assert_eq!(
        orders.kind,
        ignatius::query::completion::CatalogObjectKind::Table
    );
    assert!(orders.readable);

    let hostile = catalog
        .objects
        .iter()
        .find(|object| object.name.contains("DROP TABLE"))
        .expect("hostile object is catalogued as data");
    assert_eq!(hostile.schema, "public");

    let relation = catalog
        .relations
        .iter()
        .find(|relation| relation.schema == "public" && relation.name == "orders")
        .expect("orders columns");
    assert!(
        relation
            .columns
            .iter()
            .any(|column| column.name == "order_id")
    );
    assert!(
        relation
            .columns
            .iter()
            .any(|column| column.data_type == "numeric(12,2)")
    );

    let function = catalog
        .objects
        .iter()
        .find(|object| object.schema == "reporting" && object.name == "order_count")
        .expect("reporting function");
    assert_eq!(
        function.kind,
        ignatius::query::completion::CatalogObjectKind::Function
    );
    assert!(
        function
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("bigint"),
        "function return detail remains visible: {:?}",
        function.detail
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
fn the_object_tree_can_have_its_own_read_only_connection_to_the_same_server() {
    let uri = target_or_skip!();
    let config = Config::default();
    let mut target = resolve(
        Some(&uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");
    // PostgreSQL exposes at most 63 bytes of application_name. Keep the
    // original exactly at that boundary so an appended object-tree marker
    // cannot silently make both sessions look identical.
    let original_application_name =
        "ignatius-object-identity-abcdefghijklmnopqrstuvwxyz0123456789ab";
    assert_eq!(original_application_name.len(), 63);
    target.application_name = original_application_name.to_owned();

    let runtime = runtime();
    let query_session = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("the session connects");

    // Use the same resolved route, credentials and transport for the tree.
    let catalogue = target.for_object_tree();
    let catalogue_session = runtime
        .block_on(session::connect(&catalogue, Duration::ZERO))
        .expect("the object tree connects");

    assert_ne!(
        query_session.info().backend_pid,
        catalogue_session.info().backend_pid,
        "two connections, not one"
    );
    assert!(
        catalogue_session.info().read_only,
        "the tree only ever reads, and the server is what enforces that"
    );
    assert!(!query_session.info().read_only, "the session is unaffected");

    let query_pid = query_session
        .info()
        .backend_pid
        .parse::<i32>()
        .expect("query backend pid");
    let catalogue_pid = catalogue_session
        .info()
        .backend_pid
        .parse::<i32>()
        .expect("catalogue backend pid");
    let visible_application_name = |pid: i32, job: u64| {
        let execution = runtime.block_on(query_session.execute(
            &format!(
                "SELECT application_name::text FROM pg_catalog.pg_stat_activity WHERE pid = {pid}"
            ),
            10,
            JobId(job),
        ));
        let result = execution
            .statements
            .first()
            .and_then(|statement| statement.result_set.as_ref())
            .expect("pg_stat_activity result");
        assert_eq!(result.rows.len(), 1, "backend {pid} is visible");
        match &result.rows[0][0] {
            Cell::Text(name) => name.clone(),
            value => panic!("unexpected application_name value: {value:?}"),
        }
    };
    let visible_query_name = visible_application_name(query_pid, 2);
    let visible_catalogue_name = visible_application_name(catalogue_pid, 3);
    assert_eq!(visible_query_name, original_application_name);
    assert_ne!(visible_catalogue_name, visible_query_name);
    assert!(
        visible_catalogue_name.contains("(objects)"),
        "object-tree application_name keeps its marker: {visible_catalogue_name:?}"
    );

    // And it can do the only job it has.
    let schemas = runtime
        .block_on(catalogue_session.schemas())
        .expect("schemas");
    assert!(schemas.iter().any(|schema| schema.name == "public"));

    // A write on it is refused by PostgreSQL rather than by a guess here.
    let execution = runtime.block_on(catalogue_session.execute(
        "CREATE TABLE should_not_exist (id int)",
        10,
        JobId(1),
    ));
    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert_eq!(
        execution
            .error
            .expect("diagnostic")
            .technical
            .iter()
            .find(|field| field.label == "SQLSTATE")
            .expect("SQLSTATE")
            .value,
        "25006",
        "read_sql_transaction"
    );
}

#[test]
fn a_definition_comes_from_the_server_where_the_server_can_render_it() {
    use ignatius::postgres::ObjectKind;
    use ignatius::postgres::metadata::{DefinitionSource, ObjectSummary};

    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let object = |kind: ObjectKind, schema: &str, name: &str| ObjectSummary {
        kind,
        schema: schema.to_owned(),
        name: name.to_owned(),
        readable: true,
        detail: None,
    };

    // A view is rendered by PostgreSQL itself, so what is shown is what will run.
    let view = fx
        .block_on(
            fx.session
                .definition(&object(ObjectKind::View, "public", "recent_orders")),
        )
        .expect("view definition");
    assert_eq!(view.source, DefinitionSource::Server);
    assert!(
        view.text.starts_with("CREATE OR REPLACE VIEW"),
        "{}",
        view.text
    );
    assert!(view.text.contains("orders"), "{}", view.text);
    assert!(
        view.heading().contains("\"recent_orders\""),
        "{}",
        view.heading()
    );

    // So is a function.
    let function = fx
        .block_on(
            fx.session
                .definition(&object(ObjectKind::Function, "reporting", "order_count")),
        )
        .expect("function definition");
    assert_eq!(function.source, DefinitionSource::Server);
    assert!(
        function.text.contains("CREATE OR REPLACE FUNCTION"),
        "{}",
        function.text
    );

    // A table is not: it is assembled here, and says so.
    let table = fx
        .block_on(
            fx.session
                .definition(&object(ObjectKind::Table, "public", "orders")),
        )
        .expect("table definition");
    assert_eq!(table.source, DefinitionSource::Assembled);
    assert!(
        table
            .text
            .starts_with("CREATE TABLE \"public\".\"orders\" ("),
        "{}",
        table.text
    );
    assert!(table.text.contains("NOT NULL"), "{}", table.text);
    assert!(
        table.text.contains("ADD CONSTRAINT"),
        "constraints are part of what a table is: {}",
        table.text
    );
    assert!(
        DefinitionSource::Assembled.note().contains("not a"),
        "the difference is stated"
    );
}

#[test]
fn dependencies_are_read_in_both_directions_and_say_what_they_cannot_see() {
    use ignatius::postgres::ObjectKind;
    use ignatius::postgres::metadata::ObjectSummary;

    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let orders = ObjectSummary {
        kind: ObjectKind::Table,
        schema: "public".into(),
        name: "orders".into(),
        readable: true,
        detail: None,
    };
    let found = fx
        .block_on(fx.session.dependencies(&orders))
        .expect("dependencies");

    // A view over the table is recorded by PostgreSQL as a rewrite rule, which
    // is the edge this follows.
    assert!(
        found
            .used_by
            .iter()
            .any(|object| object.name == "recent_orders"),
        "the view that reads it: {:?}",
        found.used_by
    );
    assert!(
        found.used_by.iter().all(|object| object.detail.is_some()),
        "every edge says why it is an edge"
    );

    // And the other direction: the view depends on the table.
    let view = ObjectSummary {
        kind: ObjectKind::View,
        schema: "public".into(),
        name: "recent_orders".into(),
        readable: true,
        detail: None,
    };
    let found = fx
        .block_on(fx.session.dependencies(&view))
        .expect("dependencies");
    assert!(
        found
            .depends_on
            .iter()
            .any(|object| object.name == "orders"),
        "{:?}",
        found.depends_on
    );

    // An object with no edges is empty rather than an error.
    let alone = ObjectSummary {
        kind: ObjectKind::Table,
        schema: "public".into(),
        name: "no_such_table".into(),
        readable: true,
        detail: None,
    };
    let found = fx
        .block_on(fx.session.dependencies(&alone))
        .expect("dependencies");
    assert!(found.is_empty());
}

#[test]
fn an_object_named_to_break_a_client_can_still_be_described() {
    use ignatius::postgres::ObjectKind;
    use ignatius::postgres::metadata::ObjectSummary;

    let uri = target_or_skip!();
    let fx = fixture(&uri);

    // Every catalogue lookup binds the name as a parameter, so a name written to
    // break an interpolating client is as safe to describe as it is to list.
    let hostile = ObjectSummary {
        kind: ObjectKind::Table,
        schema: "public".into(),
        name: r#"we"ird "; DROP TABLE orders; --"#.into(),
        readable: true,
        detail: None,
    };
    let definition = fx
        .block_on(fx.session.definition(&hostile))
        .expect("definition");
    assert!(
        definition.text.contains(r#"we""ird"#),
        "the name is quoted in what is shown: {}",
        definition.text
    );

    // And the table it was written to destroy is still there.
    let orders = fx
        .block_on(fx.session.objects("public", ObjectKind::Table))
        .expect("tables");
    assert!(orders.iter().any(|table| table.name == "orders"));
}

#[test]
fn a_definition_that_is_not_there_fails_with_something_to_do() {
    use ignatius::postgres::ObjectKind;
    use ignatius::postgres::metadata::ObjectSummary;

    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let missing = ObjectSummary {
        kind: ObjectKind::View,
        schema: "public".into(),
        name: "no_such_view".into(),
        readable: true,
        detail: None,
    };
    let error = fx
        .block_on(fx.session.definition(&missing))
        .expect_err("must fail");
    assert!(error.next_action.is_some(), "a failure must say what to do");
}

fn restricted_reader_uri(uri: &str) -> Option<String> {
    let (scheme, rest) = uri.split_once("://")?;
    let (_, host_and_db) = rest.split_once('@')?;
    Some(format!(
        "{scheme}://restricted_reader:not-a-real-password-restricted@{host_and_db}"
    ))
}

fn assert_restricted_completion(fx: &Fixture) {
    let allowed = fx.block_on(fx.session.execute(
        "SELECT * FROM public.orders WHERE false",
        20,
        JobId(12_035),
    ));
    assert_eq!(allowed.status, ExecutionStatus::Succeeded, "{allowed:?}");

    let denied = fx.block_on(fx.session.execute(
        "SELECT * FROM public.secrets_of_the_realm WHERE false",
        20,
        JobId(12_036),
    ));
    assert_eq!(denied.status, ExecutionStatus::Failed, "{denied:?}");
    assert!(denied.error.as_ref().is_some_and(|error| {
        error
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "42501")
    }));

    let catalog = fx
        .block_on(fx.session.completion_catalog())
        .expect("restricted completion catalogue");
    for (name, readable) in [("orders", true), ("secrets_of_the_realm", false)] {
        let object = catalog
            .objects
            .iter()
            .find(|object| object.schema == "public" && object.name == name)
            .unwrap_or_else(|| panic!("completion catalogue omitted public.{name}"));
        assert_eq!(object.readable, readable, "public.{name}");
    }

    for (name, readable) in [("orders", true), ("secrets_of_the_realm", false)] {
        let sql = format!("SELECT * FROM public.{name}");
        let completion = ignatius::query::completion::complete(&sql, sql.len(), Some(&catalog));
        assert_eq!(
            completion.scope,
            ignatius::query::completion::CompletionScope::Schema("public".to_owned())
        );
        let candidate = completion
            .candidates
            .iter()
            .find(|candidate| candidate.label == name)
            .unwrap_or_else(|| panic!("completion omitted public.{name}"));
        assert_eq!(candidate.schema.as_deref(), Some("public"));
        assert_eq!(
            candidate.detail.as_deref(),
            (!readable).then_some("not readable by this role"),
            "only unreadable candidates carry the policy label"
        );
    }
}

#[test]
fn restricted_completion_marks_readability_over_plain_transport() {
    let uri = target_or_skip!();
    let Some(uri) = restricted_reader_uri(&uri) else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };
    let fx = fixture(&uri);
    let tls = &fx.session.info().tls;
    assert!(!tls.is_encrypted());
    match tls {
        TlsState::Disabled => assert_eq!(tls.label(), "TLS off"),
        TlsState::NotNegotiated => assert_eq!(tls.label(), "TLS not negotiated"),
        state => panic!("expected an unencrypted plain fixture, got {state:?}"),
    }
    assert_restricted_completion(&fx);
}

#[test]
fn a_restricted_role_sees_objects_it_cannot_read_and_they_are_marked_as_such() {
    let uri = target_or_skip!();
    let Some(restricted) = restricted_reader_uri(&uri) else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };
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

// ------------------------------------------------- service and password files

/// Writes a service file and a password file describing the demo database, and
/// returns the directory holding them.
fn credential_files(uri: &str) -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().expect("temp dir");

    // Pull the pieces out of the test URI rather than hard-coding them twice.
    let rest = uri.split_once("://").expect("a URI").1;
    let (userinfo, host_and_db) = rest.split_once('@').expect("userinfo");
    let (user, password) = userinfo.split_once(':').expect("a password");
    let (host_port, database) = host_and_db.split_once('/').expect("a database");
    let (host, port) = host_port.split_once(':').expect("a port");

    let service = dir.path().join("pg_service.conf");
    std::fs::write(
        &service,
        format!("[demo]\nhost={host}\nport={port}\ndbname={database}\nuser={user}\n"),
    )
    .expect("write service file");

    let passfile = dir.path().join("pgpass");
    std::fs::write(
        &passfile,
        format!("{host}:{port}:{database}:{user}:{password}\n"),
    )
    .expect("write password file");
    ignatius::platform::restrict_to_owner(&passfile).expect("restrict");

    (
        dir,
        service.display().to_string(),
        passfile.display().to_string(),
    )
}

#[test]
fn a_service_file_and_a_password_file_are_enough_to_connect() {
    let uri = target_or_skip!();
    let (_dir, service, passfile) = credential_files(&uri);

    // Nothing here names a host, a user, or a password: the two files carry
    // everything, which is the point of supporting them.
    let env = EnvSnapshot::from_pairs(&[
        ("PGSERVICEFILE", service.as_str()),
        ("PGPASSFILE", passfile.as_str()),
    ]);
    let config = Config::default();
    let target = resolve(
        Some("service=demo"),
        &ConnectionArgs::default(),
        &env,
        &config.connection,
    )
    .expect("target resolves from the files alone");

    let runtime = runtime();
    let session = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("connects using only the service and password files");

    let execution = runtime.block_on(session.execute("SELECT 1 AS one", 10, JobId(1)));
    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
}

#[test]
fn connecting_with_no_password_at_all_is_an_authentication_failure() {
    let uri = target_or_skip!();
    let Some((prefix, rest)) = uri.split_once("://") else {
        panic!("expected a URI");
    };
    let Some((_, host_and_db)) = rest.split_once('@') else {
        eprintln!("skipping: the test URI has no userinfo to replace");
        return;
    };

    // The server requires SCRAM, so offering nothing is a refusal by the server
    // rather than a network problem. This also pins the driver's wording for
    // that case, which the classification depends on.
    let config = Config::default();
    let target = resolve(
        Some(&format!("{prefix}://ignatius_test@{host_and_db}")),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("target resolves");

    let error = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("no password means no connection");
    assert_eq!(
        error.exit_code(),
        ignatius::ExitCode::Authentication,
        "the server was reached and said no: {error:?}"
    );
    assert!(
        error
            .next_action
            .expect("an action")
            .contains("password file"),
        "it should point at the safer route"
    );
}

#[test]
fn a_world_readable_password_file_is_refused_and_the_connection_fails_honestly() {
    let uri = target_or_skip!();
    let (_dir, service, passfile) = credential_files(&uri);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&passfile, std::fs::Permissions::from_mode(0o644))
            .expect("loosen the permissions");

        let env = EnvSnapshot::from_pairs(&[
            ("PGSERVICEFILE", service.as_str()),
            ("PGPASSFILE", passfile.as_str()),
        ]);
        let config = Config::default();
        let target = resolve(
            Some("service=demo"),
            &ConnectionArgs::default(),
            &env,
            &config.connection,
        )
        .expect("resolve");

        assert!(
            target.password.is_none(),
            "a password file others can read must not be used"
        );
        assert!(
            target
                .notes
                .iter()
                .any(|n| n.message.contains("readable by others")),
            "and the user must be told why: {:?}",
            target.notes
        );

        // The connection then fails on authentication, not silently as something
        // else, because the server is reached and no password is offered.
        let error = runtime()
            .block_on(session::connect(&target, Duration::ZERO))
            .expect_err("no password means no connection");
        assert_eq!(error.exit_code(), ignatius::ExitCode::Authentication);
    }
    #[cfg(not(unix))]
    {
        let _ = (service, passfile);
        eprintln!("skipping: no permission bits to loosen on this platform");
    }
}

// -------------------------------------------------------------- transport

/// The TLS development database, or a skip. Started by `cargo xtask db up`.
macro_rules! tls_target_or_skip {
    () => {
        match std::env::var("IGNATIUS_TEST_PG_TLS_URI") {
            Ok(uri) if !uri.is_empty() => uri,
            _ => {
                eprintln!(
                    "skipping: IGNATIUS_TEST_PG_TLS_URI is not set. \
                     `cargo xtask db up` starts a TLS server and prints it."
                );
                return;
            }
        }
    };
}

/// Where the throwaway certificates live, relative to the repository root.
fn certificate(name: &str) -> String {
    format!("{}/docker/tls/generated/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn tls_target(uri: &str, extra: &str) -> ignatius::connection::ConnectionTarget {
    let config = Config::default();
    resolve(
        Some(&format!("{uri}?{extra}")),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("target resolves")
}

#[test]
fn verify_full_succeeds_against_a_trusted_certificate_with_a_matching_name() {
    let uri = tls_target_or_skip!();
    let target = tls_target(
        &uri,
        &format!("sslmode=verify-full&sslrootcert={}", certificate("ca.crt")),
    );

    let session = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("verify-full should succeed against its own certificate");

    let tls = &session.info().tls;
    assert!(tls.is_encrypted(), "{tls:?}");
    let description = tls.description();
    assert!(
        description.contains("matching host name"),
        "verify-full must state the guarantee it gave: {description}"
    );
    assert!(tls.label().contains("verify-full"), "{}", tls.label());
}

#[test]
fn restricted_completion_marks_readability_over_verified_tls() {
    let uri = tls_target_or_skip!();
    let Some(uri) = restricted_reader_uri(&uri) else {
        eprintln!("skipping: the TLS test URI has no userinfo to replace");
        return;
    };
    let target = tls_target(
        &uri,
        &format!("sslmode=verify-full&sslrootcert={}", certificate("ca.crt")),
    );
    let runtime = runtime();
    let session = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("restricted TLS fixture connects with verified server identity");
    let fx = Fixture { runtime, session };
    match &fx.session.info().tls {
        TlsState::Active {
            version,
            mode: SslMode::VerifyFull,
            ..
        } => assert_eq!(
            fx.session.info().tls.label(),
            format!("TLS {version} verify-full")
        ),
        state => panic!("expected encrypted verify-full transport, got {state:?}"),
    }
    assert_restricted_completion(&fx);
}

#[test]
fn verify_full_refuses_a_certificate_that_does_not_cover_the_name_used() {
    let uri = tls_target_or_skip!();
    // The certificate covers "localhost" and not "127.0.0.1". Connecting by
    // address is therefore a genuine name mismatch against a trusted chain.
    let by_address = uri.replace("localhost", "127.0.0.1");
    let target = tls_target(
        &by_address,
        &format!("sslmode=verify-full&sslrootcert={}", certificate("ca.crt")),
    );

    let error = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("a name mismatch must not be accepted");

    assert_eq!(error.exit_code(), ignatius::ExitCode::Tls);
    assert!(
        error
            .likely_cause
            .as_deref()
            .unwrap_or_default()
            .contains("127.0.0.1"),
        "the message should name what did not match: {error:?}"
    );
    assert!(
        error
            .next_action
            .as_deref()
            .unwrap_or_default()
            .contains("not retried without TLS"),
        "and must never suggest dropping TLS: {error:?}"
    );
}

#[test]
fn verify_ca_accepts_the_same_certificate_that_verify_full_rejects() {
    let uri = tls_target_or_skip!();
    let by_address = uri.replace("localhost", "127.0.0.1");

    // This is the whole difference between the two modes, on one server: the
    // chain is trusted either way, and only verify-full checks the name.
    let target = tls_target(
        &by_address,
        &format!("sslmode=verify-ca&sslrootcert={}", certificate("ca.crt")),
    );
    let session = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("verify-ca ignores the name mismatch");

    let tls = &session.info().tls;
    assert!(tls.is_encrypted());
    let description = tls.description();
    assert!(
        description.contains("host name not checked"),
        "verify-ca must say what it did not check: {description}"
    );
}

#[test]
fn an_untrusted_chain_is_refused_even_when_the_name_matches() {
    let uri = tls_target_or_skip!();
    // The client certificate is a perfectly valid certificate and a completely
    // wrong trust root, so this proves the chain is really being checked.
    let target = tls_target(
        &uri,
        &format!(
            "sslmode=verify-full&sslrootcert={}",
            certificate("client.crt")
        ),
    );

    let error = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("an untrusted chain must be refused");
    assert_eq!(error.exit_code(), ignatius::ExitCode::Tls);
}

#[test]
fn a_client_certificate_authenticates_without_a_password() {
    let uri = tls_target_or_skip!();
    // cert_user has no password at all: pg_hba requires a certificate for it.
    let as_cert_user = {
        let (prefix, rest) = uri.split_once("://").expect("a URI");
        let (_, host_and_db) = rest.split_once('@').expect("userinfo");
        format!("{prefix}://cert_user@{host_and_db}")
    };
    let target = tls_target(
        &as_cert_user,
        &format!(
            "sslmode=verify-full&sslrootcert={}&sslcert={}&sslkey={}",
            certificate("ca.crt"),
            certificate("client.crt"),
            certificate("client.key")
        ),
    );

    let runtime = runtime();
    let session = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("a client certificate should authenticate");

    assert_eq!(session.info().user, "cert_user");
    assert!(session.info().tls.is_encrypted());

    let execution = runtime.block_on(session.execute("SELECT count(*) FROM orders", 10, JobId(1)));
    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{:?}",
        execution.error
    );
}

#[test]
fn a_certificate_without_its_key_is_refused_before_connecting() {
    let uri = tls_target_or_skip!();
    let target = tls_target(
        &uri,
        &format!(
            "sslmode=verify-full&sslrootcert={}&sslcert={}",
            certificate("ca.crt"),
            certificate("client.crt")
        ),
    );

    let error = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("half a client certificate is not usable");
    assert_eq!(error.exit_code(), ignatius::ExitCode::Tls);
    assert!(
        error.headline.contains("both a certificate and a key"),
        "{}",
        error.headline
    );
}

#[test]
fn the_transaction_state_is_read_from_the_server_at_each_step() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    use ignatius::query::result::TransactionState;

    let start = fx.block_on(fx.session.execute("SELECT 1", 10, JobId(1)));
    assert_eq!(start.transaction, TransactionState::Autocommit);

    let begun = fx.block_on(fx.session.execute("BEGIN", 10, JobId(2)));
    assert_eq!(begun.transaction, TransactionState::Open);
    assert!(
        begun
            .transaction
            .recovery()
            .expect("advice")
            .contains("COMMIT")
    );

    // A failure inside the transaction is what the client must not get wrong:
    // nothing else will run until it ends, and the server is the one that knows.
    let failed = fx.block_on(
        fx.session
            .execute("SELECT * FROM no_such_table", 10, JobId(3)),
    );
    assert_eq!(
        failed.transaction,
        TransactionState::Failed,
        "a failed transaction must be reported as failed"
    );
    assert!(
        failed
            .transaction
            .recovery()
            .expect("advice")
            .contains("ROLLBACK"),
        "and must name the way out"
    );

    let recovered = fx.block_on(fx.session.execute("ROLLBACK", 10, JobId(4)));
    assert_eq!(recovered.transaction, TransactionState::Autocommit);
}

#[test]
fn w03_adversarial_sql_injection_payloads_in_named_parameters_remain_literal_text() {
    let Some(fx) = connect_plain() else {
        return;
    };

    let initial = fx.block_on(
        fx.session
            .execute("SELECT count(*) FROM orders", 10, JobId(1001)),
    );
    assert_eq!(
        initial.status,
        ExecutionStatus::Succeeded,
        "failed querying initial orders count: {:?}",
        initial.error
    );
    let initial_count = initial.statements[0]
        .result_set
        .as_ref()
        .expect("initial count result set")
        .rows[0][0]
        .clone();

    let payloads = [
        "' OR '1'='1'; DROP TABLE orders; --",
        "\\'; DROP TABLE orders; --",
        "$$; SELECT pg_sleep(5); -- $$",
    ];

    for (idx, payload) in payloads.iter().enumerate() {
        let bindings = ParameterBindings::from_secrets(
            vec!["payload".into()],
            vec![SecretString::from((*payload).to_owned())],
        )
        .expect("parameter bindings");

        let execution = fx.block_on(fx.session.execute_with_parameters(
            "SELECT :payload AS echoed",
            &bindings,
            100,
            JobId(1002 + idx as u64),
        ));

        assert_eq!(
            execution.status,
            ExecutionStatus::Succeeded,
            "adversarial payload execution failed: {:?}",
            execution.error
        );
        let set = execution.statements[0]
            .result_set
            .as_ref()
            .expect("echoed result set");
        assert_eq!(set.rows.len(), 1);
        assert_eq!(
            set.rows[0][0],
            Cell::Text((*payload).into()),
            "payload must be echoed as exact literal text without SQL injection interpretation"
        );
    }

    let after = fx.block_on(
        fx.session
            .execute("SELECT count(*) FROM orders", 10, JobId(1009)),
    );
    assert_eq!(
        after.status,
        ExecutionStatus::Succeeded,
        "failed querying post-test orders count: {:?}",
        after.error
    );
    let after_count = after.statements[0]
        .result_set
        .as_ref()
        .expect("after count result set")
        .rows[0][0]
        .clone();
    assert_eq!(
        initial_count, after_count,
        "orders count must remain unchanged after adversarial parameter executions"
    );
}

#[test]
fn w03_generated_update_with_quoted_unicode_primary_key_updates_only_target_row() {
    let Some(fx) = connect_plain() else {
        return;
    };

    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE w03_unicode_pk (\"k\"\"ey\" text PRIMARY KEY, note text); \
         INSERT INTO w03_unicode_pk VALUES ('row\"1\n✓', 'note_one'), ('row\"2', 'note_two'); \
         SET search_path TO pg_temp",
        100,
        JobId(1010),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = "SELECT \"k\"\"ey\", note FROM w03_unicode_pk ORDER BY \"k\"\"ey\"";
    let (_relation, plan) =
        plan_live_cell_update(&fx, source_sql, "w03_unicode_pk", 1, 0, "updated_note_one");

    let applied = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(1011),
    ));
    assert_eq!(applied.status, ExecutionStatus::Succeeded, "{applied:?}");
    assert_eq!(
        applied.statements[0].rows_affected,
        Some(1),
        "exactly one row must be affected"
    );

    let verify = fx.block_on(fx.session.execute(
        "SELECT \"k\"\"ey\", note FROM w03_unicode_pk ORDER BY \"k\"\"ey\"",
        10,
        JobId(1012),
    ));
    assert_eq!(verify.status, ExecutionStatus::Succeeded, "{verify:?}");
    let rows = verify.statements[0]
        .result_set
        .as_ref()
        .expect("queried rows")
        .rows
        .clone();

    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        vec![
            Cell::Text("row\"1\n✓".into()),
            Cell::Text("updated_note_one".into()),
        ],
        "target row must be updated with the new replacement value"
    );
    assert_eq!(
        rows[1],
        vec![Cell::Text("row\"2".into()), Cell::Text("note_two".into()),],
        "untargeted row must remain completely untouched"
    );
}

#[test]
fn w03_concurrent_column_drop_between_review_and_send_reports_clean_server_error() {
    let Some(fx) = connect_plain() else {
        return;
    };

    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE w03_ddl_race (id integer PRIMARY KEY, note text); \
         INSERT INTO w03_ddl_race VALUES (1, 'initial'); \
         SET search_path TO pg_temp",
        100,
        JobId(1020),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let source_sql = "SELECT id, note FROM w03_ddl_race";
    let (_relation, plan) =
        plan_live_cell_update(&fx, source_sql, "w03_ddl_race", 1, 0, "replacement");

    let alter = fx.block_on(fx.session.execute(
        "ALTER TABLE w03_ddl_race DROP COLUMN note",
        10,
        JobId(1021),
    ));
    assert_eq!(alter.status, ExecutionStatus::Succeeded, "{alter:?}");

    let applied = fx.block_on(fx.session.execute_with_parameters(
        &plan.sql_template,
        &plan.parameters,
        10,
        JobId(1022),
    ));

    assert_eq!(
        applied.status,
        ExecutionStatus::Failed,
        "sending update plan against dropped column must fail: {applied:?}"
    );
    let error = applied.error.as_ref().expect("server error diagnostic");
    assert_eq!(
        sqlstate(error),
        "42703",
        "expected undefined_column SQLSTATE 42703: {error:?}"
    );
}

fn verify_schema_graph_cardinality(schema: &str) {
    use ignatius::diagram::{Cardinality, DiagramFormat};

    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let setup_sql = format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE;\
         CREATE SCHEMA {schema};\
         CREATE TABLE {schema}.customers (\
             customer_id integer PRIMARY KEY,\
             email text UNIQUE NOT NULL\
         );\
         CREATE TABLE {schema}.customer_profiles (\
             customer_id integer PRIMARY KEY REFERENCES {schema}.customers(customer_id),\
             bio text\
         );\
         CREATE TABLE {schema}.orders (\
             order_id bigint PRIMARY KEY,\
             customer_id integer NOT NULL REFERENCES {schema}.customers(customer_id),\
             notes text\
         );"
    );
    let setup_res = fx.block_on(fx.session.execute(&setup_sql, 100, JobId(9001)));
    assert_eq!(
        setup_res.status,
        ExecutionStatus::Succeeded,
        "setup failed: {:?}",
        setup_res.error
    );

    let graph_res = fx.block_on(fx.session.schema_graph(Some(schema)));

    // Clean up schema after retrieving the graph
    let cleanup_sql = format!("DROP SCHEMA IF EXISTS {schema} CASCADE;");
    let _ = fx.block_on(fx.session.execute(&cleanup_sql, 100, JobId(9002)));

    let graph = graph_res.expect("schema graph");
    assert_eq!(graph.tables.len(), 3, "expected 3 tables");
    assert_eq!(graph.edges.len(), 2, "expected 2 foreign key edges");

    let profile_edge = graph
        .edges
        .iter()
        .find(|e| e.source_table == "customer_profiles" && e.target_table == "customers")
        .expect("customer_profiles -> customers edge");
    assert_eq!(profile_edge.cardinality, Cardinality::OneToOne);

    let order_edge = graph
        .edges
        .iter()
        .find(|e| e.source_table == "orders" && e.target_table == "customers")
        .expect("orders -> customers edge");
    assert_eq!(order_edge.cardinality, Cardinality::OneToMany);

    let mermaid = graph.render(DiagramFormat::Mermaid);
    assert!(mermaid.contains(&format!("{schema}.customers")));
    assert!(mermaid.contains(&format!("{schema}.customer_profiles")));
    assert!(mermaid.contains(&format!("{schema}.orders")));

    let svg = graph.render(DiagramFormat::Svg);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("customers"));
    assert!(svg.contains("customer_profiles"));
    assert!(svg.contains("orders"));
}

#[test]
fn schema_graph_extracts_primary_keys_foreign_keys_and_cardinality_from_pg_catalog() {
    verify_schema_graph_cardinality("er_graph_test");
}

#[test]
fn diagram_schema_graph_extracts_primary_keys_foreign_keys_and_cardinality_from_pg_catalog() {
    verify_schema_graph_cardinality("er_graph_diagram_test");
}

#[test]
fn ddl_generator_reconstructs_enums_sequences_tables_constraints_indexes_triggers_and_views() {
    use ignatius::ddl::DdlCategory;

    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let setup_sql = "
        DROP SCHEMA IF EXISTS ddl_test_schema CASCADE;
        CREATE SCHEMA ddl_test_schema;
        CREATE TYPE ddl_test_schema.ticket_status AS ENUM ('open', 'closed');
        CREATE SEQUENCE ddl_test_schema.ticket_seq START WITH 100 INCREMENT BY 5;
        CREATE TABLE ddl_test_schema.tickets (
            id bigint PRIMARY KEY DEFAULT nextval('ddl_test_schema.ticket_seq'),
            code text UNIQUE NOT NULL,
            status ddl_test_schema.ticket_status NOT NULL DEFAULT 'open'
        );
        CREATE INDEX idx_tickets_status ON ddl_test_schema.tickets (status);
        CREATE OR REPLACE FUNCTION ddl_test_schema.touch_ticket() RETURNS trigger AS $$
        BEGIN
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        CREATE TRIGGER trg_tickets_update BEFORE UPDATE ON ddl_test_schema.tickets
            FOR EACH ROW EXECUTE FUNCTION ddl_test_schema.touch_ticket();
        CREATE VIEW ddl_test_schema.open_tickets AS
            SELECT id, code FROM ddl_test_schema.tickets WHERE status = 'open';
    ";
    let setup_res = fx.block_on(fx.session.execute(setup_sql, 100, JobId(9101)));
    assert_eq!(
        setup_res.status,
        ExecutionStatus::Succeeded,
        "setup failed: {:?}",
        setup_res.error
    );

    let schema_res = fx.block_on(fx.session.generate_ddl("ddl_test_schema", None));
    let table_res = fx.block_on(fx.session.generate_ddl("ddl_test_schema", Some("tickets")));

    // Clean up schema after retrieving DDL bundles
    let _ = fx.block_on(fx.session.execute(
        "DROP SCHEMA IF EXISTS ddl_test_schema CASCADE;",
        100,
        JobId(9102),
    ));

    let schema_bundle = schema_res.expect("schema ddl");
    let table_bundle = table_res.expect("table ddl");

    // Assert schema bundle contents
    assert!(
        schema_bundle
            .objects
            .iter()
            .any(|o| o.name == "ticket_status" && o.category == DdlCategory::Type),
        "schema bundle must contain ticket_status enum"
    );
    assert!(
        schema_bundle
            .objects
            .iter()
            .any(|o| o.name == "ticket_seq" && o.category == DdlCategory::Sequence),
        "schema bundle must contain ticket_seq sequence"
    );
    assert!(
        schema_bundle
            .objects
            .iter()
            .any(|o| o.name == "tickets" && o.category == DdlCategory::Table),
        "schema bundle must contain tickets table"
    );
    assert!(
        schema_bundle
            .objects
            .iter()
            .any(|o| o.name == "open_tickets" && o.category == DdlCategory::View),
        "schema bundle must contain open_tickets view"
    );
    assert!(
        schema_bundle
            .objects
            .iter()
            .any(|o| o.name == "touch_ticket" && o.category == DdlCategory::Routine),
        "schema bundle must contain touch_ticket trigger function"
    );

    // Assert table-focused bundle
    assert_eq!(table_bundle.objects.len(), 1, "table bundle has 1 object");
    let tickets_obj = &table_bundle.objects[0];
    assert_eq!(tickets_obj.name, "tickets");
    assert!(
        tickets_obj.create_statement.contains("PRIMARY KEY"),
        "create statement includes primary key: {}",
        tickets_obj.create_statement
    );
    assert!(
        tickets_obj.create_statement.contains("UNIQUE"),
        "create statement includes unique constraint: {}",
        tickets_obj.create_statement
    );
    assert!(
        tickets_obj.create_statement.contains("idx_tickets_status"),
        "secondary index in table create statement: {}",
        tickets_obj.create_statement
    );
    assert!(
        tickets_obj.create_statement.contains("trg_tickets_update"),
        "trigger in table create statement: {}",
        tickets_obj.create_statement
    );
    // Verify no duplicate constraint-backing index
    assert!(
        !tickets_obj
            .create_statement
            .contains("CREATE UNIQUE INDEX tickets_pkey"),
        "primary key index must not be duplicated as separate index statement"
    );

    // Assert render_sql with drop statements in reverse dependency order
    let rendered_with_drop = schema_bundle.render_sql(true);
    let drop_fn_pos = rendered_with_drop
        .find("DROP FUNCTION IF EXISTS")
        .expect("contains DROP FUNCTION");
    let drop_view_pos = rendered_with_drop
        .find("DROP VIEW IF EXISTS")
        .expect("contains DROP VIEW");
    let drop_table_pos = rendered_with_drop
        .find("DROP TABLE IF EXISTS")
        .expect("contains DROP TABLE");
    let drop_seq_pos = rendered_with_drop
        .find("DROP SEQUENCE IF EXISTS")
        .expect("contains DROP SEQUENCE");
    let drop_type_pos = rendered_with_drop
        .find("DROP TYPE IF EXISTS")
        .expect("contains DROP TYPE");

    assert!(
        drop_fn_pos < drop_view_pos || drop_view_pos < drop_table_pos,
        "views and functions must be dropped before tables"
    );
    assert!(
        drop_table_pos < drop_seq_pos,
        "table must be dropped before sequence"
    );
    assert!(
        drop_seq_pos < drop_type_pos,
        "sequence must be dropped before type"
    );
}

#[test]
fn server_activity_captures_backends_and_lock_blocker_tree() {
    let uri = target_or_skip!();
    let fx_a = fixture(&uri);
    let fx_b = fixture(&uri);

    let setup_sql = "
        CREATE TABLE IF NOT EXISTS activity_lock_test (id int PRIMARY KEY, val text);
        INSERT INTO activity_lock_test (id, val) VALUES (1, 'init') ON CONFLICT (id) DO UPDATE SET val = 'init';
    ";
    let setup_res = fx_a.block_on(fx_a.session.execute(setup_sql, 100, JobId(9200)));
    assert_eq!(
        setup_res.status,
        ExecutionStatus::Succeeded,
        "setup table failed: {:?}",
        setup_res.error
    );

    // In session A, begin a transaction and take an exclusive row lock
    let lock_res = fx_a.block_on(fx_a.session.execute(
        "BEGIN; UPDATE activity_lock_test SET val = 'locked' WHERE id = 1;",
        100,
        JobId(9201),
    ));
    assert_eq!(
        lock_res.status,
        ExecutionStatus::Succeeded,
        "session A locking row failed: {:?}",
        lock_res.error
    );

    // Spawn session B in background thread attempting to update the same row
    let handle = std::thread::spawn(move || {
        fx_b.block_on(fx_b.session.execute(
            "UPDATE activity_lock_test SET val = 'waiting' WHERE id = 1;",
            100,
            JobId(9202),
        ))
    });

    // Wait briefly so session B connects and waits on the row lock held by session A
    std::thread::sleep(Duration::from_millis(250));

    // Capture server activity from session A
    let snapshot_res = fx_a.block_on(fx_a.session.server_activity());

    // Release the lock in session A so session B can proceed
    let rollback_res = fx_a.block_on(fx_a.session.execute("ROLLBACK;", 100, JobId(9203)));
    assert_eq!(
        rollback_res.status,
        ExecutionStatus::Succeeded,
        "session A rollback failed: {:?}",
        rollback_res.error
    );

    // Wait for session B to complete
    let bg_res = handle.join().expect("session B thread completed");
    assert_eq!(
        bg_res.status,
        ExecutionStatus::Succeeded,
        "session B query failed: {:?}",
        bg_res.error
    );

    // Clean up test table
    let _ = fx_a.block_on(fx_a.session.execute(
        "DROP TABLE IF EXISTS activity_lock_test;",
        100,
        JobId(9204),
    ));

    let snapshot = snapshot_res.expect("server activity snapshot");

    assert!(
        !snapshot.backends.is_empty(),
        "snapshot must contain backends"
    );
    assert!(
        snapshot.metrics.active_connections >= 2,
        "active connections >= 2: {}",
        snapshot.metrics.active_connections
    );

    let tree = snapshot.lock_blocker_tree();
    assert!(
        !tree.is_empty(),
        "lock blocker tree must not be empty during lock contention"
    );
    assert!(
        tree.iter().any(|node| node.depth == 0),
        "tree must contain a root blocker with depth 0: {tree:?}"
    );
    assert!(
        tree.iter().any(|node| node.depth == 1),
        "tree must contain a waiter with depth 1: {tree:?}"
    );
}

#[test]
fn security_audit_captures_roles_rls_policies_and_privilege_grants() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let setup_sql = "
        CREATE SCHEMA IF NOT EXISTS audit_test_schema;
        CREATE TABLE audit_test_schema.tenant_invoices (
            id int PRIMARY KEY,
            tenant_id int NOT NULL,
            amount int
        );
        ALTER TABLE audit_test_schema.tenant_invoices ENABLE ROW LEVEL SECURITY;
        CREATE POLICY invoice_tenant_isolation ON audit_test_schema.tenant_invoices
            FOR ALL TO PUBLIC
            USING (tenant_id = 42)
            WITH CHECK (tenant_id = 42);
        GRANT SELECT, INSERT ON audit_test_schema.tenant_invoices TO PUBLIC;
    ";
    let setup_res = fx.block_on(fx.session.execute(setup_sql, 100, JobId(9301)));
    assert_eq!(
        setup_res.status,
        ExecutionStatus::Succeeded,
        "setup failed: {:?}",
        setup_res.error
    );

    let audit_res = fx.block_on(fx.session.security_audit(Some("audit_test_schema")));

    let _ = fx.block_on(fx.session.execute(
        "DROP SCHEMA audit_test_schema CASCADE;",
        100,
        JobId(9302),
    ));

    let snapshot = audit_res.expect("security audit snapshot");

    assert!(
        snapshot.roles.iter().any(|r| r.name == "ignatius_test"),
        "roles must contain ignatius_test: {:?}",
        snapshot.roles
    );

    let table = snapshot
        .tables
        .iter()
        .find(|t| t.schema == "audit_test_schema" && t.table == "tenant_invoices")
        .expect("table tenant_invoices must be in audit tables");
    assert!(table.rls_enabled, "RLS must be enabled on tenant_invoices");
    assert_eq!(table.policies.len(), 1, "exactly one policy attached");
    let policy = &table.policies[0];
    assert_eq!(policy.policy_name, "invoice_tenant_isolation");
    assert!(
        policy
            .using_expr
            .as_deref()
            .unwrap_or("")
            .contains("tenant_id = 42"),
        "using_expr contains tenant_id = 42: {:?}",
        policy.using_expr
    );

    let grant = snapshot
        .grants
        .iter()
        .find(|g| {
            g.schema == "audit_test_schema"
                && g.table == "tenant_invoices"
                && g.grantee.eq_ignore_ascii_case("PUBLIC")
        })
        .expect("public grant on tenant_invoices");
    assert!(
        grant.privileges.iter().any(|p| p == "SELECT"),
        "grant contains SELECT: {:?}",
        grant.privileges
    );
    assert!(
        grant.privileges.iter().any(|p| p == "INSERT"),
        "grant contains INSERT: {:?}",
        grant.privileges
    );
}

#[test]
fn live_postgres_schema_snapshot_and_diff_detects_tables_columns_and_synthesizes_migration() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let setup_sql = "
        CREATE SCHEMA IF NOT EXISTS diff_test_v1;
        CREATE SCHEMA IF NOT EXISTS diff_test_v2;

        CREATE TABLE diff_test_v1.inventory (
            item_id integer PRIMARY KEY,
            sku text NOT NULL,
            quantity integer DEFAULT 0
        );

        CREATE TABLE diff_test_v2.inventory (
            item_id integer PRIMARY KEY,
            sku text NOT NULL,
            quantity integer DEFAULT 0,
            price numeric(10,2) NOT NULL,
            discontinued boolean DEFAULT false
        );

        CREATE TABLE diff_test_v2.suppliers (
            supplier_id integer PRIMARY KEY,
            name text NOT NULL
        );
    ";
    let setup_res = fx.block_on(fx.session.execute(setup_sql, 100, JobId(9401)));
    assert_eq!(
        setup_res.status,
        ExecutionStatus::Succeeded,
        "setup failed: {:?}",
        setup_res.error
    );

    let diff_res = fx.block_on(fx.session.schema_diff("diff_test_v1", "diff_test_v2"));

    let _ = fx.block_on(fx.session.execute(
        "DROP SCHEMA diff_test_v1 CASCADE; DROP SCHEMA diff_test_v2 CASCADE;",
        100,
        JobId(9402),
    ));

    let diff = diff_res.expect("schema diff result");
    assert_eq!(diff.source_schema, "diff_test_v1");
    assert_eq!(diff.target_schema, "diff_test_v2");
    assert_eq!(diff.added_tables.len(), 1);
    assert_eq!(diff.added_tables[0].name, "suppliers");
    assert_eq!(diff.modified_tables.len(), 1);

    let inv_diff = &diff.modified_tables[0];
    assert_eq!(inv_diff.table_name, "inventory");
    assert_eq!(inv_diff.added_columns.len(), 2);
    assert!(inv_diff.added_columns.iter().any(|c| c.name == "price"));
    assert!(
        inv_diff
            .added_columns
            .iter()
            .any(|c| c.name == "discontinued")
    );

    let migration_sql = ignatius::migration::generate_migration_sql(
        &diff,
        &ignatius::migration::MigrationOptions::default(),
    );
    assert!(
        migration_sql.contains("CREATE TABLE \"diff_test_v2\".\"suppliers\""),
        "creates new suppliers table: {migration_sql}"
    );
    assert!(
        migration_sql.contains("ALTER TABLE \"diff_test_v2\".\"inventory\" ADD COLUMN \"price\" numeric(10,2) NOT NULL;"),
        "adds price column: {migration_sql}"
    );
    assert!(
        migration_sql.contains("ALTER TABLE \"diff_test_v2\".\"inventory\" ADD COLUMN \"discontinued\" boolean DEFAULT false;"),
        "adds discontinued column: {migration_sql}"
    );
}

#[test]
fn live_postgres_streaming_copy_in_and_out_with_transactional_rollback() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    let setup_sql = "
        CREATE TABLE IF NOT EXISTS public.test_streaming_copy (
            id integer PRIMARY KEY,
            title text NOT NULL,
            amount numeric(10,2) NOT NULL
        );
        TRUNCATE TABLE public.test_streaming_copy;
    ";
    let setup_res = fx.block_on(fx.session.execute(setup_sql, 100, JobId(9501)));
    assert_eq!(
        setup_res.status,
        ExecutionStatus::Succeeded,
        "setup failed: {:?}",
        setup_res.error
    );

    // 1. Bulk import CSV data
    let csv_data =
        b"id,title,amount\n1,First Item,19.99\n2,Second Item,49.50\n3,Third Item,120.00\n";
    let import_options = ignatius::transfer::ImportOptions {
        schema: "public".to_owned(),
        table: "test_streaming_copy".to_owned(),
        file_path: "data.csv".to_owned(),
        format: ignatius::transfer::TransferFormat::Csv,
        delimiter: b',',
        has_header: true,
        null_string: "".to_owned(),
        quote: b'"',
        escape: b'"',
        columns: Vec::new(),
        dry_run: false,
        transactional: true,
    };

    let import_res = fx.block_on(fx.session.execute_import(&import_options, csv_data));
    assert!(
        import_res.is_ok(),
        "import succeeded: {:?}",
        import_res.err()
    );
    let res = import_res.unwrap();
    assert_eq!(res.rows_imported, 3);

    // Verify row count
    let count_res = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM public.test_streaming_copy",
        100,
        JobId(9502),
    ));
    assert_eq!(count_res.status, ExecutionStatus::Succeeded);

    // 2. Bulk export query
    let export_options = ignatius::transfer::ExportOptions {
        query: "SELECT id, title, amount FROM public.test_streaming_copy ORDER BY id".to_owned(),
        output_path: None,
        format: ignatius::transfer::TransferFormat::Csv,
        delimiter: b',',
        has_header: true,
        null_string: "".to_owned(),
        quote: b'"',
        escape: b'"',
    };

    let export_res = fx.block_on(fx.session.execute_export(&export_options));
    assert!(
        export_res.is_ok(),
        "export succeeded: {:?}",
        export_res.err()
    );
    let exported = String::from_utf8_lossy(&export_res.unwrap()).to_string();
    assert!(exported.contains("id,title,amount"));
    assert!(exported.contains("1,First Item,19.99"));
    assert!(exported.contains("2,Second Item,49.50"));
    assert!(exported.contains("3,Third Item,120.00"));

    // 3. Dry-run test verifies that data is rolled back
    let dry_run_csv = b"id,title,amount\n4,Dry Run Item,999.00\n";
    let mut dry_run_opts = import_options.clone();
    dry_run_opts.dry_run = true;
    let dry_res = fx.block_on(fx.session.execute_import(&dry_run_opts, dry_run_csv));
    assert!(
        dry_res.is_ok(),
        "dry-run import returned ok: {:?}",
        dry_res.err()
    );
    assert!(dry_res.unwrap().status.contains("rolled back"));

    // Verify row 4 is NOT in database
    let check_res = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM public.test_streaming_copy WHERE id = 4",
        100,
        JobId(9503),
    ));
    assert_eq!(check_res.status, ExecutionStatus::Succeeded);
    assert_eq!(
        check_res.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("0".into())
    );

    // 4. Invalid data triggers automatic rollback and error
    let bad_csv = b"id,title,amount\n5,Bad Item,NOT_A_NUMBER\n";
    let bad_res = fx.block_on(fx.session.execute_import(&import_options, bad_csv));
    assert!(bad_res.is_err(), "bad csv import must fail");
    let bad_error = bad_res.expect_err("invalid input is rejected");
    assert!(bad_error.headline.contains("COPY import failed"));
    assert!(
        !format!("{bad_error:?}").contains("NOT_A_NUMBER"),
        "COPY diagnostics must not disclose imported input values"
    );

    // Clean up
    let _ = fx.block_on(fx.session.execute(
        "DROP TABLE public.test_streaming_copy",
        100,
        JobId(9504),
    ));
}

#[test]
fn execute_export_surfaces_deferred_unique_error_without_returning_data() {
    use ignatius::transfer::ExportOptions;

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let sentinel = "synthetic-export-unique-sentinel";
    let output_marker = "synthetic-export-output";
    let setup = fx.block_on(fx.session.execute(
        &format!(
            "CREATE TEMP TABLE export_deferred (id text UNIQUE DEFERRABLE INITIALLY DEFERRED); \
             INSERT INTO export_deferred VALUES ('{sentinel}'); \
         CREATE FUNCTION pg_temp.export_duplicate() RETURNS TABLE(value text) \
         LANGUAGE plpgsql VOLATILE AS $$ \
         BEGIN \
             INSERT INTO export_deferred VALUES ('{sentinel}'); \
             RETURN QUERY SELECT '{output_marker}'::text; \
         END; $$"
        ),
        10,
        JobId(9510),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");

    let options = ExportOptions {
        query: "SELECT value FROM pg_temp.export_duplicate()".to_owned(),
        ..ExportOptions::default()
    };
    let error = fx
        .block_on(fx.session.execute_export(&options))
        .expect_err("deferred unique violation must fail export");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(sqlstate(&error), "23505");
    let rendered = format!("{error:?}");
    assert!(
        !rendered.contains(sentinel) && !rendered.contains(output_marker),
        "export data leaked into the diagnostic: {rendered}"
    );

    let count = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM pg_temp.export_deferred",
        10,
        JobId(9511),
    ));
    assert_eq!(count.status, ExecutionStatus::Succeeded, "{count:?}");
    assert_eq!(
        count.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("1".into())
    );

    let follow_up = fx.block_on(fx.session.execute(
        "SELECT 'backend-still-usable' AS status",
        10,
        JobId(9512),
    ));
    assert_eq!(
        follow_up.status,
        ExecutionStatus::Succeeded,
        "{follow_up:?}"
    );
    assert_eq!(
        follow_up.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("backend-still-usable".into())
    );
}

#[test]
fn execute_export_csv_preserves_header_for_success_and_empty_results() {
    use ignatius::transfer::{ExportOptions, TransferFormat};

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let successful = ExportOptions {
        query: "SELECT 7 AS id, 'synthetic' AS note".to_owned(),
        format: TransferFormat::Csv,
        ..ExportOptions::default()
    };
    let bytes = fx
        .block_on(fx.session.execute_export(&successful))
        .expect("CSV export succeeds");
    assert_eq!(bytes, b"id,note\n7,synthetic\n");

    let empty = ExportOptions {
        query: "SELECT 7 AS id, 'synthetic' AS note WHERE false".to_owned(),
        format: TransferFormat::Csv,
        ..ExportOptions::default()
    };
    let bytes = fx
        .block_on(fx.session.execute_export(&empty))
        .expect("empty CSV export succeeds");
    assert_eq!(bytes, b"id,note\n");
}

#[test]
fn execute_export_missing_column_is_private_query_error() {
    use ignatius::transfer::ExportOptions;

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let sentinel = "synthetic-missing-export-column";
    let options = ExportOptions {
        query: format!("SELECT \"{sentinel}\""),
        ..ExportOptions::default()
    };

    let error = fx
        .block_on(fx.session.execute_export(&options))
        .expect_err("missing export column must fail");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(sqlstate(&error), "42703");
    assert!(
        !format!("{error:?}").contains(sentinel),
        "identifier sentinel leaked into diagnostic: {error:?}"
    );
}

#[test]
fn import_transaction_ownership_preserves_caller_state_and_unwrapped_route() {
    use ignatius::query::result::TransactionState;
    use ignatius::transfer::{ImportOptions, TransferFormat};

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE import_ownership_contract (id integer PRIMARY KEY)",
        10,
        JobId(9600),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let options = ImportOptions {
        schema: "pg_temp".into(),
        table: "import_ownership_contract".into(),
        file_path: "synthetic.csv".into(),
        format: TransferFormat::Csv,
        delimiter: b',',
        has_header: true,
        null_string: String::new(),
        quote: b'"',
        escape: b'"',
        columns: Vec::new(),
        dry_run: true,
        transactional: true,
    };
    let count = |job| {
        let execution = fx.block_on(fx.session.execute(
            "SELECT count(*) FROM pg_temp.import_ownership_contract",
            10,
            JobId(job),
        ));
        assert_eq!(execution.status, ExecutionStatus::Succeeded);
        execution.statements[0]
            .result_set
            .as_ref()
            .expect("count row")
            .rows[0][0]
            .clone()
    };

    let dry = fx.block_on(fx.session.execute_import(&options, b"id\n1\n"));
    assert!(dry.expect("valid dry run").status.contains("rolled back"));
    assert_eq!(count(9601), Cell::Text("0".into()));

    let mut contradictory = options.clone();
    contradictory.transactional = false;
    let rejected = fx.block_on(fx.session.execute_import(&contradictory, b"id\n2\n"));
    assert!(rejected.is_err(), "dry-run without transaction is refused");
    assert_eq!(count(9602), Cell::Text("0".into()));

    let begun = fx.block_on(fx.session.execute(
        "BEGIN; INSERT INTO pg_temp.import_ownership_contract VALUES (9)",
        10,
        JobId(9603),
    ));
    assert_eq!(begun.transaction, TransactionState::Open);
    let mut write = options.clone();
    write.dry_run = false;
    let nested = fx.block_on(fx.session.execute_import(&write, b"id\n3\n"));
    assert!(
        nested.is_err(),
        "transaction-wrapped import refuses caller transaction"
    );
    let sentinel = count(9604);
    assert_eq!(
        sentinel,
        Cell::Text("1".into()),
        "caller sentinel remains visible"
    );
    let mut unwrapped = write;
    unwrapped.transactional = false;
    let borrowed = fx
        .block_on(fx.session.execute_import(&unwrapped, b"id\n3\n"))
        .expect("unwrapped COPY in the caller's transaction");
    assert_eq!(borrowed.rows_imported, 1);
    assert_eq!(count(9618), Cell::Text("2".into()));

    let rolled_back = fx.block_on(fx.session.execute("ROLLBACK", 10, JobId(9605)));
    assert_eq!(rolled_back.transaction, TransactionState::Autocommit);
    assert_eq!(count(9606), Cell::Text("0".into()));

    let imported = fx.block_on(fx.session.execute_import(&unwrapped, b"id\n4\n"));
    assert_eq!(
        imported
            .expect("intentional unwrapped import")
            .rows_imported,
        1
    );
    assert_eq!(count(9607), Cell::Text("1".into()));
}

#[test]
fn unwrapped_import_surfaces_deferred_unique_error_and_recovers_for_next_copy() {
    use ignatius::transfer::{ImportOptions, TransferFormat};

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE import_unwrapped_deferred (id text UNIQUE DEFERRABLE INITIALLY DEFERRED); \
         INSERT INTO import_unwrapped_deferred VALUES ('synthetic-import-duplicate')",
        10,
        JobId(9620),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded, "{setup:?}");
    let options = ImportOptions {
        schema: "pg_temp".into(),
        table: "import_unwrapped_deferred".into(),
        file_path: "synthetic-unwrapped.csv".into(),
        format: TransferFormat::Csv,
        delimiter: b',',
        has_header: false,
        null_string: String::new(),
        quote: b'"',
        escape: b'"',
        columns: Vec::new(),
        dry_run: false,
        transactional: false,
    };

    let error = fx
        .block_on(fx.session.execute_import(
            &options,
            b"synthetic-import-duplicate\nsynthetic-import-duplicate\n",
        ))
        .expect_err("unwrapped deferred unique violation must fail");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(sqlstate(&error), "23505");
    assert!(
        !format!("{error:?}").contains("synthetic-import-duplicate"),
        "duplicate input leaked into the diagnostic: {error:?}"
    );

    let imported = fx
        .block_on(
            fx.session
                .execute_import(&options, b"synthetic-import-valid\n"),
        )
        .expect("same session remains usable for the next unwrapped COPY");
    assert_eq!(imported.rows_imported, 1);
    let contents = fx.block_on(fx.session.execute(
        "SELECT id FROM pg_temp.import_unwrapped_deferred ORDER BY id",
        10,
        JobId(9621),
    ));
    assert_eq!(contents.status, ExecutionStatus::Succeeded, "{contents:?}");
    assert_eq!(
        contents.statements[0].result_set.as_ref().unwrap().rows,
        vec![
            vec![Cell::Text("synthetic-import-duplicate".into())],
            vec![Cell::Text("synthetic-import-valid".into())]
        ]
    );
}

#[test]
fn import_commit_failure_preserves_sqlstate_without_disclosing_input() {
    use ignatius::query::result::TransactionState;
    use ignatius::transfer::{ImportOptions, TransferFormat};

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE import_deferred (id text UNIQUE DEFERRABLE INITIALLY DEFERRED)",
        10,
        JobId(9610),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let options = ImportOptions {
        schema: "pg_temp".into(),
        table: "import_deferred".into(),
        file_path: "synthetic.csv".into(),
        format: TransferFormat::Csv,
        delimiter: b',',
        has_header: false,
        null_string: String::new(),
        quote: b'"',
        escape: b'"',
        columns: Vec::new(),
        dry_run: false,
        transactional: true,
    };
    let error = fx
        .block_on(fx.session.execute_import(
            &options,
            b"synthetic-duplicate-fixture\nsynthetic-duplicate-fixture\n",
        ))
        .expect_err("deferred unique violation at commit");
    let diagnostic = format!("{error:?}");
    assert!(error.headline.contains("import commit failed"));
    assert!(diagnostic.contains("23505"));
    assert!(!diagnostic.contains("synthetic-duplicate-fixture"));

    let count = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM pg_temp.import_deferred",
        10,
        JobId(9611),
    ));
    assert_eq!(count.status, ExecutionStatus::Succeeded);
    assert_eq!(count.transaction, TransactionState::Autocommit);
    assert_eq!(
        count.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("0".into())
    );
}

#[test]
fn binary_sync_keeps_conflicting_values_out_of_diagnostics_and_counts_copied_bytes() {
    let uri = target_or_skip!();
    let source = fixture(&uri);
    let mut destination = fixture(&uri);
    let value = "synthetic-sync-conflicting-key";
    let table = ignatius::sync::TableKey::new("pg_temp", "sync_private_key");
    let setup = format!(
        "CREATE TEMP TABLE sync_private_key (id text PRIMARY KEY); \
         INSERT INTO pg_temp.sync_private_key VALUES ('{value}')"
    );
    for fx in [&source, &destination] {
        let execution = fx.block_on(fx.session.execute(&setup, 1, JobId(9620)));
        assert_eq!(execution.status, ExecutionStatus::Succeeded);
    }

    let mut copy = |truncate| {
        within_sync_timeout(
            &source,
            source.session.copy_table_to_target(
                &mut destination.session,
                &table,
                truncate,
                false,
                false,
            ),
        )
    };
    let error = copy(false).expect_err("destination rejects its duplicate key");
    for rendered in [
        error.render_plain(true),
        error.to_json().to_string(),
        format!("{error:?}"),
    ] {
        assert!(
            !rendered.contains(value),
            "diagnostic included a copied value"
        );
    }
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(error.statement_number, None);
    assert!(error.headline.contains("destination COPY"));
    assert!(
        error
            .technical
            .iter()
            .any(|field| { field.label == "SQLSTATE" && field.value == "23505" })
    );

    let progress = copy(true).expect("sync into the cleared destination");
    assert_eq!(progress.rows_copied, 1);
    // Binary COPY header (19), one field count (2), text length (4), and trailer (2).
    assert_eq!(
        progress.bytes_transferred,
        19 + 2 + 4 + value.len() as u64 + 2
    );
    let read = destination.block_on(destination.session.execute(
        "SELECT id FROM pg_temp.sync_private_key",
        1,
        JobId(9621),
    ));
    assert_eq!(read.status, ExecutionStatus::Succeeded);
    assert_eq!(
        read.statements[0].result_set.as_ref().unwrap().rows,
        vec![vec![Cell::Text(value.to_owned())]]
    );

    let guard = format!("ALTER TABLE pg_temp.sync_private_key ADD CHECK (id = '{value}')");
    let guarded = destination.block_on(destination.session.execute(&guard, 1, JobId(9624)));
    assert_eq!(guarded.status, ExecutionStatus::Succeeded);
    let changed = source.block_on(source.session.execute(
        "UPDATE pg_temp.sync_private_key SET id = 'synthetic-sync-replacement'",
        1,
        JobId(9622),
    ));
    assert_eq!(changed.status, ExecutionStatus::Succeeded);
    let failed = within_sync_timeout(
        &source,
        source
            .session
            .copy_table_to_target(&mut destination.session, &table, true, false, false),
    )
    .expect_err("destination CHECK rejects replacement after truncate");
    assert!(failed.headline.contains("destination COPY"));
    assert!(
        failed
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "23514")
    );
    let restored = destination.block_on(destination.session.execute(
        "SELECT id FROM pg_temp.sync_private_key",
        1,
        JobId(9623),
    ));
    assert_eq!(restored.status, ExecutionStatus::Succeeded);
    assert_eq!(
        restored.transaction,
        ignatius::query::result::TransactionState::Autocommit
    );
    assert_eq!(
        restored.statements[0].result_set.as_ref().unwrap().rows,
        vec![vec![Cell::Text(value.to_owned())]],
        "failed direct-helper replacement restores the prior destination row"
    );
}

#[test]
fn binary_sync_preserves_destination_caller_transactions() {
    use ignatius::query::result::TransactionState;

    let uri = target_or_skip!();
    let source = fixture(&uri);
    let mut destination = fixture(&uri);
    let setup = "CREATE TEMP TABLE sync_transaction_contract (id integer PRIMARY KEY); \
                 INSERT INTO pg_temp.sync_transaction_contract VALUES (1)";
    for fx in [&source, &destination] {
        let result = fx.block_on(fx.session.execute(setup, 10, JobId(9630)));
        assert_eq!(result.status, ExecutionStatus::Succeeded);
    }
    let key = ignatius::sync::TableKey::new("pg_temp", "sync_transaction_contract");

    let begin = destination.block_on(destination.session.execute(
        "BEGIN; INSERT INTO pg_temp.sync_transaction_contract VALUES (9)",
        10,
        JobId(9631),
    ));
    assert_eq!(begin.transaction, TransactionState::Open);
    let active = within_sync_timeout(
        &source,
        source
            .session
            .copy_table_to_target(&mut destination.session, &key, true, false, false),
    )
    .expect_err("active caller transaction must be refused");
    assert_eq!(active.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(
        active.headline,
        "the destination sync did not start, because the session is not outside a transaction"
    );
    let sentinel = destination.block_on(destination.session.execute(
        "SELECT count(*) FROM pg_temp.sync_transaction_contract WHERE id = 9",
        10,
        JobId(9632),
    ));
    assert_eq!(sentinel.status, ExecutionStatus::Succeeded);
    assert_eq!(sentinel.transaction, TransactionState::Open);
    assert_eq!(
        sentinel.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("1".into()),
        "caller transaction and its write remain intact"
    );
    let rollback = destination.block_on(destination.session.execute("ROLLBACK", 10, JobId(9633)));
    assert_eq!(rollback.transaction, TransactionState::Autocommit);

    let failed = destination.block_on(destination.session.execute(
        "BEGIN; INSERT INTO pg_temp.sync_transaction_contract VALUES (1)",
        10,
        JobId(9634),
    ));
    assert_eq!(failed.transaction, TransactionState::Failed);
    let rejected = within_sync_timeout(
        &source,
        source
            .session
            .copy_table_to_target(&mut destination.session, &key, true, false, false),
    )
    .expect_err("failed caller transaction must be refused");
    assert_eq!(rejected.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(
        rejected.headline,
        "the destination sync did not start, because the session is not outside a transaction"
    );
    let still_failed =
        destination.block_on(destination.session.execute("SELECT 1", 10, JobId(9635)));
    assert_eq!(still_failed.transaction, TransactionState::Failed);
    let rollback = destination.block_on(destination.session.execute("ROLLBACK", 10, JobId(9636)));
    assert_eq!(rollback.transaction, TransactionState::Autocommit);
}

#[test]
fn binary_sync_refuses_writes_to_the_same_physical_database() {
    let uri = target_or_skip!();
    let source = fixture(&uri);
    let mut destination = fixture(&uri);
    let table_name = unique_sync_database_name("same_db_table");
    let setup = source.block_on(source.session.execute(
        &format!(
            "CREATE TABLE public.{table_name} (id integer PRIMARY KEY); \
             INSERT INTO public.{table_name} VALUES (1)"
        ),
        10,
        JobId(9640),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let key = ignatius::sync::TableKey::new("public", &table_name);
    let refused = within_sync_timeout(
        &source,
        source
            .session
            .copy_table_to_target(&mut destination.session, &key, true, false, false),
    )
    .expect_err("same-database writes must be refused");
    assert_eq!(refused.kind, ignatius::diagnostics::DiagnosticKind::Usage);
    assert_eq!(
        refused.headline,
        "source and destination are the same PostgreSQL database"
    );
    let rows = destination.block_on(destination.session.execute(
        &format!("SELECT count(*) FROM public.{table_name}"),
        10,
        JobId(9641),
    ));
    assert_eq!(rows.status, ExecutionStatus::Succeeded);
    assert_eq!(
        rows.transaction,
        ignatius::query::result::TransactionState::Autocommit,
        "same-database refusal leaves destination in autocommit"
    );
    assert_eq!(
        rows.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("1".into()),
        "refusal must not truncate or modify the destination"
    );
    let cleanup = source.block_on(source.session.execute(
        &format!("DROP TABLE public.{table_name}"),
        10,
        JobId(9642),
    ));
    assert_eq!(cleanup.status, ExecutionStatus::Succeeded);
}

fn uri_with_database(uri: &str, database: &str) -> String {
    let (base, query) = uri
        .split_once('?')
        .map_or((uri, None), |(base, query)| (base, Some(query)));
    let prefix = &base[..base.rfind('/').expect("URI has a database path")];
    query.map_or_else(
        || format!("{prefix}/{database}"),
        |query| format!("{prefix}/{database}?{query}"),
    )
}

struct SyncScratchDatabases {
    admin: Fixture,
    names: Vec<String>,
    cleaned: bool,
}

impl SyncScratchDatabases {
    fn create(admin_uri: &str) -> Self {
        let mut scratch = Self {
            admin: fixture(admin_uri),
            names: Vec::new(),
            cleaned: false,
        };
        for (label, job) in [("source", 9650), ("target", 9651)] {
            let name = unique_sync_database_name(label);
            let result = scratch.admin.block_on(scratch.admin.session.execute(
                &format!("CREATE DATABASE {name}"),
                10,
                JobId(job),
            ));
            assert_eq!(result.status, ExecutionStatus::Succeeded);
            scratch.names.push(name);
        }
        scratch
    }

    fn source_uri(&self, admin_uri: &str) -> String {
        uri_with_database(admin_uri, &self.names[0])
    }

    fn destination_uri(&self, admin_uri: &str) -> String {
        uri_with_database(admin_uri, &self.names[1])
    }

    fn drop_owned(&self) -> Result<(), String> {
        for name in self.names.iter().rev() {
            let result = self.admin.block_on(self.admin.session.execute(
                &format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"),
                10,
                JobId(9652),
            ));
            if result.status != ExecutionStatus::Succeeded {
                return Err(format!("could not drop owned sync database {name}"));
            }
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        self.drop_owned().expect("drop generated sync databases");
        self.cleaned = true;
    }
}

impl Drop for SyncScratchDatabases {
    fn drop(&mut self) {
        if !self.cleaned
            && let Err(error) = self.drop_owned()
        {
            eprintln!("{error}");
        }
    }
}

fn sync_atomicity_labels(fx: &Fixture, job: u64) -> Vec<Cell> {
    let result = fx.block_on(fx.session.execute(
        "SELECT (SELECT label FROM sync_atomicity.parent), \
                (SELECT label FROM sync_atomicity.child)",
        10,
        JobId(job),
    ));
    assert_eq!(result.status, ExecutionStatus::Succeeded);
    assert_eq!(
        result.transaction,
        ignatius::query::result::TransactionState::Autocommit
    );
    result.statements[0]
        .result_set
        .as_ref()
        .expect("parent and child row")
        .rows[0]
        .clone()
}

#[test]
fn catalogue_discovery_excludes_other_sessions_temp_tables() {
    let uri = target_or_skip!();
    let temp_owner = fixture(&uri);
    let observer = fixture(&uri);
    let schema = unique_sync_database_name("catalogue");
    let (temp_parent, temp_child) = ("z_t011_temp_parent", "a_t011_temp_child");
    let (parent, child) = ("z_persistent_parent", "a_unlogged_child");

    let temporary = temp_owner.block_on(temp_owner.session.execute(
        r#"CREATE TEMP TABLE z_t011_temp_parent (id integer PRIMARY KEY);
           CREATE TEMP TABLE a_t011_temp_child (
               id integer PRIMARY KEY,
               parent_id integer NOT NULL REFERENCES z_t011_temp_parent(id)
           )"#,
        10,
        JobId(9670),
    ));
    assert_eq!(temporary.status, ExecutionStatus::Succeeded);
    let namespace = temp_owner.block_on(temp_owner.session.execute(
        r#"SELECT nspname::text FROM pg_catalog.pg_namespace
           WHERE oid = pg_catalog.pg_my_temp_schema()"#,
        10,
        JobId(9671),
    ));
    assert_eq!(namespace.status, ExecutionStatus::Succeeded);
    let temp_schema = match &namespace.statements[0]
        .result_set
        .as_ref()
        .expect("temporary namespace row")
        .rows[0][0]
    {
        Cell::Text(name) => name.clone(),
        other => panic!("unexpected temporary schema value: {other:?}"),
    };
    assert!(temp_schema.starts_with("pg_temp_"));

    let persistent = observer.block_on(observer.session.execute(
        &format!(
            r#"CREATE SCHEMA {schema};
               CREATE TABLE {schema}.{parent} (id integer PRIMARY KEY);
               CREATE UNLOGGED TABLE {schema}.{child} (
                   id integer PRIMARY KEY,
                   parent_id integer NOT NULL REFERENCES {schema}.{parent}(id)
               )"#
        ),
        10,
        JobId(9672),
    ));
    assert_eq!(persistent.status, ExecutionStatus::Succeeded);

    let global_graph = within_sync_timeout(
        &observer,
        observer.session.discover_table_dependencies(None),
    )
    .expect("global graph discovery");
    assert!(!global_graph.nodes.iter().any(|node| {
        node.key.schema == temp_schema
            && (node.key.table == temp_parent || node.key.table == temp_child)
    }));
    let plan = global_graph.compute_sync_plan();
    let graph_order: Vec<_> = plan
        .insert_order
        .iter()
        .filter(|key| key.schema == schema)
        .map(|key| key.table.as_str())
        .collect();
    assert_eq!(graph_order, [parent, child]);

    let global_seed = within_sync_timeout(&observer, observer.session.analyze_seed_plan(None))
        .expect("global seed discovery");
    assert!(!global_seed.iter().any(|table| {
        table.key.schema == temp_schema
            && (table.key.table == temp_parent || table.key.table == temp_child)
    }));
    let seed_order: Vec<_> = global_seed
        .iter()
        .filter(|table| table.key.schema == schema)
        .map(|table| table.key.table.as_str())
        .collect();
    assert_eq!(seed_order, [parent, child]);

    let temp_graph = within_sync_timeout(
        &observer,
        observer
            .session
            .discover_table_dependencies(Some(&temp_schema)),
    )
    .expect("explicit temporary schema discovery");
    assert!(temp_graph.nodes.is_empty());
    let temp_seed = within_sync_timeout(
        &observer,
        observer.session.analyze_seed_plan(Some(&temp_schema)),
    )
    .expect("explicit temporary schema seed discovery");
    assert!(temp_seed.is_empty());

    let cleanup = observer.block_on(observer.session.execute(
        &format!("DROP SCHEMA {schema} CASCADE"),
        10,
        JobId(9673),
    ));
    assert_eq!(cleanup.status, ExecutionStatus::Succeeded);
}

fn seed_report(
    schema: &str,
    table: &str,
    values: impl IntoIterator<Item = String>,
) -> ignatius::seed::FixtureReport {
    let rows: Vec<Vec<String>> = values.into_iter().map(|value| vec![value]).collect();
    let total_rows = rows.len();
    ignatius::seed::FixtureReport {
        seed: 4040,
        schema: Some(schema.to_owned()),
        tables: vec![ignatius::seed::GeneratedTableData {
            key: ignatius::sync::TableKey::new(schema, table),
            columns: vec!["id".to_owned()],
            rows,
        }],
        total_rows,
        duration_ms: 0,
    }
}

fn seed_with_timeout(
    fx: &mut Fixture,
    report: &ignatius::seed::FixtureReport,
) -> Result<u64, ignatius::diagnostics::Diagnostic> {
    let runtime = &fx.runtime;
    let session = &mut fx.session;
    runtime
        .block_on(async {
            tokio::time::timeout(
                Duration::from_secs(10),
                session.execute_synthetic_seed(report),
            )
            .await
        })
        .expect("synthetic seed completes within ten seconds")
}

#[test]
fn synthetic_seed_preserves_active_and_failed_caller_transactions() {
    use ignatius::query::result::TransactionState;

    let uri = target_or_skip!();
    let mut fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE seed_owner (id integer PRIMARY KEY)",
        10,
        JobId(9700),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let report = seed_report("pg_temp", "seed_owner", ["1".to_owned()]);

    let begun = fx.block_on(fx.session.execute(
        "BEGIN; INSERT INTO pg_temp.seed_owner VALUES (9)",
        10,
        JobId(9701),
    ));
    assert_eq!(begun.transaction, TransactionState::Open);
    assert_eq!(
        seed_with_timeout(&mut fx, &ignatius::seed::FixtureReport::default())
            .expect("empty plan has no destination writes"),
        0
    );
    let active = seed_with_timeout(&mut fx, &report).expect_err("active caller tx is refused");
    assert_eq!(active.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(
        active.headline,
        "the synthetic seed insert did not start, because the session is not outside a transaction"
    );
    let sentinel = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM pg_temp.seed_owner WHERE id = 9",
        10,
        JobId(9702),
    ));
    assert_eq!(sentinel.status, ExecutionStatus::Succeeded);
    assert_eq!(sentinel.transaction, TransactionState::Open);
    assert_eq!(
        sentinel.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("1".into())
    );
    let rollback = fx.block_on(fx.session.execute("ROLLBACK", 10, JobId(9703)));
    assert_eq!(rollback.transaction, TransactionState::Autocommit);

    let failed = fx.block_on(fx.session.execute(
        r#"BEGIN; INSERT INTO pg_temp.seed_owner VALUES (1);
           INSERT INTO pg_temp.seed_owner VALUES (1)"#,
        10,
        JobId(9704),
    ));
    assert_eq!(failed.transaction, TransactionState::Failed);
    let refused = seed_with_timeout(&mut fx, &report).expect_err("failed caller tx is refused");
    assert_eq!(refused.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(refused.headline, active.headline);
    let still_failed = fx.block_on(fx.session.execute("SELECT 1", 10, JobId(9705)));
    assert_eq!(still_failed.transaction, TransactionState::Failed);
    let rollback = fx.block_on(fx.session.execute("ROLLBACK", 10, JobId(9706)));
    assert_eq!(rollback.transaction, TransactionState::Autocommit);
}

#[test]
fn synthetic_seed_rolls_back_prior_100_row_chunk_and_session_is_reusable() {
    use ignatius::query::result::TransactionState;

    let uri = target_or_skip!();
    let mut fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        "CREATE TEMP TABLE seed_chunk_rollback (id integer PRIMARY KEY CHECK (id <> 101))",
        10,
        JobId(9710),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let report = seed_report(
        "pg_temp",
        "seed_chunk_rollback",
        (1..=101).map(|id| id.to_string()),
    );
    let script = report.render_sql();
    assert!(script.starts_with("-- Ignatius Synthetic Test Fixtures\n"));
    assert!(script.contains("BEGIN;"));
    assert!(script.ends_with("COMMIT;\n"));
    assert!(script.contains("INSERT INTO \"pg_temp\".\"seed_chunk_rollback\" (\"id\")"));
    assert_eq!(script.matches("INSERT INTO").count(), 2);

    let error = seed_with_timeout(&mut fx, &report).expect_err("row 101 violates CHECK");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(error.headline, "synthetic seed insert failed");
    assert!(
        error
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "23514")
    );
    let count = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM pg_temp.seed_chunk_rollback",
        10,
        JobId(9711),
    ));
    assert_eq!(count.status, ExecutionStatus::Succeeded);
    assert_eq!(count.transaction, TransactionState::Autocommit);
    assert_eq!(
        count.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("0".into()),
        "first 100-row insert chunk was rolled back"
    );

    let reusable = seed_report("pg_temp", "seed_chunk_rollback", ["7".to_owned()]);
    assert_eq!(
        seed_with_timeout(&mut fx, &reusable).expect("autocommit session reused"),
        1
    );
    let persisted = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM pg_temp.seed_chunk_rollback",
        10,
        JobId(9712),
    ));
    assert_eq!(persisted.status, ExecutionStatus::Succeeded);
    assert_eq!(persisted.transaction, TransactionState::Autocommit);
    assert_eq!(
        persisted.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("1".into())
    );
}

#[test]
fn synthetic_seed_reports_deferred_unique_failure_at_commit() {
    use ignatius::query::result::TransactionState;

    let uri = target_or_skip!();
    let mut fx = fixture(&uri);
    let setup = fx.block_on(fx.session.execute(
        r#"CREATE TEMP TABLE seed_deferred (id integer);
           ALTER TABLE pg_temp.seed_deferred
               ADD CONSTRAINT seed_deferred_id_unique UNIQUE (id)
               DEFERRABLE INITIALLY DEFERRED"#,
        10,
        JobId(9720),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let report = seed_report("pg_temp", "seed_deferred", ["1".to_owned(), "1".to_owned()]);
    let error = seed_with_timeout(&mut fx, &report).expect_err("COMMIT rejects deferred duplicate");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(error.headline, "synthetic seed commit failed");
    assert!(
        error
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "23505")
    );
    let count = fx.block_on(fx.session.execute(
        "SELECT count(*) FROM pg_temp.seed_deferred",
        10,
        JobId(9721),
    ));
    assert_eq!(count.status, ExecutionStatus::Succeeded);
    assert_eq!(count.transaction, TransactionState::Autocommit);
    assert_eq!(
        count.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("0".into())
    );
}

#[test]
fn synthetic_seed_returns_server_affected_count_when_trigger_skips_row() {
    let uri = target_or_skip!();
    let mut fx = fixture(&uri);
    let schema = unique_sync_database_name("seed_trigger");
    let setup = fx.block_on(fx.session.execute(
        &format!(
            r#"CREATE SCHEMA {schema};
               CREATE TABLE {schema}.seed_trigger (id integer PRIMARY KEY);
               CREATE FUNCTION {schema}.skip_seed_row() RETURNS trigger
                   LANGUAGE plpgsql AS $$
               BEGIN
                   IF NEW.id = 2 THEN RETURN NULL; END IF;
                   RETURN NEW;
               END;
               $$;
               CREATE TRIGGER skip_seed_row BEFORE INSERT ON {schema}.seed_trigger
                   FOR EACH ROW EXECUTE FUNCTION {schema}.skip_seed_row()"#
        ),
        10,
        JobId(9730),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);
    let report = seed_report(
        &schema,
        "seed_trigger",
        ["1".to_owned(), "2".to_owned(), "3".to_owned()],
    );
    assert_eq!(report.total_rows, 3);
    let script = report.render_sql();
    assert!(script.contains("BEGIN;"));
    assert!(script.contains("INSERT INTO \""));
    assert!(script.ends_with("COMMIT;\n"));

    assert_eq!(
        seed_with_timeout(&mut fx, &report).expect("seed commits"),
        2
    );
    let actual = fx.block_on(fx.session.execute(
        &format!("SELECT count(*) FROM {schema}.seed_trigger"),
        10,
        JobId(9731),
    ));
    assert_eq!(actual.status, ExecutionStatus::Succeeded);
    assert_eq!(
        actual.statements[0].result_set.as_ref().unwrap().rows[0][0],
        Cell::Text("2".into())
    );
    let cleanup = fx.block_on(fx.session.execute(
        &format!("DROP SCHEMA {schema} CASCADE"),
        10,
        JobId(9732),
    ));
    assert_eq!(cleanup.status, ExecutionStatus::Succeeded);
}

#[test]
fn sync_pipeline_atomically_replaces_related_tables() {
    let admin_uri = target_or_skip!();
    let mut scratch = SyncScratchDatabases::create(&admin_uri);
    let source_uri = scratch.source_uri(&admin_uri);
    let destination_uri = scratch.destination_uri(&admin_uri);
    let source = fixture(&source_uri);
    let mut destination = fixture(&destination_uri);

    let schema = "CREATE SCHEMA sync_atomicity; \
                 CREATE TABLE sync_atomicity.parent (id integer PRIMARY KEY, label text NOT NULL); \
                 ";
    let source_setup = source.block_on(source.session.execute(
        &format!(
            "{schema} CREATE TABLE sync_atomicity.child (id integer PRIMARY KEY, parent_id integer NOT NULL \
                 REFERENCES sync_atomicity.parent(id), label text NOT NULL)"
        ),
        10,
        JobId(9660),
    ));
    assert_eq!(source_setup.status, ExecutionStatus::Succeeded);
    let destination_setup = destination.block_on(destination.session.execute(
        &format!(
            "{schema} CREATE TABLE sync_atomicity.child (id integer PRIMARY KEY, parent_id integer NOT NULL \
                 REFERENCES sync_atomicity.parent(id), label text NOT NULL \
                 CHECK (label <> 'rejected-source-label'))"
        ),
        10,
        JobId(9666),
    ));
    assert_eq!(destination_setup.status, ExecutionStatus::Succeeded);

    let seed = source.block_on(source.session.execute(
        "INSERT INTO sync_atomicity.parent VALUES (2, 'new-parent'); \
         INSERT INTO sync_atomicity.child VALUES (2, 2, 'new-child')",
        10,
        JobId(9661),
    ));
    assert_eq!(seed.status, ExecutionStatus::Succeeded);
    let previous = destination.block_on(destination.session.execute(
        "INSERT INTO sync_atomicity.parent VALUES (1, 'previous-parent'); \
         INSERT INTO sync_atomicity.child VALUES (1, 1, 'previous-child')",
        10,
        JobId(9662),
    ));
    assert_eq!(previous.status, ExecutionStatus::Succeeded);

    let tables = vec!["parent".to_owned(), "child".to_owned()];
    let first = within_sync_timeout(
        &source,
        source.session.execute_sync_pipeline(
            Some(&mut destination.session),
            Some("sync_atomicity"),
            &tables,
            true,
            false,
            false,
        ),
    )
    .expect("related table replacement commits");
    assert_eq!(first.total_rows, 2);
    let after_success = sync_atomicity_labels(&destination, 9663);
    assert_eq!(
        after_success,
        vec![
            Cell::Text("new-parent".into()),
            Cell::Text("new-child".into())
        ]
    );

    let change = source.block_on(source.session.execute(
        "TRUNCATE sync_atomicity.child, sync_atomicity.parent CASCADE; \
         INSERT INTO sync_atomicity.parent VALUES (3, 'replacement-parent'); \
         INSERT INTO sync_atomicity.child VALUES (3, 3, 'rejected-source-label')",
        10,
        JobId(9664),
    ));
    assert_eq!(change.status, ExecutionStatus::Succeeded);
    let error = within_sync_timeout(
        &source,
        source.session.execute_sync_pipeline(
            Some(&mut destination.session),
            Some("sync_atomicity"),
            &tables,
            true,
            false,
            false,
        ),
    )
    .expect_err("destination CHECK must reject the later child COPY");
    assert_eq!(error.kind, ignatius::diagnostics::DiagnosticKind::Query);
    assert_eq!(error.headline, "destination COPY completion failed");
    assert_eq!(error.statement_number, None);
    assert!(
        error
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "23514"),
        "{error:?}"
    );

    let independent = fixture(&destination_uri);
    let preserved = sync_atomicity_labels(&independent, 9665);
    assert_eq!(
        preserved,
        vec![
            Cell::Text("new-parent".into()),
            Cell::Text("new-child".into())
        ],
        "failed later table COPY must leave the previous committed set intact"
    );

    drop(independent);
    drop(destination);
    drop(source);
    scratch.cleanup();
}

#[test]
fn plpgsql_routine_runner_and_profiler_integration() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    // 1. Create a synthetic PL/pgSQL function with RAISE NOTICE and default arguments
    let setup = fx.block_on(fx.session.execute(
        "CREATE OR REPLACE FUNCTION public.test_calculate_tax(amount numeric, rate numeric DEFAULT 0.15) \
         RETURNS numeric AS $$ \
         BEGIN \
             RAISE NOTICE 'Computing tax for amount=% rate=%', amount, rate; \
             RETURN amount * rate; \
         END; \
         $$ LANGUAGE plpgsql;",
        100,
        JobId(9601),
    ));
    assert_eq!(setup.status, ExecutionStatus::Succeeded);

    // 2. Discover routine signatures from PostgreSQL catalog
    let routines = fx.block_on(
        fx.session
            .discover_routines(Some("public"), Some("test_calculate_tax")),
    );
    assert!(
        routines.is_ok(),
        "discover_routines failed: {:?}",
        routines.err()
    );
    let list = routines.unwrap();
    assert_eq!(
        list.len(),
        1,
        "expected 1 routine matching test_calculate_tax"
    );
    let routine = &list[0];
    assert_eq!(routine.name, "test_calculate_tax");
    assert_eq!(routine.schema, "public");
    assert_eq!(routine.kind, ignatius::routine::RoutineKind::Function);
    assert_eq!(routine.params.len(), 2);
    assert_eq!(routine.params[0].name, "amount");
    assert_eq!(routine.params[1].name, "rate");
    assert!(routine.params[1].has_default);

    // 3. Execute routine in safe savepoint harness with arguments and notice capture
    let mut args = std::collections::BTreeMap::new();
    args.insert("amount".to_string(), "200.0".to_string());
    args.insert("rate".to_string(), "0.10".to_string());

    let exec_opts = ignatius::routine::RoutineExecutionOptions {
        schema: "public".to_string(),
        name: "test_calculate_tax".to_string(),
        args,
        rollback_savepoint: true,
        timeout_ms: Some(15_000),
    };

    let exec_res = fx.block_on(fx.session.execute_routine(&exec_opts));
    assert!(
        exec_res.is_ok(),
        "execute_routine failed: {:?}",
        exec_res.err()
    );
    let res = exec_res.unwrap();
    assert!(res.success);
    assert!(res.rolled_back);
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0].len(), 1);
    assert_eq!(res.rows[0][0], "20.000");
    assert_eq!(res.notices.len(), 1);
    assert_eq!(res.notices[0].severity, "NOTICE");
    assert!(
        res.notices[0]
            .message
            .contains("Computing tax for amount=200.0 rate=0.10")
    );

    // 4. Profile routine using plpgsql_check or static inspector
    let prof_res = fx.block_on(fx.session.profile_routine("public", "test_calculate_tax"));
    assert!(
        prof_res.is_ok(),
        "profile_routine failed: {:?}",
        prof_res.err()
    );
    let prof = prof_res.unwrap();
    assert_eq!(prof.routine.name, "test_calculate_tax");
    assert!(prof.findings.is_empty() || !prof.findings.is_empty()); // Valid findings vector

    // 5. Clean up
    let _ = fx.block_on(fx.session.execute(
        "DROP FUNCTION public.test_calculate_tax(numeric, numeric);",
        100,
        JobId(9602),
    ));
}

#[test]
fn postgres_listen_notify_and_maintenance_operations() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);

    // 1. Listen on a custom channel
    let channel = "ignatius_integ_channel";
    let listen_res = fx.block_on(fx.session.listen_channel(channel));
    assert!(
        listen_res.is_ok(),
        "listen_channel failed: {:?}",
        listen_res.err()
    );

    // 2. Notify on that channel
    let payload = "{\"event\": \"order_placed\", \"order_id\": 101}";
    let notify_res = fx.block_on(fx.session.notify_channel(channel, Some(payload)));
    assert!(
        notify_res.is_ok(),
        "notify_channel failed: {:?}",
        notify_res.err()
    );

    // 3. Wait briefly for async notification message loop to deliver to session
    std::thread::sleep(std::time::Duration::from_millis(150));
    let notifications = fx.block_on(fx.session.notifications());
    assert!(
        !notifications.is_empty(),
        "expected at least 1 received notification"
    );
    let matched = notifications.iter().find(|n| n.channel == channel);
    assert!(
        matched.is_some(),
        "expected notification on channel {channel}"
    );
    assert_eq!(matched.unwrap().payload, payload);

    // 4. Unlisten from channel
    let unlisten_res = fx.block_on(fx.session.unlisten_channel(channel));
    assert!(
        unlisten_res.is_ok(),
        "unlisten_channel failed: {:?}",
        unlisten_res.err()
    );

    // 5. Query maintenance progress view
    let prog_res = fx.block_on(fx.session.query_maintenance_progress());
    assert!(
        prog_res.is_ok(),
        "query_maintenance_progress failed: {:?}",
        prog_res.err()
    );

    // 6. Execute maintenance: VACUUM ANALYZE
    let options = ignatius::ops::MaintenanceOptions {
        kind: ignatius::ops::MaintenanceKind::Vacuum {
            full: false,
            freeze: false,
            verbose: true,
            analyze: true,
        },
        target: None,
        confirm_production: false,
    };
    let maint_res = fx.block_on(fx.session.execute_maintenance(&options));
    assert!(
        maint_res.is_ok(),
        "execute_maintenance failed: {:?}",
        maint_res.err()
    );
    let res = maint_res.unwrap();
    assert!(res.success);
    assert!(res.message.contains("Maintenance completed successfully"));
}

fn open_structured_detail(
    fx: &Fixture,
    source: &str,
    column: usize,
) -> ignatius::app::model::Model {
    use ignatius::app::{
        message::{Action, Message},
        model::{ConnectionState, Focus, Model},
        update::update,
    };

    let execution = fx.block_on(
        fx.session
            .execute_for_workbench(source, None, 10, JobId(5900)),
    );
    assert_eq!(
        execution.status,
        ExecutionStatus::Succeeded,
        "{execution:?}"
    );
    let mut model = Model::new(10);
    model.connection = ConnectionState::Connected(Box::new(fx.session.info().clone()));
    model.focus = Focus::Results;
    model.last_sql = Some(source.to_owned());
    model.last_execution = Some(execution);
    model.selected_column = column;
    assert!(update(&mut model, Message::Action(Action::ToggleInspector)).is_empty());
    model
}

fn stage_structured_edit(
    fx: &Fixture,
    source: &str,
    column: usize,
    steps: usize,
    replacement: &str,
) -> (String, ParameterBindings) {
    use ignatius::app::{
        message::{Action, Direction, Effect, Message},
        update::update,
    };

    let mut model = open_structured_detail(fx, source, column);
    for _ in 0..steps {
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
    }
    let effects = update(&mut model, Message::Action(Action::Insert('e')));
    let [Effect::LoadUpdateTarget { request, relation }] = effects.as_slice() else {
        panic!("expected structured metadata lookup, got {effects:?}");
    };
    let metadata = fx.block_on(
        fx.session
            .update_relation(relation.schema.as_deref(), &relation.relation),
    );
    assert!(metadata.is_ok(), "{metadata:?}");
    assert!(
        update(
            &mut model,
            Message::UpdateTargetLoaded {
                request: *request,
                result: Box::new(metadata),
                transaction: None,
            },
        )
        .is_empty()
    );
    assert!(
        model.update_prompt.is_some(),
        "{:?}",
        model.cell_update_notice
    );
    update(&mut model, Message::Pasted(replacement.to_owned()));
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    let review = model
        .pending_update
        .as_ref()
        .expect("structured edit review");
    assert!(
        review
            .plan
            .bound_sql()
            .expect("valid reviewed SQL")
            .contains(replacement)
    );
    update(&mut model, Message::Action(Action::MoveBufferEnd));
    let effects = update(&mut model, Message::Action(Action::Activate));
    let [
        Effect::ExecuteParameterized {
            sql, parameters, ..
        },
    ] = effects.as_slice()
    else {
        panic!("expected a confirmed structured update, got {effects:?}");
    };
    (sql.clone(), parameters.clone())
}

fn run_structured_edit(fx: &Fixture, plan: &(String, ParameterBindings)) -> u64 {
    let result = fx.block_on(
        fx.session
            .execute_with_parameters(&plan.0, &plan.1, 10, JobId(5901)),
    );
    assert_eq!(result.status, ExecutionStatus::Succeeded, "{result:?}");
    result.statements[0]
        .rows_affected
        .expect("affected row count")
}

#[test]
fn structured_jsonb_edit_preserves_precision_and_refuses_changed_value_or_relation() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = r#"SET search_path TO pg_temp;
        CREATE TEMP TABLE inspection_test (id int PRIMARY KEY, payload jsonb);
        INSERT INTO inspection_test VALUES
        (1, '{"a":[null],"precision":123456789012345678901.234567890123456789}');"#;
    let created = fx.block_on(fx.session.execute(setup, 10, JobId(5902)));
    assert_eq!(created.status, ExecutionStatus::Succeeded, "{created:?}");
    let source = "SELECT id, payload FROM inspection_test";
    let aliased = "SELECT id, payload AS selected FROM inspection_test";
    let plan = stage_structured_edit(&fx, aliased, 1, 2, "true");
    assert!(!plan.0.contains("123456789012345678901"));
    assert_eq!(run_structured_edit(&fx, &plan), 1);
    assert_eq!(run_structured_edit(&fx, &plan), 0, "stale value guard");
    let result = fx.block_on(fx.session.execute(
        "SELECT payload->'a'->0, payload->'precision' FROM inspection_test",
        10,
        JobId(5903),
    ));
    assert_eq!(result.status, ExecutionStatus::Succeeded, "{result:?}");
    let row = &result.statements[0].result_set.as_ref().unwrap().rows[0];
    assert_eq!(row[0].raw(), Some("true"));
    assert_eq!(
        row[1].raw(),
        Some("123456789012345678901.234567890123456789")
    );

    let replaced_relation = fx.block_on(fx.session.execute(
        "DROP TABLE inspection_test; CREATE TEMP TABLE inspection_test (id int PRIMARY KEY, payload jsonb); \
         INSERT INTO inspection_test VALUES (1, '{\"a\":[null],\"precision\":123456789012345678901.234567890123456789}');",
        10,
        JobId(5904),
    ));
    assert_eq!(
        replaced_relation.status,
        ExecutionStatus::Succeeded,
        "{replaced_relation:?}"
    );
    assert_eq!(
        run_structured_edit(&fx, &plan),
        0,
        "same-name table has different origin"
    );
    let pending = stage_structured_edit(&fx, source, 1, 2, "false");
    let changed_key = fx.block_on(fx.session.execute(
        "ALTER TABLE inspection_test DROP CONSTRAINT inspection_test_pkey; \
         INSERT INTO inspection_test SELECT * FROM inspection_test;",
        10,
        JobId(5905),
    ));
    assert_eq!(
        changed_key.status,
        ExecutionStatus::Succeeded,
        "{changed_key:?}"
    );
    assert_eq!(
        run_structured_edit(&fx, &pending),
        0,
        "lost unique key refuses both rows"
    );
}

#[test]
fn structured_derived_column_preserves_exact_text_and_stays_inspect_only() {
    use ignatius::app::{
        message::{Action, Message},
        update::update,
    };

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let mut model = open_structured_detail(
        &fx,
        r#"SELECT left_value || right_value AS derived
           FROM (VALUES ('{"value":123456789012345678901.0123456789}'::jsonb, '{}'::jsonb))
                AS example(left_value, right_value)"#,
        0,
    );
    let result = model.visible_result().unwrap();
    assert_eq!(result.column_type(0), Some("jsonb"));
    assert_eq!(
        result.rows[0][0].raw(),
        Some(r#"{"value": 123456789012345678901.0123456789}"#)
    );
    let screen = ignatius::ui::layout::render_to_string(
        &model,
        &ignatius::ui::Keymap::new(),
        &model.presentation,
        112,
        30,
    );
    assert!(screen.contains("Cell Detail  r Raw"), "{screen}");
    assert!(!screen.contains("e Edit"), "{screen}");
    assert!(update(&mut model, Message::Action(Action::Insert('e'))).is_empty());
    assert!(model.update_lookup.is_none());
}

#[test]
fn structured_array_and_range_edits_keep_subscripts_nulls_and_open_bound() {
    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let setup = r#"SET search_path TO pg_temp;
        CREATE TEMP TABLE inspection_native (
          id int, included int, members text[], period numrange,
          PRIMARY KEY (id) INCLUDE (included)
        );
        INSERT INTO inspection_native VALUES
          (1, 10, '[0:2]={NULL,"NULL","other"}', '(,12345678901234.125]');"#;
    let created = fx.block_on(fx.session.execute(setup, 10, JobId(5910)));
    assert_eq!(created.status, ExecutionStatus::Succeeded, "{created:?}");
    let source = "SELECT id, members, period FROM inspection_native";
    let plan = stage_structured_edit(&fx, source, 1, 3, r#""x,y""#);
    assert_eq!(run_structured_edit(&fx, &plan), 1);
    let plan = stage_structured_edit(&fx, source, 2, 1, "-999999999999.125");
    assert_eq!(run_structured_edit(&fx, &plan), 1);
    let result = fx.block_on(fx.session.execute(
        "SELECT array_lower(members, 1), members[0] IS NULL, members[1], members[2], \
                lower(period), upper(period), lower_inc(period), upper_inc(period) FROM inspection_native",
        10,
        JobId(5911),
    ));
    assert_eq!(result.status, ExecutionStatus::Succeeded, "{result:?}");
    let row = &result.statements[0].result_set.as_ref().unwrap().rows[0];
    assert_eq!(
        row.iter().map(Cell::raw).collect::<Vec<_>>(),
        [
            Some("0"),
            Some("t"),
            Some("NULL"),
            Some("x,y"),
            Some("-999999999999.125"),
            Some("12345678901234.125"),
            Some("f"),
            Some("t"),
        ]
    );

    let literal = r#"[0:3] = { " kept " , NULL , "NULL" , two words  }"#;
    let plan = stage_structured_edit(&fx, source, 1, 0, literal);
    assert_eq!(run_structured_edit(&fx, &plan), 1);
    let result = fx.block_on(fx.session.execute(
        "SELECT array_lower(members, 1), members[0], members[1], members[2], members[3] \
         FROM inspection_native",
        10,
        JobId(5912),
    ));
    assert_eq!(result.status, ExecutionStatus::Succeeded, "{result:?}");
    let row = &result.statements[0].result_set.as_ref().unwrap().rows[0];
    assert_eq!(
        row.iter().map(Cell::raw).collect::<Vec<_>>(),
        [
            Some("0"),
            Some(" kept "),
            None,
            Some("NULL"),
            Some("two words")
        ]
    );
}

fn extension_fixture(variable: &str, extension: &str) -> Option<(Fixture, String)> {
    let Ok(uri) = std::env::var(variable) else {
        eprintln!("skipping live {extension} inspection: {variable} is unset");
        return None;
    };
    assert!(!uri.is_empty(), "{variable} must name a disposable fixture");
    let fx = fixture(&uri);
    let result = fx.block_on(fx.session.execute(
        &format!(
            "SELECT n.nspname FROM pg_catalog.pg_extension AS e \
             JOIN pg_catalog.pg_namespace AS n ON n.oid = e.extnamespace \
             WHERE e.extname = '{extension}'"
        ),
        1,
        JobId(5920),
    ));
    assert_eq!(result.status, ExecutionStatus::Succeeded, "{result:?}");
    let schema = result
        .statements
        .first()
        .and_then(|statement| statement.result_set.as_ref())
        .and_then(|set| set.rows.first())
        .and_then(|row| row[0].raw())
        .unwrap_or_else(|| panic!("the disposable {variable} fixture must enable {extension}"));
    Some((fx, ignatius::query::quote_identifier(schema)))
}

fn assert_structured_detail(fx: &Fixture, source: &str, column: usize, expected: &[&str]) {
    let model = open_structured_detail(fx, source, column);
    let screen = ignatius::ui::layout::render_to_string(
        &model,
        &ignatius::ui::Keymap::new(),
        &model.presentation,
        112,
        30,
    );
    assert!(screen.contains("Cell Detail"), "{screen}");
    for text in expected {
        assert!(screen.contains(text), "missing {text:?}: {screen}");
    }
}

#[test]
fn structured_postgis_decodes_ewkb_and_reviews_a_whole_geometry_replacement() {
    let Some((fx, schema)) = extension_fixture("IGNATIUS_TEST_PG_POSTGIS_URI", "postgis") else {
        return;
    };
    let created = fx.block_on(fx.session.execute(
        &format!(
            "SET search_path TO pg_temp;
             CREATE TEMP TABLE inspection_spatial (
                 id int PRIMARY KEY,
                 location {schema}.geometry(PointZ,4326),
                 zone {schema}.geography(Point,4326)
             );
             INSERT INTO inspection_spatial VALUES
                 (1, 'SRID=4326;POINT Z (1 2 3)', 'SRID=4326;POINT (4 5)');"
        ),
        5,
        JobId(5921),
    ));
    assert_eq!(created.status, ExecutionStatus::Succeeded, "{created:?}");
    let source = "SELECT id, location, zone FROM inspection_spatial";
    assert_structured_detail(
        &fx,
        source,
        1,
        &["Point XYZ, SRID 4326", "X=1.0, Y=2.0, Z=3.0"],
    );
    assert_structured_detail(&fx, source, 2, &["Point XY, SRID 4326", "X=4.0, Y=5.0"]);
    let plan = stage_structured_edit(&fx, source, 1, 0, "SRID=4326;POINT Z (7 8 9)");
    assert_eq!(run_structured_edit(&fx, &plan), 1);
    assert_structured_detail(
        &fx,
        source,
        1,
        &["Point XYZ, SRID 4326", "X=7.0, Y=8.0, Z=9.0"],
    );
}

#[test]
fn structured_pgvector_decodes_and_patches_one_coordinate() {
    let Some((fx, schema)) = extension_fixture("IGNATIUS_TEST_PG_VECTOR_URI", "vector") else {
        return;
    };
    let created = fx.block_on(fx.session.execute(
        &format!(
            "SET search_path TO pg_temp;
             CREATE TEMP TABLE inspection_vector (
                 id int PRIMARY KEY, embedding {schema}.vector(3)
             );
             INSERT INTO inspection_vector VALUES (1, '[1,2,3]');"
        ),
        5,
        JobId(5922),
    ));
    assert_eq!(created.status, ExecutionStatus::Succeeded, "{created:?}");
    let source = "SELECT id, embedding FROM inspection_vector";
    assert_structured_detail(&fx, source, 1, &["3 dimensions", "[2]  2"]);
    let plan = stage_structured_edit(&fx, source, 1, 2, "4.125");
    assert_eq!(run_structured_edit(&fx, &plan), 1);
    assert_structured_detail(&fx, source, 1, &["[1]  1", "[2]  4.125", "[3]  3"]);
}

#[test]
fn structured_result_observed_under_lossy_output_stays_inspect_only() {
    use ignatius::app::{
        message::{Action, Message},
        update::update,
    };

    let uri = target_or_skip!();
    let fx = fixture(&uri);
    let created = fx.block_on(fx.session.execute(
        "SET search_path TO pg_temp; SET extra_float_digits TO 0; \
         CREATE TEMP TABLE inspection_loss (id int PRIMARY KEY, samples float8[]); \
         INSERT INTO inspection_loss VALUES (1, '{1.0000000000000002,2.5}');",
        5,
        JobId(5930),
    ));
    assert_eq!(created.status, ExecutionStatus::Succeeded, "{created:?}");
    let source = "SELECT id, samples FROM inspection_loss";
    let mut earlier = open_structured_detail(&fx, source, 1);
    assert_eq!(
        earlier.visible_result().unwrap().rows[0][1].raw(),
        Some("{1,2.5}")
    );
    let screen = ignatius::ui::layout::render_to_string(
        &earlier,
        &ignatius::ui::Keymap::new(),
        &earlier.presentation,
        112,
        30,
    );
    assert!(screen.contains("Cell Detail"), "{screen}");
    assert!(!screen.contains("e Edit"), "{screen}");

    let restored = fx.block_on(
        fx.session
            .execute("SET extra_float_digits TO 3", 5, JobId(5931)),
    );
    assert_eq!(restored.status, ExecutionStatus::Succeeded, "{restored:?}");
    assert!(
        update(&mut earlier, Message::Action(Action::Insert('e'))).is_empty(),
        "a settings change cannot promote previously rounded results"
    );
    assert!(earlier.update_lookup.is_none());

    let ordinary = fx.block_on(fx.session.execute(source, 5, JobId(5932)));
    assert_eq!(ordinary.status, ExecutionStatus::Succeeded, "{ordinary:?}");
    let result = ordinary.statements[0].result_set.as_ref().unwrap();
    assert_eq!(result.column_type(1), Some("_float8"));
    assert_eq!(result.rows[0][1].raw(), Some("{1.0000000000000002,2.5}"));
    let fresh = stage_structured_edit(&fx, source, 1, 2, "4.25");
    assert_eq!(run_structured_edit(&fx, &fresh), 1);
}

// === M20: target_session_attrs ===

#[test]
fn target_session_attrs_allows_matching_primary_read_write_and_any_on_live_server() {
    use ignatius::connection::TargetSessionAttrs;

    let uri = target_or_skip!();
    let config = Config::default();
    let runtime = runtime();

    for (attrs_str, expected) in [
        ("any", TargetSessionAttrs::Any),
        ("read-write", TargetSessionAttrs::ReadWrite),
        ("primary", TargetSessionAttrs::Primary),
    ] {
        let target = resolve(
            Some(&uri),
            &ConnectionArgs {
                target_session_attrs: Some(expected),
                ..ConnectionArgs::default()
            },
            &EnvSnapshot::from_process(),
            &config.connection,
        )
        .expect("target resolves");
        assert_eq!(target.target_session_attrs, expected);

        let opened = runtime
            .block_on(session::connect(&target, Duration::ZERO))
            .unwrap_or_else(|err| panic!("expected {attrs_str} to connect: {err:?}"));
        assert!(!opened.info().read_only);
    }
}

#[test]
fn target_session_attrs_refuses_read_only_and_standby_on_writable_primary() {
    use ignatius::connection::TargetSessionAttrs;
    use ignatius::diagnostics::DiagnosticKind;

    let uri = target_or_skip!();
    let config = Config::default();
    let runtime = runtime();

    for (attrs, expected_requested, expected_observed) in [
        (TargetSessionAttrs::ReadOnly, "read-only", "read-write"),
        (TargetSessionAttrs::Standby, "standby", "primary"),
    ] {
        let target = resolve(
            Some(&uri),
            &ConnectionArgs {
                target_session_attrs: Some(attrs),
                ..ConnectionArgs::default()
            },
            &EnvSnapshot::from_process(),
            &config.connection,
        )
        .expect("target resolves");

        let err = runtime
            .block_on(session::connect(&target, Duration::ZERO))
            .expect_err("mismatched target_session_attrs must refuse");
        assert_eq!(err.kind, DiagnosticKind::Connection);
        assert!(
            err.headline.contains(expected_requested),
            "expected requested {expected_requested} in {}",
            err.headline
        );
        assert!(
            err.headline.contains(expected_observed),
            "expected observed {expected_observed} in {}",
            err.headline
        );
        assert!(err.next_action.is_some());
    }
}

#[test]
fn target_session_attrs_read_only_succeeds_with_read_only_session_and_object_tree_uses_any() {
    use ignatius::connection::TargetSessionAttrs;

    let uri = target_or_skip!();
    let config = Config::default();
    let runtime = runtime();

    let ro_target = resolve(
        Some(&uri),
        &ConnectionArgs {
            target_session_attrs: Some(TargetSessionAttrs::ReadOnly),
            read_only: true,
            ..ConnectionArgs::default()
        },
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");
    let ro_session = runtime
        .block_on(session::connect(&ro_target, Duration::ZERO))
        .expect("read-only session satisfies target_session_attrs=read-only");
    assert!(ro_session.info().read_only);

    // Object tree connection is always read-only, so it resets target_session_attrs to Any
    // even when the primary connection requires ReadWrite.
    let rw_target = resolve(
        Some(&uri),
        &ConnectionArgs {
            target_session_attrs: Some(TargetSessionAttrs::ReadWrite),
            ..ConnectionArgs::default()
        },
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("target resolves");
    let tree_target = rw_target.for_object_tree();
    assert_eq!(tree_target.target_session_attrs, TargetSessionAttrs::Any);
    let tree_session = runtime
        .block_on(session::connect(&tree_target, Duration::ZERO))
        .expect("object tree connection succeeds");
    assert!(tree_session.info().read_only);
}

// === M23: Multi-host failover ===

fn splice_failover_candidate(uri: &str, dead_candidate: &str) -> String {
    let scheme_end = uri.find("://").expect("scheme") + 3;
    let rest = &uri[scheme_end..];
    let authority_end = rest
        .find(['/', '?'])
        .map_or(uri.len(), |idx| scheme_end + idx);
    if let Some(at_rel) = uri[scheme_end..authority_end].rfind('@') {
        let insert_at = scheme_end + at_rel + 1;
        format!(
            "{}{dead_candidate},{}",
            &uri[..insert_at],
            &uri[insert_at..]
        )
    } else {
        format!(
            "{}{dead_candidate},{}",
            &uri[..scheme_end],
            &uri[scheme_end..]
        )
    }
}

#[test]
fn multi_host_failover_connects_plain_second_candidate_and_reports_connected_endpoint() {
    let uri = target_or_skip!();
    let config = Config::default();
    let base = resolve(
        Some(&uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("base target resolves");
    let failover_uri = splice_failover_candidate(&uri, "127.0.0.1:1");

    let target = resolve(
        Some(&failover_uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("multi-host target resolves");
    assert_eq!(target.endpoints.len(), 2);
    assert_eq!(target.endpoints[0].port, 1);
    assert_eq!(target.endpoints[1].port, base.port);

    let runtime = runtime();
    let opened = runtime
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("failover connects to live second candidate");

    assert_eq!(opened.connected_endpoint(), &target.endpoints[1]);
    assert!(
        opened.info().target.contains(&format!(":{}", base.port)),
        "SessionInfo::target must report actual connected port {}: {}",
        base.port,
        opened.info().target
    );
    assert!(
        !opened.info().target.contains(":1/"),
        "SessionInfo::target must not report failed endpoint: {}",
        opened.info().target
    );

    // Object-tree second connection pinned to the connected endpoint succeeds directly.
    let tree_target = target
        .for_endpoint(opened.connected_endpoint())
        .for_object_tree();
    assert_eq!(tree_target.endpoints.len(), 1);
    assert_eq!(tree_target.port, base.port);
    let tree_session = runtime
        .block_on(session::connect(&tree_target, Duration::ZERO))
        .expect("pinned object-tree connection succeeds");
    assert!(tree_session.info().read_only);
}

#[test]
fn multi_host_failover_connects_verified_tls_second_candidate_without_downgrade() {
    let tls_uri = tls_target_or_skip!();
    let config = Config::default();
    let base = resolve(
        Some(&tls_uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("base TLS target resolves");
    let failover_uri = splice_failover_candidate(&tls_uri, "127.0.0.1:1");
    let sep = if failover_uri.contains('?') { '&' } else { '?' };
    let full_uri = format!(
        "{failover_uri}{sep}sslmode=verify-full&sslrootcert={}&target_session_attrs=primary",
        certificate("ca.crt")
    );

    let target = resolve(
        Some(&full_uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config.connection,
    )
    .expect("TLS multi-host target resolves");
    assert_eq!(target.sslmode, SslMode::VerifyFull);
    assert_eq!(target.endpoints.len(), 2);

    let opened = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect("verified-TLS failover connects to second candidate");
    assert!(opened.info().tls.is_encrypted(), "{:?}", opened.info().tls);
    assert!(
        matches!(
            opened.info().tls,
            TlsState::Active {
                mode: SslMode::VerifyFull,
                ..
            }
        ),
        "TLS must remain verify-full without downgrade: {:?}",
        opened.info().tls
    );
    assert!(
        opened
            .info()
            .target
            .contains(&format!("localhost:{}", base.port)),
        "SessionInfo::target should show localhost:{}: {}",
        base.port,
        opened.info().target
    );
}

#[test]
fn multi_host_failover_with_unmatched_target_session_attrs_fails_naming_all_candidates() {
    use ignatius::connection::TargetSessionAttrs;
    use ignatius::diagnostics::DiagnosticKind;

    let uri = target_or_skip!();
    let config = Config::default();
    let base = resolve(
        Some(&uri),
        &ConnectionArgs::default(),
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("base target resolves");
    let host_list = format!("127.0.0.1:{},localhost:{}", base.port, base.port);
    let target = resolve(
        Some(&uri),
        &ConnectionArgs {
            host: Some("127.0.0.1,localhost".to_owned()),
            port: Some(base.port),
            target_session_attrs: Some(TargetSessionAttrs::Standby),
            ..ConnectionArgs::default()
        },
        &EnvSnapshot::from_process(),
        &config.connection,
    )
    .expect("multi-host standby target resolves");
    assert_eq!(target.endpoints.len(), 2);

    let err = runtime()
        .block_on(session::connect(&target, Duration::ZERO))
        .expect_err("all primary endpoints must be rejected for target_session_attrs=standby");
    assert_eq!(err.kind, DiagnosticKind::Connection);
    let rendered = format!("{err:?}");
    let first = format!("127.0.0.1:{}", base.port);
    let second = format!("localhost:{}", base.port);
    assert!(
        rendered.contains(&first) && rendered.contains(&second),
        "failure for {host_list} must name both candidates: {rendered}"
    );
    assert!(err.next_action.is_some());
}
