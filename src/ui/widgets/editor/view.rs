//! One display-cell view shared by editor painting, completion and hit testing.

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

#[derive(Debug)]
pub(crate) struct LineView<'a> {
    pub units: Vec<Unit<'a>>,
    pub width: usize,
    pub end_column: usize,
}

impl<'a> LineView<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut result = Self {
            units: Vec::new(),
            width: 0,
            end_column: 1,
        };
        let mut normal_start = 0;
        let mut normal_column = 1;
        let mut column = 1;
        for (byte, ch) in source.char_indices() {
            let original = ch.to_string();
            let safe = sanitize_for_display(&original);
            if safe != original {
                result.push_graphemes(&source[normal_start..byte], normal_start, normal_column);
                result.push(Cow::Owned(safe), byte..byte + ch.len_utf8(), column);
                normal_start = byte + ch.len_utf8();
                normal_column = column + 1;
            }
            column += 1;
        }
        result.push_graphemes(&source[normal_start..], normal_start, normal_column);
        result.end_column = column;
        result
    }

    fn push_graphemes(&mut self, text: &'a str, byte: usize, column: usize) {
        let span = Span::raw(text);
        let source_start = byte;
        let mut byte = byte;
        let mut column = column;
        for grapheme in span.styled_graphemes(Style::default()) {
            let end = byte + grapheme.symbol.len();
            self.push(
                Cow::Borrowed(&text[byte - source_start..end - source_start]),
                byte..end,
                column,
            );
            column += grapheme.symbol.chars().count();
            byte = end;
        }
    }

    fn push(&mut self, text: Cow<'a, str>, source: Range<usize>, column: usize) {
        let width = display_width(&text);
        self.units.push(Unit {
            text,
            source,
            column,
            cell: self.width,
            width,
        });
        self.width = self.width.saturating_add(width);
    }

    pub fn column_at(&self, cell: usize) -> usize {
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
    pub fn new(line: &LineView<'_>, cursor: usize, capacity: usize) -> Self {
        let unit = line.units.iter().find(|unit| unit.source.contains(&cursor));
        let cell = unit.map_or(line.width, |unit| unit.cell);
        let width = unit.map_or(0, |unit| unit.width);
        let blank_caret = width == 0 || width > capacity;
        let shown = if blank_caret { 1 } else { width };
        let offset = cell.saturating_add(shown).saturating_sub(capacity);
        Self {
            offset,
            caret_cell: cell.saturating_sub(offset),
            blank_caret,
            caret_column: unit.map_or(line.end_column, |unit| unit.column),
        }
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
    Viewport::new(&LineView::new(source), cursor, capacity)
}

#[cfg(test)]
mod tests;
