//! Explicit local recording and offline display of query plan observations.

use super::{ConnectionOptions, connect_and_run, io_diagnostic};
use crate::ExitCode;
use crate::config::Paths;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::plan_history::store::{Catalog, Store};
use crate::plan_history::{MAX_SQL_BYTES, Timeline, digest, valid_series_name};
use crate::query::{classify, statements};
use clap::{Args, Subcommand, ValueEnum};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Arguments for the `plan-history` command.
#[derive(Debug, Args)]
pub struct PlanHistoryArgs {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
enum Action {
    /// Capture an estimate or explicitly measure one read statement.
    Capture(Box<CaptureArgs>),
    /// View a series without connecting to PostgreSQL.
    Show(ViewArgs),
    /// Clear a named series from local storage.
    Forget {
        #[arg(long, value_parser = parse_series)]
        series: String,
    },
}

#[derive(Debug, Args)]
struct CaptureArgs {
    /// Database, @profile or connection string.
    target: Option<String>,
    /// The one schema whose catalog revisions to track.
    #[arg(long)]
    schema: String,
    /// One read statement.
    #[arg(
        short = 'c',
        long,
        conflicts_with = "sql_file",
        required_unless_present = "sql_file"
    )]
    command: Option<String>,
    /// UTF-8 file containing one read statement.
    #[arg(long, value_name = "PATH")]
    sql_file: Option<PathBuf>,
    /// Run EXPLAIN ANALYZE and benchmark the query within a read-only transaction.
    #[arg(long)]
    measure: bool,
    #[command(flatten)]
    view: ViewArgs,
    #[command(flatten)]
    connection: ConnectionOptions,
}

#[derive(Debug, Args)]
struct ViewArgs {
    /// Local series name, up to 64 lowercase letters, digits, hyphens or underscores.
    #[arg(long, value_parser = parse_series)]
    series: String,
    /// Output format.
    #[arg(long, value_enum, default_value = "plain")]
    format: Format,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Plain,
    Json,
}

fn parse_series(name: &str) -> Result<String, &'static str> {
    if valid_series_name(name) {
        Ok(name.to_owned())
    } else {
        Err("expected 1 to 64 lowercase ASCII letters, digits, hyphens or underscores")
    }
}

pub(super) fn run(
    args: &PlanHistoryArgs,
    paths: &Paths,
    no_history: bool,
    out: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let store = Store::new(&paths.data_dir);
    match &args.action {
        Action::Capture(args) => {
            if no_history {
                return Err(usage(
                    "capture saves a local observation and conflicts with --no-history",
                ));
            }
            let catalog = capture(args, paths, &store)?;
            write_timeline(&args.view, &catalog, out)?;
        }
        Action::Show(view) => write_timeline(view, &store.read()?, out)?,
        Action::Forget { series } => {
            let message = if store.forget(series)? {
                "Cleared plan history for"
            } else {
                "No plan history for"
            };
            writeln!(out, "{message} {series}.").map_err(io_diagnostic)?;
        }
    }
    Ok(ExitCode::Success)
}

fn capture(args: &CaptureArgs, paths: &Paths, store: &Store) -> Result<Catalog, Diagnostic> {
    if args.schema.trim().is_empty() || args.schema.len() > 63 || args.schema.contains('\0') {
        return Err(usage("provide one PostgreSQL schema name of 1 to 63 bytes"));
    }
    let input = match (args.command.as_deref(), args.sql_file.as_deref()) {
        (Some(sql), None) => sql.to_owned(),
        (None, Some(path)) => read_sql_file(path)?,
        _ => return Err(usage("provide exactly one of --command and --sql-file")),
    };
    let sql = validate_sql(&input)?;
    let catalog = store.read()?;
    let mut connection = args.connection.clone();
    connection.read_only = true;
    let series = &args.view.series;
    let schema = &args.schema;
    let measure = args.measure;

    let (binding, observation) = connect_and_run(
        args.target.as_deref(),
        &connection,
        None,
        false,
        paths,
        |session| async move {
            let info = session.info();
            let binding = digest(&(
                &info.target,
                &info.database,
                &info.user,
                &info.search_path,
                info.tls.label(),
                schema,
                &sql,
            ))?;
            catalog.check_binding(series, &binding)?;
            let observation =
                crate::postgres::plan_history::capture(session, schema, &sql, measure).await?;
            Ok((binding, observation))
        },
    )?;
    store.append(series, binding, observation)
}

fn read_sql_file(path: &Path) -> Result<String, Diagnostic> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io_diagnostic)?
        .take(MAX_SQL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_diagnostic)?;
    if bytes.len() > MAX_SQL_BYTES {
        return Err(usage("the input exceeds the 64 KiB SQL limit"));
    }
    String::from_utf8(bytes).map_err(|_| usage("the SQL file must be UTF-8"))
}

fn validate_sql(input: &str) -> Result<String, Diagnostic> {
    if input.len() > MAX_SQL_BYTES || input.contains('\0') {
        return Err(usage(
            "plan history accepts at most 64 KiB of SQL without NUL bytes",
        ));
    }
    let mut parsed = statements::split(input);
    if parsed.len() != 1 {
        return Err(usage("provide one SELECT, WITH, TABLE or VALUES statement"));
    }
    let statement = parsed.remove(0);
    let without_comments = statements::strip_comments(&statement.text);
    let verb = without_comments
        .trim_start()
        .split(|letter: char| !(letter.is_ascii_alphanumeric() || letter == '_'))
        .next()
        .unwrap_or_default();
    if !["select", "with", "table", "values"]
        .iter()
        .any(|candidate| verb.eq_ignore_ascii_case(candidate))
        || classify::classify(&statement) != classify::Impact::Read
    {
        return Err(usage("plan history only captures read queries"));
    }
    Ok(statement.text)
}

fn write_timeline(
    view: &ViewArgs,
    catalog: &Catalog,
    out: &mut impl Write,
) -> Result<(), Diagnostic> {
    let timeline = Timeline::new(&view.series, catalog.get(&view.series));
    match view.format {
        Format::Plain => out
            .write_all(timeline.render_text().as_bytes())
            .map_err(io_diagnostic),
        Format::Json => {
            serde_json::to_writer_pretty(&mut *out, &timeline)
                .map_err(|error| io_diagnostic(std::io::Error::other(error)))?;
            writeln!(out).map_err(io_diagnostic)
        }
    }
}

fn usage(headline: &str) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        headline,
        "validating plan history capture",
    )
}
