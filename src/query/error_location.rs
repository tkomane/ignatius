//! Map PostgreSQL error positions back to the submitted editor buffer.
//!
//! PostgreSQL reports a one-based character position relative to the statement
//! it received. The editor stores byte offsets, so this module is the one place
//! that translates between those coordinate systems. It deliberately does not
//! parse SQL or fetch metadata: statement boundaries and syntax tokens are the
//! existing local views, and an unmappable position stays unmappable.

use crate::diagnostics::SqlPosition;
use crate::query::{highlight, statements};
use std::ops::Range;

/// The source span of a statement in the editor buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatementSource {
    /// Byte offset of the first character sent for this statement.
    pub start: usize,
    /// Byte offset one past the last character sent for this statement.
    pub end: usize,
    /// One-based statement ordinal in the editor buffer.
    pub number: usize,
}

impl StatementSource {
    fn from_statement(number: usize, statement: &statements::Statement) -> Self {
        Self {
            start: statement.start,
            end: statement.end,
            number,
        }
    }
}

/// A validated location in the current editor buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorLocation {
    /// Byte offset for the editor caret.
    pub cursor: usize,
    /// Smallest source token that can be identified at the caret.
    pub token: Option<Range<usize>>,
    /// One-based line in the whole editor buffer.
    pub line: usize,
    /// One-based character column in the whole editor buffer.
    pub column: usize,
    /// One-based statement ordinal in the editor buffer.
    pub statement_number: usize,
    /// The original one-based server position, retained for JSON and support.
    pub character: u32,
}

/// Finds the exact statement span under the editor cursor.
#[must_use]
pub fn source_at_cursor(sql: &str, cursor: usize) -> Option<StatementSource> {
    if cursor > sql.len() || !sql.is_char_boundary(cursor) {
        return None;
    }
    let parsed = statements::split(sql);
    parsed
        .iter()
        .enumerate()
        .find(|(_, statement)| cursor >= statement.start && cursor <= statement.end)
        .or_else(|| {
            parsed
                .iter()
                .enumerate()
                .rev()
                .find(|(_, statement)| statement.end <= cursor)
        })
        .or_else(|| parsed.iter().enumerate().next())
        .map(|(index, statement)| StatementSource::from_statement(index + 1, statement))
}

/// Maps a PostgreSQL position relative to the failed statement into the buffer.
///
/// `source` is supplied for Run Statement, where PostgreSQL saw one isolated
/// statement but the editor still needs the original ordinal. For Run Buffer,
/// it is `None` and `statement_number` selects the statement in the buffer.
#[must_use]
pub fn locate(
    sql: &str,
    source: Option<StatementSource>,
    statement_number: usize,
    position: SqlPosition,
) -> Option<ErrorLocation> {
    let parsed = statements::split(sql);
    let selected = source.or_else(|| {
        let statement = parsed.get(statement_number.checked_sub(1)?)?;
        Some(StatementSource::from_statement(statement_number, statement))
    })?;
    if selected.number == 0
        || selected.start > selected.end
        || selected.end > sql.len()
        || !sql.is_char_boundary(selected.start)
        || !sql.is_char_boundary(selected.end)
    {
        return None;
    }

    let statement_text = &sql[selected.start..selected.end];
    let character = usize::try_from(position.character).ok()?;
    if character == 0 {
        return None;
    }
    let character_index = character - 1;
    let statement_char_count = statement_text.chars().count();
    if character_index > statement_char_count {
        return None;
    }
    // `nth(len)` intentionally resolves to the end of the statement, which is
    // useful for a server caret at end-of-line. Every other result is a UTF-8
    // boundary because char_indices returns the start of a whole character.
    let relative_byte = statement_text
        .char_indices()
        .nth(character_index)
        .map_or(statement_text.len(), |(byte, _)| byte);
    let cursor = selected.start + relative_byte;
    let (line, column) = line_and_column(sql, cursor)?;
    let token = token_span(sql, &selected, cursor);

    Some(ErrorLocation {
        cursor,
        token,
        line,
        column,
        statement_number: selected.number,
        character: position.character,
    })
}

/// Renders a safe line-oriented source excerpt and caret.
#[must_use]
pub fn render_marker(sql: &str, location: &ErrorLocation) -> Option<String> {
    if location.cursor > sql.len() || !sql.is_char_boundary(location.cursor) || location.line == 0 {
        return None;
    }
    let line_start = sql[..location.cursor]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = sql[location.cursor..]
        .find('\n')
        .map_or(sql.len(), |index| location.cursor + index);
    if location.cursor > line_end {
        return None;
    }
    let line = &sql[line_start..line_end];
    let mut displayed = String::new();
    let mut before = String::new();
    for (byte, character) in line.char_indices() {
        let fragment = display_character(character);
        if line_start + byte < location.cursor {
            before.push_str(&fragment);
        }
        displayed.push_str(&fragment);
    }
    let gutter = format!("{:>4} | ", location.line);
    let mut out = format!("{gutter}{displayed}\n");
    out.push_str(&" ".repeat(gutter.len() + crate::query::value::display_width(&before)));
    out.push('^');
    Some(out)
}

fn line_and_column(sql: &str, cursor: usize) -> Option<(usize, usize)> {
    if cursor > sql.len() || !sql.is_char_boundary(cursor) {
        return None;
    }
    let before = &sql[..cursor];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before.chars().count(), |(_, last)| last.chars().count())
        + 1;
    Some((line, column))
}

fn token_span(sql: &str, statement: &StatementSource, cursor: usize) -> Option<Range<usize>> {
    if cursor >= statement.end {
        return None;
    }
    if let Some(token) = highlight::tokens(sql)
        .into_iter()
        .find(|token| token.start <= cursor && cursor < token.end)
    {
        return Some(token.start..token.end);
    }

    let character = sql[cursor..].chars().next()?;
    if character.is_whitespace() {
        return None;
    }
    let start = expand_left(sql, cursor, statement.start);
    let end = expand_right(sql, cursor, statement.end);
    Some(start..end)
}

fn expand_left(sql: &str, cursor: usize, lower: usize) -> usize {
    let mut start = cursor;
    while start > lower {
        let previous = previous_boundary(sql, start);
        let Some(character) = sql[previous..].chars().next() else {
            break;
        };
        if !is_identifier_character(character) {
            break;
        }
        start = previous;
    }
    start
}

fn expand_right(sql: &str, cursor: usize, upper: usize) -> usize {
    let Some(first) = sql[cursor..].chars().next() else {
        return cursor;
    };
    if !is_identifier_character(first) {
        return next_boundary(sql, cursor).min(upper);
    }
    let mut end = next_boundary(sql, cursor);
    while end < upper {
        let Some(character) = sql[end..].chars().next() else {
            break;
        };
        if !is_identifier_character(character) {
            break;
        }
        end = next_boundary(sql, end);
    }
    end
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$' || !character.is_ascii()
}

fn previous_boundary(sql: &str, from: usize) -> usize {
    let mut index = from.saturating_sub(1);
    while index > 0 && !sql.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn next_boundary(sql: &str, from: usize) -> usize {
    let mut index = (from + 1).min(sql.len());
    while index < sql.len() && !sql.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn display_character(character: char) -> String {
    if character == '\t' {
        "    ".to_owned()
    } else {
        crate::query::value::sanitize_for_display(&character.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(sql: &str, source: Option<StatementSource>, number: usize, character: u32) {
        let location =
            locate(sql, source, number, SqlPosition { character }).expect("position should map");
        assert!(sql.is_char_boundary(location.cursor));
    }

    #[test]
    fn maps_a_later_statement_relative_to_its_own_text() {
        let sql = "SELECT 1;\nINSERT INTO orders (name) VALUES ('x');";
        let location =
            locate(sql, None, 2, SqlPosition { character: 13 }).expect("second statement position");
        assert_eq!(location.statement_number, 2);
        assert_eq!(&sql[location.cursor..][..5], "order");
        assert_eq!((location.line, location.column), (2, 13));
    }

    #[test]
    fn counts_utf8_as_characters_and_never_splits_it() {
        let sql = "SELECT 'café'";
        let location = locate(sql, None, 1, SqlPosition { character: 12 })
            .expect("accented character position");
        assert_eq!(&sql[location.cursor..].chars().next(), &Some('é'));
        assert_eq!(location.token, Some(7..14));
        position(sql, None, 1, 14);
    }

    #[test]
    fn finds_small_tokens_in_comments_and_quoted_or_dollar_strings() {
        let sql = "SELECT x -- note\nFROM \"table\";\nDO $$ body; $$;";
        let comment =
            locate(sql, None, 1, SqlPosition { character: 10 }).expect("comment position");
        assert_eq!(
            &sql[comment.token.clone().expect("comment token")],
            "-- note"
        );

        let quoted = locate(sql, None, 1, SqlPosition { character: 23 })
            .expect("quoted identifier position");
        assert_eq!(
            &sql[quoted.token.clone().expect("quoted token")],
            "\"table\""
        );

        let dollar =
            locate(sql, None, 2, SqlPosition { character: 8 }).expect("dollar body position");
        assert_eq!(
            &sql[dollar.token.clone().expect("dollar token")],
            "$$ body; $$"
        );
    }

    #[test]
    fn handles_tabs_blank_lines_and_end_of_line_without_guessing() {
        let sql = "SELECT 1;\n\n\tWHERE café";
        let location =
            locate(sql, None, 2, SqlPosition { character: 11 }).expect("end of statement position");
        assert_eq!((location.line, location.column), (3, 12));
        let marker = render_marker(sql, &location).expect("safe marker");
        assert!(marker.contains("    WHERE café"), "{marker}");
        assert!(marker.ends_with('^'));
    }

    #[test]
    fn rejects_missing_zero_and_out_of_range_positions() {
        let sql = "SELECT 1; SELECT 2";
        assert!(locate(sql, None, 1, SqlPosition { character: 0 }).is_none());
        assert!(locate(sql, None, 1, SqlPosition { character: 999 }).is_none());
        assert!(locate(sql, None, 3, SqlPosition { character: 1 }).is_none());
        assert!(
            render_marker(
                sql,
                &ErrorLocation {
                    cursor: 999,
                    token: None,
                    line: 1,
                    column: 1,
                    statement_number: 1,
                    character: 1,
                }
            )
            .is_none()
        );
    }

    #[test]
    fn a_run_statement_keeps_the_editor_statement_ordinal() {
        let sql = "SELECT 1;\nSELECT café";
        let source = source_at_cursor(sql, sql.len()).expect("source");
        let location = locate(sql, Some(source), 1, SqlPosition { character: 9 })
            .expect("isolated statement position");
        assert_eq!(location.statement_number, 2);
        assert_eq!((location.line, location.column), (2, 9));
    }

    #[test]
    fn marker_escapes_terminal_controls() {
        let sql = "SELECT \u{1b}[31msecret";
        let location = locate(sql, None, 1, SqlPosition { character: 8 }).expect("position");
        let marker = render_marker(sql, &location).expect("marker");
        assert!(!marker.contains('\u{1b}'), "{marker:?}");
        assert!(marker.contains("\\x1B"), "{marker}");
    }
}
