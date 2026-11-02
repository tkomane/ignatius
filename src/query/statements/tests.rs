use super::*;

fn texts(sql: &str) -> Vec<String> {
    split(sql).into_iter().map(|s| s.text).collect()
}

#[test]
fn splits_plain_statements() {
    assert_eq!(
        texts("SELECT 1; SELECT 2;"),
        vec!["SELECT 1".to_owned(), "SELECT 2".to_owned()]
    );
}

#[test]
fn an_unterminated_final_statement_is_still_a_statement() {
    let statements = split("SELECT 1;\nSELECT 2");
    assert_eq!(statements.len(), 2);
    assert!(statements[0].terminated);
    assert!(
        !statements[1].terminated,
        "the buffer ended without a semicolon"
    );
}

#[test]
fn a_semicolon_inside_a_string_is_not_a_boundary() {
    assert_eq!(texts("SELECT 'a;b'"), vec!["SELECT 'a;b'".to_owned()]);
    assert_eq!(
        texts("SELECT 'it''s; fine', 2"),
        vec!["SELECT 'it''s; fine', 2".to_owned()]
    );
    assert_eq!(
        texts(r"SELECT E'escaped \' quote; still one'"),
        vec![r"SELECT E'escaped \' quote; still one'".to_owned()]
    );
}

#[test]
fn a_semicolon_inside_a_quoted_identifier_is_not_a_boundary() {
    assert_eq!(
        texts(r#"SELECT * FROM "weird;name""#),
        vec![r#"SELECT * FROM "weird;name""#.to_owned()]
    );
    assert_eq!(
        texts(r#"SELECT "a""b;c" FROM t"#),
        vec![r#"SELECT "a""b;c" FROM t"#.to_owned()]
    );
}

#[test]
fn dollar_quoted_function_bodies_survive_intact() {
    let sql =
        "CREATE FUNCTION f() RETURNS int AS $$\n  SELECT 1; SELECT 2;\n$$ LANGUAGE sql;\nSELECT 3;";
    let statements = texts(sql);
    assert_eq!(statements.len(), 2, "{statements:#?}");
    assert!(statements[0].contains("SELECT 1; SELECT 2;"));
    assert_eq!(statements[1], "SELECT 3");
}

#[test]
fn tagged_dollar_quotes_are_matched_by_tag() {
    let sql = "DO $body$ BEGIN PERFORM 1; END $body$;";
    assert_eq!(texts(sql).len(), 1);
    // A different tag inside the body must not close it.
    let sql = "DO $outer$ SELECT $inner$ a; b $inner$; $outer$;";
    assert_eq!(texts(sql).len(), 1);
}

#[test]
fn parameter_placeholders_are_not_dollar_quotes() {
    assert_eq!(
        texts("SELECT $1; SELECT $2;"),
        vec!["SELECT $1".to_owned(), "SELECT $2".to_owned()]
    );
}

#[test]
fn comments_are_not_boundaries_and_comment_only_input_yields_nothing() {
    assert_eq!(
        texts("SELECT 1 -- trailing; comment\n;"),
        vec!["SELECT 1 -- trailing; comment".to_owned()]
    );
    assert_eq!(
        texts("/* a; b */ SELECT 1;"),
        vec!["/* a; b */ SELECT 1".to_owned()]
    );
    assert!(texts("-- nothing here\n").is_empty());
    assert!(texts("/* nor here */").is_empty());
    assert!(texts("   \n\t  ").is_empty());
    assert!(texts(";;;").is_empty(), "stray semicolons send nothing");
}

#[test]
fn nested_block_comments_close_correctly() {
    let sql = "/* outer /* inner */ still comment; */ SELECT 1;";
    assert_eq!(
        texts(sql),
        vec!["/* outer /* inner */ still comment; */ SELECT 1".to_owned()]
    );
}

#[test]
fn offsets_point_at_the_statement_in_the_original_buffer() {
    let sql = "  SELECT 1;\n\n  SELECT 2;";
    let statements = split(sql);
    assert_eq!(&sql[statements[0].start..statements[0].end], "SELECT 1");
    assert_eq!(&sql[statements[1].start..statements[1].end], "SELECT 2");
}

#[test]
fn statement_at_cursor_picks_the_statement_under_the_caret() {
    let sql = "SELECT 1;\nSELECT 2;\nSELECT 3;";
    assert_eq!(statement_at(sql, 3).expect("first").text, "SELECT 1");
    assert_eq!(statement_at(sql, 12).expect("second").text, "SELECT 2");
    // Just after a semicolon, the statement just typed wins.
    assert_eq!(statement_at(sql, 9).expect("boundary").text, "SELECT 1");
    assert_eq!(statement_at(sql, sql.len()).expect("end").text, "SELECT 3");
    assert!(
        statement_at("   ", 1).is_none(),
        "no statement in whitespace"
    );
}

#[test]
fn unterminated_quotes_do_not_hang_or_panic() {
    for sql in [
        "SELECT 'unterminated",
        "SELECT \"unterminated",
        "SELECT $$unterminated",
        "SELECT /* unterminated",
        "SELECT $tag$ unterminated",
        "SELECT E'unterminated \\",
    ] {
        let statements = split(sql);
        assert_eq!(statements.len(), 1, "{sql:?} should stay one statement");
        assert!(!statements[0].terminated);
    }
}

#[test]
fn multibyte_text_keeps_offsets_on_character_boundaries() {
    let sql = "SELECT 'héllo wörld';\nSELECT '日本語';";
    let statements = split(sql);
    assert_eq!(statements.len(), 2);
    for statement in &statements {
        assert_eq!(&sql[statement.start..statement.end], statement.text);
    }
}
