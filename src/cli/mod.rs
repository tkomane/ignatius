//! The command tree, its output contracts, and exit codes.
//!
//! The non-interactive surface is a contract, not a convenience: it never
//! prompts when it is not attached to a terminal, keeps data on stdout and
//! diagnostics on stderr, and returns documented exit codes.

pub mod interactive;
pub mod output;
pub mod plain;
pub mod prompt;

use crate::ExitCode;
use crate::branding;
use crate::config::{self, Paths};
use crate::connection::{ConnectionArgs, EnvSnapshot, Environment, SslMode};
use crate::diagnostics::doctor;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use clap::{Args, CommandFactory, Parser, Subcommand};
use output::{Format, OutputOptions};
use secrecy::SecretString;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

/// A terminal-native PostgreSQL workbench.
#[derive(Debug, Parser)]
#[command(
    name = branding::BINARY_NAME,
    version = branding::VERSION,
    about = branding::TAGLINE,
    long_about = None,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Global options shared by every command.
    #[command(flatten)]
    pub global: GlobalArgs,

    /// The command to run. Without one, the interactive client opens.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Options that apply to every command.
#[derive(Debug, Args, Clone, Default)]
pub struct GlobalArgs {
    /// Use this directory for configuration instead of the platform default.
    #[arg(long, global = true, value_name = "PATH")]
    pub config_dir: Option<PathBuf>,

    /// When to use colour.
    #[arg(long, global = true, value_name = "WHEN")]
    pub color: Option<ColorChoice>,

    /// Which colour depth to use.
    ///
    /// `auto` detects from the environment. `truecolor` and `256` paint the
    /// theme's surfaces; `16` uses the basic palette and leaves surfaces
    /// unpainted. `NO_COLOR` and `TERM=dumb` still win over any of them.
    #[arg(long, global = true, value_name = "DEPTH")]
    pub color_depth: Option<config::ColorDepthChoice>,

    /// Use ASCII instead of Unicode box drawing.
    #[arg(long, global = true)]
    pub ascii: bool,

    /// Which glyphs to draw with.
    ///
    /// `nerd-font` adds icons and needs a patched font. It is never chosen
    /// automatically, because whether the terminal's font carries the icon range
    /// cannot be detected from inside the terminal.
    #[arg(long, global = true, value_name = "TIER")]
    pub glyphs: Option<GlyphChoice>,

    /// Plain output: no colour, no box drawing, no decoration.
    #[arg(long, global = true)]
    pub plain: bool,

    /// Show technical detail in errors.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Do not record statements in the history for this session.
    ///
    /// Configuration decides whether history is kept at all; this pauses it for
    /// one run without changing the file.
    #[arg(long, global = true)]
    pub no_history: bool,
}

/// Which glyphs may be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum GlyphChoice {
    /// Decide from the environment, falling back to ASCII when unsure.
    Auto,
    /// Box drawing and widely supported symbols.
    Unicode,
    /// ASCII only.
    Ascii,
    /// Icons from a patched Nerd Font.
    NerdFont,
}

impl GlyphChoice {
    const fn tier(self, unicode_capable: bool) -> crate::ui::GlyphTier {
        use crate::ui::GlyphTier;
        match self {
            Self::Auto => {
                if unicode_capable {
                    GlyphTier::Unicode
                } else {
                    GlyphTier::Ascii
                }
            }
            Self::Unicode => GlyphTier::Unicode,
            Self::Ascii => GlyphTier::Ascii,
            Self::NerdFont => GlyphTier::Nerd,
        }
    }
}

/// When colour may be emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum ColorChoice {
    /// Decide from the terminal and from `NO_COLOR`.
    Auto,
    /// Always.
    Always,
    /// Never.
    Never,
}

/// The commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open or test a connection.
    Connect {
        /// Connection target: a URI, a keyword string, or a database name.
        target: Option<String>,
        /// Test the connection and report each stage, without opening the client.
        #[arg(long)]
        check: bool,
        #[command(flatten)]
        connection: ConnectionOptions,
    },

    /// Execute SQL from an argument, a file, or standard input.
    Query {
        /// Connection target: a URI, a keyword string, or a database name.
        target: Option<String>,
        /// SQL to execute.
        #[arg(short = 'c', long, value_name = "SQL", conflicts_with = "file")]
        command: Option<String>,
        /// Read SQL from a file. Use `-` for standard input.
        #[arg(short = 'f', long, value_name = "PATH")]
        file: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Destination table for `--format insert`.
        #[arg(long, value_name = "TABLE")]
        insert_table: Option<String>,
        /// Read one named parameter from an environment variable.
        ///
        /// Repeat once per distinct `:name`, for example
        /// `--param-env customer_id=IGNATIUS_CUSTOMER_ID`. Values stay out of
        /// process arguments and are never printed by this command.
        #[arg(long = "param-env", value_name = "NAME=VARIABLE")]
        parameter_env: Vec<String>,
        /// Omit the header row.
        #[arg(long)]
        no_header: bool,
        /// Text used for SQL NULL in csv and tsv output.
        #[arg(long, value_name = "TEXT", default_value = "")]
        null: String,
        /// Maximum rows to hold in memory. Truncation is always reported.
        #[arg(long, value_name = "N")]
        max_rows: Option<usize>,
        /// Stream the result into a file instead of standard output.
        ///
        /// The entire result is not retained in memory. Individual values,
        /// rows and driver frames still use memory. An interrupted export
        /// leaves a `.partial` file and reports row progress and flush uncertainty.
        #[arg(short = 'o', long, value_name = "PATH")]
        output: Option<PathBuf>,
        /// Replace the output file if it already exists.
        #[arg(long)]
        force: bool,
        /// Allow a statement that writes when the target is classified as
        /// production.
        ///
        /// Without it, a production target refuses anything that is not a read.
        #[arg(long)]
        allow_write: bool,
        #[command(flatten)]
        connection: ConnectionOptions,
    },

    /// Inspect configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Check that this machine is set up correctly.
    Doctor {
        /// Emit the report as JSON.
        #[arg(long)]
        json: bool,
        /// Also test this connection target.
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
    },

    /// Print a shell completion script.
    Completion {
        /// The shell to generate for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Read or clear the statements that have run.
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Print version and build identity.
    Version {
        /// Include build identity and source revision.
        #[arg(long)]
        verbose: bool,
    },

    /// Render or export a schema entity-relationship (ER) diagram.
    Diagram(DiagramArgs),

    /// Generate deterministic SQL DDL scripts for a schema or database object.
    Ddl(DdlArgs),

    /// Inspect live server activity, lock blocker chains, and database health metrics.
    Activity(ActivityArgs),

    /// Audit database roles, privilege grants, and Row-Level Security (RLS) policies.
    Audit(AuditArgs),

    /// Compare schemas and generate reviewable, transactional SQL migration scripts.
    Diff(DiffArgs),

    /// Bulk import data into a table using streaming COPY FROM STDIN.
    Import(ImportArgs),

    /// Bulk export queries or tables using streaming COPY TO STDOUT.
    Export(ExportArgs),

    /// Back up a database or schema using pg_dump with safe credential handling.
    Backup(BackupArgs),

    /// Restore a database or schema from backup using pg_restore.
    Restore(RestoreArgs),
}

/// Arguments for the audit command.
#[derive(Debug, Args, Clone)]
pub struct AuditArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Filter tables, policies, and grants to one schema.
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Limit output to a single audit section: roles, rls, or grants.
    #[arg(long, value_enum)]
    pub section: Option<CliAuditSection>,
    /// Output format: table or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliActivityFormat::Table)]
    pub format: CliActivityFormat,
    /// Write the security audit report to a file instead of standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Audit section filter for the audit command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliAuditSection {
    /// Overview of database roles, attributes, and group memberships.
    Roles,
    /// Row-Level Security (RLS) posture and policy definitions.
    Rls,
    /// Tabular privilege grants across relations.
    Grants,
}

impl CliAuditSection {
    /// Maps CLI audit section to domain audit section.
    #[must_use]
    pub const fn into_domain(self) -> crate::audit::AuditSection {
        match self {
            Self::Roles => crate::audit::AuditSection::Roles,
            Self::Rls => crate::audit::AuditSection::RlsPolicies,
            Self::Grants => crate::audit::AuditSection::Grants,
        }
    }
}

/// Arguments for the diff command (`030-visual-schema-designer-and-migrations`).
#[derive(Debug, Args, Clone)]
pub struct DiffArgs {
    /// Connection target or source database: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Optional second connection target (e.g. staging or production database).
    #[arg(long, value_name = "TARGET")]
    pub target2: Option<String>,
    /// Source schema name (default: "public").
    #[arg(long, value_name = "SCHEMA", default_value = "public")]
    pub source_schema: String,
    /// Target schema name (e.g. "staging" when comparing within one database).
    #[arg(long, value_name = "SCHEMA")]
    pub target_schema: Option<String>,
    /// Output representation format: sql (default), tree, or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliDiffFormat::Sql)]
    pub format: CliDiffFormat,
    /// Generate migration statements without BEGIN and COMMIT wrapper.
    #[arg(long)]
    pub no_transaction: bool,
    /// Write the schema diff or migration script to a file instead of standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Output representation format for schema diffs and migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
#[clap(rename_all = "kebab-case")]
pub enum CliDiffFormat {
    /// Reviewable transactional SQL migration script (`BEGIN; ... COMMIT;`).
    #[default]
    Sql,
    /// Human-readable visual tree showing additions (+), modifications (~), and deletions (-).
    Tree,
    /// Machine-readable structured JSON report (`schema_version: 1`).
    Json,
}

/// Arguments for the activity command.
#[derive(Debug, Args, Clone)]
pub struct ActivityArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Display the hierarchical lock blocker tree instead of the backend table.
    #[arg(long)]
    pub locks: bool,
    /// Output format: table or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliActivityFormat::Table)]
    pub format: CliActivityFormat,
    /// Write the activity snapshot to a file instead of standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Output representation format for server activity and locks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliActivityFormat {
    /// Tabular or hierarchical text representation.
    Table,
    /// Machine-readable JSON snapshot.
    Json,
}

/// Arguments for the ddl command.
#[derive(Debug, Args, Clone)]
pub struct DdlArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Optional object name to inspect (e.g. table or view name).
    pub object_pos: Option<String>,
    /// Schema to inspect (defaults to "public").
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Database object name to inspect.
    #[arg(long, value_name = "OBJECT")]
    pub object: Option<String>,
    /// Include DROP statements before CREATE statements.
    #[arg(long)]
    pub drop: bool,
    /// Destination file to write DDL output to.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Output representation format for schema ER diagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliDiagramFormat {
    /// Terminal cards using Unicode box-drawing.
    Unicode,
    /// Terminal cards using ASCII.
    Ascii,
    /// Mermaid erDiagram syntax.
    Mermaid,
    /// Graphviz DOT syntax.
    Dot,
    /// Standalone vector SVG.
    Svg,
}

impl From<CliDiagramFormat> for crate::diagram::DiagramFormat {
    fn from(format: CliDiagramFormat) -> Self {
        match format {
            CliDiagramFormat::Unicode => Self::Unicode,
            CliDiagramFormat::Ascii => Self::Ascii,
            CliDiagramFormat::Mermaid => Self::Mermaid,
            CliDiagramFormat::Dot => Self::Dot,
            CliDiagramFormat::Svg => Self::Svg,
        }
    }
}

/// Arguments for the diagram command.
#[derive(Debug, Args, Clone)]
pub struct DiagramArgs {
    /// Connection target: a URI, a keyword string, or a database name.
    pub target: Option<String>,
    /// Schema to inspect (defaults to all non-system schemas).
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Anchor table for neighbourhood traversal.
    #[arg(short = 't', long, value_name = "TABLE")]
    pub table: Option<String>,
    /// Neighbourhood hop distance (1 or 2).
    #[arg(long, value_name = "HOPS", value_parser = clap::value_parser!(u8).range(1..=2))]
    pub hops: Option<u8>,
    /// Output representation format.
    #[arg(long, value_enum)]
    pub format: Option<CliDiagramFormat>,
    /// Destination file to write diagram output to.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// History sub-commands.
#[derive(Debug, Subcommand)]
pub enum HistoryAction {
    /// List recent statements, newest first.
    List {
        /// How many to show.
        #[arg(long, value_name = "N", default_value_t = 20)]
        limit: usize,
        /// Emit the entries as JSON, one object per line.
        #[arg(long)]
        json: bool,
    },
    /// Delete the history file.
    Clear {
        /// Required, because this cannot be undone.
        #[arg(long)]
        yes: bool,
    },
    /// Print where the history is kept.
    Path,
}

/// Connection options shared by `connect` and `query`.
#[derive(Debug, Args, Clone, Default)]
pub struct ConnectionOptions {
    /// Server host name, address, or socket directory.
    #[arg(short = 'H', long, value_name = "HOST")]
    pub host: Option<String>,
    /// Server port.
    #[arg(short = 'p', long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Database name.
    #[arg(short = 'd', long, value_name = "NAME")]
    pub dbname: Option<String>,
    /// Role name.
    #[arg(short = 'U', long, value_name = "NAME")]
    pub username: Option<String>,
    /// Transport protection: disable, prefer, require, verify-ca, verify-full.
    #[arg(long, value_name = "MODE")]
    pub sslmode: Option<String>,
    /// How this database is classified, for example production.
    ///
    /// Classification is never inferred from a host name.
    #[arg(long, value_name = "NAME")]
    pub environment: Option<String>,
    /// Ask the server to refuse writes for this session.
    ///
    /// Enforced by PostgreSQL itself, not by guessing at what a statement does.
    #[arg(long)]
    pub read_only: bool,
    /// Obtain the credential from a cloud identity provider.
    ///
    /// `entra` for Microsoft Entra ID, `aws` for RDS and Aurora IAM, `gcp` for
    /// Cloud SQL IAM, or a name defined under `[auth.providers]`. The
    /// credential is a short-lived token fetched at connection time, and the
    /// connection is refused unless the transport is encrypted.
    #[arg(long, value_name = "PROVIDER")]
    pub auth: Option<String>,

    /// Use a named connection from the configuration file.
    ///
    /// `--profile orders-prod` and the shorthand `@orders-prod` are the same
    /// thing. A profile cannot be combined with a connection target: both are
    /// ways of saying where to connect, and choosing between them silently
    /// would be worse than asking.
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,
}

impl ConnectionOptions {
    /// Whether the user supplied any connection-route or safety option that
    /// should keep implicit startup on the existing direct path.
    #[must_use]
    pub fn has_explicit_route(&self) -> bool {
        self.host.is_some()
            || self.port.is_some()
            || self.dbname.is_some()
            || self.username.is_some()
            || self.sslmode.is_some()
            || self.environment.is_some()
            || self.read_only
            || self.auth.is_some()
            || self.profile.is_some()
    }

    /// Converts to resolved connection arguments.
    pub fn to_args(&self) -> Result<ConnectionArgs, Diagnostic> {
        let sslmode = match &self.sslmode {
            Some(text) => Some(SslMode::parse(text).map_err(|message| {
                Diagnostic::new(DiagnosticKind::Usage, message, "reading --sslmode")
                    .next_action("use verify-full for remote databases")
            })?),
            None => None,
        };
        let environment = match &self.environment {
            Some(text) => Some(Environment::parse(text).map_err(|message| {
                Diagnostic::new(DiagnosticKind::Usage, message, "reading --environment")
                    .next_action(
                        "use local, development, test, staging, production, or a name of your own",
                    )
            })?),
            None => None,
        };
        Ok(ConnectionArgs {
            read_only: self.read_only,
            host: self.host.clone(),
            port: self.port,
            dbname: self.dbname.clone(),
            username: self.username.clone(),
            sslmode,
            environment,
            auth: self.auth.clone(),
        })
    }

    /// Applies a named connection to these arguments.
    ///
    /// A flag the user typed always wins: the profile fills in what was not
    /// said, which is what makes `connect @orders-prod --read-only` mean the
    /// obvious thing.
    fn apply_profile(
        &self,
        args: &mut ConnectionArgs,
        profile: &crate::config::schema::Profile,
        name: &str,
    ) -> Result<(), Diagnostic> {
        profile.validate(name)?;
        if args.host.is_none() {
            args.host.clone_from(&profile.host);
        }
        if args.port.is_none() {
            args.port = profile.port;
        }
        if args.dbname.is_none() {
            args.dbname.clone_from(&profile.dbname);
        }
        if args.username.is_none() {
            args.username.clone_from(&profile.user);
        }
        if args.sslmode.is_none()
            && let Some(text) = &profile.sslmode
        {
            args.sslmode = Some(SslMode::parse(text).map_err(|message| {
                Diagnostic::new(
                    DiagnosticKind::Config,
                    format!("profile {name:?} has an sslmode this build does not know"),
                    "reading connection profiles",
                )
                .likely_cause(message)
                .next_action("use verify-full for remote databases")
            })?);
        }
        if args.environment.is_none()
            && let Some(text) = &profile.environment
        {
            args.environment = Some(Environment::parse(text).map_err(|message| {
                Diagnostic::new(
                    DiagnosticKind::Config,
                    format!("profile {name:?} has an environment this build does not know"),
                    "reading connection profiles",
                )
                .likely_cause(message)
                .next_action(
                    "use local, development, test, staging, production, or a name of your own",
                )
            })?);
        }
        if args.auth.is_none() {
            args.auth.clone_from(&profile.auth);
        }
        // A profile can only add read-only, never take it away: a flag that
        // makes a session safer must not be undone by a file.
        args.read_only = args.read_only || profile.read_only;
        Ok(())
    }
}

/// The connection target and arguments after a profile has been applied.
///
/// Returns the target to connect to, which is `None` when a profile said
/// everything. A profile and a target together are refused rather than merged:
/// both say where to connect, and choosing between them quietly would be worse
/// than asking.
pub(crate) fn resolve_target_and_profile<'a>(
    target: Option<&'a str>,
    options: &ConnectionOptions,
    config: &crate::config::Config,
) -> Result<(Option<&'a str>, ConnectionArgs), Diagnostic> {
    let mut args = options.to_args()?;

    // `@name` is a profile reference. A database really named `@x` is still
    // reachable, through a full connection string, and that is the trade: the
    // shorthand is worth more than the name nobody has.
    let (target, named) = match (target, options.profile.as_deref()) {
        (Some(text), Some(flag)) if text.strip_prefix('@') == Some(flag) => (None, Some(flag)),
        (Some(_), Some(_)) => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a connection target and --profile were both given",
                "choosing where to connect",
            )
            .likely_cause("both say where to connect, and they may not agree")
            .next_action("give one of them"));
        }
        (Some(text), None) => match text.strip_prefix('@') {
            Some(name) => (None, Some(name)),
            None => (Some(text), None),
        },
        (None, flag) => (None, flag),
    };

    if let Some(name) = named {
        let Some(profile) = config.profiles.get(name) else {
            let known: Vec<&str> = config.profiles.keys().map(String::as_str).collect();
            let listing = if known.is_empty() {
                "no profiles are defined in the configuration file".to_owned()
            } else {
                format!("it defines: {}", known.join(", "))
            };
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                format!("profile {name:?} is not in the configuration file"),
                "reading connection profiles",
            )
            .likely_cause(listing)
            .next_action("check the spelling, or add the profile"));
        };
        options.apply_profile(&mut args, profile, name)?;
    }

    Ok((target, args))
}

/// Whether implicit interactive startup should ask for a profile before doing
/// any target or credential work.
#[must_use]
pub(crate) fn should_open_connection_picker(
    target: Option<&str>,
    connection: &ConnectionOptions,
    has_profiles: bool,
) -> bool {
    target.is_none() && has_profiles && !connection.has_explicit_route()
}

/// Configuration sub-commands.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Show where configuration and local state live.
    Paths,
    /// Show the effective configuration.
    Show,
    /// Write a starter configuration file.
    Init {
        /// Replace an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Check the configuration file.
    Validate,
    /// Update the configuration file to the current schema.
    Migrate {
        /// Report what would change without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
}

/// How output should be presented, after resolving flags and the environment.
#[derive(Debug, Clone)]
pub struct Presentation {
    /// Whether colour may be emitted.
    pub color: bool,
    /// Explicit colour flag, or the absolute plain-mode request; None uses config.
    pub color_override: Option<crate::config::ColorMode>,
    /// The `--color-depth` override, when the flag was given.
    pub color_depth: Option<crate::config::ColorDepthChoice>,
    /// The tier resolved from flags and the environment alone.
    pub glyphs: crate::ui::GlyphTier,
    /// A tier the user asked for explicitly, which overrides configuration.
    pub glyph_override: Option<crate::ui::GlyphTier>,
    /// Whether the environment looks able to render Unicode.
    pub unicode_capable: bool,
    /// Whether to show technical detail in errors.
    pub verbose: bool,
    /// Whether plain output mode was requested.
    pub plain: bool,
    /// Whether colour output is disabled.
    pub no_color: bool,
}

impl Presentation {
    /// Resolves configured colour before depth or a validated theme is selected.
    fn with_configured_color(
        &self,
        configured: crate::config::ColorMode,
        facts: &doctor::TerminalFacts,
    ) -> Self {
        let mut presentation = self.clone();
        presentation.color =
            crate::ui::terminal::should_use_color(self.color_override.unwrap_or(configured), facts);
        presentation.no_color = !presentation.color;
        presentation
    }

    /// Whether anything beyond ASCII may be drawn.
    #[must_use]
    pub fn unicode(&self) -> bool {
        self.glyphs != crate::ui::GlyphTier::Ascii
    }
}

/// Runs the command line, writing to the given streams.
///
/// Returns the exit code rather than exiting, so the whole surface is testable.
pub fn run(cli: &Cli, out: &mut impl Write, err: &mut impl Write) -> ExitCode {
    let paths = cli
        .global
        .config_dir
        .as_ref()
        .map_or_else(Paths::resolve, |dir| Paths::rooted_at(dir));

    let facts = crate::ui::terminal::capabilities();
    let presentation = resolve_presentation(&cli.global, &facts);

    let result = match &cli.command {
        Some(Command::Completion { shell }) => completion(*shell, out),
        Some(Command::Version { verbose }) => version(*verbose || presentation.verbose, out),
        Some(Command::Config { action }) => config_command(action, &paths, out),
        Some(Command::History { action }) => history_command(action, &paths, out, err),
        Some(Command::Doctor { json, target }) => {
            doctor_command(*json, target.as_deref(), &paths, &facts, &presentation, out)
        }
        Some(Command::Query {
            target,
            command,
            file,
            format,
            insert_table,
            parameter_env,
            no_header,
            null,
            max_rows,
            output,
            force,
            allow_write,
            connection,
        }) => query_command(
            QueryRequest {
                target: target.as_deref(),
                command: command.as_deref(),
                file: file.as_deref(),
                options: OutputOptions {
                    format: *format,
                    header: !*no_header,
                    null_encoding: null.clone(),
                    unicode: presentation.unicode(),
                    insert_table: insert_table.clone(),
                },
                max_rows: *max_rows,
                output: output.as_deref(),
                force: *force,
                allow_write: *allow_write,
                parameter_env,
                connection,
            },
            &paths,
            out,
            err,
        ),
        Some(Command::Diagram(args)) => diagram_command(args, &paths, &presentation, out, err),
        Some(Command::Ddl(args)) => ddl_command(args, &paths, out, err),
        Some(Command::Activity(args)) => run_activity(args, &paths, &presentation, out, err),
        Some(Command::Audit(args)) => run_audit(args, &paths, &presentation, out, err),
        Some(Command::Diff(args)) => run_diff(args, &paths, &presentation, out, err),
        Some(Command::Import(args)) => run_import(args, &paths, &presentation, out, err),
        Some(Command::Export(args)) => run_export(args, &paths, &presentation, out, err),
        Some(Command::Backup(args)) => run_backup(args, &paths, &presentation, out, err),
        Some(Command::Restore(args)) => run_restore(args, &paths, &presentation, out, err),
        Some(Command::Connect {
            target,
            check,
            connection,
        }) => {
            if *check {
                check_command(target.as_deref(), connection, &paths, out)
            } else if cli.global.plain {
                plain_command(
                    target.as_deref(),
                    connection,
                    &paths,
                    cli.global.no_history,
                    out,
                    err,
                )
            } else {
                interactive::run(
                    target.as_deref(),
                    connection,
                    &paths,
                    &presentation,
                    cli.global.no_history,
                )
            }
        }
        None if cli.global.plain => plain_command(
            None,
            &ConnectionOptions::default(),
            &paths,
            cli.global.no_history,
            out,
            err,
        ),
        None => interactive::run(
            None,
            &ConnectionOptions::default(),
            &paths,
            &presentation,
            cli.global.no_history,
        ),
    };

    match result {
        Ok(code) => code,
        Err(diagnostic) => {
            if let Some(line) = diagnostic.theme_refusal_line() {
                let _ = writeln!(err, "{line}");
            } else {
                let _ = write!(err, "{}", diagnostic.render_plain(presentation.verbose));
            }
            diagnostic.exit_code()
        }
    }
}

/// Resolves flag/environment defaults; loaded configuration applies afterward.
fn resolve_presentation(global: &GlobalArgs, facts: &doctor::TerminalFacts) -> Presentation {
    let color_override = if global.plain {
        Some(crate::config::ColorMode::Never)
    } else {
        global.color.map(|choice| match choice {
            ColorChoice::Always => crate::config::ColorMode::Always,
            ColorChoice::Never => crate::config::ColorMode::Never,
            ColorChoice::Auto => crate::config::ColorMode::Auto,
        })
    };
    let color_mode = color_override.unwrap_or(crate::config::ColorMode::Auto);
    // Plain and --ascii are absolute: they are what a user reaches for when the
    // terminal cannot cope, so configuration does not get to override them.
    let forced_ascii = global.plain || global.ascii;
    let unicode_capable = !forced_ascii && facts.unicode;
    let glyph_override = if forced_ascii {
        Some(crate::ui::GlyphTier::Ascii)
    } else {
        global.glyphs.map(|choice| choice.tier(unicode_capable))
    };

    let color = crate::ui::terminal::should_use_color(color_mode, facts);
    Presentation {
        color,
        color_override,
        color_depth: global.color_depth,
        glyphs: glyph_override.unwrap_or(if unicode_capable {
            crate::ui::GlyphTier::Unicode
        } else {
            crate::ui::GlyphTier::Ascii
        }),
        glyph_override,
        unicode_capable,
        verbose: global.verbose,
        plain: global.plain,
        no_color: !color,
    }
}

/// Resolves the active colour depth with flag-over-configuration precedence.
pub(crate) fn resolve_depth(
    flag: Option<crate::config::ColorDepthChoice>,
    configured: crate::config::ColorDepthChoice,
    color: bool,
    facts: &doctor::TerminalFacts,
) -> (crate::ui::theme::ColorDepth, crate::ui::theme::DepthSource) {
    use crate::ui::theme::DepthSource;
    let source = if flag.is_some() {
        Some(DepthSource::Flag)
    } else if configured != crate::config::ColorDepthChoice::Auto {
        Some(DepthSource::Configuration)
    } else {
        None
    };
    crate::ui::terminal::resolve_color_depth(flag.unwrap_or(configured), source, color, facts)
}

fn completion(shell: clap_complete::Shell, out: &mut impl Write) -> Result<ExitCode, Diagnostic> {
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, branding::BINARY_NAME, out);
    Ok(ExitCode::Success)
}

fn version(verbose: bool, out: &mut impl Write) -> Result<ExitCode, Diagnostic> {
    let write = |out: &mut dyn Write| -> std::io::Result<()> {
        writeln!(out, "{} {}", branding::PRODUCT_NAME, branding::VERSION)?;
        if verbose {
            // Product version, source revision, build identity and target are
            // four separate facts. None of them is derived from another.
            writeln!(
                out,
                "  source revision: {} ({})",
                branding::SOURCE_REVISION,
                branding::SOURCE_CLEAN
            )?;
            writeln!(out, "  build identity:  {}", branding::BUILD_IDENTITY)?;
            writeln!(out, "  target:          {}", branding::TARGET_TRIPLE)?;
            writeln!(out, "  compiler:        {}", branding::RUSTC_VERSION)?;
            writeln!(out, "  platform:        {}", crate::platform::name())?;
        }
        Ok(())
    };
    write(out).map_err(io_diagnostic)?;
    Ok(ExitCode::Success)
}

fn config_command(
    action: &ConfigAction,
    paths: &Paths,
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    match action {
        ConfigAction::Paths => {
            for (label, path) in paths.labelled() {
                writeln!(out, "{label}: {}", path.display()).map_err(io_diagnostic)?;
            }
            Ok(ExitCode::Success)
        }
        ConfigAction::Show => {
            let loaded = config::load(paths)?;
            let rendered = toml::to_string_pretty(&loaded.config).map_err(|e| {
                Diagnostic::new(
                    DiagnosticKind::Internal,
                    "could not render configuration",
                    "showing configuration",
                )
                .likely_cause(e.to_string())
            })?;
            let source = match loaded.source {
                config::ConfigSource::Defaults => "built-in defaults (no file yet)",
                config::ConfigSource::File => "file",
            };
            writeln!(out, "# source: {source}").map_err(io_diagnostic)?;
            if let Some((from, to)) = loaded.pending_migration {
                writeln!(
                    out,
                    "# schema version {from} is older than {to}; run `{} config migrate`",
                    branding::BINARY_NAME
                )
                .map_err(io_diagnostic)?;
            }
            write!(out, "{rendered}").map_err(io_diagnostic)?;
            Ok(ExitCode::Success)
        }
        ConfigAction::Init { force } => {
            if paths.config_file.exists() && !*force {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("{} already exists", paths.config_file.display()),
                    "writing a starter configuration file",
                )
                .likely_cause("writing over it would lose whatever it says now")
                .next_action("edit it, or pass --force to replace it"));
            }
            config::write_template(paths)?;
            writeln!(out, "Wrote {}", paths.config_file.display()).map_err(io_diagnostic)?;
            writeln!(
                out,
                "Every value in it is this build's default. No password belongs in it."
            )
            .map_err(io_diagnostic)?;
            Ok(ExitCode::Success)
        }
        ConfigAction::Validate => {
            let loaded = config::load(paths)?;
            // Everything the client would refuse at startup is checked here, or
            // the command would bless a file that then fails to open. The key
            // bindings are the part whose validity is more than TOML shape.
            crate::ui::keymap::Keymap::from_config(&loaded.config.keys)?;
            writeln!(out, "Configuration is valid.").map_err(io_diagnostic)?;
            if let Some((from, to)) = loaded.pending_migration {
                writeln!(
                    out,
                    "Schema version {from} is older than {to}. Run `{} config migrate` to update it.",
                    branding::BINARY_NAME
                )
                .map_err(io_diagnostic)?;
            }
            Ok(ExitCode::Success)
        }
        ConfigAction::Migrate { dry_run } => {
            let report = config::migrate(paths, *dry_run)?;
            if report.no_op {
                for step in &report.steps {
                    writeln!(out, "{step}").map_err(io_diagnostic)?;
                }
                writeln!(out, "Nothing to do.").map_err(io_diagnostic)?;
                return Ok(ExitCode::Success);
            }
            writeln!(
                out,
                "{} schema version {} to {}:",
                if report.dry_run {
                    "Would migrate"
                } else {
                    "Migrated"
                },
                report.from_version,
                report.to_version
            )
            .map_err(io_diagnostic)?;
            for step in &report.steps {
                writeln!(out, "  - {step}").map_err(io_diagnostic)?;
            }
            if let Some(backup) = &report.backup {
                writeln!(out, "Backup written to {}", backup.display()).map_err(io_diagnostic)?;
            }
            Ok(ExitCode::Success)
        }
    }
}

fn doctor_command(
    json: bool,
    target: Option<&str>,
    paths: &Paths,
    facts: &doctor::TerminalFacts,
    presentation: &Presentation,
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths);
    let configured = loaded
        .as_ref()
        .map(|l| l.config.clone())
        .unwrap_or_default();
    let presentation = presentation.with_configured_color(configured.ui.color, facts);
    // Report the depth actually in use: the flag, then the configuration key,
    // then the environment, with colour-off routes defeating all of them.
    let mut facts = facts.clone();
    let (depth, source) = resolve_depth(
        presentation.color_depth,
        configured.ui.color_depth,
        presentation.color,
        &facts,
    );
    facts.color_depth = depth;
    facts.color_depth_source = source;
    if configured.ui.theme == crate::config::ThemeSelection::Auto
        && facts.is_terminal
        && std::io::stdin().is_terminal()
        && let Ok(mut guard) = crate::ui::TerminalGuard::acquire_raw()
    {
        facts.background_rgb = crate::ui::terminal::probe_osc11_background();
        let _ = guard.restore();
    }
    let mut report = doctor::environment_report(paths, &loaded, &facts);

    let env = EnvSnapshot::from_process();
    report.checks.push(if env.values.is_empty() {
        doctor::Check::ok("postgres environment", "no PG* variables are set")
    } else {
        // Names only. One of these variables holds a password.
        doctor::Check::ok(
            "postgres environment",
            format!("{} set: {}", env.values.len(), env.names().join(", ")),
        )
    });

    if let Some(target) = target {
        let checks = runtime()?.block_on(interactive::probe(target, &configured))?;
        report.checks.extend(checks);
    } else {
        report.checks.push(doctor::Check::skipped(
            "connection",
            "no target given; pass --target to test one",
        ));
    }

    if json {
        writeln!(out, "{}", report.to_json()).map_err(io_diagnostic)?;
    } else {
        write!(out, "{}", report.render_plain()).map_err(io_diagnostic)?;
    }

    Ok(match report.worst() {
        doctor::CheckStatus::Fail => ExitCode::Config,
        _ => ExitCode::Success,
    })
}

/// Everything `query` needs, grouped so the function signature stays readable.
struct QueryRequest<'a> {
    target: Option<&'a str>,
    command: Option<&'a str>,
    file: Option<&'a std::path::Path>,
    options: OutputOptions,
    max_rows: Option<usize>,
    output: Option<&'a std::path::Path>,
    force: bool,
    allow_write: bool,
    parameter_env: &'a [String],
    connection: &'a ConnectionOptions,
}

/// Resolves complete named-parameter mappings before any target or database
/// work. The declaration contains only names; secret values come from the
/// process environment and are kept in `SecretString` immediately.
fn parameter_bindings_from_env(
    sql: &str,
    mappings: &[String],
) -> Result<Option<crate::query::ParameterBindings>, Diagnostic> {
    let template = crate::query::discover_parameters(sql)
        .map_err(|error| error.diagnostic("discovering named parameters"))?;

    if template.is_empty() {
        if mappings.is_empty() {
            return Ok(None);
        }
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "--param-env was supplied, but the statement has no named parameters",
            "reading --param-env mappings",
        )
        .next_action("remove --param-env, or add a :name placeholder to the statement"));
    }

    if mappings.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "the statement has named parameters but no --param-env mappings",
            "reading named parameter values",
        )
        .next_action(format!(
            "provide one --param-env NAME=VARIABLE mapping for each of: {}",
            template
                .names()
                .iter()
                .map(|name| format!(":{name}"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    let mut mapped = std::collections::BTreeMap::<String, String>::new();
    for mapping in mappings {
        if mapping.matches('=').count() != 1 {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "each --param-env mapping must have the form NAME=VARIABLE",
                "reading --param-env mappings",
            )
            .next_action("use shell-safe parameter and environment names without secret values"));
        }
        let (name, variable) = mapping
            .split_once('=')
            .expect("validated exactly one equals sign");
        if !is_environment_name(name) || !is_environment_name(variable) {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "each --param-env mapping must use non-empty ASCII names",
                "reading --param-env mappings",
            )
            .next_action(
                "use letters, digits and underscores, with a letter or underscore first",
            ));
        }
        if !template.names().iter().any(|expected| expected == name) {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("--param-env names unknown parameter :{name}"),
                "checking named parameter mappings",
            )
            .next_action("map only the distinct :name placeholders in the statement"));
        }
        if mapped
            .insert(name.to_owned(), variable.to_owned())
            .is_some()
        {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("parameter :{name} was mapped more than once"),
                "checking named parameter mappings",
            )
            .next_action("provide exactly one --param-env mapping for each :name"));
        }
    }

    for name in template.names() {
        if !mapped.contains_key(name) {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("no --param-env mapping was supplied for :{name}"),
                "checking named parameter mappings",
            )
            .next_action(format!("add --param-env {name}=VARIABLE")));
        }
    }

    let mut values = Vec::with_capacity(template.names().len());
    for name in template.names() {
        let variable = mapped.get(name).expect("missing mappings were rejected");
        let value = std::env::var(variable).map_err(|error| match error {
            std::env::VarError::NotPresent => Diagnostic::new(
                DiagnosticKind::Usage,
                format!("environment variable {variable} is not set"),
                "reading named parameter values",
            )
            .next_action(format!("set {variable} before running the query")),
            std::env::VarError::NotUnicode(_) => Diagnostic::new(
                DiagnosticKind::Usage,
                format!("environment variable {variable} is not valid UTF-8"),
                "reading named parameter values",
            )
            .next_action(format!("set {variable} to UTF-8 text")),
        })?;
        values.push(SecretString::from(value));
    }

    let bindings = crate::query::ParameterBindings::from_secrets(template.names().to_vec(), values)
        .map_err(|error| error.diagnostic("building named parameter bindings"))?;
    bindings
        .validate()
        .map_err(|error| error.diagnostic("validating named parameter values"))?;
    Ok(Some(bindings))
}

fn is_environment_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn query_command(
    request: QueryRequest<'_>,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let sql = read_sql(request.command, request.file)?;
    if sql.trim().is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "no SQL to run",
            "reading the statement to execute",
        )
        .next_action(format!(
            "pass --command \"SELECT 1\", --file query.sql, or pipe SQL into `{} query`",
            branding::BINARY_NAME
        )));
    }

    output::validate_options(&request.options)?;

    let parameters = parameter_bindings_from_env(&sql, request.parameter_env)?;

    let loaded = config::load(paths)?;
    let row_cap = request
        .max_rows
        .unwrap_or(loaded.config.query.max_buffered_rows);
    let (requested, args) =
        resolve_target_and_profile(request.target, request.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    // A production target refuses anything that is not a read unless the caller
    // said otherwise. The classification is advisory and says so; what it buys
    // is that a script cannot write to production by accident on the strength of
    // a copied command line.
    if target.environment.is_production() && !request.allow_write {
        let impact = crate::query::classify_all(&crate::query::split(&sql));
        if impact.needs_confirmation() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "this statement {} and the target is classified as production",
                    impact.label()
                ),
                "checking a statement against the target's classification",
            )
            .likely_cause(
                "the connection was given --environment production, which refuses anything \
                 that is not a read",
            )
            .next_action(
                "pass --allow-write if that is what you meant, or --read-only to have the \
                 server refuse writes itself. This check reads leading keywords only and is \
                 not a security boundary: database permissions are.",
            )
            .technical("Impact", impact.label())
            .technical("Environment", target.environment.label()));
        }
    }

    // An export streams to a file without retaining the complete result.
    if let Some(destination) = request.output {
        let writer = output::StreamWriter::new(request.options.clone())?;
        let export = crate::query::export::Export::create(destination, request.force)?;
        let outcome = runtime()?.block_on(interactive::export_once_with_parameters_reported(
            target,
            &loaded.config,
            sql,
            parameters,
            export,
            writer,
        ));
        write_notice_counts(err, outcome.notice_report.counts).map_err(io_diagnostic)?;
        let outcome = outcome.value?;
        return match outcome {
            interactive::ExportOutcome::Completed(finished) => {
                // The summary is a diagnostic, not data, so it goes to stderr
                // even though stdout is empty for an export.
                writeln!(
                    err,
                    "Exported {} row(s) to {}",
                    finished.rows,
                    finished.path.display()
                )
                .map_err(io_diagnostic)?;
                Ok(ExitCode::Success)
            }
            interactive::ExportOutcome::Interrupted { abandoned, reason } => {
                writeln!(err, "{}", abandoned.message()).map_err(io_diagnostic)?;
                Err(Diagnostic::new(
                    DiagnosticKind::ExportInterrupted,
                    reason.headline.clone(),
                    "exporting rows to a file",
                )
                .likely_cause(
                    reason
                        .likely_cause
                        .clone()
                        .unwrap_or_else(|| "the stream ended early".to_owned()),
                )
                .next_action(format!(
                    "the rows written are in {}",
                    abandoned.partial.display()
                )))
            }
        };
    }

    let execution = runtime()?.block_on(interactive::execute_once_with_parameters_reported(
        target,
        &loaded.config,
        sql,
        parameters,
        row_cap,
    ));

    let execution = match execution {
        Ok(execution) => execution,
        Err(failure) => {
            return write_query_failure(
                err,
                failure.value,
                &failure.notice_report,
                request.options.format,
            );
        }
    };
    write_query_execution(out, err, &execution, &request.options)
}

fn write_query_execution(
    out: &mut impl Write,
    err: &mut impl Write,
    execution: &crate::query::result::Execution,
    options: &output::OutputOptions,
) -> Result<ExitCode, Diagnostic> {
    if let Err(error) = output::write_execution(out, execution, options) {
        let diagnostic = match error.kind() {
            std::io::ErrorKind::BrokenPipe => return Ok(ExitCode::Success),
            std::io::ErrorKind::InvalidInput => {
                Diagnostic::new(DiagnosticKind::Usage, error.to_string(), "writing results")
                    .next_action(
                        "run a single statement, or choose a format that carries result sets",
                    )
            }
            _ => io_diagnostic(error),
        };
        return write_query_failure(err, diagnostic, &execution.notice_report, options.format);
    }
    write_buffered_truncation(err, execution, options.format).map_err(io_diagnostic)?;
    if let Some(error) = &execution.error {
        return write_query_failure(err, error.clone(), &execution.notice_report, options.format);
    }
    if options.format == Format::Json && execution.notice_report.counts.needs_report() {
        writeln!(err,"{}",serde_json::json!({"kind":"notice_retention","source":"client","notice_report":execution.notice_report.to_json()})).map_err(io_diagnostic)?;
    } else {
        write_notice_counts(err, execution.notice_report.counts).map_err(io_diagnostic)?;
    }
    Ok(ExitCode::Success)
}

fn write_query_failure(
    err: &mut impl Write,
    diagnostic: Diagnostic,
    report: &crate::query::result::NoticeReport,
    format: Format,
) -> Result<ExitCode, Diagnostic> {
    if format == Format::Json {
        // Completed-prefix result bytes remain on stdout; stderr has one diagnostic.
        let mut value = diagnostic.to_json();
        if report.counts.needs_report() {
            value["notice_report"] = report.to_json();
        }
        writeln!(err, "{value}").map_err(io_diagnostic)?;
        Ok(diagnostic.exit_code())
    } else {
        write_notice_counts(err, report.counts).map_err(io_diagnostic)?;
        Err(diagnostic)
    }
}

pub(crate) fn write_notice_counts(
    err: &mut impl Write,
    counts: crate::query::result::NoticeCounts,
) -> std::io::Result<()> {
    for line in counts.lines() {
        writeln!(err, "{line}")?;
    }
    Ok(())
}

/// Reports client retention only after buffered data writing succeeds. Counts
/// describe received rows, never an unknown server total or a reason to replay.
fn write_buffered_truncation(
    err: &mut impl Write,
    execution: &crate::query::result::Execution,
    format: Format,
) -> std::io::Result<()> {
    if !matches!(
        format,
        Format::Csv | Format::Tsv | Format::Ndjson | Format::Markdown | Format::Insert
    ) {
        return Ok(());
    }
    for (index, set) in execution
        .statements
        .iter()
        .filter_map(|statement| statement.result_set.as_ref())
        .enumerate()
    {
        if !set.is_available() {
            writeln!(
                err,
                "Result set {} data unavailable: retained no description for {} source columns and {} received rows because the retained-result limit was reached. For a future complete streaming export, choose --output before execution.",
                index + 1,
                set.source_column_count(),
                set.rows_seen
            )?;
        } else if set.is_truncated() {
            writeln!(
                err,
                "Result set {} truncated: retained {} of {} received rows. For a future complete streaming export, choose --output before execution.",
                index + 1,
                set.rows.len(),
                set.rows_seen,
            )?;
        }
    }
    Ok(())
}

/// Opens the plain, line-oriented client.
fn plain_command(
    target: Option<&str>,
    connection: &ConnectionOptions,
    paths: &Paths,
    no_history: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, args) = resolve_target_and_profile(target, connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let resolved = authenticate_target(resolved, &loaded.config)?;
    let history = crate::history::History::open(paths, &loaded.config.history, no_history);
    plain::run(resolved, &loaded.config, &history, out, err)
}

/// Reads or clears the statements that have run.
///
/// The history is a file of SQL on this machine, so every command here is about
/// seeing it or removing it. Nothing sends it anywhere.
fn history_command(
    action: &HistoryAction,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let history = crate::history::History::open(paths, &loaded.config.history, false);

    match action {
        HistoryAction::Path => {
            writeln!(out, "{}", history.path().display()).ok();
            Ok(ExitCode::Success)
        }
        HistoryAction::List { limit, json } => {
            let entries = history.recent(*limit)?;
            if entries.is_empty() {
                let reason = if loaded.config.history.enabled {
                    "No statements have been recorded yet."
                } else {
                    "History is switched off in configuration (history.enabled = false)."
                };
                writeln!(err, "{reason}").ok();
                return Ok(ExitCode::Success);
            }
            for entry in &entries {
                if *json {
                    let line = serde_json::to_string(entry).unwrap_or_default();
                    writeln!(out, "{line}").ok();
                } else {
                    writeln!(
                        out,
                        "{}  {:<9} {:<12} {}",
                        entry.when(),
                        entry.outcome.label(),
                        entry.database,
                        entry.one_line()
                    )
                    .ok();
                }
            }
            // The rule that keeps secrets out of the file belongs where someone
            // is looking at the file.
            if !*json {
                writeln!(
                    err,
                    "Statements that mention a credential are never recorded. \
                     {} entries kept, at most {}.",
                    entries.len(),
                    loaded.config.history.max_entries
                )
                .ok();
            }
            Ok(ExitCode::Success)
        }
        HistoryAction::Clear { yes } => {
            if !*yes {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "clearing the history cannot be undone",
                    "clearing the statement history",
                )
                .likely_cause(format!("this would delete {}", history.path().display()))
                .next_action("pass --yes to confirm"));
            }
            history.clear()?;
            writeln!(err, "History cleared: {}", history.path().display()).ok();
            Ok(ExitCode::Success)
        }
    }
}

fn check_command(
    target: Option<&str>,
    connection: &ConnectionOptions,
    paths: &Paths,
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, args) = resolve_target_and_profile(target, connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    for note in &resolved.notes {
        writeln!(out, "Note: {}: {}", note.subject, note.message).map_err(io_diagnostic)?;
    }
    // `check` tests the whole route, and for a cloud target the credential is
    // part of the route. A check that skipped it would report a healthy
    // connection for a target that cannot in fact be connected to.
    let resolved = authenticate_target(resolved, &loaded.config)?;
    let checks = runtime()?.block_on(interactive::probe_target(resolved, &loaded.config))?;
    let report = doctor::Report { checks };
    write!(out, "{}", report.render_plain()).map_err(io_diagnostic)?;
    Ok(match report.worst() {
        doctor::CheckStatus::Fail => ExitCode::Connection,
        _ => ExitCode::Success,
    })
}

fn diagram_command(
    args: &DiagramArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    use crate::diagram::{DiagramFormat, DiagramHopDepth};

    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    let mut graph = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let graph = session.schema_graph(args.schema.as_deref()).await;
        drop(session);
        graph
    })?;

    if let Some(ref table) = args.table {
        let depth = match args.hops.unwrap_or(1) {
            1 => DiagramHopDepth::OneHop,
            _ => DiagramHopDepth::TwoHops,
        };
        graph = graph.filter_neighborhood(table, depth);
    }

    let format = match args.format {
        Some(f) => DiagramFormat::from(f),
        None => {
            if presentation.no_color || presentation.plain {
                DiagramFormat::Ascii
            } else {
                DiagramFormat::Unicode
            }
        }
    };

    let rendered = graph.render(format);

    if let Some(ref destination) = args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export
            .writer()
            .write_all(rendered.as_bytes())
            .map_err(io_diagnostic)?;
        let _ = export.finish()?;
        writeln!(
            err,
            "Wrote {} diagram ({} table(s), {} relationship(s)) to {}",
            format.label(),
            graph.tables.len(),
            graph.edges.len(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    } else {
        out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    }
}

fn ddl_command(
    args: &DdlArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let (target_raw, object_raw) = match (args.target.as_deref(), args.object_pos.as_deref()) {
        (Some(t), Some(o)) => (Some(t), Some(o)),
        (Some(single), None) => {
            if args.object.is_some()
                || single.starts_with("postgres://")
                || single.starts_with("postgresql://")
                || single.starts_with('@')
                || single.contains('=')
            {
                (Some(single), None)
            } else if single.contains('.') {
                (None, Some(single))
            } else {
                (Some(single), None)
            }
        }
        (None, Some(o)) => (None, Some(o)),
        (None, None) => (None, None),
    };

    let object_ident = args.object.as_deref().or(object_raw);
    let (schema_name, object_name) = match object_ident {
        Some(ident) => {
            if let Some((s, obj)) = ident.split_once('.') {
                (s.to_owned(), Some(obj.to_owned()))
            } else {
                (
                    args.schema.clone().unwrap_or_else(|| "public".to_owned()),
                    Some(ident.to_owned()),
                )
            }
        }
        None => (
            args.schema.clone().unwrap_or_else(|| "public".to_owned()),
            None,
        ),
    };

    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(target_raw, &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    let bundle = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let result = session
            .generate_ddl(&schema_name, object_name.as_deref())
            .await;
        drop(session);
        result
    })?;

    let rendered = bundle.render_sql(args.drop);

    if let Some(ref destination) = args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export
            .writer()
            .write_all(rendered.as_bytes())
            .map_err(io_diagnostic)?;
        let _ = export.finish()?;
        writeln!(
            err,
            "Wrote DDL ({} object(s)) to {}",
            bundle.objects.len(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    } else {
        out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    }
}

fn run_activity(
    args: &ActivityArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    let snapshot = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let result = session.server_activity().await;
        drop(session);
        result
    })?;

    let rendered = match args.format {
        CliActivityFormat::Json => snapshot.render_json(),
        CliActivityFormat::Table => {
            let mode = if args.locks {
                crate::activity::ActivityViewMode::LocksTree
            } else {
                crate::activity::ActivityViewMode::Backends
            };
            snapshot.render_text(mode, presentation.glyphs != crate::ui::GlyphTier::Ascii)
        }
    };

    if let Some(ref destination) = args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export
            .writer()
            .write_all(rendered.as_bytes())
            .map_err(io_diagnostic)?;
        let _ = export.finish()?;
        writeln!(
            err,
            "Wrote server activity snapshot ({} backend(s)) to {}",
            snapshot.backends.len(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    } else {
        out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    }
}

fn run_audit(
    args: &AuditArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    let snapshot = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let result = session.security_audit(args.schema.as_deref()).await;
        drop(session);
        result
    })?;

    let rendered = match args.format {
        CliActivityFormat::Json => snapshot.render_json(),
        CliActivityFormat::Table => snapshot.render_text(
            args.section.map(CliAuditSection::into_domain),
            presentation.glyphs != crate::ui::GlyphTier::Ascii,
        ),
    };

    if let Some(ref destination) = args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export
            .writer()
            .write_all(rendered.as_bytes())
            .map_err(io_diagnostic)?;
        let _ = export.finish()?;
        writeln!(
            err,
            "Wrote security audit report ({} role(s), {} table(s), {} grant(s)) to {}",
            snapshot.roles.len(),
            snapshot.tables.len(),
            snapshot.grants.len(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    } else {
        out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    }
}

fn run_diff(
    args: &DiffArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    let target2_info = if let Some(ref target2_str) = args.target2 {
        let (requested2, conn_args2) = resolve_target_and_profile(
            Some(target2_str.as_str()),
            &args.connection,
            &loaded.config,
        )?;
        let t2 = crate::connection::resolve(
            requested2,
            &conn_args2,
            &EnvSnapshot::from_process(),
            &loaded.config.connection,
        )?;
        Some(authenticate_target(t2, &loaded.config)?)
    } else {
        None
    };

    let target_schema = match (&args.target_schema, &args.target2) {
        (Some(ts), _) => ts.clone(),
        (None, Some(_)) => args.source_schema.clone(),
        (None, None) => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "missing target schema or second connection target to compare",
                "comparing schemas with ignatius diff",
            )
            .likely_cause(
                "ignatius diff requires either --target-schema <SCHEMA> within the same database or --target2 <TARGET> to compare across databases",
            )
            .next_action(
                "specify --target-schema <SCHEMA> (e.g. --target-schema staging) or --target2 <TARGET>",
            ));
        }
    };

    let diff = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;

        if let Some(ref t2) = target2_info {
            let session2 = crate::postgres::session::connect_reported(
                t2,
                std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
            )
            .await
            .value?;
            let source_snap = session.schema_snapshot(&args.source_schema).await;
            let target_snap = session2.schema_snapshot(&target_schema).await;
            drop(session);
            drop(session2);
            let s_snap = source_snap?;
            let t_snap = target_snap?;
            Ok(crate::migration::compute_schema_diff(&s_snap, &t_snap))
        } else {
            let res = session
                .schema_diff(&args.source_schema, &target_schema)
                .await;
            drop(session);
            res
        }
    })?;

    let rendered = match args.format {
        CliDiffFormat::Sql => {
            let options = crate::migration::MigrationOptions {
                use_transaction: !args.no_transaction,
                include_comments: true,
                target_schema_override: None,
            };
            crate::migration::generate_migration_sql(&diff, &options)
        }
        CliDiffFormat::Tree => crate::migration::render_diff_tree(
            &diff,
            presentation.glyphs != crate::ui::GlyphTier::Ascii,
        ),
        CliDiffFormat::Json => crate::migration::render_diff_json(&diff),
    };

    if let Some(ref destination) = args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export
            .writer()
            .write_all(rendered.as_bytes())
            .map_err(io_diagnostic)?;
        let _ = export.finish()?;
        writeln!(
            err,
            "Wrote schema diff ({} change(s)) to {}",
            diff.total_changes(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    } else {
        out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
        Ok(ExitCode::Success)
    }
}

/// Supported formats for the import and export CLI subcommands.
#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq, Default)]
pub enum CliTransferFormat {
    /// Comma-separated values (RFC 4180).
    #[default]
    Csv,
    /// Tab-separated values.
    Tsv,
    /// Newline-delimited JSON.
    Jsonl,
    /// SQL INSERT statements.
    Sql,
    /// PostgreSQL binary COPY protocol format.
    Binary,
}

impl CliTransferFormat {
    /// Converts to domain TransferFormat.
    #[must_use]
    pub const fn to_domain(self) -> crate::transfer::TransferFormat {
        match self {
            Self::Csv => crate::transfer::TransferFormat::Csv,
            Self::Tsv => crate::transfer::TransferFormat::Tsv,
            Self::Jsonl => crate::transfer::TransferFormat::Jsonl,
            Self::Sql => crate::transfer::TransferFormat::SqlInsert,
            Self::Binary => crate::transfer::TransferFormat::Binary,
        }
    }
}

/// Supported formats for the backup CLI subcommand.
#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq, Default)]
pub enum CliBackupFormat {
    /// Custom pg_dump archive format (-F c).
    #[default]
    Custom,
    /// Plain SQL script (-F p).
    Plain,
    /// Directory archive (-F d).
    Directory,
    /// Tar archive (-F t).
    Tar,
}

impl CliBackupFormat {
    /// Converts to domain BackupFormat.
    #[must_use]
    pub const fn to_domain(self) -> crate::transfer::BackupFormat {
        match self {
            Self::Custom => crate::transfer::BackupFormat::Custom,
            Self::Plain => crate::transfer::BackupFormat::Plain,
            Self::Directory => crate::transfer::BackupFormat::Directory,
            Self::Tar => crate::transfer::BackupFormat::Tar,
        }
    }
}

/// Arguments for the import command.
#[derive(Debug, Args, Clone)]
pub struct ImportArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Destination table name.
    #[arg(short = 't', long, value_name = "TABLE")]
    pub table: String,
    /// Source file path to import.
    #[arg(short = 'f', long, value_name = "FILE")]
    pub file: PathBuf,
    /// Destination schema.
    #[arg(short = 's', long, value_name = "SCHEMA", default_value = "public")]
    pub schema: String,
    /// Input file format.
    #[arg(long, value_enum, default_value_t = CliTransferFormat::Csv)]
    pub format: CliTransferFormat,
    /// Field delimiter character.
    #[arg(long, value_name = "CHAR")]
    pub delimiter: Option<String>,
    /// The input file does not contain a header row.
    #[arg(long)]
    pub no_header: bool,
    /// Text representing NULL values.
    #[arg(long, value_name = "TEXT", default_value = "")]
    pub null: String,
    /// Dry-run test: executes inside a transaction and rolls back.
    #[arg(long)]
    pub dry_run: bool,
    /// Do not wrap the import in a transaction.
    #[arg(long)]
    pub no_transaction: bool,
    /// Confirm write execution against a production target.
    #[arg(long)]
    pub confirm_production: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Arguments for the export command.
#[derive(Debug, Args, Clone)]
pub struct ExportArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// SQL query or table name to export.
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,
    /// Output file format.
    #[arg(long, value_enum, default_value_t = CliTransferFormat::Csv)]
    pub format: CliTransferFormat,
    /// Field delimiter character.
    #[arg(long, value_name = "CHAR")]
    pub delimiter: Option<String>,
    /// Omit header row from output.
    #[arg(long)]
    pub no_header: bool,
    /// Text representing NULL values.
    #[arg(long, value_name = "TEXT", default_value = "")]
    pub null: String,
    /// Output file path (defaults to standard output).
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Arguments for the backup command.
#[derive(Debug, Args, Clone)]
pub struct BackupArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Destination backup file or directory path.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: PathBuf,
    /// Backup archive format.
    #[arg(long, value_enum, default_value_t = CliBackupFormat::Custom)]
    pub format: CliBackupFormat,
    /// Filter backup to a single schema.
    #[arg(short = 'n', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Filter backup to a single table.
    #[arg(short = 't', long, value_name = "TABLE")]
    pub table: Option<String>,
    /// Drop database objects before recreating them.
    #[arg(long)]
    pub clean: bool,
    /// Include commands to create the database in plain text dumps.
    #[arg(long)]
    pub create: bool,
    /// Dump only table data, no schema DDL.
    #[arg(long)]
    pub data_only: bool,
    /// Dump only schema DDL, no table data.
    #[arg(long)]
    pub schema_only: bool,
    /// Print verbose progress and diagnostic messages.
    #[arg(short = 'v', long)]
    pub verbose: bool,
    /// Print the pg_dump command and redacted environment without executing.
    #[arg(long)]
    pub dry_run: bool,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Arguments for the restore command.
#[derive(Debug, Args, Clone)]
pub struct RestoreArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Source backup file or directory path.
    #[arg(short = 'i', long, value_name = "PATH")]
    pub input: PathBuf,
    /// Drop database objects before restoring them.
    #[arg(long)]
    pub clean: bool,
    /// Create the database before restoring into it.
    #[arg(long)]
    pub create: bool,
    /// Restore only table data.
    #[arg(long)]
    pub data_only: bool,
    /// Restore only schema DDL.
    #[arg(long)]
    pub schema_only: bool,
    /// Print verbose progress messages.
    #[arg(short = 'v', long)]
    pub verbose: bool,
    /// Print the pg_restore command and redacted environment without executing.
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_import(
    args: &ImportArgs,
    paths: &Paths,
    _presentation: &Presentation,
    out: &mut impl Write,
    _err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    if target.environment == crate::connection::Environment::Production && !args.confirm_production
    {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "refusing bulk import into a production database without confirmation",
            "importing data with ignatius import",
        )
        .likely_cause("target is classified as production")
        .next_action("pass --confirm-production if this import is intentional"));
    }

    let data = std::fs::read(&args.file).map_err(|e| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not read import file {}: {e}", args.file.display()),
            "reading input file",
        )
        .next_action("verify that the file exists and is readable")
    })?;

    let delimiter = if let Some(ref d) = args.delimiter {
        d.as_bytes().first().copied().unwrap_or(b',')
    } else if args.format == CliTransferFormat::Tsv {
        b'\t'
    } else {
        let sample = String::from_utf8_lossy(&data[..data.len().min(4096)]);
        crate::transfer::detect_csv_delimiter(&sample)
    };

    let options = crate::transfer::ImportOptions {
        schema: args.schema.clone(),
        table: args.table.clone(),
        file_path: args.file.display().to_string(),
        format: args.format.to_domain(),
        delimiter,
        has_header: !args.no_header,
        null_string: args.null.clone(),
        quote: b'"',
        escape: b'"',
        columns: Vec::new(),
        dry_run: args.dry_run,
        transactional: !args.no_transaction,
    };

    let result = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let res = session.execute_import(&options, &data).await;
        drop(session);
        res
    })?;

    writeln!(out, "Import completed: {}", result.status).map_err(io_diagnostic)?;
    writeln!(
        out,
        "Rows: {}, Bytes: {}, Elapsed: {}ms",
        result.rows_imported, result.bytes_processed, result.elapsed_ms
    )
    .map_err(io_diagnostic)?;

    Ok(ExitCode::Success)
}

fn run_export(
    args: &ExportArgs,
    paths: &Paths,
    _presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    if args
        .output
        .as_ref()
        .is_some_and(|destination| destination.exists())
        && !args.force
    {
        let destination = args.output.as_ref().unwrap();
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "refusing to overwrite existing file {}",
                destination.display()
            ),
            "exporting data with ignatius export",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

    let query_or_table = match args
        .query
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(q) => q.to_string(),
        None => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "an export query or table name is required",
                "exporting data with ignatius export",
            )
            .next_action(
                "pass a SQL query or table name, e.g. ignatius export \"SELECT * FROM users\"",
            ));
        }
    };

    let delimiter = if let Some(ref d) = args.delimiter {
        d.as_bytes().first().copied().unwrap_or(b',')
    } else if args.format == CliTransferFormat::Tsv {
        b'\t'
    } else {
        b','
    };

    let options = crate::transfer::ExportOptions {
        query: query_or_table,
        output_path: args.output.as_ref().map(|p| p.display().to_string()),
        format: args.format.to_domain(),
        delimiter,
        has_header: !args.no_header,
        null_string: args.null.clone(),
        quote: b'"',
        escape: b'"',
    };

    let bytes = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let res = session.execute_export(&options).await;
        drop(session);
        res
    })?;

    if let Some(ref destination) = args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export.writer().write_all(&bytes).map_err(io_diagnostic)?;
        let _ = export.finish()?;
        writeln!(
            err,
            "Exported {} bytes to {}",
            bytes.len(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
    } else {
        out.write_all(&bytes).map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

fn run_backup(
    args: &BackupArgs,
    paths: &Paths,
    _presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    if args.output.exists() && !args.force && !args.dry_run {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "refusing to overwrite existing file {}",
                args.output.display()
            ),
            "backing up database with ignatius backup",
        )
        .likely_cause("the target output backup file already exists")
        .next_action("pass --force to overwrite the file"));
    }

    let host_str = match &target.host {
        crate::connection::Host::Tcp(h) => Some(h.clone()),
        crate::connection::Host::Socket(s) => Some(s.display().to_string()),
    };

    let options = crate::transfer::BackupOptions {
        database: target.database.clone(),
        host: host_str,
        port: Some(target.port),
        user: Some(target.user.clone()),
        output_path: args.output.display().to_string(),
        format: args.format.to_domain(),
        schema: args.schema.clone(),
        table: args.table.clone(),
        clean: args.clean,
        create: args.create,
        data_only: args.data_only,
        schema_only: args.schema_only,
        verbose: args.verbose,
    };

    use secrecy::ExposeSecret;
    let password = target.password.as_ref().map(|p| p.expose_secret());

    if args.dry_run {
        let (cmd, cmd_args, env) = crate::transfer::build_pg_dump_command(&options, password, true);
        writeln!(out, "Command: {} {}", cmd, cmd_args.join(" ")).map_err(io_diagnostic)?;
        for (k, v) in env {
            writeln!(out, "Env: {k}={v}").map_err(io_diagnostic)?;
        }
        return Ok(ExitCode::Success);
    }

    let (cmd, cmd_args, env) = crate::transfer::build_pg_dump_command(&options, password, false);
    let mut process = std::process::Command::new(&cmd);
    process.args(&cmd_args);
    for (k, v) in env {
        process.env(k, v);
    }

    let status = process.status().map_err(|e| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("could not execute {cmd}: {e}"),
            "running pg_dump subprocess",
        )
        .next_action("ensure PostgreSQL client utilities (pg_dump) are installed and on PATH")
    })?;

    if !status.success() {
        return Err(Diagnostic::new(
            DiagnosticKind::Internal,
            format!("pg_dump exited with error code {:?}", status.code()),
            "backing up database",
        ));
    }

    writeln!(
        err,
        "Backup completed successfully to {}",
        args.output.display()
    )
    .map_err(io_diagnostic)?;
    Ok(ExitCode::Success)
}

fn run_restore(
    args: &RestoreArgs,
    paths: &Paths,
    _presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let target = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(target, &loaded.config)?;

    if !args.input.exists() && !args.dry_run {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("backup file does not exist: {}", args.input.display()),
            "restoring database with ignatius restore",
        )
        .next_action("verify the backup file path"));
    }

    let host_str = match &target.host {
        crate::connection::Host::Tcp(h) => Some(h.clone()),
        crate::connection::Host::Socket(s) => Some(s.display().to_string()),
    };

    let options = crate::transfer::RestoreOptions {
        database: target.database.clone(),
        host: host_str,
        port: Some(target.port),
        user: Some(target.user.clone()),
        input_path: args.input.display().to_string(),
        clean: args.clean,
        create: args.create,
        data_only: args.data_only,
        schema_only: args.schema_only,
        verbose: args.verbose,
    };

    use secrecy::ExposeSecret;
    let password = target.password.as_ref().map(|p| p.expose_secret());

    if args.dry_run {
        let (cmd, cmd_args, env) =
            crate::transfer::build_pg_restore_command(&options, password, true);
        writeln!(out, "Command: {} {}", cmd, cmd_args.join(" ")).map_err(io_diagnostic)?;
        for (k, v) in env {
            writeln!(out, "Env: {k}={v}").map_err(io_diagnostic)?;
        }
        return Ok(ExitCode::Success);
    }

    let (cmd, cmd_args, env) = crate::transfer::build_pg_restore_command(&options, password, false);
    let mut process = std::process::Command::new(&cmd);
    process.args(&cmd_args);
    for (k, v) in env {
        process.env(k, v);
    }

    let status = process.status().map_err(|e| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            format!("could not execute {cmd}: {e}"),
            "running pg_restore subprocess",
        )
        .next_action("ensure PostgreSQL client utilities (pg_restore) are installed and on PATH")
    })?;

    if !status.success() {
        return Err(Diagnostic::new(
            DiagnosticKind::Internal,
            format!("pg_restore exited with error code {:?}", status.code()),
            "restoring database",
        ));
    }

    writeln!(
        err,
        "Restore completed successfully from {}",
        args.input.display()
    )
    .map_err(io_diagnostic)?;
    Ok(ExitCode::Success)
}

/// Reads SQL from the chosen source, refusing to hang on a terminal.
fn read_sql(command: Option<&str>, file: Option<&std::path::Path>) -> Result<String, Diagnostic> {
    if let Some(sql) = command {
        return Ok(sql.to_owned());
    }
    match file {
        Some(path) if path == std::path::Path::new("-") => read_stdin(),
        Some(path) => std::fs::read_to_string(path).map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("could not read {}", path.display()),
                "reading the SQL file",
            )
            .likely_cause(err.to_string())
            .next_action("check the path and its permissions")
        }),
        None => read_stdin(),
    }
}

fn read_stdin() -> Result<String, Diagnostic> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        // Never sit waiting for input that a person did not mean to give.
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "no SQL given and standard input is a terminal",
            "reading the statement to execute",
        )
        .next_action(format!(
            "pass --command \"SELECT 1\", --file query.sql, or pipe SQL into `{} query`",
            branding::BINARY_NAME
        )));
    }
    let mut buffer = String::new();
    stdin
        .lock()
        .read_to_string(&mut buffer)
        .map_err(io_diagnostic)?;
    Ok(buffer)
}

/// Builds the async runtime used by the commands that talk to a server.
fn runtime() -> Result<tokio::runtime::Runtime, Diagnostic> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not start the async runtime",
                "preparing to connect",
            )
            .likely_cause(err.to_string())
        })
}

/// The cloud identity providers this run understands, from configuration.
///
/// Built-ins are added by the registry; this is only what the file defined, and
/// each is validated here so that a definition which could not work is a
/// configuration error before a terminal is taken rather than a failure at the
/// moment somebody tries to connect.
pub(crate) fn auth_providers(
    config: &crate::config::schema::Config,
) -> Result<std::collections::BTreeMap<String, crate::connection::cloud::Provider>, Diagnostic> {
    let mut providers = std::collections::BTreeMap::new();
    for (name, defined) in &config.auth.providers {
        defined.validate(name)?;
        providers.insert(name.clone(), defined.to_provider(name));
    }
    Ok(providers)
}

/// Gives a resolved target its cloud credential, from a synchronous caller.
///
/// The asynchronous paths call `cloud::authenticate` directly. This exists so
/// the command functions, which are ordinary synchronous code, go through the
/// same single place rather than each growing its own idea of when to fetch.
pub(crate) fn authenticate_target(
    target: crate::connection::ConnectionTarget,
    config: &crate::config::schema::Config,
) -> Result<crate::connection::ConnectionTarget, Diagnostic> {
    if target.auth.is_none() {
        return Ok(target);
    }
    let providers = auth_providers(config)?;
    runtime()?
        .block_on(crate::connection::cloud::authenticate(target, &providers))
        .map_err(|error| *error)
}

fn io_diagnostic(err: std::io::Error) -> Diagnostic {
    Diagnostic::new(DiagnosticKind::Internal, err.to_string(), "writing output")
        .next_action("check the output stream and available space")
}

#[cfg(test)]
mod tests;
