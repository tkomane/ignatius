use super::render_palette;
use crate::app::Action;
use crate::app::palette::{Palette, PaletteCommand, PaletteEntry};
use crate::config::{Density, ThemeChoice};
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

const AREA: Rect = Rect::new(0, 0, 100, 24);

fn palette() -> Palette {
    Palette::new(vec![
        PaletteEntry {
            group: "Session",
            label: "Switch theme".into(),
            detail: "Cycle theme for this session".into(),
            command: PaletteCommand::Run(Action::SwitchTheme),
        },
        PaletteEntry {
            group: "Editor",
            label: "Format buffer".into(),
            detail: "Format without running SQL".into(),
            command: PaletteCommand::Run(Action::FormatBuffer),
        },
    ])
}

fn render(presentation: &Presentation, selected: usize) -> Buffer {
    let mut palette = palette();
    palette.selected = selected;
    let mut buf = Buffer::empty(AREA);
    render_palette(&palette, presentation, AREA, &mut buf);
    buf
}

fn cell(buf: &Buffer, word: &str) -> (u16, u16) {
    for y in 0..AREA.height {
        let mut text = String::new();
        let mut starts = Vec::new();
        for x in 0..AREA.width {
            starts.push((text.len(), x));
            text.push_str(buf[(x, y)].symbol());
        }
        if let Some(byte) = text.find(word) {
            return (
                starts.iter().find(|(start, _)| *start == byte).unwrap().1,
                y,
            );
        }
    }
    panic!("missing {word}: {}", buffer_to_string(buf));
}

#[test]
fn selected_high_contrast_group_and_detail_use_selection_ink_after_muted() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::HighContrast, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let buf = render(&p, 0);
    let selection = p.theme.style(Token::Selection);
    assert_eq!(selection.fg, Some(Color::Rgb(0, 0, 0)));
    assert_eq!(selection.bg, Some(Color::Rgb(255, 255, 255)));
    let label = &buf[cell(&buf, "Switch theme")];
    assert_eq!(
        label.fg,
        Color::Rgb(0, 0, 0),
        "label is the positive control"
    );
    assert_eq!(label.bg, Color::Rgb(255, 255, 255));
    let failures: Vec<_> = ["Session", "Cycle theme"]
        .into_iter()
        .filter_map(|word| {
            let actual = &buf[cell(&buf, word)];
            assert_eq!(actual.bg, Color::Rgb(255, 255, 255));
            assert!(actual.modifier.contains(Modifier::DIM));
            (actual.fg != Color::Rgb(0, 0, 0)).then_some((word, actual.fg, actual.bg))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "selected group/detail ink lost: {failures:?}"
    );
}

#[test]
fn selected_palette_spans_keep_semantics_and_unselected_styles_at_every_builtin_depth() {
    for choice in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
    ] {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Indexed256,
            ColorDepth::Basic16,
            ColorDepth::None,
        ] {
            for color in [true, false] {
                for glyphs in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
                    for density in [Density::Comfortable, Density::Compact] {
                        let p = Presentation::new(
                            Theme::new(choice, color).with_depth(depth),
                            Glyphs::new(glyphs),
                            true,
                        )
                        .with_density(density);
                        let selected = render(&p, 0);
                        let ordinary = render(&p, 1);
                        assert_eq!(buffer_to_string(&selected), buffer_to_string(&ordinary));
                        let selection = p.theme.style(Token::Selection);
                        for (word, token) in [
                            ("Session", Token::Muted),
                            ("Switch theme", Token::Text),
                            ("Cycle theme", Token::Muted),
                        ] {
                            let xy = cell(&selected, word);
                            assert_eq!(xy, cell(&ordinary, word));
                            let semantic = p.theme.style(token);
                            let old = &ordinary[xy];
                            let actual = &selected[xy];
                            assert_eq!(old.fg, semantic.fg.unwrap_or(Color::Reset));
                            assert!(old.modifier.contains(semantic.add_modifier));
                            assert!(!old.modifier.contains(Modifier::REVERSED));
                            let expected_ink = if token == Token::Text {
                                selection.fg.unwrap_or(Color::Reset)
                            } else {
                                selection.fg.unwrap_or(old.fg)
                            };
                            assert_eq!(actual.fg, expected_ink, "{choice:?}/{depth:?}/{word}");
                            assert_eq!(actual.bg, selection.bg.unwrap_or(old.bg));
                            assert_eq!(actual.modifier, old.modifier | selection.add_modifier);
                        }
                        if !color || depth == ColorDepth::None {
                            assert!(
                                selected
                                    .content
                                    .iter()
                                    .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
                            );
                        }
                        if depth == ColorDepth::Basic16 {
                            assert!(selected.content.iter().all(|cell| cell.bg == Color::Reset));
                        }
                    }
                }
            }
        }
    }
}
