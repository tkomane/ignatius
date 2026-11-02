//! Colouring SQL.
//!
//! This is decoration and nothing else. It changes no character, decides no
//! behaviour, and is absent with colour off; the buffer means exactly the same
//! thing with and without it. Anything that reads a statement to decide what to
//! do uses [`crate::query::statements`] or [`crate::query::classify()`], never
//! this.
//!
//! It shares the quoting rules of the statement lexer rather than reimplementing
//! them, because two lexers that disagree about where a string ends would show
//! the user one thing and send the server another.
//!
//! A keyword this list has never heard of is coloured as an identifier, which
//! costs nothing. That is the whole point of keeping the list short and honest
//! rather than exhaustive and stale.

use crate::query::statements;

/// What a run of characters is, for colouring only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A reserved or otherwise notable SQL word.
    Keyword,
    /// A string literal, including dollar-quoted bodies.
    Literal,
    /// A numeric literal.
    Number,
    /// A comment, in either form.
    Comment,
    /// A quoted identifier, or a `$1` parameter placeholder.
    Identifier,
    /// Anything else: bare identifiers, operators, punctuation, whitespace.
    Plain,
}

/// A run of characters and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
    /// What it is.
    pub kind: TokenKind,
}

/// Finds the coloured runs in a buffer.
///
/// Only runs that are not [`TokenKind::Plain`] are returned, so the common case
/// costs nothing to store. Offsets are byte offsets into the input, and every
/// one falls on a character boundary.
#[must_use]
pub fn tokens(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let byte = bytes[i];
        match byte {
            b'\'' => {
                let end = statements::skip_single_quoted(sql, i);
                // An E prefix belongs to the literal it introduces, but only
                // when the E stands alone: the last letter of an identifier is
                // not a prefix.
                let prefixed = i > 0
                    && matches!(bytes[i - 1], b'e' | b'E')
                    && (i == 1 || !is_word_byte(bytes[i - 2]));
                let start = if prefixed { i - 1 } else { i };
                push(&mut tokens, start, end, TokenKind::Literal);
                i = end;
            }
            b'"' => {
                let end = statements::skip_double_quoted(sql, i);
                push(&mut tokens, i, end, TokenKind::Identifier);
                i = end;
            }
            b'$' => {
                let end = statements::skip_dollar_quoted(sql, i);
                if end > i {
                    push(&mut tokens, i, end, TokenKind::Literal);
                    i = end;
                } else {
                    // Not a dollar quote: `$1` and friends are placeholders.
                    let mut j = i + 1;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > i + 1 {
                        push(&mut tokens, i, j, TokenKind::Identifier);
                    }
                    i = j.max(i + 1);
                }
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                let end = statements::skip_line_comment(sql, i);
                // The newline that ends the comment is not part of it.
                let end = if sql[i..end].ends_with('\n') {
                    end - 1
                } else {
                    end
                };
                push(&mut tokens, i, end, TokenKind::Comment);
                i = end;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let end = statements::skip_block_comment(sql, i);
                push(&mut tokens, i, end, TokenKind::Comment);
                i = end;
            }
            b'0'..=b'9' => {
                let mut j = i;
                while j < bytes.len()
                    && (bytes[j].is_ascii_digit() || bytes[j] == b'.' || bytes[j] == b'_')
                {
                    j += 1;
                }
                push(&mut tokens, i, j, TokenKind::Number);
                i = j;
            }
            _ if is_word_start(byte) => {
                let mut j = i;
                while j < bytes.len() && is_word_byte(bytes[j]) {
                    j += 1;
                }
                if is_keyword(&sql[i..j]) {
                    push(&mut tokens, i, j, TokenKind::Keyword);
                }
                i = j;
            }
            _ => i += 1,
        }
    }

    tokens
}

/// What the character at an offset is, for the renderer.
///
/// The tokens are in order and do not overlap, so this is a binary search rather
/// than a scan: it runs once per character drawn.
#[must_use]
pub fn kind_at(tokens: &[Token], offset: usize) -> TokenKind {
    match tokens.binary_search_by(|token| {
        if offset < token.start {
            std::cmp::Ordering::Greater
        } else if offset >= token.end {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(index) => tokens[index].kind,
        Err(_) => TokenKind::Plain,
    }
}

fn push(tokens: &mut Vec<Token>, start: usize, end: usize, kind: TokenKind) {
    if end > start {
        tokens.push(Token { start, end, kind });
    }
}

pub(crate) const fn is_word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

pub(crate) const fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

pub(crate) fn is_keyword(word: &str) -> bool {
    if !word.is_ascii() {
        return false;
    }
    let lower = word.to_ascii_lowercase();
    KEYWORDS.binary_search(&lower.as_str()).is_ok()
}

/// Words coloured as keywords.
///
/// Deliberately not the full reserved-word list. This exists to make a statement
/// scannable, and a word that is missing simply looks like an identifier, which
/// is a cost of nothing. Kept sorted, which a test enforces.
pub(crate) const KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "analyze",
    "and",
    "any",
    "as",
    "asc",
    "begin",
    "between",
    "by",
    "cascade",
    "case",
    "cast",
    "check",
    "column",
    "comment",
    "commit",
    "concurrently",
    "conflict",
    "constraint",
    "copy",
    "create",
    "cross",
    "current_date",
    "current_timestamp",
    "database",
    "default",
    "delete",
    "desc",
    "distinct",
    "do",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "explain",
    "extension",
    "false",
    "fetch",
    "filter",
    "first",
    "for",
    "foreign",
    "from",
    "full",
    "function",
    "grant",
    "group",
    "having",
    "if",
    "ilike",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "last",
    "lateral",
    "left",
    "like",
    "limit",
    "materialized",
    "merge",
    "not",
    "nothing",
    "null",
    "nulls",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "over",
    "partition",
    "primary",
    "procedure",
    "references",
    "refresh",
    "reindex",
    "rename",
    "replace",
    "returning",
    "revoke",
    "right",
    "rollback",
    "row",
    "savepoint",
    "schema",
    "select",
    "sequence",
    "set",
    "table",
    "temporary",
    "then",
    "to",
    "trigger",
    "true",
    "truncate",
    "union",
    "unique",
    "update",
    "using",
    "vacuum",
    "values",
    "view",
    "when",
    "where",
    "window",
    "with",
];

#[cfg(test)]
mod tests;
