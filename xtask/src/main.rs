//! Development tasks for Ignatius.
//!
//! One portable entry point instead of a shell script and a PowerShell twin that
//! drift apart. Run through the cargo alias:
//!
//! ```text
//! cargo xtask db up        Start the disposable database and print its URI
//! cargo xtask db down      Stop it and delete its data
//! cargo xtask db status    Is it running, and is it ready
//! cargo xtask run          Open the client against the development database
//! cargo xtask sql "SQL"    Run one statement against it
//! cargo xtask test         Everything, including the integration tests
//! cargo xtask verify       Every gate, collecting failures, with a summary
//! cargo xtask install      Build a release binary and put it on PATH
//! ```
//!
//! It has no dependencies on purpose: it runs before anything else is known to
//! work, so it must not be able to fail for a reason of its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let result = match refs.as_slice() {
        [] | ["help"] | ["--help"] | ["-h"] => {
            print_help();
            Ok(())
        }
        ["db", "up"] => db_up(),
        ["db", "down"] => db_down(),
        ["db", "status"] => db_status(),
        ["run", rest @ ..] => run_client(rest),
        ["sql", statement, rest @ ..] => run_sql(statement, rest),
        ["test"] => test(),
        ["verify"] => verify(),
        ["install", rest @ ..] => install(rest),
        other => Err(format!(
            "unknown task: {}\nRun `cargo xtask help` for the list.",
            other.join(" ")
        )),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("\nxtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "Development tasks for Ignatius.

  cargo xtask db up        Start the disposable database and print its URI
  cargo xtask db down      Stop it and delete its data
  cargo xtask db status    Is it running, and is it ready
  cargo xtask run [args]   Open the client against the development database
  cargo xtask sql \"SQL\"    Run one statement against it
  cargo xtask test         Everything, including the integration tests
  cargo xtask verify       Every gate, collecting failures, with a summary
  cargo xtask install [--dir PATH]
                           Build a release binary and put it on PATH

The database credentials are synthetic and live in docker/dev.env."
    );
}

// ---------------------------------------------------------------- environment

/// The repository root, found from this crate's own location.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repository root")
        .to_path_buf()
}

fn compose_file() -> PathBuf {
    repo_root().join("docker").join("compose.yaml")
}

/// Reads the synthetic development credentials.
fn dev_env() -> Result<BTreeMap<String, String>, String> {
    let path = repo_root().join("docker").join("dev.env");
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("could not read {}: {err}", path.display()))?;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    Ok(values)
}

/// Port the development database is published on. Mirrors `docker/compose.yaml`.
const DEV_PORT: u16 = 55432;

/// The connection URI, including the synthetic password.
fn dev_uri() -> Result<String, String> {
    let env = dev_env()?;
    let get = |key: &str| -> Result<&String, String> {
        env.get(key)
            .ok_or_else(|| format!("{key} is missing from docker/dev.env"))
    };
    Ok(format!(
        "postgres://{}:{}@127.0.0.1:{DEV_PORT}/{}",
        get("POSTGRES_USER")?,
        get("POSTGRES_PASSWORD")?,
        get("POSTGRES_DB")?
    ))
}

/// The same URI without the password, for printing.
fn dev_uri_safe() -> Result<String, String> {
    let env = dev_env()?;
    Ok(format!(
        "postgres://{}@127.0.0.1:{DEV_PORT}/{}",
        env.get("POSTGRES_USER").map_or("?", String::as_str),
        env.get("POSTGRES_DB").map_or("?", String::as_str)
    ))
}

// -------------------------------------------------------------- process plumbing

/// Runs a command with inherited output, failing with a readable message.
fn run(program: &str, args: &[&str], env: &[(&str, &str)]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(repo_root())
        .status()
        .map_err(|err| format!("could not start `{program}`: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{program} {}` failed with {}",
            args.join(" "),
            status
                .code()
                .map_or_else(|| "a signal".to_owned(), |c| format!("exit code {c}"))
        ))
    }
}

/// Runs a command quietly and reports only whether it succeeded.
fn run_quiet(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .current_dir(repo_root())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

// --------------------------------------------------------------------- tasks

fn db_up() -> Result<(), String> {
    let compose = compose_file();
    let compose = compose.to_string_lossy().into_owned();
    println!("Starting the disposable database...");
    run("docker", &["compose", "-f", &compose, "up", "-d"], &[])?;

    print!("Waiting for it to accept connections");
    flush();
    for attempt in 0..60 {
        if postgres_ready() {
            println!(" ready after {attempt}s.");
            print_connection_details()?;
            return Ok(());
        }
        print!(".");
        flush();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err("the database did not become ready within 60 seconds. \
         Check `docker compose -f docker/compose.yaml logs postgres`."
        .to_owned())
}

fn postgres_ready() -> bool {
    let Ok(env) = dev_env() else { return false };
    let (Some(user), Some(db)) = (env.get("POSTGRES_USER"), env.get("POSTGRES_DB")) else {
        return false;
    };
    let compose = compose_file();
    // Checked over TCP, not the socket. While the entrypoint is still loading
    // the fixtures it runs a temporary server with `listen_addresses` empty, so
    // a socket check would report ready before the tables exist.
    run_quiet(
        "docker",
        &[
            "compose",
            "-f",
            &compose.to_string_lossy(),
            "exec",
            "-T",
            "postgres",
            "pg_isready",
            "-h",
            "127.0.0.1",
            "-U",
            user,
            "-d",
            db,
        ],
    )
}

fn print_connection_details() -> Result<(), String> {
    println!(
        "
The database is ready.

  Connection   {}
  Password     synthetic, in docker/dev.env

Try it:

  cargo xtask run                       open the client
  cargo xtask sql \"SELECT * FROM orders\"  run one statement
  cargo xtask test                      run everything, including integration tests

Stop it with `cargo xtask db down`, which also deletes its data.",
        dev_uri_safe()?
    );
    Ok(())
}

fn db_down() -> Result<(), String> {
    let compose = compose_file();
    run(
        "docker",
        &["compose", "-f", &compose.to_string_lossy(), "down", "-v"],
        &[],
    )?;
    println!("Stopped. Its data is gone; `cargo xtask db up` recreates it from the fixtures.");
    Ok(())
}

fn db_status() -> Result<(), String> {
    let compose = compose_file();
    let running = run_quiet(
        "docker",
        &["compose", "-f", &compose.to_string_lossy(), "ps", "--quiet"],
    );
    if !running {
        println!("Not running. Start it with `cargo xtask db up`.");
        return Ok(());
    }
    if postgres_ready() {
        println!("Running and accepting connections on port {DEV_PORT}.");
        println!("  {}", dev_uri_safe()?);
    } else {
        println!("The container is up but the server is not accepting connections yet.");
    }
    Ok(())
}

/// Opens the client against the development database.
fn run_client(extra: &[&str]) -> Result<(), String> {
    require_database()?;
    let uri = dev_uri()?;
    // The URI carries the synthetic password, so it goes in the argument list of
    // a child process rather than into the user's shell history.
    let mut args = vec![
        "run",
        "--quiet",
        "--package",
        "ignatius",
        "--",
        "connect",
        &uri,
    ];
    args.extend_from_slice(extra);
    run("cargo", &args, &[])
}

/// Runs one statement against the development database.
fn run_sql(statement: &str, extra: &[&str]) -> Result<(), String> {
    require_database()?;
    let uri = dev_uri()?;
    let mut args = vec![
        "run",
        "--quiet",
        "--package",
        "ignatius",
        "--",
        "query",
        &uri,
        "-c",
        statement,
    ];
    args.extend_from_slice(extra);
    run("cargo", &args, &[])
}

fn require_database() -> Result<(), String> {
    if postgres_ready() {
        return Ok(());
    }
    Err("the development database is not running. Start it with `cargo xtask db up`.".to_owned())
}

/// Runs the whole test suite, including the tests that need a real server.
fn test() -> Result<(), String> {
    let uri = dev_uri()?;
    if postgres_ready() {
        println!("Running everything, including the integration tests.\n");
        run("cargo", &["test"], &[("IGNATIUS_TEST_PG_URI", &uri)])
    } else {
        println!(
            "The development database is not running, so the integration tests will skip.
Start it with `cargo xtask db up` to run them.\n"
        );
        run("cargo", &["test"], &[])
    }
}

/// Every gate, in the order that fails fastest, collecting independent failures.
///
/// A single early exit would hide the other three problems, so each gate runs
/// even when an earlier one failed, and the summary at the end is the answer.
fn verify() -> Result<(), String> {
    let uri = dev_uri()?;
    let database_available = postgres_ready();

    let mut results: Vec<(&str, Result<(), String>)> = Vec::new();

    println!("== formatting ==");
    results.push(("formatting", run("cargo", &["fmt", "--check"], &[])));

    println!("\n== lints ==");
    results.push((
        "lints",
        run(
            "cargo",
            &["clippy", "--all-targets", "--", "-D", "warnings"],
            &[],
        ),
    ));

    println!("\n== unit and layout tests ==");
    results.push(("unit tests", run("cargo", &["test", "--lib"], &[])));

    println!("\n== command-line contract ==");
    let env: Vec<(&str, &str)> = if database_available {
        vec![("IGNATIUS_TEST_PG_URI", uri.as_str())]
    } else {
        Vec::new()
    };
    results.push((
        "cli contract",
        run("cargo", &["test", "--test", "cli_contract"], &env),
    ));

    println!("\n== postgresql integration ==");
    if database_available {
        results.push((
            "integration",
            run(
                "cargo",
                &["test", "--test", "postgres_integration"],
                &[("IGNATIUS_TEST_PG_URI", &uri)],
            ),
        ));
    } else {
        println!("skipped: no development database. `cargo xtask db up` starts one.");
    }

    println!("\n{}", "=".repeat(60));
    let failures: Vec<&str> = results
        .iter()
        .filter(|(_, result)| result.is_err())
        .map(|(name, _)| *name)
        .collect();

    for (name, result) in &results {
        let status = if result.is_ok() { "pass" } else { "FAIL" };
        println!("  {status:<5} {name}");
    }
    if !database_available {
        // An absent container is an environment limitation, not a failure, but
        // it must never be reported as if the tests had passed.
        println!("  skip  integration (no development database)");
    }

    if failures.is_empty() {
        if database_available {
            println!("\nEverything passed.");
        } else {
            println!("\nEverything that ran passed. The integration tests did not run.");
        }
        Ok(())
    } else {
        Err(format!(
            "{} gate(s) failed: {}",
            failures.len(),
            failures.join(", ")
        ))
    }
}

/// Builds a release binary and puts it somewhere on PATH.
fn install(args: &[&str]) -> Result<(), String> {
    let target_dir = match args {
        ["--dir", dir] => PathBuf::from(dir),
        [] => default_install_dir()?,
        other => {
            return Err(format!(
                "unexpected arguments: {}. Use `cargo xtask install [--dir PATH]`.",
                other.join(" ")
            ));
        }
    };

    println!("Building a release binary...");
    run(
        "cargo",
        &["build", "--release", "--package", "ignatius"],
        &[],
    )?;

    let binary_name = if cfg!(windows) {
        "ignatius.exe"
    } else {
        "ignatius"
    };
    let built = repo_root().join("target").join("release").join(binary_name);
    if !built.exists() {
        return Err(format!("expected a binary at {}", built.display()));
    }

    std::fs::create_dir_all(&target_dir)
        .map_err(|err| format!("could not create {}: {err}", target_dir.display()))?;
    let destination = target_dir.join(binary_name);
    std::fs::copy(&built, &destination)
        .map_err(|err| format!("could not write {}: {err}", destination.display()))?;

    println!("Installed to {}", destination.display());
    if on_path(&target_dir) {
        println!("That directory is on your PATH, so `ignatius` works from anywhere now.");
    } else {
        println!(
            "That directory is NOT on your PATH. Add it, or run the binary by its full path:
  export PATH=\"{}:$PATH\"",
            target_dir.display()
        );
    }
    println!("\nRemove it again with:\n  rm {}", destination.display());
    Ok(())
}

/// Where to install by default: a per-user directory, never a system one.
fn default_install_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "could not find your home directory; pass --dir".to_owned())?;
    Ok(PathBuf::from(home).join(".local").join("bin"))
}

fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

fn flush() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
}
