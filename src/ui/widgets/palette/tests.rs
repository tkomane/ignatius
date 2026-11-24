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

#[test]
fn selected_palette_match_row_pads_full_width_with_selection_background() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let selection_bg = p.theme.color(Token::Selection).expect("selection color");

    // 1. Non-preview mode (width 70 < MIN_COLUMNS)
    let narrow_area = Rect::new(0, 0, 70, 20);
    let mut narrow_buf = Buffer::empty(narrow_area);
    let pal = palette();
    render_palette(&pal, &p, narrow_area, &mut narrow_buf);
    let box_area = super::palette_box(&pal, &p, narrow_area);
    let row_y = box_area.y + 2; // first match row
    let inner_start_x = box_area.x + 1;
    let inner_end_x = box_area.x + box_area.width - 1;
    for x in inner_start_x..inner_end_x {
        assert_eq!(
            narrow_buf[(x, row_y)].bg,
            selection_bg,
            "non-preview selected row cell at x={x} must carry selection background"
        );
    }

    // 2. Preview mode (width 100 >= MIN_COLUMNS)
    let wide_area = Rect::new(0, 0, 100, 24);
    let mut wide_buf = Buffer::empty(wide_area);
    let saved_pal = Palette::over_saved_queries(vec![PaletteEntry {
        label: "active_users".into(),
        detail: "SELECT * FROM users;".into(),
        group: "Saved",
        command: PaletteCommand::Open("active_users".into()),
    }]);
    render_palette(&saved_pal, &p, wide_area, &mut wide_buf);
    let wide_box = super::palette_box(&saved_pal, &p, wide_area);
    let inner_width = wide_box.width - 2;
    let preview_width = inner_width * 60 / 100;
    let list_width = inner_width - preview_width;
    let wide_row_y = wide_box.y + 2;
    for x in (wide_box.x + 1)..(wide_box.x + 1 + list_width) {
        assert_eq!(
            wide_buf[(x, wide_row_y)].bg,
            selection_bg,
            "preview mode selected row cell at x={x} must carry selection background across list_width"
        );
    }
}

#[test]
fn saved_query_preview_pane_highlights_sql_syntax_and_styles_headers() {
    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let wide_area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(wide_area);
    let saved_pal = Palette::over_saved_queries(vec![PaletteEntry {
        label: "users_query".into(),
        detail: "SELECT id FROM users\nWHERE active = true;".into(),
        group: "Saved",
        command: PaletteCommand::Open("users_query".into()),
    }]);
    render_palette(&saved_pal, &p, wide_area, &mut buf);

    let query_hdr_pos = cell(&buf, "Query:");
    assert!(
        buf[query_hdr_pos].modifier.contains(Modifier::BOLD),
        "Query: key must have Header style (bold)"
    );

    let sql_hdr_pos = cell(&buf, "SQL:");
    assert!(
        buf[sql_hdr_pos].modifier.contains(Modifier::BOLD),
        "SQL: header must have Header style (bold)"
    );

    let keyword_color = p.theme.color(Token::SyntaxKeyword).expect("keyword color");
    let select_pos = cell(&buf, "SELECT");
    assert_eq!(
        buf[select_pos].fg, keyword_color,
        "SELECT must have SyntaxKeyword foreground"
    );
    let from_pos = cell(&buf, "FROM");
    assert_eq!(
        buf[from_pos].fg, keyword_color,
        "FROM must have SyntaxKeyword foreground"
    );
    let where_pos = cell(&buf, "WHERE");
    assert_eq!(
        buf[where_pos].fg, keyword_color,
        "WHERE must have SyntaxKeyword foreground"
    );
}

#[test]
fn palette_spotlight_strip_split_title_and_preview_gutter_across_depths() {
    let long_label = "very_long_saved_query_title_that_must_not_touch_the_vertical_divider";
    let saved_pal = Palette::over_saved_queries(vec![PaletteEntry {
        label: long_label.into(),
        detail:
            "SELECT column_alpha, column_beta, column_gamma FROM public.very_long_relation_name;"
                .into(),
        group: "Saved",
        command: PaletteCommand::Open(long_label.into()),
    }]);

    let p = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let mut buf = Buffer::empty(AREA);
    render_palette(&saved_pal, &p, AREA, &mut buf);

    let box_area = super::palette_box(&saved_pal, &p, AREA);
    let inner_width = box_area.width - 2;
    let preview_width = inner_width * 60 / 100;
    let list_width = inner_width - preview_width;
    let divider_x = box_area.x + 1 + list_width;
    let search_y = box_area.y + 1;
    let row0_y = box_area.y + 2;

    let spotlight_bg = p.theme.surface(Token::SurfaceAlt).bg.unwrap();
    for x in (box_area.x + 1)..(box_area.x + box_area.width - 1) {
        assert_eq!(
            buf[(x, search_y)].bg,
            spotlight_bg,
            "Spotlight search strip at x={x} must carry SurfaceAlt background"
        );
    }

    let text = buffer_to_string(&buf);
    assert!(text.contains("1 match"), "{text}");
    assert_eq!(buf[(divider_x, row0_y)].symbol(), "\u{2502}");
    // Selected item text stays at least 1 cell away from divider_x while Selection bg fills up to divider_x.
    assert_eq!(buf[(divider_x - 1, row0_y)].symbol(), " ");
    assert_eq!(
        buf[(divider_x - 1, row0_y)].bg,
        p.theme.style(Token::Selection).bg.unwrap()
    );
    // Wrapped SQL preview keeps 1-cell breathing gutter immediately right of divider_x.
    for row in row0_y..(box_area.y + box_area.height - 1) {
        assert_eq!(
            buf[(divider_x + 1, row)].symbol(),
            " ",
            "preview gutter cell right of divider on row {row} must stay blank"
        );
    }

    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut low_buf = Buffer::empty(AREA);
        render_palette(&saved_pal, &low, AREA, &mut low_buf);
        assert!(buffer_to_string(&low_buf).is_ascii(), "{depth:?}");
        assert_eq!(low_buf[(box_area.x + 2, search_y)].bg, Color::Reset);
        assert_eq!(low_buf[(divider_x, row0_y)].symbol(), "|");
    }
}

#[test]
fn palette_uses_purpose_icons_and_preview_hairline() {
    let saved_pal = Palette::over_saved_queries(vec![
        PaletteEntry {
            label: "revenue_by_region".into(),
            detail: "SELECT region FROM reporting.order_totals;".into(),
            group: "saved",
            command: PaletteCommand::Open("revenue_by_region".into()),
        },
        PaletteEntry {
            label: "active_orders".into(),
            detail: "SELECT id FROM public.orders;".into(),
            group: "saved",
            command: PaletteCommand::Open("active_orders".into()),
        },
    ]);
    let nerd = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    let mut buf = Buffer::empty(AREA);
    render_palette(&saved_pal, &nerd, AREA, &mut buf);
    let text = buffer_to_string(&buf);

    let editor_glyph = nerd.icon(crate::ui::glyphs::Icon::Editor);
    assert!(
        text.contains(&format!("{editor_glyph}Saved queries")),
        "saved queries palette should display the Editor icon: {text}"
    );
    assert!(
        text.contains("\u{2500}\u{2500}\u{2500}\u{2500}"),
        "preview pane should separate metadata from SQL with a hairline rule: {text}"
    );

    let mut history_pal = Palette::over_history(vec![PaletteEntry {
        label: "SELECT 1;".into(),
        detail: "1 row".into(),
        group: "history",
        command: PaletteCommand::Open("SELECT 1;".into()),
    }]);
    history_pal.query = String::new();
    let mut hist_buf = Buffer::empty(AREA);
    render_palette(&history_pal, &nerd, AREA, &mut hist_buf);
    let clock_glyph = nerd.icon(crate::ui::glyphs::Icon::Clock);
    let hist_text = buffer_to_string(&hist_buf);
    assert!(
        hist_text.contains(&format!("{clock_glyph}History")),
        "history palette should display the Clock icon: {hist_text}"
    );

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut ascii_buf = Buffer::empty(AREA);
    render_palette(&saved_pal, &ascii, AREA, &mut ascii_buf);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(ascii_text.is_ascii(), "{ascii_text:?}");
    assert!(ascii_text.contains("----"), "{ascii_text}");
}

fn search_line(buf: &Buffer) -> String {
    let y = (0..AREA.height)
        .find(|&y| {
            (0..AREA.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("match")
        })
        .expect("search strip");
    (0..AREA.width).map(|x| buf[(x, y)].symbol()).collect()
}

#[test]
fn the_palette_prompt_follows_the_glyph_tier_and_counts_in_words() {
    for (tier, prompt) in [
        (GlyphTier::Ascii, " > "),
        (GlyphTier::Unicode, " \u{203a} "),
        (GlyphTier::Nerd, " \u{ea6d} "),
    ] {
        let presentation = Presentation::new(
            Theme::new(ThemeChoice::Dark, true),
            Glyphs::new(tier),
            false,
        );
        let line = search_line(&render(&presentation, 0));
        assert!(line.contains(prompt), "{tier:?}: {line:?}");
        assert!(!line.contains('\u{2315}'), "{tier:?}: {line:?}");
        assert!(line.contains("2 matches"), "{tier:?}: {line:?}");
    }

    let presentation = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let mut one = palette();
    one.query = "Switch".into();
    let mut buf = Buffer::empty(AREA);
    render_palette(&one, &presentation, AREA, &mut buf);
    let line = search_line(&buf);
    assert!(line.contains("1 match"), "{line:?}");
    assert!(!line.contains("matches"), "{line:?}");
}

#[test]
fn an_empty_palette_query_shows_a_muted_placeholder_that_typing_replaces() {
    let presentation = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let buf = render(&presentation, 0);
    let (x, y) = cell(&buf, "Type to filter");
    assert_eq!(
        Some(buf[(x, y)].fg),
        presentation.theme.color(Token::Muted),
        "the placeholder is the quietest ink"
    );
    assert_eq!(
        buf[(x, y)].bg,
        presentation.theme.surface(Token::SurfaceAlt).bg.unwrap()
    );

    let mut typed = palette();
    typed.query = "for".into();
    let mut buf = Buffer::empty(AREA);
    render_palette(&typed, &presentation, AREA, &mut buf);
    let line = search_line(&buf);
    assert!(!line.contains("Type to filter"), "{line:?}");
    assert!(line.contains("for"), "{line:?}");

    // Without colour or Unicode it is still words.
    let plain = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let line = search_line(&render(&plain, 0));
    assert!(line.contains(" > |"), "{line:?}");
    assert!(line.contains("Type to filter"), "{line:?}");
    assert!(line.is_ascii(), "{line:?}");
}

#[test]
fn palette_query_highlights_matched_substrings_and_subsequences_without_changing_text() {
    let entries = vec![
        PaletteEntry {
            group: "Editor",
            label: "Format first".into(),
            detail: "First formatter".into(),
            command: PaletteCommand::Run(Action::FormatBuffer),
        },
        PaletteEntry {
            group: "Editor",
            label: "Format second".into(),
            detail: "Second formatter".into(),
            command: PaletteCommand::Run(Action::FormatBuffer),
        },
        PaletteEntry {
            group: "Session",
            label: "Switch theme".into(),
            detail: "Cycle theme".into(),
            command: PaletteCommand::Run(Action::SwitchTheme),
        },
    ];
    let p_true = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );

    // Non-empty query highlights contiguous substring matches in unselected rows
    // with Focus + BOLD + UNDERLINED, keeping plain text identical.
    let mut sub_pal = Palette::new(entries.clone());
    for ch in "for".chars() {
        sub_pal.push(ch);
    }
    sub_pal.selected = 0;

    let mut plain_pal = Palette::new(entries.clone());
    plain_pal.selected = 0;

    let mut sub_buf = Buffer::empty(AREA);
    render_palette(&sub_pal, &p_true, AREA, &mut sub_buf);
    let mut plain_buf = Buffer::empty(AREA);
    render_palette(&plain_pal, &p_true, AREA, &mut plain_buf);

    let sub_str = buffer_to_string(&sub_buf);
    let plain_str = buffer_to_string(&plain_buf);
    // Matched rows contain exact labels with zero extra characters inserted.
    assert!(sub_str.contains("Format first"), "{sub_str}");
    assert!(sub_str.contains("Format second"), "{sub_str}");
    assert!(plain_str.contains("Format second"), "{plain_str}");

    let focus_fg = p_true.theme.color(Token::Focus).unwrap();
    let text_fg = p_true.theme.color(Token::Text).unwrap();
    let (fx, fy) = cell(&sub_buf, "Format second");
    for offset in 0..3 {
        let c = &sub_buf[(fx + offset, fy)];
        assert_eq!(c.fg, focus_fg, "matched char {offset} should use Focus fg");
        assert!(c.modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
    }
    let unmatched_m = &sub_buf[(fx + 3, fy)];
    assert_eq!(unmatched_m.fg, text_fg);
    assert!(!unmatched_m.modifier.contains(Modifier::UNDERLINED));

    // Selected row stays uniformly styled in Selection.
    let (sx, sy) = cell(&sub_buf, "Format first");
    let sel_fg = p_true.theme.style(Token::Selection).fg.unwrap();
    assert_eq!(sub_buf[(sx, sy)].fg, sel_fg);
    assert!(!sub_buf[(sx, sy)].modifier.contains(Modifier::UNDERLINED));

    // Sparse subsequence match (`"fs"` against `"Format second"`).
    // Select row 1 ("Format first") so row 0 ("Format second") is unselected.
    let mut seq_pal = Palette::new(entries);
    for ch in "fs".chars() {
        seq_pal.push(ch);
    }
    seq_pal.selected = 1;
    let mut seq_buf = Buffer::empty(AREA);
    render_palette(&seq_pal, &p_true, AREA, &mut seq_buf);
    let (qx, qy) = cell(&seq_buf, "Format second");
    assert!(
        seq_buf[(qx, qy)]
            .modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED),
        "leading 'F' in subsequence must be highlighted"
    );
    assert_eq!(seq_buf[(qx + 7, qy)].symbol(), "s");
    assert!(
        seq_buf[(qx + 7, qy)]
            .modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED),
        "'s' in 'second' must be highlighted for subsequence 'fs'"
    );
    assert!(
        !seq_buf[(qx + 1, qy)]
            .modifier
            .contains(Modifier::UNDERLINED)
    );

    // At ColorDepth::None, plain text remains clean ASCII without color.
    let p_none = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut none_buf = Buffer::empty(AREA);
    render_palette(&sub_pal, &p_none, AREA, &mut none_buf);
    let none_text = buffer_to_string(&none_buf);
    assert!(none_text.is_ascii(), "{none_text}");
    let (nx, ny) = cell(&none_buf, "Format second");
    assert_eq!(none_buf[(nx, ny)].fg, Color::Reset);
}

#[test]
fn palette_preview_paints_sql_block_and_single_line_sql_on_full_width_stripe() {
    let long_sql = "SELECT id, email, status FROM orders WHERE created_at >= now() - interval '7 days' ORDER BY id DESC;";
    let single_line_pal = Palette::over_saved_queries(vec![PaletteEntry {
        label: "recent_orders".into(),
        detail: long_sql.into(),
        group: "Saved",
        command: PaletteCommand::Open("recent_orders".into()),
    }]);
    let block_pal = Palette::over_saved_queries(vec![PaletteEntry {
        label: "users_query".into(),
        detail: "SELECT id FROM users\nWHERE active = true;".into(),
        group: "Saved",
        command: PaletteCommand::Open("users_query".into()),
    }]);

    for pal in [&single_line_pal, &block_pal] {
        let p_true = Presentation::new(
            Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
            Glyphs::new(GlyphTier::Unicode),
            false,
        );
        let mut true_buf = Buffer::empty(AREA);
        render_palette(pal, &p_true, AREA, &mut true_buf);

        let stripe_bg = p_true.theme.surface(Token::SurfaceAlt).bg.unwrap();
        let kw_fg = p_true.theme.color(Token::SyntaxKeyword).unwrap();
        let box_area = super::palette_box(pal, &p_true, AREA);
        let inner_w = box_area.width.saturating_sub(2);
        let preview_w = inner_w * 60 / 100;
        let list_w = inner_w.saturating_sub(preview_w);
        let divider_x = box_area.x + 1 + list_w;
        let preview_x = divider_x + 2;
        let preview_end = box_area.right().saturating_sub(1);

        let sel_pos = (box_area.y + 2..box_area.bottom() - 1)
            .find_map(|y| {
                let row: String = (preview_x..preview_end)
                    .map(|x| true_buf[(x, y)].symbol())
                    .collect();
                row.find("SELECT")
                    .map(|byte| (preview_x + row[..byte].chars().count() as u16, y))
            })
            .expect("SELECT in preview SQL card");
        assert_eq!(true_buf[sel_pos].fg, kw_fg);
        for x in preview_x..preview_end {
            assert_eq!(
                true_buf[(x, sel_pos.1)].bg,
                stripe_bg,
                "cell ({x}, {}) in SQL card should carry SurfaceAlt stripe background",
                sel_pos.1
            );
        }

        // Across None depth the preview stays readable plain text with Reset bg.
        let p_none = Presentation::new(
            Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut none_buf = Buffer::empty(AREA);
        render_palette(pal, &p_none, AREA, &mut none_buf);
        assert!(buffer_to_string(&none_buf).is_ascii());
        let sel_none = cell(&none_buf, "SELECT");
        assert_eq!(none_buf[sel_none].bg, Color::Reset);
    }
}

#[test]
fn palette_divider_draws_scroll_thumb_without_consuming_extra_columns_or_affecting_hit_row() {
    let entries: Vec<PaletteEntry> = (0..25)
        .map(|i| PaletteEntry {
            group: "Saved",
            label: format!("saved_query_{i:02}"),
            detail: format!("SELECT {i};"),
            command: PaletteCommand::Open(format!("saved_query_{i:02}")),
        })
        .collect();
    let mut pal = Palette::over_saved_queries(entries);

    let p_uni = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let box_area = super::palette_box(&pal, &p_uni, AREA);
    let inner_w = box_area.width.saturating_sub(2);
    let preview_w = inner_w * 60 / 100;
    let list_w = inner_w - preview_w;
    let divider_x = box_area.x + 1 + list_w;
    let first_list_y = box_area.y + 2;
    let end_list_y = box_area.bottom().saturating_sub(2);

    let mut top_buf = Buffer::empty(AREA);
    render_palette(&pal, &p_uni, AREA, &mut top_buf);
    let focus_fg = p_uni.theme.color(Token::Focus).unwrap();
    let border_fg = p_uni.theme.color(Token::Border).unwrap();

    let top_thumbs: Vec<u16> = (first_list_y..=end_list_y)
        .filter(|&y| top_buf[(divider_x, y)].symbol() == "\u{2503}")
        .collect();
    assert!(!top_thumbs.is_empty(), "expected ┃ thumb on divider_x");
    assert_eq!(top_buf[(divider_x, top_thumbs[0])].fg, focus_fg);
    let non_thumb_y = (first_list_y..=end_list_y)
        .find(|y| !top_thumbs.contains(y))
        .expect("track row");
    assert_eq!(top_buf[(divider_x, non_thumb_y)].symbol(), "\u{2502}");
    assert_eq!(top_buf[(divider_x, non_thumb_y)].fg, border_fg);

    // Hit testing stays identical along list column and rejects divider column.
    assert_eq!(
        super::hit_row(&pal, &p_uni, AREA, box_area.x + 1, first_list_y),
        Some(0)
    );
    assert_eq!(
        super::hit_row(&pal, &p_uni, AREA, divider_x, first_list_y),
        None
    );

    // Moving selection near bottom moves thumb down.
    pal.selected = 24;
    let mut bot_buf = Buffer::empty(AREA);
    render_palette(&pal, &p_uni, AREA, &mut bot_buf);
    let bot_thumbs: Vec<u16> = (first_list_y..=end_list_y)
        .filter(|&y| bot_buf[(divider_x, y)].symbol() == "\u{2503}")
        .collect();
    assert!(bot_thumbs[0] > top_thumbs[0], "thumb should scroll down");

    // In ASCII mode, thumb renders as `#` cleanly.
    let p_ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut ascii_buf = Buffer::empty(AREA);
    render_palette(&pal, &p_ascii, AREA, &mut ascii_buf);
    assert!(buffer_to_string(&ascii_buf).is_ascii());
    assert!(
        (first_list_y..=end_list_y).any(|y| ascii_buf[(divider_x, y)].symbol() == "#"),
        "ASCII palette divider should show '#' scroll thumb"
    );

    // When all matches fit on screen, divider stays plain track without thumb.
    let short_pal = Palette::over_saved_queries(vec![PaletteEntry {
        group: "Saved",
        label: "only_one".into(),
        detail: "SELECT 1;".into(),
        command: PaletteCommand::Open("only_one".into()),
    }]);
    let mut short_buf = Buffer::empty(AREA);
    render_palette(&short_pal, &p_uni, AREA, &mut short_buf);
    assert!(
        (first_list_y..=end_list_y).all(|y| short_buf[(divider_x, y)].symbol() == "\u{2502}"),
        "unscrollable list should not render a thumb"
    );
}
