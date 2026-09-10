//! The SQL buffer.
//!
//! A client people write real statements in needs an editor, not a text field.
//! What that means here is specific: the cursor moves the way it does everywhere
//! else, a mistake can be taken back, and nothing ever splits a character.
//!
//! The cursor is a byte offset, always on a character boundary. Byte offsets are
//! what statement selection needs, so keeping them is what lets "run the
//! statement under the cursor" be exact rather than approximate.
//!
//! Undo keeps whole snapshots of a buffer that is, by its nature, small: a
//! statement someone typed. Snapshots are simple enough to be obviously correct,
//! and correctness is worth more here than the memory a diff would save. The
//! history is capped so a long session cannot grow without bound.

/// How many undo steps are kept.
///
/// Enough to cover a session's worth of mistakes, bounded so the buffer's
/// history cannot outgrow the buffer by an unlimited factor.
const HISTORY_LIMIT: usize = 200;

/// What kind of change was last made, so a run of typing folds into one undo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    /// Characters were typed.
    Insert,
    /// Characters were removed.
    Delete,
    /// The whole buffer was replaced.
    Replace,
}

/// A point the buffer can be returned to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    cursor: usize,
}

/// A multi-line SQL buffer with movement, editing and undo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Editor {
    text: String,
    cursor: usize,
    /// Changes only when the buffer text changes, so async diagnostics can tell
    /// whether their source span still belongs to the visible buffer.
    revision: u64,
    modified: bool,
    /// The column vertical movement is trying to keep, in characters.
    ///
    /// Without it, moving down through a short line and back up would land in
    /// the wrong place, which is the small thing that makes an editor feel wrong.
    goal_column: Option<usize>,
    past: Vec<Snapshot>,
    future: Vec<Snapshot>,
    last_edit: Option<EditKind>,
    /// Whether the last character typed was whitespace, which is where one undo
    /// step ends and the next begins.
    last_insert_was_space: bool,
}

impl Editor {
    /// Creates an editor holding the given text, cursor at the end.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            ..Self::default()
        }
    }

    /// The whole buffer.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cursor position as a byte offset, always on a character boundary.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Revision of the buffer text. Cursor movement does not change it.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Places the caret on a UTF-8 boundary without adding undo history.
    ///
    /// Error mapping uses this after a server response. It is intentionally not
    /// an edit: the SQL, modified flag and undo history remain untouched.
    pub fn set_cursor(&mut self, byte_offset: usize) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = self.clamp_to_boundary(byte_offset);
    }

    /// Records that the buffer now matches something on disk.
    pub const fn mark_saved(&mut self) {
        self.modified = false;
    }

    /// Whether the buffer has unsaved edits.
    #[must_use]
    pub const fn is_modified(&self) -> bool {
        self.modified
    }

    /// The line and column of the cursor, both one-based, for the status bar.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        let before = &self.text[..self.cursor];
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit_once('\n')
            .map_or(before.chars().count(), |(_, last)| last.chars().count())
            + 1;
        (line, column)
    }

    /// How many lines the buffer has. An empty buffer is one line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    /// Whether anything can be undone, for the interface to say so.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether anything can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    // ------------------------------------------------------------- editing

    /// Inserts a character at the cursor.
    pub fn insert(&mut self, ch: char) {
        // One undo step is one word and the space that follows it. The step ends
        // when a word starts, not when a space is typed, so undo takes back
        // "two" rather than leaving a stranded space behind.
        let starts_a_word = self.last_insert_was_space && !ch.is_whitespace();
        let coalesce = self.last_edit == Some(EditKind::Insert) && !starts_a_word;
        self.record(EditKind::Insert, coalesce);
        self.last_insert_was_space = ch.is_whitespace();
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.after_edit();
    }

    /// Inserts a line break, keeping the indentation of the line it left.
    ///
    /// Indented SQL that loses its indentation on every newline is SQL people
    /// stop indenting.
    pub fn insert_newline(&mut self) {
        self.record(EditKind::Insert, false);
        self.last_insert_was_space = true;
        let indent: String = self
            .current_line()
            .chars()
            .take_while(|c| *c == ' ')
            .collect();
        let mut inserted = String::with_capacity(indent.len() + 1);
        inserted.push('\n');
        // Only the indentation before the cursor is copied. Splitting a line in
        // the middle of its text must not invent leading spaces from further on.
        let column = self.text[self.line_start()..self.cursor].chars().count();
        inserted.push_str(&indent[..indent.len().min(column)]);
        self.text.insert_str(self.cursor, &inserted);
        self.cursor += inserted.len();
        self.after_edit();
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.previous_boundary(self.cursor);
        self.record(EditKind::Delete, self.last_edit == Some(EditKind::Delete));
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.after_edit();
    }

    /// Deletes the character after the cursor.
    pub fn delete_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.next_boundary(self.cursor);
        self.record(EditKind::Delete, self.last_edit == Some(EditKind::Delete));
        self.text.replace_range(self.cursor..next, "");
        self.after_edit();
    }

    /// Deletes from the cursor back to the start of the previous word.
    pub fn delete_word_left(&mut self) {
        let target = self.word_left_from(self.cursor);
        if target == self.cursor {
            return;
        }
        self.record(EditKind::Delete, false);
        self.text.replace_range(target..self.cursor, "");
        self.cursor = target;
        self.after_edit();
    }

    /// Replaces the whole buffer, for example when loading a file.
    ///
    /// This is undoable like any other change: text arriving from elsewhere is
    /// exactly the kind of thing someone wants back.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text == self.text {
            return;
        }
        self.record(EditKind::Replace, false);
        self.text = text;
        self.cursor = self.text.len();
        self.goal_column = None;
        self.last_edit = Some(EditKind::Replace);
        // Loading is not editing: the buffer matches what it was loaded from.
        self.modified = false;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Replaces one UTF-8-safe range, for example the word accepted from a
    /// completion menu.
    ///
    /// This is deliberately one non-coalesced edit. Accepting a candidate is a
    /// single user decision, so one undo returns the exact text and cursor that
    /// existed before the menu changed anything.
    pub fn replace_range(&mut self, range: std::ops::Range<usize>, replacement: &str) {
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
            || &self.text[range.clone()] == replacement
        {
            return;
        }
        self.record(EditKind::Replace, false);
        let replaced =
            crate::query::completion::replace_range(&mut self.text, range.clone(), replacement);
        debug_assert!(replaced);
        self.cursor = range.start + replacement.len();
        self.last_insert_was_space = replacement.chars().last().is_some_and(char::is_whitespace);
        self.after_edit();
    }

    /// Takes back the last change.
    pub fn undo(&mut self) {
        let Some(previous) = self.past.pop() else {
            return;
        };
        self.future.push(Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
        });
        self.text = previous.text;
        self.cursor = previous.cursor.min(self.text.len());
        self.cursor = self.clamp_to_boundary(self.cursor);
        self.goal_column = None;
        self.last_edit = None;
        self.modified = true;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Puts back what undo took away.
    pub fn redo(&mut self) {
        let Some(next) = self.future.pop() else {
            return;
        };
        self.past.push(Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
        });
        self.text = next.text;
        self.cursor = next.cursor.min(self.text.len());
        self.cursor = self.clamp_to_boundary(self.cursor);
        self.goal_column = None;
        self.last_edit = None;
        self.modified = true;
        self.revision = self.revision.wrapping_add(1);
    }

    // ------------------------------------------------------------ movement

    /// Moves the cursor one character left.
    pub fn move_left(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        if self.cursor > 0 {
            self.cursor = self.previous_boundary(self.cursor);
        }
    }

    /// Moves the cursor one character right.
    pub fn move_right(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        if self.cursor < self.text.len() {
            self.cursor = self.next_boundary(self.cursor);
        }
    }

    /// Moves the cursor up one line, keeping the column where it can.
    pub fn move_up(&mut self) {
        self.end_undo_run();
        let start = self.line_start();
        if start == 0 {
            // Already on the first line: go to its beginning, which is what
            // every editor does and what stops the key feeling dead.
            self.cursor = 0;
            return;
        }
        let column = self.goal_column.unwrap_or_else(|| self.column());
        self.goal_column = Some(column);
        let previous_start = self.text[..start - 1]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        self.cursor = self.offset_in_line(previous_start, column);
    }

    /// Moves the cursor down one line, keeping the column where it can.
    pub fn move_down(&mut self) {
        self.end_undo_run();
        let end = self.line_end();
        if end >= self.text.len() {
            self.cursor = self.text.len();
            return;
        }
        let column = self.goal_column.unwrap_or_else(|| self.column());
        self.goal_column = Some(column);
        self.cursor = self.offset_in_line(end + 1, column);
    }

    /// Moves to the first character of the line.
    pub fn move_line_start(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = self.line_start();
    }

    /// Moves past the last character of the line.
    pub fn move_line_end(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = self.line_end();
    }

    /// Moves to the start of the buffer.
    pub fn move_buffer_start(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = 0;
    }

    /// Moves to the end of the buffer.
    pub fn move_buffer_end(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = self.text.len();
    }

    /// Moves to the start of the previous word.
    pub fn move_word_left(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = self.word_left_from(self.cursor);
    }

    /// Moves past the end of the next word.
    pub fn move_word_right(&mut self) {
        self.end_undo_run();
        self.goal_column = None;
        self.cursor = self.word_right_from(self.cursor);
    }

    /// Moves up by a screenful, given how many lines are on screen.
    pub fn move_page_up(&mut self, lines: usize) {
        for _ in 0..lines.max(1) {
            self.move_up();
        }
    }

    /// Moves down by a screenful.
    pub fn move_page_down(&mut self, lines: usize) {
        for _ in 0..lines.max(1) {
            self.move_down();
        }
    }

    // ------------------------------------------------------------ internals

    /// Ends the run of edits that undo folds into one step.
    ///
    /// Typing, moving the cursor somewhere else, and typing again is two changes
    /// to the person who did it, so it must be two undo steps. Without this the
    /// two runs coalesce and one undo takes back work in a place the cursor is
    /// no longer near, which reads as the editor losing text.
    fn end_undo_run(&mut self) {
        self.last_edit = None;
    }

    /// Records the state before a change, unless it folds into the last one.
    fn record(&mut self, kind: EditKind, coalesce: bool) {
        if !coalesce {
            self.past.push(Snapshot {
                text: self.text.clone(),
                cursor: self.cursor,
            });
            if self.past.len() > HISTORY_LIMIT {
                self.past.remove(0);
            }
        }
        // A new change makes the redo path unreachable, as it does everywhere.
        self.future.clear();
        self.last_edit = Some(kind);
    }

    fn after_edit(&mut self) {
        self.modified = true;
        self.goal_column = None;
        self.revision = self.revision.wrapping_add(1);
    }

    /// The byte offset of the start of the line the cursor is on.
    fn line_start(&self) -> usize {
        self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1)
    }

    /// The byte offset of the end of the line the cursor is on, before the break.
    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |index| self.cursor + index)
    }

    /// The text of the line the cursor is on.
    fn current_line(&self) -> &str {
        &self.text[self.line_start()..self.line_end()]
    }

    /// The cursor's column, in characters, zero-based.
    fn column(&self) -> usize {
        self.text[self.line_start()..self.cursor].chars().count()
    }

    /// The offset of a column within the line starting at `start`, clamped to
    /// that line's length.
    fn offset_in_line(&self, start: usize, column: usize) -> usize {
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |index| start + index);
        let line = &self.text[start..end];
        line.char_indices()
            .nth(column)
            .map_or(end, |(index, _)| start + index)
    }

    fn previous_boundary(&self, from: usize) -> usize {
        let mut index = from.saturating_sub(1);
        while index > 0 && !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn next_boundary(&self, from: usize) -> usize {
        let mut index = (from + 1).min(self.text.len());
        while index < self.text.len() && !self.text.is_char_boundary(index) {
            index += 1;
        }
        index
    }

    fn clamp_to_boundary(&self, from: usize) -> usize {
        let mut index = from.min(self.text.len());
        while index > 0 && !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    /// The start of the word before an offset.
    ///
    /// Words are runs of the characters SQL identifiers are made of. Punctuation
    /// runs are their own words, so moving through `orders.customer_id` stops
    /// where someone editing it would expect.
    fn word_left_from(&self, from: usize) -> usize {
        let mut index = from;
        while index > 0 {
            let previous = self.previous_boundary(index);
            if self.char_at(previous).is_some_and(char::is_whitespace) {
                index = previous;
            } else {
                break;
            }
        }
        let Some(anchor) = index.checked_sub(1).and_then(|_| {
            let previous = self.previous_boundary(index);
            self.char_at(previous)
        }) else {
            return index;
        };
        let word = is_word_char(anchor);
        while index > 0 {
            let previous = self.previous_boundary(index);
            match self.char_at(previous) {
                Some(ch) if !ch.is_whitespace() && is_word_char(ch) == word => index = previous,
                _ => break,
            }
        }
        index
    }

    /// The end of the word after an offset.
    fn word_right_from(&self, from: usize) -> usize {
        let mut index = from;
        while index < self.text.len() && self.char_at(index).is_some_and(char::is_whitespace) {
            index = self.next_boundary(index);
        }
        let Some(anchor) = self.char_at(index) else {
            return index;
        };
        let word = is_word_char(anchor);
        while index < self.text.len() {
            match self.char_at(index) {
                Some(ch) if !ch.is_whitespace() && is_word_char(ch) == word => {
                    index = self.next_boundary(index);
                }
                _ => break,
            }
        }
        index
    }

    fn char_at(&self, index: usize) -> Option<char> {
        self.text[index..].chars().next()
    }
}

/// Whether a character is part of an identifier-like word.
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> Editor {
        let mut editor = Editor::default();
        for ch in text.chars() {
            if ch == '\n' {
                editor.insert_newline();
            } else {
                editor.insert(ch);
            }
        }
        editor
    }

    #[test]
    fn completion_replacement_is_one_undoable_edit() {
        let mut editor = Editor::with_text("SELECT ord");
        editor.replace_range(7..10, "\"orders\"");
        assert_eq!(editor.text(), "SELECT \"orders\"");
        assert_eq!(editor.cursor(), "SELECT \"orders\"".len());
        assert!(editor.is_modified());

        editor.undo();
        assert_eq!(editor.text(), "SELECT ord");
        assert_eq!(editor.cursor(), "SELECT ord".len());
        assert!(!editor.can_undo());
    }

    #[test]
    fn completion_replacement_preserves_utf8_boundaries() {
        let mut editor = Editor::with_text("SELECT café");
        let start = "SELECT ".len();
        editor.replace_range(start..editor.text().len(), "\"café\"");
        assert_eq!(editor.text(), "SELECT \"café\"");
        assert_eq!(editor.cursor(), editor.text().len());
    }

    #[test]
    fn formatted_buffer_replacement_is_one_edit_with_cursor_recovery() {
        let source = "select café,total from orders where total>0;";
        let source_cursor = source.find("total>0").expect("token") + "total".len();
        let mut editor = Editor::with_text(source);
        editor.set_cursor(source_cursor);
        let formatted = crate::query::format_sql(source, source_cursor).expect("format");

        editor.replace_range(0..source.len(), &formatted.text);
        editor.set_cursor(formatted.cursor);
        assert!(editor.is_modified());
        assert!(editor.can_undo());
        assert!(editor.text().is_char_boundary(editor.cursor()));

        editor.undo();
        assert_eq!(editor.text(), source);
        assert_eq!(editor.cursor(), source_cursor);
        assert!(!editor.can_undo());
        editor.redo();
        assert_eq!(editor.text(), formatted.text);
        assert_eq!(editor.cursor(), formatted.cursor);
    }

    #[test]
    fn formatted_cursor_fixtures_stay_on_ascii_and_unicode_character_boundaries() {
        for (source, token, inside) in [
            ("select total from orders;", "total", "tot"),
            ("select café from orders;", "café", "ca"),
        ] {
            let source_cursor = source.find(token).expect("token") + inside.len();
            let formatted = crate::query::format_sql(source, source_cursor).expect("format");
            let mut editor = Editor::with_text(source);
            editor.set_cursor(source_cursor);
            editor.replace_range(0..source.len(), &formatted.text);
            editor.set_cursor(formatted.cursor);
            assert!(editor.text().is_char_boundary(editor.cursor()));
            assert!(editor.cursor() <= editor.text().len());
        }
    }

    #[test]
    fn a_noop_format_replacement_does_not_change_revision_or_history() {
        let source = "SELECT 1\nFROM orders;";
        let mut editor = Editor::with_text(source);
        editor.set_cursor(4);
        let revision = editor.revision();
        let cursor = editor.cursor();
        editor.replace_range(0..source.len(), source);
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.cursor(), cursor);
        assert!(!editor.can_undo());
        assert!(!editor.is_modified());
    }

    #[test]
    fn text_revision_changes_for_edits_but_not_cursor_positioning() {
        let mut editor = Editor::with_text("SELECT café");
        let initial = editor.revision();
        editor.set_cursor("SELECT ".len() + "caf".len());
        assert_eq!(editor.revision(), initial);
        assert!(!editor.is_modified());
        assert!(!editor.can_undo());

        editor.insert('!');
        assert_eq!(editor.revision(), initial + 1);
        editor.undo();
        assert_eq!(editor.revision(), initial + 2);
        editor.redo();
        assert_eq!(editor.revision(), initial + 3);
    }

    #[test]
    fn cursor_setter_clamps_invalid_offsets_without_splitting_utf8() {
        let mut editor = Editor::with_text("café");
        editor.set_cursor(4);
        assert_eq!(
            editor.cursor(),
            3,
            "the caret backs up to a character boundary"
        );
        editor.set_cursor(999);
        assert_eq!(editor.cursor(), editor.text().len());
    }

    #[test]
    fn vertical_movement_keeps_the_column_it_started_from() {
        // The behaviour every editor has and every text field lacks: passing
        // through a short line must not forget where the cursor was.
        let mut editor = Editor::with_text("SELECT customer_id\nFROM x\nWHERE total > 100");
        editor.move_buffer_start();
        for _ in 0..15 {
            editor.move_right();
        }
        assert_eq!(editor.position(), (1, 16));

        editor.move_down();
        assert_eq!(editor.position(), (2, 7), "the short line clamps");
        editor.move_down();
        assert_eq!(
            editor.position(),
            (3, 16),
            "and the column comes back on a line long enough for it"
        );

        editor.move_up();
        editor.move_up();
        assert_eq!(editor.position(), (1, 16));
    }

    #[test]
    fn vertical_movement_stops_at_the_ends_rather_than_doing_nothing() {
        let mut editor = Editor::with_text("one\ntwo");
        editor.move_buffer_start();
        editor.move_right();
        editor.move_up();
        assert_eq!(editor.cursor(), 0, "up on the first line goes to the start");

        editor.move_buffer_end();
        editor.move_left();
        editor.move_down();
        assert_eq!(
            editor.cursor(),
            editor.text().len(),
            "down on the last line goes to the end"
        );
    }

    #[test]
    fn line_and_buffer_keys_land_where_they_say() {
        let mut editor = Editor::with_text("SELECT 1\nFROM orders\nWHERE id = 2");
        editor.move_buffer_start();
        editor.move_down();
        editor.move_line_end();
        assert_eq!(editor.position(), (2, 12));
        editor.move_line_start();
        assert_eq!(editor.position(), (2, 1));
        editor.move_buffer_end();
        assert_eq!(editor.position(), (3, 13));
        editor.move_buffer_start();
        assert_eq!(editor.position(), (1, 1));
    }

    #[test]
    fn word_movement_treats_identifiers_and_punctuation_as_separate_words() {
        let mut editor = Editor::with_text("SELECT orders.customer_id FROM t");
        editor.move_buffer_start();

        editor.move_word_right();
        assert_eq!(editor.position().1, 7, "past SELECT");
        editor.move_word_right();
        assert_eq!(editor.position().1, 14, "past orders");
        editor.move_word_right();
        assert_eq!(editor.position().1, 15, "past the dot on its own");
        editor.move_word_right();
        assert_eq!(editor.position().1, 26, "past customer_id");

        editor.move_word_left();
        assert_eq!(editor.position().1, 15, "back to the start of customer_id");
        editor.move_word_left();
        assert_eq!(editor.position().1, 14);
        editor.move_word_left();
        assert_eq!(editor.position().1, 8, "back to the start of orders");
    }

    #[test]
    fn word_movement_stops_at_the_ends() {
        let mut editor = Editor::with_text("word");
        editor.move_buffer_start();
        editor.move_word_left();
        assert_eq!(editor.cursor(), 0);
        editor.move_buffer_end();
        editor.move_word_right();
        assert_eq!(editor.cursor(), 4);

        let mut empty = Editor::default();
        empty.move_word_left();
        empty.move_word_right();
        empty.move_up();
        empty.move_down();
        assert_eq!(empty.cursor(), 0, "an empty buffer survives every key");
    }

    #[test]
    fn a_new_line_keeps_the_indentation_of_the_one_it_left() {
        let mut editor = typed("SELECT 1\n    FROM orders");
        editor.insert_newline();
        editor.insert('W');
        assert_eq!(
            editor.text(),
            "SELECT 1\n    FROM orders\n    W",
            "indented SQL that loses its indent is SQL people stop indenting"
        );

        // Splitting a line in the middle copies only the indentation before the
        // cursor, so no leading space is invented.
        let mut editor = Editor::with_text("  abc");
        editor.move_buffer_start();
        editor.move_right();
        editor.insert_newline();
        assert_eq!(
            editor.text(),
            " \n  abc",
            "only the indent before the cursor is copied; the rest of the line \
             keeps its own characters"
        );
    }

    #[test]
    fn deleting_forwards_backwards_and_by_word_all_respect_characters() {
        let mut editor = Editor::with_text("SELECT héllo 日本;");
        editor.move_buffer_end();
        editor.delete_word_left();
        assert_eq!(editor.text(), "SELECT héllo 日本");
        editor.delete_word_left();
        assert_eq!(editor.text(), "SELECT héllo ");
        assert!(editor.text().is_char_boundary(editor.cursor()));

        editor.move_buffer_start();
        editor.delete_forward();
        assert_eq!(editor.text(), "ELECT héllo ");
        for _ in 0..50 {
            editor.delete_forward();
        }
        assert!(editor.text().is_empty());
        editor.delete_forward();
        editor.backspace();
        assert!(editor.text().is_empty(), "both ends are no-ops, not panics");
    }

    #[test]
    fn undo_takes_back_a_word_at_a_time_and_redo_puts_it_back() {
        let mut editor = typed("SELECT one two");
        assert_eq!(editor.text(), "SELECT one two");

        editor.undo();
        assert_eq!(editor.text(), "SELECT one ", "a word is one step");
        editor.undo();
        assert_eq!(editor.text(), "SELECT ");
        assert!(editor.can_redo());

        editor.redo();
        assert_eq!(editor.text(), "SELECT one ");
        editor.redo();
        assert_eq!(editor.text(), "SELECT one two");
        assert!(!editor.can_redo());

        // Typing after an undo makes the redo path unreachable, as everywhere.
        editor.undo();
        editor.insert('!');
        assert!(!editor.can_redo());
    }

    #[test]
    fn undo_covers_deletions_and_a_replaced_buffer() {
        let mut editor = Editor::with_text("SELECT 1;");
        editor.set_text("DROP TABLE orders;");
        assert_eq!(editor.text(), "DROP TABLE orders;");
        editor.undo();
        assert_eq!(
            editor.text(),
            "SELECT 1;",
            "text arriving from elsewhere is exactly what someone wants back"
        );

        editor.move_buffer_end();
        editor.delete_word_left();
        editor.backspace();
        assert_eq!(editor.text(), "SELECT ");
        editor.undo();
        editor.undo();
        assert_eq!(editor.text(), "SELECT 1;");
    }

    #[test]
    fn undo_on_an_untouched_buffer_does_nothing_at_all() {
        let mut editor = Editor::with_text("SELECT 1");
        assert!(!editor.can_undo());
        editor.undo();
        editor.redo();
        assert_eq!(editor.text(), "SELECT 1");
        assert_eq!(editor.cursor(), 8);
    }

    #[test]
    fn the_history_is_bounded_so_a_long_session_cannot_grow_without_end() {
        let mut editor = Editor::default();
        for index in 0..(HISTORY_LIMIT * 2) {
            // Whitespace ends each run, so every character is its own step.
            editor.insert(char::from_digit((index % 10) as u32, 10).unwrap_or('0'));
            editor.insert(' ');
        }
        assert!(editor.past.len() <= HISTORY_LIMIT);
        // The recent past still works, which is the part anyone reaches for.
        let before = editor.text().to_owned();
        editor.undo();
        assert_ne!(editor.text(), before);
    }

    #[test]
    fn multibyte_text_is_never_split_by_any_movement() {
        let mut editor = typed("SELECT 'héllo 日本';");
        assert_eq!(editor.text(), "SELECT 'héllo 日本';");
        for _ in 0..40 {
            editor.move_left();
            assert!(editor.text().is_char_boundary(editor.cursor()));
        }
        for _ in 0..40 {
            editor.move_right();
            assert!(editor.text().is_char_boundary(editor.cursor()));
        }
        editor.move_buffer_start();
        editor.move_word_right();
        assert!(editor.text().is_char_boundary(editor.cursor()));
    }

    #[test]
    fn a_page_moves_by_a_screenful_and_stops_at_the_end() {
        let mut editor = Editor::with_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
        editor.move_buffer_start();
        editor.move_page_down(4);
        assert_eq!(editor.position().0, 5);
        editor.move_page_up(4);
        assert_eq!(editor.position().0, 1);
        editor.move_page_down(100);
        assert_eq!(editor.position().0, 10, "it stops at the last line");
    }

    #[test]
    fn loading_text_is_not_an_edit_but_typing_is() {
        let mut editor = Editor::with_text("SELECT 1");
        assert!(!editor.is_modified());
        editor.set_text("SELECT 2");
        assert!(!editor.is_modified(), "the buffer matches what it holds");
        editor.insert('!');
        assert!(editor.is_modified());
    }

    #[test]
    fn moving_the_cursor_ends_the_undo_run() {
        // Typing in one place, going somewhere else, and typing there is two
        // changes. Before this was fixed they shared one undo step, so a single
        // undo took back work in a place the cursor was no longer near.
        let mut editor = typed("SELECT");
        editor.move_buffer_start();
        for ch in "x".chars() {
            editor.insert(ch);
        }

        editor.undo();
        assert_eq!(editor.text(), "SELECT", "only the second run comes back");

        editor.undo();
        assert_eq!(editor.text(), "", "and the first run is a step of its own");
    }

    #[test]
    fn every_movement_ends_the_undo_run_not_just_the_arrow_keys() {
        // One test per key, because the run is ended in each method and an
        // added movement that forgets to do it would otherwise go unnoticed.
        type Movement = fn(&mut Editor);
        let movements: Vec<(&str, Movement)> = vec![
            ("left", |e| e.move_left()),
            ("right", |e| e.move_right()),
            ("up", |e| e.move_up()),
            ("down", |e| e.move_down()),
            ("line start", |e| e.move_line_start()),
            ("line end", |e| e.move_line_end()),
            ("buffer start", |e| e.move_buffer_start()),
            ("buffer end", |e| e.move_buffer_end()),
            ("word left", |e| e.move_word_left()),
            ("word right", |e| e.move_word_right()),
            ("page up", |e| e.move_page_up(4)),
            ("page down", |e| e.move_page_down(4)),
        ];

        // No trailing space or newline: a run that ends mid-word is the only one
        // the next insert would coalesce into, so this can actually fail. The
        // first version of this test typed a trailing newline, which starts a
        // fresh undo step by itself, and it passed with the fix taken out.
        let source = "SELECT one\nFROM two\nWHERE three\nAND four";

        for (name, movement) in movements {
            let mut editor = typed(source);
            movement(&mut editor);
            editor.insert('x');
            editor.undo();
            assert_eq!(
                editor.text(),
                source,
                "moving by {name} did not end the undo run, so one undo took back \
                 the typing before the move as well"
            );
        }
    }
}
