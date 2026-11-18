//! Sidecar tests for the pure `psql` meta-command lexer and bounded watch dispatcher.

use super::*;

#[test]
fn constants_and_bounds_match_contract() {
    assert_eq!(DEFAULT_WATCH_INTERVAL_SECS, 2);
    assert_eq!(MAX_WATCH_INTERVAL_SECS, 3600);
    assert_eq!(MAX_WATCH_ITERATIONS, 1000);
    assert_eq!(CATALOG_ROW_LIMIT, 500);
}

#[test]
fn contains_metacommand_detects_standalone_and_companion_watch_only_outside_quotes() {
    assert!(contains_metacommand("\\dt"));
    assert!(contains_metacommand("  \\d+ orders;  "));
    assert!(contains_metacommand("SELECT now();\n\\watch 5"));
    assert!(contains_metacommand("SELECT now() \\watch 2"));
    assert!(contains_metacommand("SELECT 1;\\watch"));

    assert!(!contains_metacommand(""));
    assert!(!contains_metacommand("   \n\t  "));
    assert!(!contains_metacommand("SELECT 1;"));
    assert!(!contains_metacommand("SELECT '\\watch 2';"));
    assert!(!contains_metacommand("SELECT E'hello \\watch 2';"));
    assert!(!contains_metacommand("SELECT \"\\watch 2\" FROM t;"));
    assert!(!contains_metacommand("SELECT $$\\watch 2$$;"));
    assert!(!contains_metacommand("SELECT $body$\\watch 2$body$;"));
    assert!(!contains_metacommand("SELECT 1; -- \\watch 2"));
    assert!(!contains_metacommand("SELECT 1 /* \\watch 2 */;"));
    assert!(!contains_metacommand(
        "SELECT 1 /* outer /* inner */ \\watch 2 */;"
    ));
    assert!(!contains_metacommand("SELECT 1;\n\\watch 2\nSELECT 2;"));
}

#[test]
fn plain_sql_with_embedded_watch_or_backslashes_returns_none() {
    for sql in [
        "SELECT 1;",
        "SELECT '\\dt';",
        "SELECT E'line1\\nline2 \\watch 2';",
        "SELECT \"col\\watch\" FROM items;",
        "SELECT $$\\watch 3$$;",
        "SELECT $fn$BEGIN RETURN '\\watch 4'; END;$fn$;",
        "SELECT 1 -- \\watch 5",
        "SELECT 1 /* \\watch 6 */",
        "SELECT 1 /* a /* b */ \\watch 7 */",
        "SELECT 'unterminated \\watch 2",
    ] {
        assert_eq!(
            parse_input(sql),
            None,
            "SQL must not be parsed as metacommand: {sql}"
        );
    }
}

#[test]
fn catalog_commands_generate_bounded_pg_catalog_queries() {
    let cases = [
        ("\\d", "\\d", "pg_catalog.pg_class", "LIMIT 500;"),
        ("\\d;", "\\d", "pg_catalog.pg_class", "LIMIT 500;"),
        ("\\d+", "\\d+", "pg_total_relation_size", "LIMIT 500;"),
        ("\\dt", "\\dt", "c.relkind IN ('r', 'p')", "LIMIT 500;"),
        ("\\dt+", "\\dt+", "pg_total_relation_size", "LIMIT 500;"),
        ("\\dv", "\\dv", "c.relkind IN ('v')", "LIMIT 500;"),
        ("\\dm", "\\dm", "c.relkind IN ('m')", "LIMIT 500;"),
        ("\\di", "\\di", "c.relkind IN ('i', 'I')", "LIMIT 500;"),
        ("\\ds", "\\ds", "c.relkind IN ('S')", "LIMIT 500;"),
        ("\\dn", "\\dn", "pg_catalog.pg_namespace", "LIMIT 500;"),
        ("\\du", "\\du", "pg_catalog.pg_roles", "LIMIT 500;"),
        ("\\dg", "\\dg", "pg_catalog.pg_roles", "LIMIT 500;"),
        ("\\df", "\\df", "pg_catalog.pg_proc", "LIMIT 500;"),
        ("\\dx", "\\dx", "pg_catalog.pg_extension", "LIMIT 500;"),
        ("\\l", "\\l", "pg_catalog.pg_database", "LIMIT 500;"),
        ("\\l+", "\\l+", "pg_catalog.pg_database_size", "LIMIT 500;"),
    ];

    for (input, expected_cmd, expected_fragment, expected_limit) in cases {
        let parsed = parse_input(input)
            .unwrap_or_else(|| panic!("expected Some for {input}"))
            .unwrap_or_else(|e| panic!("expected Ok for {input}: {}", e.headline));
        match parsed {
            MetaCommand::CatalogQuery {
                command,
                title,
                sql,
            } => {
                assert_eq!(command, expected_cmd, "input: {input}");
                assert!(!title.is_empty(), "title empty for {input}");
                assert!(
                    sql.contains(expected_fragment),
                    "sql for {input} missing `{expected_fragment}`: {sql}"
                );
                assert!(
                    sql.ends_with(expected_limit),
                    "sql for {input} must be bounded by `{expected_limit}`: {sql}"
                );
            }
            other => panic!("expected CatalogQuery for {input}, got {other:?}"),
        }
    }

    let conninfo = parse_input("\\conninfo").expect("Some").expect("Ok");
    match conninfo {
        MetaCommand::CatalogQuery { command, sql, .. } => {
            assert_eq!(command, "\\conninfo");
            assert!(sql.contains("current_database()"));
            assert!(sql.contains("current_user"));
            assert!(sql.contains("inet_server_addr()"));
            assert!(sql.contains("inet_server_port()"));
            assert!(sql.contains("version()"));
        }
        other => panic!("expected CatalogQuery for \\conninfo, got {other:?}"),
    }
}

#[test]
fn describe_table_inspects_attributes_types_defaults_not_null_and_comments() {
    let basic = parse_input("\\d orders").expect("Some").expect("Ok");
    match basic {
        MetaCommand::CatalogQuery { command, sql, .. } => {
            assert_eq!(command, "\\d");
            assert!(sql.contains("pg_catalog.pg_attribute"));
            assert!(sql.contains("pg_catalog.format_type"));
            assert!(sql.contains("NOT a.attnotnull AS nullable"));
            assert!(sql.contains("pg_catalog.pg_attrdef"));
            assert!(sql.contains("pg_catalog.col_description"));
            assert!(sql.contains("c.relname LIKE 'orders' ESCAPE '!'"));
            assert!(sql.ends_with("LIMIT 500;"));
        }
        other => panic!("expected CatalogQuery, got {other:?}"),
    }

    let detailed = parse_input("\\d+ billing.orders;")
        .expect("Some")
        .expect("Ok");
    match detailed {
        MetaCommand::CatalogQuery { command, sql, .. } => {
            assert_eq!(command, "\\d+");
            assert!(sql.contains("n.nspname LIKE 'billing' ESCAPE '!'"));
            assert!(sql.contains("c.relname LIKE 'orders' ESCAPE '!'"));
            assert!(sql.contains("attstorage"));
            assert!(sql.contains("pg_total_relation_size"));
            assert!(sql.contains("pg_catalog.pg_index"));
            assert!(sql.ends_with("LIMIT 500;"));
        }
        other => panic!("expected CatalogQuery, got {other:?}"),
    }
}

#[test]
fn pattern_globs_convert_wildcards_and_split_schema_and_double_quotes() {
    let parsed = parse_input("\\dt app_*.ord?").expect("Some").expect("Ok");
    match parsed {
        MetaCommand::CatalogQuery { sql, .. } => {
            assert!(sql.contains("n.nspname LIKE 'app!_%' ESCAPE '!'"));
            assert!(sql.contains("c.relname LIKE 'ord_' ESCAPE '!'"));
        }
        other => panic!("expected CatalogQuery, got {other:?}"),
    }

    let quoted = parse_input("\\d \"public\".\"my.table%1!\"")
        .expect("Some")
        .expect("Ok");
    match quoted {
        MetaCommand::CatalogQuery { sql, .. } => {
            assert!(sql.contains("n.nspname LIKE 'public' ESCAPE '!'"));
            assert!(sql.contains("c.relname LIKE 'my.table!%1!!' ESCAPE '!'"));
        }
        other => panic!("expected CatalogQuery, got {other:?}"),
    }

    let err_multi_dot = parse_input("\\dt a.b.c").expect("Some").unwrap_err();
    assert_eq!(err_multi_dot.kind, DiagnosticKind::Usage);

    let err_empty_schema = parse_input("\\dt .orders").expect("Some").unwrap_err();
    assert_eq!(err_empty_schema.kind, DiagnosticKind::Usage);

    let err_unqualified_dot = parse_input("\\dn a.b").expect("Some").unwrap_err();
    assert_eq!(err_unqualified_dot.kind, DiagnosticKind::Usage);
}

#[test]
fn pattern_quoting_blocks_sql_injection_attempts() {
    // Quoted single-token injection payload is escaped inside the LIKE string literal.
    let quoted_payload = parse_input("\\dt \"O'Reilly; DROP TABLE x; --\"")
        .expect("Some")
        .expect("Ok");
    match quoted_payload {
        MetaCommand::CatalogQuery { sql, .. } => {
            assert!(
                sql.contains("c.relname LIKE 'O''Reilly; DROP TABLE x; --' ESCAPE '!'"),
                "expected escaped literal, got: {sql}"
            );
            assert!(sql.starts_with("SELECT "));
            assert_eq!(sql.matches(';').count(), 3); // two inside literal, one terminator
            assert_eq!(crate::query::statements::split(&sql).len(), 1);
        }
        other => panic!("expected CatalogQuery, got {other:?}"),
    }

    // Compact unquoted single-token quote payload is also escaped cleanly.
    let compact_payload = parse_input("\\d+ O'Reilly;--").expect("Some").expect("Ok");
    match compact_payload {
        MetaCommand::CatalogQuery { sql, .. } => {
            assert!(sql.contains("c.relname LIKE 'O''Reilly;--' ESCAPE '!'"));
        }
        other => panic!("expected CatalogQuery, got {other:?}"),
    }

    // Space-separated injection payload is rejected as extra arguments before any SQL is built.
    let multi_token_payload = parse_input("\\dt O'Reilly; DROP TABLE x; --")
        .expect("Some")
        .unwrap_err();
    assert_eq!(multi_token_payload.kind, DiagnosticKind::Usage);
}

#[test]
fn toggle_commands_parse_optional_and_explicit_booleans() {
    assert_eq!(
        parse_input("\\x"),
        Some(Ok(MetaCommand::ToggleExpanded(None)))
    );
    assert_eq!(
        parse_input("\\x on;"),
        Some(Ok(MetaCommand::ToggleExpanded(Some(true))))
    );
    assert_eq!(
        parse_input("\\x OFF"),
        Some(Ok(MetaCommand::ToggleExpanded(Some(false))))
    );
    assert_eq!(
        parse_input("\\timing"),
        Some(Ok(MetaCommand::ToggleTiming(None)))
    );
    assert_eq!(
        parse_input("\\timing true"),
        Some(Ok(MetaCommand::ToggleTiming(Some(true))))
    );
    assert_eq!(
        parse_input("\\timing 0;"),
        Some(Ok(MetaCommand::ToggleTiming(Some(false))))
    );

    assert_eq!(
        parse_input("\\x maybe").expect("Some").unwrap_err().kind,
        DiagnosticKind::Usage
    );
    assert_eq!(
        parse_input("\\timing on extra")
            .expect("Some")
            .unwrap_err()
            .kind,
        DiagnosticKind::Usage
    );
}

#[test]
fn watch_command_handles_standalone_companion_stop_and_interval_bounds() {
    assert_eq!(
        parse_input("\\watch"),
        Some(Ok(MetaCommand::StartWatch {
            interval_secs: DEFAULT_WATCH_INTERVAL_SECS,
            companion_sql: None,
        }))
    );
    assert_eq!(
        parse_input("\\watch 15;"),
        Some(Ok(MetaCommand::StartWatch {
            interval_secs: 15,
            companion_sql: None,
        }))
    );
    assert_eq!(
        parse_input("\\watch 3600"),
        Some(Ok(MetaCommand::StartWatch {
            interval_secs: MAX_WATCH_INTERVAL_SECS,
            companion_sql: None,
        }))
    );
    assert_eq!(
        parse_input("SELECT count(*) FROM orders;\n\\watch 5"),
        Some(Ok(MetaCommand::StartWatch {
            interval_secs: 5,
            companion_sql: Some("SELECT count(*) FROM orders;".to_owned()),
        }))
    );
    assert_eq!(
        parse_input("SELECT now() \\watch 2"),
        Some(Ok(MetaCommand::StartWatch {
            interval_secs: 2,
            companion_sql: Some("SELECT now()".to_owned()),
        }))
    );
    assert_eq!(
        parse_input("SELECT now();\n\\watch\n  "),
        Some(Ok(MetaCommand::StartWatch {
            interval_secs: DEFAULT_WATCH_INTERVAL_SECS,
            companion_sql: Some("SELECT now();".to_owned()),
        }))
    );
    assert_eq!(
        parse_input("\\watch stop"),
        Some(Ok(MetaCommand::StopWatch))
    );
    assert_eq!(
        parse_input("\\watch OFF;"),
        Some(Ok(MetaCommand::StopWatch))
    );

    for invalid in [
        "\\watch 0",
        "\\watch -1",
        "\\watch 3601",
        "\\watch 99999999999999999999999999",
        "\\watch abc",
        "\\watch 1.5",
        "\\watch 2 extra",
        "SELECT 1;\n\\watch 0",
        "SELECT 1;\n\\watch stop",
    ] {
        let err = parse_input(invalid)
            .unwrap_or_else(|| panic!("expected Some(Err) for `{invalid}`"))
            .unwrap_err();
        assert_eq!(err.kind, DiagnosticKind::Usage, "input: {invalid}");
    }
}

#[test]
fn shell_escapes_and_unsupported_psql_commands_are_refused_with_guidance() {
    let shell_err = parse_input("\\! ls -la").expect("Some").unwrap_err();
    assert_eq!(shell_err.kind, DiagnosticKind::Usage);
    assert!(shell_err.headline.contains("\\!"));
    assert!(
        shell_err
            .next_action
            .as_deref()
            .is_some_and(|n| n.contains("\\?"))
    );

    let include_err = parse_input("\\i schema.sql").expect("Some").unwrap_err();
    assert_eq!(include_err.kind, DiagnosticKind::Usage);
    assert!(
        include_err
            .next_action
            .as_deref()
            .is_some_and(|n| n.contains("ignatius query -f") && n.contains("\\?"))
    );

    let copy_err = parse_input("\\copy orders to 'out.csv'")
        .expect("Some")
        .unwrap_err();
    assert_eq!(copy_err.kind, DiagnosticKind::Usage);
    assert!(
        copy_err
            .next_action
            .as_deref()
            .is_some_and(|n| n.contains("ignatius import") && n.contains("\\?"))
    );

    for bad in [
        "\\set FOO bar",
        "\\",
        "\\dt;;",
        "\\dt a b",
        "\\dt\n\\du",
        "\\d+orders",
    ] {
        let err = parse_input(bad)
            .unwrap_or_else(|| panic!("expected Some(Err) for `{bad}`"))
            .unwrap_err();
        assert_eq!(err.kind, DiagnosticKind::Usage, "input: {bad}");
    }
}

#[test]
fn help_command_and_result_set_cover_supported_commands() {
    assert_eq!(parse_input("\\?"), Some(Ok(MetaCommand::Help)));
    assert_eq!(parse_input("\\h;"), Some(Ok(MetaCommand::Help)));
    assert_eq!(
        parse_input("\\? extra").expect("Some").unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let help = help_result();
    assert_eq!(help.columns, vec!["Command", "Syntax", "Description"]);
    assert!(!help.rows.is_empty());
    assert!(!help.is_truncated());
    let commands: Vec<&str> = help
        .rows
        .iter()
        .filter_map(|row| row.first().and_then(Cell::raw))
        .collect();
    for expected in [
        "\\d",
        "\\d+",
        "\\dt",
        "\\du",
        "\\l+",
        "\\conninfo",
        "\\x",
        "\\timing",
        "\\watch",
    ] {
        assert!(
            commands.contains(&expected),
            "help_result missing {expected}"
        );
    }
}
