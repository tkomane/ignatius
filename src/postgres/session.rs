//! The PostgreSQL session: connecting, executing, cancelling.
//!
//! Design notes that matter:
//!
//! - Statements are executed one at a time through the simple query protocol, so
//!   values arrive in the server's own text rendering. That preserves numeric
//!   precision, timestamp offsets, and every extension type without this client
//!   needing to know the type exists.
//! - Rows beyond the cap are consumed without retention. Individual values and
//!   driver frames still allocate; a row cap is not a process-memory ceiling.
//! - Cancellation uses PostgreSQL's own cancellation request on a second
//!   connection. Nothing is ever replayed or retried.
//! - Whether the connection is really encrypted is read back from the server
//!   through `pg_stat_ssl` rather than assumed from the requested mode.

use crate::connection::{ConnectionTarget, Host, SslMode};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::{
    from_connect_error, from_parameterized_query_error, from_query_error, safe_sqlstate,
};
use crate::postgres::notices::{Notices, Operation};
use crate::postgres::tls::{TlsState, client_config};
use crate::query::result::RetentionReason;
use crate::query::result::retention_budget::{
    DescriptionAdmission, RetentionBudget, RowAdmission, TypeAdmission,
};
use crate::query::result::{
    Execution, ExecutionStatus, JobId, Notice, NoticeCheckpoint, NoticeReport, NoticeScope,
    Reported, ResultSet, StatementResult,
};
use crate::query::statements;
use crate::query::value::Cell;
use futures_util::StreamExt;
use secrecy::ExposeSecret;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio_postgres::tls::{MakeTlsConnect, TlsConnect};
use tokio_postgres::{AsyncMessage, Client, Socket, config::SslMode as PgSslMode};

/// What is known about a live session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// Safe display form of the target: never contains a password.
    pub target: String,
    /// Database name in use.
    pub database: String,
    /// Role in use.
    pub user: String,
    /// Server version string as the server reports it.
    pub server_version: String,
    /// Backend process id, useful when asking a DBA about a session.
    pub backend_pid: String,
    /// Effective `search_path`.
    pub search_path: String,
    /// Whether the session is read-only at the server.
    pub read_only: bool,
    /// The protection actually in force.
    pub tls: TlsState,
    /// The user's classification of this database.
    pub environment: crate::connection::Environment,
}

impl SessionInfo {
    /// Read/write posture as words, for the status bar.
    #[must_use]
    pub const fn posture(&self) -> &'static str {
        if self.read_only {
            "read-only"
        } else {
            "read-write"
        }
    }
}

/// A live connection to PostgreSQL.
#[derive(Debug)]
pub struct Session {
    client: Client,
    /// Serializes arbitrary user SQL without blocking cancellation or metadata.
    user_sql_gate: tokio::sync::Mutex<()>,
    info: SessionInfo,
    tls: crate::postgres::tls::TlsOptions,
    notices: Arc<Notices>,
    notice_privacy: Arc<AtomicBool>,
    connection_lost: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy)]
enum QueryPrivacy {
    Ordinary,
    Parameterized,
}

impl QueryPrivacy {
    fn diagnostic(self, error: &tokio_postgres::Error, statement_number: usize) -> Diagnostic {
        match self {
            Self::Ordinary => from_query_error(error, statement_number),
            Self::Parameterized => from_parameterized_query_error(error, statement_number),
        }
    }
}

pub(super) const PRIVATE_NOTICE_MESSAGE: &str =
    "Server notice text omitted to protect parameter values.";

pub(super) fn private_notice(code: Option<&str>) -> Notice {
    Notice {
        severity: "NOTICE".to_owned(),
        message: PRIVATE_NOTICE_MESSAGE.to_owned(),
        code: code.and_then(safe_sqlstate).map(str::to_owned),
    }
}

fn activate_notice_privacy(notices: &Notices, privacy: &AtomicBool) {
    notices.activate_privacy(privacy);
}

fn retain_notice(
    notices: &Notices,
    privacy: &AtomicBool,
    severity: &str,
    message: &str,
    code: &str,
) {
    notices.retain(privacy, severity, message, code);
}

/// Plan result and the existing post-request transaction facts.
#[derive(Debug)]
pub struct PlanResponse {
    /// Structured server JSON or the original diagnostic.
    pub result: Result<String, Diagnostic>,
    /// State observed by the preserved final probe.
    pub transaction: crate::query::result::TransactionState,
    /// Whether this physical session closed.
    pub connection_lost: bool,
}

/// One thing that happened while streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// The column names, once, before any row.
    Columns(Vec<String>),
    /// One row.
    Row(Vec<Cell>),
}

/// Why a stream stopped early.
#[derive(Debug)]
pub enum StreamStop {
    /// The server, or the shape of the result, ended it.
    Server(Box<Diagnostic>),
    /// The destination could not be written.
    Sink(std::io::Error),
}

/// A handle that can cancel the session's running statement from elsewhere.
///
/// Cancellation opens its own connection, which is why it can work while the
/// session is busy. It asks the server to cancel; the server decides.
#[derive(Clone)]
pub struct CancelHandle {
    token: tokio_postgres::CancelToken,
    tls: crate::postgres::tls::TlsOptions,
}

impl std::fmt::Debug for CancelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The driver's token is not Debug, and it holds connection details that
        // have no business appearing in a log line.
        f.debug_struct("CancelHandle")
            .field("sslmode", &self.tls.mode)
            .finish_non_exhaustive()
    }
}

impl CancelHandle {
    /// Sends a cancellation request for whatever the session is running.
    ///
    /// Returning `Ok` means the request was delivered, not that the statement has
    /// stopped. The interface says "Cancellation requested" until the server
    /// confirms with SQLSTATE 57014.
    pub async fn cancel(&self) -> Result<(), Diagnostic> {
        let result = match client_config(&self.tls)? {
            None => self.token.cancel_query(tokio_postgres::NoTls).await,
            Some(config) => {
                self.token
                    .cancel_query(tokio_postgres_rustls::MakeRustlsConnect::new(config))
                    .await
            }
        };
        result.map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Connection,
                "the cancellation request could not be delivered",
                "cancelling the running statement",
            )
            .likely_cause(err.to_string())
            .next_action(
                "the statement may still be running on the server; it can be stopped with \
                 pg_cancel_backend() by an administrator",
            )
        })
    }
}

impl Session {
    /// Facts about this session.
    #[must_use]
    pub const fn info(&self) -> &SessionInfo {
        &self.info
    }

    /// A handle for cancelling from another task.
    #[must_use]
    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle {
            token: self.client.cancel_token(),
            tls: self.tls.clone(),
        }
    }

    /// Whether the connection has been lost.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.client.is_closed() || self.connection_lost.load(Ordering::SeqCst)
    }

    /// Payload-only compatibility boundary for idle notices. Does not drain
    /// active operations or erase cumulative accounting. First-party consumers
    /// use [`Self::take_connection_notice_report`] instead.
    pub fn take_notices(&self) -> Vec<Notice> {
        self.take_connection_notice_report()
            .map_or_else(Vec::new, |report| report.unattributed)
    }

    /// Moves one bounded idle report; active statement spending is untouched.
    pub fn take_connection_notice_report(&self) -> Option<NoticeReport> {
        self.notices.take_idle()
    }

    /// Committed client accounting for this physical connection, without payload.
    #[must_use]
    pub fn notice_checkpoint(&self) -> NoticeCheckpoint {
        self.notices.checkpoint()
    }

    /// Nonmutating counts observed at actual retirement, including uncommitted
    /// active/idle accounting. It does not reset spending or imply later delivery.
    #[must_use]
    pub fn notice_retirement_counts(&self) -> crate::query::result::NoticeCounts {
        self.notices.retirement_counts()
    }

    /// Schemas visible to this session, with counts by object kind.
    pub async fn schemas(
        &self,
    ) -> Result<Vec<crate::postgres::metadata::SchemaSummary>, Diagnostic> {
        crate::postgres::metadata::schemas(&self.client).await
    }

    /// Objects of one kind inside one schema.
    pub async fn objects(
        &self,
        schema: &str,
        kind: crate::postgres::metadata::ObjectKind,
    ) -> Result<Vec<crate::postgres::metadata::ObjectSummary>, Diagnostic> {
        crate::postgres::metadata::objects(&self.client, schema, kind).await
    }

    /// Columns of a relation.
    pub async fn columns(
        &self,
        schema: &str,
        relation: &str,
    ) -> Result<Vec<crate::postgres::metadata::ColumnInfo>, Diagnostic> {
        crate::postgres::metadata::columns(&self.client, schema, relation).await
    }

    /// Resolves a result source and reads the live facts required for a cell update.
    pub async fn update_relation(
        &self,
        schema: Option<&str>,
        relation: &str,
    ) -> Result<crate::postgres::metadata::UpdateRelation, Diagnostic> {
        crate::postgres::metadata::update_relation(&self.client, schema, relation).await
    }

    /// Reads the one catalogue snapshot used for local completion.
    pub async fn completion_catalog(
        &self,
    ) -> Result<crate::query::completion::CompletionCatalog, Diagnostic> {
        crate::postgres::metadata::completion_catalog(&self.client).await
    }

    /// Indexes on a relation.
    pub async fn indexes(
        &self,
        schema: &str,
        relation: &str,
    ) -> Result<Vec<crate::postgres::metadata::ObjectSummary>, Diagnostic> {
        crate::postgres::metadata::indexes(&self.client, schema, relation).await
    }

    /// The definition of one object, as text.
    pub async fn definition(
        &self,
        object: &crate::postgres::metadata::ObjectSummary,
    ) -> Result<crate::postgres::metadata::Definition, Diagnostic> {
        crate::postgres::metadata::definition(&self.client, object).await
    }

    /// What an object depends on, and what depends on it.
    pub async fn dependencies(
        &self,
        object: &crate::postgres::metadata::ObjectSummary,
    ) -> Result<crate::postgres::metadata::Dependencies, Diagnostic> {
        crate::postgres::metadata::dependencies(&self.client, object).await
    }

    /// Reads one statement's structured PostgreSQL plan (payload-only compatibility).
    /// Use [`Self::explain_reported`] for observed notice accounting.
    ///
    /// This is deliberately separate from [`Self::execute`]. The generated
    /// EXPLAIN command must not become an ordinary result, history entry, or
    /// export, and keeping the boundary here makes that distinction visible at
    /// the only place that can send it to PostgreSQL.
    pub async fn explain(&self, sql: &str, analyze: bool) -> Result<String, Diagnostic> {
        self.explain_reported(sql, analyze).await.value.result
    }

    /// Plan outcome plus observed notice accounting on every return path.
    pub async fn explain_reported(&self, sql: &str, analyze: bool) -> Reported<PlanResponse> {
        let _gate = self.user_sql_gate.lock().await;
        let operation = self.notices.begin(NoticeScope::Plan);
        let result = self.explain_inner(sql, analyze).await;
        let connection_lost = self.is_closed();
        let transaction = if connection_lost {
            crate::query::result::TransactionState::Unknown
        } else {
            self.transaction_state().await
        };
        Reported {
            value: PlanResponse {
                result,
                transaction,
                connection_lost,
            },
            notice_report: operation.finish(),
        }
    }

    async fn explain_inner(&self, sql: &str, analyze: bool) -> Result<String, Diagnostic> {
        const PLAIN_OPTIONS: &str = "ANALYZE false, COSTS true, SUMMARY true, FORMAT JSON";
        const ANALYZE_OPTIONS: &str =
            "ANALYZE true, COSTS true, SUMMARY true, TIMING true, FORMAT JSON";
        let statement = format!(
            "EXPLAIN ({}) {}",
            if analyze {
                ANALYZE_OPTIONS
            } else {
                PLAIN_OPTIONS
            },
            sql.trim()
        );
        let stream = self
            .client
            .simple_query_raw(&statement)
            .await
            .map_err(|err| from_query_error(&err, 1))?;
        tokio::pin!(stream);

        let mut payload = None;
        while let Some(message) = stream.next().await {
            let message = message.map_err(|err| from_query_error(&err, 1))?;
            if let tokio_postgres::SimpleQueryMessage::Row(row) = message {
                if payload.is_some() {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Query,
                        "PostgreSQL returned more than one plan row",
                        "reading a structured query plan",
                    )
                    .next_action("try one statement at a time"));
                }
                let Some(value) = row.get(0) else {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Query,
                        "PostgreSQL returned an empty plan row",
                        "reading a structured query plan",
                    )
                    .next_action("check the PostgreSQL version and EXPLAIN permissions"));
                };
                if value.len() > crate::query::plan::MAX_PLAN_JSON_BYTES {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Query,
                        "the query plan exceeded the client display bound",
                        "reading a structured query plan",
                    )
                    .likely_cause(format!(
                        "the JSON response is larger than {} bytes",
                        crate::query::plan::MAX_PLAN_JSON_BYTES
                    ))
                    .next_action(
                        "simplify the statement or inspect its plan with PostgreSQL tools",
                    ));
                }
                payload = Some(value.to_owned());
            }
        }

        payload.ok_or_else(|| {
            Diagnostic::new(
                DiagnosticKind::Query,
                "PostgreSQL returned no query plan",
                "reading a structured query plan",
            )
            .next_action("check the PostgreSQL version and EXPLAIN permissions")
        })
    }

    /// Extensions installed in this database.
    pub async fn extensions(
        &self,
    ) -> Result<Vec<crate::postgres::metadata::ObjectSummary>, Diagnostic> {
        crate::postgres::metadata::extensions(&self.client).await
    }

    /// Asks the server what the session's transaction state is.
    ///
    /// One extra round trip after an execution, which is the price of not
    /// guessing. The driver does not expose the protocol's own transaction
    /// status, and inferring it from the statements sent would be wrong exactly
    /// when it matters: after a server-side rollback, or an error raised inside
    /// a function.
    ///
    /// The question asked is deliberate. `pg_stat_activity` cannot answer it:
    /// a backend asking about itself is always `active`, because it is running
    /// this very query. Comparing the transaction's timestamp with the
    /// statement's does answer it, since the two differ only inside an explicit
    /// transaction block. And a transaction that has failed cannot answer at
    /// all, which is exactly how a failed transaction is recognised.
    pub(crate) async fn transaction_state(&self) -> crate::query::result::TransactionState {
        use crate::query::result::TransactionState;
        const PROBE: &str =
            "SELECT (transaction_timestamp() <> statement_timestamp())::text AS in_transaction";

        match self.client.simple_query(PROBE).await {
            Ok(messages) => {
                let answer = messages.into_iter().find_map(|message| match message {
                    tokio_postgres::SimpleQueryMessage::Row(row) => {
                        row.get(0).map(|value| value == "true")
                    }
                    _ => None,
                });
                TransactionState::from_probe(answer)
            }
            Err(err) => {
                if err.as_db_error().is_some_and(|db| {
                    *db.code() == tokio_postgres::error::SqlState::IN_FAILED_SQL_TRANSACTION
                }) {
                    TransactionState::Failed
                } else {
                    TransactionState::Unknown
                }
            }
        }
    }

    /// Streams a single statement's rows without retaining any of them.
    ///
    /// The full result is not retained. Individual values, driver frames,
    /// the bounded notice inbox and downstream encoders still allocate; this
    /// is not a process-memory ceiling. The sink receives events in order and
    /// may fail. This compatibility adapter returns payload only; use
    /// [`Self::stream_reported`] for observed notice accounting.
    pub async fn stream(
        &self,
        sql: &str,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
    ) -> Result<u64, StreamStop> {
        self.stream_reported(sql, sink).await.value
    }

    /// Streams unchanged Query protocol bytes and returns accounting on every stop.
    pub async fn stream_reported(
        &self,
        sql: &str,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
    ) -> Reported<Result<u64, StreamStop>> {
        let _gate = self.user_sql_gate.lock().await;
        let operation = self.notices.begin(NoticeScope::Export);
        let value = self.stream_bound(sql, sink, QueryPrivacy::Ordinary).await;
        Reported {
            value,
            notice_report: operation.finish(),
        }
    }

    async fn stream_bound(
        &self,
        sql: &str,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
        privacy: QueryPrivacy,
    ) -> Result<u64, StreamStop> {
        let stream = self
            .client
            .simple_query_raw(sql)
            .await
            .map_err(|err| StreamStop::Server(Box::new(privacy.diagnostic(&err, 1))))?;
        tokio::pin!(stream);

        let mut rows = 0u64;
        let mut seen_columns = false;

        while let Some(message) = stream.next().await {
            let message =
                message.map_err(|err| StreamStop::Server(Box::new(privacy.diagnostic(&err, 1))))?;
            match message {
                tokio_postgres::SimpleQueryMessage::RowDescription(columns) => {
                    if seen_columns {
                        // A second result set cannot be appended to the first
                        // without inventing a meaning for the join.
                        return Err(StreamStop::Server(Box::new(
                            Diagnostic::new(
                                DiagnosticKind::Usage,
                                "an export needs a single result set",
                                "streaming rows to a file",
                            )
                            .likely_cause("the statement produced more than one set of rows")
                            .next_action("export one statement at a time"),
                        )));
                    }
                    seen_columns = true;
                    let names = columns.iter().map(|c| c.name().to_owned()).collect();
                    sink(StreamEvent::Columns(names)).map_err(StreamStop::Sink)?;
                }
                tokio_postgres::SimpleQueryMessage::Row(row) => {
                    let cells = (0..row.len())
                        .map(|i| Cell::from_option(row.get(i)))
                        .collect();
                    sink(StreamEvent::Row(cells)).map_err(StreamStop::Sink)?;
                    rows += 1;
                }
                tokio_postgres::SimpleQueryMessage::CommandComplete(_) => {}
                _ => {}
            }
        }

        Ok(rows)
    }

    /// Binds named values and streams the resulting simple query.
    ///
    /// The template is the only SQL descriptor retained by callers. The bound
    /// text exists only for this request and is never logged or put in history.
    /// Payload-only compatibility adapter; use the reported form for client accounting.
    pub async fn stream_with_parameters(
        &self,
        sql: &str,
        parameters: &crate::query::ParameterBindings,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
    ) -> Result<u64, StreamStop> {
        self.stream_with_parameters_reported(sql, parameters, sink)
            .await
            .value
    }

    /// Parameterized streaming with sticky privacy before probe, binding and adoption.
    pub async fn stream_with_parameters_reported(
        &self,
        sql: &str,
        parameters: &crate::query::ParameterBindings,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
    ) -> Reported<Result<u64, StreamStop>> {
        let _gate = self.user_sql_gate.lock().await;
        activate_notice_privacy(&self.notices, &self.notice_privacy);
        let operation = self.notices.begin(NoticeScope::Export);
        let value = async {
            self.require_parameter_semantics(1)
                .await
                .map_err(|error| StreamStop::Server(Box::new(error)))?;
            let bound = crate::query::discover_parameters(sql)
                .and_then(|template| template.bind(sql, parameters))
                .map_err(|error| {
                    StreamStop::Server(Box::new(
                        error.diagnostic("binding named parameters for export"),
                    ))
                })?;
            self.stream_bound(&bound, sink, QueryPrivacy::Parameterized)
                .await
        }
        .await;
        Reported {
            value,
            notice_report: operation.finish(),
        }
    }

    /// Executes every statement in a buffer, in order.
    ///
    /// Statements after a failure do not run. Results produced before the failure
    /// are kept and returned along with the diagnostic, because they happened.
    pub async fn execute(&self, sql: &str, row_cap: usize, job: JobId) -> Execution {
        let _operation = self.user_sql_gate.lock().await;
        tracing::debug!(
            target: "ignatius::query",
            "{}",
            crate::diagnostics::logging::statement_descriptor(job.0, sql)
        );
        let operation = self.notices.begin(NoticeScope::Query);
        let mut execution = self
            .execute_bound(sql, row_cap, job, QueryPrivacy::Ordinary, &operation)
            .await;
        execution.notice_report = operation.finish();
        execution
    }

    /// Binds named values and executes the resulting simple query.
    ///
    /// Binding happens after the connection exists but before the statement is
    /// submitted. A binding failure is represented as a failed execution so
    /// the reducer and plain client retain their normal recovery paths.
    pub async fn execute_with_parameters(
        &self,
        sql: &str,
        parameters: &crate::query::ParameterBindings,
        row_cap: usize,
        job: JobId,
    ) -> Execution {
        let _operation = self.user_sql_gate.lock().await;
        activate_notice_privacy(&self.notices, &self.notice_privacy);
        let operation = self.notices.begin(NoticeScope::Query);
        let mut execution = async {
            tracing::debug!(
                target: "ignatius::query",
                "{}",
                crate::diagnostics::logging::statement_descriptor(job.0, sql)
            );
            let started = Instant::now();
            if let Err(diagnostic) = self.require_parameter_semantics(1).await {
                return self
                    .failed_execution(job, started, Vec::new(), diagnostic)
                    .await;
            }
            let bound = match crate::query::discover_parameters(sql)
                .and_then(|template| template.bind(sql, parameters))
            {
                Ok(bound) => bound,
                Err(error) => {
                    return Execution {
                        notice_report: crate::query::result::NoticeReport::default(),
                        job,
                        statements: Vec::new(),
                        status: ExecutionStatus::Failed,
                        elapsed: started.elapsed(),
                        error: Some(error.diagnostic("binding named parameters for execution")),
                        transaction: self.transaction_state().await,
                    };
                }
            };
            let mut execution = self
                .execute_bound(
                    &bound,
                    row_cap,
                    job,
                    QueryPrivacy::Parameterized,
                    &operation,
                )
                .await;
            execution.elapsed = started.elapsed();
            execution
        }
        .await;
        execution.notice_report = operation.finish();
        execution
    }

    /// Checks the live lexer setting while the user-SQL gate is held.
    ///
    /// A connection-time snapshot is insufficient: preceding SQL can change
    /// this setting. The probe never changes it and uses the value-free error
    /// route even when PostgreSQL cannot answer (for example a failed txn).
    async fn require_parameter_semantics(&self, statement_number: usize) -> Result<(), Diagnostic> {
        let messages = self
            .client
            .simple_query("SHOW standard_conforming_strings")
            .await
            .map_err(|error| {
                let mut diagnostic = from_parameterized_query_error(&error, statement_number);
                // A fatal ErrorResponse can reach its waiter before the
                // connection task has marked the socket closed. Its severity
                // is used only for classification, never displayed.
                if self.is_closed() || error.as_db_error().is_some_and(|db| matches!(db.severity(), "FATAL" | "PANIC")) {
                    diagnostic.kind = DiagnosticKind::Connection;
                }
                diagnostic.attempted = format!(
                    "checking standard_conforming_strings before statement {statement_number}"
                );
                diagnostic.headline = match diagnostic.kind {
                    DiagnosticKind::Cancelled => "the server cancelled the string-semantics check; the requested statement was not sent",
                    DiagnosticKind::Connection => "the connection closed during the string-semantics check; the requested statement was not sent",
                    _ => "the string-semantics check failed; the requested statement was not sent (server text omitted for privacy)",
                }.to_owned();
                if diagnostic.kind == DiagnosticKind::Connection {
                    diagnostic.likely_cause = Some("the connection ended before the requested statement could be sent".to_owned());
                    diagnostic.next_action = Some("reconnect and inspect any earlier statements before trying again; nothing was retried".to_owned());
                }
                diagnostic
            })?;
        let mut rows = messages.iter().filter_map(|message| match message {
            tokio_postgres::SimpleQueryMessage::Row(row) => Some(row),
            _ => None,
        });
        let setting = rows
            .next()
            .filter(|row| row.len() == 1)
            .and_then(|row| row.get(0));
        // More than one answer is an unknown response, not permission to guess.
        let setting = if rows.next().is_some() { None } else { setting };
        validate_parameter_semantics(setting, statement_number)
    }

    /// Builds an execution failure without re-entering the user-SQL gate.
    async fn failed_execution(
        &self,
        job: JobId,
        started: Instant,
        statements: Vec<StatementResult>,
        diagnostic: Diagnostic,
    ) -> Execution {
        let status = if diagnostic.kind == DiagnosticKind::Cancelled {
            ExecutionStatus::Cancelled
        } else if diagnostic.kind == DiagnosticKind::Connection || self.is_closed() {
            ExecutionStatus::ConnectionLost
        } else {
            ExecutionStatus::Failed
        };
        let transaction = if status == ExecutionStatus::ConnectionLost {
            crate::query::result::TransactionState::Unknown
        } else {
            self.transaction_state().await
        };
        Execution {
            notice_report: crate::query::result::NoticeReport::default(),
            job,
            statements,
            status,
            elapsed: started.elapsed(),
            error: Some(diagnostic),
            transaction,
        }
    }

    async fn execute_bound(
        &self,
        sql: &str,
        row_cap: usize,
        job: JobId,
        privacy: QueryPrivacy,
        operation: &Operation<'_>,
    ) -> Execution {
        let started = Instant::now();
        let parsed = statements::split(sql);
        let mut results = Vec::new();
        let mut budget = RetentionBudget::new();

        if parsed.is_empty() {
            return Execution {
                notice_report: crate::query::result::NoticeReport::default(),
                job,
                statements: results,
                status: ExecutionStatus::Succeeded,
                elapsed: started.elapsed(),
                error: None,
                transaction: self.transaction_state().await,
            };
        }

        for (index, statement) in parsed.iter().enumerate() {
            // The initial check precedes binding and covers the first send.
            // No arbitrary SQL can intervene while the gate is held. A prior
            // statement in this same buffer may change the setting, however.
            if index > 0
                && matches!(privacy, QueryPrivacy::Parameterized)
                && let Err(diagnostic) = self.require_parameter_semantics(index + 1).await
            {
                return self
                    .failed_execution(job, started, results, diagnostic)
                    .await;
            }
            operation.hold_unattributed();
            let statement_started = Instant::now();
            match self
                .execute_one(
                    &statement.text,
                    row_cap,
                    index + 1,
                    privacy,
                    operation,
                    &mut budget,
                )
                .await
            {
                Ok((result_set, rows_affected, notices)) => {
                    results.push(StatementResult {
                        result_set,
                        rows_affected,
                        elapsed: statement_started.elapsed(),
                        notices,
                    });
                }
                Err(diagnostic) => {
                    return self
                        .failed_execution(job, started, results, diagnostic)
                        .await;
                }
            }
        }

        let transaction = self.transaction_state().await;
        Execution {
            notice_report: crate::query::result::NoticeReport::default(),
            job,
            statements: results,
            status: ExecutionStatus::Succeeded,
            elapsed: started.elapsed(),
            error: None,
            transaction,
        }
    }

    /// Runs one statement, streaming rows and stopping retention at the cap.
    async fn execute_one(
        &self,
        sql: &str,
        row_cap: usize,
        statement_number: usize,
        privacy: QueryPrivacy,
        operation: &Operation<'_>,
        budget: &mut RetentionBudget,
    ) -> Result<(Option<ResultSet>, Option<u64>, Vec<Notice>), Diagnostic> {
        let stream = self
            .client
            .simple_query_raw(sql)
            .await
            .map_err(|err| privacy.diagnostic(&err, statement_number))?;
        tokio::pin!(stream);

        let mut result_set: Option<ResultSet> = None;
        let mut rows_affected = None;

        while let Some(message) = stream.next().await {
            match message.map_err(|err| privacy.diagnostic(&err, statement_number))? {
                tokio_postgres::SimpleQueryMessage::RowDescription(columns) => {
                    result_set = Some(retain_description(
                        columns.iter().map(|c| c.name()),
                        columns.len(),
                        row_cap,
                        budget,
                    ));
                }
                tokio_postgres::SimpleQueryMessage::Row(row) => {
                    let set = result_set.get_or_insert_with(|| {
                        retain_description(std::iter::empty(), 0, row_cap, budget)
                    });
                    retain_buffered_row(set, (0..row.len()).map(|i| row.get(i)), budget).map_err(
                        |bytes| retained_value_limit_diagnostic(statement_number, bytes),
                    )?;
                }
                tokio_postgres::SimpleQueryMessage::CommandComplete(count) => {
                    rows_affected = Some(count);
                }
                _ => {}
            }
        }

        // Capture only the completed query phase before prepare/describe.
        // This is an observation boundary, not a causal protocol fence.
        let notices = operation.take_statement();

        // The simple-query protocol gives us faithful text values and column
        // names, but not type OIDs. Parse/describe is deliberately after the
        // result has arrived: it does not execute the statement, and a failure
        // here must never turn a successful query into a failed one.
        if !budget.data_is_closed()
            && let Some(set) = result_set
                .as_mut()
                .filter(|set| set.is_available() && !set.columns.is_empty())
            && let Some(types) = self.describe_columns(sql, &set.columns, budget).await
        {
            let _ = set.set_column_types(types);
        }

        operation.hold_unattributed();
        Ok((result_set, rows_affected, notices))
    }

    /// Describes one already-received result without replaying its SQL.
    ///
    /// The names are checked as well as the count because a description that is
    /// accepted against the wrong shape would make a truthful type label into a
    /// quiet lie. A failed prepare or mismatch is ordinary unavailability for
    /// the grid, not a query failure.
    async fn describe_columns(
        &self,
        sql: &str,
        names: &[String],
        budget: &mut RetentionBudget,
    ) -> Option<Vec<Option<String>>> {
        let statement = self.client.prepare(sql).await.ok()?;
        let columns = statement.columns();
        if columns.len() != names.len()
            || columns
                .iter()
                .zip(names)
                .any(|(column, name)| column.name() != name)
        {
            return None;
        }
        if !matches!(
            budget.admit_type_names(
                columns.len(),
                columns.iter().map(|column| Some(column.type_().name()))
            ),
            TypeAdmission::Admitted { .. }
        ) {
            return None;
        }
        Some(
            columns
                .iter()
                .map(|column| Some(column.type_().name().to_owned()))
                .collect(),
        )
    }
}

/// The actual borrowed-name admission boundary, before owned names/type slots.
fn retain_description<'a>(
    names: impl Iterator<Item = &'a str> + Clone,
    source_columns: usize,
    cap: usize,
    budget: &mut RetentionBudget,
) -> ResultSet {
    match budget.admit_description(names.clone()) {
        DescriptionAdmission::Admitted { .. } => {
            ResultSet::new(names.map(str::to_owned).collect(), cap)
        }
        DescriptionAdmission::Unavailable => ResultSet::unavailable(source_columns, cap),
    }
}

/// Counts each delivered row once, without owned copies for a refused row.
fn retain_buffered_row<'a>(
    set: &mut ResultSet,
    cells: impl Iterator<Item = Option<&'a str>> + Clone,
    budget: &mut RetentionBudget,
) -> Result<(), usize> {
    if !set.is_available() {
        set.discard_row(RetentionReason::DescriptionUnavailable);
    } else if set.retained() >= set.cap {
        set.discard_row(RetentionReason::RowCap);
    } else {
        match budget.admit_row(cells.clone()) {
            RowAdmission::Admitted { .. } => {
                set.push(cells.map(Cell::from_option).collect());
            }
            RowAdmission::DataLimit => set.discard_row(RetentionReason::RetainedResultLimit),
            RowAdmission::OversizedCell { bytes } => return Err(bytes),
        }
    }
    Ok(())
}

fn validate_parameter_semantics(
    setting: Option<&str>,
    statement_number: usize,
) -> Result<(), Diagnostic> {
    if setting == Some("on") {
        return Ok(());
    }
    Err(Diagnostic::new(
        DiagnosticKind::Query,
        "named parameters require standard_conforming_strings = on",
        format!("checking standard_conforming_strings before statement {statement_number}"),
    )
    .in_statement(statement_number)
    .likely_cause("the server did not confirm the supported string-literal semantics")
    .next_action("restore standard_conforming_strings = on separately, then run the statement again; nothing was changed or retried"))
}

fn retained_value_limit_diagnostic(statement_number: usize, bytes: usize) -> Diagnostic {
    let limit = crate::query::value::MAX_RETAINED_VALUE_BYTES;
    Diagnostic::new(
        DiagnosticKind::Query,
        "a result value is too large to retain",
        format!("retaining result values from statement {statement_number}"),
    )
    .in_statement(statement_number)
    .likely_cause(format!(
        "one server-rendered value is {bytes} bytes; the retained-value limit is {limit} bytes"
    ))
    .next_action(
        "use streaming export as csv, tsv, ndjson or insert, or select a smaller value before \
         running it in the interactive result view",
    )
    .technical("Limit", limit.to_string())
}

/// Opens a session, applying the resolved target's policy.
/// Payload-only compatibility adapter. Use [`connect_reported`] for accounting.
pub async fn connect(
    target: &ConnectionTarget,
    statement_timeout: Duration,
) -> Result<Session, Diagnostic> {
    connect_reported(target, statement_timeout).await.value
}

/// Connects with a report on bootstrap failure, preserving successful idle notices.
pub async fn connect_reported(
    target: &ConnectionTarget,
    statement_timeout: Duration,
) -> Reported<Result<Session, Diagnostic>> {
    let mut observed = None;
    let value = async {
        // A backstop, not a feature. Every path that opens a connection is supposed
        // to have run `connection::cloud::authenticate` first; a path that forgot
        // would otherwise connect without the token and fall through to the password
        // prompt, asking a person for something no person has. Failing here instead
        // makes the omission loud and keeps the prompt honest.
        if target.auth.is_some() && target.password.is_none() {
            return Err(Diagnostic::new(
            crate::diagnostics::DiagnosticKind::Internal,
            "a connection using a cloud identity provider was opened without a credential",
            "connecting to PostgreSQL",
        )
        .likely_cause(
            "this is a defect in this build: a code path opened a connection without asking the \
             provider for a token first",
        )
        .next_action("report it, and connect without --auth as a way through in the meantime")
        .technical_opt("Provider", target.auth.clone()));
        }

        let config = build_config(target);
        let tls = crate::postgres::tls::TlsOptions {
            mode: target.sslmode,
            root_cert: target.root_cert.clone(),
            client_cert: target.client_cert.clone(),
            client_key: target.client_key.clone(),
        };
        let tls_config = client_config(&tls)?;
        let notice_privacy = Arc::new(AtomicBool::new(false));

        let (client, notices, connection_lost) = match tls_config {
            None => {
                establish(
                    &config,
                    tokio_postgres::NoTls,
                    target,
                    Arc::clone(&notice_privacy),
                )
                .await?
            }
            Some(rustls_config) => {
                establish(
                    &config,
                    tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config),
                    target,
                    Arc::clone(&notice_privacy),
                )
                .await?
            }
        };

        observed = Some(Arc::clone(&notices));

        // Asked for before anything else runs, and enforced by the server rather
        // than by this client guessing at what a statement does.
        if target.read_only {
            client
                .batch_execute("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY")
                .await
                .map_err(|err| from_query_error(&err, 0))?;
        }

        if statement_timeout > Duration::ZERO {
            let millis = statement_timeout.as_millis();
            client
                .batch_execute(&format!("SET statement_timeout = {millis}"))
                .await
                .map_err(|err| from_query_error(&err, 0))?;
        }

        let info = bootstrap(&client, target).await?;
        Ok(Session {
            client,
            user_sql_gate: tokio::sync::Mutex::new(()),
            info,
            tls,
            notices,
            notice_privacy,
            connection_lost,
        })
    }
    .await;
    let notice_report = if value.is_err() {
        observed
            .and_then(|notices: Arc<Notices>| notices.take_idle())
            .unwrap_or_else(|| NoticeReport::empty(NoticeScope::Connection))
    } else {
        NoticeReport::empty(NoticeScope::Connection)
    };
    Reported {
        value,
        notice_report,
    }
}

type Established = (Client, Arc<Notices>, Arc<AtomicBool>);

/// Connects, then moves the connection onto a task that forwards notices.
async fn establish<T>(
    config: &tokio_postgres::Config,
    tls: T,
    target: &ConnectionTarget,
    notice_privacy: Arc<AtomicBool>,
) -> Result<Established, Diagnostic>
where
    T: MakeTlsConnect<Socket> + Send + 'static,
    T::Stream: Send + 'static,
    T::TlsConnect: Send,
    <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
{
    let attempt = tokio::time::timeout(target.connect_timeout, config.connect(tls));
    let (client, mut connection) = match attempt.await {
        Ok(Ok(pair)) => pair,
        Ok(Err(err)) => return Err(from_connect_error(&err, target)),
        Err(_elapsed) => {
            return Err(Diagnostic::new(
                DiagnosticKind::Connection,
                format!(
                    "no response from {} after {} seconds",
                    target.host.display(),
                    target.connect_timeout.as_secs()
                ),
                format!("connecting to {}", target.safe_display()),
            )
            .likely_cause(
                "the host is unreachable, or a firewall is dropping packets rather than \
                 refusing them",
            )
            .next_action(
                "check the host and port, or raise connection.connect-timeout-seconds if the \
                 server is simply slow to accept",
            ));
        }
    };

    let notices = Arc::new(Notices::default());
    let connection_lost = Arc::new(AtomicBool::new(false));
    let task_notices = Arc::clone(&notices);
    let task_lost = Arc::clone(&connection_lost);

    tokio::spawn(async move {
        // Polling the connection directly, rather than awaiting it as a future,
        // is what makes server notices visible to the interface.
        let mut stream = futures_util::stream::poll_fn(move |cx| connection.poll_message(cx));
        while let Some(message) = stream.next().await {
            match message {
                Ok(AsyncMessage::Notice(notice)) => {
                    retain_notice(
                        &task_notices,
                        &notice_privacy,
                        notice.severity(),
                        notice.message(),
                        notice.code().code(),
                    );
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        task_lost.store(true, Ordering::SeqCst);
    });

    Ok((client, notices, connection_lost))
}

/// Builds the driver configuration from a resolved target.
fn build_config(target: &ConnectionTarget) -> tokio_postgres::Config {
    let mut config = tokio_postgres::Config::new();
    match &target.host {
        Host::Tcp(host) => {
            config.host(host);
        }
        // `host_path` exists only where Unix sockets do. Target resolution
        // already refuses a socket on platforms without them and says so, which
        // makes the second arm unreachable; it is written as a total function
        // rather than a panic so a future caller cannot turn it into a crash.
        #[cfg(unix)]
        Host::Socket(path) => {
            config.host_path(path);
        }
        #[cfg(not(unix))]
        Host::Socket(path) => {
            config.host(&path.display().to_string());
        }
    }
    config
        .port(target.port)
        .user(&target.user)
        .dbname(&target.database)
        .application_name(&target.application_name)
        .connect_timeout(target.connect_timeout);

    if let Some(password) = &target.password {
        config.password(password.expose_secret());
    }

    config.ssl_mode(match target.sslmode {
        SslMode::Disable => PgSslMode::Disable,
        SslMode::Prefer => PgSslMode::Prefer,
        // Require, verify-ca and verify-full all require TLS at the protocol
        // level. What differs is the certificate policy, which lives in the
        // rustls configuration, not here.
        SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => PgSslMode::Require,
    });

    config
}

/// Reads back the facts about the session from the server itself.
async fn bootstrap(client: &Client, target: &ConnectionTarget) -> Result<SessionInfo, Diagnostic> {
    const FULL: &str = "SELECT current_setting('server_version') AS server_version, \
         pg_backend_pid()::text AS backend_pid, \
         current_setting('search_path') AS search_path, \
         current_setting('transaction_read_only') AS read_only, \
         coalesce((SELECT ssl::text FROM pg_stat_ssl WHERE pid = pg_backend_pid()), 'unknown') AS ssl, \
         coalesce((SELECT version FROM pg_stat_ssl WHERE pid = pg_backend_pid()), '') AS ssl_version, \
         coalesce((SELECT cipher FROM pg_stat_ssl WHERE pid = pg_backend_pid()), '') AS ssl_cipher";
    const MINIMAL: &str = "SELECT current_setting('server_version') AS server_version, \
         pg_backend_pid()::text AS backend_pid, \
         current_setting('search_path') AS search_path, \
         current_setting('transaction_read_only') AS read_only";

    // pg_stat_ssl can be restricted on a hardened server. Falling back keeps the
    // session usable, and the TLS state is then reported as unknown rather than
    // guessed from what was requested.
    let (row, has_ssl_columns) = match single_row(client, FULL).await {
        Ok(Some(row)) => (row, true),
        _ => (
            single_row(client, MINIMAL)
                .await
                .map_err(|err| from_connect_error(&err, target))?
                .ok_or_else(|| {
                    Diagnostic::new(
                        DiagnosticKind::Connection,
                        "the server accepted the connection but returned no session information",
                        "reading session state after connecting",
                    )
                    .likely_cause("a proxy or pooler may be intercepting the connection")
                    .next_action("connect directly to the PostgreSQL server to confirm")
                })?,
            false,
        ),
    };

    let get = |name: &str| -> String { row.get(name).unwrap_or_default().to_owned() };

    let tls = if target.sslmode == SslMode::Disable {
        TlsState::Disabled
    } else if !has_ssl_columns {
        TlsState::Unknown
    } else {
        match get("ssl").as_str() {
            "t" | "true" => TlsState::Active {
                version: non_empty(get("ssl_version"), "TLS"),
                cipher: non_empty(get("ssl_cipher"), "unknown cipher"),
                mode: target.sslmode,
            },
            "f" | "false" => TlsState::NotNegotiated,
            _ => TlsState::Unknown,
        }
    };

    Ok(SessionInfo {
        target: target.safe_display(),
        database: target.database.clone(),
        user: target.user.clone(),
        server_version: get("server_version"),
        backend_pid: get("backend_pid"),
        search_path: get("search_path"),
        read_only: matches!(get("read_only").as_str(), "on" | "t" | "true"),
        tls,
        environment: target.environment.clone(),
    })
}

/// Runs a query expected to return exactly one row.
async fn single_row(
    client: &Client,
    sql: &str,
) -> Result<Option<tokio_postgres::SimpleQueryRow>, tokio_postgres::Error> {
    let messages = client.simple_query(sql).await?;
    Ok(messages.into_iter().find_map(|message| match message {
        tokio_postgres::SimpleQueryMessage::Row(row) => Some(row),
        _ => None,
    }))
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value
    }
}

#[cfg(test)]
mod tests;
