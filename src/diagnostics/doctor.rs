//! The `doctor` command's check model.
//!
//! Doctor answers one question: is this machine set up so the product can work,
//! and if not, what should the user do about it. Every check therefore carries a
//! status, a detail, and - when it is not `Ok` - a concrete next action.
//!
//! Doctor collects independent failures rather than stopping at the first, and
//! distinguishes a real failure from an expected condition. Checks take plain
//! facts as input so this module depends on neither the terminal nor the driver.

use crate::config::paths::Paths;
use crate::config::store::{ConfigSource, Loaded};
use crate::diagnostics::Diagnostic;
use std::path::Path;

/// Outcome of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckStatus {
    /// Working as intended.
    Ok,
    /// Deliberately not run, with a stated reason.
    Skipped,
    /// Usable, but something is worth knowing.
    Warn,
    /// Not usable as configured.
    Fail,
}

impl CheckStatus {
    /// Text label. Status is never carried by colour alone.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Skipped => "SKIPPED",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

/// One diagnostic check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Short name, stable enough to grep for in a support conversation.
    pub name: String,
    /// Outcome.
    pub status: CheckStatus,
    /// What was found.
    pub detail: String,
    /// What to do about it, required for anything that is not `Ok`.
    pub next_action: Option<String>,
}

impl Check {
    /// A passing check.
    #[must_use]
    pub fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Ok,
            detail: crate::diagnostics::redact_text(&detail.into()),
            next_action: None,
        }
    }

    /// A check that did not run, with the reason it was skipped.
    #[must_use]
    pub fn skipped(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Skipped,
            detail: crate::diagnostics::redact_text(&detail.into()),
            next_action: None,
        }
    }

    /// A warning, with the action that clears it.
    #[must_use]
    pub fn warn(
        name: impl Into<String>,
        detail: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warn,
            detail: crate::diagnostics::redact_text(&detail.into()),
            next_action: Some(crate::diagnostics::redact_text(&next_action.into())),
        }
    }

    /// A failure, with the action that fixes it.
    #[must_use]
    pub fn fail(
        name: impl Into<String>,
        detail: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: crate::diagnostics::redact_text(&detail.into()),
            next_action: Some(crate::diagnostics::redact_text(&next_action.into())),
        }
    }
}

/// Facts about the terminal, gathered by the UI layer and passed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFacts {
    /// Whether stdout is attached to a terminal.
    pub is_terminal: bool,
    /// Value of `TERM`, when set.
    pub term: Option<String>,
    /// Value of `TERM_PROGRAM`, when set. Identifies Warp and similar.
    pub term_program: Option<String>,
    /// Whether `NO_COLOR` is set.
    pub no_color: bool,
    /// Detected size, when it could be read.
    pub size: Option<(u16, u16)>,
    /// Whether the environment looks able to render Unicode box drawing.
    pub unicode: bool,
}

/// The complete doctor report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Checks in the order they were run.
    pub checks: Vec<Check>,
}

impl Report {
    /// Worst status in the report, used for the exit code.
    #[must_use]
    pub fn worst(&self) -> CheckStatus {
        self.checks
            .iter()
            .map(|c| c.status)
            .max()
            .unwrap_or(CheckStatus::Ok)
    }

    /// Counts by status, for the summary line.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut counts = (0, 0, 0, 0);
        for check in &self.checks {
            match check.status {
                CheckStatus::Ok => counts.0 += 1,
                CheckStatus::Warn => counts.1 += 1,
                CheckStatus::Fail => counts.2 += 1,
                CheckStatus::Skipped => counts.3 += 1,
            }
        }
        counts
    }

    /// Human-readable report with an explicit final summary and visible counts.
    #[must_use]
    pub fn render_plain(&self) -> String {
        let mut out = String::new();
        let width = self
            .checks
            .iter()
            .map(|c| c.name.chars().count())
            .max()
            .unwrap_or(0);
        for check in &self.checks {
            out.push_str(&format!(
                "{:<7} {:<width$}  {}\n",
                check.status.label(),
                check.name,
                check.detail
            ));
            if let Some(action) = &check.next_action {
                out.push_str(&format!("{:<7} {:<width$}  -> {}\n", "", "", action));
            }
        }
        let (ok, warn, fail, skipped) = self.counts();
        out.push('\n');
        out.push_str(&format!(
            "Summary: {ok} ok, {warn} warning(s), {fail} failure(s), {skipped} skipped\n"
        ));
        out
    }

    /// Machine-readable report for `doctor --json`.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let (ok, warn, fail, skipped) = self.counts();
        serde_json::json!({
            "product": crate::branding::PRODUCT_NAME,
            "version": crate::branding::VERSION,
            "checks": self.checks.iter().map(|c| serde_json::json!({
                "name": c.name,
                "status": c.status.label(),
                "detail": c.detail,
                "next_action": c.next_action,
            })).collect::<Vec<_>>(),
            "summary": { "ok": ok, "warn": warn, "fail": fail, "skipped": skipped },
        })
    }
}

/// Builds the environment portion of the report.
///
/// `config` is the result of loading configuration, so a broken configuration
/// file becomes a reported check rather than stopping doctor from running - which
/// is exactly when a user needs doctor most.
#[must_use]
pub fn environment_report(
    paths: &Paths,
    config: &Result<Loaded, Diagnostic>,
    terminal: &TerminalFacts,
) -> Report {
    let mut checks = Vec::new();

    checks.push(Check::ok(
        "product",
        format!(
            "{} {} ({}, {})",
            crate::branding::PRODUCT_NAME,
            crate::branding::VERSION,
            crate::branding::TARGET_TRIPLE,
            crate::branding::BUILD_IDENTITY
        ),
    ));

    checks.push(Check::ok(
        "platform",
        format!(
            "{}, file permissions {} enforced, unix sockets {}",
            crate::platform::name(),
            if crate::platform::enforces_file_permissions() {
                "are"
            } else {
                "are not"
            },
            if crate::platform::supports_unix_sockets() {
                "supported"
            } else {
                "unsupported"
            },
        ),
    ));

    checks.push(match config {
        Ok(loaded) => {
            let source = match loaded.source {
                ConfigSource::Defaults => "built-in defaults (no file yet)".to_owned(),
                ConfigSource::File => format!("loaded from {}", paths.config_file.display()),
            };
            match loaded.pending_migration {
                Some((from, to)) => Check::warn(
                    "configuration",
                    format!("{source}; schema version {from} is older than {to}"),
                    format!(
                        "run `{} config migrate` to update it",
                        crate::branding::BINARY_NAME
                    ),
                ),
                None => Check::ok("configuration", source),
            }
        }
        Err(diagnostic) => Check::fail(
            "configuration",
            diagnostic.headline.clone(),
            diagnostic.next_action.clone().unwrap_or_else(|| {
                format!("run `{} config validate`", crate::branding::BINARY_NAME)
            }),
        ),
    });

    checks.push(directory_check("config directory", &paths.config_dir));
    checks.push(directory_check("data directory", &paths.data_dir));

    checks.push(terminal_check(terminal));
    checks.push(colour_check(terminal));
    checks.push(size_check(terminal));

    Report { checks }
}

fn directory_check(name: &str, dir: &Path) -> Check {
    if !dir.exists() {
        // Not an error: directories are created on first write.
        return Check::ok(
            name,
            format!("{} (will be created on first write)", dir.display()),
        );
    }
    match std::fs::metadata(dir) {
        Ok(meta) if meta.permissions().readonly() => Check::warn(
            name,
            format!("{} is read-only", dir.display()),
            "grant write permission, or point IGNATIUS_CONFIG_DIR somewhere writable",
        ),
        Ok(_) => Check::ok(name, dir.display().to_string()),
        Err(err) => Check::fail(
            name,
            format!("{} could not be inspected: {err}", dir.display()),
            "check the path and its permissions",
        ),
    }
}

fn terminal_check(terminal: &TerminalFacts) -> Check {
    if !terminal.is_terminal {
        return Check::skipped(
            "terminal",
            "stdout is not a terminal, so interactive capabilities were not probed",
        );
    }
    let term = terminal.term.as_deref().unwrap_or("unset");
    let program = terminal.term_program.as_deref().unwrap_or("unknown");
    if term == "dumb" {
        return Check::warn(
            "terminal",
            format!("TERM={term} cannot support the full-screen interface"),
            "use --plain for line-oriented output",
        );
    }
    Check::ok("terminal", format!("TERM={term}, program={program}"))
}

fn colour_check(terminal: &TerminalFacts) -> Check {
    if terminal.no_color {
        return Check::ok("colour", "NO_COLOR is set, so output stays uncoloured");
    }
    if !terminal.is_terminal {
        return Check::ok("colour", "not a terminal, so colour is off");
    }
    Check::ok(
        "colour",
        "colour available; meaning is also carried by text labels",
    )
}

fn size_check(terminal: &TerminalFacts) -> Check {
    match terminal.size {
        None => Check::skipped("terminal size", "size is only measurable on a terminal"),
        Some((cols, rows)) if cols < crate::ui::MIN_COLUMNS || rows < crate::ui::MIN_ROWS => {
            Check::warn(
                "terminal size",
                format!(
                    "{cols}x{rows} is below the {}x{} minimum for the full layout",
                    crate::ui::MIN_COLUMNS,
                    crate::ui::MIN_ROWS
                ),
                "resize the window, or use --plain",
            )
        }
        Some((cols, rows)) => Check::ok("terminal size", format!("{cols}x{rows}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::store::Loaded;
    use crate::config::{Config, Paths};

    fn facts() -> TerminalFacts {
        TerminalFacts {
            is_terminal: true,
            term: Some("xterm-256color".into()),
            term_program: Some("WarpTerminal".into()),
            no_color: false,
            size: Some((120, 30)),
            unicode: true,
        }
    }

    fn ok_config() -> Result<Loaded, Diagnostic> {
        Ok(Loaded {
            config: Config::default(),
            source: ConfigSource::Defaults,
            pending_migration: None,
        })
    }

    #[test]
    fn a_healthy_environment_reports_no_failures() {
        let dir = tempfile::tempdir().expect("temp dir");
        let report = environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &facts());
        assert_eq!(report.worst(), CheckStatus::Ok, "{}", report.render_plain());
        assert!(report.checks.len() >= 6);
    }

    #[test]
    fn every_non_ok_check_states_a_next_action() {
        let dir = tempfile::tempdir().expect("temp dir");
        let broken = Err(Diagnostic::new(
            crate::diagnostics::DiagnosticKind::Config,
            "configuration file is not valid",
            "parsing configuration",
        )
        .next_action("fix the syntax"));
        let mut small = facts();
        small.size = Some((40, 10));
        small.term = Some("dumb".into());

        let report = environment_report(&Paths::rooted_at(dir.path()), &broken, &small);
        for check in &report.checks {
            if matches!(check.status, CheckStatus::Warn | CheckStatus::Fail) {
                assert!(
                    check.next_action.is_some(),
                    "{} has no next action",
                    check.name
                );
            }
        }
        assert_eq!(report.worst(), CheckStatus::Fail);
    }

    #[test]
    fn a_pending_migration_warns_rather_than_failing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let loaded = Ok(Loaded {
            config: Config::default(),
            source: ConfigSource::File,
            pending_migration: Some((0, 1)),
        });
        let report = environment_report(&Paths::rooted_at(dir.path()), &loaded, &facts());
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "configuration")
            .expect("check");
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(
            check
                .next_action
                .as_ref()
                .expect("action")
                .contains("config migrate")
        );
    }

    #[test]
    fn non_terminal_invocation_skips_rather_than_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let piped = TerminalFacts {
            is_terminal: false,
            size: None,
            ..facts()
        };
        let report = environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &piped);
        assert_eq!(report.worst(), CheckStatus::Skipped);
        let (_, _, fail, skipped) = report.counts();
        assert_eq!(fail, 0, "piping output is not a failure");
        assert!(skipped >= 2);
    }

    #[test]
    fn plain_output_shows_status_labels_and_a_summary_with_counts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let report = environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &facts());
        let text = report.render_plain();
        assert!(text.contains("OK      product"), "{text}");
        assert!(text.contains("Summary: "), "{text}");
        assert!(text.contains("failure(s)"), "{text}");
    }

    #[test]
    fn json_output_carries_the_same_checks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let report = environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &facts());
        let json = report.to_json();
        assert_eq!(
            json["checks"].as_array().expect("array").len(),
            report.checks.len()
        );
        assert_eq!(json["version"], crate::branding::VERSION);
    }
}
