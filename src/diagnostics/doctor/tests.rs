use super::*;
use crate::config::store::Loaded;
use crate::config::{Config, Paths};

fn facts() -> TerminalFacts {
    TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: Some("WarpTerminal".into()),
        no_color: false,
        colorterm: Some("truecolor".into()),
        size: Some((120, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
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

fn colour_depth_check(report: &Report) -> &Check {
    report
        .checks
        .iter()
        .find(|c| c.name == "colour depth")
        .expect("a colour depth check")
}

fn report_with_depth(
    depth: crate::ui::theme::ColorDepth,
    source: crate::ui::theme::DepthSource,
) -> Report {
    let dir = tempfile::tempdir().expect("temp dir");
    let terminal = TerminalFacts {
        color_depth: depth,
        color_depth_source: source,
        ..facts()
    };
    // The temp dir is leaked into the report path only; keep it alive for the
    // duration of the call by building the report before it is dropped.
    environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &terminal)
}

#[test]
fn plain_output_names_the_colour_depth_and_its_source() {
    let report = report_with_depth(
        crate::ui::theme::ColorDepth::Indexed256,
        crate::ui::theme::DepthSource::Term,
    );
    let check = colour_depth_check(&report);
    assert_eq!(check.status, CheckStatus::Ok);
    assert_eq!(check.detail, "256, source TERM");

    // The exact rendered line, aligned by the report's column width.
    let expected = format!("{:<7} {:<16}  {}", "OK", "colour depth", "256, source TERM");
    let text = report.render_plain();
    let line = text
        .lines()
        .find(|line| line.contains("colour depth"))
        .expect("a colour depth line");
    assert_eq!(line, expected, "{text}");
}

#[test]
fn json_output_names_the_colour_depth_and_its_source() {
    let report = report_with_depth(
        crate::ui::theme::ColorDepth::Basic16,
        crate::ui::theme::DepthSource::Configuration,
    );
    let json = report.to_json();
    let entry = json["checks"]
        .as_array()
        .expect("array")
        .iter()
        .find(|check| check["name"] == "colour depth")
        .expect("a colour depth check");
    assert_eq!(entry["detail"], "16, source configuration");
    assert_eq!(entry["status"], "OK");
}

#[test]
fn every_depth_and_source_label_is_reported_verbatim() {
    use crate::ui::theme::{ColorDepth, DepthSource};
    let dir = tempfile::tempdir().expect("temp dir");
    for (depth, depth_word) in [
        (ColorDepth::TrueColor, "truecolor"),
        (ColorDepth::Indexed256, "256"),
        (ColorDepth::Basic16, "16"),
        (ColorDepth::None, "none"),
    ] {
        for (source, source_word) in [
            (DepthSource::Flag, "flag"),
            (DepthSource::Configuration, "configuration"),
            (DepthSource::Colorterm, "COLORTERM"),
            (DepthSource::Term, "TERM"),
            (DepthSource::Default, "default"),
            (DepthSource::ForcedOff, "forced off"),
        ] {
            let terminal = TerminalFacts {
                color_depth: depth,
                color_depth_source: source,
                ..facts()
            };
            let report = environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &terminal);
            assert_eq!(
                colour_depth_check(&report).detail,
                format!("{depth_word}, source {source_word}")
            );
        }
    }
}

#[test]
fn the_reported_depth_comes_from_the_facts_not_the_environment() {
    // The facts were already resolved by the runtime: a forced-off
    // environment with a resolved indexed depth still reports the latter.
    let terminal = TerminalFacts {
        no_color: true,
        term: Some("dumb".into()),
        colorterm: None,
        color_depth: crate::ui::theme::ColorDepth::Indexed256,
        color_depth_source: crate::ui::theme::DepthSource::Term,
        ..facts()
    };
    let dir = tempfile::tempdir().expect("temp dir");
    let report = environment_report(&Paths::rooted_at(dir.path()), &ok_config(), &terminal);
    assert_eq!(colour_depth_check(&report).detail, "256, source TERM");
}
