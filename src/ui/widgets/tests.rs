//! Focused tests for the pure hit-region function.
//!
//! Each expectation is the screen cell a renderer paints, so a region can only
//! pass these tests if it agrees with the drawing arithmetic.

use super::{
    Focus, Region, region_at, render_editor, render_objects, render_palette,
    render_password_prompt, render_results,
};
use crate::app::model::{Model, PasswordPrompt};
use crate::app::palette::{Palette, PaletteCommand, PaletteEntry};
use crate::config::ThemeChoice;
use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
use crate::query::value::Cell;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, body_area, main_panes, sidebar_width};
use crate::ui::theme::{ColorDepth, Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use std::time::Duration;

#[test]
fn density_title_padding_preserves_content_whitespace_on_panes_and_overlays() {
    for density in [
        crate::config::Density::Comfortable,
        crate::config::Density::Compact,
    ] {
        let p = colour_off_presentation().with_density(density);
        for overlay in [false, true] {
            let area = Rect::new(0, 0, 30, 3);
            let mut buf = Buffer::empty(area);
            let block = if overlay {
                super::overlay_block("  name  ".into(), &p)
            } else {
                super::pane_block("  name  ".into(), true, &p)
            };
            use ratatui::widgets::Widget;
            block.render(area, &mut buf);
            let line = crate::ui::layout::buffer_to_string(&buf)
                .lines()
                .next()
                .unwrap()
                .to_owned();
            let n = line.chars().position(|ch| ch == 'n').unwrap();
            assert_eq!(
                n,
                if density == crate::config::Density::Comfortable {
                    3
                } else {
                    2
                },
                "{line}"
            );
            assert_eq!(
                line.chars().nth(n + 4),
                Some(' '),
                "the content's final space survives: {line}"
            );
        }
    }
}

// ------------------------------------------------- painted surfaces (T017)

/// A presentation on one palette at a chosen colour depth.
fn presentation_at(depth: ColorDepth) -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(depth),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

/// The same palette with colour resolved off entirely.
fn colour_off_presentation() -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, false),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

/// A whole frame's worth of state: a loaded tree, multi-line SQL and a grid.
fn full_model() -> Model {
    let mut model = grid_model(2, 4);
    model.sidebar_visible = true;
    model.focus = Focus::Results;
    model
        .editor
        .set_text("SELECT id, name\nFROM orders\nORDER BY id;");
    with_tree(&mut model);
    model
}

/// The three pane rectangles the full renderer draws.
fn pane_rects(area: Rect) -> (Rect, Rect, Rect) {
    let body = body_area(area);
    let width = sidebar_width(body.width);
    let [tree, main] =
        Layout::horizontal([Constraint::Length(width), Constraint::Min(30)]).areas(body);
    let [editor, results] =
        Layout::vertical([Constraint::Percentage(40), Constraint::Min(3)]).areas(main);
    (tree, editor, results)
}

/// The rectangle `render_palette` computes for its bordered box.
fn palette_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(8).min(76);
    let height = area.height.saturating_sub(6).min(18);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + 2,
        width,
        height,
    }
}

/// The rectangle `render_password_prompt` computes for its bordered box.
fn password_prompt_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(6).min(72);
    let height = 9.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn render_panes(model: &Model, presentation: &Presentation, area: Rect) -> Buffer {
    let (tree, editor, results) = pane_rects(area);
    let mut buf = Buffer::empty(area);
    let keymap = Keymap::new();
    render_objects(model, &keymap, presentation, tree, &mut buf);
    render_editor(model, &keymap, presentation, editor, &mut buf);
    render_results(model, &keymap, presentation, results, &mut buf);
    buf
}

/// The background a painted surface token resolves to, or `None` at depths
/// that must not paint.
fn surface_background(presentation: &Presentation, token: Token) -> Option<Color> {
    presentation.theme.surface(token).bg
}

fn count_background(buf: &Buffer, rect: Rect, colour: Color) -> usize {
    let mut count = 0;
    for y in rect.y..rect.y.saturating_add(rect.height) {
        for x in rect.x..rect.x.saturating_add(rect.width) {
            if buf[(x, y)].bg == colour {
                count += 1;
            }
        }
    }
    count
}

/// Every cell of `rect` must be exactly `colour`.
fn assert_background(buf: &Buffer, rect: Rect, colour: Color, label: &str) {
    for y in rect.y..rect.y.saturating_add(rect.height) {
        for x in rect.x..rect.x.saturating_add(rect.width) {
            assert_eq!(
                buf[(x, y)].bg,
                colour,
                "{label}: cell ({x},{y}) is not the surface it should be"
            );
        }
    }
}

/// Every cell of `rect` must be painted, and only with a documented surface.
fn assert_theme_surfaced(buf: &Buffer, rect: Rect, allowed: &[Color], label: &str) {
    for y in rect.y..rect.y.saturating_add(rect.height) {
        for x in rect.x..rect.x.saturating_add(rect.width) {
            let bg = buf[(x, y)].bg;
            assert_ne!(bg, Color::Reset, "{label}: cell ({x},{y}) is unpainted");
            assert!(
                allowed.contains(&bg),
                "{label}: cell ({x},{y}) painted {bg:?}, not a documented surface"
            );
        }
    }
}

/// No cell of `rect` may carry a background at all.
fn assert_unpainted(buf: &Buffer, rect: Rect, label: &str) {
    for y in rect.y..rect.y.saturating_add(rect.height) {
        for x in rect.x..rect.x.saturating_add(rect.width) {
            assert_eq!(
                buf[(x, y)].bg,
                Color::Reset,
                "{label}: cell ({x},{y}) painted a surface at a depth that must not"
            );
        }
    }
}

#[test]
fn pane_interiors_carry_the_pane_surface_at_truecolor_and_256() {
    let model = full_model();
    let area = Rect::new(0, 0, 100, 30);
    let (tree, editor, results) = pane_rects(area);

    for depth in [ColorDepth::TrueColor, ColorDepth::Indexed256] {
        let presentation = presentation_at(depth);
        let buf = render_panes(&model, &presentation, area);
        let pane =
            surface_background(&presentation, Token::SurfacePane).expect("a painted pane surface");

        // This model keeps focus on the results, so the tree and the editor
        // draw no selected cell and every interior cell is the pane itself.
        assert_background(&buf, inner(tree), pane, "object tree");
        assert_background(&buf, inner(editor), pane, "editor");

        // The grid paints a selected row and alternate stripes over the pane.
        // Every cell must still be a documented theme surface, none left to
        // the terminal, and the pane fill must be present.
        let stripe =
            surface_background(&presentation, Token::SurfaceAlt).expect("a stripe surface");
        let selection =
            surface_background(&presentation, Token::Selection).expect("a selection surface");
        assert_theme_surfaced(&buf, inner(results), &[pane, stripe, selection], "results");
        assert!(
            count_background(&buf, inner(results), pane) > 0,
            "{depth:?}: the results interior carried no pane surface"
        );
    }
}

#[test]
fn overlay_interiors_carry_the_overlay_surface_at_truecolor_and_256() {
    let area = Rect::new(0, 0, 100, 30);
    for depth in [ColorDepth::TrueColor, ColorDepth::Indexed256] {
        let presentation = presentation_at(depth);
        let overlay = surface_background(&presentation, Token::SurfaceOverlay)
            .expect("a painted overlay surface");
        let selection =
            surface_background(&presentation, Token::Selection).expect("a selection surface");

        let palette = Palette::new(vec![PaletteEntry {
            label: "Run".into(),
            detail: "run the buffer".into(),
            group: "action",
            command: PaletteCommand::Run(crate::app::Action::RunBuffer),
        }]);
        let mut buf = Buffer::empty(area);
        render_palette(&palette, &presentation, area, &mut buf);
        let box_area = palette_area(area);
        assert_theme_surfaced(&buf, inner(box_area), &[overlay, selection], "palette");
        assert!(
            count_background(&buf, inner(box_area), overlay) > 0,
            "{depth:?}: the palette interior carried no overlay surface"
        );

        let prompt = PasswordPrompt::new("orders", "the server asked");
        let mut buf = Buffer::empty(area);
        render_password_prompt(&prompt, &presentation, area, &mut buf);
        let box_area = password_prompt_area(area);
        assert_theme_surfaced(&buf, inner(box_area), &[overlay], "password prompt");
        assert!(
            count_background(&buf, inner(box_area), overlay) > 0,
            "{depth:?}: the prompt interior carried no overlay surface"
        );
    }
}

#[test]
fn panes_and_overlays_stay_unpainted_at_16_colour_and_with_colour_off() {
    let model = full_model();
    let area = Rect::new(0, 0, 100, 30);
    let (tree, editor, results) = pane_rects(area);
    let palette = Palette::new(vec![PaletteEntry {
        label: "Run".into(),
        detail: "run the buffer".into(),
        group: "action",
        command: PaletteCommand::Run(crate::app::Action::RunBuffer),
    }]);
    let prompt = PasswordPrompt::new("orders", "the server asked");

    for presentation in [
        presentation_at(ColorDepth::Basic16),
        presentation_at(ColorDepth::None),
        colour_off_presentation(),
    ] {
        let buf = render_panes(&model, &presentation, area);
        assert_unpainted(&buf, inner(tree), "object tree");
        assert_unpainted(&buf, inner(editor), "editor");
        assert_unpainted(&buf, inner(results), "results");

        let mut buf = Buffer::empty(area);
        render_palette(&palette, &presentation, area, &mut buf);
        assert_unpainted(&buf, inner(palette_area(area)), "palette");

        let mut buf = Buffer::empty(area);
        render_password_prompt(&prompt, &presentation, area, &mut buf);
        assert_unpainted(&buf, inner(password_prompt_area(area)), "password prompt");
    }
}

#[test]
fn meaning_carrying_words_survive_every_colour_depth() {
    let mut model = full_model();
    model.password_prompt = Some(PasswordPrompt::new("orders", "the server asked"));
    let keymap = Keymap::new();

    let mut expected: Option<String> = None;
    for (label, presentation) in [
        ("truecolor", presentation_at(ColorDepth::TrueColor)),
        ("256", presentation_at(ColorDepth::Indexed256)),
        ("16", presentation_at(ColorDepth::Basic16)),
        ("none", presentation_at(ColorDepth::None)),
    ] {
        let text = crate::ui::layout::render_to_string(&model, &keymap, &presentation, 100, 30);
        match &expected {
            Some(first) => assert_eq!(first, &text, "the {label} frame changed the words"),
            None => expected = Some(text),
        }
    }
}

/// The rectangle inside a one-cell bordered pane.
fn inner(rect: Rect) -> Rect {
    Rect {
        x: rect.x + 1,
        y: rect.y + 1,
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    }
}

fn with_tree(model: &mut Model) {
    use crate::postgres::{ObjectKind, SchemaSummary};
    let mut counts = std::collections::BTreeMap::new();
    counts.insert(ObjectKind::Table, 2);
    model.tree.set_schemas(vec![SchemaSummary {
        name: "public".into(),
        usable: true,
        counts,
    }]);
    assert!(model.tree.expand_schema(&[0]));
}

fn grid_model(column_count: usize, row_count: usize) -> Model {
    let mut set = ResultSet::new(
        (0..column_count).map(|index| format!("c{index}")).collect(),
        100,
    );
    for row in 0..row_count {
        assert!(
            set.push(
                (0..column_count)
                    .map(|column| Cell::Text(format!("{row}-{column}")))
                    .collect()
            )
        );
    }
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.last_execution = Some(Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(row_count as u64),
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    model
}

#[test]
fn header_and_footer_own_their_rows_in_both_layouts() {
    let model = Model::new(100);
    let full = Rect::new(0, 0, 100, 30);
    assert_eq!(region_at(&model, full, 0, 0), Some(Region::Header));
    assert_eq!(region_at(&model, full, 99, 0), Some(Region::Header));
    assert_eq!(region_at(&model, full, 0, 29), Some(Region::Footer));
    assert_eq!(region_at(&model, full, 99, 29), Some(Region::Footer));

    let compact = Rect::new(0, 0, 60, 20);
    assert_eq!(region_at(&model, compact, 0, 0), Some(Region::Header));
    assert_eq!(region_at(&model, compact, 59, 0), Some(Region::Header));
    assert_eq!(region_at(&model, compact, 0, 19), Some(Region::Footer));
    assert_eq!(region_at(&model, compact, 59, 19), Some(Region::Footer));
}

#[test]
fn the_vertical_split_is_the_divider_between_tree_and_panes() {
    let model = Model::new(100);
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    assert_eq!(body, Rect::new(0, 1, 100, 28));
    let width = sidebar_width(body.width);
    assert_eq!(width, 34);
    let divider = width - 1;
    assert_eq!(
        region_at(&model, area, divider, body.y),
        Some(Region::VerticalSplit)
    );
    assert_eq!(
        region_at(&model, area, width, body.y + body.height - 1),
        Some(Region::VerticalSplit)
    );

    let mut hidden = Model::new(100);
    hidden.sidebar_visible = false;
    assert_ne!(
        region_at(&hidden, area, divider, body.y),
        Some(Region::VerticalSplit)
    );
}

#[test]
fn the_horizontal_split_is_where_editor_meets_results() {
    let model = Model::new(100);
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let (editor, results) = main_panes(body, true, &model.splits);
    let editor_bottom = editor.y + editor.height - 1;
    assert_eq!(
        region_at(&model, area, editor.x + 5, editor_bottom),
        Some(Region::HorizontalSplit)
    );
    assert_eq!(
        region_at(&model, area, results.x + 5, results.y),
        Some(Region::HorizontalSplit)
    );
}

#[test]
fn sidebar_rows_carry_the_visible_row_index() {
    let mut model = Model::new(100);
    with_tree(&mut model);
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let pane = Rect::new(0, body.y, sidebar_width(body.width), body.height);
    let inside = inner(pane);
    let rows = model.tree.rows();
    assert!(rows.len() >= 3, "the fixture expands into several rows");
    assert_eq!(
        region_at(&model, area, inside.x, inside.y),
        Some(Region::SidebarRow { index: 0 })
    );
    assert_eq!(
        region_at(&model, area, inside.x + 1, inside.y + 1),
        Some(Region::SidebarRow { index: 1 })
    );
    // The pane border is not a row.
    assert_eq!(region_at(&model, area, pane.x, inside.y), None);
}

#[test]
fn filtered_sidebar_rows_start_below_the_filter_line() {
    let mut model = Model::new(100);
    with_tree(&mut model);
    model.tree.filtering = true;
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let pane = Rect::new(0, body.y, sidebar_width(body.width), body.height);
    let inside = inner(pane);
    let rows = model.tree.rows();
    assert!(rows.len() >= 3, "the fixture expands into several rows");
    let last = u16::try_from(rows.len()).expect("a small fixture row count");

    // The filter line is not a row and selects nothing.
    assert_eq!(region_at(&model, area, inside.x, inside.y), None);
    // The first drawn row is the one below the filter line.
    assert_eq!(
        region_at(&model, area, inside.x, inside.y + 1),
        Some(Region::SidebarRow { index: 0 })
    );
    assert_eq!(
        region_at(&model, area, inside.x, inside.y + last),
        Some(Region::SidebarRow {
            index: rows.len() - 1
        })
    );
    // One row past the last drawn row is a miss.
    assert_eq!(region_at(&model, area, inside.x, inside.y + last + 1), None);
}

#[test]
fn compact_sidebar_rows_follow_the_tree_when_it_has_focus() {
    let mut model = Model::new(100);
    with_tree(&mut model);
    model.sidebar_visible = false;
    model.focus = Focus::Objects;
    let area = Rect::new(0, 0, 60, 20);
    let inside = inner(body_area(area));
    assert_eq!(
        region_at(&model, area, inside.x, inside.y),
        Some(Region::SidebarRow { index: 0 })
    );
    // With a filter open, the filter line still owns the first inner row.
    model.tree.filtering = true;
    assert_eq!(region_at(&model, area, inside.x, inside.y), None);
    assert_eq!(
        region_at(&model, area, inside.x, inside.y + 1),
        Some(Region::SidebarRow { index: 0 })
    );
}

#[test]
fn compact_geometry_still_reports_overlay_items() {
    let mut model = Model::new(100);
    model.palette = Some(Palette::new(vec![PaletteEntry {
        label: "Run".into(),
        detail: String::new(),
        group: "action",
        command: PaletteCommand::Run(crate::app::Action::RunBuffer),
    }]));
    let area = Rect::new(0, 0, 60, 20);
    let inside = inner(Rect::new(4, 2, 52, 14));
    assert_eq!(
        region_at(&model, area, inside.x, inside.y + 1),
        Some(Region::OverlayItem { index: 0 })
    );
    assert_eq!(region_at(&model, area, inside.x, inside.y), None);
}

#[test]
fn editor_cells_carry_line_and_column() {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.editor.set_text("SELECT 1;\nFROM t;");
    model.editor.set_cursor(0);
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let (editor, _) = main_panes(body, false, &model.splits);
    let inside = inner(editor);
    let text_start = inside.x + 4; // two-digit gutter, a space and the marker

    assert_eq!(
        region_at(&model, area, inside.x, inside.y),
        Some(Region::EditorGutter { line: 1 })
    );
    assert_eq!(
        region_at(&model, area, text_start - 1, inside.y),
        Some(Region::EditorGutter { line: 1 })
    );
    assert_eq!(
        region_at(&model, area, text_start, inside.y),
        Some(Region::EditorText { line: 1, column: 1 })
    );
    assert_eq!(
        region_at(&model, area, text_start + 6, inside.y),
        Some(Region::EditorText { line: 1, column: 7 })
    );
    assert_eq!(
        region_at(&model, area, text_start, inside.y + 1),
        Some(Region::EditorText { line: 2, column: 1 })
    );
    // One row past the last line is not a text position.
    assert_eq!(region_at(&model, area, text_start, inside.y + 2), None);
}

#[test]
fn compact_regions_follow_focus() {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.editor.set_text("SELECT 1;");
    model.focus = Focus::Editor;
    let area = Rect::new(0, 0, 60, 20);
    let inside = inner(body_area(area));
    let text_start = inside.x + 4;
    assert_eq!(
        region_at(&model, area, inside.x, inside.y),
        Some(Region::EditorGutter { line: 1 })
    );
    assert_eq!(
        region_at(&model, area, text_start, inside.y),
        Some(Region::EditorText { line: 1, column: 1 })
    );

    model.focus = Focus::Results;
    assert_eq!(region_at(&model, area, inside.x, inside.y), None);
}

#[test]
fn result_grid_cells_headers_and_edges_carry_source_indices() {
    let model = grid_model(2, 2);
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let (_, results) = main_panes(body, false, &model.splits);
    let inside = inner(results);
    let content_x = inside.x + 3; // two-digit gutter plus its space
    let header_y = inside.y + 3;
    // The fixture's values are shorter than the minimum width, so each column
    // is exactly `MIN_COLUMN_WIDTH` cells plus its one-cell padding. The rule
    // and the space after it separate two columns.
    let column_block =
        u16::try_from(crate::app::grid::MIN_COLUMN_WIDTH + 1).expect("a small column width");
    let second_x = content_x + column_block + 2;

    assert_eq!(
        region_at(&model, area, content_x, header_y),
        Some(Region::ResultsHeaderCell { column: 0 })
    );
    assert_eq!(
        region_at(&model, area, second_x, header_y),
        Some(Region::ResultsHeaderCell { column: 1 })
    );
    assert_eq!(
        region_at(&model, area, content_x + column_block, header_y),
        Some(Region::ColumnEdge { column: 0 })
    );
    assert_eq!(
        region_at(&model, area, content_x + column_block + 1, header_y),
        Some(Region::ColumnEdge { column: 0 })
    );

    let first_row_y = inside.y + 5;
    assert_eq!(
        region_at(&model, area, content_x, first_row_y),
        Some(Region::ResultsCell { row: 0, column: 0 })
    );
    assert_eq!(
        region_at(&model, area, second_x, first_row_y + 1),
        Some(Region::ResultsCell { row: 1, column: 1 })
    );
    // The row-number gutter is not a value cell.
    assert_eq!(region_at(&model, area, inside.x, first_row_y), None);
}

#[test]
fn an_overflowing_grid_exposes_the_scrollbar() {
    let model = grid_model(2, 40);
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let (_, results) = main_panes(body, false, &model.splits);
    let inside = inner(results);
    let scrollbar_x = inside.x + inside.width - 1;
    assert_eq!(
        region_at(&model, area, scrollbar_x, inside.y + 5),
        Some(Region::ResultsScrollbar)
    );
    // Above the data area the same column is a state line, not the scrollbar.
    assert_eq!(region_at(&model, area, scrollbar_x, inside.y), None);
}

#[test]
fn compact_results_use_the_whole_body() {
    let mut model = grid_model(1, 2);
    model.focus = Focus::Results;
    let area = Rect::new(0, 0, 60, 20);
    let inside = inner(body_area(area));
    let content_x = inside.x + 3;
    assert_eq!(
        region_at(&model, area, content_x, inside.y + 3),
        Some(Region::ResultsHeaderCell { column: 0 })
    );
    assert_eq!(
        region_at(&model, area, content_x, inside.y + 5),
        Some(Region::ResultsCell { row: 0, column: 0 })
    );
}

#[test]
fn borders_and_out_of_area_cells_are_misses() {
    let model = Model::new(100);
    let area = Rect::new(0, 0, 100, 30);
    assert_eq!(region_at(&model, area, 100, 0), None);
    assert_eq!(region_at(&model, area, 0, 30), None);

    let body = body_area(area);
    let (editor, _) = main_panes(body, true, &model.splits);
    // The editor's top border is not a text cell.
    assert_eq!(region_at(&model, area, editor.x + 1, editor.y), None);

    // Below the usable size nothing is a region.
    let tiny = Rect::new(0, 0, 30, 6);
    assert_eq!(region_at(&model, tiny, 0, 0), None);
}

#[test]
fn an_open_palette_reports_items_and_hides_the_panes_beneath() {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.editor.set_text("SELECT 1;");
    model.palette = Some(Palette::new(vec![
        PaletteEntry {
            label: "Run".into(),
            detail: String::new(),
            group: "action",
            command: PaletteCommand::Run(crate::app::Action::RunBuffer),
        },
        PaletteEntry {
            label: "Quit".into(),
            detail: String::new(),
            group: "action",
            command: PaletteCommand::Run(crate::app::Action::Quit),
        },
    ]));
    let area = Rect::new(0, 0, 100, 30);
    let inside = inner(Rect::new(12, 2, 76, 18));
    let first_item = inside.y + 1; // the query line sits above the items
    assert_eq!(
        region_at(&model, area, inside.x, first_item),
        Some(Region::OverlayItem { index: 0 })
    );
    assert_eq!(
        region_at(&model, area, inside.x, first_item + 1),
        Some(Region::OverlayItem { index: 1 })
    );
    // The query line is not an item.
    assert_eq!(region_at(&model, area, inside.x, inside.y), None);

    // A pane cell is owned by the overlay, not by the editor beneath it.
    let body = body_area(area);
    let (editor, _) = main_panes(body, false, &model.splits);
    let editor_inner = inner(editor);
    assert_eq!(
        region_at(&model, area, editor_inner.x, editor_inner.y),
        None
    );

    // A higher modal hides the palette as well.
    model.password_prompt = Some(PasswordPrompt::new("orders", "the server asked"));
    assert_eq!(region_at(&model, area, inside.x, first_item), None);
}

#[test]
fn an_open_help_overlay_hides_the_panes_beneath() {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.editor.set_text("SELECT 1;");
    model.help_open = true;
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let (editor, _) = main_panes(body, false, &model.splits);
    let editor_inner = inner(editor);
    assert_eq!(
        region_at(&model, area, editor_inner.x, editor_inner.y),
        None
    );
    assert_eq!(region_at(&model, area, 0, 0), None);
}

#[test]
fn an_open_notice_details_overlay_hides_the_panes_beneath() {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.editor.set_text("SELECT 1;");
    model.notice_details = Some(crate::app::model::NoticeDetails {
        snapshot: model.notice_state.snapshot(),
        scroll: 0,
    });
    let area = Rect::new(0, 0, 100, 30);
    let body = body_area(area);
    let (editor, _) = main_panes(body, false, &model.splits);
    let editor_inner = inner(editor);
    assert_eq!(
        region_at(&model, area, editor_inner.x, editor_inner.y),
        None
    );
    assert_eq!(region_at(&model, area, 0, 0), None);
}

fn editor_eol_probe(
    text: &str,
    cursor: usize,
    name: &str,
) -> (Model, Buffer, Rect, Vec<(u16, u16)>) {
    let area = Rect::new(0, 0, 100, 30);
    let mut model = Model::new(100);
    model.size = (area.width, area.height);
    model.focus = Focus::Editor;
    model.editor.set_text(text);
    model.editor.set_cursor(cursor);
    model.presentation = Presentation::new(
        Theme::new(ThemeChoice::HighContrast, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let pane = crate::ui::layout::pane_geometry(&model, area)
        .editor
        .unwrap();
    let inner = super::inner_rect(pane);
    let mut buf = Buffer::empty(area);
    crate::ui::render(&model, &Keymap::new(), area, &mut buf);
    let selection = model.presentation.theme.style(Token::Selection);
    let carets = (inner.y..inner.bottom())
        .flat_map(|y| (inner.x..inner.right()).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            selection.bg == Some(buf[(x, y)].bg) && selection.fg == Some(buf[(x, y)].fg)
        })
        .collect::<Vec<_>>();
    if let Some(dir) = std::env::var_os("IGNATIUS_EDITOR_REPRO_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cells = (area.y..area.bottom()).flat_map(|y| (area.x..area.right()).map(move |x| (x,y)))
            .map(|(x,y)| serde_json::json!({"x":x,"y":y,"symbol":buf[(x,y)].symbol(),"fg":format!("{:?}",buf[(x,y)].fg),"bg":format!("{:?}",buf[(x,y)].bg),"modifiers":buf[(x,y)].modifier.bits()})).collect::<Vec<_>>();
        std::fs::write(dir.join(format!("{name}.json")), serde_json::to_vec_pretty(&serde_json::json!({"kind":"SyntheticRatatuiBuffer","source":text,"cursor_byte":cursor,"scalar_position":model.editor.position(),"pane":[pane.x,pane.y,pane.width,pane.height],"carets":carets,"cells":cells})).unwrap()).unwrap();
    }
    assert_eq!(model.editor.cursor(), cursor, "source cursor remains valid");
    (model, buf, area, carets)
}

#[test]
fn editor_eol_probe_short_ascii_control() {
    let (model, _, _, carets) = editor_eol_probe("select 1", 8, "short-ascii");
    assert_eq!(model.editor.position(), (1, 9));
    assert_eq!(carets.len(), 1);
}

#[test]
fn editor_eol_probe_long_ascii_caret_visible() {
    let text = "a".repeat(160);
    let (_, _, _, carets) = editor_eol_probe(&text, text.len(), "long-ascii");
    assert_eq!(
        carets.len(),
        1,
        "valid end cursor must remain visible in actual 100x30 Buffer"
    );
}

fn editor_probe_hit_matches_caret(text: &str, cursor: usize, name: &str) {
    let (model, _, area, carets) = editor_eol_probe(text, cursor, name);
    let (x, y) = *carets.first().expect("short line caret is visible");
    let (line, column) = model.editor.position();
    assert_eq!(
        region_at(&model, area, x, y),
        Some(Region::EditorText { line, column }),
        "painted caret cell must hit the same source scalar"
    );
}

#[test]
fn editor_eol_probe_wide_hit_matches_caret() {
    editor_probe_hit_matches_caret("界ab", "界".len(), "wide");
}

#[test]
fn editor_eol_probe_combining_hit_matches_caret() {
    editor_probe_hit_matches_caret("e\u{301}X", "e\u{301}".len(), "combining");
}

#[test]
fn editor_eol_probe_control_hit_matches_caret() {
    editor_probe_hit_matches_caret("A\tB", 2, "control");
}

#[test]
fn editor_eol_probe_home_and_very_long_ascii_remain_visible() {
    let text = "a".repeat(70_000);
    let (_, _, _, end) = editor_eol_probe(&text, text.len(), "very-long-ascii");
    assert_eq!(end.len(), 1);
    let (model, buf, area, home) = editor_eol_probe(&text, 0, "long-ascii-home");
    assert_eq!(home.len(), 1);
    assert_eq!(
        region_at(&model, area, home[0].0, home[0].1),
        Some(Region::EditorText { line: 1, column: 1 })
    );
    assert_eq!(buf[home[0]].symbol(), "a");
}

#[test]
fn editor_eol_probe_cluster_interior_highlights_whole_unit() {
    for (name, text, cursor, symbol, width) in [
        ("combining-interior", "e\u{301}X", 1, "e\u{301}", 1),
        (
            "zwj-interior",
            "👩\u{200d}💻X",
            "👩".len(),
            "👩\u{200d}💻",
            2,
        ),
    ] {
        let (model, buf, area, carets) = editor_eol_probe(text, cursor, name);
        // Ratatui stores a wide grapheme at its leading cell and resets the
        // skipped continuation cell. The whole symbol must stay in that cell.
        assert_eq!(carets.len(), 1);
        assert_eq!(buf[carets[0]].symbol(), symbol);
        let (first_x, y) = carets[0];
        for x in first_x..first_x + width {
            assert_eq!(
                region_at(&model, area, x, y),
                Some(Region::EditorText { line: 1, column: 1 })
            );
        }
    }
}

#[test]
fn editor_eol_probe_tiny_capacity_and_degradation_keep_selected_cell() {
    for density in [
        crate::config::Density::Comfortable,
        crate::config::Density::Compact,
    ] {
        for glyph in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
            for depth in [
                ColorDepth::TrueColor,
                ColorDepth::Indexed256,
                ColorDepth::Basic16,
                ColorDepth::None,
            ] {
                let mut model = Model::new(100);
                model.focus = Focus::Editor;
                model.editor.set_text("界X");
                model.editor.set_cursor(0);
                model.presentation = Presentation::new(
                    Theme::new(ThemeChoice::HighContrast, depth != ColorDepth::None)
                        .with_depth(depth),
                    Glyphs::new(glyph),
                    true,
                )
                .with_density(density);
                let area = Rect::new(0, 0, 7, 3);
                let mut buf = Buffer::empty(area);
                render_editor(&model, &Keymap::new(), &model.presentation, area, &mut buf);
                if density == crate::config::Density::Comfortable
                    && glyph == GlyphTier::Unicode
                    && depth == ColorDepth::TrueColor
                {
                    editor_probe_capture_extra("tiny-one-text-cell", &model, &buf);
                }
                let cell = &buf[(5, 1)];
                assert_eq!(cell.symbol(), " ");
                let selection = model.presentation.theme.style(Token::Selection);
                assert!(cell.modifier.contains(selection.add_modifier));
                if let Some(bg) = selection.bg {
                    assert_eq!(cell.bg, bg);
                }
                if let Some(fg) = selection.fg {
                    assert_eq!(cell.fg, fg);
                }
                assert_eq!(
                    super::editor_region(&model, area, 5, 1),
                    Some(Region::EditorText { line: 1, column: 1 })
                );
            }
        }
    }
}

fn editor_probe_capture_extra(name: &str, model: &Model, buf: &Buffer) {
    if let Some(dir) = std::env::var_os("IGNATIUS_EDITOR_REPRO_DIR") {
        let cells=(buf.area.y..buf.area.bottom()).flat_map(|y| (buf.area.x..buf.area.right()).map(move |x|(x,y)))
            .map(|(x,y)| serde_json::json!({"x":x,"y":y,"symbol":buf[(x,y)].symbol(),"fg":format!("{:?}",buf[(x,y)].fg),"bg":format!("{:?}",buf[(x,y)].bg),"modifiers":buf[(x,y)].modifier.bits()})).collect::<Vec<_>>();
        std::fs::write(std::path::PathBuf::from(dir).join(format!("{name}.json")),serde_json::to_vec_pretty(&serde_json::json!({"kind":"SyntheticRatatuiBuffer","source":model.editor.text(),"cursor_byte":model.editor.cursor(),"area":[buf.area.x,buf.area.y,buf.area.width,buf.area.height],"cells":cells})).unwrap()).unwrap();
    }
}

#[test]
fn editor_eol_probe_completion_anchor_uses_display_cells_and_no_text_capacity_is_noop() {
    let mut anchors = Vec::new();
    for (name, text) in [
        ("completion-wide", "界界界SEL"),
        ("completion-ascii", "aaaaaaSEL"),
    ] {
        let (mut model, _, area, _) = editor_eol_probe(text, text.len(), name);
        model.completion.menu = Some(crate::app::completion::CompletionMenu::new(
            crate::query::completion::complete("SEL", 3, None),
            true,
        ));
        let mut buf = Buffer::empty(area);
        crate::ui::render(&model, &Keymap::new(), area, &mut buf);
        let title = (area.y..area.bottom())
            .find_map(|y| {
                let text = (area.x..area.right())
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>();
                text.find("Complete")
                    .map(|byte| (text[..byte].chars().count(), y))
            })
            .expect("completion title");
        anchors.push(title);
        editor_probe_capture_extra(&format!("{name}-popup"), &model, &buf);
        model.sidebar_visible = false;
        model.splits.zoomed = true;
        // A deliberately very large line-count gutter consumes all available
        // text cells only in a direct narrow pane; render_completion uses the
        // actual shared pane geometry and refuses its zero-capacity case.
        let small = Rect::new(0, 0, 6, 3);
        let mut tinybuf = Buffer::empty(small);
        render_editor(
            &model,
            &Keymap::new(),
            &model.presentation,
            small,
            &mut tinybuf,
        );
        editor_probe_capture_extra("tiny-no-text-capacity", &model, &tinybuf);
        assert!(
            (1..5).all(|x| tinybuf[(x, 1)].bg
                != model.presentation.theme.style(Token::Selection).bg.unwrap())
        );
        let before = tinybuf.clone();
        super::render_completion(&model, &model.presentation, small, &mut tinybuf);
        assert_eq!(tinybuf, before, "no text capacity must not draw a popup");
    }
    assert_eq!(
        anchors[0], anchors[1],
        "equal display-cell carets need equal popup anchors"
    );
}

#[test]
fn editor_eol_probe_cluster_error_uses_any_source_byte_and_caret_wins() {
    let (mut model, _, area, carets) = editor_eol_probe("e\u{301}X", 1, "cluster-error-setup");
    let point = carets[0];
    model.error_location = Some(crate::query::error_location::ErrorLocation {
        cursor: 1,
        token: Some(1..3),
        line: 1,
        column: 2,
        statement_number: 1,
        character: 2,
    });
    let mut buf = Buffer::empty(area);
    crate::ui::render(&model, &Keymap::new(), area, &mut buf);
    assert_eq!(buf[point].symbol(), "e\u{301}");
    assert_eq!(
        Some(buf[point].bg),
        model.presentation.theme.style(Token::Selection).bg
    );
    model.focus = Focus::Results;
    crate::ui::render(&model, &Keymap::new(), area, &mut buf);
    assert_eq!(
        Some(buf[point].fg),
        model.presentation.theme.style(Token::Danger).fg
    );
    editor_probe_capture_extra("cluster-error-unfocused", &model, &buf);
}

#[test]
fn editor_eol_probe_unfocused_zero_width_maps_painted_following_unit() {
    let (mut model, _, area, carets) = editor_eol_probe("\u{301}X", 0, "zero-width-focused");
    let point = carets[0];
    assert_eq!(
        region_at(&model, area, point.0, point.1),
        Some(Region::EditorText { line: 1, column: 1 })
    );
    model.focus = Focus::Results;
    let mut buf = Buffer::empty(area);
    crate::ui::render(&model, &Keymap::new(), area, &mut buf);
    assert_eq!(
        buf[point].symbol(),
        "X",
        "unfocused rendering exposes the following source unit"
    );
    editor_probe_capture_extra("zero-width-unfocused", &model, &buf);
    assert_eq!(
        region_at(&model, area, point.0, point.1),
        Some(Region::EditorText { line: 1, column: 2 }),
        "hit must follow painted X when no caret blank is painted"
    );
}

#[test]
fn editor_eol_probe_oversized_escape_blank_retains_source_mapping() {
    for text in ["\tX", "\u{009b}X"] {
        let mut model = Model::new(100);
        model.focus = Focus::Editor;
        model.editor.set_text(text);
        model.editor.set_cursor(0);
        model.presentation = Presentation::new(
            Theme::new(ThemeChoice::HighContrast, true),
            Glyphs::new(GlyphTier::Unicode),
            true,
        );
        let pane = Rect::new(0, 0, 7, 3);
        let mut buf = Buffer::empty(pane);
        render_editor(&model, &Keymap::new(), &model.presentation, pane, &mut buf);
        assert_eq!(buf[(5, 1)].symbol(), " ");
        assert_eq!(
            Some(buf[(5, 1)].bg),
            model.presentation.theme.style(Token::Selection).bg
        );
        assert_eq!(
            super::editor_region(&model, pane, 5, 1),
            Some(Region::EditorText { line: 1, column: 1 })
        );
        editor_probe_capture_extra(
            if text.starts_with('\t') {
                "oversized-tab-blank"
            } else {
                "oversized-c1-blank"
            },
            &model,
            &buf,
        );
    }
}
