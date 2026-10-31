pub mod banner;
pub mod editor;
pub mod footer;
pub mod header;
pub mod objects;
pub mod overlays;
pub mod palette;
pub mod results;

pub(crate) use banner::render_banner;
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

#[cfg(test)]
use crate::app::model::Focus;
use crate::app::model::Model;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{LayoutMode, Presentation, layout_mode, pane_geometry};
use crate::ui::theme::Token;
use ratatui::layout::Rect;
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
    // Existing callers own at most one decorative edge space. Removing that
    // wrapper, rather than trimming, preserves whitespace in title content.
    let content = title.strip_prefix(' ').unwrap_or(&title);
    let content = content.strip_suffix(' ').unwrap_or(content);
    let padding = " ".repeat(usize::from(presentation.spacing().title_padding));
    let title = format!("{padding}{content}{padding}");
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
        return palette_item_at(model, palette, area, column, row);
    }
    if model.definition.is_some() || model.inspector.is_some() || model.help_open {
        return None;
    }
    if model.completion.menu.is_some() {
        return None;
    }

    let geometry = pane_geometry(model, area);
    if layout_mode(area) == LayoutMode::TooSmall {
        return None;
    }
    if in_rect(geometry.header, column, row) {
        return Some(Region::Header);
    }
    if in_rect(geometry.footer, column, row) {
        return Some(Region::Footer);
    }
    if geometry
        .vertical_split
        .is_some_and(|rect| in_rect(rect, column, row))
    {
        return Some(Region::VerticalSplit);
    }
    if geometry
        .horizontal_split
        .is_some_and(|rect| in_rect(rect, column, row))
    {
        return Some(Region::HorizontalSplit);
    }
    if let Some(tree) = geometry.objects.filter(|rect| in_rect(*rect, column, row)) {
        return sidebar_row_region(model, tree, column, row);
    }
    if let Some(editor) = geometry.editor.filter(|rect| in_rect(*rect, column, row)) {
        return editor_region(model, editor, column, row);
    }
    if let Some(results) = geometry.results.filter(|rect| in_rect(*rect, column, row)) {
        return results_region(model, results, column, row);
    }
    None
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
    let body = objects::content_area(model, inner);
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
    let capacity = usize::from(inner.width).saturating_sub(gutter + 2);
    if capacity == 0 {
        return None;
    }
    let viewport = editor::view::viewport(&model.editor, capacity);
    let cell = usize::from(column.saturating_sub(text_start));
    let line_text = model
        .editor
        .text()
        .split('\n')
        .nth(line.saturating_sub(1))
        .unwrap_or("");
    let column = if model.focus == crate::app::model::Focus::Editor
        && line == cursor_line
        && viewport.blank_caret
        && cell == viewport.caret_cell
    {
        viewport.caret_column
    } else {
        editor::view::LineView::project(line_text, viewport.offset.saturating_add(cell), 1, None)
            .column_at(viewport.offset.saturating_add(cell))
    };
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
    let set = model.visible_result().filter(|set| set.is_available())?;
    let inner = inner_rect(pane);
    if !in_rect(inner, column, row) || inner.width == 0 || inner.height == 0 {
        return None;
    }

    let rows = model.displayed_rows();
    let header_rows = results::grid_header_rows(model, set, model.presentation.spacing(), inner);
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
    model: &Model,
    palette: &crate::app::palette::Palette,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<Region> {
    palette::hit_row(palette, &model.presentation, area, column, row)
        .map(|index| Region::OverlayItem { index })
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
