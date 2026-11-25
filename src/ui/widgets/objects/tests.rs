//! Focused tests for the object tree's loading, expansion and first-frame
//! states.
//!
//! These carry the US4 tree contract: a catalogue load states itself in words
//! without blocking the editor, the first connected frame shows the first
//! schema level expanded, and an unconfigured client is given one next action
//! instead of an empty pane.

use super::render_objects;
use crate::app::connection_picker::ConnectionProfileSummary;
use crate::app::model::{ConnectingStep, ConnectionState, Focus, Model};
use crate::app::{Message, update};
use crate::config::ThemeChoice;
use crate::connection::Environment;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::{ObjectKind, SchemaSummary, SessionInfo, TlsState};
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::collections::BTreeMap;

fn presentation() -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

fn session() -> Box<SessionInfo> {
    Box::new(SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "42".into(),
        search_path: "public".into(),
        read_only: false,
        tls: TlsState::Disabled,
        environment: Environment::Local,
    })
}

fn schema(name: &str, tables: i64) -> SchemaSummary {
    let mut counts = BTreeMap::new();
    if tables > 0 {
        counts.insert(ObjectKind::Table, tables);
    }
    SchemaSummary {
        name: name.into(),
        usable: true,
        counts,
    }
}

fn profile() -> ConnectionProfileSummary {
    ConnectionProfileSummary {
        name: "orders".into(),
        description: None,
        location: "app@localhost:5432".into(),
        database: "orders".into(),
        user: "app".into(),
        transport: "disable".into(),
        environment: "local".into(),
        read_only: false,
        auth: None,
        provider_presentation: None,
        validation: None,
    }
}

fn render_tree(model: &Model, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    render_objects(model, &Keymap::new(), &presentation(), area, &mut buf);
    buffer_to_string(&buf)
}

fn render_frame(model: &Model, width: u16, height: u16) -> String {
    crate::ui::layout::render_to_string(model, &Keymap::new(), &presentation(), width, height)
}

#[test]
fn a_loading_tree_states_that_in_words_and_leaves_the_editor_usable() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.focus = Focus::Editor;
    model.tree.loading = true;

    let tree = render_tree(&model, 80, 30);
    assert!(
        tree.contains("Reading the catalogue"),
        "the tree states its loading in words: {tree}"
    );
    assert!(
        !tree.contains("No object rows are loaded yet."),
        "a load is not the same fact as an empty tree: {tree}"
    );

    let frame = render_frame(&model, 80, 30);
    assert!(
        frame.contains("Start here"),
        "the editor is usable while the catalogue loads: {frame}"
    );
    assert!(
        frame.contains("Ctrl+R Run the buffer once SQL is ready."),
        "the run hint is visible while the catalogue loads: {frame}"
    );
}

#[test]
fn the_first_schema_level_renders_expanded_after_connect() {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::ConnectingStep {
            step: ConnectingStep::ServerHandshake,
            target: "app@localhost:5432/orders".into(),
        },
    );
    update(&mut model, Message::Connected(session()));
    update(
        &mut model,
        Message::SchemasLoaded(Box::new(Ok(vec![schema("public", 2)]))),
    );

    assert_eq!(
        model.focus,
        Focus::Editor,
        "the editor owns the first frame"
    );
    let tree = render_tree(&model, 80, 30);
    assert!(tree.contains("public"), "{tree}");
    assert!(
        tree.contains("tables (2)"),
        "the first schema level is expanded, with the group in words: {tree}"
    );

    let frame = render_frame(&model, 80, 30);
    assert!(frame.contains("[focused]"), "focus is visible: {frame}");
    assert!(
        frame.contains("Start here"),
        "the run hint is visible on the first connected frame: {frame}"
    );
}

#[test]
fn the_unconfigured_first_frame_names_one_next_action() {
    let model = Model::new(100);
    let tree = render_tree(&model, 80, 30);
    assert!(
        tree.contains("Object tree unavailable until connected."),
        "{tree}"
    );
    assert!(
        tree.contains("Add a profile in config.toml"),
        "the next action is named: {tree}"
    );
    assert!(
        tree.contains("pass a connection target"),
        "the other route is named: {tree}"
    );
    assert!(
        !tree.contains("Choose a connection"),
        "no picker is offered when there is nothing to pick: {tree}"
    );
}

#[test]
fn a_configured_first_frame_offers_the_connection_picker_key() {
    let mut model = Model::new(100);
    model.connection_profiles.push(profile());
    let tree = render_tree(&model, 80, 30);
    assert!(
        tree.contains("Ctrl+K n"),
        "the picker key is the existing keymap route: {tree}"
    );
    assert!(
        tree.contains("Ctrl+K n Connections."),
        "the keymap wording is reused: {tree}"
    );
    assert!(
        !tree.contains("Add a profile in config.toml"),
        "a configured client is not told to add a profile: {tree}"
    );
}

#[test]
fn a_tree_error_keeps_its_headline_and_next_action() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.tree.error = Some(
        Diagnostic::new(
            DiagnosticKind::Connection,
            "permission denied for schema public",
            "reading the catalogue",
        )
        .next_action("Reload the object tree when the connection is usable."),
    );
    let tree = render_tree(&model, 80, 30);
    assert!(
        tree.contains("permission denied for schema public"),
        "{tree}"
    );
    assert!(
        tree.contains("Reload the object tree"),
        "the next action is still named: {tree}"
    );
    assert!(!tree.contains("Reading the catalogue"), "{tree}");
}

#[test]
fn a_connected_empty_tree_states_emptiness_not_unavailability() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    let tree = render_tree(&model, 80, 30);
    assert!(tree.contains("No object rows are loaded yet."), "{tree}");
    assert!(tree.contains("Reload"), "the reload key is named: {tree}");
    assert!(
        !tree.contains("Object tree unavailable until connected."),
        "a connected empty tree is not disconnected: {tree}"
    );
}

#[test]
fn a_connecting_tree_says_the_connection_is_being_established() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connecting {
        step: ConnectingStep::ServerHandshake,
        target: "app@localhost:5432/orders".into(),
    };
    let tree = render_tree(&model, 80, 30);
    assert!(
        tree.contains("The connection is being established."),
        "{tree}"
    );
    assert!(
        tree.contains("Object tree unavailable until connected."),
        "the tree still says why it is empty: {tree}"
    );
}

#[test]
fn objects_tree_renders_full_row_selection_and_semantic_icon_coloring() {
    let mut model = Model::new(100);
    update(
        &mut model,
        Message::ConnectingStep {
            step: ConnectingStep::ServerHandshake,
            target: "app@localhost:5432/orders".into(),
        },
    );
    update(&mut model, Message::Connected(session()));
    update(
        &mut model,
        Message::SchemasLoaded(Box::new(Ok(vec![schema("public", 2)]))),
    );

    let pres = presentation();
    let theme = &pres.theme;
    let selection_bg = theme.style(crate::ui::theme::Token::Selection).bg.unwrap();
    let surface_pane_bg = theme
        .surface(crate::ui::theme::Token::SurfacePane)
        .bg
        .unwrap();
    let area = Rect::new(0, 0, 40, 10);

    // 1. When focused on Objects: full row selection across the inner width
    model.focus = Focus::Objects;
    model.tree.selected = 0;
    let mut buf = Buffer::empty(area);
    render_objects(&model, &Keymap::new(), &pres, area, &mut buf);

    // Selected row is at y = 1 (inside border). All cells from x=1 to x=38 must carry selection_bg.
    for x in 1..39 {
        assert_eq!(
            buf[(x, 1)].bg,
            selection_bg,
            "x={x} on selected row must have selection background"
        );
    }
    // Unselected row at y = 2 must NOT have selection background at trailing cells
    assert_eq!(
        buf[(38, 2)].bg,
        surface_pane_bg,
        "trailing cell on unselected row must have surface pane background"
    );

    // 2. When unfocused (Focus::Editor): semantic icon coloring and no selection bar
    model.focus = Focus::Editor;
    let mut buf_unfocused = Buffer::empty(area);
    render_objects(&model, &Keymap::new(), &pres, area, &mut buf_unfocused);

    // Every trailing cell must have surface pane background
    for x in 1..39 {
        assert_eq!(
            buf_unfocused[(x, 1)].bg,
            surface_pane_bg,
            "unfocused tree row must keep pane surface background"
        );
    }

    // Row 0 is Schema "public": chevron in Muted, icon in Focus, name in Header
    let schema_icon_fg = theme.color(crate::ui::theme::Token::Focus).unwrap();
    let chevron_fg = theme.color(crate::ui::theme::Token::Muted).unwrap();
    let header_fg = theme.color(crate::ui::theme::Token::Header).unwrap();

    // Row 0 starts at x = 1: chevron
    assert_eq!(buf_unfocused[(1, 1)].fg, chevron_fg);
    // Schema icon is at x = 3
    assert_eq!(buf_unfocused[(3, 1)].fg, schema_icon_fg);
    // Name "public" starts at x = 5
    assert_eq!(buf_unfocused[(5, 1)].fg, header_fg);

    // Row 1 is Group "tables (2)": guide at x=1..2 (Border), chevron at x=3..4 (Muted), icon at x=5 (Info), "tables" at x=7 (Text), " (2)" at x=13 (Muted)
    let info_fg = theme.color(crate::ui::theme::Token::Info).unwrap();
    let text_fg = theme.color(crate::ui::theme::Token::Text).unwrap();
    let muted_fg = theme.color(crate::ui::theme::Token::Muted).unwrap();
    let border_fg = theme.color(crate::ui::theme::Token::Border).unwrap();

    // Guide at x = 1
    assert_eq!(buf_unfocused[(1, 2)].fg, border_fg);
    // Chevron at x = 3
    assert_eq!(buf_unfocused[(3, 2)].fg, muted_fg);
    // Group icon at x = 5
    assert_eq!(buf_unfocused[(5, 2)].fg, info_fg);
    // "tables" label starts at x = 7
    assert_eq!(buf_unfocused[(7, 2)].fg, text_fg);
    // " (2)" count starts at x = 13
    assert_eq!(buf_unfocused[(13, 2)].fg, muted_fg);
}

#[test]
fn editor_renders_cursorline_and_cursorlinenr_when_focused_and_unfocused() {
    use super::super::render_editor;
    let mut model = Model::new(100);
    model.editor.set_text("SELECT 1;\nSELECT 2;");
    model.editor.set_position(1, 1);

    let pres = presentation();
    let theme = &pres.theme;
    let stripe_bg = theme.stripe().bg.unwrap();
    let surface_pane_bg = theme
        .surface(crate::ui::theme::Token::SurfacePane)
        .bg
        .unwrap();
    let cursor_linenr_fg = theme.color(crate::ui::theme::Token::Focus).unwrap();
    let unfocused_linenr_fg = theme.color(crate::ui::theme::Token::Header).unwrap();

    let area = Rect::new(0, 0, 40, 10);

    // 1. When focused on Editor: line 1 has CursorLine stripe and CursorLineNr electric blue
    model.focus = Focus::Editor;
    let mut buf = Buffer::empty(area);
    render_editor(&model, &Keymap::new(), &pres, area, &mut buf);

    // Row 1 (y=1) is Line 1 (cursor line). Line number fg is Focus, bg is stripe_bg.
    assert_eq!(buf[(2, 1)].fg, cursor_linenr_fg);
    assert_eq!(buf[(2, 1)].bg, stripe_bg);

    // Across the full row (e.g. trailing empty cell at x=38), bg must be stripe_bg.
    assert_eq!(
        buf[(38, 1)].bg,
        stripe_bg,
        "trailing cell on active cursorline must carry stripe background"
    );

    // Row 2 (y=2) is Line 2 (not cursor line). Trailing empty cell must keep surface_pane_bg.
    assert_eq!(
        buf[(38, 2)].bg,
        surface_pane_bg,
        "trailing cell on non-cursorline must carry surface pane background"
    );

    // 2. When unfocused (Focus::Objects): no cursorline stripe, line number in Header
    model.focus = Focus::Objects;
    let mut buf_unfocused = Buffer::empty(area);
    render_editor(&model, &Keymap::new(), &pres, area, &mut buf_unfocused);

    // Line 1 is cursor line, but unfocused: line number fg is Header
    assert_eq!(buf_unfocused[(2, 1)].fg, unfocused_linenr_fg);
    // Trailing cell on line 1 must keep surface_pane_bg (no cursorline stripe when unfocused)
    assert_eq!(
        buf_unfocused[(38, 1)].bg,
        surface_pane_bg,
        "unfocused editor must not paint cursorline stripe"
    );
}

#[test]
fn objects_title_breadcrumbs_and_column_details_truncate_cleanly_at_narrow_width() {
    use crate::app::tree::MetadataPayload;
    use crate::postgres::{ColumnInfo, ObjectSummary};
    use ratatui::style::Color;

    let mut model = Model::new(100);
    update(&mut model, Message::Connected(session()));
    update(
        &mut model,
        Message::SchemasLoaded(Box::new(Ok(vec![schema("public", 1)]))),
    );
    model.tree.selected = 1; // Tables group
    let (req_objs, path_objs, _) = model.tree.expand_selected().expect("expand tables");
    model.tree.apply(
        req_objs,
        &path_objs,
        MetadataPayload::Objects(vec![ObjectSummary {
            schema: "public".into(),
            name: "very_long_orders_table".into(),
            kind: ObjectKind::Table,
            readable: true,
            detail: Some("4 rows".into()),
        }]),
    );
    model.tree.selected = 2; // Table row
    let (req_cols, path_cols, _) = model.tree.expand_selected().expect("expand columns");
    model.tree.apply(
        req_cols,
        &path_cols,
        MetadataPayload::Relation {
            columns: vec![
                ColumnInfo {
                    name: "primary_identifier".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    primary_key: true,
                    default: None,
                },
                ColumnInfo {
                    name: "customer_comment".into(),
                    data_type: "text".into(),
                    nullable: false,
                    primary_key: false,
                    default: None,
                },
            ],
            indexes: Vec::new(),
        },
    );
    model.tree.selected = 4; // First column so full breadcrumb path exceeds 34 cols

    let area = Rect::new(0, 0, 34, 12);
    for (tier, depth, expected_ellipsis) in [
        (GlyphTier::Unicode, ColorDepth::TrueColor, "\u{2026}"),
        (GlyphTier::Ascii, ColorDepth::Basic16, "..."),
        (GlyphTier::Ascii, ColorDepth::None, "..."),
    ] {
        let pres = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(tier),
            false,
        );
        let mut buf = Buffer::empty(area);
        render_objects(&model, &Keymap::new(), &pres, area, &mut buf);
        let text = buffer_to_string(&buf);
        let top_row = text.lines().next().unwrap();
        assert_eq!(top_row.chars().count(), 34, "{top_row}");
        assert!(
            top_row.contains(expected_ellipsis),
            "long breadcrumb title must truncate cleanly with {expected_ellipsis}: {top_row}"
        );
        assert!(
            !text.contains("primary k") && !text.contains("not nu"),
            "column constraints must truncate cleanly instead of mid-word clipping:\n{text}"
        );
        if tier == GlyphTier::Ascii {
            assert!(text.is_ascii(), "{depth:?}: {text:?}");
            assert_eq!(buf[(1, 1)].bg, Color::Reset);
        }
    }

    // Empty connected pane at compact 40x8 in ASCII + ColorDepth::None stays strictly ASCII.
    let empty_model = {
        let mut m = Model::new(100);
        m.connection = ConnectionState::Connected(session());
        m
    };
    let ascii_none = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let compact = Rect::new(0, 0, 40, 8);
    let mut empty_buf = Buffer::empty(compact);
    render_objects(
        &empty_model,
        &Keymap::new(),
        &ascii_none,
        compact,
        &mut empty_buf,
    );
    let empty_text = buffer_to_string(&empty_buf);
    assert!(empty_text.is_ascii(), "{empty_text:?}");
    assert!(
        empty_text.contains("No object rows are loaded yet."),
        "{empty_text}"
    );
}

#[test]
fn a_column_detail_is_shortened_at_a_fact_boundary() {
    let detail = "text, not null, default 'x'";
    assert_eq!(super::clip_detail(detail, 40, true), detail);
    assert_eq!(
        super::clip_detail(detail, 18, true),
        "text, not null, \u{2026}"
    );
    assert_eq!(super::clip_detail(detail, 12, true), "text, \u{2026}");
    assert_eq!(super::clip_detail(detail, 12, false), "text, ...");
    // A single fact too wide for the budget falls back to plain truncation.
    assert_eq!(
        super::clip_detail("character varying(255)", 10, true),
        crate::query::value::truncate_to_width("character varying(255)", 10, true)
    );
    for budget in 3..30 {
        let clipped = super::clip_detail(detail, budget, true);
        assert!(
            crate::query::value::display_width(&clipped) <= budget,
            "{budget}: {clipped:?}"
        );
        assert!(!clipped.contains("not n\u{2026}"), "{budget}: {clipped:?}");
    }
}

#[test]
fn the_filter_line_takes_its_search_mark_from_the_glyph_tier() {
    let mut model = Model::new(100);
    update(&mut model, Message::Connected(session()));
    update(
        &mut model,
        Message::SchemasLoaded(Box::new(Ok(vec![schema("public", 2)]))),
    );
    model.tree.filtering = true;
    model.tree.filter = "ord".into();
    let area = Rect::new(0, 0, 40, 12);
    for (tier, expected) in [
        (GlyphTier::Nerd, " \u{ea6d} ord"),
        (GlyphTier::Unicode, " \u{203a} ord"),
        (GlyphTier::Ascii, " /ord"),
    ] {
        let presentation = Presentation::new(
            Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
            Glyphs::new(tier),
            false,
        );
        let mut buf = Buffer::empty(area);
        render_objects(&model, &Keymap::new(), &presentation, area, &mut buf);
        let text = buffer_to_string(&buf);
        let filter_line = text.lines().nth(1).unwrap_or_default();
        assert!(filter_line.contains(expected), "{tier:?}: {text}");
        assert!(!filter_line.contains('\u{2315}'), "{tier:?}: {text}");
    }
}

#[test]
fn objects_scrollbar_renders_only_when_overflowing_with_focus_tokens() {
    use crate::app::tree::MetadataPayload;
    use crate::postgres::ObjectSummary;

    let mut model = Model::new(100);
    update(&mut model, Message::Connected(session()));
    update(
        &mut model,
        Message::SchemasLoaded(Box::new(Ok(vec![schema("public", 12)]))),
    );
    model.tree.selected = 1;
    let (req, path, _) = model.tree.expand_selected().expect("expand tables");
    let tables = (1..=12)
        .map(|idx| ObjectSummary {
            schema: "public".into(),
            name: format!("orders_{idx:02}"),
            kind: ObjectKind::Table,
            readable: true,
            detail: None,
        })
        .collect();
    model
        .tree
        .apply(req, &path, MetadataPayload::Objects(tables));

    let area = Rect::new(0, 0, 36, 8);
    let scroll_x = area.width - 2;
    let unicode = presentation();

    model.focus = Focus::Objects;
    let mut focused_buf = Buffer::empty(area);
    render_objects(&model, &Keymap::new(), &unicode, area, &mut focused_buf);
    let thumb_y = (1..area.height - 1)
        .find(|&y| focused_buf[(scroll_x, y)].symbol() == "\u{2503}")
        .expect("unicode scrollbar thumb");
    let track_y = (1..area.height - 1)
        .find(|&y| focused_buf[(scroll_x, y)].symbol() == "\u{2502}")
        .expect("unicode scrollbar track");
    assert_eq!(
        Some(focused_buf[(scroll_x, thumb_y)].fg),
        unicode.theme.color(crate::ui::theme::Token::Focus)
    );
    assert_eq!(
        Some(focused_buf[(scroll_x, track_y)].fg),
        unicode.theme.color(crate::ui::theme::Token::Border)
    );

    model.focus = Focus::Editor;
    let mut unfocused_buf = Buffer::empty(area);
    render_objects(&model, &Keymap::new(), &unicode, area, &mut unfocused_buf);
    assert_eq!(
        Some(unfocused_buf[(scroll_x, thumb_y)].fg),
        unicode.theme.color(crate::ui::theme::Token::Muted)
    );

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut ascii_buf = Buffer::empty(area);
    render_objects(&model, &Keymap::new(), &ascii, area, &mut ascii_buf);
    assert!((1..area.height - 1).any(|y| ascii_buf[(scroll_x, y)].symbol() == "#"));
    assert!((1..area.height - 1).any(|y| ascii_buf[(scroll_x, y)].symbol() == "|"));
}

#[test]
fn objects_filter_uses_surface_alt_strip_and_highlights_unselected_utf8_matches() {
    use crate::app::tree::MetadataPayload;
    use crate::postgres::ObjectSummary;
    use ratatui::style::Modifier;

    let mut model = Model::new(100);
    update(&mut model, Message::Connected(session()));
    update(
        &mut model,
        Message::SchemasLoaded(Box::new(Ok(vec![schema("public", 2)]))),
    );
    model.tree.selected = 1;
    let (req, path, _) = model.tree.expand_selected().expect("expand tables");
    model.tree.apply(
        req,
        &path,
        MetadataPayload::Objects(vec![
            ObjectSummary {
                schema: "public".into(),
                name: "café_Orders".into(),
                kind: ObjectKind::Table,
                readable: true,
                detail: None,
            },
            ObjectSummary {
                schema: "public".into(),
                name: "audit_orders".into(),
                kind: ObjectKind::Table,
                readable: true,
                detail: None,
            },
        ]),
    );
    model.focus = Focus::Objects;
    model.tree.filtering = true;
    model.tree.filter = "ORD".into();
    // Keep row 0 ("public") selected so the object rows below are unselected.
    model.tree.selected = 0;

    let pres = presentation();
    let area = Rect::new(0, 0, 44, 10);
    let mut buf = Buffer::empty(area);
    render_objects(&model, &Keymap::new(), &pres, area, &mut buf);

    let stripe_bg = pres.theme.stripe().bg.expect("SurfaceAlt bg");
    for x in 1..area.width - 1 {
        assert_eq!(
            buf[(x, 1)].bg,
            stripe_bg,
            "filter strip cell at x={x} uses SurfaceAlt"
        );
    }

    let focus_fg = pres.theme.color(crate::ui::theme::Token::Focus);
    let cafe_y = (2..area.height - 1)
        .find(|&y| {
            (1..area.width - 1)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("café_Orders")
        })
        .expect("utf-8 filtered row");
    let row_str: String = (0..area.width).map(|x| buf[(x, cafe_y)].symbol()).collect();
    let ord_x = row_str[..row_str.find("Ord").expect("Ord substring")]
        .chars()
        .count() as u16;
    for offset in 0..3 {
        let cell = &buf[(ord_x + offset, cafe_y)];
        assert_eq!(Some(cell.fg), focus_fg);
        assert!(
            cell.modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
    }
    let c_x = row_str[..row_str.find("caf").expect("caf prefix")]
        .chars()
        .count() as u16;
    assert!(!buf[(c_x, cafe_y)].modifier.contains(Modifier::UNDERLINED));
}
