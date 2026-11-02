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
    let location =
        locate(sql, None, 1, SqlPosition { character: 12 }).expect("accented character position");
    assert_eq!(&sql[location.cursor..].chars().next(), &Some('é'));
    assert_eq!(location.token, Some(7..14));
    position(sql, None, 1, 14);
}

#[test]
fn finds_small_tokens_in_comments_and_quoted_or_dollar_strings() {
    let sql = "SELECT x -- note\nFROM \"table\";\nDO $$ body; $$;";
    let comment = locate(sql, None, 1, SqlPosition { character: 10 }).expect("comment position");
    assert_eq!(
        &sql[comment.token.clone().expect("comment token")],
        "-- note"
    );

    let quoted =
        locate(sql, None, 1, SqlPosition { character: 23 }).expect("quoted identifier position");
    assert_eq!(
        &sql[quoted.token.clone().expect("quoted token")],
        "\"table\""
    );

    let dollar = locate(sql, None, 2, SqlPosition { character: 8 }).expect("dollar body position");
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
