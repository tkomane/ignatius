//! Focused tests for the transient paste notices on the editor footer rail.

use super::*;
use crate::app::model::PasteNotice;
use crate::config::ThemeChoice;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::theme::Theme;
use ratatui::style::Modifier;

fn model_with(notice: PasteNotice) -> Model {
    let mut model = Model::new(120);
    model.paste_notice = Some(notice);
    model
}

fn presentation(theme: ThemeChoice, color: bool, tier: GlyphTier) -> Presentation {
    Presentation::new(Theme::new(theme, color), Glyphs::new(tier), false)
}

#[test]
fn density_footer_uses_render_override_caps_remaps_and_complete_pairs() {
    use crate::app::{Action, model::Focus};
    use crate::config::{Density, schema::KeySpec};
    use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
    use crate::query::value::Cell;
    use std::time::Duration;
    let mut model = Model::new(100);
    model.focus = Focus::Results;
    let mut set = ResultSet::new(vec!["value".into()], 100);
    set.push(vec![Cell::Text("data".into())]);
    model.last_execution = Some(Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: None,
            elapsed: Duration::ZERO,
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::ZERO,
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    let keymap = Keymap::from_config(&std::collections::BTreeMap::from([
        ("narrow-column".into(), KeySpec::One("alt+f10".into())),
        ("open-result-grid".into(), KeySpec::One("alt+f11".into())),
    ]))
    .unwrap();
    let draw = |model: &Model, p: &Presentation, width: u16| {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        render_footer(model, &keymap, p, area, &mut buffer);
        crate::ui::layout::buffer_to_string(&buffer)
    };
    for density in [Density::Comfortable, Density::Compact] {
        model.presentation =
            model
                .presentation
                .clone()
                .with_density(if density == Density::Comfortable {
                    Density::Compact
                } else {
                    Density::Comfortable
                });
        let p = presentation(ThemeChoice::Dark, false, GlyphTier::Ascii).with_density(density);
        let actions = crate::app::discovery::hint_actions_for(&model, &p);
        assert_eq!(
            actions.len(),
            if density == Density::Comfortable {
                6
            } else {
                4
            }
        );
        let wide = draw(&model, &p, 400);
        for action in &actions {
            let (key, label) = keymap.contextual_hint(action).unwrap();
            assert!(wide.contains(&format!("{key} {label}")), "{wide}");
        }
        assert!(!wide.contains("Widen column"));
        if density == Density::Comfortable {
            assert!(
                wide.contains("Alt+F10 Narrow column") && wide.contains("Alt+F11 Grid controls")
            );
        } else {
            assert!(!wide.contains("Alt+F10") && !wide.contains("Alt+F11"));
        }
        for width in (20..150).step_by(7) {
            let line = draw(&model, &p, width);
            for action in &actions {
                let (key, label) = keymap.contextual_hint(action).unwrap();
                if line.contains(&key) || line.contains(label) {
                    assert!(
                        line.contains(&format!("{key} {label}")),
                        "partial pair at {width}: {line}"
                    );
                }
            }
        }
        assert!(actions.contains(&Action::CopyValue));
    }
}

#[test]
fn split_feedback_keeps_its_meaning_when_transaction_facts_fill_the_footer() {
    use crate::app::model::LayoutNotice;
    use crate::query::result::TransactionState;
    for (color, tier) in [(false, GlyphTier::Ascii), (true, GlyphTier::Nerd)] {
        for (notice, wording) in [
            (LayoutNotice::SinglePane, "Already single-pane."),
            (LayoutNotice::SafetyDetails, "Zoom Results for details."),
        ] {
            let mut model = Model::new(100);
            model.size = (40, 8);
            model.transaction = TransactionState::Unknown;
            model.layout_notice = Some(notice);
            model.editor.set_text("SELECT 1;");
            let text = crate::ui::layout::render_to_string(
                &model,
                &Keymap::new(),
                &presentation(ThemeChoice::Dark, color, tier),
                40,
                8,
            );
            assert!(
                text.contains(wording),
                "the explanation was clipped: {text}"
            );
            assert!(
                text.contains("SELECT 1;"),
                "pane content was covered: {text}"
            );
            let footer = text.lines().last().unwrap();
            assert!(footer.contains("Ready") && footer.contains("Transaction state unknown"));
            let baseline = crate::ui::layout::pane_geometry(&model, Rect::new(0, 0, 40, 8));
            model.layout_notice = None;
            assert_eq!(
                crate::ui::layout::pane_geometry(&model, Rect::new(0, 0, 40, 8)),
                baseline
            );
            let cleared = crate::ui::layout::render_to_string(
                &model,
                &Keymap::new(),
                &presentation(ThemeChoice::Dark, color, tier),
                40,
                8,
            );
            assert!(!cleared.contains(wording));
        }
    }
}

#[test]
fn split_footer_feedback_fits_minimum_size_and_preserves_transaction_facts() {
    use crate::app::{Action, Message, update};
    use crate::query::result::TransactionState;
    for (color, tier) in [(false, GlyphTier::Ascii), (true, GlyphTier::Nerd)] {
        for state in [
            TransactionState::Autocommit,
            TransactionState::Failed,
            TransactionState::Unknown,
        ] {
            let mut model = Model::new(100);
            model.size = (40, 8);
            model.transaction = state;
            update(&mut model, Message::Action(Action::ZoomPane));
            let text = crate::ui::layout::render_to_string(
                &model,
                &Keymap::new(),
                &presentation(ThemeChoice::Dark, color, tier),
                40,
                8,
            );
            let footer = text.lines().last().unwrap();
            assert!(footer.contains("Ready"), "{footer}");
            assert!(footer.contains(state.label()), "{footer}");
            if state != TransactionState::Unknown {
                assert!(footer.to_lowercase().contains("single-pane"), "{footer}");
            }
            if !color && state == TransactionState::Autocommit {
                assert!(footer.contains("Already single-pane."), "{footer}");
            }
            if !color && state == TransactionState::Failed {
                assert!(footer.contains("Single-pane"), "{footer}");
            }
        }
    }
}

#[test]
fn split_footer_safety_refusal_uses_the_first_wording_that_fits() {
    use crate::app::{Action, Message, update};
    use crate::diagnostics::{Diagnostic, DiagnosticKind};
    for (color, tier) in [(false, GlyphTier::Ascii), (true, GlyphTier::Nerd)] {
        let mut model = Model::new(100);
        model.size = (120, 40);
        model.error = Some(Diagnostic::new(
            DiagnosticKind::Query,
            "Synthetic error",
            "a synthetic operation",
        ));
        update(&mut model, Message::Action(Action::ZoomPane));
        for (width, expected) in [
            (40, "Zoom Results"),
            (45, "Zoom Results for details."),
            (
                79,
                "Zoom Results to keep error and recovery details visible.",
            ),
        ] {
            update(&mut model, Message::Resized(width, 8));
            let text = crate::ui::layout::render_to_string(
                &model,
                &Keymap::new(),
                &presentation(ThemeChoice::Dark, color, tier),
                width,
                8,
            );
            let footer = text.lines().last().unwrap();
            assert!(
                footer.contains("Ready") && footer.contains("Autocommit"),
                "{footer}"
            );
            if !color {
                assert!(footer.contains(expected), "{width}: {footer}");
            }
            assert!(footer.contains("Zoom Results"), "{footer}");
        }
    }
}

#[test]
fn paste_refusal_renders_its_exact_wording() {
    let model = model_with(PasteNotice::Refused);
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(ThemeChoice::Dark, true, GlyphTier::Nerd),
        120,
        30,
    );
    assert!(
        text.contains("Paste refused: the text is over the 1 MiB limit."),
        "{text}"
    );
}

#[test]
fn paste_ignored_renders_its_exact_wording() {
    let model = model_with(PasteNotice::Ignored);
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(ThemeChoice::Dark, true, GlyphTier::Nerd),
        120,
        30,
    );
    assert!(
        text.contains("Paste ignored: this surface does not accept pasted text."),
        "{text}"
    );
}

#[test]
fn a_paste_notice_appears_in_a_narrow_layout() {
    let model = model_with(PasteNotice::Refused);
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(ThemeChoice::Dark, true, GlyphTier::Nerd),
        60,
        20,
    );
    assert!(
        text.contains("Paste refused"),
        "the narrow footer lost the notice: {text}"
    );
}

#[test]
fn a_paste_notice_stands_without_colour_and_in_ascii() {
    let model = model_with(PasteNotice::Ignored);
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
        60,
        20,
    );
    assert!(text.contains("Paste ignored"), "{text}");
    assert!(text.is_ascii(), "ASCII output carried a glyph: {text:?}");
}

#[test]
fn the_paste_notice_takes_the_rail_over_an_older_format_notice() {
    let mut model = model_with(PasteNotice::Refused);
    model.format_notice = Some(crate::app::model::FormatNotice::Empty);
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(ThemeChoice::Dark, true, GlyphTier::Nerd),
        120,
        30,
    );
    assert!(
        text.contains("Paste refused: the text is over the 1 MiB limit."),
        "{text}"
    );
    assert!(
        !text.contains("no SQL to format") && !text.contains("There is no SQL to format."),
        "the older format notice hid the paste outcome: {text}"
    );
}

#[test]
fn a_paste_notice_never_carries_sql_or_a_credential() {
    for notice in [PasteNotice::Refused, PasteNotice::Ignored] {
        let mut model = Model::new(120);
        model.editor =
            crate::app::editor::Editor::with_text("SELECT secret_column FROM secret_table;");
        model.password_prompt = Some(crate::app::model::PasswordPrompt::new(
            "orders",
            "the server asked for a password",
        ));
        if let Some(prompt) = model.password_prompt.as_mut() {
            for character in "hunter2".chars() {
                prompt.push(character);
            }
        }
        model.paste_notice = Some(notice.clone());

        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);
        render_footer(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Unicode),
            area,
            &mut buf,
        );
        let text = crate::ui::layout::buffer_to_string(&buf);
        assert!(
            text.contains(&notice.message()),
            "the notice wording was not rendered: {text}"
        );
        assert!(!text.contains("secret_column"), "{text}");
        assert!(!text.contains("secret_table"), "{text}");
        assert!(!text.contains("hunter2"), "a credential was echoed: {text}");
    }
}

#[test]
fn retained_data_footer_badge_is_complete_or_deferred_and_never_clears_facts() {
    use crate::query::result::{RetentionLoss, RetentionLossSummary, RetentionReason};
    let mut model = Model::new(100);
    model.retained_data_loss = RetentionLossSummary {
        limited_sets: 12,
        first: Some(RetentionLoss {
            ordinal: 2,
            reason: RetentionReason::RetainedResultLimit,
            retained: 1,
            received: 3,
            source_columns: 2,
        }),
    };
    let before = model.retained_data_loss;
    let p = presentation(ThemeChoice::Dark, false, GlyphTier::Ascii);
    let draw = |m: &Model, width| {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        render_footer(m, &Keymap::new(), &p, area, &mut buffer);
        crate::ui::layout::buffer_to_string(&buffer)
    };
    let wide = draw(&model, 120);
    assert!(wide.contains("Limited sets: 12"), "{wide}");
    for width in 0..55 {
        let text = draw(&model, width);
        assert!(
            !text.contains("Limited") || text.contains("Limited sets: 12"),
            "{width}: {text}"
        );
    }
    model.help_open = true;
    assert!(!draw(&model, 120).contains("Limited sets"));
    assert_eq!(model.retained_data_loss, before);
}

#[test]
fn us8_footer_hint_cap_and_ready_status() {
    use crate::app::model::Focus;
    use crate::config::Density;

    let mut model = Model::new(100);
    model.focus = Focus::Results;
    let keymap = Keymap::new();

    // When idle, footer must render "Ready" on the left
    let p_comf = presentation(ThemeChoice::Dark, true, GlyphTier::Unicode)
        .with_density(Density::Comfortable);
    let area = Rect::new(0, 0, 160, 1);
    let mut buf = Buffer::empty(area);
    render_footer(&model, &keymap, &p_comf, area, &mut buf);
    let text = crate::ui::layout::buffer_to_string(&buf);
    assert!(
        text.trim_start().starts_with("Ready") || text.contains("Ready"),
        "idle status must show Ready: {text}"
    );

    // Comfortable density caps hints at 6
    let comf_actions = crate::app::discovery::hint_actions_for(&model, &p_comf);
    assert!(
        comf_actions.len() <= 6,
        "Comfortable hint cap is 6: {}",
        comf_actions.len()
    );

    // Compact density caps hints at 4
    let p_comp =
        presentation(ThemeChoice::Dark, true, GlyphTier::Unicode).with_density(Density::Compact);
    let comp_actions = crate::app::discovery::hint_actions_for(&model, &p_comp);
    assert!(
        comp_actions.len() <= 4,
        "Compact hint cap is 4: {}",
        comp_actions.len()
    );
}

#[test]
fn us8_footer_narrow_truncation_drops_whole_hints() {
    use crate::app::model::Focus;
    use crate::config::Density;

    let mut model = Model::new(100);
    model.focus = Focus::Editor;
    let keymap = Keymap::new();
    let p =
        presentation(ThemeChoice::Dark, false, GlyphTier::Ascii).with_density(Density::Comfortable);
    let actions = crate::app::discovery::hint_actions_for(&model, &p);

    let draw = |width: u16| {
        let area = Rect::new(0, 0, width, 1);
        let mut buffer = Buffer::empty(area);
        render_footer(&model, &keymap, &p, area, &mut buffer);
        crate::ui::layout::buffer_to_string(&buffer)
    };

    // For various narrow widths, verify no key/label pair is half-rendered
    for width in 15..120 {
        let line = draw(width);
        for action in &actions {
            if let Some((key, label)) = keymap.contextual_hint(action)
                && (line.contains(&key) || line.contains(label))
            {
                assert!(
                    line.contains(&format!("{key} {label}")),
                    "partial hint pair at width {width}: key='{key}', label='{label}' in line: {line}"
                );
            }
        }
    }
}

#[test]
fn footer_renders_surface_alt_statusbar() {
    let model = Model::new(100);
    let keymap = Keymap::new();
    let p = presentation(ThemeChoice::Dark, true, GlyphTier::Unicode);
    let area = Rect::new(0, 0, 80, 1);
    let mut buf = Buffer::empty(area);
    render_footer(&model, &keymap, &p, area, &mut buf);

    let surface_alt_bg = p.theme.surface(Token::SurfaceAlt).bg.unwrap();
    // Verify trailing cell carries SurfaceAlt background
    assert_eq!(buf[(79, 0)].bg, surface_alt_bg);
}

#[test]
fn footer_uses_tonal_key_pills_and_ascii_or_unicode_hint_separators() {
    use crate::app::model::{ConnectionState, Focus};
    let mut model = Model::new(120);
    model.connection = ConnectionState::Connected(Box::new(crate::postgres::SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "42".into(),
        tls: crate::postgres::TlsState::Disabled,
        read_only: false,
        search_path: "public".into(),
        environment: crate::connection::Environment::Local,
    }));
    model.focus = Focus::Editor;
    model.editor.set_text("SELECT 1;");
    let keymap = Keymap::new();

    // Unicode + Material theme: uses middle dot separator and Focus pill styling
    let p_unicode = presentation(ThemeChoice::Material, true, GlyphTier::Unicode);
    let area = Rect::new(0, 0, 140, 1);
    let mut buf_unicode = Buffer::empty(area);
    render_footer(&model, &keymap, &p_unicode, area, &mut buf_unicode);
    let text_unicode = crate::ui::layout::buffer_to_string(&buf_unicode);
    assert!(
        text_unicode.contains(" \u{00b7} "),
        "Unicode footer hints should use middle-dot separator: {text_unicode}"
    );
    let run_pos = text_unicode
        .find("Ctrl+R")
        .unwrap_or_else(|| panic!("Ctrl+R hint missing: {text_unicode}"));
    let run_col = text_unicode[..run_pos].chars().count() as u16;
    let expected_pill = p_unicode.theme.keycap(Token::Focus);
    assert_eq!(buf_unicode[(run_col, 0)].fg, expected_pill.fg.unwrap());
    assert_eq!(buf_unicode[(run_col, 0)].bg, expected_pill.bg.unwrap());
    assert!(buf_unicode[(run_col, 0)].modifier.contains(Modifier::BOLD));
    assert!(
        text_unicode.contains("\u{2502} Autocommit \u{2502}"),
        "Unicode footer should use vertical dividers between status, tx, and search path: {text_unicode}"
    );
    assert!(
        run_col > 60,
        "wide footer should right-align shortcut hints, found Ctrl+R at col {run_col}: {text_unicode}"
    );

    // ASCII + NO_COLOR: uses plain ASCII pipe separator, zero ANSI/Unicode, bold keys
    let p_ascii = presentation(ThemeChoice::Material, false, GlyphTier::Ascii);
    let mut buf_ascii = Buffer::empty(area);
    render_footer(&model, &keymap, &p_ascii, area, &mut buf_ascii);
    let text_ascii = crate::ui::layout::buffer_to_string(&buf_ascii);
    assert!(
        text_ascii.is_ascii(),
        "ASCII footer must stay strictly ASCII: {text_ascii:?}"
    );
    assert!(
        text_ascii.contains(" | "),
        "ASCII footer hints should use pipe separator: {text_ascii}"
    );
    let run_ascii_pos = text_ascii.find("Ctrl+R").expect("Ctrl+R hint in ASCII");
    let run_ascii_col = text_ascii[..run_ascii_pos].chars().count() as u16;
    assert_eq!(
        buf_ascii[(run_ascii_col, 0)].fg,
        ratatui::style::Color::Reset
    );
    assert!(
        buf_ascii[(run_ascii_col, 0)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn compact_footer_at_forty_by_eight_keeps_divider_and_non_color_parity() {
    use crate::query::result::TransactionState;
    use crate::ui::theme::ColorDepth;
    use ratatui::style::Color;

    let mut model = Model::new(100);
    model.size = (40, 8);
    model.transaction = TransactionState::Open;
    let keymap = Keymap::new();
    let area = Rect::new(0, 0, 40, 1);

    for (tier, color, depth, divider) in [
        (GlyphTier::Unicode, true, ColorDepth::TrueColor, "\u{2502}"),
        (GlyphTier::Ascii, true, ColorDepth::Basic16, "|"),
        (GlyphTier::Ascii, false, ColorDepth::None, "|"),
    ] {
        let pres = Presentation::new(
            Theme::new(ThemeChoice::Dark, color).with_depth(depth),
            Glyphs::new(tier),
            false,
        );
        let mut buf = Buffer::empty(area);
        render_footer(&model, &keymap, &pres, area, &mut buf);
        let line = crate::ui::layout::buffer_to_string(&buf);
        assert!(line.contains("Ready"), "{depth:?}: {line}");
        assert!(line.contains("In transaction"), "{depth:?}: {line}");
        assert!(line.contains(divider), "{depth:?}: {line}");
        if tier == GlyphTier::Ascii {
            assert!(line.is_ascii(), "{depth:?}: {line:?}");
            assert_eq!(buf[(0, 0)].bg, Color::Reset);
        }
    }
}

fn connected_editor_model() -> Model {
    let mut model = Model::new(100);
    model.connection =
        crate::app::model::ConnectionState::Connected(Box::new(crate::postgres::SessionInfo {
            target: "app@localhost:5432/orders".into(),
            database: "orders".into(),
            user: "app".into(),
            server_version: "18.0".into(),
            backend_pid: "1".into(),
            search_path: "public".into(),
            read_only: false,
            tls: crate::postgres::TlsState::Disabled,
            environment: crate::connection::Environment::Local,
        }));
    model.focus = crate::app::model::Focus::Editor;
    model.editor.set_text("SELECT 1;");
    model
}

fn footer_text(model: &Model, width: u16) -> String {
    let area = Rect::new(0, 0, width, 1);
    let mut buffer = Buffer::empty(area);
    render_footer(
        model,
        &Keymap::new(),
        &presentation(ThemeChoice::Dark, true, GlyphTier::Unicode),
        area,
        &mut buffer,
    );
    crate::ui::layout::buffer_to_string(&buffer)
}

#[test]
fn an_open_overlay_replaces_workbench_hints_with_its_own_keys() {
    let workbench = connected_editor_model();
    let plain = footer_text(&workbench, 160);
    assert!(plain.contains("Ctrl+R Run"), "{plain}");

    let mut palette = workbench.clone();
    palette.palette = Some(crate::app::palette::Palette::new(Vec::new()));
    let text = footer_text(&palette, 160);
    assert!(text.contains("Enter Choose"), "{text}");
    assert!(text.contains("Up/Down Move"), "{text}");
    assert!(text.contains("Esc Close"), "{text}");
    assert!(
        !text.contains("Ctrl+R"),
        "a swallowed key is advertised: {text}"
    );
    assert!(
        !text.contains("Plan"),
        "a swallowed key is advertised: {text}"
    );
    // The status, transaction and search path segments stay.
    for segment in ["Ready", "public"] {
        assert!(text.contains(segment), "{segment}: {text}");
    }

    let mut help = workbench.clone();
    help.help_open = true;
    let text = footer_text(&help, 160);
    assert!(text.contains("Esc Close"), "{text}");
    assert!(!text.contains("Ctrl+R"), "{text}");

    // A confirmation offers only its way out; accepting is worded on the
    // surface beside what is being accepted.
    let mut confirming = workbench.clone();
    confirming.prefix_pending = true;
    let text = footer_text(&confirming, 160);
    assert!(text.contains("Esc Cancel"), "{text}");
    assert!(!text.contains("Enter"), "{text}");
    assert!(!text.contains("Ctrl+R"), "{text}");

    // A dashboard that is not in the pane-blocking list still owns input.
    let mut snippets = workbench;
    snippets.snippet_library = Some(crate::app::model::SnippetLibraryState::default());
    let text = footer_text(&snippets, 160);
    assert!(text.contains("Esc Close"), "{text}");
    assert!(!text.contains("Ctrl+R"), "{text}");
}

/// One open overlay of every family the footer maps, over a session with the
/// default search path inside an open transaction, so the status side of the
/// rail is as wide as it ordinarily gets.
fn overlay_families() -> Vec<(&'static str, Model)> {
    use crate::app::completion::CompletionMenu;
    use crate::query::completion::{Candidate, CandidateKind, CompletionResult, CompletionScope};
    let mut base = connected_editor_model();
    if let crate::app::model::ConnectionState::Connected(info) = &mut base.connection {
        info.search_path = "\"$user\", public".into();
    }
    base.transaction = crate::query::result::TransactionState::Open;
    let with = |change: &dyn Fn(&mut Model)| {
        let mut model = base.clone();
        change(&mut model);
        model
    };
    vec![
        (
            "password prompt",
            with(&|m| {
                m.password_prompt = Some(crate::app::model::PasswordPrompt::new(
                    "orders",
                    "the server asked",
                ));
            }),
        ),
        (
            "typed confirmation",
            with(&|m| {
                m.pending_run = Some(crate::app::model::PendingRun {
                    sql: "DROP TABLE orders;".into(),
                    impact: crate::query::Impact::Destructive,
                    typed: String::new(),
                    required: "orders".into(),
                    source: None,
                });
            }),
        ),
        ("chord prefix", with(&|m| m.prefix_pending = true)),
        ("connection details", with(&|m| m.connection_details = true)),
        (
            "palette",
            with(&|m| m.palette = Some(crate::app::palette::Palette::new(Vec::new()))),
        ),
        ("help", with(&|m| m.help_open = true)),
        (
            "completion",
            with(&|m| {
                m.completion.menu = Some(CompletionMenu::new(
                    CompletionResult {
                        replacement: 0..3,
                        prefix: "sel".into(),
                        scope: CompletionScope::Keywords,
                        candidates: vec![Candidate {
                            label: "SELECT".into(),
                            insert_text: "SELECT".into(),
                            kind: CandidateKind::Keyword,
                            schema: None,
                            source: None,
                            detail: None,
                        }],
                        matching_count: 1,
                        total_count: 1,
                    },
                    true,
                ));
            }),
        ),
        (
            "inspector",
            with(&|m| m.inspector = Some(crate::app::inspect::Inspector::new())),
        ),
        (
            "dashboard",
            with(&|m| {
                m.query_performance = Some(crate::app::model::QueryPerformanceState::default());
            }),
        ),
        (
            "snippet library",
            with(&|m| {
                m.snippet_library = Some(crate::app::model::SnippetLibraryState::default());
            }),
        ),
    ]
}

#[test]
fn every_pane_blocking_overlay_has_footer_keys_of_its_own() {
    for (name, model) in overlay_families() {
        assert!(crate::ui::widgets::overlay_blocks_panes(&model), "{name}");
        assert!(
            crate::app::discovery::hint_actions_for(&model, &model.presentation).is_empty(),
            "{name}: workbench hints while an overlay owns input"
        );
        let hints = crate::app::discovery::overlay_hint_actions(&model).expect("overlay keys");
        assert!(hints.len() <= crate::app::discovery::MAX_CONTEXTUAL_HINTS);
        assert!(
            hints
                .iter()
                .any(|(action, _)| *action == crate::app::Action::Dismiss),
            "{name}: no way out"
        );
        for (action, _) in hints {
            assert!(
                super::overlay_key(&Keymap::new(), &action).is_some(),
                "{name}: {action:?} has no key"
            );
        }
    }
}

#[test]
fn the_way_out_is_the_last_overlay_key_a_short_rail_gives_up() {
    for (name, model) in overlay_families() {
        for width in 40u16..=160 {
            let text = footer_text(&model, width);
            let shows_any = ["Choose", "Accept", "Move", "Close", "Cancel"]
                .iter()
                .any(|label| text.contains(label));
            if shows_any {
                assert!(
                    text.contains("Esc Close") || text.contains("Esc Cancel"),
                    "{name} at {width}: overlay keys without the way out: {text:?}"
                );
            }
            assert!(!text.contains("Ctrl+R"), "{name} at {width}: {text:?}");
            // Status and transaction always stay.
            assert!(text.contains("Ready"), "{name} at {width}: {text:?}");
        }
        // At the 80-column minimum the way out is always offered.
        let text = footer_text(&model, 80);
        assert!(
            text.contains("Esc Close") || text.contains("Esc Cancel"),
            "{name} at 80: {text:?}"
        );
    }

    // A wide rail still draws the palette's keys in their natural order.
    let (_, palette) = overlay_families()
        .into_iter()
        .find(|(name, _)| *name == "palette")
        .expect("palette family");
    let text = footer_text(&palette, 160);
    let choose = text.find("Enter Choose").expect("Enter Choose");
    let moving = text.find("Up/Down Move").expect("Up/Down Move");
    let close = text.find("Esc Close").expect("Esc Close");
    assert!(choose < moving && moving < close, "{text}");
}

#[test]
fn an_idle_footer_does_not_wear_a_success_mark() {
    let mut model = connected_editor_model();
    model.error = Some(crate::diagnostics::Diagnostic::new(
        crate::diagnostics::DiagnosticKind::Query,
        "syntax error",
        "executing buffer",
    ));
    let text = footer_text(&model, 120);
    assert!(text.contains("Ready"), "{text}");
    assert!(
        !text.contains('\u{2713}'),
        "a check mark beside Ready: {text}"
    );
    assert!(text.contains("\u{25cb} Ready"), "{text}");
}

#[test]
fn a_running_benchmark_keeps_quit_on_a_short_rail() {
    let mut model = connected_editor_model();
    model.connection =
        crate::app::model::ConnectionState::Connected(Box::new(crate::postgres::SessionInfo {
            search_path: "\"$user\", public, reporting, analytics".into(),
            ..model.connection.info().expect("connected").clone()
        }));
    model.query_bench = Some(crate::app::model::QueryBenchState {
        loading: true,
        ..Default::default()
    });
    for width in 40u16..=160 {
        let text = footer_text(&model, width);
        assert!(text.contains("Quit"), "at {width}: {text:?}");
        assert!(!text.contains("Esc Close"), "at {width}: {text:?}");
        if width >= 80 {
            assert!(text.contains("Ctrl+C Cancel"), "at {width}: {text:?}");
        }
    }
}

#[test]
fn notice_settle_tints_only_applied_format_notice_and_returns_to_resting() {
    use crate::ui::layout::buffer_to_string;
    use crate::ui::theme::{Rgb, ThemeDefinition, ThemeSelection, ValidatedThemes};
    use std::time::Duration;

    let mut model = connected_editor_model();
    model.format_notice = Some(crate::app::model::FormatNotice::Applied {
        before_lines: 1,
        after_lines: 2,
    });
    let area = Rect::new(0, 0, 120, 1);
    let pres = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let keymap = Keymap::new();

    let mut resting = Buffer::empty(area);
    render_footer(&model, &keymap, &pres, area, &mut resting);

    model.motion.enable();
    model.motion.begin_notice_settle();
    let mut at_zero = Buffer::empty(area);
    render_footer(&model, &keymap, &pres, area, &mut at_zero);

    let msg_x = (0..area.width)
        .find(|&x| resting[(x, 0)].symbol() == "F")
        .expect("Formatted SQL notice start");
    assert_eq!(
        Some(at_zero[(msg_x, 0)].fg),
        pres.theme.color(Token::Success)
    );
    assert_eq!(buffer_to_string(&at_zero), buffer_to_string(&resting));

    let mid = Duration::from_millis(240);
    model.motion.advance(mid);
    let mut at_mid = Buffer::empty(area);
    render_footer(&model, &keymap, &pres, area, &mut at_mid);
    let progress = model
        .motion()
        .notice_settle()
        .expect("active notice settle");
    assert_eq!(
        Some(at_mid[(msg_x, 0)].fg),
        pres.theme.mix(Token::Success, Token::Info, progress)
    );

    model.motion.advance(Duration::from_millis(300));
    let mut settled = Buffer::empty(area);
    render_footer(&model, &keymap, &pres, area, &mut settled);
    assert_eq!(settled, resting, "settled footer matches resting frame");

    // Refused and AlreadyFormatted notices never tint toward Success.
    for static_notice in [
        crate::app::model::FormatNotice::AlreadyFormatted,
        crate::app::model::FormatNotice::Refused {
            message: "unclosed".to_owned(),
        },
    ] {
        let mut static_model = connected_editor_model();
        static_model.format_notice = Some(static_notice);
        let mut static_resting = Buffer::empty(area);
        render_footer(&static_model, &keymap, &pres, area, &mut static_resting);
        static_model.motion.enable();
        static_model.motion.begin_notice_settle();
        let mut static_active = Buffer::empty(area);
        render_footer(&static_model, &keymap, &pres, area, &mut static_active);
        assert_eq!(static_active, static_resting);
    }

    // Low-contrast custom Success on SurfaceAlt keeps resting ink.
    let faint = Rgb(0x30, 0x8c, 0x50);
    let definition = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Success, faint);
    let custom_theme = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition))
        .expect("custom theme passes 3:1 gate")
        .selected_theme(true, crate::ui::theme::ColorDepth::TrueColor);
    assert!(faint.contrast(custom_theme.rgb(Token::SurfaceAlt)) < 4.5);
    let custom_pres = Presentation::new(custom_theme, Glyphs::new(GlyphTier::Unicode), false);
    let mut custom_resting = Buffer::empty(area);
    let mut custom_model = connected_editor_model();
    custom_model.format_notice = Some(crate::app::model::FormatNotice::Applied {
        before_lines: 1,
        after_lines: 2,
    });
    render_footer(
        &custom_model,
        &keymap,
        &custom_pres,
        area,
        &mut custom_resting,
    );
    custom_model.motion.enable();
    custom_model.motion.begin_notice_settle();
    let mut custom_zero = Buffer::empty(area);
    render_footer(&custom_model, &keymap, &custom_pres, area, &mut custom_zero);
    assert_eq!(custom_zero, custom_resting);
}

#[test]
fn buffer_notice_shows_pinned_close_refusal_and_truncates_narrowly() {
    let mut model = connected_editor_model();
    model.buffer_notice = Some("Buffer is pinned; unpin it before closing.".to_owned());
    let keymap = Keymap::new();
    let pres = presentation(ThemeChoice::Dark, true, GlyphTier::Unicode);

    let wide = Rect::new(0, 0, 100, 1);
    let mut buf = Buffer::empty(wide);
    assert!(render_footer(&model, &keymap, &pres, wide, &mut buf));
    let rendered: String = (0..wide.width).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(
        rendered.contains("Buffer is pinned; unpin it before closing."),
        "{rendered}"
    );

    let narrow = Rect::new(0, 0, 42, 1);
    let mut narrow_buf = Buffer::empty(narrow);
    assert!(render_footer(
        &model,
        &keymap,
        &pres,
        narrow,
        &mut narrow_buf
    ));
    let narrow_text: String = (0..narrow.width)
        .map(|x| narrow_buf[(x, 0)].symbol())
        .collect();
    assert!(narrow_text.contains("Buffer pinned"), "{narrow_text}");
}
