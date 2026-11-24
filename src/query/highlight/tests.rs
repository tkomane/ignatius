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

#[test]
fn matching_delimiter_pairs_parentheses_and_brackets_at_or_after_cursor() {
    let sql = "SELECT arr[coalesce(1, 2)]";
    // '[' at 10, '(' at 19, ')' at 24, ']' at 25.
    assert_eq!(matching_delimiter(sql, 10), Some((10, 25)));
    assert_eq!(matching_delimiter(sql, 11), Some((10, 25)));
    assert_eq!(matching_delimiter(sql, 25), Some((25, 10)));
    assert_eq!(matching_delimiter(sql, 26), Some((25, 10)));
    assert_eq!(matching_delimiter(sql, 19), Some((19, 24)));
    assert_eq!(matching_delimiter(sql, 20), Some((19, 24)));
    assert_eq!(matching_delimiter(sql, 24), Some((24, 19)));

    // Cursor directly on a delimiter takes precedence over the character before it.
    let adjacent = "SELECT (1)(2)";
    assert_eq!(matching_delimiter(adjacent, 9), Some((9, 7)));
    assert_eq!(matching_delimiter(adjacent, 10), Some((10, 12)));
    assert_eq!(matching_delimiter(adjacent, 13), Some((12, 10)));

    // Unrelated positions and out-of-bounds or mid-codepoint cursors yield None.
    assert_eq!(matching_delimiter(sql, 0), None);
    assert_eq!(matching_delimiter(sql, 9), None);
    assert_eq!(matching_delimiter(sql, 999), None);
    assert_eq!(matching_delimiter("('é')", 3), None);
}

#[test]
fn matching_delimiter_skips_strings_comments_and_quoted_identifiers() {
    // Delimiters inside literals, comments, and quoted identifiers are ignored.
    let sql = "SELECT '(', -- )\n/* ] */ \"a(b\" , (1 + 2)";
    let open = sql.rfind('(').unwrap();
    let close = sql.rfind(')').unwrap();
    assert_eq!(matching_delimiter(sql, open), Some((open, close)));
    assert_eq!(matching_delimiter(sql, close), Some((close, open)));
    assert_eq!(matching_delimiter(sql, close + 1), Some((close, open)));

    // Cursor inside a string/comment/quoted identifier does not match delimiters.
    assert_eq!(matching_delimiter(sql, sql.find("'('").unwrap() + 1), None);
    assert_eq!(matching_delimiter(sql, sql.find("-- )").unwrap() + 3), None);
    assert_eq!(
        matching_delimiter(sql, sql.find("\"a(b\"").unwrap() + 2),
        None
    );

    // Dollar-quoted strings and E-strings are skipped as literals.
    let dollar = "DO $$ SELECT (1); $$";
    assert_eq!(matching_delimiter(dollar, dollar.find('(').unwrap()), None);

    // Mismatched or unclosed delimiters never guess a false partner.
    assert_eq!(matching_delimiter("SELECT (a[1)", 7), None);
    assert_eq!(matching_delimiter("SELECT (a[1)", 11), None);
    assert_eq!(matching_delimiter("SELECT (a", 7), None);
    assert_eq!(matching_delimiter("SELECT a)", 8), None);
}
