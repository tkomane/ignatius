//! Focused tests for the pure hit-region function.
//!
//! Each expectation is the screen cell a renderer paints, so a region can only
//! pass these tests if it agrees with the drawing arithmetic.

use super::{Focus, Region, region_at};
use crate::app::model::{Model, PasswordPrompt};
use crate::app::palette::{Palette, PaletteCommand, PaletteEntry};
use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
use crate::query::value::Cell;
use crate::ui::layout::{body_area, main_panes, sidebar_width};
use ratatui::layout::Rect;
use std::time::Duration;

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
    let (editor, results) = main_panes(body, true);
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
    let (editor, _) = main_panes(body, false);
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
    let (_, results) = main_panes(body, false);
    let inside = inner(results);
    let content_x = inside.x + 3; // two-digit gutter plus its space
    let header_y = inside.y + 3;
    let second_x = content_x + 6; // first column block (4) and its rule (2)

    assert_eq!(
        region_at(&model, area, content_x, header_y),
        Some(Region::ResultsHeaderCell { column: 0 })
    );
    assert_eq!(
        region_at(&model, area, second_x, header_y),
        Some(Region::ResultsHeaderCell { column: 1 })
    );
    assert_eq!(
        region_at(&model, area, content_x + 4, header_y),
        Some(Region::ColumnEdge { column: 0 })
    );
    assert_eq!(
        region_at(&model, area, content_x + 5, header_y),
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
    let (_, results) = main_panes(body, false);
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
    let (editor, _) = main_panes(body, true);
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
    let (editor, _) = main_panes(body, false);
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
    let (editor, _) = main_panes(body, false);
    let editor_inner = inner(editor);
    assert_eq!(
        region_at(&model, area, editor_inner.x, editor_inner.y),
        None
    );
    assert_eq!(region_at(&model, area, 0, 0), None);
}
