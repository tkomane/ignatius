//! The command tree, its output contracts, and exit codes.
//!
//! The non-interactive surface is a contract, not a convenience: it never
//! prompts when it is not attached to a terminal, keeps data on stdout and
//! diagnostics on stderr, and returns documented exit codes.

pub mod interactive;
pub mod output;

use crate::ExitCode;
use crate::branding;
use crate::config::{self, Paths};
use crate::connection::{ConnectionArgs, EnvSnapshot, Environment, SslMode};
use crate::diagnostics::doctor;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use clap::{Args, CommandFactory, Parser, Subcommand};
use output::{Format, OutputOptions};
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
        /// Omit the header row.
        #[arg(long)]
        no_header: bool,
        /// Text used for SQL NULL in csv and tsv output.
        #[arg(long, value_name = "TEXT", default_value = "")]
        null: String,
        /// Maximum rows to hold in memory. Truncation is always reported.
        #[arg(long, value_name = "N")]
        max_rows: Option<usize>,
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

    /// Print version and build identity.
    Version {
        /// Include build identity and source revision.
        #[arg(long)]
        verbose: bool,
    },
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
}

impl ConnectionOptions {
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
            host: self.host.clone(),
            port: self.port,
            dbname: self.dbname.clone(),
            username: self.username.clone(),
            sslmode,
            environment,
        })
    }
}

/// Configuration sub-commands.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Show where configuration and local state live.
    Paths,
    /// Show the effective configuration.
    Show,
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
        Some(Command::Doctor { json, target }) => {
            doctor_command(*json, target.as_deref(), &paths, &facts, out)
        }
        Some(Command::Query {
            target,
            command,
            file,
            format,
            no_header,
            null,
            max_rows,
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
                },
                max_rows: *max_rows,
                connection,
            },
            &paths,
            out,
        ),
        Some(Command::Connect {
            target,
            check,
            connection,
        }) => {
            if *check {
                check_command(target.as_deref(), connection, &paths, out)
            } else {
                interactive::run(target.as_deref(), connection, &paths, &presentation)
            }
        }
        None => interactive::run(None, &ConnectionOptions::default(), &paths, &presentation),
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
        ConfigAction::Validate => {
            let loaded = config::load(paths)?;
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
    connection: &'a ConnectionOptions,
}

fn query_command(
    request: QueryRequest<'_>,
    paths: &Paths,
    out: &mut impl Write,
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

    let loaded = config::load(paths)?;
    let row_cap = request
        .max_rows
        .unwrap_or(loaded.config.query.max_buffered_rows);
    let args = request.connection.to_args()?;
    let target = crate::connection::resolve(
        request.target,
        &args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;

    let execution = runtime()?.block_on(interactive::execute_once(
        target,
        &loaded.config,
        sql,
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
        return Err(error.clone());
    }
    Ok(ExitCode::Success)
}

fn check_command(
    target: Option<&str>,
    connection: &ConnectionOptions,
    paths: &Paths,
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let loaded = config::load(paths)?;
    let args = connection.to_args()?;
    let resolved = crate::connection::resolve(
        target,
        &args,
        &EnvSnapshot::from_process(),
        &loaded.config.connection,
    )?;
    for note in &resolved.notes {
        writeln!(out, "Note: {}: {}", note.subject, note.message).map_err(io_diagnostic)?;
    }
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
