//! Focused tests for the transient paste notices on the editor footer rail.

use super::*;
use crate::app::model::PasteNotice;
use crate::config::ThemeChoice;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::theme::Theme;

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
