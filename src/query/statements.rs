//! Statement boundaries.
//!
//! SQL is never split by searching for semicolons: a semicolon inside a string,
//! an identifier, a comment or a dollar-quoted function body is not a boundary,
//! and treating it as one silently sends the server half a statement.
//!
//! This is a lexer over PostgreSQL's own quoting rules, not a parser. It knows
//! exactly enough to find boundaries and to keep byte offsets accurate so the
//! editor can highlight the statement under the cursor and the server's reported
//! error position can be mapped back into the buffer.
//!
//! Assumption: `standard_conforming_strings` is `on`, the default since
//! PostgreSQL 9.1. Backslashes therefore escape only inside `E'...'` strings.
//! This is stated in `docs/support/compatibility.md`.

/// One statement found in a buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// The statement text, without the trailing semicolon.
    pub text: String,
    /// Byte offset of the first character in the source buffer.
    pub start: usize,
    /// Byte offset one past the last character, excluding the semicolon.
    pub end: usize,
    /// Whether the statement was closed by an explicit semicolon.
    pub terminated: bool,
}

impl Statement {
    /// Whether the statement contains anything besides whitespace and comments.
    #[must_use]
    pub fn is_effectively_empty(&self) -> bool {
        strip_comments(&self.text).trim().is_empty()
    }
}

/// Splits a buffer into statements, preserving byte offsets.
///
/// Empty fragments - trailing whitespace after the last semicolon, or a buffer of
/// nothing but comments - are dropped, so an accidental trailing semicolon does
/// not send an empty statement to the server.
#[must_use]
pub fn split(sql: &str) -> Vec<Statement> {
    let mut statements = Vec::new();
    let bytes = sql.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_single_quoted(sql, i),
            b'"' => i = skip_double_quoted(sql, i),
            b'$' => {
                let after = skip_dollar_quoted(sql, i);
                i = if after == i { i + 1 } else { after };
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => i = skip_line_comment(sql, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(sql, i),
            b';' => {
                push_statement(&mut statements, sql, start, i, true);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    push_statement(&mut statements, sql, start, bytes.len(), false);
    statements
}

fn push_statement(
    statements: &mut Vec<Statement>,
    sql: &str,
    start: usize,
    end: usize,
    terminated: bool,
) {
    let raw = &sql[start..end];
    let leading = raw.len() - raw.trim_start().len();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    let statement = Statement {
        text: trimmed.to_owned(),
        start: start + leading,
        end: start + leading + trimmed.len(),
        terminated,
    };
    if statement.is_effectively_empty() {
        return;
    }
    statements.push(statement);
}

/// Returns the statement containing a byte offset, for "run the statement at the
/// cursor". When the cursor sits between statements, the preceding one wins,
/// which matches where the user's attention is after typing a semicolon.
#[must_use]
pub fn statement_at(sql: &str, byte_offset: usize) -> Option<Statement> {
    let statements = split(sql);
    statements
        .iter()
        .find(|s| byte_offset >= s.start && byte_offset <= s.end)
        .or_else(|| statements.iter().rev().find(|s| s.end <= byte_offset))
        .or_else(|| statements.first())
        .cloned()
}

/// Skips a `'...'` string, honouring the `''` escape and `E'...\'` escapes.
pub(crate) fn skip_single_quoted(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    // An E-prefixed string treats backslash as an escape character.
    let escaped = start > 0 && matches!(bytes[start - 1], b'e' | b'E');
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if escaped => i += 2,
            b'\'' if bytes.get(i + 1) == Some(&b'\'') => i += 2,
            b'\'' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Skips a `"..."` quoted identifier, honouring the `""` escape.
pub(crate) fn skip_double_quoted(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if bytes.get(i + 1) == Some(&b'"') => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Skips a `$tag$...$tag$` body. Returns `start` when this `$` is not a quote,
/// for example the `$1` of a parameter placeholder.
pub(crate) fn skip_dollar_quoted(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        // A tag may not begin with a digit, which is what separates $tag$ from $1.
        if i == start + 1 && bytes[i].is_ascii_digit() {
            return start;
        }
        i += 1;
    }
    if bytes.get(i) != Some(&b'$') {
        return start;
    }
    let tag = &sql[start..=i];
    sql[i + 1..]
        .find(tag)
        .map_or(bytes.len(), |rel| i + 1 + rel + tag.len())
}

pub(crate) fn skip_line_comment(sql: &str, start: usize) -> usize {
    sql[start..]
        .find('\n')
        .map_or(sql.len(), |rel| start + rel + 1)
}

/// Skips `/* ... */`, which PostgreSQL allows to nest.
pub(crate) fn skip_block_comment(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            depth += 1;
            i += 2;
        } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// Removes comments, so what is left is the statement itself.
///
/// Used to decide whether a fragment is empty, and by the classifier, which must
/// never be fooled by a comment in front of a destructive statement.
pub(crate) fn strip_comments(sql: &str) -> String {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => i = skip_line_comment(sql, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(sql, i),
            _ => {
                let next = next_char_boundary(sql, i);
                out.push_str(&sql[i..next]);
                i = next;
            }
        }
    }
    out
}

fn next_char_boundary(sql: &str, i: usize) -> usize {
    let mut next = i + 1;
    while next < sql.len() && !sql.is_char_boundary(next) {
        next += 1;
    }
    next.min(sql.len())
}

#[cfg(test)]
mod tests;
