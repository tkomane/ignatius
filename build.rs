//! Captures release identity at build time.
//!
//! Release identity has four distinct parts and none of them may be inferred from
//! another: the semantic product version comes from `Cargo.toml`, the source
//! revision from git, the build identity from CI, and the target triple from cargo.
//! A rebuild of the same source produces the same product version.

use std::{path::Path, process::Command};

fn main() {
    let revision = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let clean = match git(&["status", "--porcelain"]) {
        Some(out) if out.is_empty() => "clean",
        Some(_) => "modified",
        None => "unknown",
    };
    let build_identity =
        std::env::var("IGNATIUS_BUILD_IDENTITY").unwrap_or_else(|_| "local".into());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    let rustc = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=IGNATIUS_SOURCE_REVISION={revision}");
    println!("cargo:rustc-env=IGNATIUS_SOURCE_CLEAN={clean}");
    println!("cargo:rustc-env=IGNATIUS_TARGET_TRIPLE={target}");
    println!("cargo:rustc-env=IGNATIUS_BUILD_IDENTITY={build_identity}");
    println!("cargo:rustc-env=IGNATIUS_RUSTC_VERSION={rustc}");
    println!("cargo:rerun-if-env-changed=IGNATIUS_BUILD_IDENTITY");
    println!("cargo:rerun-if-changed=build.rs");
    // The identity includes Git state, so a source edit, staged change, branch
    // commit or packed ref change must rebuild the metadata before the binary is
    // considered current. Listing Git's known worktree files avoids watching
    // `target/`, while the index and ref files cover changes Cargo cannot infer
    // from a Rust source dependency.
    for path in git_lines(&["ls-files", "-co", "--exclude-standard"]) {
        watch_if_exists(&path);
    }
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        watch_if_exists(&format!("{dir}/HEAD"));
        watch_if_exists(&format!("{dir}/index"));
        watch_if_exists(&format!("{dir}/packed-refs"));
    }
    if let Some(reference) = git(&["symbolic-ref", "--quiet", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        watch_if_exists(&path);
    }
}

fn watch_if_exists(path: &str) {
    if Path::new(path).exists() {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}

fn git_lines(args: &[&str]) -> Vec<String> {
    let Some(out) = Command::new("git").args(args).output().ok() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}
