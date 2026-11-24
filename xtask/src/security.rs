//! Local reproducible security, policy, and repository hygiene gates.
//!
//! Container-backed scanners (`actionlint`, `zizmor`, and `gitleaks`) read
//! their immutable `sha256` image references directly from
//! `.github/workflows/ci.yml` so there is never a second copy to drift.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const ACTIONLINT_REPOSITORY: &str = "docker.io/rhysd/actionlint";
const ZIZMOR_REPOSITORY: &str = "ghcr.io/zizmorcore/zizmor";
const GITLEAKS_REPOSITORY: &str = "ghcr.io/gitleaks/gitleaks";

const GOVERNED_DASH_DOCS: [&str; 5] = [
    "README.md",
    "docs/status.md",
    "docs/product/roadmap.md",
    "docs/operations/verification.md",
    ".specify/memory/constitution.md",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PinnedSecurityImages {
    pub(crate) actionlint: String,
    pub(crate) zizmor: String,
    pub(crate) gitleaks: String,
}

enum GateOutcome {
    Pass,
    Fail(String),
    Skip(String),
}

struct GateReport {
    name: &'static str,
    outcome: GateOutcome,
}

/// Runs every local policy, hygiene, and security gate, printing a
/// `pass` / `FAIL` / `skip` summary and exiting non-zero if any gate fails.
pub(crate) fn run() -> Result<(), String> {
    let root = super::repo_root();
    let ci_path = root.join(".github").join("workflows").join("ci.yml");
    let ci_yaml = fs::read_to_string(&ci_path)
        .map_err(|err| format!("could not read {}: {err}", ci_path.display()))?;
    let images = parse_pinned_security_images(&ci_yaml)?;
    let docker_available = super::run_quiet("docker", &["info"]);

    let mut reports = Vec::new();

    println!("== policy & supply-chain contracts ==");
    reports.push(run_gate("policy & supply-chain contracts", || {
        run_policy_and_supply_chain_contracts()
    }));

    println!("\n== repository hygiene ==");
    reports.push(run_gate("AGENTS.md == CLAUDE.md", || {
        check_agent_instructions_match(&root)
    }));
    reports.push(run_gate("sidecar unit tests (src/)", || {
        check_no_inline_tests_in_src(&root.join("src")).map(|checked| {
            println!("  checked {checked} production Rust source file(s) under src/");
        })
    }));
    reports.push(run_gate("governed prose dashes", || {
        check_no_em_or_en_dashes(&root).map(|checked| {
            println!("  checked {checked} governed markdown file(s)");
        })
    }));

    println!("\n== actionlint ==");
    reports.push(GateReport {
        name: "actionlint",
        outcome: run_container_or_local_tool(
            docker_available,
            &root,
            &images.actionlint,
            &["-color"],
            "actionlint",
            &[],
        ),
    });

    println!("\n== zizmor ==");
    let zizmor_args = [
        "--offline",
        "--no-progress",
        "--config",
        ".github/zizmor.yml",
        ".",
    ];
    reports.push(GateReport {
        name: "zizmor",
        outcome: run_container_or_local_tool(
            docker_available,
            &root,
            &images.zizmor,
            &zizmor_args,
            "zizmor",
            &zizmor_args,
        ),
    });

    println!("\n== gitleaks ==");
    reports.push(GateReport {
        name: "gitleaks",
        outcome: run_gitleaks(docker_available, &root, &images.gitleaks),
    });

    println!("\n== cargo-deny ==");
    reports.push(GateReport {
        name: "cargo-deny",
        outcome: run_cargo_deny(&root),
    });

    print_summary(&reports)
}

fn run_gate(name: &'static str, check: impl FnOnce() -> Result<(), String>) -> GateReport {
    let outcome = match check() {
        Ok(()) => GateOutcome::Pass,
        Err(reason) => {
            eprintln!("  FAIL ({name}): {reason}");
            GateOutcome::Fail(reason)
        }
    };
    GateReport { name, outcome }
}

fn print_summary(reports: &[GateReport]) -> Result<(), String> {
    println!("\n{}", "=".repeat(60));
    let mut failed_names = Vec::new();
    let mut skipped_count = 0usize;

    for report in reports {
        match &report.outcome {
            GateOutcome::Pass => println!("  pass  {}", report.name),
            GateOutcome::Fail(reason) => {
                println!("  FAIL  {} ({reason})", report.name);
                failed_names.push(report.name);
            }
            GateOutcome::Skip(reason) => {
                println!("  skip  {} ({reason})", report.name);
                skipped_count += 1;
            }
        }
    }

    if failed_names.is_empty() {
        if skipped_count == 0 {
            println!("\nAll security gates passed.");
        } else {
            println!(
                "\nAll executed security gates passed. {skipped_count} skipped gate(s) remain unverified."
            );
        }
        Ok(())
    } else {
        Err(format!(
            "{} security gate(s) failed: {}",
            failed_names.len(),
            failed_names.join(", ")
        ))
    }
}

fn run_policy_and_supply_chain_contracts() -> Result<(), String> {
    super::run(
        "cargo",
        &[
            "test",
            "--locked",
            "--test",
            "workflow_contract",
            "--test",
            "release_contract",
            "--test",
            "runtime_privacy_contract",
            "--test",
            "documentation_matches_the_build",
        ],
        &[],
    )
}

/// Extracts the immutable digest-pinned image references for `actionlint`,
/// `zizmor`, and `gitleaks` from `.github/workflows/ci.yml`.
pub(crate) fn parse_pinned_security_images(ci_yaml: &str) -> Result<PinnedSecurityImages, String> {
    let mut pins: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();

    for (line_index, line) in ci_yaml.lines().enumerate() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        for raw in line.split_whitespace() {
            let token = raw.trim_matches(|character: char| matches!(character, '"' | '\'' | '\\'));
            let token = token.strip_prefix("docker://").unwrap_or(token);
            let Some(repository) = matches_security_repository(token) else {
                continue;
            };
            let Some((image_and_tag, digest_part)) = token.split_once('@') else {
                return Err(format!(
                    ".github/workflows/ci.yml:{} references {repository} without an immutable digest: {token}",
                    line_index + 1
                ));
            };
            let parsed_repo = match image_and_tag.rsplit_once(':') {
                Some((repo, tag)) if !tag.contains('/') => repo,
                _ => image_and_tag,
            };
            if parsed_repo != repository {
                continue;
            }
            let Some(hex_digest) = digest_part.strip_prefix("sha256:") else {
                return Err(format!(
                    ".github/workflows/ci.yml:{} pins {repository} without a sha256 digest: {token}",
                    line_index + 1
                ));
            };
            if !is_lowercase_sha256_hex(hex_digest) {
                return Err(format!(
                    ".github/workflows/ci.yml:{} must pin {repository} to a 64-character lowercase sha256 digest: {token}",
                    line_index + 1
                ));
            }
            pins.entry(repository).or_default().insert(token.to_owned());
        }
    }

    Ok(PinnedSecurityImages {
        actionlint: take_single_pin(&pins, ACTIONLINT_REPOSITORY)?,
        zizmor: take_single_pin(&pins, ZIZMOR_REPOSITORY)?,
        gitleaks: take_single_pin(&pins, GITLEAKS_REPOSITORY)?,
    })
}

fn matches_security_repository(token: &str) -> Option<&'static str> {
    for repository in [
        ACTIONLINT_REPOSITORY,
        ZIZMOR_REPOSITORY,
        GITLEAKS_REPOSITORY,
    ] {
        if token == repository {
            return Some(repository);
        }
        if let Some(rest) = token.strip_prefix(repository)
            && (rest.starts_with(':') || rest.starts_with('@'))
        {
            return Some(repository);
        }
    }
    None
}

fn is_lowercase_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn take_single_pin(
    pins: &BTreeMap<&'static str, BTreeSet<String>>,
    repository: &'static str,
) -> Result<String, String> {
    let Some(values) = pins.get(repository) else {
        return Err(format!(
            "missing digest-pinned {repository} image in .github/workflows/ci.yml"
        ));
    };
    if values.len() != 1 {
        return Err(format!(
            "{repository} has inconsistent image pins in .github/workflows/ci.yml: {values:?}"
        ));
    }
    Ok(values.iter().next().expect("checked len == 1").clone())
}

pub(crate) fn check_agent_instructions_match(root: &Path) -> Result<(), String> {
    let agents_path = root.join("AGENTS.md");
    let claude_path = root.join("CLAUDE.md");
    let agents = fs::read(&agents_path)
        .map_err(|err| format!("could not read {}: {err}", agents_path.display()))?;
    let claude = fs::read(&claude_path)
        .map_err(|err| format!("could not read {}: {err}", claude_path.display()))?;

    if agents == claude {
        return Ok(());
    }
    let first_diff = agents
        .iter()
        .zip(claude.iter())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| agents.len().min(claude.len()));
    Err(format!(
        "AGENTS.md ({} bytes) and CLAUDE.md ({} bytes) differ starting at byte offset {first_diff}",
        agents.len(),
        claude.len()
    ))
}

pub(crate) fn check_no_inline_tests_in_src(src_root: &Path) -> Result<usize, String> {
    let mut files = Vec::new();
    collect_production_rust_files(src_root, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!(
            "found no production Rust files under {}",
            src_root.display()
        ));
    }

    let mut violations = Vec::new();
    for path in &files {
        let source = fs::read_to_string(path)
            .map_err(|err| format!("could not read {}: {err}", path.display()))?;
        for line_no in find_inline_test_modules(&source) {
            violations.push(format!("{}:{line_no}", path.display()));
        }
    }

    if violations.is_empty() {
        Ok(files.len())
    } else {
        Err(format!(
            "inline `mod tests {{` found in production source (move to sidecar tests.rs so CodeQL excludes tests): {}",
            violations.join(", ")
        ))
    }
}

fn collect_production_rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|err| format!("could not read {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("could not read directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("tests") {
                continue;
            }
            collect_production_rust_files(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs")
            && path.file_name().and_then(|name| name.to_str()) != Some("tests.rs")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Returns 1-based line numbers containing inline `mod tests {` declarations,
/// ignoring `//` comments and allowing sidecar `mod tests;` declarations.
pub(crate) fn find_inline_test_modules(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| has_inline_mod_tests_on_line(line).then_some(index + 1))
        .collect()
}

fn has_inline_mod_tests_on_line(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("");
    let bytes = code.as_bytes();
    let mut offset = 0;

    while let Some(found) = code[offset..].find("mod") {
        let idx = offset + found;
        let boundary_before =
            idx == 0 || (!bytes[idx - 1].is_ascii_alphanumeric() && bytes[idx - 1] != b'_');
        if boundary_before {
            let after_mod = &code[idx + "mod".len()..];
            let trimmed_after_mod = after_mod.trim_start();
            let had_whitespace = trimmed_after_mod.len() < after_mod.len();
            if had_whitespace
                && let Some(after_tests) = trimmed_after_mod.strip_prefix("tests")
                && after_tests.trim_start().starts_with('{')
            {
                return true;
            }
        }
        offset = idx + "mod".len();
    }

    false
}

pub(crate) fn check_no_em_or_en_dashes(root: &Path) -> Result<usize, String> {
    let mut violations = Vec::new();
    for relative in GOVERNED_DASH_DOCS {
        let path = root.join(relative);
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("could not read {}: {err}", path.display()))?;
        for (line_no, ch) in find_disallowed_dashes(&text) {
            let kind = if ch == '\u{2014}' {
                "em dash (U+2014)"
            } else {
                "en dash (U+2013)"
            };
            violations.push(format!("{relative}:{line_no}: {kind}"));
        }
    }

    if violations.is_empty() {
        Ok(GOVERNED_DASH_DOCS.len())
    } else {
        Err(format!(
            "disallowed Unicode dashes in governed docs: {}",
            violations.join(", ")
        ))
    }
}

/// Returns 1-based `(line, char)` pairs for any Unicode em dash (`\u{2014}`)
/// or en dash (`\u{2013}`) in `text`.
pub(crate) fn find_disallowed_dashes(text: &str) -> Vec<(usize, char)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for ch in line.chars() {
            if matches!(ch, '\u{2014}' | '\u{2013}') {
                found.push((index + 1, ch));
            }
        }
    }
    found
}

fn run_container_or_local_tool(
    docker_available: bool,
    root: &Path,
    pinned_image: &str,
    container_args: &[&str],
    local_program: &str,
    local_args: &[&str],
) -> GateOutcome {
    if docker_available {
        return match run_docker_readonly(root, pinned_image, container_args) {
            Ok(()) => GateOutcome::Pass,
            Err(reason) => {
                eprintln!("  FAIL ({local_program}): {reason}");
                GateOutcome::Fail(reason)
            }
        };
    }

    if let Some(binary) = find_on_path(local_program) {
        let binary_str = binary.to_string_lossy().into_owned();
        return match super::run(&binary_str, local_args, &[]) {
            Ok(()) => GateOutcome::Pass,
            Err(reason) => {
                eprintln!("  FAIL ({local_program}): {reason}");
                GateOutcome::Fail(reason)
            }
        };
    }

    let reason = format!("docker daemon unavailable and `{local_program}` is not on PATH");
    println!("  skipping: {reason}");
    GateOutcome::Skip(reason)
}

fn docker_readonly_args<'a>(
    volume: &'a str,
    image: &'a str,
    tool_args: &[&'a str],
) -> Vec<&'a str> {
    let mut args = vec![
        "run",
        "--rm",
        "--network",
        "none",
        "--volume",
        volume,
        "--workdir",
        "/repo",
        image,
    ];
    args.extend_from_slice(tool_args);
    args
}

fn run_docker_readonly(root: &Path, image: &str, tool_args: &[&str]) -> Result<(), String> {
    let volume = format!("{}:/repo:ro", root.display());
    let args = docker_readonly_args(&volume, image, tool_args);
    super::run("docker", &args, &[])
}

fn run_gitleaks(docker_available: bool, root: &Path, pinned_image: &str) -> GateOutcome {
    let has_git_dir = root.join(".git").is_dir();
    let gitleaks_args = ["git", "--no-banner", "--redact", "--log-opts=--all"];

    if docker_available && has_git_dir {
        let volume = format!("{}:/repo:ro", root.display());
        let args = docker_readonly_args(&volume, pinned_image, &gitleaks_args);
        return run_gitleaks_checked("docker", &args, root);
    }

    // Linked git worktrees store `.git` as a pointer file to an external
    // `gitdir`, which is not inside `<repo>:/repo:ro`. Use host `gitleaks` when
    // available rather than reporting a broken zero-commit container scan.
    if let Some(binary) = find_on_path("gitleaks") {
        let binary_str = binary.to_string_lossy().into_owned();
        return run_gitleaks_checked(&binary_str, &gitleaks_args, root);
    }

    let reason = if docker_available && !has_git_dir {
        "linked git worktree (.git points outside /repo mount) and `gitleaks` is not on PATH"
    } else {
        "docker daemon unavailable and `gitleaks` is not on PATH"
    };
    println!("  skipping: {reason}");
    GateOutcome::Skip(reason.to_owned())
}

fn run_gitleaks_checked(program: &str, args: &[&str], root: &Path) -> GateOutcome {
    let output = match Command::new(program).args(args).current_dir(root).output() {
        Ok(output) => output,
        Err(err) => {
            let reason = format!("could not start `{program}`: {err}");
            eprintln!("  FAIL (gitleaks): {reason}");
            return GateOutcome::Fail(reason);
        }
    };

    let _ = std::io::stdout().write_all(&output.stdout);
    let _ = std::io::stderr().write_all(&output.stderr);

    if !output.status.success() {
        let reason = format!(
            "`{program} {}` failed with {}",
            args.join(" "),
            output
                .status
                .code()
                .map_or_else(|| "a signal".to_owned(), |c| format!("exit code {c}"))
        );
        eprintln!("  FAIL (gitleaks): {reason}");
        return GateOutcome::Fail(reason);
    }

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !gitleaks_output_scanned_history(&combined) {
        let reason =
            "gitleaks exited 0 without scanning git history (reported git error or 0 commits scanned)"
                .to_owned();
        eprintln!("  FAIL (gitleaks): {reason}");
        return GateOutcome::Fail(reason);
    }

    GateOutcome::Pass
}

/// Rejects false-green `gitleaks git` runs where git could not open the repo
/// and `gitleaks` still exited 0 after scanning zero commits.
pub(crate) fn gitleaks_output_scanned_history(output: &str) -> bool {
    if output.contains("fatal: not a git repository") {
        return false;
    }

    let mut saw_positive = false;
    for line in output.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        for window in words.windows(3) {
            let [count_token, unit_token, scanned_token] = [window[0], window[1], window[2]];
            if matches!(unit_token, "commit" | "commits")
                && scanned_token.trim_end_matches('.') == "scanned"
                && let Ok(count) = count_token.parse::<u64>()
            {
                if count == 0 {
                    return false;
                }
                saw_positive = true;
            }
        }
    }

    saw_positive
}

fn run_cargo_deny(root: &Path) -> GateOutcome {
    let Some(cargo_deny) = find_on_path("cargo-deny") else {
        let reason = "`cargo-deny` is not on PATH".to_owned();
        println!("  skipping: {reason}");
        return GateOutcome::Skip(reason);
    };

    let cargo_deny_str = cargo_deny.to_string_lossy().into_owned();
    let workspace_check = super::run(
        &cargo_deny_str,
        &[
            "--locked",
            "--workspace",
            "--all-features",
            "check",
            "advisories",
            "licenses",
            "bans",
            "sources",
        ],
        &[],
    );

    let script_path = root
        .join("scripts")
        .join("check-driver-registry-advisories.py");
    let script_str = script_path.to_string_lossy().into_owned();
    let registry_check = super::run("python3", &[&script_str, &cargo_deny_str], &[]);

    match (workspace_check, registry_check) {
        (Ok(()), Ok(())) => GateOutcome::Pass,
        (Err(err), Ok(())) | (Ok(()), Err(err)) => {
            eprintln!("  FAIL (cargo-deny): {err}");
            GateOutcome::Fail(err)
        }
        (Err(first), Err(second)) => {
            let combined = format!("{first}; {second}");
            eprintln!("  FAIL (cargo-deny): {combined}");
            GateOutcome::Fail(combined)
        }
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(program);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
        if cfg!(windows) && candidate.extension().is_none() {
            let exe = dir.join(format!("{program}.exe"));
            if is_executable_file(&exe) {
                return Some(exe);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
