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
    sslmode: SslMode,
    notices: Arc<Mutex<Vec<Notice>>>,
    connection_lost: Arc<AtomicBool>,
}

/// A handle that can cancel the session's running statement from elsewhere.
///
/// Cancellation opens its own connection, which is why it can work while the
/// session is busy. It asks the server to cancel; the server decides.
#[derive(Clone)]
pub struct CancelHandle {
    token: tokio_postgres::CancelToken,
    sslmode: SslMode,
}

impl std::fmt::Debug for CancelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The driver's token is not Debug, and it holds connection details that
        // have no business appearing in a log line.
        f.debug_struct("CancelHandle")
            .field("sslmode", &self.sslmode)
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
        let result = match client_config(self.sslmode)? {
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
            sslmode: self.sslmode,
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

    /// Indexes on a relation.
    pub async fn indexes(
        &self,
        schema: &str,
        relation: &str,
    ) -> Result<Vec<crate::postgres::metadata::ObjectSummary>, Diagnostic> {
        crate::postgres::metadata::indexes(&self.client, schema, relation).await
    }

    /// Extensions installed in this database.
    pub async fn extensions(
        &self,
    ) -> Result<Vec<crate::postgres::metadata::ObjectSummary>, Diagnostic> {
        crate::postgres::metadata::extensions(&self.client).await
    }

    /// Executes every statement in a buffer, in order.
    ///
    /// Statements after a failure do not run. Results produced before the failure
    /// are kept and returned along with the diagnostic, because they happened.
    pub async fn execute(&self, sql: &str, row_cap: usize, job: JobId) -> Execution {
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
                    return Execution {
                        job,
                        statements: results,
                        status,
                        elapsed: started.elapsed(),
                        error: Some(diagnostic),
                    };
                }
            }
        }

        Execution {
            job,
            statements: results,
            status: ExecutionStatus::Succeeded,
            elapsed: started.elapsed(),
            error: None,
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

        Ok((result_set, rows_affected))
    }
}

/// Opens a session, applying the resolved target's policy.
pub async fn connect(
    target: &ConnectionTarget,
    statement_timeout: Duration,
) -> Result<Session, Diagnostic> {
    let config = build_config(target);
    let tls_config = client_config(target.sslmode)?;

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
        sslmode: target.sslmode,
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
