//! Standalone test-only library consumer. No application/session startup.
#![forbid(unsafe_code)]
use ignatius::ui::terminal::{TerminalGuard, TerminalOptions, install_panic_hook};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn connect(pipe: &str) -> io::Result<File> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match OpenOptions::new().read(true).write(true).open(pipe) {
            Ok(file) => return Ok(file),
            Err(error) if error.raw_os_error() == Some(231) && Instant::now() < deadline => {
                // ERROR_PIPE_BUSY: a bounded connection retry, never a phase assertion.
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => return Err(error),
        }
    }
}
fn exchange(control: &mut File, line: &str) -> io::Result<()> {
    if line.len() > 512 || line.contains(['\n', '\r']) {
        return Err(io::Error::other("controlled line bound"));
    }
    writeln!(control, "{line}")?;
    control.flush()?;
    let mut ack = [0; 4];
    control.read_exact(&mut ack)?;
    if &ack != b"ACK\n" {
        return Err(io::Error::other("phase was not acknowledged"));
    }
    control.write_all(b"K\n")?;
    control.flush()
}
fn probe(
    host: &Path,
    pipe: &str,
    nonce: &str,
    sentinel: &str,
    operation: &str,
    phase: &str,
) -> io::Result<()> {
    // Reserve the authenticated Rust-client connection BEFORE spawning. The
    // observer can only connect after supervisor duplicates this actual Child
    // handle and ACKs registration, avoiding a PID/first-connection race.
    let mut control = connect(pipe)?;
    let mut observer = Command::new(host)
        .args(["--probe", pipe, nonce, sentinel, operation, phase])
        .arg(std::process::id().to_string())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    #[cfg(windows)]
    let handle = observer.as_raw_handle() as usize as u64;
    #[cfg(not(windows))]
    let handle = 0_u64; // Inert branch, still type checked on local macOS.
    let registration = exchange(
        &mut control,
        &format!(
            "P {nonce} {} {} {}",
            observer.id(),
            handle as u32,
            (handle >> 32) as u32
        ),
    );
    drop(control);
    if let Err(error) = registration {
        // Only this actual owned Child is killed/reaped on registration failure.
        let terminated = observer.kill().is_ok();
        let waited = observer.wait().is_ok();
        return Err(io::Error::other(format!(
            "observer registration failed; owned termination={terminated}; owned wait={waited}; original={error}"
        )));
    }
    let status = observer.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("owned native observation failed"))
    }
}
fn hang_ready(pipe: &str, nonce: &str) -> io::Result<()> {
    exchange(&mut connect(pipe)?, &format!("H {nonce}"))
}

const RENDERER_PANIC_PAYLOAD: &str = "W07_RENDERER_SYNTHETIC_PANIC";
const RENDERER_EXPECTED_ERROR_EXIT: i32 = 23;
const RENDERER_HOOK_FAILURE_EXIT: i32 = 24;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RendererCase {
    Normal,
    Error,
    Panic,
}

#[derive(Debug)]
struct RendererExpectedError;

impl fmt::Display for RendererExpectedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("deliberate renderer guard I/O error")
    }
}

impl std::error::Error for RendererExpectedError {}

fn renderer_phase(pipe: &str, nonce: &str, phase: &str) -> io::Result<()> {
    exchange(&mut connect(pipe)?, &format!("G {nonce} {phase}"))
}

fn renderer_marker(marker: &str) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "{marker}")?;
    stderr.flush()
}

fn install_renderer_previous_hook(pipe: String, nonce: String) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // The production hook has already attempted restoration. The actor
        // must verify it before any test marker or the default panic details.
        if info.payload().downcast_ref::<&str>().copied() != Some(RENDERER_PANIC_PAYLOAD)
            || renderer_phase(&pipe, &nonce, "restored").is_err()
            || renderer_marker("W07_RENDERER_PREVIOUS_HOOK").is_err()
        {
            let _ = renderer_marker("W07_RENDERER_HOOK_FAILED");
            std::process::exit(RENDERER_HOOK_FAILURE_EXIT);
        }
        previous(info);
    }));
}

fn renderer_deliberate_error() -> io::Result<()> {
    Err(io::Error::other(RendererExpectedError))
}

fn with_renderer_guard(
    case: RendererCase,
    options: TerminalOptions,
    pipe: &str,
    nonce: &str,
) -> io::Result<()> {
    let _guard = TerminalGuard::acquire(options)?;
    renderer_phase(pipe, nonce, "acquired")?;
    match case {
        RendererCase::Normal => {}
        RendererCase::Error => renderer_deliberate_error()?,
        RendererCase::Panic => std::panic::panic_any(RENDERER_PANIC_PAYLOAD),
    }
    Ok(())
}

fn require_terminal() -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "guard child did not receive native console handles",
        ));
    }
    Ok(())
}

fn run_renderer_case(case: &str, bits: &str, pipe: &str, nonce: &str) -> io::Result<()> {
    let case = match case {
        "normal" => RendererCase::Normal,
        "error" => RendererCase::Error,
        "panic" => RendererCase::Panic,
        _ => return Err(io::Error::other("invalid renderer guard case")),
    };
    let [digit @ b'0'..=b'7'] = bits.as_bytes() else {
        return Err(io::Error::other("invalid renderer guard option bits"));
    };
    let bits = *digit - b'0';
    if nonce.len() != 32
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || pipe != format!(r"\\.\pipe\ignatius-w07-renderer-guard-{nonce}")
    {
        return Err(io::Error::other("invalid renderer guard pipe or nonce"));
    }
    require_terminal()?;

    if case == RendererCase::Panic {
        install_renderer_previous_hook(pipe.to_owned(), nonce.to_owned());
    }
    install_panic_hook();
    let options = TerminalOptions {
        mouse: bits & 1 != 0,
        bracketed_paste: bits & 2 != 0,
        alternate_screen: bits & 4 != 0,
    };
    let expected_error = match with_renderer_guard(case, options, pipe, nonce) {
        Ok(()) if case == RendererCase::Normal => false,
        Err(error)
            if case == RendererCase::Error
                && error
                    .get_ref()
                    .is_some_and(|cause| cause.is::<RendererExpectedError>()) =>
        {
            true
        }
        Err(error) => return Err(error),
        Ok(()) => return Err(io::Error::other("unexpected renderer guard return")),
    };
    // Both return paths have left the guard's scope before this exchange.
    renderer_phase(pipe, nonce, "restored")?;
    if expected_error {
        renderer_marker("W07_RENDERER_EXPECTED_ERROR")?;
        std::process::exit(RENDERER_EXPECTED_ERROR_EXIT);
    }
    Ok(())
}

fn run() -> io::Result<()> {
    // All shared code is type-checked on other hosts; no terminal is acquired.
    if !cfg!(windows) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows ConPTY child requires actual Windows",
        ));
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let [mode, case, bits, pipe, nonce] = arguments.as_slice()
        && mode == "--renderer-case"
    {
        return run_renderer_case(case, bits, pipe, nonce);
    }
    if arguments.len() != 5 {
        return Err(io::Error::other(
            "expected controlled case, host, pipe, nonce and sentinel",
        ));
    }
    let case = arguments[0].as_str();
    if ![
        "no-guard",
        "known-unrestored",
        "hung-child",
        "capability",
        "cursor-routing",
    ]
    .contains(&case)
    {
        return Err(io::Error::other("case is outside first capability scope"));
    }
    let host = PathBuf::from(&arguments[1]);
    if !host.is_absolute() || !host.is_file() {
        return Err(io::Error::other(
            "native observer must be an existing absolute executable",
        ));
    }
    require_terminal()?;
    let (pipe, nonce, sentinel) = (&arguments[2], &arguments[3], &arguments[4]);
    if case == "cursor-routing" {
        // Exactly one native actor starts before baseline and owns all stages.
        // This selection never acquires a product Guard or accepts capability.
        return probe(&host, pipe, nonce, sentinel, "calibrate", "baseline");
    }
    probe(&host, pipe, nonce, sentinel, "seed", "baseline")?;
    match case {
        "no-guard" => {
            probe(&host, pipe, nonce, sentinel, "observe", "acquired")?;
            probe(&host, pipe, nonce, sentinel, "observe", "restored")?;
        }
        "known-unrestored" => probe(&host, pipe, nonce, sentinel, "perturb", "restored")?,
        "hung-child" => {
            // Baseline probe has exited. This process itself acknowledges the
            // phase, then performs no work; cold startup is not a proven hang.
            hang_ready(pipe, nonce)?;
            loop {
                std::thread::park();
            }
        }
        "capability" => {
            install_panic_hook();
            // Native raw/mouse/cursor capability first. Paste is deliberately
            // unobserved; the default/24-path matrix is not run by this child.
            let guard = TerminalGuard::acquire(TerminalOptions {
                mouse: true,
                alternate_screen: true,
                bracketed_paste: false,
            })?;
            probe(&host, pipe, nonce, sentinel, "observe", "acquired")?;
            drop(guard);
            probe(&host, pipe, nonce, sentinel, "observe", "restored")?;
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("W07-064 controlled child: {error}");
        std::process::exit(2);
    }
}
