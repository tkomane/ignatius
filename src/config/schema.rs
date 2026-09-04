//! The configuration schema.
//!
//! This file is the single authority for what may appear in `config.toml`. The
//! documented schema in `docs/operations/local-development.md` and the output of
//! `ignatius config show` are both generated from these types, so they cannot
//! drift from what the code accepts.
//!
//! No secret value may ever appear here. Profiles (Feature 002) will store
//! references to credentials, never credentials.

use serde::{Deserialize, Serialize};

/// Schema version understood by this build.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Non-secret user configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Version of this file's schema. Written by the product, migrated on upgrade.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Presentation settings.
    #[serde(default)]
    pub ui: UiConfig,
    /// Query execution limits.
    #[serde(default)]
    pub query: QueryConfig,
    /// Connection defaults applied when a target does not say otherwise.
    #[serde(default)]
    pub connection: ConnectionConfig,
    /// What is kept about statements that have run.
    #[serde(default)]
    pub history: HistoryConfig,
    /// Named connections, by profile name.
    ///
    /// A profile holds where a database is and how it is classified. It never
    /// holds a password: the routes for those are a password file, the
    /// environment, the connection string, and the prompt.
    #[serde(default)]
    pub profiles: std::collections::BTreeMap<String, Profile>,
    /// Key bindings that replace the built-in ones, by action name.
    ///
    /// A name this build does not know, a key it cannot parse, or a binding
    /// that collides with another is an error rather than something to ignore.
    /// A key that silently does nothing is the worst outcome for a file whose
    /// whole purpose is to say what the keyboard does.
    #[serde(default)]
    pub keys: std::collections::BTreeMap<String, KeySpec>,
    /// How a short-lived cloud credential is obtained.
    ///
    /// Built-in providers cover Azure, AWS and Google Cloud. This section adds
    /// others, or replaces a built-in when a cloud changes its tool between our
    /// releases - which is the whole reason providers are configuration rather
    /// than code.
    #[serde(default)]
    pub auth: AuthConfig,
}

/// One or several keys bound to the same action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeySpec {
    /// A single key, as in `quit = "ctrl+q"`.
    One(String),
    /// Several, as in `run-buffer = ["ctrl+r", "f5"]`.
    Many(Vec<String>),
}

impl KeySpec {
    /// The keys, however they were written.
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Self::One(key) => vec![key.as_str()],
            Self::Many(keys) => keys.iter().map(String::as_str).collect(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            ui: UiConfig::default(),
            query: QueryConfig::default(),
            connection: ConnectionConfig::default(),
            history: HistoryConfig::default(),
            keys: std::collections::BTreeMap::new(),
            profiles: std::collections::BTreeMap::new(),
            auth: AuthConfig::default(),
        }
    }
}

/// Cloud identity providers defined by the user.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct AuthConfig {
    /// Providers by name. A name that matches a built-in replaces it.
    #[serde(default)]
    pub providers: std::collections::BTreeMap<String, ProviderConfig>,
}

/// One way of obtaining a short-lived credential.
///
/// This says which program to run and where the token is in what it prints.
/// Naming a program here means this client will execute it; that is inside the
/// user's own trust boundary, since anything able to write their configuration
/// can already run programs as them, but it is written down in the threat model
/// because the format did not previously have that reach.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProviderConfig {
    /// The program and its arguments. `{host}`, `{port}`, `{user}` and
    /// `{database}` are substituted whole. No shell is involved.
    pub command: Vec<String>,
    /// The JSON field holding the token. Omitted means the whole of stdout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_field: Option<String>,
    /// How long to wait for the program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// What to tell someone when it fails, such as the sign-in command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

impl ProviderConfig {
    /// The runtime provider this describes.
    #[must_use]
    pub fn to_provider(&self, name: &str) -> crate::connection::cloud::Provider {
        use crate::connection::cloud::{DEFAULT_TIMEOUT, Extract, Provider};
        Provider {
            name: name.to_owned(),
            command: self.command.clone(),
            extract: self
                .json_field
                .clone()
                .map_or(Extract::Raw, Extract::JsonField),
            timeout: self
                .timeout_seconds
                .map_or(DEFAULT_TIMEOUT, std::time::Duration::from_secs),
            remedy: self.remedy.clone().unwrap_or_default(),
        }
    }

    /// Refuses a definition that could not work.
    pub fn validate(&self, name: &str) -> Result<(), crate::diagnostics::Diagnostic> {
        use crate::diagnostics::{Diagnostic, DiagnosticKind};
        if self.command.is_empty() || self.command[0].trim().is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                format!("the {name:?} authentication provider has no program to run"),
                "reading [auth.providers] from the configuration file",
            )
            .next_action(
                "give it `command = [\"program\", \"argument\"]`, starting with the program",
            ));
        }
        Ok(())
    }
}

/// A named connection.
///
/// Unknown fields are kept rather than rejected by serde, so that the ones
/// people will actually try - `password` above all - can be refused by name with
/// an answer, instead of with "unknown field".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Profile {
    /// Server host name, address, or socket directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Server port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Database name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dbname: Option<String>,
    /// Role name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Transport protection: disable, prefer, require, verify-ca, verify-full.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sslmode: Option<String>,
    /// How this database is classified, for example production.
    ///
    /// This is the reason profiles exist: a classification that is remembered
    /// cannot be forgotten, and forgetting it is what puts a write on the wrong
    /// database.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// Ask the server to refuse writes for sessions on this profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    /// A short line describing what this connection is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The cloud identity provider that supplies this connection's credential.
    ///
    /// A name, not a secret: `entra`, `aws`, `gcp`, or one defined under
    /// `[auth.providers]`. The credential itself is fetched at connection time
    /// and is never written anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    /// Anything else the file said, kept so it can be refused by name.
    #[serde(flatten)]
    pub other: std::collections::BTreeMap<String, toml::Value>,
}

/// The fields a profile understands, for error messages and documentation.
pub const PROFILE_FIELDS: &[&str] = &[
    "host",
    "port",
    "dbname",
    "user",
    "sslmode",
    "environment",
    "read-only",
    "description",
    "auth",
];

impl Profile {
    /// Refuses a profile that names something this build will not do.
    ///
    /// Two kinds of refusal, because they deserve different answers: a field
    /// that would hold a secret is refused with the routes that exist, and
    /// anything else unknown is refused with the fields that do.
    pub fn validate(&self, name: &str) -> Result<(), crate::diagnostics::Diagnostic> {
        use crate::diagnostics::{Diagnostic, DiagnosticKind};
        // A field that would hold a secret is looked for across the whole
        // profile before anything else. Reporting whichever unknown field
        // happened to sort first would bury the one that matters.
        let secret = |key: &String| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "password" | "pgpassword" | "sslpassword" | "secret"
            )
        };
        if self.other.keys().any(secret) {
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                format!("profile {name:?} tries to hold a password"),
                "reading connection profiles",
            )
            .likely_cause("a profile says where a database is, never how to prove who you are")
            .next_action(
                "put the password in a password file (~/.pgpass), in the environment, or \
                 type it when the client asks",
            ));
        }

        if self
            .other
            .keys()
            .any(|key| key.eq_ignore_ascii_case("passfile"))
        {
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                format!("profile {name:?} names a password file"),
                "reading connection profiles",
            )
            .likely_cause("this release chooses the password file in one place, not per profile")
            .next_action("set PGPASSFILE, or use the default ~/.pgpass"));
        }

        if let Some(key) = self.other.keys().next() {
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                format!("profile {name:?} has a field this build does not know: {key}"),
                "reading connection profiles",
            )
            .likely_cause(format!(
                "a profile understands: {}",
                PROFILE_FIELDS.join(", ")
            ))
            .next_action("correct the field name, or remove it"));
        }

        Ok(())
    }
}

/// What is kept about statements that have run.
///
/// The history is a file of SQL on a disk. It is on by default because retyping
/// a query is the most common small misery of a terminal client, and every
/// control over it is explicit: turn it off here, pause it for a session with
/// `--no-history`, or clear it with `ignatius history clear`. A statement that
/// mentions a credential is never written whatever this says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct HistoryConfig {
    /// Whether statements are recorded at all.
    #[serde(default = "default_history_enabled")]
    pub enabled: bool,
    /// How many entries to keep. The oldest are dropped first.
    #[serde(default = "default_history_max_entries")]
    pub max_entries: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_history_enabled(),
            max_entries: default_history_max_entries(),
        }
    }
}

const fn default_history_enabled() -> bool {
    true
}

const fn default_history_max_entries() -> usize {
    1_000
}

const fn default_schema_version() -> u32 {
    // A file with no schema_version is treated as version 0 and migrated.
    0
}

/// Presentation settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct UiConfig {
    /// Which built-in theme to use.
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Whether to draw with Unicode box characters or plain ASCII.
    #[serde(default)]
    pub glyphs: GlyphMode,
    /// Whether to emit colour.
    #[serde(default)]
    pub color: ColorMode,
    /// Mouse reporting. Off by default so terminal text selection keeps working.
    #[serde(default)]
    pub mouse: bool,
    /// Replaces animated feedback with static text.
    #[serde(default)]
    pub reduced_motion: bool,
    /// Whether typing may open automatic schema-completion menus.
    #[serde(default = "default_completion")]
    pub completion: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            glyphs: GlyphMode::Auto,
            color: ColorMode::Auto,
            mouse: false,
            reduced_motion: false,
            completion: default_completion(),
        }
    }
}

const fn default_completion() -> bool {
    true
}

/// Built-in themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeChoice {
    /// Tuned for dark terminals.
    #[default]
    Dark,
    /// Designed for light terminals, not an inverted dark palette.
    Light,
    /// Maximum contrast.
    HighContrast,
}

impl ThemeChoice {
    /// Accepted values, for help text and error messages.
    pub const ACCEPTED: &'static [&'static str] = &["dark", "light", "high-contrast"];
}

/// Whether to draw with Unicode or ASCII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GlyphMode {
    /// Decide from the environment, falling back to ASCII when unsure.
    #[default]
    Auto,
    /// Always use Unicode box drawing.
    Unicode,
    /// Always use ASCII.
    Ascii,
    /// Use a patched Nerd Font, so icons are drawn as well.
    ///
    /// Opt-in, because whether the terminal's font carries the icon range
    /// cannot be detected from inside the terminal, and guessing wrong fills the
    /// screen with replacement characters.
    NerdFont,
}

impl GlyphMode {
    /// Accepted values, for help text and error messages.
    pub const ACCEPTED: &'static [&'static str] = &["auto", "unicode", "ascii", "nerd-font"];
}

/// Whether to emit colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMode {
    /// Honour `NO_COLOR`, `TERM=dumb`, and whether the stream is a terminal.
    #[default]
    Auto,
    /// Always emit colour.
    Always,
    /// Never emit colour.
    Never,
}

impl ColorMode {
    /// Accepted values, for help text and error messages.
    pub const ACCEPTED: &'static [&'static str] = &["auto", "always", "never"];
}

/// Query execution limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct QueryConfig {
    /// How many rows the interactive result grid keeps in memory.
    ///
    /// Reaching this limit is shown as explicit truncation, never hidden. Export
    /// streams to disk and is not affected by this cap.
    #[serde(default = "default_max_buffered_rows")]
    pub max_buffered_rows: usize,
    /// `statement_timeout` applied to the session, in milliseconds. Zero leaves
    /// the server default in place.
    #[serde(default)]
    pub statement_timeout_ms: u64,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            max_buffered_rows: default_max_buffered_rows(),
            statement_timeout_ms: 0,
        }
    }
}

const fn default_max_buffered_rows() -> usize {
    10_000
}

/// Connection defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ConnectionConfig {
    /// Seconds to wait for a connection before giving up.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_seconds: u64,
    /// `application_name` reported to the server.
    #[serde(default = "default_application_name")]
    pub application_name: String,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: default_connect_timeout(),
            application_name: default_application_name(),
        }
    }
}

const fn default_connect_timeout() -> u64 {
    10
}

fn default_application_name() -> String {
    crate::branding::DEFAULT_APPLICATION_NAME.to_owned()
}

/// A problem found while validating configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// Dotted path of the offending setting, for example `query.max-buffered-rows`.
    pub path: String,
    /// What is wrong, in plain language.
    pub message: String,
    /// What to do about it.
    pub suggestion: String,
}

impl Config {
    /// Checks values that the type system cannot express.
    ///
    /// Returns every issue found rather than stopping at the first, so one run of
    /// `config validate` fixes one round of edits.
    #[must_use]
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        if self.schema_version > CURRENT_SCHEMA_VERSION {
            issues.push(ValidationIssue {
                path: "schema-version".into(),
                message: format!(
                    "file uses schema version {} but this build understands {CURRENT_SCHEMA_VERSION}",
                    self.schema_version
                ),
                suggestion: "upgrade Ignatius, or point at an older configuration directory".into(),
            });
        }

        if self.query.max_buffered_rows == 0 {
            issues.push(ValidationIssue {
                path: "query.max-buffered-rows".into(),
                message: "zero would leave the result grid permanently empty".into(),
                suggestion: format!(
                    "use a positive value, for example {}",
                    default_max_buffered_rows()
                ),
            });
        } else if self.query.max_buffered_rows > 5_000_000 {
            issues.push(ValidationIssue {
                path: "query.max-buffered-rows".into(),
                message: "value is high enough to exhaust memory on a wide result".into(),
                suggestion: "keep it at or below 5000000 and use export for larger extracts".into(),
            });
        }

        if self.connection.connect_timeout_seconds == 0 {
            issues.push(ValidationIssue {
                path: "connection.connect-timeout-seconds".into(),
                message: "zero would wait forever on an unreachable host".into(),
                suggestion: format!(
                    "use a positive value, for example {}",
                    default_connect_timeout()
                ),
            });
        }

        if self.connection.application_name.trim().is_empty() {
            issues.push(ValidationIssue {
                path: "connection.application-name".into(),
                message: "an empty application name hides this session in pg_stat_activity".into(),
                suggestion: format!(
                    "use a non-empty name, for example {}",
                    default_application_name()
                ),
            });
        }

        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_current() {
        let config = Config::default();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(config.validate().is_empty(), "{:?}", config.validate());
        // Safe by default: mouse capture off keeps terminal selection working.
        assert!(!config.ui.mouse);
    }

    #[test]
    fn round_trips_through_toml_without_loss() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).expect("serialise");
        let parsed: Config = toml::from_str(&text).expect("parse");
        assert_eq!(config, parsed);
    }

    #[test]
    fn unknown_keys_are_rejected_rather_than_silently_ignored() {
        let err = toml::from_str::<Config>("schema_version = 1\nthemes = \"dark\"\n")
            .expect_err("unknown key must fail");
        assert!(err.to_string().contains("themes"), "{err}");

        let err =
            toml::from_str::<Config>("schema_version = 1\n[ui]\ntheme = \"dark\"\nunknown = 1\n")
                .expect_err("unknown nested key must fail");
        assert!(err.to_string().contains("unknown"), "{err}");
    }

    #[test]
    fn missing_schema_version_parses_as_version_zero_for_migration() {
        let config: Config = toml::from_str("[ui]\ntheme = \"light\"\n").expect("parse");
        assert_eq!(config.schema_version, 0);
        assert_eq!(config.ui.theme, ThemeChoice::Light);
    }

    #[test]
    fn validation_reports_every_problem_at_once() {
        let config = Config {
            schema_version: CURRENT_SCHEMA_VERSION,
            query: QueryConfig {
                max_buffered_rows: 0,
                statement_timeout_ms: 0,
            },
            connection: ConnectionConfig {
                connect_timeout_seconds: 0,
                application_name: "  ".into(),
            },
            ..Config::default()
        };
        let issues = config.validate();
        assert_eq!(issues.len(), 3, "{issues:#?}");
        for issue in &issues {
            assert!(
                !issue.suggestion.is_empty(),
                "every issue needs a next action"
            );
        }
    }

    #[test]
    fn a_newer_schema_version_is_reported_not_guessed_at() {
        let config = Config {
            schema_version: CURRENT_SCHEMA_VERSION + 5,
            ..Config::default()
        };
        let issues = config.validate();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].path, "schema-version");
    }

    #[test]
    fn the_nerd_font_tier_round_trips_and_is_opt_in() {
        // Adding a tier must not invalidate a configuration written before it
        // existed, so this is an additive change with no migration.
        let config: Config = toml::from_str("[ui]\nglyphs = \"nerd-font\"\n").expect("parse");
        assert_eq!(config.ui.glyphs, GlyphMode::NerdFont);
        assert_eq!(UiConfig::default().glyphs, GlyphMode::Auto, "never assumed");
        let text = toml::to_string(&config).expect("serialise");
        assert!(text.contains("glyphs = \"nerd-font\""), "{text}");
    }

    #[test]
    fn enum_values_are_kebab_case_in_the_file() {
        let text = toml::to_string(&Config {
            ui: UiConfig {
                theme: ThemeChoice::HighContrast,
                ..UiConfig::default()
            },
            ..Config::default()
        })
        .expect("serialise");
        assert!(text.contains("theme = \"high-contrast\""), "{text}");
        for accepted in ThemeChoice::ACCEPTED {
            assert!(!accepted.contains('_'), "documented values stay kebab-case");
        }
    }
}
