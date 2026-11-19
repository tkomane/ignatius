//! Read-only measurement against a single PostgreSQL snapshot.

use crate::bench::{BenchConfig, DEFAULT_MEASURED_RUNS, DEFAULT_WARMUP_RUNS};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::plan_history::Observation;
use chrono::Utc;
use std::time::Duration;
use tokio_postgres::Client;

use super::error::{from_query_error, safe_sqlstate};
use super::{Session, bench, plandiff, snapshot};

pub(crate) async fn capture(
    session: Session,
    schema: &str,
    sql: &str,
    measure: bool,
) -> Result<Observation, Diagnostic> {
    tokio::time::timeout(
        Duration::from_secs(60),
        run_capture(session.client(), schema, sql, measure),
    )
    .await
    .map_err(|_| {
        Diagnostic::new(
            DiagnosticKind::Cancelled,
            "the capture exceeded its 60 second deadline",
            "capturing plan history",
        )
        .next_action("reduce the query or schema scope and rerun capture")
    })?
}

async fn run_capture(
    client: &Client,
    schema: &str,
    sql: &str,
    measure: bool,
) -> Result<Observation, Diagnostic> {
    client
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .map_err(private_driver_error)?;
    let result = observe(client, schema, sql, measure).await;
    let rollback = client.batch_execute("ROLLBACK").await;
    match (result, rollback) {
        (Ok(observation), Ok(())) => Ok(observation),
        (Ok(_), Err(error)) => Err(private_driver_error(error).likely_cause(
            "capture finished but the read-only transaction did not roll back cleanly",
        )),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(_)) => Err(error.technical("Cleanup", "ROLLBACK failed")),
    }
}

async fn observe(
    client: &Client,
    schema: &str,
    sql: &str,
    measure: bool,
) -> Result<Observation, Diagnostic> {
    client
        .batch_execute(
            "SET LOCAL statement_timeout = '10s'; SET LOCAL standard_conforming_strings = on",
        )
        .await
        .map_err(private_driver_error)?;
    let row = client
        .query_one(
            "SELECT current_setting('server_version_num')::int4, \
             EXISTS (SELECT FROM pg_catalog.pg_namespace WHERE nspname = $1)",
            &[&schema],
        )
        .await
        .map_err(private_driver_error)?;
    let server_version = row.try_get(0).map_err(private_driver_error)?;
    if !row.try_get::<_, bool>(1).map_err(private_driver_error)? {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the requested schema does not exist on this connection",
            "resolving the plan history schema scope",
        ));
    }

    // PostgreSQL extended Parse rejects multiple statements. Use the same
    // string interpretation for it and the subsequent simple-query collectors.
    let prepared = client.prepare(sql).await.map_err(private_driver_error)?;
    if !prepared.params().is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the statement requires bind parameters",
            "validating plan history SQL",
        )
        .next_action("use a standalone read query"));
    }
    let schema = snapshot::capture_schema_snapshot(client, Some(schema))
        .await
        .map_err(private_capture_error)?;
    if schema.truncated {
        return Err(Diagnostic::new(
            DiagnosticKind::Query,
            "the schema snapshot exceeded its catalog bounds",
            "capturing plan history",
        )
        .next_action("reduce the schema scope before comparing revisions"));
    }
    let plan = plandiff::explain_in_transaction(client, sql, measure, "Plan history")
        .await
        .map_err(private_capture_error)?;
    let measured = if measure {
        // Reuse the existing sampler without starting an inner transaction.
        let sampling = BenchConfig::new(DEFAULT_WARMUP_RUNS, DEFAULT_MEASURED_RUNS, false);
        Some(
            bench::run_benchmark(client, sql, &sampling, "Plan history")
                .await
                .map_err(private_capture_error)?,
        )
    } else {
        None
    };
    Observation::from_capture(
        server_version,
        schema.fingerprint_sha256,
        &plan,
        measured.as_ref(),
        Utc::now().timestamp(),
    )
}

fn private_driver_error(error: tokio_postgres::Error) -> Diagnostic {
    private_capture_error(from_query_error(&error, 1))
}

fn private_capture_error(error: Diagnostic) -> Diagnostic {
    let state = error
        .technical
        .iter()
        .find(|field| field.label == "SQLSTATE")
        .and_then(|field| safe_sqlstate(&field.value));
    Diagnostic::new(
        error.kind,
        "PostgreSQL did not complete the observation",
        "capturing plan history",
    )
    .technical_opt("SQLSTATE", state)
    .next_action("check the read query and schema scope; no observation was saved")
}
