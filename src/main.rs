//! Process entry point.
//!
//! Deliberately thin: parse, run, map the outcome to an exit code. Everything
//! else lives in the library so it can be tested without spawning a process.

use clap::Parser;
use ignatius::ExitCode;
use ignatius::cli::{Cli, run};
use std::io::Write;

fn main() -> std::process::ExitCode {
    // Logging is opt-in and writes nothing unless IGNATIUS_LOG is set.
    let paths = ignatius::config::Paths::resolve();
    let _logging = ignatius::diagnostics::logging::init(&paths.log_dir);

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            // clap prints help and version to stdout, real errors to stderr.
            let _ = err.print();
            return match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayVersion
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    ExitCode::Success.into()
                }
                _ => ExitCode::Usage.into(),
            };
        }
    };

    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();

    let code = run(&cli, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    code.into()
}
