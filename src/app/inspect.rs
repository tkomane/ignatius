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
mod tests {
    use super::*;

    fn set() -> ResultSet {
        let mut set = ResultSet::new(vec!["id".into(), "note".into(), "amount".into()], 10);
        set.push(vec![
            Cell::Text("1".into()),
            Cell::Text("a fairly long note that will not fit".into()),
            Cell::Null,
        ]);
        set.push(vec![
            Cell::Text("2".into()),
            Cell::Text(String::new()),
            Cell::Text("NULL".into()),
        ]);
        set
    }

    #[test]
    fn a_cell_view_is_derived_from_the_selection_and_never_stored() {
        let set = set();
        let view = CellView::build(&set, 0, 1, 10).expect("a cell");
        assert_eq!(view.column, "note");
        assert_eq!(view.position, (2, 3));
        assert_eq!(view.row, (1, 2));
        assert_eq!(view.description, "text, 36 characters");
        assert!(view.lines.len() > 1, "a long value wraps");
        for line in &view.lines {
            assert!(value::display_width(line) <= 10);
        }
        assert_eq!(view.lines.concat(), "a fairly long note that will not fit");
    }

    #[test]
    fn a_selection_that_outlived_its_result_yields_nothing_rather_than_a_panic() {
        let set = set();
        assert!(CellView::build(&set, 99, 0, 20).is_none());
        assert!(CellView::build(&set, 0, 99, 20).is_none());

        let empty = ResultSet::new(vec!["id".into()], 10);
        assert!(CellView::build(&empty, 0, 0, 20).is_none());

        let no_columns = ResultSet::new(Vec::new(), 10);
        assert!(CellView::build(&no_columns, 0, 0, 20).is_none());
    }

    #[test]
    fn null_an_empty_string_and_the_text_null_are_told_apart_in_words() {
        // This is the promise made in the NULL_MARKER documentation: where the
        // grid can only show a marker, the inspector says which it is.
        let set = set();
        let null = CellView::build(&set, 0, 2, 20).expect("a cell");
        let empty = CellView::build(&set, 1, 1, 20).expect("a cell");
        let literal = CellView::build(&set, 1, 2, 20).expect("a cell");

        assert_eq!(null.description, "SQL NULL");
        assert!(null.is_null);
        assert!(null.lines.is_empty(), "a NULL has no text to show");

        assert_eq!(empty.description, "empty string, 0 characters");
        assert!(!empty.is_null);
        assert_eq!(empty.lines, vec![""]);

        assert_eq!(literal.description, "text, 4 characters");
        assert_eq!(literal.lines, vec!["NULL"]);

        assert_ne!(null.description, empty.description);
        assert_ne!(null.description, literal.description);
    }

    #[test]
    fn a_value_that_would_drive_the_terminal_is_shown_as_text() {
        let mut set = ResultSet::new(vec!["payload".into()], 10);
        set.push(vec![Cell::Text("\x1b[2J\x1b[Hgone".into())]);
        let view = CellView::build(&set, 0, 0, 40).expect("a cell");
        assert!(
            !view.lines.iter().any(|line| line.contains('\x1b')),
            "an escape reached the inspector: {:?}",
            view.lines
        );
        assert!(view.lines.concat().contains("\\x1B[2J"));

        // A column name is server-controlled too.
        let mut named = ResultSet::new(vec!["\x1b[31mred".into()], 10);
        named.push(vec![Cell::Text("v".into())]);
        let view = CellView::build(&named, 0, 0, 40).expect("a cell");
        assert!(!view.column.contains('\x1b'), "{}", view.column);
    }

    #[test]
    fn counts_describe_the_value_stored_not_the_way_it_was_wrapped() {
        let mut set = ResultSet::new(vec!["doc".into()], 10);
        set.push(vec![Cell::Text("aaaa\nbb".into())]);

        // Four characters, a newline, two characters: seven, whatever the pane
        // does with them. The wrapped line count is a fact about the pane.
        let narrow = CellView::build(&set, 0, 0, 2).expect("a cell");
        let wide = CellView::build(&set, 0, 0, 80).expect("a cell");
        assert_eq!(narrow.description, "text, 7 characters");
        assert_eq!(wide.description, narrow.description);
        assert_eq!(wide.lines, vec!["aaaa", "bb"], "the value's own lines");
        assert_eq!(narrow.lines, vec!["aa", "aa", "bb"]);

        // The grid escapes the newline, because there it would break the row.
        // The inspector uses it, and neither one lets it reach the terminal.
        assert_eq!(Cell::Text("aaaa\nbb".into()).display(), "aaaa\\nbb");
        assert!(!wide.lines.iter().any(|line| line.contains('\n')));
    }

    #[test]
    fn scrolling_stops_at_both_ends_and_states_where_it_is() {
        let mut set = ResultSet::new(vec!["doc".into()], 10);
        set.push(vec![Cell::Text("abcdefghij".into())]);
        let view = CellView::build(&set, 0, 0, 2).expect("a cell");
        assert_eq!(view.lines.len(), 5);

        let mut inspector = Inspector::new();
        assert_eq!(inspector.scroll(), 0);
        inspector.scroll_up();
        assert_eq!(inspector.scroll(), 0, "the top is the top");

        for _ in 0..10 {
            inspector.scroll_down(view.lines.len(), 2);
        }
        assert_eq!(
            inspector.scroll(),
            3,
            "scrolling stops with the last line on screen"
        );

        let (visible, above, below) = view.window(inspector.scroll(), 2);
        assert_eq!(visible, ["gh", "ij"]);
        assert_eq!(above, 3);
        assert_eq!(below, 0);

        let label = view.scroll_label(0, 2).expect("a position");
        assert_eq!(label, "showing 1-2 of 5 wrapped lines (3 below)");
        assert!(
            view.scroll_label(0, 5).is_none(),
            "a value that fits says nothing about scrolling"
        );
        assert!(view.scroll_label(0, 0).is_none());
    }

    #[test]
    fn a_window_never_indexes_past_the_value() {
        let mut set = ResultSet::new(vec!["doc".into()], 10);
        set.push(vec![Cell::Text("abc".into())]);
        let view = CellView::build(&set, 0, 0, 10).expect("a cell");
        let (visible, _, below) = view.window(99, 3);
        assert_eq!(visible, ["abc"], "a stale offset still shows the value");
        assert_eq!(below, 0);
        assert_eq!(view.window(0, 0).0.len(), 0);
    }

    #[test]
    fn an_expanded_row_puts_one_column_on_each_line_with_the_names_aligned() {
        let set = set();
        let fields = expand_row(&set, 0, 40, true);
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].name, "id    ");
        assert_eq!(fields[1].name, "note  ");
        assert_eq!(fields[2].name, "amount");
        assert_eq!(fields[0].value, "1");
        assert!(fields[2].is_null);
        assert_eq!(fields[2].value, crate::query::value::NULL_MARKER);

        let widths: Vec<usize> = fields
            .iter()
            .map(|f| value::display_width(&f.name))
            .collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "names are padded to a common width so values line up"
        );
    }

    #[test]
    fn an_expanded_value_too_wide_for_the_pane_is_marked_as_shortened() {
        let set = set();
        let fields = expand_row(&set, 0, 20, true);
        let note = &fields[1];
        assert!(note.truncated, "it does not fit and says so");
        assert!(note.value.ends_with('\u{2026}'));
        assert!(!fields[0].truncated, "a value that fits is not marked");

        // In ASCII the marker is still there, in ASCII.
        let ascii = expand_row(&set, 0, 20, false);
        assert!(ascii[1].value.ends_with("..."));
    }

    #[test]
    fn an_expanded_view_can_hide_columns_without_shifting_source_values() {
        let set = set();
        let fields = expand_row_columns(&set, 0, 80, true, &[0, 2]);

        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name.trim(), "id");
        assert_eq!(fields[0].value, "1");
        assert_eq!(fields[1].name.trim(), "amount");
        assert_eq!(fields[1].value, crate::query::value::NULL_MARKER);
        assert!(fields[1].is_null);
    }

    #[test]
    fn expanding_a_row_that_is_not_there_yields_nothing() {
        let set = set();
        assert!(expand_row(&set, 99, 40, true).is_empty());
        assert!(expand_row(&ResultSet::new(vec!["a".into()], 10), 0, 40, true).is_empty());
    }

    #[test]
    fn an_extremely_narrow_pane_still_shows_something_of_every_column() {
        // One cell of value per line is the floor. Showing no value at all would
        // be worse than showing one character of it.
        let set = set();
        for width in [0usize, 1, 2, 8] {
            let fields = expand_row(&set, 0, width, true);
            assert_eq!(fields.len(), 3, "width {width}");
            for field in fields {
                assert!(
                    !field.value.is_empty(),
                    "width {width} showed nothing for {}",
                    field.name
                );
            }
        }
    }
}
