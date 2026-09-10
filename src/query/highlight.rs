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
mod tests {
    use super::*;

    fn kinds(sql: &str) -> Vec<(&str, TokenKind)> {
        tokens(sql)
            .into_iter()
            .map(|token| (&sql[token.start..token.end], token.kind))
            .collect()
    }

    #[test]
    fn keywords_strings_numbers_and_comments_are_each_recognised() {
        let sql = "SELECT 42 FROM orders -- a note\nWHERE name = 'x'";
        assert_eq!(
            kinds(sql),
            vec![
                ("SELECT", TokenKind::Keyword),
                ("42", TokenKind::Number),
                ("FROM", TokenKind::Keyword),
                ("-- a note", TokenKind::Comment),
                ("WHERE", TokenKind::Keyword),
                ("'x'", TokenKind::Literal),
            ],
            "orders and name are identifiers, which need no colour"
        );
    }

    #[test]
    fn keywords_are_recognised_whatever_case_they_are_written_in() {
        for sql in ["select", "SELECT", "SeLeCt"] {
            assert_eq!(kinds(sql), vec![(sql, TokenKind::Keyword)]);
        }
        // A word that merely contains a keyword is not one.
        assert!(kinds("selected").is_empty());
        assert!(kinds("my_select").is_empty());
    }

    #[test]
    fn a_semicolon_or_keyword_inside_a_string_is_part_of_the_string() {
        // The same rule the statement lexer follows, because two lexers that
        // disagree would colour one thing and send another.
        let sql = "SELECT 'SELECT; FROM' AS x";
        assert_eq!(
            kinds(sql),
            vec![
                ("SELECT", TokenKind::Keyword),
                ("'SELECT; FROM'", TokenKind::Literal),
                ("AS", TokenKind::Keyword),
            ]
        );

        let doubled = "SELECT 'it''s here'";
        assert_eq!(kinds(doubled)[1], ("'it''s here'", TokenKind::Literal));

        let escaped = r"SELECT E'a\'b' FROM t";
        assert_eq!(
            kinds(escaped)[1],
            (r"E'a\'b'", TokenKind::Literal),
            "the E prefix belongs to the literal it introduces"
        );
    }

    #[test]
    fn dollar_quoted_bodies_are_literals_and_placeholders_are_not() {
        let sql = "DO $$ SELECT 1; $$";
        assert_eq!(
            kinds(sql),
            vec![
                ("DO", TokenKind::Keyword),
                ("$$ SELECT 1; $$", TokenKind::Literal),
            ]
        );

        let tagged = "DO $body$ x $body$";
        assert_eq!(kinds(tagged)[1].1, TokenKind::Literal);

        let parameter = "SELECT * FROM t WHERE id = $1";
        assert_eq!(
            kinds(parameter).last().copied(),
            Some(("$1", TokenKind::Identifier))
        );
    }

    #[test]
    fn quoted_identifiers_are_their_own_kind() {
        let sql = r#"SELECT "we""ird" FROM "public"."orders""#;
        let found = kinds(sql);
        assert_eq!(found[1], (r#""we""ird""#, TokenKind::Identifier));
        assert!(
            found
                .iter()
                .filter(|(_, kind)| *kind == TokenKind::Identifier)
                .count()
                == 3
        );
    }

    #[test]
    fn a_block_comment_spans_lines_and_nests() {
        let sql = "SELECT /* one\n/* two */ still */ 1";
        let found = kinds(sql);
        assert_eq!(found[1].1, TokenKind::Comment);
        assert_eq!(found[1].0, "/* one\n/* two */ still */");
        assert_eq!(found[2], ("1", TokenKind::Number));
    }

    #[test]
    fn an_unterminated_string_colours_to_the_end_rather_than_giving_up() {
        // Someone is halfway through typing. The buffer must still render.
        let sql = "SELECT 'unfinished";
        assert_eq!(kinds(sql)[1], ("'unfinished", TokenKind::Literal));

        let comment = "SELECT 1 /* unfinished";
        assert_eq!(
            kinds(comment).last().expect("a token").1,
            TokenKind::Comment
        );
    }

    #[test]
    fn the_kind_at_an_offset_is_found_without_scanning() {
        let sql = "SELECT 'abc' FROM t";
        let tokens = tokens(sql);
        assert_eq!(kind_at(&tokens, 0), TokenKind::Keyword);
        assert_eq!(kind_at(&tokens, 5), TokenKind::Keyword);
        assert_eq!(kind_at(&tokens, 6), TokenKind::Plain, "the space");
        assert_eq!(kind_at(&tokens, 8), TokenKind::Literal);
        assert_eq!(kind_at(&tokens, 18), TokenKind::Plain, "the table name");
        assert_eq!(kind_at(&tokens, 9999), TokenKind::Plain, "past the end");
        assert_eq!(kind_at(&[], 0), TokenKind::Plain);
    }

    #[test]
    fn multibyte_text_keeps_every_offset_on_a_character_boundary() {
        let sql = "SELECT 'héllo 日本' AS \"名前\" -- コメント";
        for token in tokens(sql) {
            assert!(sql.is_char_boundary(token.start), "{token:?}");
            assert!(sql.is_char_boundary(token.end), "{token:?}");
        }
        // An identifier in another script is an identifier, not a keyword.
        assert!(kinds("日本語").is_empty());
    }

    #[test]
    fn the_keyword_list_is_sorted_so_the_lookup_is_a_binary_search() {
        let mut sorted = KEYWORDS.to_vec();
        sorted.sort_unstable();
        assert_eq!(KEYWORDS, sorted.as_slice(), "keep KEYWORDS sorted");
        for keyword in KEYWORDS {
            assert!(
                keyword.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{keyword} must be lowercase for the case-insensitive lookup"
            );
            assert!(is_keyword(keyword));
        }
    }

    #[test]
    fn colouring_never_alters_the_buffer() {
        // The property that matters: tokens are a view over the text, so what is
        // drawn can always be reassembled into exactly what was typed.
        let sql = "WITH x AS (SELECT 1) -- note\nSELECT 'a''b', $1, \"c\" FROM x;";
        let mut rebuilt = String::new();
        let mut cursor = 0usize;
        for token in tokens(sql) {
            assert!(token.start >= cursor, "tokens must not overlap");
            rebuilt.push_str(&sql[cursor..token.start]);
            rebuilt.push_str(&sql[token.start..token.end]);
            cursor = token.end;
        }
        rebuilt.push_str(&sql[cursor..]);
        assert_eq!(rebuilt, sql);
    }
}
