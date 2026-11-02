use super::*;

fn formatted(source: &str) -> String {
    format_sql(source, source.len())
        .expect("format should succeed")
        .text
}

fn protected(source: &str) -> Vec<&str> {
    scan(source)
        .expect("fixture should scan")
        .into_iter()
        .filter(|token| {
            matches!(
                token.kind,
                TokenKind::StringLiteral
                    | TokenKind::QuotedIdentifier
                    | TokenKind::DollarQuoted
                    | TokenKind::LineComment
                    | TokenKind::BlockComment
            )
        })
        .map(|token| token.text(source))
        .collect()
}

#[test]
fn major_clauses_and_lists_get_a_stable_readable_layout() {
    let source = "select o.id,o.total from orders o where o.total>0 and o.status='ready';";
    assert_eq!(
        formatted(source),
        "select o.id,\n  o.total\nfrom orders o\nwhere o.total > 0\n  and o.status = 'ready';"
    );
}

#[test]
fn joins_and_nested_queries_are_indented_without_a_parser() {
    let source = "SELECT a.id FROM accounts a LEFT JOIN (SELECT id FROM orders WHERE total>0) o ON o.id=a.id;";
    assert_eq!(
        formatted(source),
        "SELECT a.id\nFROM accounts a\nLEFT JOIN (\n  SELECT id\n  FROM orders\n  WHERE total > 0\n) o\nON o.id = a.id;"
    );
}

#[test]
fn operators_casts_arrays_and_parameters_remain_lexically_separated() {
    assert_eq!(
        formatted("SELECT -$1::int + data[1] FROM t WHERE data->>'name'='x';"),
        "SELECT -$1::int + data[1]\nFROM t\nWHERE data->>'name' = 'x';"
    );
    assert_eq!(
        formatted("SELECT count(*) FROM t;"),
        "SELECT count(*)\nFROM t;"
    );
    assert_eq!(
        formatted("SELECT foo$bar, 1e2, .5e-1 FROM t;"),
        "SELECT foo$bar,\n  1e2,\n  .5e-1\nFROM t;"
    );
    assert_eq!(
        formatted("SELECT embedding<->query_vector FROM items;"),
        "SELECT embedding <-> query_vector\nFROM items;"
    );
}

#[test]
fn multiple_statements_keep_order_and_semicolons() {
    assert_eq!(formatted("select 1;select 2"), "select 1;\nselect 2");
}

#[test]
fn formatting_is_idempotent() {
    let once = formatted(
        "WITH recent AS (SELECT id,total FROM orders WHERE total>0) SELECT id,total FROM recent;",
    );
    let twice = formatted(&once);
    assert_eq!(once, twice);
}

#[test]
fn protected_regions_are_copied_byte_for_byte() {
    let source = "SELECT 'a;  b', E'a\\'b', E'\u{1b}]52;c;SGVsbG8=', \"odd; name\", $$SELECT  1;\nDROP TABLE x;$$ /* outer /* inner; */ end */ -- keep;\nFROM t;";
    let result = formatted(source);
    for value in protected(source) {
        assert!(
            result.contains(value),
            "protected region changed: {value:?}"
        );
    }
    assert!(result.contains("SELECT  1;\nDROP TABLE x;"));
    assert!(result.contains("-- keep;"));
    assert!(result.contains('\u{1b}'), "protected control byte changed");
}

#[test]
fn comments_keep_their_content_and_line_role() {
    let source = "SELECT 1\n-- a line comment\nFROM t /* block\n comment */ WHERE a=1;";
    let result = formatted(source);
    assert!(result.contains("-- a line comment\nFROM t"), "{result}");
    assert!(result.contains("/* block\n comment */"), "{result}");
}

#[test]
fn empty_and_comment_only_buffers_are_unchanged() {
    for source in ["", "  \n", "-- only this\n", "/* only this */"] {
        let result = format_sql(source, source.len()).expect("no-op should succeed");
        assert_eq!(result.text, source);
        assert_eq!(result.cursor, source.len());
    }
}

#[test]
fn unterminated_protected_regions_fail_closed() {
    for (source, kind) in [
        ("SELECT 'unfinished", ProtectedRegion::StringLiteral),
        ("SELECT \"unfinished", ProtectedRegion::QuotedIdentifier),
        ("SELECT $tag$ unfinished", ProtectedRegion::DollarQuotedBody),
        ("SELECT /* unfinished", ProtectedRegion::BlockComment),
    ] {
        let error = format_sql(source, source.len()).expect_err("must refuse");
        assert_eq!(
            error,
            FormatError::Unterminated {
                kind,
                position: source
                    .find(if kind == ProtectedRegion::StringLiteral {
                        '\''
                    } else if kind == ProtectedRegion::QuotedIdentifier {
                        '"'
                    } else if kind == ProtectedRegion::BlockComment {
                        '/'
                    } else {
                        '$'
                    })
                    .expect("opening delimiter")
            }
        );
        let message = error.message(source);
        assert!(!message.contains(source), "raw SQL leaked: {message}");
        assert!(message.contains("Format skipped"));
    }
    let nested = format_sql("SELECT /* outer /* inner */", 8).expect_err("must refuse");
    assert_eq!(
        nested,
        FormatError::Unterminated {
            kind: ProtectedRegion::BlockComment,
            position: 7
        }
    );
}

#[test]
fn the_one_mib_limit_is_inclusive_and_refuses_before_formatting() {
    let exact = "x".repeat(MAX_FORMAT_BYTES);
    assert!(format_sql(&exact, exact.len()).is_ok());
    let over = "x".repeat(MAX_FORMAT_BYTES + 1);
    assert_eq!(
        format_sql(&over, over.len()),
        Err(FormatError::TooLarge {
            bytes: MAX_FORMAT_BYTES + 1
        })
    );
}

#[test]
fn cursor_follows_the_same_unicode_token() {
    let source = "select   café,total from t";
    let source_start = source.find("café").expect("source token");
    let cursor = source_start + "ca".len();
    let result = format_sql(source, cursor).expect("format should succeed");
    let output_start = result.text.find("café").expect("output token");
    assert_eq!(result.cursor, output_start + "ca".len());
    assert!(result.text.is_char_boundary(result.cursor));
}

#[test]
fn unicode_identifiers_do_not_merge_with_following_words() {
    assert_eq!(formatted("SELECT café   FROM t;"), "SELECT café\nFROM t;");
}

#[test]
fn cursor_in_collapsed_whitespace_maps_to_an_output_boundary() {
    let source = "select    total from t";
    let cursor = source.find("   total").expect("gap") + 2;
    let result = format_sql(source, cursor).expect("format should succeed");
    assert!(result.text.is_char_boundary(result.cursor));
    assert!(result.cursor <= result.text.find("total").expect("total"));
}

#[test]
fn debug_does_not_print_formatted_sql() {
    let result = format_sql("SELECT secret_value FROM private_table", 4).expect("format");
    let debug = format!("{result:?}");
    assert!(!debug.contains("secret_value"));
    assert!(!debug.contains("private_table"));
    assert!(debug.contains("<hidden>"));
}
