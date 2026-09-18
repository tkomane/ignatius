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
