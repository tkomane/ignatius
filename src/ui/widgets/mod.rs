pub mod editor;
pub mod footer;
pub mod header;
pub mod objects;
pub mod overlays;
pub mod palette;
pub mod results;

pub(crate) use editor::{render_completion, render_editor};
pub(crate) use footer::render_footer;
pub(crate) use header::{render_compact_header, render_header};
pub(crate) use objects::render_objects;
pub(crate) use overlays::{
    render_chords, render_confirmation, render_copy_confirmation, render_definition, render_help,
    render_name_prompt, render_parameter_prompt, render_password_prompt, render_plan_confirmation,
    render_update_review, render_update_value_prompt,
};
pub(crate) use palette::{render_connection_details, render_palette};
pub(crate) use results::{render_error, render_inspector, render_results};

use crate::app::model::{Focus, Model};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{
    LayoutMode, Presentation, body_area, layout_mode, main_panes, sidebar_width,
};
use crate::ui::theme::Token;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Span;
use ratatui::widgets::Block;

/// A visible text cursor that belongs to the selected glyph tier.
pub(crate) fn input_cursor(presentation: &Presentation) -> &'static str {
    if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "\u{2588}"
    }
}
/// Returns the active key for a discovery prompt, or a plain action label when
/// a test model has not been attached to a runtime keymap yet.
pub(crate) fn action_key_label(keymap: &Keymap, action: &crate::app::Action) -> String {
    keymap
        .contextual_hint(action)
        .map(|(key, _)| key)
        .unwrap_or_else(|| "the command palette".to_owned())
}
/// The theme token that draws a syntax kind.
pub(crate) const fn syntax_token(kind: crate::query::highlight::TokenKind) -> Token {
    use crate::query::highlight::TokenKind as Kind;
    match kind {
        Kind::Keyword => Token::SyntaxKeyword,
        Kind::Literal => Token::SyntaxLiteral,
        Kind::Number => Token::SyntaxNumber,
        Kind::Comment => Token::SyntaxComment,
        Kind::Identifier => Token::SyntaxIdentifier,
        Kind::Plain => Token::Text,
    }
}
/// The shared bordered block behind a pane.
///
/// It carries the pane elevation as its fill, so every pane interior is painted
/// from the theme rather than inheriting the terminal's background. At
/// 16-colour depth and below the theme returns an empty surface style and
/// nothing is painted, which is the documented colour-off behaviour.
pub(crate) fn pane_block(
    title: String,
    focused: bool,
    presentation: &Presentation,
) -> Block<'static> {
    bordered_block(title, focused, presentation)
        .style(presentation.theme.surface(Token::SurfacePane))
}

/// The shared bordered block behind an overlay.
///
/// An overlay is one elevation above a pane, so it paints the overlay surface.
/// Callers clear the rectangle first, then draw this block over it, so no stale
/// cell shows through the frame.
pub(crate) fn overlay_block(title: String, presentation: &Presentation) -> Block<'static> {
    bordered_block(title, true, presentation)
        .style(presentation.theme.surface(Token::SurfaceOverlay))
}

/// The border, title and focus styling shared by panes and overlays.
fn bordered_block(title: String, focused: bool, presentation: &Presentation) -> Block<'static> {
    let theme = &presentation.theme;
    Block::bordered()
        .border_set(presentation.glyphs.borders(focused))
        .border_style(theme.style(if focused { Token::Focus } else { Token::Border }))
        .title(Span::styled(
            title,
            if focused {
                theme.style(Token::Focus)
            } else {
                theme.style(Token::Muted)
            },
        ))
}

// ------------------------------------------------------------------- regions

/// What sits under a screen cell, for mouse dispatch.
///
/// Every position is derived from the same layout arithmetic the renderers use,
/// so a hit region can never disagree with what was drawn. The carried indices
/// are the ones a mouse verb needs to act on the same row, column or line the
/// keyboard would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The status header row.
    Header,
    /// The status footer row.
    Footer,
    /// One row of the object tree.
    SidebarRow {
        /// Zero-based index within the tree's visible rows.
        index: usize,
    },
    /// A position in the editor text.
    EditorText {
        /// One-based document line.
        line: usize,
        /// One-based document column.
        column: usize,
    },
    /// The line-number gutter beside the editor text.
    EditorGutter {
        /// One-based document line.
        line: usize,
    },
    /// A result column header cell.
    ResultsHeaderCell {
        /// Zero-based source column.
        column: usize,
    },
    /// A result value cell.
    ResultsCell {
        /// Zero-based position within the displayed rows.
        row: usize,
        /// Zero-based source column.
        column: usize,
    },
    /// The result grid scrollbar.
    ResultsScrollbar,
    /// The divider between the object tree and the main panes.
    VerticalSplit,
    /// The divider between the editor and the results.
    HorizontalSplit,
    /// One item of an overlay list.
    OverlayItem {
        /// Zero-based item index.
        index: usize,
    },
    /// The rule cell between two result columns, for resizing.
    ColumnEdge {
        /// Zero-based source column to the left of the edge.
        column: usize,
    },
}

/// Resolves a screen cell to the region under it.
///
/// Returns `None` for a border, blank space, the wrong layout, or any position
/// an open overlay owns, so a click can never fall through to a pane the user
/// cannot see.
#[must_use]
pub fn region_at(model: &Model, area: Rect, column: u16, row: u16) -> Option<Region> {
    if !in_rect(area, column, row) {
        return None;
    }

    // Overlays own the frame they are painted over. They are checked topmost
    // first, in the same order `layout::render` draws them, so a click resolves
    // to the region a user can actually see.
    if model.password_prompt.is_some()
        || model.name_prompt.is_some()
        || model.parameter_prompt.is_some()
        || model.update_prompt.is_some()
        || model.pending_update.is_some()
        || model.pending_copy.is_some()
        || model.pending_plan.is_some()
        || model.pending_run.is_some()
        || model.prefix_pending
        || model.connection_details
    {
        return None;
    }
    if let Some(palette) = &model.palette {
        return palette_item_at(palette, area, column, row);
    }
    if model.definition.is_some() || model.inspector.is_some() || model.help_open {
        return None;
    }
    if model.completion.menu.is_some() {
        return None;
    }

    match layout_mode(area) {
        LayoutMode::TooSmall => None,
        LayoutMode::Full => full_region_at(model, area, column, row),
        LayoutMode::Compact => compact_region_at(model, area, column, row),
    }
}

/// The regions of the two-pane full layout.
fn full_region_at(model: &Model, area: Rect, column: u16, row: u16) -> Option<Region> {
    if row == area.y {
        return Some(Region::Header);
    }
    if row == area.y.saturating_add(area.height).saturating_sub(1) {
        return Some(Region::Footer);
    }
    let body = body_area(area);

    let mut sidebar = None;
    let mut main = body;
    if model.sidebar_visible {
        let width = sidebar_width(body.width);
        let [tree, panes] =
            Layout::horizontal([Constraint::Length(width), Constraint::Min(30)]).areas(body);
        sidebar = Some(tree);
        main = panes;
    }
    let (editor, results) = main_panes(body, model.sidebar_visible);

    // The divider columns exist only when both panes are on screen.
    if let Some(tree) = sidebar
        && in_rect(body, column, row)
    {
        let divider = tree.x.saturating_add(tree.width).saturating_sub(1);
        if column == divider || column == main.x {
            return Some(Region::VerticalSplit);
        }
    }

    // The two border rows where the editor meets the results.
    let editor_bottom = editor.y.saturating_add(editor.height).saturating_sub(1);
    let results_top = editor.y.saturating_add(editor.height);
    if in_rect(main, column, row) && (row == editor_bottom || row == results_top) {
        return Some(Region::HorizontalSplit);
    }

    if let Some(tree) = sidebar
        && in_rect(tree, column, row)
    {
        return sidebar_row_region(model, tree, column, row);
    }
    if in_rect(editor, column, row) {
        return editor_region(model, editor, column, row);
    }
    if in_rect(results, column, row) {
        return results_region(model, results, column, row);
    }
    None
}

/// The regions of the single-pane compact layout.
fn compact_region_at(model: &Model, area: Rect, column: u16, row: u16) -> Option<Region> {
    if row == area.y {
        return Some(Region::Header);
    }
    if row == area.y.saturating_add(area.height).saturating_sub(1) {
        return Some(Region::Footer);
    }
    let body = body_area(area);

    if model.transaction == crate::query::result::TransactionState::Failed {
        return results_region(model, body, column, row);
    }
    if model.error.is_some() {
        // An error keeps the editor below it, mirroring the compact renderer.
        let error_height = body.height.saturating_sub(4).clamp(2, 8);
        let [_, editor_area] =
            Layout::vertical([Constraint::Length(error_height), Constraint::Min(1)]).areas(body);
        return editor_region(model, editor_area, column, row);
    }
    match model.focus {
        Focus::Editor => editor_region(model, body, column, row),
        Focus::Results => results_region(model, body, column, row),
        Focus::Objects => sidebar_row_region(model, body, column, row),
    }
}

/// Maps a cell inside the object tree to the row it selects.
fn sidebar_row_region(model: &Model, pane: Rect, column: u16, row: u16) -> Option<Region> {
    let inner = inner_rect(pane);
    if !in_rect(inner, column, row) {
        return None;
    }
    if model.tree.error.is_some() || (model.tree.loading && model.tree.roots.is_empty()) {
        return None;
    }
    let rows = model.tree.rows();
    if rows.is_empty() {
        return None;
    }
    // The filter sits on its own row above the rows, exactly as it is drawn.
    let body = if model.tree.filtering || !model.tree.filter.is_empty() {
        Rect {
            y: inner.y.saturating_add(1),
            height: inner.height.saturating_sub(1),
            ..inner
        }
    } else {
        inner
    };
    if !in_rect(body, column, row) {
        return None;
    }
    let visible = body.height as usize;
    let offset = model
        .tree
        .selected
        .saturating_sub(visible.saturating_sub(1))
        .min(rows.len().saturating_sub(1));
    let index = offset.saturating_add(usize::from(row.saturating_sub(body.y)));
    if index >= rows.len() {
        return None;
    }
    Some(Region::SidebarRow { index })
}

/// Maps a cell inside the editor pane to a gutter line or a text position.
fn editor_region(model: &Model, editor: Rect, column: u16, row: u16) -> Option<Region> {
    let inner = inner_rect(editor);
    if !in_rect(inner, column, row) {
        return None;
    }
    let line_count = model.editor.line_count();
    let gutter = line_count.to_string().len().max(2);
    // The renderer draws the right-aligned number, a space, a statement marker
    // and then the text, so text starts this far in from the pane border.
    let text_start = inner
        .x
        .saturating_add(u16::try_from(gutter.saturating_add(2)).unwrap_or(u16::MAX));
    let (cursor_line, _) = model.editor.position();
    let height = inner.height as usize;
    let offset = cursor_line
        .saturating_sub(1)
        .saturating_sub(height.saturating_sub(1));
    let line = offset
        .saturating_add(usize::from(row.saturating_sub(inner.y)))
        .saturating_add(1);
    if line > line_count {
        return None;
    }
    if column < text_start {
        return Some(Region::EditorGutter { line });
    }
    let requested = usize::from(column.saturating_sub(text_start)).saturating_add(1);
    let line_text = model
        .editor
        .text()
        .split('\n')
        .nth(line.saturating_sub(1))
        .unwrap_or("");
    let column = requested.min(line_text.chars().count().saturating_add(1));
    Some(Region::EditorText { line, column })
}

/// Maps a cell inside the results pane to a header, value cell or scrollbar.
fn results_region(model: &Model, pane: Rect, column: u16, row: u16) -> Option<Region> {
    if model.plan.is_visible()
        || model.expanded_row
        || model.transaction == crate::query::result::TransactionState::Failed
        || model.error.is_some()
    {
        return None;
    }
    let set = model.visible_result()?;
    let inner = inner_rect(pane);
    if !in_rect(inner, column, row) || inner.width == 0 || inner.height == 0 {
        return None;
    }

    let rows = model.displayed_rows();
    let has_type_row = model.result_grid.show_types && set.column_types.iter().any(Option::is_some);
    let header_rows = 5 + usize::from(has_type_row);
    let visible_rows = (inner.height as usize).saturating_sub(header_rows).max(1);
    let needs_scrollbar = rows.len() > visible_rows;
    let grid_width = (inner.width as usize).saturating_sub(usize::from(needs_scrollbar));

    if needs_scrollbar
        && column == inner.x.saturating_add(inner.width).saturating_sub(1)
        && usize::from(row.saturating_sub(inner.y)) >= header_rows
    {
        return Some(Region::ResultsScrollbar);
    }

    let gutter = set.rows.len().max(1).to_string().len().max(2);
    let available = grid_width.saturating_sub(gutter + 1);
    let automatic_widths: Vec<usize> = (0..set.columns.len())
        .map(|index| crate::app::grid::automatic_column_width(set, index))
        .collect();
    let widths: Vec<usize> = automatic_widths
        .iter()
        .enumerate()
        .map(|(index, automatic)| model.result_grid.width_for(index, *automatic))
        .collect();
    let visible_columns = model.result_grid.visible_columns(set.columns.len());
    let selected_column = visible_columns
        .iter()
        .copied()
        .find(|index| *index == model.selected_column)
        .or_else(|| visible_columns.first().copied());
    let window = crate::app::grid::column_window(
        &visible_columns,
        selected_column.unwrap_or(0),
        &widths,
        available,
        model.result_grid.freeze_first,
        model.result_grid.horizontal_start,
    );
    let content_x = inner
        .x
        .saturating_add(u16::try_from(gutter.saturating_add(1)).unwrap_or(u16::MAX));

    let top = usize::from(row.saturating_sub(inner.y));
    if top == 3 {
        if let Some(column) = column_at(&window, &widths, content_x, column) {
            return Some(Region::ResultsHeaderCell { column });
        }
        if let Some(column) = column_edge_at(&window, &widths, content_x, column) {
            return Some(Region::ColumnEdge { column });
        }
        return None;
    }
    if top < header_rows {
        return None;
    }
    let position = model
        .selected_row
        .saturating_sub(visible_rows.saturating_sub(1))
        .saturating_add(top.saturating_sub(header_rows));
    if position >= rows.len() {
        return None;
    }
    column_at(&window, &widths, content_x, column).map(|column| Region::ResultsCell {
        row: position,
        column,
    })
}

/// Maps a cell inside an overlay list to its item.
fn palette_item_at(
    palette: &crate::app::palette::Palette,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<Region> {
    let width = area.width.saturating_sub(8).min(76);
    let height = area.height.saturating_sub(6).min(18);
    let palette_area = Rect {
        x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
        y: area.y.saturating_add(2),
        width,
        height,
    };
    let inner = inner_rect(palette_area);
    if !in_rect(inner, column, row) {
        return None;
    }
    let matches = palette.matches();
    let note_lines = palette
        .context_note
        .as_deref()
        .map_or(0, |note| note.split(". ").count());
    let visible = (height as usize).saturating_sub(3 + note_lines).max(1);
    let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
    let first_item = inner
        .y
        .saturating_add(1)
        .saturating_add(u16::try_from(note_lines).unwrap_or(u16::MAX));
    if row < first_item {
        return None;
    }
    let position = usize::from(row.saturating_sub(first_item));
    if position >= visible {
        return None;
    }
    let index = offset.saturating_add(position);
    if index >= matches.len() {
        return None;
    }
    Some(Region::OverlayItem { index })
}

/// The source column whose cell contains a screen column, if any.
fn column_at(window: &[usize], widths: &[usize], content_x: u16, column: u16) -> Option<usize> {
    let mut x = content_x;
    for (position, index) in window.iter().enumerate() {
        if position > 0 {
            // The rule and the space after it separate two columns.
            if column == x || column == x.saturating_add(1) {
                return None;
            }
            x = x.saturating_add(2);
        }
        let width = widths
            .get(*index)
            .copied()
            .unwrap_or(crate::app::grid::MIN_COLUMN_WIDTH);
        let block = u16::try_from(width.saturating_add(1)).unwrap_or(u16::MAX);
        if column >= x && column < x.saturating_add(block) {
            return Some(*index);
        }
        x = x.saturating_add(block);
    }
    None
}

/// The source column to the left of the rule cell, for resizing.
fn column_edge_at(
    window: &[usize],
    widths: &[usize],
    content_x: u16,
    column: u16,
) -> Option<usize> {
    let mut x = content_x;
    let mut left = None;
    for (position, index) in window.iter().enumerate() {
        if position > 0 {
            if column == x || column == x.saturating_add(1) {
                return left;
            }
            x = x.saturating_add(2);
        }
        let width = widths
            .get(*index)
            .copied()
            .unwrap_or(crate::app::grid::MIN_COLUMN_WIDTH);
        x = x.saturating_add(u16::try_from(width.saturating_add(1)).unwrap_or(u16::MAX));
        left = Some(*index);
    }
    None
}

/// Whether a screen cell lies inside a rectangle.
fn in_rect(rect: Rect, column: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

/// The rectangle inside a one-cell bordered pane.
const fn inner_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x.saturating_add(1),
        y: rect.y.saturating_add(1),
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    }
}

#[cfg(test)]
mod tests;
