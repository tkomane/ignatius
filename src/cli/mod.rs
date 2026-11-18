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
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use output::{Format, OutputOptions};
use secrecy::SecretString;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

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

    /// SQL files to preload into editor buffers at startup.
    #[arg(short = 'f', long = "file", value_name = "PATH")]
    pub file: Vec<PathBuf>,

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
        /// SQL files to preload into editor buffers at startup.
        #[arg(short = 'f', long = "file", value_name = "PATH")]
        file: Vec<PathBuf>,
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

    /// Lint SQL files or standard input without connecting to a server.
    #[command(name = "lint")]
    Lint(LintArgs),

    /// Inspect, plan, apply or verify versioned SQL migrations.
    #[command(name = "migrate")]
    Migrate(MigrateArgs),

    /// Generate a Markdown, JSON or Mermaid data dictionary from the catalog.
    #[command(name = "docs")]
    Docs(DocsArgs),

    /// Audit table growth, vacuum debt, sequence headroom and partitioning candidates.
    #[command(name = "growth")]
    Growth(GrowthArgs),

    /// Inspect partitions and generate reviewed range-partition DDL.
    #[command(name = "partition")]
    Partition(PartitionArgs),

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

    /// Prove an archive restores into a scratch database and matches the source.
    #[command(name = "restore-drill")]
    RestoreDrill(RestoreDrillArgs),

    /// Execute a stored procedure or function under a safe testing harness with rollback.
    #[command(name = "run-routine")]
    RunRoutine(RunRoutineArgs),

    /// Profile a PL/pgSQL routine for performance bottlenecks, dead code, and injection risks.
    #[command(name = "profile-routine")]
    ProfileRoutine(ProfileRoutineArgs),

    /// Listen for asynchronous pub/sub notifications on a channel.
    Listen(ListenArgs),

    /// Dispatch an asynchronous pub/sub notification to a channel.
    Notify(NotifyArgs),

    /// Execute or monitor database maintenance operations (vacuum, analyze, reindex, progress).
    Maintain(MaintainArgs),

    /// Profile query execution telemetry and stat statements forensics from pg_stat_statements.
    #[command(name = "profile-queries")]
    ProfileQueries(ProfileQueriesArgs),

    /// Analyze redundant indexes, B-tree bloat forensics, and unindexed foreign keys.
    #[command(name = "index-advisor")]
    IndexAdvisor(IndexAdvisorArgs),

    /// Observe transaction throughput, WAL write rates, checkpointer telemetry, and replication lag.
    #[command(name = "wal-stats")]
    WalStats(WalStatsArgs),

    /// Observe connection pool health, client wait queues, and server connection exhaustion.
    #[command(name = "pool-stats")]
    PoolStats(PoolStatsArgs),

    /// Analyze sensitive PII columns, infer masking rules, and generate anonymized view DDL.
    #[command(name = "mask-rules")]
    MaskRules(MaskRulesArgs),

    /// Execute cross-database copy and synchronization pipelines with topological DAG dependency ordering.
    #[command(name = "sync")]
    Sync(SyncArgs),

    /// Synthesize deterministic mock test fixtures across database tables with referential integrity.
    #[command(name = "seed")]
    Seed(SeedArgs),

    /// Inspect, show, or persist reusable SQL snippets and scratchpads.
    #[command(name = "snippet")]
    Snippet(SnippetArgs),

    /// Inspect, restore, or discard an unrecovered crash session journal.
    #[command(name = "recovery")]
    Recovery(RecoveryArgs),
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
        Some(Command::Lint(args)) => run_lint(args, out),
        Some(Command::Migrate(args)) => run_migrate(args, &paths, out, err),
        Some(Command::Docs(args)) => run_docs(args, &paths, out, err),
        Some(Command::Growth(args)) => run_growth(args, &paths, out, err),
        Some(Command::Partition(args)) => run_partition(args, &paths, out, err),
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
        Some(Command::RestoreDrill(args)) => run_restore_drill(args, &paths, out, err),
        Some(Command::RunRoutine(args)) => {
            run_routine_command(args, &paths, &presentation, out, err)
        }
        Some(Command::ProfileRoutine(args)) => {
            profile_routine_command(args, &paths, &presentation, out, err)
        }
        Some(Command::Listen(args)) => run_listen(args, &paths, out, err),
        Some(Command::Notify(args)) => run_notify(args, &paths, out, err),
        Some(Command::Maintain(args)) => run_maintain(args, &paths, &presentation, out, err),
        Some(Command::ProfileQueries(args)) => run_profile_queries(args, &paths, out, err),
        Some(Command::IndexAdvisor(args)) => run_index_advisor(args, &paths, out, err),
        Some(Command::WalStats(args)) => run_wal_stats(args, &paths, out, err),
        Some(Command::PoolStats(args)) => run_pool_stats(args, &paths, out, err),
        Some(Command::MaskRules(args)) => run_mask_rules(args, &paths, out, err),
        Some(Command::Sync(args)) => run_sync(args, &paths, out, err),
        Some(Command::Seed(args)) => run_seed(args, &paths, out, err),
        Some(Command::Snippet(args)) => run_snippet(args, &paths, out, err),
        Some(Command::Recovery(args)) => run_recovery(args, &paths, out, err),
        Some(Command::Connect {
            target,
            check,
            file,
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
                let preload = if !file.is_empty() {
                    file.as_slice()
                } else {
                    cli.file.as_slice()
                };
                interactive::run(
                    target.as_deref(),
                    connection,
                    &paths,
                    &presentation,
                    cli.global.no_history,
                    preload,
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
            cli.file.as_slice(),
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

/// Arguments for `ignatius restore-drill` (`047-restore-drills`).
#[derive(Debug, Args, Clone)]
pub struct RestoreDrillArgs {
    /// Connection target: the source database, as a URI, keyword string, @profile, or name.
    pub target: Option<String>,
    /// Archive to restore: a custom, directory or tar dump.
    #[arg(short = 'b', long, value_name = "PATH")]
    pub backup: PathBuf,
    /// Override the generated scratch database name.
    #[arg(long, value_name = "NAME")]
    pub scratch_database: Option<String>,
    /// Keep the scratch database after a passing drill.
    #[arg(long)]
    pub keep: bool,
    /// Drop the scratch database even when the drill fails.
    #[arg(long)]
    pub drop_on_failure: bool,
    /// Permit creating and dropping a scratch database on a production server.
    #[arg(long)]
    pub allow_production: bool,
    /// Inspect the archive and print the exact steps without changing the server.
    #[arg(long)]
    pub dry_run: bool,
    /// Report format: table or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliActivityFormat::Table)]
    pub format: CliActivityFormat,
    /// Write the report to a file instead of standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
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
        exit_on_error: false,
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

/// Runs `pg_restore --list` and returns its bounded stdout.
fn pg_restore_list(path: &std::path::Path) -> Result<String, Diagnostic> {
    let output = std::process::Command::new("pg_restore")
        .arg("--list")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("could not execute pg_restore: {error}"),
                "inspecting the backup archive",
            )
            .next_action(
                "ensure PostgreSQL client utilities (pg_restore) are installed and on PATH",
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("text format dump") {
            return Err(drill_usage(crate::drill::DrillError::PlainSql));
        }
        let reason = crate::drill::bounded(
            &crate::diagnostics::redaction::redact_text(&stderr),
            crate::drill::MAX_FAILURE_CHARS,
        );
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            if reason.is_empty() {
                "pg_restore could not read the archive table of contents".to_owned()
            } else {
                format!("pg_restore could not read the archive table of contents: {reason}")
            },
            "inspecting the backup archive",
        )
        .likely_cause(
            "the file is not a pg_dump custom, directory or tar archive, or it is truncated",
        )
        .next_action("verify the archive with `pg_restore --list` and retry"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn drill_usage(error: crate::drill::DrillError) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        error.to_string(),
        "running the restore drill",
    )
    .likely_cause("the archive or the requested scratch database cannot be used")
    .next_action("fix the input and retry; no server changes were made")
}

fn run_restore_drill(
    args: &RestoreDrillArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let format = crate::drill::ArchiveFormat::detect(&args.backup).map_err(drill_usage)?;
    let listing = pg_restore_list(&args.backup)?;
    let summary = crate::drill::parse_toc(&listing).map_err(drill_usage)?;

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

    if target.environment.is_production() && !args.allow_production {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "restore drills create and drop a database on the target server",
            "running the restore drill",
        )
        .likely_cause("the connection target is classified as production")
        .next_action("pass --allow-production when you intend to drill against this server"));
    }

    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let scratch = args
        .scratch_database
        .clone()
        .unwrap_or_else(|| crate::drill::scratch_name(&target.database, &timestamp));
    crate::drill::validate_scratch_name(&target.database, &scratch).map_err(drill_usage)?;

    let request = crate::drill::DrillRequest {
        source_database: target.database.clone(),
        scratch_database: scratch.clone(),
        archive_path: args.backup.display().to_string(),
        format,
        keep: args.keep,
        drop_on_failure: args.drop_on_failure,
    };

    if args.dry_run {
        writeln!(out, "Restore drill plan (no changes will be made)").map_err(io_diagnostic)?;
        let _ = writeln!(
            out,
            "Archive: {} ({}, {})",
            request.archive_path,
            format.label(),
            summary.summary()
        );
        for (index, step) in crate::drill::planned_steps(&request, &summary)
            .iter()
            .enumerate()
        {
            let _ = writeln!(out, "{}. {}: {}", index + 1, step.action, step.detail);
        }
        return Ok(ExitCode::Success);
    }

    let host_str = match &target.host {
        crate::connection::Host::Tcp(host) => Some(host.clone()),
        crate::connection::Host::Socket(socket) => Some(socket.display().to_string()),
    };
    let restore_options = crate::transfer::RestoreOptions {
        database: scratch.clone(),
        host: host_str,
        port: Some(target.port),
        user: Some(target.user.clone()),
        input_path: args.backup.display().to_string(),
        clean: false,
        create: false,
        data_only: false,
        schema_only: false,
        exit_on_error: true,
        verbose: false,
    };
    use secrecy::ExposeSecret;
    let password = target.password.as_ref().map(|value| value.expose_secret());
    let (cmd, cmd_args, env) =
        crate::transfer::build_pg_restore_command(&restore_options, password, false);

    let started = std::time::Instant::now();
    let runtime = runtime()?;
    let timeout = std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds);
    let source_relations = runtime.block_on(async {
        let session = crate::postgres::session::connect_reported(&target, timeout)
            .await
            .value?;
        if session.drill_database_exists(&scratch).await? {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("scratch database {scratch} already exists"),
                "preparing the restore drill",
            )
            .likely_cause("a previous drill used the same name, or the name was chosen explicitly")
            .next_action("pass --scratch-database with a new name or drop the existing database"));
        }
        if !session.drill_can_create_database().await? {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("role {} cannot create a database", target.user),
                "preparing the restore drill",
            )
            .likely_cause("restore drills create and drop a scratch database on the source server")
            .next_action("grant CREATEDB to this role or run the drill as a role that has it"));
        }
        let count = session.drill_user_relation_count().await?;
        session.drill_create_database(&scratch).await?;
        Ok(count)
    })?;

    let mut report = crate::drill::DrillReport {
        source_database: target.database.clone(),
        scratch_database: scratch.clone(),
        archive: request.archive_path.clone(),
        archive_format: format.label().to_owned(),
        toc_entries: summary.entries,
        outcome: crate::drill::DrillOutcome::Passed,
        duration_ms: 0,
        source_relations: Some(source_relations),
        restored_relations: None,
        restored_bytes: None,
        scratch_kept: true,
        failure: None,
    };

    let restore_failure = run_drill_restore(&cmd, &cmd_args, &env);
    if let Some(reason) = restore_failure {
        report.outcome = crate::drill::DrillOutcome::Failed;
        report.failure = Some(reason);
    } else {
        let scratch_target = target.for_database(&scratch);
        let verified = runtime.block_on(async {
            let session = crate::postgres::session::connect_reported(&scratch_target, timeout)
                .await
                .value?;
            let count = session.drill_user_relation_count().await?;
            let bytes = session.drill_database_size().await?;
            Ok::<(u64, u64), Diagnostic>((count, bytes))
        });
        match verified {
            Ok((restored, bytes)) => {
                report.restored_relations = Some(restored);
                report.restored_bytes = Some(bytes);
                if restored != source_relations {
                    report.outcome = crate::drill::DrillOutcome::Failed;
                    report.failure = Some(format!(
                        "relation count mismatch: {source_relations} in the source, {restored} restored"
                    ));
                }
            }
            Err(error) => {
                report.outcome = crate::drill::DrillOutcome::Failed;
                report.failure = Some(crate::drill::bounded(
                    &error.to_string(),
                    crate::drill::MAX_FAILURE_CHARS,
                ));
            }
        }
    }

    if crate::drill::should_drop(report.outcome, args.keep, args.drop_on_failure) {
        let drop_result = runtime.block_on(async {
            let session = crate::postgres::session::connect_reported(&target, timeout)
                .await
                .value?;
            session.drill_drop_database(&scratch).await
        });
        match drop_result {
            Ok(()) => report.scratch_kept = false,
            Err(error) => {
                report.outcome = crate::drill::DrillOutcome::Failed;
                report.scratch_kept = true;
                let reason = crate::drill::bounded(
                    &format!("{error}; the scratch database was left in place"),
                    crate::drill::MAX_FAILURE_CHARS,
                );
                report.failure = Some(match report.failure.take() {
                    Some(existing) => format!("{existing}; {reason}"),
                    None => reason,
                });
            }
        }
    } else {
        report.scratch_kept = true;
    }
    report.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let rendered = match args.format {
        CliActivityFormat::Json => report.render_json(),
        CliActivityFormat::Table => report.render_table(),
    };
    if let Some(destination) = &args.output {
        let mut export = crate::query::export::Export::create(destination, args.force)?;
        export
            .writer()
            .write_all(rendered.as_bytes())
            .map_err(io_diagnostic)?;
        let _ = export.finish()?;
    } else {
        out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
    }

    let _ = writeln!(
        err,
        "Restore drill {} in {} ms; {} source and {} restored relation(s); scratch database {}.",
        report.outcome.label().to_ascii_lowercase(),
        report.duration_ms,
        report
            .source_relations
            .map(|count| count.to_string())
            .unwrap_or_else(|| "?".to_owned()),
        report
            .restored_relations
            .map(|count| count.to_string())
            .unwrap_or_else(|| "?".to_owned()),
        if report.scratch_kept {
            "kept"
        } else {
            "dropped"
        }
    );
    Ok(if report.passed() {
        ExitCode::Success
    } else {
        ExitCode::Findings
    })
}

/// Runs `pg_restore` with bounded, redacted diagnostics.
fn run_drill_restore(cmd: &str, cmd_args: &[String], env: &[(String, String)]) -> Option<String> {
    let mut process = std::process::Command::new(cmd);
    process.args(cmd_args).stdin(std::process::Stdio::null());
    for (key, value) in env {
        process.env(key, value);
    }
    let output = match process.output() {
        Ok(output) => output,
        Err(error) => {
            return Some(crate::drill::bounded(
                &format!("could not execute {cmd}: {error}"),
                crate::drill::MAX_FAILURE_CHARS,
            ));
        }
    };
    if output.status.success() {
        return None;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.as_ref()
    } else {
        stderr.as_ref()
    };
    let redacted = crate::diagnostics::redaction::redact_text(detail);
    Some(crate::drill::bounded(
        &format!(
            "pg_restore exited with code {:?}: {}",
            output.status.code(),
            redacted
        ),
        crate::drill::MAX_FAILURE_CHARS,
    ))
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

/// Output format for routine execution and profiling reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum CliRoutineFormat {
    /// Human-readable tabular summary.
    #[default]
    Table,
    /// Machine-readable structured JSON.
    Json,
}

/// Arguments for the run-routine command (`032-plpgsql-routine-runner-and-profiler`).
#[derive(Debug, Args, Clone)]
pub struct RunRoutineArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Routine name or qualified name (e.g. public.calculate_tax).
    pub routine: Option<String>,
    /// Argument values in key=value or positional format.
    #[arg(short = 'a', long = "param", value_name = "KEY=VALUE")]
    pub params: Vec<String>,
    /// Execute inside an automatic rollback savepoint (enabled by default).
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub rollback: bool,
    /// Output representation format: table (default) or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliRoutineFormat::Table)]
    pub format: CliRoutineFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    /// Explicit confirmation required for non-rollback execution against production.
    #[arg(long)]
    pub confirm_production: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Arguments for the profile-routine command (`032-plpgsql-routine-runner-and-profiler`).
#[derive(Debug, Args, Clone)]
pub struct ProfileRoutineArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Routine name or qualified name (e.g. public.calculate_tax).
    pub routine: Option<String>,
    /// Output representation format: table (default) or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliRoutineFormat::Table)]
    pub format: CliRoutineFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn parse_routine_identifier(raw: &str) -> (String, String) {
    if let Some((schema, name)) = raw.split_once('.') {
        (schema.trim().to_string(), name.trim().to_string())
    } else {
        ("public".to_string(), raw.trim().to_string())
    }
}

fn parse_cli_routine_args(param_entries: &[String]) -> std::collections::BTreeMap<String, String> {
    let mut args = std::collections::BTreeMap::new();
    for (idx, entry) in param_entries.iter().enumerate() {
        if let Some((k, v)) = entry.split_once('=') {
            args.insert(k.trim().to_string(), v.trim().to_string());
        } else {
            args.insert(format!("${}", idx + 1), entry.trim().to_string());
        }
    }
    args
}

fn run_routine_command(
    args: &RunRoutineArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let routine_raw = match args
        .routine
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(r) => r,
        None => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a routine name is required",
                "running routine with ignatius run-routine",
            )
            .next_action("pass a routine name, e.g. ignatius run-routine calculate_tax"));
        }
    };

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
            "executing routine with ignatius run-routine",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    if !args.rollback && target.environment.is_production() && !args.confirm_production {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "refusing non-rollback routine execution against a production database without confirmation",
            "executing routine without rollback",
        )
        .likely_cause("target is flagged as production and non-rollback execution can mutate data")
        .next_action("pass --confirm-production or keep rollback enabled"));
    }

    let (schema, name) = parse_routine_identifier(routine_raw);
    let parsed_args = parse_cli_routine_args(&args.params);

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let options = crate::routine::RoutineExecutionOptions {
        schema,
        name,
        args: parsed_args,
        rollback_savepoint: args.rollback,
        timeout_ms: Some(30_000),
    };

    let result = rt.block_on(session.execute_routine(&options))?;

    let rendered = match args.format {
        CliRoutineFormat::Table => crate::routine::render_routine_result_table(&result),
        CliRoutineFormat::Json => crate::routine::render_routine_result_json(&result),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Routine results written to {}", dest.display()).map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    if presentation.verbose {
        writeln!(
            err,
            "Executed in {:.2}ms (rollback: {})",
            result.elapsed_ms, result.rolled_back
        )
        .map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

fn profile_routine_command(
    args: &ProfileRoutineArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let routine_raw = match args
        .routine
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(r) => r,
        None => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a routine name is required",
                "profiling routine with ignatius profile-routine",
            )
            .next_action("pass a routine name, e.g. ignatius profile-routine calculate_tax"));
        }
    };

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
            "profiling routine with ignatius profile-routine",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let (schema, name) = parse_routine_identifier(routine_raw);

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let report = rt.block_on(session.profile_routine(&schema, &name))?;

    let rendered = match args.format {
        CliRoutineFormat::Table => crate::routine::render_routine_profile_table(&report),
        CliRoutineFormat::Json => crate::routine::render_routine_profile_json(&report),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Routine profile written to {}", dest.display()).map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    if presentation.verbose {
        writeln!(
            err,
            "Profiled {} findings in {:.2}ms",
            report.findings.len(),
            report.elapsed_ms
        )
        .map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Output stream format for pub/sub event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum CliStreamFormat {
    /// Human-readable aligned table.
    #[default]
    Table,
    /// Line-delimited JSON.
    Jsonl,
    /// Plain payload string only.
    Plain,
}

/// Arguments for the listen command (`033-listen-notify-and-maintenance-ops`).
#[derive(Debug, Args, Clone)]
pub struct ListenArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Pub/Sub channel topic to listen on.
    pub channel: Option<String>,
    /// Seconds to listen for incoming notifications before exiting (default: 10s).
    #[arg(short = 't', long, default_value_t = 10)]
    pub timeout: u64,
    /// Output representation format: table (default), jsonl, or plain.
    #[arg(short = 'f', long, value_enum, default_value_t = CliStreamFormat::Table)]
    pub format: CliStreamFormat,
    /// Destination file to write captured notification stream.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Arguments for the notify command (`033-listen-notify-and-maintenance-ops`).
#[derive(Debug, Args, Clone)]
pub struct NotifyArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Pub/Sub channel topic to dispatch message to.
    pub channel: Option<String>,
    /// Notification payload string (text or JSON).
    pub payload: Option<String>,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Maintenance subcommands for `ignatius maintain`.
#[derive(Debug, Subcommand, Clone)]
pub enum MaintainAction {
    /// Garbage-collect dead tuples and optimize table storage (VACUUM).
    Vacuum {
        /// Relation/table to vacuum (or omit for entire database).
        #[arg(value_name = "TABLE")]
        table: Option<String>,
        /// Reclaim maximum space by rewriting relation on disk (requires exclusive lock).
        #[arg(long)]
        full: bool,
        /// Freeze tuple transaction IDs.
        #[arg(long)]
        freeze: bool,
        /// Print progress details and dead tuple statistics.
        #[arg(long)]
        verbose: bool,
        /// Update optimizer statistics during vacuum.
        #[arg(long)]
        analyze: bool,
    },
    /// Collect optimizer query statistics on tables (ANALYZE).
    Analyze {
        /// Relation/table to analyze (or omit for entire database).
        #[arg(value_name = "TABLE")]
        table: Option<String>,
        /// Print progress details.
        #[arg(long)]
        verbose: bool,
    },
    /// Rebuild indexes to eliminate bloat and fragmentation (REINDEX).
    Reindex {
        /// Relation/table or index to reindex.
        #[arg(value_name = "TARGET")]
        target: Option<String>,
        /// Build the index concurrently without blocking writes.
        #[arg(long)]
        concurrently: bool,
        /// Reindex an entire schema.
        #[arg(long)]
        schema: bool,
        /// Reindex the entire database.
        #[arg(long)]
        database: bool,
    },
    /// Inspect active maintenance operations from pg_stat_progress_*.
    Progress,
}

/// Arguments for the maintain command (`033-listen-notify-and-maintenance-ops`).
#[derive(Debug, Args, Clone)]
pub struct MaintainArgs {
    /// The maintenance command to run.
    #[command(subcommand)]
    pub action: MaintainAction,
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Output representation format: table (default) or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliRoutineFormat::Table)]
    pub format: CliRoutineFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    /// Explicit confirmation required for maintenance on production targets.
    #[arg(long)]
    pub confirm_production: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_listen(
    args: &ListenArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let channel = match args.channel.as_deref() {
        Some(ch) if !ch.trim().is_empty() => ch.trim(),
        _ => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a pub/sub channel name is required",
                "listening on pub/sub channel",
            )
            .likely_cause("ignatius listen expects [TARGET] [CHANNEL]")
            .next_action("provide channel name: ignatius listen [TARGET] <CHANNEL>"));
        }
    };

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
            "capturing notification stream with ignatius listen",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    rt.block_on(session.listen_channel(channel))?;
    writeln!(
        err,
        "Listening on channel \"{}\" (timeout: {}s)...",
        channel, args.timeout
    )
    .map_err(io_diagnostic)?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.timeout);
    while std::time::Instant::now() < deadline {
        rt.block_on(tokio::time::sleep(std::time::Duration::from_millis(50)));
    }

    let events = rt.block_on(session.notifications());
    let _ = rt.block_on(session.unlisten_channel(channel));

    let rendered = match args.format {
        CliStreamFormat::Table => crate::ops::render_notifications_table(&events),
        CliStreamFormat::Jsonl => crate::ops::render_notifications_jsonl(&events),
        CliStreamFormat::Plain => {
            let mut buf = String::new();
            for ev in &events {
                buf.push_str(&ev.payload);
                buf.push('\n');
            }
            buf
        }
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(
            err,
            "Captured {} notifications to {}",
            events.len(),
            dest.display()
        )
        .map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

fn run_notify(
    args: &NotifyArgs,
    paths: &Paths,
    out: &mut impl Write,
    _err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let channel = match args.channel.as_deref() {
        Some(ch) if !ch.trim().is_empty() => ch.trim(),
        _ => {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a pub/sub channel name is required",
                "dispatching notification with ignatius notify",
            )
            .likely_cause("ignatius notify expects [TARGET] [CHANNEL] [PAYLOAD]")
            .next_action("pass a channel name, e.g. ignatius notify order_events '{\"id\": 1}'"));
        }
    };

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    rt.block_on(session.notify_channel(channel, args.payload.as_deref()))?;
    writeln!(out, "Dispatched notification to channel \"{}\"", channel).map_err(io_diagnostic)?;

    Ok(ExitCode::Success)
}

fn run_maintain(
    args: &MaintainArgs,
    paths: &Paths,
    presentation: &Presentation,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing maintenance command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    match &args.action {
        MaintainAction::Progress => {
            let progress = rt.block_on(session.query_maintenance_progress())?;
            let rendered = match args.format {
                CliRoutineFormat::Table => crate::ops::render_maintenance_progress_table(&progress),
                CliRoutineFormat::Json => crate::ops::render_maintenance_progress_json(&progress),
            };

            if let Some(ref dest) = args.output {
                std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
                writeln!(err, "Progress report written to {}", dest.display())
                    .map_err(io_diagnostic)?;
            } else {
                write!(out, "{rendered}").map_err(io_diagnostic)?;
            }
        }
        MaintainAction::Vacuum {
            table,
            full,
            freeze,
            verbose,
            analyze,
        } => {
            let kind = crate::ops::MaintenanceKind::Vacuum {
                full: *full,
                freeze: *freeze,
                verbose: *verbose,
                analyze: *analyze,
            };
            if target.environment.is_production() && *full && !args.confirm_production {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "refusing VACUUM FULL against production database without confirmation",
                    "executing maintenance command",
                )
                .likely_cause("VACUUM FULL acquires AccessExclusiveLock, blocking concurrent reads and writes")
                .next_action("pass --confirm-production to confirm table-locking maintenance"));
            }

            let opts = crate::ops::MaintenanceOptions {
                kind,
                target: table.clone(),
                confirm_production: args.confirm_production,
            };

            let res = rt.block_on(session.execute_maintenance(&opts))?;
            for notice in &res.notices {
                writeln!(err, "{notice}").map_err(io_diagnostic)?;
            }
            writeln!(out, "{}", res.message).map_err(io_diagnostic)?;
            if presentation.verbose {
                writeln!(err, "Executed: {}", res.executed_sql).map_err(io_diagnostic)?;
            }
        }
        MaintainAction::Analyze { table, verbose } => {
            let kind = crate::ops::MaintenanceKind::Analyze { verbose: *verbose };
            let opts = crate::ops::MaintenanceOptions {
                kind,
                target: table.clone(),
                confirm_production: args.confirm_production,
            };

            let res = rt.block_on(session.execute_maintenance(&opts))?;
            for notice in &res.notices {
                writeln!(err, "{notice}").map_err(io_diagnostic)?;
            }
            writeln!(out, "{}", res.message).map_err(io_diagnostic)?;
        }
        MaintainAction::Reindex {
            target: reindex_target,
            concurrently,
            schema,
            database,
        } => {
            let scope = if *database {
                crate::ops::ReindexScope::Database
            } else if *schema {
                crate::ops::ReindexScope::Schema
            } else {
                crate::ops::ReindexScope::Table
            };

            let kind = crate::ops::MaintenanceKind::Reindex {
                concurrently: *concurrently,
                scope,
            };

            if target.environment.is_production() && !*concurrently && !args.confirm_production {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "refusing non-concurrent REINDEX against production database without confirmation",
                    "executing maintenance command",
                )
                .likely_cause("REINDEX without CONCURRENTLY locks out concurrent reads and writes")
                .next_action("pass --concurrently or --confirm-production"));
            }

            let opts = crate::ops::MaintenanceOptions {
                kind,
                target: reindex_target.clone(),
                confirm_production: args.confirm_production,
            };

            let res = rt.block_on(session.execute_maintenance(&opts))?;
            for notice in &res.notices {
                writeln!(err, "{notice}").map_err(io_diagnostic)?;
            }
            writeln!(out, "{}", res.message).map_err(io_diagnostic)?;
        }
    }

    Ok(ExitCode::Success)
}

fn io_diagnostic(err: std::io::Error) -> Diagnostic {
    Diagnostic::new(DiagnosticKind::Internal, err.to_string(), "writing output")
        .next_action("check the output stream and available space")
}

/// Writes a generated report through an exclusive sibling temp and a rename.
fn write_report_atomic(path: &Path, contents: &str) -> Result<(), Diagnostic> {
    use std::io::Write as _;

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("report");
    let temporary = path.with_file_name(format!(".{file_name}.tmp.{}.{nonce}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    let mut file = options.open(&temporary).map_err(io_diagnostic)?;
    if let Err(error) = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
    {
        let _ = std::fs::remove_file(&temporary);
        return Err(io_diagnostic(error));
    }
    drop(file);
    // Windows refuses a rename onto an existing file. The callers have
    // already decided that replacement is allowed, so remove the destination
    // there; on Unix the rename stays atomic.
    #[cfg(windows)]
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(io_diagnostic(error));
    }
    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && let Ok(directory) = std::fs::File::open(parent)
    {
        let _ = directory.sync_all();
    }
    Ok(())
}

/// Output representation format for query performance profiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CliPerformanceFormat {
    /// Aligned tabular overview with statistics capsules.
    #[default]
    Table,
    /// Structured JSON snapshot document.
    Json,
    /// Plain tab-delimited text.
    Plain,
}

/// Arguments for the `profile-queries` command.
#[derive(Debug, Args, Clone)]
pub struct ProfileQueriesArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Metric to rank queries by (mean, total, calls, hit-ratio, io, rows).
    #[arg(short = 's', long, value_name = "METRIC", default_value = "total")]
    pub sort: String,
    /// Maximum number of statements to return (1..200).
    #[arg(short = 'l', long, value_name = "LIMIT", default_value_t = crate::perf::DEFAULT_PERFORMANCE_LIMIT)]
    pub limit: usize,
    /// Minimum execution calls required to include a statement.
    #[arg(long, value_name = "COUNT", default_value_t = 0)]
    pub min_calls: u64,
    /// Output representation format: table (default), json, or plain.
    #[arg(short = 'f', long, value_enum, default_value_t = CliPerformanceFormat::Table)]
    pub format: CliPerformanceFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    /// Reset execution statistics in pg_stat_statements (SELECT pg_stat_statements_reset()).
    #[arg(long)]
    pub reset: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_profile_queries(
    args: &ProfileQueriesArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing profile-queries command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    if args.reset {
        rt.block_on(session.reset_performance_stats())?;
        writeln!(
            out,
            "Reset pg_stat_statements query execution statistics successfully."
        )
        .map_err(io_diagnostic)?;
        return Ok(ExitCode::Success);
    }

    let sort_order =
        crate::perf::PerformanceSortOrder::from_str_loose(&args.sort).ok_or_else(|| {
            Diagnostic::new(
                DiagnosticKind::Usage,
                format!("unrecognized sort metric '{}'", args.sort),
                "executing profile-queries command",
            )
            .likely_cause(format!(
                "accepted sort metrics are: {}",
                crate::perf::PerformanceSortOrder::ACCEPTED.join(", ")
            ))
            .next_action("choose one of: mean, total, calls, hit-ratio, io, rows")
        })?;

    let mut snapshot = rt.block_on(session.query_performance(sort_order, args.limit))?;

    if args.min_calls > 0 {
        snapshot.queries.retain(|q| q.calls >= args.min_calls);
    }

    let rendered = match args.format {
        CliPerformanceFormat::Table => snapshot.render_table(),
        CliPerformanceFormat::Json => snapshot.render_json().map_err(|e| {
            Diagnostic::new(DiagnosticKind::Usage, e.to_string(), "serializing JSON")
        })?,
        CliPerformanceFormat::Plain => snapshot.render_plain(),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Performance report written to {}", dest.display()).map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Output representation format for index advisor forensics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CliAdvisorFormat {
    /// Aligned tabular overview with status capsules.
    #[default]
    Table,
    /// Structured JSON report document.
    Json,
    /// Plain text report.
    Plain,
}

/// Filter section for index advisor reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CliAdvisorSection {
    /// Include all diagnostics (redundant, bloat, unindexed, hypo).
    #[default]
    All,
    /// Redundant and duplicate indexes.
    Redundant,
    /// B-Tree index bloat forensics.
    Bloat,
    /// Missing foreign key indexes.
    Unindexed,
    /// Hypothetical index test results.
    Hypo,
}

impl From<CliAdvisorSection> for crate::advisor::AdvisorSection {
    fn from(val: CliAdvisorSection) -> Self {
        match val {
            CliAdvisorSection::All => crate::advisor::AdvisorSection::All,
            CliAdvisorSection::Redundant => crate::advisor::AdvisorSection::Redundant,
            CliAdvisorSection::Bloat => crate::advisor::AdvisorSection::Bloat,
            CliAdvisorSection::Unindexed => crate::advisor::AdvisorSection::Unindexed,
            CliAdvisorSection::Hypo => crate::advisor::AdvisorSection::Hypo,
        }
    }
}

/// Arguments for the `index-advisor` command.
#[derive(Debug, Args, Clone)]
pub struct IndexAdvisorArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Report section to display (all, redundant, bloat, unindexed, hypo).
    #[arg(short = 's', long, value_enum, default_value_t = CliAdvisorSection::All)]
    pub section: CliAdvisorSection,
    /// Output representation format: table (default), json, or plain.
    #[arg(short = 'f', long, value_enum, default_value_t = CliAdvisorFormat::Table)]
    pub format: CliAdvisorFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
    /// Minimum index size in MB to report for bloat or redundant indexes.
    #[arg(long, value_name = "MB", default_value_t = 0)]
    pub min_size: u64,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_index_advisor(
    args: &IndexAdvisorArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing index-advisor command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let min_bytes = args.min_size.saturating_mul(1024 * 1024);
    let report = rt.block_on(session.analyze_indexes(min_bytes))?;

    let section = crate::advisor::AdvisorSection::from(args.section);
    let rendered = match args.format {
        CliAdvisorFormat::Table => report.render_table(section),
        CliAdvisorFormat::Json => report.render_json(),
        CliAdvisorFormat::Plain => report.render_plain(section),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Index advisor report written to {}", dest.display())
            .map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Output representation format for transaction and WAL telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CliTelemetryFormat {
    /// Aligned ASCII table with headers.
    #[default]
    Table,
    /// Structured JSON payload.
    Json,
    /// Plain machine-readable text stream.
    Plain,
}

/// Filter section for transaction and WAL telemetry reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CliTelemetrySection {
    /// Include all telemetry diagnostics.
    #[default]
    All,
    /// Checkpointer and background writer operational metrics.
    Checkpointer,
    /// Write-Ahead Logging (WAL) and archiver telemetry.
    Wal,
    /// Database transaction throughput, rollbacks, and deadlocks.
    Transactions,
    /// Transaction ID (XID) freeze and wraparound horizon forensics.
    Freeze,
    /// Streaming replication standbys and replication lag.
    Replication,
}

impl From<CliTelemetrySection> for crate::telemetry::TelemetrySection {
    fn from(val: CliTelemetrySection) -> Self {
        match val {
            CliTelemetrySection::All => crate::telemetry::TelemetrySection::All,
            CliTelemetrySection::Checkpointer => crate::telemetry::TelemetrySection::Checkpointer,
            CliTelemetrySection::Wal => crate::telemetry::TelemetrySection::Wal,
            CliTelemetrySection::Transactions => crate::telemetry::TelemetrySection::Transactions,
            CliTelemetrySection::Freeze => crate::telemetry::TelemetrySection::Freeze,
            CliTelemetrySection::Replication => crate::telemetry::TelemetrySection::Replication,
        }
    }
}

/// Arguments for the `wal-stats` command.
#[derive(Debug, Args, Clone)]
pub struct WalStatsArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Report section to display (all, checkpointer, wal, transactions, freeze, replication).
    #[arg(short = 's', long, value_enum, default_value_t = CliTelemetrySection::All)]
    pub section: CliTelemetrySection,
    /// Output representation format: table (default), json, or plain.
    #[arg(short = 'f', long, value_enum, default_value_t = CliTelemetryFormat::Table)]
    pub format: CliTelemetryFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite destination file without prompting if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_wal_stats(
    args: &WalStatsArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing wal-stats command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let report = rt.block_on(session.wal_and_transaction_telemetry())?;

    let section = crate::telemetry::TelemetrySection::from(args.section);
    let rendered = match args.format {
        CliTelemetryFormat::Table => report.render_table(section),
        CliTelemetryFormat::Json => report.render_json(),
        CliTelemetryFormat::Plain => report.render_plain(section),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(
            err,
            "WAL and transaction report written to {}",
            dest.display()
        )
        .map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Section filter for connection pool reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CliPoolSection {
    /// All sections (server saturation, pool allocations, and waiting clients).
    #[default]
    All,
    /// Pool allocations and queue metrics.
    Pools,
    /// Server connection capacity, state breakdown, and headroom.
    Server,
    /// Waiting or lock-blocked client forensics.
    Wait,
}

impl From<CliPoolSection> for crate::pool::PoolSection {
    fn from(val: CliPoolSection) -> Self {
        match val {
            CliPoolSection::All => crate::pool::PoolSection::All,
            CliPoolSection::Pools => crate::pool::PoolSection::Pools,
            CliPoolSection::Server => crate::pool::PoolSection::Server,
            CliPoolSection::Wait => crate::pool::PoolSection::Wait,
        }
    }
}

/// Arguments for the `pool-stats` command.
#[derive(Debug, Args, Clone)]
pub struct PoolStatsArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Report section to display (all, pools, server, wait).
    #[arg(short = 's', long, value_enum, default_value_t = CliPoolSection::All)]
    pub section: CliPoolSection,
    /// Output representation format: table (default), json, or plain.
    #[arg(short = 'f', long, value_enum, default_value_t = CliTelemetryFormat::Table)]
    pub format: CliTelemetryFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite destination file without prompting if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_pool_stats(
    args: &PoolStatsArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing pool-stats command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let report = rt.block_on(session.connection_pool_telemetry())?;

    let section = crate::pool::PoolSection::from(args.section);
    let rendered = match args.format {
        CliTelemetryFormat::Table => report.render_table(section),
        CliTelemetryFormat::Json => report.render_json(),
        CliTelemetryFormat::Plain => report.render_plain(section),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Connection pool report written to {}", dest.display())
            .map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Output format for the data masking CLI command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliMaskFormat {
    /// Formatted ASCII / Unicode tabular report.
    Table,
    /// Structured JSON payload.
    Json,
    /// Plain text tabular representation.
    Plain,
    /// Executable PostgreSQL view definitions DDL.
    Sql,
}

/// Arguments for the `mask-rules` command.
#[derive(Debug, Args, Clone)]
pub struct MaskRulesArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Filter tables and columns to one schema.
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Prefix prepended to generated view names.
    #[arg(long, default_value = "masked_")]
    pub view_prefix: String,
    /// Output representation format: table (default), json, plain, or sql.
    #[arg(short = 'f', long, value_enum, default_value = "table")]
    pub format: CliMaskFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite destination file without prompting if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_mask_rules(
    args: &MaskRulesArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing mask-rules command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let report = rt.block_on(session.analyze_data_masking(args.schema.as_deref()))?;

    let rendered = match args.format {
        CliMaskFormat::Table => report.render_table(),
        CliMaskFormat::Json => report.render_json(),
        CliMaskFormat::Plain => report.render_plain(),
        CliMaskFormat::Sql => report.render_sql(&args.view_prefix),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Data masking report written to {}", dest.display())
            .map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Output format for cross-database synchronization telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliSyncFormat {
    /// Human-readable aligned tabular report.
    Table,
    /// Structured JSON telemetry payload.
    Json,
    /// Plain text tabular representation.
    Plain,
}

/// Arguments for the cross-database copy and sync pipeline command.
#[derive(Debug, Clone, Args)]
pub struct SyncArgs {
    /// Source connection target: a URI, a keyword string, @profile, or a database name.
    #[arg(short = 's', long, value_name = "SOURCE_TARGET")]
    pub source: Option<String>,
    /// Target connection target: a URI, a keyword string, @profile, or a database name.
    #[arg(short = 't', long, value_name = "TARGET_TARGET")]
    pub target: Option<String>,
    /// Optional positional connection target used as source if --source is omitted.
    pub target_pos: Option<String>,
    /// Filter synchronization to one schema.
    #[arg(long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Specific tables to synchronize (can be repeated). If omitted, all base tables in the schema/database are synchronized.
    #[arg(long = "table", value_name = "TABLE")]
    pub tables: Vec<String>,
    /// Truncate target tables in reverse topological order before streaming data.
    #[arg(long)]
    pub truncate: bool,
    /// Use CASCADE when truncating tables.
    #[arg(long)]
    pub cascade: bool,
    /// Run dry-run verification queries without writing or truncating.
    #[arg(long)]
    pub dry_run: bool,
    /// Output representation format: table (default), json, or plain.
    #[arg(short = 'f', long, value_enum, default_value = "table")]
    pub format: CliSyncFormat,
    /// Destination file to write output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite destination file without prompting if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_sync(
    args: &SyncArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing sync command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

    if args.target.is_none() && !args.dry_run {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "a target database connection is required for data synchronization",
            "executing sync command",
        )
        .likely_cause("neither --target nor --dry-run was provided")
        .next_action("provide --target <URI> or pass --dry-run to verify without writing"));
    }

    let loaded = config::load(paths)?;
    let src_str = args.source.as_deref().or(args.target_pos.as_deref());
    let (src_requested, src_conn_args) =
        resolve_target_and_profile(src_str, &args.connection, &loaded.config)?;
    let src_target = crate::connection::resolve(
        src_requested,
        &src_conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let src_target = authenticate_target(src_target, &loaded.config)?;

    let rt = runtime()?;
    let src_session = rt.block_on(crate::postgres::session::connect(
        &src_target,
        std::time::Duration::from_secs(30),
    ))?;

    let report = if let Some(ref tgt_str) = args.target {
        let (tgt_requested, tgt_conn_args) = resolve_target_and_profile(
            Some(tgt_str.as_str()),
            &ConnectionOptions::default(),
            &loaded.config,
        )?;
        let tgt_target = crate::connection::resolve(
            tgt_requested,
            &tgt_conn_args,
            &EnvSnapshot::from_process(),
            &loaded.config.connection,
        )?;
        let tgt_target = authenticate_target(tgt_target, &loaded.config)?;
        let tgt_session = rt.block_on(crate::postgres::session::connect(
            &tgt_target,
            std::time::Duration::from_secs(30),
        ))?;
        rt.block_on(src_session.execute_sync_pipeline(
            &tgt_session,
            args.schema.as_deref(),
            &args.tables,
            args.truncate,
            args.cascade,
            args.dry_run,
        ))?
    } else {
        // Dry-run mode without target: verify source tables and topological ordering
        rt.block_on(src_session.execute_sync_pipeline(
            &src_session,
            args.schema.as_deref(),
            &args.tables,
            false,
            false,
            true,
        ))?
    };

    let rendered = match args.format {
        CliSyncFormat::Table => report.render_table(),
        CliSyncFormat::Json => report.render_json(),
        CliSyncFormat::Plain => report.render_plain(),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Sync report written to {}", dest.display()).map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Output formats for the synthetic seed CLI subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CliSeedFormat {
    /// Standard executable PostgreSQL INSERT statements inside a transaction.
    #[default]
    Sql,
    /// Newline-delimited JSON (NDJSON) records.
    Jsonl,
    /// CSV formatted records per table.
    Csv,
    /// Formatted human-readable summary table.
    Table,
}

/// Arguments for the synthetic test fixture generator command.
#[derive(Debug, Clone, Args)]
pub struct SeedArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Filter table discovery to one schema.
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Filter to specific table name(s).
    #[arg(short = 't', long = "table", value_name = "TABLE")]
    pub tables: Vec<String>,
    /// Number of synthetic rows to generate per table (default: 10).
    #[arg(short = 'n', long = "rows", default_value_t = 10)]
    pub rows: usize,
    /// Deterministic 64-bit PRNG seed (default: 42).
    #[arg(long = "seed", default_value_t = 42)]
    pub seed: u64,
    /// Directly execute and insert generated fixtures into the database.
    #[arg(long)]
    pub insert: bool,
    /// Output serialization format (sql, jsonl, csv, table).
    #[arg(short = 'f', long, value_enum, default_value_t = CliSeedFormat::Sql)]
    pub format: CliSeedFormat,
    /// Destination file for output; defaults to standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Overwrite destination file without prompting if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

fn run_seed(
    args: &SeedArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
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
            "executing seed command",
        )
        .likely_cause("the target output file already exists")
        .next_action("pass --force to overwrite the file"));
    }

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

    let rt = runtime()?;
    let session = rt.block_on(crate::postgres::session::connect(
        &target,
        std::time::Duration::from_secs(30),
    ))?;

    let all_tables = rt.block_on(session.analyze_seed_plan(args.schema.as_deref()))?;
    let filtered_tables: Vec<crate::seed::TableGenerator> = if args.tables.is_empty() {
        all_tables
    } else {
        all_tables
            .into_iter()
            .filter(|t| {
                args.tables
                    .iter()
                    .any(|req| req.eq_ignore_ascii_case(&t.key.table))
            })
            .collect()
    };

    let report = crate::seed::generate_fixtures(&filtered_tables, args.rows, args.seed);

    if args.insert {
        rt.block_on(session.execute_synthetic_seed(&report))?;
        writeln!(
            err,
            "Successfully inserted {} synthetic rows across {} tables into database",
            report.total_rows,
            report.tables.len()
        )
        .map_err(io_diagnostic)?;
    }

    let rendered = match args.format {
        CliSeedFormat::Sql => report.render_sql(),
        CliSeedFormat::Jsonl => report.render_jsonl(),
        CliSeedFormat::Csv => report.render_csv(),
        CliSeedFormat::Table => report.render_table(),
    };

    if let Some(ref dest) = args.output {
        std::fs::write(dest, &rendered).map_err(io_diagnostic)?;
        writeln!(err, "Synthetic fixtures written to {}", dest.display()).map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    Ok(ExitCode::Success)
}

/// Arguments for the snippet command.
#[derive(Debug, Args, Clone)]
pub struct SnippetArgs {
    #[command(subcommand)]
    pub action: SnippetAction,
}

/// Actions available for the snippet command.
#[derive(Debug, Subcommand, Clone)]
pub enum SnippetAction {
    /// List available built-in and user snippets.
    List {
        /// Filter by category: all, admin, performance, patterns, user.
        #[arg(short = 'c', long, value_enum, default_value = "all")]
        category: CliSnippetCategory,
        /// Output format: table or json.
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: CliSnippetFormat,
    },
    /// Display a snippet's SQL or metadata.
    Show {
        /// Unique snippet identifier or name.
        name: String,
        /// Output format: sql or json.
        #[arg(short = 'f', long, value_enum, default_value = "sql")]
        format: CliSnippetShowFormat,
    },
    /// Save a reusable SQL snippet to disk.
    Add {
        /// Unique snippet name (alphanumeric, hyphens, underscores).
        name: String,
        /// Human-readable title.
        #[arg(short = 't', long)]
        title: Option<String>,
        /// Description of the snippet's purpose.
        #[arg(short = 'd', long)]
        description: Option<String>,
        /// Categorical domain: admin, performance, patterns, user.
        #[arg(short = 'c', long, value_enum, default_value = "user")]
        category: CliSnippetCategory,
        /// Path to SQL file to read snippet contents from. Use '-' for stdin.
        #[arg(short = 'f', long, value_name = "PATH")]
        file: Option<PathBuf>,
        /// Inline SQL statement for the snippet.
        #[arg(short = 's', long, value_name = "SQL", conflicts_with = "file")]
        sql: Option<String>,
    },
}

/// Categorical domain for snippet filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliSnippetCategory {
    All,
    Admin,
    Performance,
    Patterns,
    User,
}

impl From<CliSnippetCategory> for crate::snippet::SnippetCategory {
    fn from(val: CliSnippetCategory) -> Self {
        match val {
            CliSnippetCategory::All => Self::All,
            CliSnippetCategory::Admin => Self::Admin,
            CliSnippetCategory::Performance => Self::Performance,
            CliSnippetCategory::Patterns => Self::Patterns,
            CliSnippetCategory::User => Self::User,
        }
    }
}

/// Output format for snippet listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliSnippetFormat {
    Table,
    Json,
}

/// Output format for snippet display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliSnippetShowFormat {
    Sql,
    Json,
}

fn run_snippet(
    args: &SnippetArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let snippets_dir = paths.config_dir.join("snippets");
    let mut library = crate::snippet::SnippetLibrary::new();
    library.load_user_directory(&snippets_dir);

    match &args.action {
        SnippetAction::List { category, format } => {
            let cat: crate::snippet::SnippetCategory = (*category).into();
            library.category_filter = cat;
            let snippets = library.filtered_snippets();

            match format {
                CliSnippetFormat::Json => {
                    let json = serde_json::to_string_pretty(&snippets).map_err(|e| {
                        Diagnostic::new(
                            DiagnosticKind::Usage,
                            format!("failed to serialize snippets to JSON: {e}"),
                            "formatting snippet list",
                        )
                    })?;
                    writeln!(out, "{json}").map_err(io_diagnostic)?;
                }
                CliSnippetFormat::Table => {
                    if snippets.is_empty() {
                        writeln!(err, "No snippets found for the selected category.")
                            .map_err(io_diagnostic)?;
                    } else {
                        writeln!(
                            out,
                            "{:<20} {:<12} {:<8} TITLE",
                            "NAME", "CATEGORY", "BUILTIN"
                        )
                        .map_err(io_diagnostic)?;
                        writeln!(
                            out,
                            "{:<20} {:<12} {:<8} -----------------------------------",
                            "--------------------", "------------", "--------",
                        )
                        .map_err(io_diagnostic)?;
                        for s in snippets {
                            let builtin_label = if s.is_builtin { "yes" } else { "no" };
                            writeln!(
                                out,
                                "{:<20} {:<12} {:<8} {}",
                                s.name,
                                s.category.label(),
                                builtin_label,
                                s.title
                            )
                            .map_err(io_diagnostic)?;
                        }
                    }
                }
            }
        }
        SnippetAction::Show { name, format } => {
            let target_name = name.trim().to_lowercase();
            let snippet = library.snippets.iter().find(|s| {
                s.id.to_lowercase() == target_name || s.name.to_lowercase() == target_name
            });

            let Some(snippet) = snippet else {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("snippet '{name}' was not found in library"),
                    "executing snippet show",
                )
                .likely_cause("the snippet name does not exist in built-ins or user library")
                .next_action("run 'ignatius snippet list' to view available snippets"));
            };

            match format {
                CliSnippetShowFormat::Sql => {
                    writeln!(out, "{}", snippet.sql).map_err(io_diagnostic)?;
                }
                CliSnippetShowFormat::Json => {
                    let json = serde_json::to_string_pretty(snippet).map_err(|e| {
                        Diagnostic::new(
                            DiagnosticKind::Usage,
                            format!("failed to serialize snippet to JSON: {e}"),
                            "formatting snippet show",
                        )
                    })?;
                    writeln!(out, "{json}").map_err(io_diagnostic)?;
                }
            }
        }
        SnippetAction::Add {
            name,
            title,
            description,
            category,
            file,
            sql,
        } => {
            let content = if let Some(raw_sql) = sql {
                raw_sql.clone()
            } else if let Some(path) = file {
                if path.as_os_str() == "-" {
                    let mut buffer = String::new();
                    std::io::stdin()
                        .read_to_string(&mut buffer)
                        .map_err(io_diagnostic)?;
                    buffer
                } else {
                    std::fs::read_to_string(path).map_err(|e| {
                        Diagnostic::new(
                            DiagnosticKind::Usage,
                            format!("failed to read snippet SQL file {}: {e}", path.display()),
                            "reading snippet source file",
                        )
                        .likely_cause("file does not exist or permission denied")
                    })?
                }
            } else {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "either --sql or --file must be specified",
                    "adding user snippet",
                )
                .next_action(
                    "provide SQL text via '--sql <SQL>' or '--file <PATH>' (or '--file -' for stdin)",
                ));
            };

            if content.trim().is_empty() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "snippet SQL content cannot be empty",
                    "adding user snippet",
                ));
            }

            let effective_title = title.as_deref().unwrap_or(name.as_str());
            let effective_desc = description.as_deref().unwrap_or("User-defined snippet.");
            let cat: crate::snippet::SnippetCategory = (*category).into();

            let saved = library.add_user_snippet(
                name,
                effective_title,
                effective_desc,
                cat,
                &content,
                Some(&snippets_dir),
            )?;

            if let Some(ref path) = saved.path {
                writeln!(err, "Saved snippet '{}' to {}", saved.id, path.display())
                    .map_err(io_diagnostic)?;
            } else {
                writeln!(err, "Saved snippet '{}'", saved.id).map_err(io_diagnostic)?;
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Arguments for the schema documentation generator (`046-schema-documentation-generator`).
#[derive(Debug, Args, Clone)]
pub struct DocsArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Schema to document.
    #[arg(long, value_name = "SCHEMA", default_value = "public")]
    pub schema: String,
    /// Output format: markdown, json or mermaid.
    #[arg(short = 'f', long, value_enum, default_value = "markdown")]
    pub format: CliDocsFormat,
    /// Write the document to a file instead of standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Output format for `ignatius docs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliDocsFormat {
    Markdown,
    Json,
    Mermaid,
}

fn run_docs(
    args: &DocsArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(resolved, &loaded.config)?;
    let runtime = runtime()?;
    let document = runtime.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        session.schema_documentation(&args.schema).await
    })?;

    let rendered = match args.format {
        CliDocsFormat::Markdown => crate::dbdoc::render_markdown(&document),
        CliDocsFormat::Mermaid => crate::dbdoc::render_mermaid(&document),
        CliDocsFormat::Json => crate::dbdoc::render_json(&document).map_err(|error| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                format!("failed to serialize the data dictionary: {error}"),
                "formatting schema documentation",
            )
        })?,
    };
    if let Some(destination) = args.output.as_ref() {
        if destination.exists() && !args.force {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "refusing to overwrite existing file {}",
                    destination.display()
                ),
                "writing schema documentation",
            )
            .likely_cause("the output file already exists")
            .next_action("pass --force to replace it"));
        }
        write_report_atomic(destination, &rendered)?;
        writeln!(
            err,
            "Wrote {} relation(s), {} column(s) to {}",
            document.relations.len(),
            document.column_count(),
            destination.display()
        )
        .map_err(io_diagnostic)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }
    Ok(ExitCode::Success)
}

/// Arguments for the retention and growth audit (`049-retention-and-growth-audit`).
#[derive(Debug, Args, Clone)]
pub struct GrowthArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Restrict every section to one schema.
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Number of relations in the size ranking, from 1 to 200.
    #[arg(long, value_name = "N", default_value_t = crate::growth::DEFAULT_TOP)]
    pub top: usize,
    /// Size at which an unpartitioned table becomes a partitioning candidate, in bytes.
    #[arg(long, value_name = "BYTES", default_value_t = crate::growth::DEFAULT_LARGE_TABLE_BYTES)]
    pub large_table_bytes: u64,
    /// Output format: table or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliActivityFormat::Table)]
    pub format: CliActivityFormat,
    /// Exit 1 when findings meet this threshold.
    #[arg(long, value_enum, default_value_t = CliGrowthFailOn::Never)]
    pub fail_on: CliGrowthFailOn,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Finding threshold that fails `ignatius growth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliGrowthFailOn {
    /// Always exit 0 after a successful audit.
    Never,
    /// Exit 1 when a warning or critical finding is present.
    Warning,
    /// Exit 1 only when a critical finding is present.
    Critical,
}

impl CliGrowthFailOn {
    /// Maps the CLI threshold to the domain policy.
    #[must_use]
    pub const fn into_domain(self) -> crate::growth::GrowthFailOn {
        match self {
            Self::Never => crate::growth::GrowthFailOn::Never,
            Self::Warning => crate::growth::GrowthFailOn::Warning,
            Self::Critical => crate::growth::GrowthFailOn::Critical,
        }
    }
}

fn run_growth(
    args: &GrowthArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    if args.top == 0 || args.top > crate::growth::MAX_RELATIONS {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "--top must be between 1 and {}",
                crate::growth::MAX_RELATIONS
            ),
            "auditing growth",
        )
        .next_action("pass a smaller --top value"));
    }
    if args.large_table_bytes == 0 {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "--large-table-bytes must be at least 1",
            "auditing growth",
        )
        .next_action("pass a byte count above zero"));
    }

    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(resolved, &loaded.config)?;

    let options = crate::growth::GrowthOptions {
        schema: args.schema.clone(),
        top: args.top,
        large_table_bytes: args.large_table_bytes,
        fail_on: args.fail_on.into_domain(),
    }
    .bounded();

    let snapshot = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let result = session.growth_snapshot(&options).await;
        drop(session);
        result
    })?;

    let rendered = match args.format {
        CliActivityFormat::Json => snapshot.render_json(&options),
        CliActivityFormat::Table => snapshot.render_table(&options),
    };
    out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;

    if let Some(threshold) = options.fail_on.threshold()
        && snapshot.has_at_or_above(threshold)
    {
        writeln!(
            err,
            "Growth audit: {} critical and {} warning finding(s); the {} threshold was met.",
            snapshot.count_at(crate::growth::GrowthSeverity::Critical),
            snapshot.count_at(crate::growth::GrowthSeverity::Warning),
            threshold.label().to_ascii_lowercase()
        )
        .map_err(io_diagnostic)?;
        return Ok(ExitCode::Findings);
    }
    Ok(ExitCode::Success)
}

/// Arguments for `ignatius partition`.
#[derive(Debug, Args, Clone)]
pub struct PartitionArgs {
    #[command(subcommand)]
    pub action: PartitionAction,
}

/// Partition subcommands.
#[derive(Debug, clap::Subcommand, Clone)]
pub enum PartitionAction {
    /// Show partitioned tables, their keys, bounds, sizes and row estimates.
    Status(PartitionStatusArgs),
    /// Generate reviewed DDL for future intervals and expired partitions.
    Plan(PartitionPlanArgs),
}

/// Arguments for `ignatius partition status`.
#[derive(Debug, Args, Clone)]
pub struct PartitionStatusArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Filter to one schema.
    #[arg(short = 's', long, value_name = "SCHEMA")]
    pub schema: Option<String>,
    /// Output format: table or json.
    #[arg(short = 'f', long, value_enum, default_value_t = CliActivityFormat::Table)]
    pub format: CliActivityFormat,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Arguments for `ignatius partition plan`.
#[derive(Debug, Args, Clone)]
pub struct PartitionPlanArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Partitioned table as schema.table, with quoting when needed.
    #[arg(long, value_name = "SCHEMA.TABLE")]
    pub table: String,
    /// Interval length.
    #[arg(long, value_enum)]
    pub period: CliPartitionPeriod,
    /// First interval start; must align to the period.
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub start: String,
    /// Number of intervals to generate, from 1 to 120.
    #[arg(long, value_name = "N", default_value_t = 3)]
    pub count: usize,
    /// Append detach and drop statements for partitions ending on or before this date.
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub expire_before: Option<String>,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Interval length for `ignatius partition plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliPartitionPeriod {
    /// One day per interval.
    Daily,
    /// One ISO week per interval, starting Monday.
    Weekly,
    /// One calendar month per interval.
    Monthly,
    /// One calendar year per interval.
    Yearly,
}

impl CliPartitionPeriod {
    /// Maps the CLI period to the domain period.
    #[must_use]
    pub const fn into_domain(self) -> crate::partition::PlanPeriod {
        match self {
            Self::Daily => crate::partition::PlanPeriod::Daily,
            Self::Weekly => crate::partition::PlanPeriod::Weekly,
            Self::Monthly => crate::partition::PlanPeriod::Monthly,
            Self::Yearly => crate::partition::PlanPeriod::Yearly,
        }
    }
}

fn run_partition(
    args: &PartitionArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    match &args.action {
        PartitionAction::Status(status) => run_partition_status(status, paths, out),
        PartitionAction::Plan(plan) => run_partition_plan(plan, paths, out, err),
    }
}

fn run_partition_status(
    args: &PartitionStatusArgs,
    paths: &Paths,
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(resolved, &loaded.config)?;

    let snapshot = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let result = session.partition_snapshot(args.schema.as_deref()).await;
        drop(session);
        result
    })?;

    let rendered = match args.format {
        CliActivityFormat::Json => snapshot.render_json(),
        CliActivityFormat::Table => snapshot.render_table(),
    };
    out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
    Ok(ExitCode::Success)
}

fn run_partition_plan(
    args: &PartitionPlanArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let (schema, table) = parse_qualified_name(&args.table)?;
    let start = parse_plan_date(&args.start, "--start")?;
    let expire_before = args
        .expire_before
        .as_deref()
        .map(|value| parse_plan_date(value, "--expire-before"))
        .transpose()?;

    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(args.target.as_deref(), &args.connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(resolved, &loaded.config)?;

    let snapshot = runtime()?.block_on(async {
        let session = crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value?;
        let result = session.partition_snapshot(Some(&schema)).await;
        drop(session);
        result
    })?;
    let parent = snapshot.find(&schema, &table).ok_or_else(|| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("no partitioned table matches {schema}.{table}"),
            "planning partition changes",
        )
        .likely_cause("the table is not partitioned, is outside the schema, or the name is quoted incorrectly")
        .next_action("run `ignatius partition status` for this schema and use its qualified name")
    })?;

    let plan = crate::partition::plan_partitions(
        parent,
        args.period.into_domain(),
        start,
        args.count,
        expire_before,
    )
    .map_err(partition_plan_diagnostic)?;

    let rendered = plan.render_sql();
    out.write_all(rendered.as_bytes()).map_err(io_diagnostic)?;
    writeln!(
        err,
        "Plan: {} create, {} expire statement(s); nothing was executed.",
        plan.creates.len(),
        plan.expires.len()
    )
    .map_err(io_diagnostic)?;
    Ok(ExitCode::Success)
}

fn partition_plan_diagnostic(error: crate::partition::PlanError) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        error.to_string(),
        "planning partition changes",
    )
    .likely_cause("the table shape or an existing partition does not match the requested plan")
    .next_action("choose a supported table and interval, or create the partition through a reviewed migration")
}

fn parse_plan_date(value: &str, flag: &str) -> Result<chrono::NaiveDate, Diagnostic> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("{flag} must be a date in YYYY-MM-DD form: {value}"),
            "planning partition changes",
        )
        .next_action("pass a valid calendar date, for example 2026-12-01")
    })
}

/// Splits `schema.table`, accepting double-quoted parts with `""` escapes.
fn parse_qualified_name(raw: &str) -> Result<(String, String), Diagnostic> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut characters = raw.chars().peekable();
    while let Some(character) = characters.next() {
        if quoted {
            match character {
                '"' if characters.peek() == Some(&'"') => {
                    current.push('"');
                    characters.next();
                }
                '"' => quoted = false,
                other => current.push(other),
            }
        } else if character == '"' {
            quoted = true;
        } else if character == '.' {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(character);
        }
    }
    if quoted {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("unterminated quoted identifier in {raw}"),
            "planning partition changes",
        )
        .next_action("close the quoted identifier"));
    }
    parts.push(current);
    if parts.len() != 2 || parts.iter().any(String::is_empty) {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("--table must be schema.table: {raw}"),
            "planning partition changes",
        )
        .next_action("qualify the table, for example billing.invoices"));
    }
    let table = parts.pop().unwrap_or_default();
    let schema = parts.pop().unwrap_or_default();
    Ok((schema, table))
}

/// Arguments for the migration runner (`045-migration-runner-and-ledger`).
#[derive(Debug, Args, Clone)]
pub struct MigrateArgs {
    #[command(subcommand)]
    pub action: MigrateAction,
}

/// Migration subcommands.
#[derive(Debug, Subcommand, Clone)]
pub enum MigrateAction {
    /// Show applied, pending, drifted and missing migrations.
    Status {
        #[command(flatten)]
        target: MigrateTargetArgs,
        /// Output format: table or json.
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: CliMigrateApplyFormat,
    },
    /// Show the pending order, optionally capped at a version.
    Plan {
        #[command(flatten)]
        target: MigrateTargetArgs,
        /// Stop after this version (inclusive).
        #[arg(long, value_name = "VERSION")]
        to: Option<u64>,
        /// Output format: table, json or sql.
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: CliMigratePlanFormat,
    },
    /// Apply pending migrations transactionally.
    Apply {
        #[command(flatten)]
        target: MigrateTargetArgs,
        /// Stop after this version (inclusive).
        #[arg(long, value_name = "VERSION")]
        to: Option<u64>,
        /// Print the plan without writing.
        #[arg(long)]
        dry_run: bool,
        /// Allow writes when the target is classified as production.
        #[arg(long)]
        allow_production: bool,
        /// Output format: table or json.
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: CliMigrateApplyFormat,
    },
    /// Check the ledger against the files without writing.
    Verify(MigrateTargetArgs),
}

/// Target, directory and connection options shared by migration commands.
#[derive(Debug, Args, Clone)]
pub struct MigrateTargetArgs {
    /// Connection target: a URI, a keyword string, @profile, or a database name.
    pub target: Option<String>,
    /// Directory containing `<version>_<name>.sql` migrations.
    #[arg(long, value_name = "DIR", default_value = "migrations")]
    pub dir: PathBuf,
    #[command(flatten)]
    pub connection: ConnectionOptions,
}

/// Output format for `migrate plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliMigratePlanFormat {
    Table,
    Json,
    Sql,
}

/// Output format for `migrate apply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliMigrateApplyFormat {
    Table,
    Json,
}

fn migrate_connect(
    target: &MigrateTargetArgs,
    paths: &Paths,
) -> Result<(tokio::runtime::Runtime, crate::postgres::Session), Diagnostic> {
    let loaded = config::load(paths)?;
    let (requested, conn_args) =
        resolve_target_and_profile(target.target.as_deref(), &target.connection, &loaded.config)?;
    let resolved = crate::connection::resolve(
        requested,
        &conn_args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    let target = authenticate_target(resolved, &loaded.config)?;
    let runtime = runtime()?;
    let session = runtime.block_on(async {
        crate::postgres::session::connect_reported(
            &target,
            std::time::Duration::from_secs(loaded.config.connection.connect_timeout_seconds),
        )
        .await
        .value
    })?;
    Ok((runtime, session))
}

fn migrate_ledger_mismatch(versions: &[&str]) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        format!(
            "migration ledger does not match the files: {}",
            versions.join(", ")
        ),
        "planning migrations",
    )
    .likely_cause("an applied migration was edited, or its file was removed")
    .next_action("restore the exact applied bytes before applying anything")
}

fn run_migrate(
    args: &MigrateArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    match &args.action {
        MigrateAction::Status { target, format } => {
            let (runtime, session) = migrate_connect(target, paths)?;
            let files = crate::migrate::discover(&target.dir)?;
            let records = runtime.block_on(session.migration_records())?;
            let plan = crate::migrate::MigrationPlan::new(files, records)?;
            match format {
                CliMigrateApplyFormat::Table => {
                    write!(out, "{}", crate::migrate::render_status_table(&plan))
                        .map_err(io_diagnostic)?;
                }
                CliMigrateApplyFormat::Json => {
                    writeln!(
                        out,
                        "{}",
                        crate::migrate::render_status_json(&plan).map_err(|error| {
                            Diagnostic::new(
                                DiagnosticKind::Internal,
                                format!("failed to serialize migration status: {error}"),
                                "formatting migration status",
                            )
                        })?
                    )
                    .map_err(io_diagnostic)?;
                }
            }
        }
        MigrateAction::Plan { target, to, format } => {
            let (runtime, session) = migrate_connect(target, paths)?;
            let files = crate::migrate::discover(&target.dir)?;
            let records = runtime.block_on(session.migration_records())?;
            let plan = crate::migrate::MigrationPlan::new(files, records)?;
            match format {
                CliMigratePlanFormat::Table => {
                    write!(out, "{}", crate::migrate::render_plan_table(&plan, *to))
                        .map_err(io_diagnostic)?
                }
                CliMigratePlanFormat::Json => {
                    writeln!(
                        out,
                        "{}",
                        crate::migrate::render_status_json(&plan).map_err(|error| {
                            Diagnostic::new(
                                DiagnosticKind::Internal,
                                format!("failed to serialize migration plan: {error}"),
                                "formatting the migration plan",
                            )
                        })?
                    )
                    .map_err(io_diagnostic)?;
                }
                CliMigratePlanFormat::Sql => {
                    write!(out, "{}", crate::migrate::render_plan_sql(&plan, *to))
                        .map_err(io_diagnostic)?;
                }
            }
        }
        MigrateAction::Apply {
            target,
            to,
            dry_run,
            allow_production,
            format,
        } => {
            let (runtime, session) = migrate_connect(target, paths)?;
            let files = crate::migrate::discover(&target.dir)?;
            if *dry_run {
                let records = runtime.block_on(session.migration_records())?;
                let plan = crate::migrate::MigrationPlan::new(files, records)?;
                // A dry run bypasses the production write guard, not the
                // ledger check: printing a plan that apply would refuse is a
                // false assurance.
                if !plan.is_clean() {
                    let versions: Vec<&str> = plan
                        .drifted()
                        .iter()
                        .chain(plan.missing().iter())
                        .map(|entry| entry.raw_version())
                        .collect();
                    return Err(migrate_ledger_mismatch(&versions));
                }
                write!(out, "{}", crate::migrate::render_plan_sql(&plan, *to))
                    .map_err(io_diagnostic)?;
                writeln!(err, "Dry run: nothing was applied.").map_err(io_diagnostic)?;
                return Ok(ExitCode::Success);
            }
            let report =
                runtime.block_on(session.apply_migrations(files, *to, *allow_production))?;
            match format {
                CliMigrateApplyFormat::Table => {
                    if report.applied.is_empty() {
                        writeln!(out, "No pending migrations were applied.")
                            .map_err(io_diagnostic)?;
                    } else {
                        writeln!(out, "APPLIED  VERSION  NAME  MS").map_err(io_diagnostic)?;
                        for applied in &report.applied {
                            writeln!(
                                out,
                                "{:<7} {:<8} {:<20} {}",
                                "yes", applied.version, applied.name, applied.execution_ms
                            )
                            .map_err(io_diagnostic)?;
                        }
                    }
                    if report.skipped > 0 {
                        writeln!(
                            err,
                            "{} pending migration(s) after --to were skipped.",
                            report.skipped
                        )
                        .map_err(io_diagnostic)?;
                    }
                }
                CliMigrateApplyFormat::Json => {
                    writeln!(
                        out,
                        "{}",
                        serde_json::to_string_pretty(&report).map_err(|error| {
                            Diagnostic::new(
                                DiagnosticKind::Internal,
                                format!("failed to serialize migration report: {error}"),
                                "formatting the migration report",
                            )
                        })?
                    )
                    .map_err(io_diagnostic)?;
                }
            }
        }
        MigrateAction::Verify(target) => {
            let (runtime, session) = migrate_connect(target, paths)?;
            let files = crate::migrate::discover(&target.dir)?;
            let records = runtime.block_on(session.migration_records())?;
            let plan = crate::migrate::MigrationPlan::new(files, records)?;
            if plan.is_clean() {
                writeln!(
                    out,
                    "Migration ledger matches {} file(s).",
                    plan.entries.len()
                )
                .map_err(io_diagnostic)?;
                return Ok(ExitCode::Success);
            }
            for entry in plan.drifted() {
                writeln!(err, "drifted: {} {}", entry.raw_version(), entry.name())
                    .map_err(io_diagnostic)?;
            }
            for entry in plan.missing() {
                writeln!(err, "missing: {} {}", entry.raw_version(), entry.name())
                    .map_err(io_diagnostic)?;
            }
            return Ok(ExitCode::Findings);
        }
    }
    Ok(ExitCode::Success)
}

/// Arguments for the offline SQL linter (`044-sql-linter-and-safety-analyzer`).
#[derive(Debug, Args, Clone)]
pub struct LintArgs {
    /// Files or directories to lint. Use `-` for standard input.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
    /// Output format: table, json or sarif.
    #[arg(short = 'f', long, value_enum, default_value = "table")]
    pub format: CliLintFormat,
    /// Hide findings below this severity.
    #[arg(long = "min-severity", value_enum, default_value = "info")]
    pub min_severity: CliLintSeverity,
    /// Exit 1 when any finding reaches this severity.
    #[arg(long = "fail-on", value_enum, default_value = "error")]
    pub fail_on: CliLintFailOn,
    /// Write the report to a file instead of standard output.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,
    /// Replace the output file if it already exists.
    #[arg(long)]
    pub force: bool,
}

/// Output format for `ignatius lint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliLintFormat {
    Table,
    Json,
    Sarif,
}

/// Minimum displayed severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliLintSeverity {
    Info,
    Warning,
    Error,
}

impl From<CliLintSeverity> for crate::lint::LintSeverity {
    fn from(value: CliLintSeverity) -> Self {
        match value {
            CliLintSeverity::Info => Self::Info,
            CliLintSeverity::Warning => Self::Warning,
            CliLintSeverity::Error => Self::Error,
        }
    }
}

/// When `ignatius lint` should exit 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliLintFailOn {
    Never,
    Info,
    Warning,
    Error,
}

impl CliLintFailOn {
    const fn threshold(self) -> Option<crate::lint::LintSeverity> {
        match self {
            Self::Never => None,
            Self::Info => Some(crate::lint::LintSeverity::Info),
            Self::Warning => Some(crate::lint::LintSeverity::Warning),
            Self::Error => Some(crate::lint::LintSeverity::Error),
        }
    }
}

fn run_lint(args: &LintArgs, out: &mut impl Write) -> Result<ExitCode, Diagnostic> {
    if args.paths.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "no SQL path was provided",
            "linting SQL",
        )
        .next_action("pass a file, a directory, or `-` for standard input"));
    }

    let mut report = crate::lint::LintReport::default();
    let mut stdin_seen = false;
    let mut files = Vec::new();
    for path in &args.paths {
        if path.as_os_str() == "-" {
            if stdin_seen {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "standard input was requested more than once",
                    "linting SQL",
                ));
            }
            stdin_seen = true;
            use std::io::Read as _;
            let mut sql = String::new();
            std::io::stdin()
                .take(crate::lint::MAX_FILE_BYTES as u64 + 1)
                .read_to_string(&mut sql)
                .map_err(io_diagnostic)?;
            if sql.len() > crate::lint::MAX_FILE_BYTES {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "standard input is over the 1 MiB lint bound",
                    "linting SQL",
                ));
            }
            report.extend(crate::lint::lint_source(None, &sql));
        } else {
            files.push(path.clone());
        }
    }
    if !files.is_empty() {
        report.extend(crate::lint::lint_paths(&files)?);
    }

    let failed = args
        .fail_on
        .threshold()
        .is_some_and(|threshold| report.meets(threshold));
    let display_threshold = crate::lint::LintSeverity::from(args.min_severity);
    report.retain_at_or_above(display_threshold);
    let rendered = match args.format {
        CliLintFormat::Table => crate::lint::render_table(&report),
        CliLintFormat::Json => crate::lint::render_json(&report).map_err(|error| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                format!("failed to serialize lint report: {error}"),
                "formatting lint findings",
            )
        })?,
        CliLintFormat::Sarif => crate::lint::render_sarif(&report).map_err(|error| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                format!("failed to serialize SARIF report: {error}"),
                "formatting lint findings",
            )
        })?,
    };
    if let Some(destination) = args.output.as_ref() {
        if destination.exists() && !args.force {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "refusing to overwrite existing file {}",
                    destination.display()
                ),
                "writing the lint report",
            )
            .likely_cause("the output file already exists")
            .next_action("pass --force to replace it"));
        }
        write_report_atomic(destination, &rendered)?;
    } else {
        write!(out, "{rendered}").map_err(io_diagnostic)?;
    }

    if failed {
        return Ok(ExitCode::Findings);
    }
    Ok(ExitCode::Success)
}

/// Arguments for the crash recovery command (`043-session-state-and-crash-recovery`).
#[derive(Debug, Args, Clone)]
pub struct RecoveryArgs {
    #[command(subcommand)]
    pub action: RecoveryAction,
}

/// Actions available for the recovery command.
#[derive(Debug, Subcommand, Clone)]
pub enum RecoveryAction {
    /// Report whether an unrecovered session journal exists.
    Status {
        /// Output format: table or json.
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: CliRecoveryStatusFormat,
    },
    /// Print the recovered buffers without deleting the journal.
    Restore {
        /// Output format: sql or json.
        #[arg(short = 'f', long, value_enum, default_value = "sql")]
        format: CliRecoveryRestoreFormat,
    },
    /// Delete the recovery journal.
    Discard,
}

/// Output representation for `recovery status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliRecoveryStatusFormat {
    Table,
    Json,
}

/// Output representation for `recovery restore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum CliRecoveryRestoreFormat {
    Sql,
    Json,
}

#[derive(serde::Serialize)]
struct RecoveryStatusReport<'a> {
    present: bool,
    clean_exit: bool,
    session_id: &'a str,
    started_at: &'a str,
    saved_at: &'a str,
    recoverable_buffers: usize,
    unsaved_buffers: usize,
    truncated_buffers: usize,
}

fn run_recovery(
    args: &RecoveryArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let recovery_dir = paths.data_dir.join(crate::recovery::RECOVERY_DIR_NAME);

    match &args.action {
        RecoveryAction::Status { format } => {
            let Some(journal) = crate::recovery::RecoveryJournal::load_from_disk(&recovery_dir)?
            else {
                match format {
                    CliRecoveryStatusFormat::Json => {
                        writeln!(out, r#"{{"present":false}}"#).map_err(io_diagnostic)?;
                    }
                    CliRecoveryStatusFormat::Table => {
                        writeln!(out, "No session recovery journal.").map_err(io_diagnostic)?;
                    }
                }
                return Ok(ExitCode::Success);
            };

            let report = RecoveryStatusReport {
                present: true,
                clean_exit: journal.clean_exit,
                session_id: &journal.session_id,
                started_at: &journal.started_at,
                saved_at: &journal.saved_at,
                recoverable_buffers: journal.recoverable_buffer_count(),
                unsaved_buffers: journal.unsaved_buffer_count(),
                truncated_buffers: journal.truncated_buffer_count(),
            };

            match format {
                CliRecoveryStatusFormat::Json => {
                    let json = serde_json::to_string_pretty(&report).map_err(|e| {
                        Diagnostic::new(
                            DiagnosticKind::Usage,
                            format!("failed to serialize recovery status: {e}"),
                            "formatting recovery status",
                        )
                    })?;
                    writeln!(out, "{json}").map_err(io_diagnostic)?;
                }
                CliRecoveryStatusFormat::Table => {
                    if journal.clean_exit {
                        writeln!(out, "Last session exited cleanly; nothing to recover.")
                            .map_err(io_diagnostic)?;
                    } else if journal.has_recoverable_content() {
                        writeln!(
                            out,
                            "Unrecovered session from {} ({}).",
                            journal.started_at,
                            journal.saved_when()
                        )
                        .map_err(io_diagnostic)?;
                        writeln!(
                            out,
                            "{} buffer(s), {} unsaved, {} truncated.",
                            journal.recoverable_buffer_count(),
                            journal.unsaved_buffer_count(),
                            journal.truncated_buffer_count()
                        )
                        .map_err(io_diagnostic)?;
                    } else {
                        writeln!(out, "A journal exists but holds nothing worth recovering.")
                            .map_err(io_diagnostic)?;
                    }
                }
            }
        }
        RecoveryAction::Restore { format } => {
            let Some(journal) = crate::recovery::RecoveryJournal::load_from_disk(&recovery_dir)?
            else {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "no session recovery journal exists",
                    "restoring a crash session",
                )
                .likely_cause("the previous session exited cleanly or the journal was discarded")
                .next_action("run `ignatius recovery status` to confirm"));
            };
            if !journal.has_recoverable_content() {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    "the session recovery journal holds nothing worth restoring",
                    "restoring a crash session",
                )
                .likely_cause("the last session exited cleanly or the buffers were empty")
                .next_action("run `ignatius recovery discard` to remove the journal"));
            }

            match format {
                CliRecoveryRestoreFormat::Json => {
                    let json = serde_json::to_string_pretty(&journal).map_err(|e| {
                        Diagnostic::new(
                            DiagnosticKind::Usage,
                            format!("failed to serialize recovered session: {e}"),
                            "formatting recovered session",
                        )
                    })?;
                    writeln!(out, "{json}").map_err(io_diagnostic)?;
                }
                CliRecoveryRestoreFormat::Sql => {
                    for (index, buffer) in journal.buffers.iter().enumerate() {
                        if buffer.sql.trim().is_empty() {
                            continue;
                        }
                        let location = buffer
                            .file_path
                            .as_ref()
                            .map_or_else(String::new, |path| format!(" ({})", path.display()));
                        writeln!(
                            out,
                            "-- ---- buffer {}: {}{} ----",
                            index + 1,
                            buffer.title,
                            location
                        )
                        .map_err(io_diagnostic)?;
                        if buffer.truncated {
                            writeln!(
                                out,
                                "-- truncated at {} bytes; the tail was not retained",
                                crate::recovery::MAX_RECOVERED_BUFFER_BYTES
                            )
                            .map_err(io_diagnostic)?;
                        }
                        writeln!(out, "{}", buffer.sql).map_err(io_diagnostic)?;
                    }
                    writeln!(
                        err,
                        "Recovered SQL was written to standard output. The journal is still on disk;"
                    )
                    .map_err(io_diagnostic)?;
                    writeln!(
                        err,
                        "run `ignatius recovery discard` after saving what you need."
                    )
                    .map_err(io_diagnostic)?;
                }
            }
        }
        RecoveryAction::Discard => {
            let existed = recovery_dir
                .join(crate::recovery::RECOVERY_FILE_NAME)
                .exists();
            crate::recovery::RecoveryJournal::discard(&recovery_dir)?;
            if existed {
                writeln!(out, "Discarded the session recovery journal.").map_err(io_diagnostic)?;
            } else {
                writeln!(out, "No session recovery journal to discard.").map_err(io_diagnostic)?;
            }
        }
    }

    Ok(ExitCode::Success)
}

#[cfg(test)]
mod tests;
