//! Resolving where and how to connect.
//!
//! Precedence, highest first. This order is a contract, tested below and
//! published in `docs/support/compatibility.md`:
//!
//! 1. Explicit command-line arguments (`--host`, `--port`, ...)
//! 2. A connection string given as the target (URI or keyword/value)
//! 3. A named service from `pg_service.conf`
//! 4. PostgreSQL environment variables (`PGHOST`, `PGPORT`, ...)
//! 5. Built-in defaults
//!
//! The password has its own order, because it comes from different places:
//! the connection string, then `PGPASSWORD`, then `.pgpass`. A password file is
//! consulted only when nothing else supplied one.
//!
//! Parameters that libpq defines but this build does not implement are never
//! ignored in silence. Security-relevant ones fail the connection; the rest are
//! reported as notes. Which is which is decided by whether ignoring the parameter
//! could weaken confidentiality or identity checking.

use crate::config::ConnectionConfig;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use secrecy::SecretString;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// Transport-level protection requested for the connection.
///
/// These follow libpq's names. `Require` encrypts but does not check identity,
/// which is a different guarantee from `VerifyFull`, and the interface never
/// presents them as equivalent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SslMode {
    /// Never use TLS.
    Disable,
    /// Use TLS if the server offers it. No identity guarantee.
    Prefer,
    /// Require TLS. Encryption only: the certificate is not verified.
    Require,
    /// Require TLS and verify the certificate chain, but not the host name.
    VerifyCa,
    /// Require TLS, verify the chain and the host name.
    #[default]
    VerifyFull,
}

impl SslMode {
    /// Parses a libpq `sslmode` value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "disable" => Ok(Self::Disable),
            "allow" | "prefer" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            "verify-ca" => Ok(Self::VerifyCa),
            "verify-full" => Ok(Self::VerifyFull),
            other => Err(format!(
                "unknown sslmode {other:?}; expected one of disable, allow, prefer, require, verify-ca, verify-full"
            )),
        }
    }

    /// The libpq name, for display and for round-tripping.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }

    /// Plain-language description of what this mode actually guarantees.
    #[must_use]
    pub const fn guarantee(self) -> &'static str {
        match self {
            Self::Disable => "no encryption",
            Self::Prefer => "encryption if the server offers it, no identity check",
            Self::Require => "encryption, no identity check",
            Self::VerifyCa => "encryption and a trusted certificate, host name not checked",
            Self::VerifyFull => "encryption, a trusted certificate, and a matching host name",
        }
    }

    /// Whether TLS must be established for the connection to proceed.
    #[must_use]
    pub const fn requires_tls(self) -> bool {
        matches!(self, Self::Require | Self::VerifyCa | Self::VerifyFull)
    }

    /// Whether the server's certificate is checked against a trust store.
    #[must_use]
    pub const fn verifies_certificate(self) -> bool {
        matches!(self, Self::VerifyCa | Self::VerifyFull)
    }
}

/// How the connection reaches the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    /// A TCP host name or IP address.
    Tcp(String),
    /// A Unix-domain socket directory.
    Socket(PathBuf),
}

impl Host {
    /// Whether this host is on the local machine, which decides the default
    /// `sslmode`. Loopback is decided by the literal address, never by guessing
    /// from a name.
    #[must_use]
    pub fn is_local(&self) -> bool {
        match self {
            Self::Socket(_) => true,
            Self::Tcp(host) => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
        }
    }

    /// Display form used in the status bar and diagnostics.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Tcp(host) => host.clone(),
            Self::Socket(path) => path.display().to_string(),
        }
    }
}

/// How a connection is classified by the user. Never inferred from a host name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Environment {
    /// No classification was given.
    #[default]
    Unclassified,
    /// A database on this machine.
    Local,
    /// A shared development database.
    Development,
    /// A test database.
    Test,
    /// A staging database.
    Staging,
    /// Production. Marked persistently and in words.
    Production,
    /// A user-defined classification.
    Custom(String),
}

impl Environment {
    /// Parses a classification name.
    pub fn parse(value: &str) -> Result<Self, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("environment name cannot be empty".to_owned());
        }
        Ok(match trimmed.to_ascii_lowercase().as_str() {
            "local" => Self::Local,
            "development" | "dev" => Self::Development,
            "test" => Self::Test,
            "staging" => Self::Staging,
            "production" | "prod" => Self::Production,
            _ => Self::Custom(trimmed.to_owned()),
        })
    }

    /// Short label shown in the status bar. Always text, never colour alone.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Unclassified => "UNCLASSIFIED".to_owned(),
            Self::Local => "LOCAL".to_owned(),
            Self::Development => "DEV".to_owned(),
            Self::Test => "TEST".to_owned(),
            Self::Staging => "STAGING".to_owned(),
            Self::Production => "PROD".to_owned(),
            Self::Custom(name) => name.to_ascii_uppercase(),
        }
    }

    /// Whether this classification is production.
    #[must_use]
    pub const fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }
}

/// A snapshot of the PostgreSQL environment variables.
///
/// Captured explicitly rather than read at the point of use, so resolution is a
/// pure function and the test suite never mutates process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSnapshot {
    /// Variables that were set, keyed by name.
    pub values: BTreeMap<String, String>,
    /// Whether this came from the real process environment.
    ///
    /// A synthetic snapshot never falls back to files in a real home directory,
    /// so a test cannot accidentally read the developer's own `.pgpass`.
    pub from_process: bool,
}

impl EnvSnapshot {
    /// Variables read from the process environment.
    #[must_use]
    pub fn from_process() -> Self {
        const KEYS: &[&str] = &[
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
        ];
        let mut values = BTreeMap::new();
        for key in KEYS {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                values.insert((*key).to_owned(), value);
            }
        }
        Self {
            values,
            from_process: true,
        }
    }

    /// Builds a snapshot from pairs, for tests and for `doctor`.
    #[must_use]
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            values: pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            from_process: false,
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Names of the variables that were set, for `doctor`. Values are never shown:
    /// one of them is a password.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.values.keys().map(String::as_str).collect()
    }
}

/// Explicit command-line connection arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectionArgs {
    /// `--host`
    pub host: Option<String>,
    /// `--port`
    pub port: Option<u16>,
    /// `--dbname`
    pub dbname: Option<String>,
    /// `--username`
    pub username: Option<String>,
    /// `--sslmode`
    pub sslmode: Option<SslMode>,
    /// `--environment`
    pub environment: Option<Environment>,
    /// `--read-only`
    pub read_only: bool,
    /// `--auth`: the cloud identity provider that supplies the credential.
    pub auth: Option<String>,
}

/// Copying a password on purpose.
///
/// `SecretString` is not `Clone`, which is the right default: a secret should
/// not be duplicated by habit. Where a second connection genuinely needs the
/// same credential - the object tree's - this says so at the call site.
trait CloneSecret {
    /// A copy of this secret, if there is one.
    fn clone_secret(&self) -> Option<SecretString>;
}

impl CloneSecret for Option<SecretString> {
    fn clone_secret(&self) -> Option<SecretString> {
        self.as_ref().map(|secret| {
            SecretString::from(secrecy::ExposeSecret::expose_secret(secret).to_owned())
        })
    }
}

/// A note about the resolved target that the user should see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionNote {
    /// Short subject, for example the parameter name.
    pub subject: String,
    /// What the note says.
    pub message: String,
}

/// A fully resolved connection target.
///
/// Deliberately not `Clone`. A value holding a password should be moved, not
/// copied around by habit; where a second target is genuinely needed, it is
/// built explicitly by [`ConnectionTarget::with_password`], which says in its
/// name what it is doing with the secret.
#[derive(Debug)]
pub struct ConnectionTarget {
    /// Where to connect.
    pub host: Host,
    /// TCP port, also used to name the socket file.
    pub port: u16,
    /// Database name.
    pub database: String,
    /// Role name.
    pub user: String,
    /// Password, when one was supplied. Never logged or displayed.
    pub password: Option<SecretString>,
    /// Requested transport protection.
    pub sslmode: SslMode,
    /// An explicit trust root, replacing the operating system's.
    pub root_cert: Option<std::path::PathBuf>,
    /// A client certificate to present.
    pub client_cert: Option<std::path::PathBuf>,
    /// The key for that certificate.
    pub client_key: Option<std::path::PathBuf>,
    /// `application_name` reported to the server.
    pub application_name: String,
    /// How long to wait for the connection.
    pub connect_timeout: Duration,
    /// User-declared classification of this database.
    pub environment: Environment,
    /// Whether the session should be made read-only at the server.
    ///
    /// This is a real control, unlike the advisory statement classification:
    /// the server refuses writes itself.
    pub read_only: bool,
    /// Notes worth showing, such as unsupported non-security parameters.
    pub notes: Vec<ResolutionNote>,
    /// The cloud identity provider that supplies this connection's credential.
    ///
    /// When this is set, the password is a short-lived token obtained by running
    /// a program, and the connection may not proceed without one: see
    /// `connection::cloud`. It is kept on the target rather than discarded after
    /// the fetch so that any path opening a connection can check it was done.
    pub auth: Option<String>,
}

impl ConnectionTarget {
    /// The same target with a password the user has just supplied.
    #[must_use]
    pub fn with_password(&self, password: SecretString) -> Self {
        self.copy_with(Some(password))
    }

    /// The same target, for the connection the object tree reads on.
    ///
    /// The same server, the same credentials, the same protection. Two things
    /// differ and both are deliberate: it names itself in `application_name` so
    /// the two connections can be told apart in `pg_stat_activity`, and it is
    /// read-only, because reading the catalogue is all it does.
    #[must_use]
    pub fn for_object_tree(&self) -> Self {
        let mut copy = self.copy_with(self.password.clone_secret());
        copy.application_name = format!("{} (objects)", copy.application_name);
        copy.read_only = true;
        copy
    }

    /// Copies every field, taking the password given.
    ///
    /// Written out field by field rather than derived, so that adding a field
    /// to the target forces a decision here about whether a copy should carry
    /// it. The target itself is deliberately not `Clone`: a value holding a
    /// password should be copied on purpose, through a method that says why.
    fn copy_with(&self, password: Option<SecretString>) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            database: self.database.clone(),
            user: self.user.clone(),
            password,
            sslmode: self.sslmode,
            root_cert: self.root_cert.clone(),
            client_cert: self.client_cert.clone(),
            client_key: self.client_key.clone(),
            application_name: self.application_name.clone(),
            connect_timeout: self.connect_timeout,
            environment: self.environment.clone(),
            read_only: self.read_only,
            notes: self.notes.clone(),
            auth: self.auth.clone(),
        }
    }

    /// A display string that is safe to log: it never contains a password.
    #[must_use]
    pub fn safe_display(&self) -> String {
        format!(
            "{}@{}:{}/{}",
            self.user,
            self.host.display(),
            self.port,
            self.database
        )
    }
}

/// Default PostgreSQL port.
pub const DEFAULT_PORT: u16 = 5432;

/// Parameters that are understood and applied.
const SUPPORTED_KEYS: &[&str] = &[
    "service",
    "passfile",
    "host",
    "port",
    "dbname",
    "user",
    "password",
    "sslmode",
    "sslrootcert",
    "sslcert",
    "sslkey",
    "application_name",
    "connect_timeout",
];

/// Parameters whose silent omission could weaken confidentiality or identity
/// checking. Supplying one fails rather than proceeding with weaker protection.
const SECURITY_KEYS: &[&str] = &[
    "sslcrl",
    "sslcrldir",
    "sslpassword",
    "requiressl",
    "channel_binding",
    "gssencmode",
    "krbsrvname",
    "requirepeer",
    "sslsni",
];

/// Resolves a connection target from every source, in precedence order.
pub fn resolve(
    target: Option<&str>,
    args: &ConnectionArgs,
    env: &EnvSnapshot,
    config: &ConnectionConfig,
) -> Result<ConnectionTarget, Diagnostic> {
    let mut notes = Vec::new();
    let parsed = match target {
        Some(text) => parse_connection_string(text)?,
        None => BTreeMap::new(),
    };

    // A named service contributes parameters below the connection string and
    // above the environment. It is resolved first so its values can be checked
    // for the same unsupported parameters as anything else.
    let service_name = parsed
        .get("service")
        .cloned()
        .or_else(|| env.get("PGSERVICE").map(str::to_owned));
    let service_values = match &service_name {
        Some(name) => {
            let path = env
                .get("PGSERVICEFILE")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    env.from_process
                        .then(crate::connection::service::default_path)
                        .flatten()
                });
            let values = crate::connection::service::resolve(name, path.as_deref())?;
            notes.push(ResolutionNote {
                subject: "service".into(),
                message: format!("using service {name:?} ({} parameter(s))", values.len()),
            });
            values
        }
        None => BTreeMap::new(),
    };

    reject_unsupported_security_parameters(&parsed, env)?;
    reject_unsupported_security_parameters(&service_values, &EnvSnapshot::default())?;
    collect_unsupported_notes(&parsed, &mut notes);
    collect_unsupported_notes(&service_values, &mut notes);

    // Looks a parameter up through the layers below the command line.
    let layered = |key: &str, variable: &str| -> Option<String> {
        parsed
            .get(key)
            .cloned()
            .or_else(|| service_values.get(key).cloned())
            .or_else(|| env.get(variable).map(str::to_owned))
    };

    let host_value = args.host.clone().or_else(|| layered("host", "PGHOST"));

    let host = match host_value {
        Some(value) if value.starts_with('/') => Host::Socket(PathBuf::from(value)),
        Some(value) => Host::Tcp(value),
        None => Host::Tcp("localhost".to_owned()),
    };

    if matches!(host, Host::Socket(_)) && !crate::platform::supports_unix_sockets() {
        return Err(Diagnostic::new(
            DiagnosticKind::Config,
            "Unix-domain sockets are not available on this platform",
            "resolving the connection target",
        )
        .likely_cause(format!(
            "a socket directory was requested on {}",
            crate::platform::name()
        ))
        .next_action("connect over TCP instead, for example --host localhost --port 5432"));
    }

    let port = match args.port {
        Some(port) => port,
        None => {
            let text = layered("port", "PGPORT");
            match text {
                Some(value) => value.trim().parse::<u16>().map_err(|_| {
                    Diagnostic::new(
                        DiagnosticKind::Config,
                        format!("{value:?} is not a valid port"),
                        "resolving the connection target",
                    )
                    .next_action("use a number between 1 and 65535")
                })?,
                None => DEFAULT_PORT,
            }
        }
    };

    let user = args
        .username
        .clone()
        .or_else(|| layered("user", "PGUSER"))
        .or_else(default_user)
        .unwrap_or_else(|| "postgres".to_owned());

    let database = args
        .dbname
        .clone()
        .or_else(|| layered("dbname", "PGDATABASE"))
        .unwrap_or_else(|| user.clone());

    let mut password = parsed
        .get("password")
        .cloned()
        .or_else(|| service_values.get("password").cloned())
        .or_else(|| {
            env.get("PGPASSWORD")
                .inspect(|_| {
                    // Consumed, never displayed, and the safer route is offered once.
                    notes.push(ResolutionNote {
                        subject: "PGPASSWORD".into(),
                        message: "using the password from the environment; it is visible to \
                              other processes on this machine. A password file is safer."
                            .into(),
                    });
                })
                .map(str::to_owned)
        })
        .map(SecretString::from);

    let sslmode = match args.sslmode {
        Some(mode) => mode,
        None => {
            let text = layered("sslmode", "PGSSLMODE");
            match text {
                Some(value) => SslMode::parse(&value).map_err(|message| {
                    Diagnostic::new(
                        DiagnosticKind::Config,
                        message,
                        "resolving the connection target",
                    )
                    .next_action("use verify-full for remote databases")
                })?,
                None => default_sslmode(&host, &mut notes),
            }
        }
    };

    let certificate_path = |key: &str, variable: &str| -> Option<std::path::PathBuf> {
        layered(key, variable).map(std::path::PathBuf::from)
    };
    let root_cert = certificate_path("sslrootcert", "PGSSLROOTCERT");
    let client_cert = certificate_path("sslcert", "PGSSLCERT");
    let client_key = certificate_path("sslkey", "PGSSLKEY");

    let application_name =
        layered("application_name", "PGAPPNAME").unwrap_or_else(|| config.application_name.clone());

    let connect_timeout = layered("connect_timeout", "PGCONNECT_TIMEOUT")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or_else(
            || Duration::from_secs(config.connect_timeout_seconds),
            Duration::from_secs,
        );

    // A password file is the last resort and the safest of the routes: it keeps
    // the password off the command line, out of the environment, and out of
    // shell history. It is only read when nothing else supplied one.
    if password.is_none() {
        let path = parsed
            .get("passfile")
            .map(std::path::PathBuf::from)
            .or_else(|| env.get("PGPASSFILE").map(std::path::PathBuf::from))
            .or_else(|| {
                env.from_process
                    .then(crate::connection::passfile::default_path)
                    .flatten()
            });
        if let Some(path) = path {
            let lookup =
                crate::connection::passfile::lookup(&path, &host.display(), port, &database, &user);
            if let Some(note) = lookup.note() {
                notes.push(ResolutionNote {
                    subject: "password file".into(),
                    message: note,
                });
            }
            password = lookup.password().cloned();
        }
    }

    Ok(ConnectionTarget {
        host,
        port,
        database,
        user,
        password,
        sslmode,
        root_cert,
        client_cert,
        client_key,
        application_name,
        connect_timeout,
        environment: args.environment.clone().unwrap_or_default(),
        read_only: args.read_only,
        notes,
        // Resolution says which provider was asked for; it never runs it. The
        // credential is fetched afterwards, by a step that can be async and can
        // refuse the transport first.
        auth: args.auth.clone(),
    })
}

/// Chooses a default `sslmode` from where the server is.
///
/// A remote database defaults to full verification. A local one defaults to
/// `prefer`, because local sockets and loopback are not exposed to the network
/// and requiring certificates there would only teach users to turn checks off.
fn default_sslmode(host: &Host, notes: &mut Vec<ResolutionNote>) -> SslMode {
    if host.is_local() {
        SslMode::Prefer
    } else {
        notes.push(ResolutionNote {
            subject: "sslmode".into(),
            message: "defaulting to verify-full for a remote host".into(),
        });
        SslMode::VerifyFull
    }
}

fn default_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|v| !v.is_empty())
}

fn reject_unsupported_security_parameters(
    parsed: &BTreeMap<String, String>,
    env: &EnvSnapshot,
) -> Result<(), Diagnostic> {
    let mut offenders: Vec<String> = parsed
        .keys()
        .filter(|k| SECURITY_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    for (key, name) in [
        ("PGREQUIRESSL", "requiressl"),
        ("PGCHANNELBINDING", "channel_binding"),
        ("PGGSSENCMODE", "gssencmode"),
    ] {
        if env.get(key).is_some() {
            offenders.push(format!("{name} (from {key})"));
        }
    }
    if offenders.is_empty() {
        return Ok(());
    }
    Err(Diagnostic::new(
        DiagnosticKind::Config,
        format!(
            "{} security parameter(s) are not supported yet: {}",
            offenders.len(),
            offenders.join(", ")
        ),
        "resolving the connection target",
    )
    .likely_cause(
        "this release supports password and SCRAM authentication with system trust roots only",
    )
    .next_action(
        "remove the parameter, or keep using psql for this connection until client certificates \
         are supported. Ignoring it would give you weaker protection than you asked for.",
    ))
}

fn collect_unsupported_notes(parsed: &BTreeMap<String, String>, notes: &mut Vec<ResolutionNote>) {
    for key in parsed.keys() {
        if !SUPPORTED_KEYS.contains(&key.as_str()) && !SECURITY_KEYS.contains(&key.as_str()) {
            notes.push(ResolutionNote {
                subject: key.clone(),
                message: "parameter is not applied by this release".into(),
            });
        }
    }
}

/// Parses either a `postgres://` URI or a libpq keyword/value string.
pub fn parse_connection_string(input: &str) -> Result<BTreeMap<String, String>, Diagnostic> {
    let trimmed = input.trim();
    if trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://") {
        parse_uri(trimmed)
    } else if trimmed.contains('=') {
        parse_keyword_value(trimmed)
    } else if trimmed.is_empty() {
        Ok(BTreeMap::new())
    } else {
        // A bare word is the database name, matching psql's positional argument.
        Ok(BTreeMap::from([("dbname".to_owned(), trimmed.to_owned())]))
    }
}

fn parse_uri(uri: &str) -> Result<BTreeMap<String, String>, Diagnostic> {
    let mut out = BTreeMap::new();
    let rest = uri
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default();

    let (authority_and_path, query) = match rest.split_once('?') {
        Some((left, right)) => (left, Some(right)),
        None => (rest, None),
    };
    let (authority, path) = match authority_and_path.split_once('/') {
        Some((left, right)) => (left, Some(right)),
        None => (authority_and_path, None),
    };

    let host_part = match authority.rsplit_once('@') {
        Some((userinfo, host_part)) => {
            let (user, password) = match userinfo.split_once(':') {
                Some((u, p)) => (u, Some(p)),
                None => (userinfo, None),
            };
            if !user.is_empty() {
                out.insert("user".to_owned(), percent_decode(user)?);
            }
            if let Some(password) = password {
                out.insert("password".to_owned(), percent_decode(password)?);
            }
            host_part
        }
        None => authority,
    };

    if !host_part.is_empty() {
        // Bracketed IPv6 literal, for example [::1]:5432.
        if let Some(close) = host_part.strip_prefix('[').and_then(|r| r.find(']')) {
            out.insert("host".to_owned(), host_part[1..=close].to_owned());
            if let Some(port) = host_part[close + 2..].strip_prefix(':') {
                out.insert("port".to_owned(), port.to_owned());
            }
        } else if host_part.matches(':').count() == 1
            && let Some((host, port)) = host_part.rsplit_once(':')
        {
            if !host.is_empty() {
                out.insert("host".to_owned(), percent_decode(host)?);
            }
            out.insert("port".to_owned(), port.to_owned());
        } else {
            // More than one colon and no brackets: an unbracketed IPv6 literal.
            // Splitting on the last colon would invent a port number.
            out.insert("host".to_owned(), percent_decode(host_part)?);
        }
    }

    if let Some(path) = path.filter(|p| !p.is_empty()) {
        out.insert("dbname".to_owned(), percent_decode(path)?);
    }

    if let Some(query) = query {
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            out.insert(key.to_ascii_lowercase(), percent_decode(value)?);
        }
    }

    Ok(out)
}

/// Parses `key=value` pairs, honouring single quotes and backslash escapes.
fn parse_keyword_value(input: &str) -> Result<BTreeMap<String, String>, Diagnostic> {
    let mut out = BTreeMap::new();
    let bytes = input.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let key = input[key_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if key.is_empty() || bytes.get(i) != Some(&b'=') {
            return Err(malformed_connection_string(input, "expected key=value"));
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        let mut value = String::new();
        if bytes.get(i) == Some(&b'\'') {
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() => {
                        value.push(bytes[i + 1] as char);
                        i += 2;
                    }
                    b'\'' => {
                        i += 1;
                        closed = true;
                        break;
                    }
                    _ => {
                        let next = next_boundary(input, i);
                        value.push_str(&input[i..next]);
                        i = next;
                    }
                }
            }
            if !closed {
                return Err(malformed_connection_string(
                    input,
                    "unterminated quoted value",
                ));
            }
        } else {
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    value.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                let next = next_boundary(input, i);
                value.push_str(&input[i..next]);
                i = next;
            }
        }
        out.insert(key, value);
    }
    Ok(out)
}

fn next_boundary(input: &str, i: usize) -> usize {
    let mut next = i + 1;
    while next < input.len() && !input.is_char_boundary(next) {
        next += 1;
    }
    next.min(input.len())
}

fn malformed_connection_string(input: &str, reason: &str) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Config,
        format!("connection string could not be parsed: {reason}"),
        "reading the connection string",
    )
    .likely_cause("the string is neither a postgres:// URI nor libpq key=value pairs")
    .next_action("use postgres://user@host:5432/dbname, or host=... port=... dbname=...")
    .technical("Input", crate::diagnostics::redact_text(input))
}

fn percent_decode(input: &str) -> Result<String, Diagnostic> {
    if !input.contains('%') {
        return Ok(input.to_owned());
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = input
                .get(i + 1..i + 3)
                .ok_or_else(|| malformed_connection_string(input, "truncated percent-escape"))?;
            let value = u8::from_str_radix(hex, 16)
                .map_err(|_| malformed_connection_string(input, "invalid percent-escape"))?;
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out)
        .map_err(|_| malformed_connection_string(input, "escape is not valid UTF-8"))
}

#[cfg(test)]
mod tests;
