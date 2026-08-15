//! Captures release identity at build time.
//!
//! Release identity has four distinct parts and none of them may be inferred from
//! another: the semantic product version comes from `Cargo.toml`, the source
//! revision from git, the build identity from CI, and the target triple from cargo.
//! A rebuild of the same source produces the same product version.

use std::process::Command;

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
    // Rebuild when HEAD moves so the recorded revision cannot go stale.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
    }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}
