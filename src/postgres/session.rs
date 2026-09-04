//! The PostgreSQL session: connecting, executing, cancelling.
//!
//! Design notes that matter:
//!
//! - Statements are executed one at a time through the simple query protocol, so
//!   values arrive in the server's own text rendering. That preserves numeric
//!   precision, timestamp offsets, and every extension type without this client
//!   needing to know the type exists.
//! - Rows are consumed from a stream and dropped once the row cap is reached, so
//!   a large result cannot exhaust memory. The count is still accurate.
//! - Cancellation uses PostgreSQL's own cancellation request on a second
//!   connection. Nothing is ever replayed or retried.
//! - Whether the connection is really encrypted is read back from the server
//!   through `pg_stat_ssl` rather than assumed from the requested mode.

use crate::connection::{ConnectionTarget, Host, SslMode};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::{from_connect_error, from_query_error};
use crate::postgres::tls::{TlsState, client_config};
use crate::query::result::{Execution, ExecutionStatus, JobId, Notice, ResultSet, StatementResult};
use crate::query::statements;
use crate::query::value::Cell;
use futures_util::StreamExt;
use secrecy::ExposeSecret;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    info: SessionInfo,
    tls: crate::postgres::tls::TlsOptions,
    notices: Arc<Mutex<Vec<Notice>>>,
    connection_lost: Arc<AtomicBool>,
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

    /// Takes the notices received since the last call.
    pub fn take_notices(&self) -> Vec<Notice> {
        self.notices
            .lock()
            .map(|mut held| std::mem::take(&mut *held))
            .unwrap_or_default()
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

    /// Reads one statement's structured PostgreSQL plan.
    ///
    /// This is deliberately separate from [`Self::execute`]. The generated
    /// EXPLAIN command must not become an ordinary result, history entry, or
    /// export, and keeping the boundary here makes that distinction visible at
    /// the only place that can send it to PostgreSQL.
    pub async fn explain(&self, sql: &str, analyze: bool) -> Result<String, Diagnostic> {
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
    /// This is the path an export takes. Nothing is buffered, so the memory a
    /// hundred-million-row export uses is the memory one row uses. The sink is
    /// called for each event in order and may fail, which stops the stream.
    pub async fn stream(
        &self,
        sql: &str,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
    ) -> Result<u64, StreamStop> {
        let stream = self
            .client
            .simple_query_raw(sql)
            .await
            .map_err(|err| StreamStop::Server(Box::new(from_query_error(&err, 1))))?;
        tokio::pin!(stream);

        let mut rows = 0u64;
        let mut seen_columns = false;

        while let Some(message) = stream.next().await {
            let message =
                message.map_err(|err| StreamStop::Server(Box::new(from_query_error(&err, 1))))?;
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
    pub async fn stream_with_parameters(
        &self,
        sql: &str,
        parameters: &crate::query::ParameterBindings,
        sink: &mut dyn FnMut(StreamEvent) -> std::io::Result<()>,
    ) -> Result<u64, StreamStop> {
        let bound = crate::query::discover_parameters(sql)
            .and_then(|template| template.bind(sql, parameters))
            .map_err(|error| {
                StreamStop::Server(Box::new(
                    error.diagnostic("binding named parameters for export"),
                ))
            })?;
        self.stream(&bound, sink).await
    }

    /// Executes every statement in a buffer, in order.
    ///
    /// Statements after a failure do not run. Results produced before the failure
    /// are kept and returned along with the diagnostic, because they happened.
    pub async fn execute(&self, sql: &str, row_cap: usize, job: JobId) -> Execution {
        tracing::debug!(
            target: "ignatius::query",
            "{}",
            crate::diagnostics::logging::statement_descriptor(job.0, sql)
        );
        self.execute_bound(sql, row_cap, job).await
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
        tracing::debug!(
            target: "ignatius::query",
            "{}",
            crate::diagnostics::logging::statement_descriptor(job.0, sql)
        );
        let started = Instant::now();
        let bound = match crate::query::discover_parameters(sql)
            .and_then(|template| template.bind(sql, parameters))
        {
            Ok(bound) => bound,
            Err(error) => {
                return Execution {
                    job,
                    statements: Vec::new(),
                    status: ExecutionStatus::Failed,
                    elapsed: started.elapsed(),
                    error: Some(error.diagnostic("binding named parameters for execution")),
                    transaction: self.transaction_state().await,
                };
            }
        };
        self.execute_bound(&bound, row_cap, job).await
    }

    async fn execute_bound(&self, sql: &str, row_cap: usize, job: JobId) -> Execution {
        let started = Instant::now();
        let parsed = statements::split(sql);
        let mut results = Vec::new();

        if parsed.is_empty() {
            return Execution {
                job,
                statements: results,
                status: ExecutionStatus::Succeeded,
                elapsed: started.elapsed(),
                error: None,
                transaction: self.transaction_state().await,
            };
        }

        for (index, statement) in parsed.iter().enumerate() {
            let _ = self.take_notices();
            let statement_started = Instant::now();
            match self.execute_one(&statement.text, row_cap).await {
                Ok((result_set, rows_affected)) => {
                    results.push(StatementResult {
                        result_set,
                        rows_affected,
                        elapsed: statement_started.elapsed(),
                        notices: self.take_notices(),
                    });
                }
                Err(err) => {
                    let diagnostic = from_query_error(&err, index + 1);
                    let status = if diagnostic.kind == DiagnosticKind::Cancelled {
                        ExecutionStatus::Cancelled
                    } else if err.is_closed() || self.is_closed() {
                        ExecutionStatus::ConnectionLost
                    } else {
                        ExecutionStatus::Failed
                    };
                    let transaction = if status == ExecutionStatus::ConnectionLost {
                        // Asking a closed connection would only produce another
                        // failure, and the honest answer is that we do not know.
                        crate::query::result::TransactionState::Unknown
                    } else {
                        self.transaction_state().await
                    };
                    return Execution {
                        job,
                        statements: results,
                        status,
                        elapsed: started.elapsed(),
                        error: Some(diagnostic),
                        transaction,
                    };
                }
            }
        }

        let transaction = self.transaction_state().await;
        Execution {
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
    ) -> Result<(Option<ResultSet>, Option<u64>), tokio_postgres::Error> {
        let stream = self.client.simple_query_raw(sql).await?;
        tokio::pin!(stream);

        let mut result_set: Option<ResultSet> = None;
        let mut rows_affected = None;

        while let Some(message) = stream.next().await {
            match message? {
                tokio_postgres::SimpleQueryMessage::RowDescription(columns) => {
                    let names = columns.iter().map(|c| c.name().to_owned()).collect();
                    result_set = Some(ResultSet::new(names, row_cap));
                }
                tokio_postgres::SimpleQueryMessage::Row(row) => {
                    let set = result_set.get_or_insert_with(|| ResultSet::new(Vec::new(), row_cap));
                    let cells = (0..row.len())
                        .map(|i| Cell::from_option(row.get(i)))
                        .collect();
                    // Rows past the cap are counted and dropped here, which is
                    // what keeps memory bounded on a large result.
                    set.push(cells);
                }
                tokio_postgres::SimpleQueryMessage::CommandComplete(count) => {
                    rows_affected = Some(count);
                }
                _ => {}
            }
        }

        // The simple-query protocol gives us faithful text values and column
        // names, but not type OIDs. Parse/describe is deliberately after the
        // result has arrived: it does not execute the statement, and a failure
        // here must never turn a successful query into a failed one.
        if let Some(names) = result_set
            .as_ref()
            .filter(|set| !set.columns.is_empty())
            .map(|set| set.columns.clone())
            && let Some(types) = self.describe_columns(sql, &names).await
            && let Some(set) = result_set.as_mut()
        {
            let _ = set.set_column_types(types);
        }

        Ok((result_set, rows_affected))
    }

    /// Describes one already-received result without replaying its SQL.
    ///
    /// The names are checked as well as the count because a description that is
    /// accepted against the wrong shape would make a truthful type label into a
    /// quiet lie. A failed prepare or mismatch is ordinary unavailability for
    /// the grid, not a query failure.
    async fn describe_columns(&self, sql: &str, names: &[String]) -> Option<Vec<Option<String>>> {
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
        Some(
            columns
                .iter()
                .map(|column| Some(column.type_().name().to_owned()))
                .collect(),
        )
    }
}

/// Opens a session, applying the resolved target's policy.
pub async fn connect(
    target: &ConnectionTarget,
    statement_timeout: Duration,
) -> Result<Session, Diagnostic> {
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

    let (client, notices, connection_lost) = match tls_config {
        None => establish(&config, tokio_postgres::NoTls, target).await?,
        Some(rustls_config) => {
            establish(
                &config,
                tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config),
                target,
            )
            .await?
        }
    };

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
        info,
        tls,
        notices,
        connection_lost,
    })
}

type Established = (Client, Arc<Mutex<Vec<Notice>>>, Arc<AtomicBool>);

/// Connects, then moves the connection onto a task that forwards notices.
async fn establish<T>(
    config: &tokio_postgres::Config,
    tls: T,
    target: &ConnectionTarget,
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

    let notices = Arc::new(Mutex::new(Vec::new()));
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
                    if let Ok(mut held) = task_notices.lock() {
                        held.push(Notice {
                            severity: notice.severity().to_owned(),
                            message: notice.message().to_owned(),
                            code: Some(notice.code().code().to_owned()),
                        });
                    }
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
mod tests {
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
}
