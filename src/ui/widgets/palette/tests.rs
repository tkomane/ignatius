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

#[test]
fn palette_box_occupies_eighty_percent_height_with_top_input_line() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let palette = palette();

    for (screen_h, expected_h) in [(20, 16), (25, 20), (30, 24), (40, 32)] {
        let area = Rect::new(0, 0, 100, screen_h);
        let p_box = super::palette_box(&palette, &p, area);
        assert_eq!(
            p_box.height, expected_h,
            "palette height should be 80% of screen height ({screen_h})"
        );
        // Anchor is Top: y is at top overlay margin
        assert_eq!(p_box.y, p.spacing().overlay_margin);
    }
}

#[test]
fn preview_bearing_purposes_render_sixty_percent_preview_pane_and_divider() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );

    // 1. Saved query
    let saved_palette = Palette::over_saved_queries(vec![PaletteEntry {
        label: "active_users".into(),
        detail: "SELECT * FROM users WHERE active = true;".into(),
        group: "Saved",
        command: PaletteCommand::Open("active_users".into()),
    }]);

    let mut buf = Buffer::empty(AREA);
    render_palette(&saved_palette, &p, AREA, &mut buf);
    let rendered = buffer_to_string(&buf);

    assert!(rendered.contains("active_users"));
    assert!(rendered.contains("Query: active_users"));
    assert!(rendered.contains("Origin: Saved"));
    assert!(rendered.contains("SQL:"));
    assert!(rendered.contains("SELECT * FROM users WHERE active = true;"));

    // Check divider existence
    let box_area = super::palette_box(&saved_palette, &p, AREA);
    let inner_width = box_area.width - 2;
    let preview_width = inner_width * 60 / 100;
    let list_width = inner_width - preview_width;
    let divider_x = box_area.x + 1 + list_width;
    let divider_y = box_area.y + 2; // below filter line
    assert_eq!(buf[(divider_x, divider_y)].symbol(), "\u{2502}");

    // 2. Connection profile
    let conn_palette = Palette::over_connections(vec![PaletteEntry {
        label: "staging".into(),
        detail: "staging.db:5432 / app_staging".into(),
        group: "NonProduction",
        command: PaletteCommand::ConnectProfile(Some("staging".into())),
    }]);
    let mut buf = Buffer::empty(AREA);
    render_palette(&conn_palette, &p, AREA, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("Profile: staging"));
    assert!(rendered.contains("Posture: NonProduction"));
    assert!(rendered.contains("Target: staging.db:5432 / app_staging"));

    // 3. Dependency
    let dep_palette = Palette::over_dependencies(vec![PaletteEntry {
        label: "customer_orders".into(),
        detail: "view on public.orders".into(),
        group: "Used by",
        command: PaletteCommand::Insert("customer_orders".into()),
    }]);
    let mut buf = Buffer::empty(AREA);
    render_palette(&dep_palette, &p, AREA, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("Direction: Used by"));
    assert!(rendered.contains("Object: customer_orders"));
    assert!(rendered.contains("Details: view on public.orders"));

    // 4. Definition (GoTo with object entry)
    let obj_palette = Palette::new(vec![PaletteEntry {
        label: "customers".into(),
        detail: "table public.customers".into(),
        group: "Objects",
        command: PaletteCommand::Insert("public.customers".into()),
    }]);
    let mut buf = Buffer::empty(AREA);
    render_palette(&obj_palette, &p, AREA, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("Kind: Objects"));
    assert!(rendered.contains("Identifier: public.customers"));
    assert!(rendered.contains("Summary: table public.customers"));
}

#[test]
fn non_preview_purposes_render_full_width_without_preview_pane_or_divider() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );

    // Commands palette
    let commands_palette = Palette::over_commands(
        vec![PaletteEntry {
            label: "Format buffer".into(),
            detail: "Format SQL without running".into(),
            group: "Editor",
            command: PaletteCommand::Run(Action::FormatBuffer),
        }],
        "Editor focus note",
    );
    let mut buf = Buffer::empty(AREA);
    render_palette(&commands_palette, &p, AREA, &mut buf);
    let rendered = buffer_to_string(&buf);

    assert!(rendered.contains("Format buffer"));
    assert!(rendered.contains("Format SQL without running"));
    // No preview headers
    assert!(!rendered.contains("Kind:"));
    assert!(!rendered.contains("Query:"));
    assert!(!rendered.contains("Profile:"));
    assert!(!rendered.contains("Direction:"));

    // Check that at the 40% split column there is NO divider
    let box_area = super::palette_box(&commands_palette, &p, AREA);
    let inner_width = box_area.width - 2;
    let preview_width = inner_width * 60 / 100;
    let list_width = inner_width - preview_width;
    let divider_x = box_area.x + 1 + list_width;
    let divider_y = box_area.y + 2;
    assert_ne!(
        buf[(divider_x, divider_y)].symbol(),
        "\u{2502}",
        "non-preview palette must not draw a divider"
    );
}

#[test]
fn narrow_terminal_omits_preview_pane_giving_match_list_full_width() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );

    let saved_palette = Palette::over_saved_queries(vec![PaletteEntry {
        label: "active_users".into(),
        detail: "SELECT * FROM users WHERE active = true;".into(),
        group: "Saved",
        command: PaletteCommand::Open("active_users".into()),
    }]);

    // Narrow area (width 70 < 80 MIN_COLUMNS)
    let narrow_area = Rect::new(0, 0, 70, 24);
    let mut buf = Buffer::empty(narrow_area);
    render_palette(&saved_palette, &p, narrow_area, &mut buf);
    let rendered = buffer_to_string(&buf);

    // Matches still render
    assert!(rendered.contains("active_users"));
    // But preview headers are omitted
    assert!(!rendered.contains("Query: active_users"));
    assert!(!rendered.contains("Origin: Saved"));

    let box_area = super::palette_box(&saved_palette, &p, narrow_area);
    let inner_width = box_area.width - 2;
    let preview_width = inner_width * 60 / 100;
    let list_width = inner_width - preview_width;
    let divider_x = box_area.x + 1 + list_width;
    let divider_y = box_area.y + 2;
    assert_ne!(
        buf[(divider_x, divider_y)].symbol(),
        "\u{2502}",
        "narrow palette must omit preview divider"
    );
}

#[test]
fn hit_row_respects_preview_split_and_rejects_preview_clicks() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );

    let saved_palette = Palette::over_saved_queries(vec![
        PaletteEntry {
            label: "query_1".into(),
            detail: "SELECT 1".into(),
            group: "Saved",
            command: PaletteCommand::Open("query_1".into()),
        },
        PaletteEntry {
            label: "query_2".into(),
            detail: "SELECT 2".into(),
            group: "Saved",
            command: PaletteCommand::Open("query_2".into()),
        },
    ]);

    let box_area = super::palette_box(&saved_palette, &p, AREA);
    let inner_width = box_area.width - 2;
    let preview_width = inner_width * 60 / 100;
    let list_width = inner_width - preview_width;
    let divider_x = box_area.x + 1 + list_width;

    // Match list row 0 is at y = box_area.y + 2 (row 0 is border, row 1 is filter line)
    let list_row_0_y = box_area.y + 2;
    let list_x = box_area.x + 5; // inside left match list
    assert_eq!(
        super::hit_row(&saved_palette, &p, AREA, list_x, list_row_0_y),
        Some(0),
        "clicking match list column hits row"
    );

    // Clicking divider column rejected
    assert_eq!(
        super::hit_row(&saved_palette, &p, AREA, divider_x, list_row_0_y),
        None,
        "clicking divider column must not hit row"
    );

    // Clicking preview pane rejected
    let preview_x = divider_x + 5;
    assert_eq!(
        super::hit_row(&saved_palette, &p, AREA, preview_x, list_row_0_y),
        None,
        "clicking preview pane must not hit row"
    );

    // Clicking filter input line rejected
    assert_eq!(
        super::hit_row(&saved_palette, &p, AREA, list_x, box_area.y + 1),
        None,
        "clicking filter line must not hit row"
    );
}
