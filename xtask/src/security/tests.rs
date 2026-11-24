use super::{
    PinnedSecurityImages, find_disallowed_dashes, find_inline_test_modules,
    gitleaks_output_scanned_history, parse_pinned_security_images, strip_ansi,
};
use std::fs;
use std::path::Path;

#[test]
fn parses_pinned_security_images_from_real_ci_workflow() {
    let ci_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join(".github")
        .join("workflows")
        .join("ci.yml");
    let ci_yaml = fs::read_to_string(&ci_path).expect("read ci.yml");

    let parsed = parse_pinned_security_images(&ci_yaml).expect("extract pins from ci.yml");
    assert!(
        parsed
            .actionlint
            .starts_with("docker.io/rhysd/actionlint@sha256:")
    );
    assert!(parsed.zizmor.starts_with("ghcr.io/zizmorcore/zizmor:"));
    assert!(parsed.zizmor.contains("@sha256:"));
    assert!(parsed.gitleaks.starts_with("ghcr.io/gitleaks/gitleaks:"));
    assert!(parsed.gitleaks.contains("@sha256:"));
}

#[test]
fn parses_fixture_ci_digests_and_ignores_comments() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let c = "c".repeat(64);
    let yaml = format!(
        "# comment docker.io/rhysd/actionlint:latest\n\
         jobs:\n\
           lint:\n\
             run: |\n\
               docker run --rm docker.io/rhysd/actionlint@sha256:{a} -color\n\
           sec:\n\
             run: |\n\
               docker run --rm ghcr.io/zizmorcore/zizmor:1.32.0@sha256:{b} .\n\
           leaks:\n\
             run: |\n\
               docker run --rm ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:{c} git\n"
    );

    let parsed = parse_pinned_security_images(&yaml).expect("valid fixture");
    assert_eq!(
        parsed,
        PinnedSecurityImages {
            actionlint: format!("docker.io/rhysd/actionlint@sha256:{a}"),
            zizmor: format!("ghcr.io/zizmorcore/zizmor:1.32.0@sha256:{b}"),
            gitleaks: format!("ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:{c}"),
        }
    );
}

#[test]
fn rejects_missing_or_unpinned_or_conflicting_ci_digests() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let c = "c".repeat(64);

    let missing_gitleaks = format!(
        "docker.io/rhysd/actionlint@sha256:{a}\n\
         ghcr.io/zizmorcore/zizmor:1.32.0@sha256:{b}\n"
    );
    assert!(parse_pinned_security_images(&missing_gitleaks).is_err());

    let unpinned_actionlint = format!(
        "docker.io/rhysd/actionlint:1.7.12\n\
         ghcr.io/zizmorcore/zizmor:1.32.0@sha256:{b}\n\
         ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:{c}\n"
    );
    assert!(parse_pinned_security_images(&unpinned_actionlint).is_err());

    let uppercase_digest = format!(
        "docker.io/rhysd/actionlint@sha256:{a}\n\
         ghcr.io/zizmorcore/zizmor:1.32.0@sha256:{b}\n\
         ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:{}\n",
        "C".repeat(64)
    );
    assert!(parse_pinned_security_images(&uppercase_digest).is_err());

    let conflicting_gitleaks = format!(
        "docker.io/rhysd/actionlint@sha256:{a}\n\
         ghcr.io/zizmorcore/zizmor:1.32.0@sha256:{b}\n\
         ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:{c}\n\
         ghcr.io/gitleaks/gitleaks:v8.30.0@sha256:{a}\n"
    );
    assert!(parse_pinned_security_images(&conflicting_gitleaks).is_err());
}

#[test]
fn detects_inline_mod_tests_and_ignores_sidecars_and_comments() {
    let clean = "\
// mod tests {
/// Example: mod tests {
pub fn helper() {}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_support {
    pub fn x() {}
}
";
    assert!(find_inline_test_modules(clean).is_empty());

    let offending = "\
// mod tests {
pub fn a() {}
#[cfg(test)]
mod tests {
    #[test]
    fn bad() {}
}
pub mod  tests{ }
";
    assert_eq!(find_inline_test_modules(offending), vec![4, 8]);
}

#[test]
fn detects_em_and_en_dashes_by_line() {
    let clean = "Plain hyphen - and double -- are allowed.\n";
    assert!(find_disallowed_dashes(clean).is_empty());

    let dirty = "Line one is fine.\n\
                 Line two has an em dash \u{2014} here.\n\
                 Line three has an en dash \u{2013} here.\n";
    assert_eq!(
        find_disallowed_dashes(dirty),
        vec![(2, '\u{2014}'), (3, '\u{2013}')]
    );
}

#[test]
fn gitleaks_history_guard_catches_zero_commit_worktree_failures() {
    assert!(gitleaks_output_scanned_history(
        "INF 412 commits scanned.\nINF no leaks found\n"
    ));
    assert!(gitleaks_output_scanned_history(
        "INF 10 commits scanned.\nINF no leaks found\n"
    ));
    assert!(gitleaks_output_scanned_history(
        "INF 310 commits scanned.\nINF no leaks found\n"
    ));
    assert!(gitleaks_output_scanned_history(
        "INF 1 commit scanned.\nINF no leaks found\n"
    ));
    assert!(!gitleaks_output_scanned_history(
        "ERR [git] fatal: not a git repository: /host/.git/worktrees/w\nINF 0 commits scanned.\n"
    ));
    assert!(!gitleaks_output_scanned_history("INF 0 commits scanned.\n"));
    assert!(!gitleaks_output_scanned_history("INF no leaks found\n"));
}

#[test]
fn strip_ansi_removes_csi_sequences_and_preserves_plain_text() {
    assert_eq!(strip_ansi("plain text"), "plain text");
    assert_eq!(
        strip_ansi("\x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1m312 commits scanned.\x1b[0m"),
        "12:18PM INF 312 commits scanned."
    );
    assert_eq!(
        strip_ansi("\x1b[38;5;196m✓\x1b[0m \x1b[?25h\x1b[2 qcafé\x1b[m"),
        "✓ café"
    );
    assert_eq!(strip_ansi("tail\x1b"), "tail\x1b");
    assert_eq!(strip_ansi("tail\x1b[32"), "tail\x1b[32");
    assert_eq!(
        strip_ansi("\x1b[32\nINF 312 commits scanned."),
        "\x1b[32\nINF 312 commits scanned."
    );
}

#[test]
fn gitleaks_history_guard_handles_zerolog_ansi_stderr() {
    assert!(gitleaks_output_scanned_history(
        "\x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1m312 commits scanned.\x1b[0m\n\
         \x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1mno leaks found\x1b[0m\n"
    ));
    assert!(gitleaks_output_scanned_history(
        "\x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1m1 commit scanned.\x1b[0m\n"
    ));
    assert!(!gitleaks_output_scanned_history(
        "\x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1m0 commits scanned.\x1b[0m\n"
    ));
    assert!(!gitleaks_output_scanned_history(
        "\x1b[90m12:18PM\x1b[0m \x1b[31mERR\x1b[0m \x1b[1m[git] fatal: not a git repository: /host/.git/worktrees/w\x1b[0m\n\
         \x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1m0 commits scanned.\x1b[0m\n"
    ));
    assert!(!gitleaks_output_scanned_history(
        "\x1b[90m12:18PM\x1b[0m \x1b[31mERR\x1b[0m \x1b[1m[git] fatal: not a git repository\x1b[0m\n\
         \x1b[90m12:18PM\x1b[0m \x1b[32mINF\x1b[0m \x1b[1m12 commits scanned.\x1b[0m\n"
    ));
}
