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

/// The largest paste the editor accepts, in bytes.
///
/// A paste is not typed input: it arrives all at once, and an accidental
/// clipboard of a whole document should be refused with a named reason rather
/// than frozen into the buffer and its history. The bound is one mebibyte.
pub const PASTE_LIMIT_BYTES: usize = 1_048_576;

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

    /// Inserts pasted text at the cursor as one non-coalesced undo step.
    ///
    /// Pasted text is typed text that arrived all at once. CRLF and CR line
    /// endings become LF so a query copied on Windows reads the same here, and
    /// the whole paste is a single edit so one undo takes back exactly what
    /// appeared. A paste over [`PASTE_LIMIT_BYTES`], or one that is empty after
    /// normalisation, is refused without touching the buffer, the cursor, the
    /// revision or the undo history.
    ///
    /// Returns whether the text was inserted.
    pub fn insert_paste(&mut self, text: &str) -> bool {
        if text.len() > PASTE_LIMIT_BYTES {
            return false;
        }
        let normalised = normalise_line_endings(text);
        if normalised.is_empty() {
            return false;
        }
        self.record(EditKind::Insert, false);
        self.last_insert_was_space = normalised.chars().last().is_some_and(char::is_whitespace);
        self.text.insert_str(self.cursor, &normalised);
        self.cursor += normalised.len();
        // The paste is one step and the run it starts ends here, so typing after
        // it is a step of its own rather than being folded back into the paste.
        self.end_undo_run();
        self.after_edit();
        true
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

/// Turns CRLF and lone CR into LF without touching any other character.
fn normalise_line_endings(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests;
