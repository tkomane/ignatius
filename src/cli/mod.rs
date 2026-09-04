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
        /// The rows never pass through memory, so the size of the result does
        /// not matter. An interrupted export leaves a `.partial` file and says
        /// how many rows reached it.
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
    /// The tier resolved from flags and the environment alone.
    pub glyphs: crate::ui::GlyphTier,
    /// A tier the user asked for explicitly, which overrides configuration.
    pub glyph_override: Option<crate::ui::GlyphTier>,
    /// Whether the environment looks able to render Unicode.
    pub unicode_capable: bool,
    /// Whether to show technical detail in errors.
    pub verbose: bool,
}

impl Presentation {
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
            doctor_command(*json, target.as_deref(), &paths, &facts, out)
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
            let _ = write!(err, "{}", diagnostic.render_plain(presentation.verbose));
            diagnostic.exit_code()
        }
    }
}

/// Decides colour and glyphs from flags, configuration and the environment.
fn resolve_presentation(global: &GlobalArgs, facts: &doctor::TerminalFacts) -> Presentation {
    let color_mode = if global.plain {
        crate::config::ColorMode::Never
    } else {
        match global.color {
            Some(ColorChoice::Always) => crate::config::ColorMode::Always,
            Some(ColorChoice::Never) => crate::config::ColorMode::Never,
            Some(ColorChoice::Auto) | None => crate::config::ColorMode::Auto,
        }
    };
    // Plain and --ascii are absolute: they are what a user reaches for when the
    // terminal cannot cope, so configuration does not get to override them.
    let forced_ascii = global.plain || global.ascii;
    let unicode_capable = !forced_ascii && facts.unicode;
    let glyph_override = if forced_ascii {
        Some(crate::ui::GlyphTier::Ascii)
    } else {
        global.glyphs.map(|choice| choice.tier(unicode_capable))
    };

    Presentation {
        color: crate::ui::terminal::should_use_color(color_mode, facts),
        glyphs: glyph_override.unwrap_or(if unicode_capable {
            crate::ui::GlyphTier::Unicode
        } else {
            crate::ui::GlyphTier::Ascii
        }),
        glyph_override,
        unicode_capable,
        verbose: global.verbose,
    }
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
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths);
    let mut report = doctor::environment_report(paths, &loaded, facts);

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
        let config = loaded
            .as_ref()
            .map(|l| l.config.clone())
            .unwrap_or_default();
        let checks = runtime()?.block_on(interactive::probe(target, &config))?;
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

    // An export streams rows straight to a file and never holds the result.
    if let Some(destination) = request.output {
        let writer = output::StreamWriter::new(request.options.clone())?;
        let export = crate::query::export::Export::create(destination, request.force)?;
        let outcome = runtime()?.block_on(interactive::export_once_with_parameters(
            target,
            &loaded.config,
            sql,
            parameters,
            export,
            writer,
        ))?;

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

    let execution = runtime()?.block_on(interactive::execute_once_with_parameters(
        target,
        &loaded.config,
        sql,
        parameters,
        row_cap,
    ))?;

    if let Err(err) = output::write_execution(out, &execution, &request.options) {
        return match err.kind() {
            // Piping into `head` closes the stream early. That is the pipeline
            // working, not a failure, and it exits zero like every other tool.
            std::io::ErrorKind::BrokenPipe => Ok(ExitCode::Success),
            // The format refused to represent the result unambiguously.
            std::io::ErrorKind::InvalidInput => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                err.to_string(),
                "writing results",
            )
            .next_action("run a single statement, or choose a format that carries result sets")),
            _ => Err(io_diagnostic(err)),
        };
    }

    if let Some(error) = &execution.error {
        if request.options.format == Format::Json {
            // JSON diagnostics are written to stderr because stdout remains a
            // data stream. The failed execution intentionally wrote no JSON
            // result document, so a parser cannot mistake it for success.
            writeln!(err, "{}", error.to_json()).map_err(io_diagnostic)?;
            return Ok(error.exit_code());
        }
        return Err(error.clone());
    }
    Ok(ExitCode::Success)
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
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn the_command_tree_parses_and_has_no_conflicting_definitions() {
        Cli::command().debug_assert();
    }

    #[test]
    fn commands_parse_the_way_the_documentation_says() {
        let cli = Cli::try_parse_from(["ignatius", "query", "-c", "SELECT 1", "--format", "csv"])
            .expect("parse");
        let Some(Command::Query {
            command, format, ..
        }) = cli.command
        else {
            panic!("expected query");
        };
        assert_eq!(command.as_deref(), Some("SELECT 1"));
        assert_eq!(format, Format::Csv);

        let cli = Cli::try_parse_from(["ignatius"]).expect("parse");
        assert!(
            cli.command.is_none(),
            "no subcommand opens the interactive client"
        );

        let cli = Cli::try_parse_from(["ignatius", "doctor", "--json"]).expect("parse");
        assert!(matches!(
            cli.command,
            Some(Command::Doctor { json: true, .. })
        ));
    }

    #[test]
    fn query_parses_repeatable_parameter_environment_mappings() {
        let cli = Cli::try_parse_from([
            "ignatius",
            "query",
            "-c",
            "SELECT :customer_id, :status",
            "--param-env",
            "customer_id=IGNATIUS_CUSTOMER_ID",
            "--param-env",
            "status=IGNATIUS_STATUS",
        ])
        .expect("parse");
        let Some(Command::Query { parameter_env, .. }) = cli.command else {
            panic!("expected query");
        };
        assert_eq!(
            parameter_env,
            vec![
                "customer_id=IGNATIUS_CUSTOMER_ID".to_owned(),
                "status=IGNATIUS_STATUS".to_owned()
            ]
        );
    }

    #[test]
    fn parameter_environment_mappings_are_validated_without_target_work() {
        let missing = parameter_bindings_from_env("SELECT :customer_id", &[])
            .expect_err("prompting is not allowed here");
        assert_eq!(missing.kind, DiagnosticKind::Usage);
        assert!(
            missing
                .next_action
                .as_deref()
                .is_some_and(|action| action.contains(":customer_id"))
        );

        let duplicate = parameter_bindings_from_env(
            "SELECT :customer_id",
            &["customer_id=PATH".to_owned(), "customer_id=PATH".to_owned()],
        )
        .expect_err("duplicate mapping");
        assert!(duplicate.headline.contains("mapped more than once"));

        let extra = parameter_bindings_from_env("SELECT :customer_id", &["other=PATH".to_owned()])
            .expect_err("extra mapping");
        assert!(extra.headline.contains("unknown parameter"));

        let malformed = parameter_bindings_from_env(
            "SELECT :customer_id",
            &["customer_id=PATH=OTHER".to_owned()],
        )
        .expect_err("malformed mapping");
        assert!(malformed.headline.contains("NAME=VARIABLE"));

        let no_parameters =
            parameter_bindings_from_env("SELECT 1", &["customer_id=PATH".to_owned()])
                .expect_err("mapping without a placeholder");
        assert!(no_parameters.headline.contains("no named parameters"));
    }

    #[test]
    fn a_complete_parameter_environment_mapping_keeps_values_secret() {
        if std::env::var_os("PATH").is_none() {
            return;
        }
        let bindings = parameter_bindings_from_env("SELECT :value", &["value=PATH".to_owned()])
            .expect("PATH is available")
            .expect("one binding");
        assert_eq!(bindings.names(), vec!["value"]);
        let debug = format!("{bindings:?}");
        assert!(!debug.contains("/"), "environment values stay out of debug");
    }

    #[test]
    fn command_and_file_cannot_both_be_given() {
        let err = Cli::try_parse_from(["ignatius", "query", "-c", "SELECT 1", "-f", "x.sql"])
            .expect_err("must conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn an_unknown_sslmode_is_a_usage_error_with_guidance() {
        let options = ConnectionOptions {
            sslmode: Some("sort-of".into()),
            ..ConnectionOptions::default()
        };
        let err = options.to_args().expect_err("must reject");
        assert_eq!(err.kind, DiagnosticKind::Usage);
        assert_eq!(err.exit_code(), ExitCode::Usage);
        assert!(err.next_action.is_some());
    }

    #[test]
    fn environment_is_parsed_from_the_flag_only() {
        let options = ConnectionOptions {
            environment: Some("production".into()),
            ..ConnectionOptions::default()
        };
        let args = options.to_args().expect("parse");
        assert_eq!(args.environment, Some(Environment::Production));
        assert!(
            ConnectionOptions::default()
                .to_args()
                .expect("parse")
                .environment
                .is_none()
        );
    }

    #[test]
    fn implicit_interactive_start_is_kept_only_when_no_route_option_was_typed() {
        assert!(!ConnectionOptions::default().has_explicit_route());

        for options in [
            ConnectionOptions {
                host: Some("db.example.net".into()),
                ..ConnectionOptions::default()
            },
            ConnectionOptions {
                dbname: Some("orders".into()),
                ..ConnectionOptions::default()
            },
            ConnectionOptions {
                sslmode: Some("verify-full".into()),
                ..ConnectionOptions::default()
            },
            ConnectionOptions {
                read_only: true,
                ..ConnectionOptions::default()
            },
            ConnectionOptions {
                profile: Some("orders-prod".into()),
                ..ConnectionOptions::default()
            },
        ] {
            assert!(
                options.has_explicit_route(),
                "a typed connection or safety option must bypass the picker: {options:?}"
            );
        }
    }

    #[test]
    fn the_startup_picker_requires_profiles_and_an_implicit_route() {
        let defaults = ConnectionOptions::default();
        assert!(should_open_connection_picker(None, &defaults, true));
        assert!(!should_open_connection_picker(None, &defaults, false));
        assert!(!should_open_connection_picker(
            Some("@orders-dev"),
            &defaults,
            true
        ));
        assert!(!should_open_connection_picker(
            None,
            &ConnectionOptions {
                profile: Some("orders-dev".into()),
                ..ConnectionOptions::default()
            },
            true
        ));
    }

    #[test]
    fn plain_implies_no_colour_and_no_unicode() {
        let facts = doctor::TerminalFacts {
            is_terminal: true,
            term: Some("xterm-256color".into()),
            term_program: None,
            no_color: false,
            size: Some((100, 30)),
            unicode: true,
        };
        let presentation = resolve_presentation(
            &GlobalArgs {
                plain: true,
                ..GlobalArgs::default()
            },
            &facts,
        );
        assert!(!presentation.color);
        assert!(!presentation.unicode());
        assert_eq!(presentation.glyphs, crate::ui::GlyphTier::Ascii);
    }

    #[test]
    fn the_nerd_font_tier_is_only_ever_chosen_explicitly() {
        let facts = doctor::TerminalFacts {
            is_terminal: true,
            term: Some("xterm-256color".into()),
            term_program: Some("WarpTerminal".into()),
            no_color: false,
            size: Some((100, 30)),
            unicode: true,
        };

        // Nothing about a capable terminal implies a patched font.
        let automatic = resolve_presentation(&GlobalArgs::default(), &facts);
        assert_eq!(automatic.glyphs, crate::ui::GlyphTier::Unicode);
        assert!(
            automatic.glyph_override.is_none(),
            "configuration still decides"
        );

        let asked = resolve_presentation(
            &GlobalArgs {
                glyphs: Some(GlyphChoice::NerdFont),
                ..GlobalArgs::default()
            },
            &facts,
        );
        assert_eq!(asked.glyphs, crate::ui::GlyphTier::Nerd);

        // --plain wins over an explicit request, because it is what someone
        // reaches for when the terminal cannot cope.
        let plain = resolve_presentation(
            &GlobalArgs {
                glyphs: Some(GlyphChoice::NerdFont),
                plain: true,
                ..GlobalArgs::default()
            },
            &facts,
        );
        assert_eq!(plain.glyphs, crate::ui::GlyphTier::Ascii);
    }

    #[test]
    fn version_output_separates_the_four_identity_facts() {
        let mut out = Vec::new();
        version(true, &mut out).expect("write");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains(branding::VERSION));
        assert!(text.contains("source revision:"));
        assert!(text.contains("build identity:"));
        assert!(text.contains("target:"));
    }

    #[test]
    fn config_paths_lists_every_location() {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = Paths::rooted_at(dir.path());
        let mut out = Vec::new();
        let code = config_command(&ConfigAction::Paths, &paths, &mut out).expect("run");
        assert_eq!(code, ExitCode::Success);
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("Configuration file:"));
        assert!(text.contains("Logs:"));
    }

    #[test]
    fn config_validate_reports_success_on_a_fresh_machine() {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = Paths::rooted_at(dir.path());
        let mut out = Vec::new();
        let code = config_command(&ConfigAction::Validate, &paths, &mut out).expect("run");
        assert_eq!(code, ExitCode::Success);
        assert!(String::from_utf8(out).expect("utf8").contains("valid"));
    }

    fn with_profiles(toml: &str) -> crate::config::Config {
        toml::from_str(toml).expect("a valid configuration")
    }

    fn options(profile: Option<&str>) -> ConnectionOptions {
        ConnectionOptions {
            profile: profile.map(str::to_owned),
            ..ConnectionOptions::default()
        }
    }

    #[test]
    fn a_profile_fills_in_what_was_not_said_and_pins_the_environment() {
        // The classification is the reason profiles exist: it cannot be
        // forgotten once it is written down, and forgetting it is what puts a
        // write on the wrong database.
        let config = with_profiles(
            "[profiles.orders-prod]\n\
             host = \"db.example.net\"\n\
             port = 6432\n\
             dbname = \"orders\"\n\
             user = \"app\"\n\
             sslmode = \"verify-full\"\n\
             environment = \"production\"\n",
        );

        let (target, args) =
            resolve_target_and_profile(Some("@orders-prod"), &options(None), &config)
                .expect("resolves");
        assert!(target.is_none(), "the profile is the target");
        assert_eq!(args.host.as_deref(), Some("db.example.net"));
        assert_eq!(args.port, Some(6432));
        assert_eq!(args.dbname.as_deref(), Some("orders"));
        assert_eq!(args.username.as_deref(), Some("app"));
        assert_eq!(args.sslmode, Some(SslMode::VerifyFull));
        assert_eq!(args.environment, Some(Environment::Production));
        assert!(args.environment.expect("set").is_production());

        // The flag and the shorthand are the same thing.
        let (_, from_flag) =
            resolve_target_and_profile(None, &options(Some("orders-prod")), &config)
                .expect("resolves");
        assert_eq!(from_flag.host, args.host);
    }

    #[test]
    fn a_flag_the_user_typed_beats_the_profile_and_safety_only_adds() {
        let config = with_profiles(
            "[profiles.p]\nhost = \"from-profile\"\ndbname = \"orders\"\nread-only = true\n",
        );
        let typed = ConnectionOptions {
            host: Some("from-the-flag".to_owned()),
            ..options(Some("p"))
        };
        let (_, args) = resolve_target_and_profile(None, &typed, &config).expect("resolves");
        assert_eq!(args.host.as_deref(), Some("from-the-flag"));
        assert_eq!(
            args.dbname.as_deref(),
            Some("orders"),
            "the rest is filled in"
        );
        assert!(
            args.read_only,
            "a profile can make a session safer; nothing here can make it less safe"
        );

        // And --read-only on a profile that does not ask for it still applies.
        let config = with_profiles("[profiles.p]\nhost = \"h\"\n");
        let read_only = ConnectionOptions {
            read_only: true,
            ..options(Some("p"))
        };
        let (_, args) = resolve_target_and_profile(None, &read_only, &config).expect("resolves");
        assert!(args.read_only);
    }

    #[test]
    fn a_profile_and_a_target_together_are_refused_rather_than_merged() {
        let config = with_profiles("[profiles.p]\nhost = \"h\"\n");
        let error = resolve_target_and_profile(
            Some("postgres://elsewhere/db"),
            &options(Some("p")),
            &config,
        )
        .expect_err("must refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Usage);
        assert!(
            error
                .next_action
                .unwrap_or_default()
                .contains("one of them")
        );

        // Naming the same profile twice is not a disagreement, so it is allowed.
        let (target, args) =
            resolve_target_and_profile(Some("@p"), &options(Some("p")), &config).expect("resolves");
        assert!(target.is_none());
        assert_eq!(args.host.as_deref(), Some("h"));
    }

    #[test]
    fn an_unknown_profile_lists_the_ones_that_exist() {
        let config = with_profiles(
            "[profiles.orders-prod]\nhost = \"a\"\n\n[profiles.orders-dev]\nhost = \"b\"\n",
        );
        let error = resolve_target_and_profile(Some("@orders-stage"), &options(None), &config)
            .expect_err("must refuse");
        let cause = error.likely_cause.unwrap_or_default();
        assert!(cause.contains("orders-prod"), "{cause}");
        assert!(cause.contains("orders-dev"), "{cause}");

        let empty = crate::config::Config::default();
        let error = resolve_target_and_profile(Some("@anything"), &options(None), &empty)
            .expect_err("must refuse");
        assert!(
            error
                .likely_cause
                .unwrap_or_default()
                .contains("no profiles"),
            "an empty file says so rather than listing nothing"
        );
    }

    #[test]
    fn a_profile_that_tries_to_hold_a_password_is_refused_by_name() {
        for field in ["password", "pgpassword", "sslpassword"] {
            let config = with_profiles(&format!(
                "[profiles.p]\nhost = \"h\"\n{field} = \"hunter2\"\n"
            ));
            let error = resolve_target_and_profile(Some("@p"), &options(None), &config)
                .expect_err("refuse");
            assert_eq!(error.exit_code(), crate::ExitCode::Config);
            assert!(
                error.headline.contains("password"),
                "the refusal names what it is about: {}",
                error.headline
            );
            assert!(
                !format!("{error:?}").contains("hunter2"),
                "and never repeats the value back"
            );
            let action = error.next_action.clone().unwrap_or_default();
            assert!(action.contains(".pgpass"), "{action}");
        }

        // A password file is a path rather than a secret, and is still refused
        // here, with the reason and the route that does work.
        let config = with_profiles("[profiles.p]\npassfile = \"/tmp/x\"\n");
        let error =
            resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
        assert!(error.next_action.unwrap_or_default().contains("PGPASSFILE"));

        // Anything else unknown is refused with the fields that exist.
        let config = with_profiles("[profiles.p]\nhosst = \"h\"\n");
        let error =
            resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
        assert!(error.likely_cause.unwrap_or_default().contains("dbname"));
    }

    #[test]
    fn a_profile_with_a_value_this_build_cannot_read_says_which_one() {
        let config = with_profiles("[profiles.p]\nsslmode = \"maybe\"\n");
        let error =
            resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
        assert!(error.headline.contains("sslmode"), "{}", error.headline);
        assert!(
            error
                .next_action
                .unwrap_or_default()
                .contains("verify-full")
        );

        let config = with_profiles("[profiles.p]\nenvironment = \"\"\n");
        let error =
            resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
        assert!(error.headline.contains("environment"), "{}", error.headline);
    }

    #[test]
    fn completions_are_generated_for_every_documented_shell() {
        for shell in [
            clap_complete::Shell::Zsh,
            clap_complete::Shell::Bash,
            clap_complete::Shell::Fish,
            clap_complete::Shell::PowerShell,
        ] {
            let mut out = Vec::new();
            completion(shell, &mut out).expect("generate");
            let text = String::from_utf8(out).expect("utf8");
            assert!(
                text.contains("ignatius"),
                "{shell:?} produced nothing useful"
            );
            assert!(
                text.len() > 200,
                "{shell:?} produced a suspiciously short script"
            );
        }
    }

    #[test]
    fn query_without_sql_or_a_pipe_refuses_instead_of_waiting() {
        // Standard input is a terminal only when a person is at the keyboard;
        // under `cargo test` it is not, so this asserts the file branch instead.
        let err = read_sql(None, Some(std::path::Path::new("/nonexistent/query.sql")))
            .expect_err("must fail");
        assert_eq!(err.kind, DiagnosticKind::Usage);
        assert!(err.next_action.is_some());
    }
}
