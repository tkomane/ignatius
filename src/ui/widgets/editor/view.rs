//! Cell-bounded projection shared by editor painting, completion and hit testing.

use crate::app::editor::Editor;
use crate::query::value::{display_width, sanitize_for_display};
use ratatui::style::Style;
use ratatui::text::Span;
use std::borrow::Cow;
use std::ops::Range;

/// One indivisible renderer grapheme or complete escaped source control.
#[derive(Debug)]
pub(crate) struct Unit<'a> {
    pub text: Cow<'a, str>,
    pub source: Range<usize>,
    pub column: usize,
    pub cell: usize,
    pub width: usize,
}

impl Unit<'_> {
    /// Returns the visible text and relative cell position, blanking cut units.
    pub fn clipped(&self, offset: usize, capacity: usize) -> Option<(usize, String)> {
        let first = self.cell.max(offset);
        let end = self
            .cell
            .saturating_add(self.width)
            .min(offset.saturating_add(capacity));
        if first >= end {
            return None;
        }
        let text = if first == self.cell && end == self.cell.saturating_add(self.width) {
            self.text.to_string()
        } else {
            " ".repeat(end - first)
        };
        Some((first - offset, text))
    }
}

/// Visits source units without retaining off-screen metadata.
fn scan<'a>(source: &'a str, mut emit: impl FnMut(Unit<'a>)) -> (usize, usize) {
    let safe = sanitize_for_display(source);
    let mut cell = 0;
    if safe == source {
        // The existing sanitizer proved that this ASCII contains no controls.
        // Each remaining byte is one renderer grapheme and one display cell.
        if source.is_ascii() {
            for byte in 0..source.len() {
                emit(Unit {
                    text: Cow::Borrowed(&source[byte..byte + 1]),
                    source: byte..byte + 1,
                    column: byte + 1,
                    cell: byte,
                    width: 1,
                });
            }
            return (source.len(), source.len().saturating_add(1));
        }
        let end_column = graphemes(source, 0, 1, &mut cell, &mut emit);
        return (cell, end_column);
    }
    let mut normal_start = 0;
    let mut normal_column = 1;
    let mut column = 1;
    for (byte, ch) in source.char_indices() {
        let original = ch.to_string();
        let safe = sanitize_for_display(&original);
        if safe != original {
            graphemes(
                &source[normal_start..byte],
                normal_start,
                normal_column,
                &mut cell,
                &mut emit,
            );
            let width = display_width(&safe);
            emit(Unit {
                text: Cow::Owned(safe),
                source: byte..byte + ch.len_utf8(),
                column,
                cell,
                width,
            });
            cell = cell.saturating_add(width);
            normal_start = byte + ch.len_utf8();
            normal_column = column + 1;
        }
        column += 1;
    }
    graphemes(
        &source[normal_start..],
        normal_start,
        normal_column,
        &mut cell,
        &mut emit,
    );
    (cell, column)
}

fn graphemes<'a>(
    text: &'a str,
    source_start: usize,
    mut column: usize,
    cell: &mut usize,
    emit: &mut impl FnMut(Unit<'a>),
) -> usize {
    let span = Span::raw(text);
    let mut byte = 0;
    for grapheme in span.styled_graphemes(Style::default()) {
        let end = byte + grapheme.symbol.len();
        let width = display_width(grapheme.symbol);
        emit(Unit {
            text: Cow::Borrowed(&text[byte..end]),
            source: source_start + byte..source_start + end,
            column,
            cell: *cell,
            width,
        });
        *cell = cell.saturating_add(width);
        column += grapheme.symbol.chars().count();
        byte = end;
    }
    column
}

#[derive(Debug)]
pub(crate) struct LineView<'a> {
    pub units: Vec<Unit<'a>>,
    pub width: usize,
    pub end_column: usize,
}

impl<'a> LineView<'a> {
    /// Retains only intersecting positive-width units and at most one caret unit.
    pub fn project(source: &'a str, offset: usize, capacity: usize, cursor: Option<usize>) -> Self {
        let mut units = Vec::new();
        let right = offset.saturating_add(capacity);
        let (width, end_column) = scan(source, |unit| {
            let visible = capacity > 0
                && unit.width > 0
                && unit.cell < right
                && offset < unit.cell.saturating_add(unit.width);
            if visible || cursor.is_some_and(|cursor| unit.source.contains(&cursor)) {
                units.push(unit);
            }
        });
        Self {
            units,
            width,
            end_column,
        }
    }

    /// Full small-fixture construction is test-only; production projects a window.
    #[cfg(test)]
    pub fn new(source: &'a str) -> Self {
        Self::project(source, 0, usize::MAX, Some(0))
    }

    pub fn column_at(&self, cell: usize) -> usize {
        if cell >= self.width {
            return self.end_column;
        }
        self.units
            .iter()
            .find(|unit| unit.cell <= cell && cell < unit.cell.saturating_add(unit.width))
            .map_or(self.end_column, |unit| unit.column)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Viewport {
    pub offset: usize,
    pub caret_cell: usize,
    pub blank_caret: bool,
    /// First source scalar of the caret's displayed unit, even for a blank.
    pub caret_column: usize,
}

impl Viewport {
    fn from_parts(cell: usize, width: usize, column: usize, capacity: usize) -> Self {
        let blank_caret = width == 0 || width > capacity;
        let shown = if blank_caret { 1 } else { width };
        let offset = cell.saturating_add(shown).saturating_sub(capacity);
        Self {
            offset,
            caret_cell: cell.saturating_sub(offset),
            blank_caret,
            caret_column: column,
        }
    }

    /// Finds the caret in one scan, retaining only three scalar facts.
    pub fn for_source(source: &str, cursor: usize, capacity: usize) -> Self {
        let mut caret = None;
        let (width, end_column) = scan(source, |unit| {
            if unit.source.contains(&cursor) {
                caret = Some((unit.cell, unit.width, unit.column));
            }
        });
        let (cell, width, column) = caret.unwrap_or((width, 0, end_column));
        Self::from_parts(cell, width, column, capacity)
    }

    #[cfg(test)]
    pub fn new(line: &LineView<'_>, cursor: usize, capacity: usize) -> Self {
        let unit = line.units.iter().find(|unit| unit.source.contains(&cursor));
        Self::from_parts(
            unit.map_or(line.width, |unit| unit.cell),
            unit.map_or(0, |unit| unit.width),
            unit.map_or(line.end_column, |unit| unit.column),
            capacity,
        )
    }
}

/// Current source line and its relative UTF-8 cursor, without changing editing.
pub(crate) fn caret_line(editor: &Editor) -> (&str, usize) {
    let cursor = editor.cursor();
    let start = editor.text()[..cursor]
        .rfind('\n')
        .map_or(0, |byte| byte + 1);
    let rest = &editor.text()[start..];
    (rest.split('\n').next().unwrap_or(""), cursor - start)
}

pub(crate) fn viewport(editor: &Editor, capacity: usize) -> Viewport {
    let (source, cursor) = caret_line(editor);
    Viewport::for_source(source, cursor, capacity)
}

#[cfg(test)]
mod tests;
