//! Reading one row, and one value, in full.
//!
//! The grid abbreviates: it has to, because a column is as wide as the terminal
//! allows. Both views here exist so that abbreviation is never the last word. A
//! truncated value that looks complete is the failure this project is written
//! against, so there is always a way to see the whole thing.
//!
//! Neither structure holds a copy of the cell it is showing. They hold a scroll
//! offset and nothing else; the value is derived from the current selection and
//! the current result every time it is drawn. A copy would go stale the moment
//! a new query finished, which is exactly the class of quiet lie the job
//! identity machinery exists to prevent elsewhere.

use crate::query::result::ResultSet;
use crate::query::value::{self, Cell};

/// The open cell inspector.
///
/// What it does not contain is the point: no row index, no column index, no
/// value. Those live in the model and the result set, which are the source of
/// truth for them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Inspector {
    /// First visible line of the wrapped value.
    scroll: usize,
}

impl Inspector {
    /// Opens an inspector at the top of the value.
    #[must_use]
    pub const fn new() -> Self {
        Self { scroll: 0 }
    }

    /// First visible line.
    #[must_use]
    pub const fn scroll(self) -> usize {
        self.scroll
    }

    /// Scrolls down, stopping so the last line stays on screen.
    pub const fn scroll_down(&mut self, total: usize, height: usize) {
        let max = total.saturating_sub(height);
        if self.scroll < max {
            self.scroll += 1;
        }
    }

    /// Scrolls up.
    pub const fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Returns to the top, which is what moving to another cell should do.
    pub const fn reset(&mut self) {
        self.scroll = 0;
    }
}

/// Everything needed to draw the inspector, derived rather than stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellView {
    /// The column the value came from.
    pub column: String,
    /// Which column this is, one-based, and how many there are.
    pub position: (usize, usize),
    /// Which row this is, one-based, and how many are retained.
    pub row: (usize, usize),
    /// What the value is, in words: SQL NULL, an empty string, or text.
    pub description: String,
    /// The value wrapped to the available width, already escaped for display.
    pub lines: Vec<String>,
    /// Whether the value is a SQL NULL, which has no text to show.
    pub is_null: bool,
}

impl CellView {
    /// Builds the view of one cell, or nothing when the selection points at no
    /// cell at all.
    ///
    /// A selection can outlive the result it was made in, so the indices are
    /// checked rather than trusted. Returning `None` is how an empty result and
    /// a stale selection are both handled without a panic and without inventing
    /// a value.
    #[must_use]
    pub fn build(set: &ResultSet, row: usize, column: usize, width: usize) -> Option<Self> {
        let cells = set.rows.get(row)?;
        let cell = cells.get(column)?;
        let name = set.columns.get(column)?;

        // A newline in the value becomes a line here rather than the text `\n`.
        // The grid escapes it, because there it would break the row; a value
        // laid out on its own does not have that problem, and a JSON document
        // read as one endless line is not read at all. Every other control
        // character is still escaped, and the split happens before escaping so
        // no newline survives into what is drawn.
        let lines = match cell.raw() {
            None => Vec::new(),
            Some(raw) => raw
                .split('\n')
                .flat_map(|segment| {
                    value::wrap_to_width(&value::sanitize_for_display(segment), width)
                })
                .collect(),
        };

        Some(Self {
            column: value::sanitize_for_display(name),
            position: (column + 1, set.columns.len()),
            row: (row + 1, set.rows.len()),
            description: cell.describe(),
            lines,
            is_null: cell.is_null(),
        })
    }

    /// The lines visible at a scroll offset, and how many were above and below.
    #[must_use]
    pub fn window(&self, scroll: usize, height: usize) -> (&[String], usize, usize) {
        if height == 0 || self.lines.is_empty() {
            return (&[], 0, 0);
        }
        let start = scroll.min(self.lines.len().saturating_sub(1));
        let end = (start + height).min(self.lines.len());
        (
            &self.lines[start..end],
            start,
            self.lines.len().saturating_sub(end),
        )
    }

    /// A statement of position, shown only when the value does not fit.
    ///
    /// Silence would be indistinguishable from a value that ends where the pane
    /// does, which is the ambiguity the whole inspector exists to remove.
    #[must_use]
    pub fn scroll_label(&self, scroll: usize, height: usize) -> Option<String> {
        let (_, _, below) = self.window(scroll, height);
        if height == 0 || self.lines.len() <= height {
            return None;
        }
        // "wrapped lines" rather than "lines": how many lines the pane made of
        // the value is a fact about the pane. The value's own character count is
        // stated in the description, where it belongs.
        Some(format!(
            "showing {}-{} of {} wrapped lines ({below} below)",
            scroll + 1,
            (scroll + height).min(self.lines.len()),
            self.lines.len()
        ))
    }
}

/// One line of the expanded row view: a column name and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedField {
    /// Column name, escaped for display.
    pub name: String,
    /// Value, escaped for display and shortened to the space available.
    pub value: String,
    /// Whether the value is a SQL NULL, so it can be styled as absence.
    pub is_null: bool,
    /// Whether the value shown is shorter than the value held.
    pub truncated: bool,
}

/// Lays one row out down the screen, one column per line.
///
/// This is `psql`'s `\x`, and it exists for the same reason: a row with thirty
/// columns is unreadable across an eighty-column terminal. The name column is
/// aligned to the longest name so the values line up and can be scanned.
#[must_use]
pub fn expand_row(set: &ResultSet, row: usize, width: usize, unicode: bool) -> Vec<ExpandedField> {
    let columns: Vec<usize> = (0..set.columns.len()).collect();
    expand_row_columns(set, row, width, unicode, &columns)
}

/// Lays out only the requested source columns, preserving their source order.
///
/// The expanded view follows the grid's visibility choices, but it still reads
/// values by source position. That keeps a hidden column from shifting a value
/// under the wrong header and lets the ordinary public [`expand_row`] helper
/// retain its all-column contract for callers outside the interactive grid.
#[must_use]
pub fn expand_row_columns(
    set: &ResultSet,
    row: usize,
    width: usize,
    unicode: bool,
    columns: &[usize],
) -> Vec<ExpandedField> {
    let Some(cells) = set.rows.get(row) else {
        return Vec::new();
    };
    let name_width = columns
        .iter()
        .filter_map(|index| set.columns.get(*index))
        .map(|name| value::display_width(&value::sanitize_for_display(name)))
        .max()
        .unwrap_or(0);
    // Three cells for the gap that carries the selection marker, and at least
    // one for the value itself. The renderer uses the same three.
    let value_width = width.saturating_sub(name_width + 3).max(1);

    columns
        .iter()
        .filter_map(|index| set.columns.get(*index).map(|name| (*index, name)))
        .map(|(index, name)| {
            let cell = cells.get(index).unwrap_or(&Cell::Null);
            let shown = cell.display();
            let fitted = value::truncate_to_width(&shown, value_width, unicode);
            ExpandedField {
                name: value::pad_to_width(&value::sanitize_for_display(name), name_width),
                truncated: fitted != shown,
                value: fitted,
                is_null: cell.is_null(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
