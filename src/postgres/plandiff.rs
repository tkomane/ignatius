//! Rollback-safe PostgreSQL runner for extended `EXPLAIN (BUFFERS, WAL, SETTINGS)`
//! plan capture (Feature `054-explain-buffers-wal-and-plan-diff`).
//!
//! Estimate mode (`analyze == false`) runs `EXPLAIN (FORMAT JSON, VERBOSE, SETTINGS)`
//! without `ANALYZE`-only flags so PostgreSQL 14 through 18 execute no query
//! work. Analyzed mode (`analyze == true`) wraps
//! `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT JSON)` inside `BEGIN` and
//! unconditionally issues `ROLLBACK` afterwards—even on SQL error—so DML
//! targets never persist mutations.

use futures_util::StreamExt;
use tokio_postgres::types::{FromSql, Type};
use tokio_postgres::{Client, SimpleQueryMessage};

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::plandiff::{PlanSnapshot, parse_extended_explain_json};
use crate::postgres::error::from_query_error;
use crate::query::plan::MAX_PLAN_JSON_BYTES;
use crate::query::statements;

const ESTIMATE_OPTIONS: &str = "FORMAT JSON, VERBOSE, SETTINGS";
const ANALYZE_OPTIONS: &str = "ANALYZE, BUFFERS, WAL, SETTINGS, FORMAT JSON";

/// Executes an extended PostgreSQL `EXPLAIN` for one SQL statement and parses
/// the bounded [`PlanSnapshot`].
///
/// When `analyze` is `true`, execution is bracketed by `BEGIN` and an
/// unconditional `ROLLBACK` so side effects from analyzed DML statements are
/// reverted even when `EXPLAIN` raises a SQL error.
pub async fn explain_extended(
    client: &Client,
    sql: &str,
    analyze: bool,
    label: &str,
) -> Result<PlanSnapshot, Diagnostic> {
    let stmt_text = validate_single_statement(sql)?;
    if analyze {
        client
            .simple_query("BEGIN")
            .await
            .map_err(|err| from_query_error(&err, 1))?;

        let explain_res = explain_in_transaction(client, &stmt_text, analyze, label).await;
        let rollback_res = client.simple_query("ROLLBACK").await;

        match (explain_res, rollback_res) {
            (Ok(plan), Ok(_)) => Ok(plan),
            (Ok(_), Err(error)) => Err(from_query_error(&error, 1)
                .likely_cause("EXPLAIN ANALYZE finished, but the cleanup ROLLBACK failed")),
            (Err(error), Ok(_)) => Err(error),
            (Err(error), Err(rollback)) => {
                Err(error.technical("RollbackError", rollback.to_string()))
            }
        }
    } else {
        explain_in_transaction(client, &stmt_text, analyze, label).await
    }
}

/// Explain a validated statement. The caller owns any surrounding transaction.
pub(super) async fn explain_in_transaction(
    client: &Client,
    stmt_text: &str,
    analyze: bool,
    label: &str,
) -> Result<PlanSnapshot, Diagnostic> {
    let explain_sql = explain_sql(stmt_text, analyze);
    let json_text = run_explain_raw(client, &explain_sql).await?;
    parse_extended_explain_json(label, &json_text)
}

/// Plan one extended-protocol statement and require one JSON column in one row.
pub(super) async fn explain_single(
    client: &Client,
    stmt_text: &str,
    analyze: bool,
    label: &str,
) -> Result<PlanSnapshot, Diagnostic> {
    let row = client
        .query_one(&explain_sql(stmt_text, analyze), &[])
        .await
        .map_err(|error| from_query_error(&error, 1))?;
    if row.len() != 1 {
        return Err(invalid_plan_response());
    }
    let JsonPlan(bytes) = row.try_get(0).map_err(|_| invalid_plan_response())?;
    if bytes.len() > MAX_PLAN_JSON_BYTES {
        return Err(plan_too_large());
    }
    let json = std::str::from_utf8(bytes).map_err(|_| invalid_plan_response())?;
    parse_extended_explain_json(label, json)
}

/// PostgreSQL's binary JSON format is UTF-8 text. Borrow it until size admission.
struct JsonPlan<'a>(&'a [u8]);

impl<'a> FromSql<'a> for JsonPlan<'a> {
    fn from_sql(
        _: &Type,
        bytes: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(bytes))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::JSON
    }
}

fn explain_sql(statement: &str, analyze: bool) -> String {
    let options = if analyze {
        ANALYZE_OPTIONS
    } else {
        ESTIMATE_OPTIONS
    };
    format!("EXPLAIN ({options}) {statement}")
}

fn invalid_plan_response() -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Query,
        "PostgreSQL returned an invalid JSON plan row",
        "reading an extended structured query plan",
    )
}

fn plan_too_large() -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Query,
        "the query plan exceeded the client display bound",
        "reading an extended structured query plan",
    )
    .likely_cause(format!(
        "the JSON response is larger than {MAX_PLAN_JSON_BYTES} bytes"
    ))
    .next_action("simplify the statement or inspect its plan with PostgreSQL tools")
    .technical("PlanParseError", "ResponseTooLarge")
}

fn validate_single_statement(sql: &str) -> Result<String, Diagnostic> {
    let parsed = statements::split(sql);
    if parsed.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "no SQL statement to explain",
            "validating statement for extended EXPLAIN",
        )
        .next_action("provide a single non-empty SQL statement"));
    }
    if parsed.len() > 1 {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "expected 1 SQL statement for extended EXPLAIN, found {}",
                parsed.len()
            ),
            "validating statement for extended EXPLAIN",
        )
        .likely_cause("EXPLAIN can only inspect one statement at a time")
        .next_action("pass a single SQL statement without extra trailing statements"));
    }
    Ok(parsed[0].text.clone())
}

async fn run_explain_raw(client: &Client, explain_sql: &str) -> Result<String, Diagnostic> {
    let stream = client
        .simple_query_raw(explain_sql)
        .await
        .map_err(|err| from_query_error(&err, 1))?;
    tokio::pin!(stream);

    let mut payload: Option<String> = None;
    while let Some(message) = stream.next().await {
        let message = message.map_err(|err| from_query_error(&err, 1))?;
        if let SimpleQueryMessage::Row(row) = message {
            if payload.is_some() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Query,
                    "PostgreSQL returned more than one plan row",
                    "reading an extended structured query plan",
                )
                .next_action("try one statement at a time"));
            }
            let Some(value) = row.get(0) else {
                return Err(Diagnostic::new(
                    DiagnosticKind::Query,
                    "PostgreSQL returned an empty plan row",
                    "reading an extended structured query plan",
                )
                .next_action("check the PostgreSQL version and EXPLAIN permissions"));
            };
            if value.len() > MAX_PLAN_JSON_BYTES {
                return Err(plan_too_large());
            }
            payload = Some(value.to_owned());
        }
    }

    payload.ok_or_else(|| {
        Diagnostic::new(
            DiagnosticKind::Query,
            "PostgreSQL returned no query plan",
            "reading an extended structured query plan",
        )
        .next_action("check the PostgreSQL version and EXPLAIN permissions")
    })
}
