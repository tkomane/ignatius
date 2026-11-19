use super::*;

#[test]
fn constants_match_documented_bounds() {
    assert_eq!(MAX_RUNBOOK_CELLS, 200);
    assert_eq!(MAX_RUNBOOK_BYTES, 1024 * 1024);
}

#[test]
fn parse_markdown_extracts_headings_fences_and_directives() {
    let md = "\u{FEFF}# Incident Triage ##\r\n\
        \r\n\
        ## Connections\r\n\
        ```sql\r\n\
        -- @name: active-conns\r\n\
        -- @readonly\r\n\
        -- @expect-rows: 1\r\n\
        SELECT count(*) FROM pg_stat_activity;\r\n\
        ```\r\n\
        \r\n\
        ### Locks Detail\r\n\
        ~~~postgresql\r\n\
        -- @name: cell-2\r\n\
        SELECT pid FROM pg_locks;\r\n\
        ~~~\r\n\
        \r\n\
        ````pgsql\r\n\
        -- comment mentioning ``` inside 4-backtick fence\r\n\
        INSERT INTO audit_log(note) VALUES ('checked');\r\n\
        ````\r\n";

    let runbook = parse_markdown_runbook(md, "  Daily Check  ").expect("valid markdown runbook");
    assert_eq!(runbook.title, "Daily Check");
    assert!(!runbook.truncated);
    assert_eq!(runbook.len(), 3);

    let c0 = &runbook.cells[0];
    assert_eq!(c0.index, 0);
    assert_eq!(c0.name, "active-conns");
    assert_eq!(c0.heading.as_deref(), Some("Connections"));
    assert_eq!(c0.start_line, 5);
    assert!(!c0.is_write);
    assert!(!c0.is_destructive);
    assert!(c0.readonly_hint);
    assert_eq!(c0.expect_rows, Some(1));
    assert!(c0.sql.contains("SELECT count(*) FROM pg_stat_activity;"));

    let c1 = &runbook.cells[1];
    assert_eq!(c1.index, 1);
    assert_eq!(c1.name, "cell-2");
    assert_eq!(c1.heading.as_deref(), Some("Locks Detail"));
    assert_eq!(c1.start_line, 13);
    assert!(!c1.is_write);
    assert!(!c1.readonly_hint);
    assert_eq!(c1.expect_rows, None);

    // Cell index 2 would default to "cell-3", avoiding any collision with explicit "cell-2".
    let c2 = &runbook.cells[2];
    assert_eq!(c2.index, 2);
    assert_eq!(c2.name, "cell-3");
    assert_eq!(c2.heading.as_deref(), Some("Locks Detail"));
    assert_eq!(c2.start_line, 18);
    assert!(c2.is_write);
    assert!(!c2.is_destructive);
}

#[test]
fn parse_markdown_ignores_prose_spans_non_sql_and_unclosed_non_sql_blocks() {
    let md = "# Operations\n\
        Use inline ```sql SELECT 'prose';``` carefully.\n\
        \n\
        ```bash\n\
        # Fake heading inside bash block\n\
        echo 'not sql'\n\
        ```\n\
        \n\
        ```\n\
        # Unlabelled code block heading\n\
        SELECT 999;\n\
        ```\n\
        \n\
        ```sql\n\
        -- only a comment inside empty placeholder\n\
        ```\n\
        \n\
        ```sql\n\
        \n\
        SELECT 42 AS answer;\n\
        ```\n\
        \n\
        ```json\n\
        {\"unclosed\": true}\n";

    let runbook = parse_markdown_runbook(md, "").expect("valid runbook with ignored non-sql");
    assert_eq!(runbook.title, "Untitled Runbook");
    assert_eq!(runbook.cells.len(), 1);
    assert_eq!(runbook.cells[0].heading.as_deref(), Some("Operations"));
    assert_eq!(runbook.cells[0].start_line, 20);
    assert_eq!(runbook.cells[0].sql, "SELECT 42 AS answer;");
    assert_eq!(runbook.cells[0].name, "cell-1");
}

#[test]
fn parse_markdown_rejects_oversized_empty_and_unclosed_sql() {
    let oversized = format!("```sql\nSELECT '{}';\n```\n", "a".repeat(MAX_RUNBOOK_BYTES));
    let err = parse_markdown_runbook(&oversized, "too-big").expect_err("oversized payload refused");
    assert_eq!(err.kind, DiagnosticKind::Usage);

    let err_empty = parse_markdown_runbook("   \n\t\n", "empty").expect_err("empty refused");
    assert_eq!(err_empty.kind, DiagnosticKind::Usage);

    let err_prose = parse_markdown_runbook("# Heading\nOnly prose `SELECT 1;`.\n", "prose")
        .expect_err("prose-only refused");
    assert_eq!(err_prose.kind, DiagnosticKind::Usage);

    let err_comments = parse_markdown_runbook("```sql\n-- comment only\n```\n", "comments")
        .expect_err("comment-only SQL block refused");
    assert_eq!(err_comments.kind, DiagnosticKind::Usage);

    let err_unclosed = parse_markdown_runbook(
        "```sql\nSELECT 1;\n```\n\n```sql\nDELETE FROM orders;\n",
        "unclosed",
    )
    .expect_err("unclosed SQL fence refused");
    assert_eq!(err_unclosed.kind, DiagnosticKind::Usage);
    assert!(
        err_unclosed
            .likely_cause
            .as_deref()
            .is_some_and(|c| c.contains("line 5"))
    );
}

#[test]
fn parse_markdown_truncates_at_max_runbook_cells() {
    let mut exact = String::from("# Many\n");
    for i in 0..MAX_RUNBOOK_CELLS {
        let _ = std::fmt::Write::write_fmt(&mut exact, format_args!("```sql\nSELECT {i};\n```\n"));
    }
    let exact_rb = parse_markdown_runbook(&exact, "exact").expect("200 cells ok");
    assert_eq!(exact_rb.cells.len(), MAX_RUNBOOK_CELLS);
    assert!(!exact_rb.truncated);

    let mut over = exact;
    for i in MAX_RUNBOOK_CELLS..(MAX_RUNBOOK_CELLS + 5) {
        let _ = std::fmt::Write::write_fmt(&mut over, format_args!("```sql\nSELECT {i};\n```\n"));
    }
    let over_rb = parse_markdown_runbook(&over, "over").expect("bounded at 200");
    assert_eq!(over_rb.cells.len(), MAX_RUNBOOK_CELLS);
    assert!(over_rb.truncated);
    assert_eq!(over_rb.cells.last().map(|c| c.index), Some(199));
}

#[test]
fn parse_markdown_classifies_impact_and_directives_safely() {
    let md = "```sql\n\
        -- @name: read-with-literal-directive\n\
        SELECT '-- @readonly' AS s, $$-- @name: fake$$ AS d;\n\
        ```\n\
        \n\
        ```sql\n\
        -- @name: ddl-step\n\
        CREATE INDEX CONCURRENTLY idx_orders_ts ON orders(created_at);\n\
        ```\n\
        \n\
        ```sql\n\
        -- @name: sneaky-readonly-delete\n\
        -- @readonly\n\
        SELECT 1;\n\
        WITH doomed AS (SELECT id FROM orders LIMIT 1) DELETE FROM orders USING doomed WHERE orders.id = doomed.id;\n\
        ```\n";

    let rb = parse_markdown_runbook(md, "Safety").expect("valid classification runbook");
    assert_eq!(rb.cells.len(), 3);

    let c0 = rb.find_cell("read-with-literal-directive").expect("c0");
    assert!(!c0.is_write);
    assert!(!c0.is_destructive);
    assert!(!c0.readonly_hint);

    let c1 = rb.find_cell("ddl-step").expect("c1");
    assert!(c1.is_write);
    assert!(!c1.is_destructive);

    let c2 = rb.find_cell("sneaky-readonly-delete").expect("c2");
    assert!(c2.readonly_hint);
    assert!(c2.is_write);
    assert!(c2.is_destructive);
}

#[test]
fn parse_buffer_runbook_with_cell_markers_and_preamble() {
    let sql = "SELECT 0 AS preamble;\n\
        \n\
        -- @cell inspect_strings\n\
        -- @readonly\n\
        SELECT '-- @cell fake_in_string', $$-- @cell fake_in_dollar$$ /* -- @cell fake_in_block */;\n\
        \n\
        -- @cell empty_notes\n\
        -- just a comment\n\
        \n\
        -- @cell mutate\n\
        \n\
        UPDATE accounts SET active = false WHERE id = 1;\n";

    let rb = parse_buffer_runbook(sql, "Scratchpad");
    assert!(!rb.truncated);
    assert_eq!(rb.cells.len(), 3);

    assert_eq!(rb.cells[0].index, 0);
    assert_eq!(rb.cells[0].name, "cell-1");
    assert_eq!(rb.cells[0].start_line, 1);
    assert!(!rb.cells[0].is_write);

    assert_eq!(rb.cells[1].index, 1);
    assert_eq!(rb.cells[1].name, "inspect_strings");
    assert_eq!(rb.cells[1].start_line, 4);
    assert!(rb.cells[1].readonly_hint);
    assert!(!rb.cells[1].is_write);

    assert_eq!(rb.cells[2].index, 2);
    assert_eq!(rb.cells[2].name, "mutate");
    assert_eq!(rb.cells[2].start_line, 12);
    assert!(rb.cells[2].is_write);
    assert!(!rb.cells[2].is_destructive);
    assert_eq!(
        rb.cells[2].sql,
        "UPDATE accounts SET active = false WHERE id = 1;"
    );
}

#[test]
fn parse_buffer_runbook_without_markers_splits_statements_and_bounds() {
    let sql = "-- @name: check-one\n\
        SELECT 1;\n\
        \n\
        DROP TABLE stale_cache;\n";
    let rb = parse_buffer_runbook(sql, "Buffer");
    assert!(!rb.truncated);
    assert_eq!(rb.cells.len(), 2);

    assert_eq!(rb.cells[0].name, "check-one");
    assert_eq!(rb.cells[0].start_line, 1);
    assert!(!rb.cells[0].is_write);
    assert!(rb.cells[0].sql.ends_with("SELECT 1;"));

    assert_eq!(rb.cells[1].name, "cell-2");
    assert_eq!(rb.cells[1].start_line, 4);
    assert!(rb.cells[1].is_write);
    assert!(rb.cells[1].is_destructive);

    let empty = parse_buffer_runbook("  -- comment only\n", "Empty");
    assert!(empty.is_empty());
    assert!(!empty.truncated);

    // Oversized UTF-8 buffer truncates cleanly at char boundary.
    let mut big = String::from("SELECT 1; ");
    big.push_str(&"\u{00e9}".repeat(MAX_RUNBOOK_BYTES));
    let bounded = parse_buffer_runbook(&big, "Big");
    assert!(bounded.truncated);
    assert!(!bounded.cells.is_empty());

    // > MAX_RUNBOOK_CELLS caps at 200.
    let many: String = (0..(MAX_RUNBOOK_CELLS + 10))
        .map(|i| format!("SELECT {i};\n"))
        .collect();
    let capped = parse_buffer_runbook(&many, "Many");
    assert_eq!(capped.cells.len(), MAX_RUNBOOK_CELLS);
    assert!(capped.truncated);
}

#[test]
fn plan_dry_run_enforces_write_guard_and_selected_cell() {
    let md = "# Check\n\
        ```sql\n\
        -- @name: read-step\n\
        SELECT 1;\n\
        ```\n\
        ```sql\n\
        -- @name: write-step\n\
        -- @readonly\n\
        DELETE FROM temp_jobs;\n\
        ```\n";
    let rb = parse_markdown_runbook(md, "Guard Test").expect("parsed");

    let guarded = plan_dry_run(&rb, false, None);
    assert!(guarded.dry_run);
    assert!(!guarded.allow_writes);
    assert_eq!(guarded.planned_count(), 1);
    assert_eq!(guarded.blocked_count, 1);
    assert_eq!(guarded.failed_count, 0);
    assert_eq!(guarded.skipped_count, 0);
    assert!(guarded.has_failures());
    assert!(guarded.has_blocked_writes());
    assert!(!guarded.has_execution_failures());
    assert_eq!(guarded.cells[0].status, CellStatus::Planned);
    assert_eq!(guarded.cells[1].status, CellStatus::BlockedByWriteGuard);
    assert!(
        guarded.cells[1]
            .message
            .as_deref()
            .is_some_and(|m| m.contains("--allow-writes"))
    );

    let allowed = plan_dry_run(&rb, true, None);
    assert_eq!(allowed.planned_count(), 2);
    assert_eq!(allowed.blocked_count, 0);
    assert!(!allowed.has_failures());
    assert!(
        allowed.cells[1]
            .message
            .as_deref()
            .is_some_and(|m| m.contains("@readonly"))
    );

    let scoped = plan_dry_run(&rb, false, Some("read-step"));
    assert_eq!(scoped.planned_count(), 1);
    assert_eq!(scoped.skipped_count, 1);
    assert_eq!(scoped.blocked_count, 0);
    assert!(!scoped.has_failures());
    assert_eq!(scoped.cells[0].status, CellStatus::Planned);
    assert_eq!(scoped.cells[1].status, CellStatus::Skipped);
}

#[test]
fn summarize_report_counts_saturated_duration_and_redacts_secrets() {
    let records = vec![
        CellExecutionRecord {
            index: 0,
            name: "step-1".to_owned(),
            heading: Some("Phase 1".to_owned()),
            is_write: false,
            status: CellStatus::Passed,
            rows_returned: Some(12),
            rows_affected: None,
            duration_ms: u64::MAX - 2,
            message: Some("ok".to_owned()),
        },
        CellExecutionRecord {
            index: 1,
            name: "step-2".to_owned(),
            heading: Some("Phase 2".to_owned()),
            is_write: true,
            status: CellStatus::Failed,
            rows_returned: None,
            rows_affected: Some(0),
            duration_ms: 15,
            message: Some(
                "failed on postgres://admin:not-a-real-password-one@db.internal:5432/app"
                    .to_owned(),
            ),
        },
        CellExecutionRecord {
            index: 2,
            name: "step-3".to_owned(),
            heading: None,
            is_write: true,
            status: CellStatus::BlockedByWriteGuard,
            rows_returned: None,
            rows_affected: None,
            duration_ms: 0,
            message: Some("guarded".to_owned()),
        },
        CellExecutionRecord {
            index: 3,
            name: "step-4".to_owned(),
            heading: None,
            is_write: false,
            status: CellStatus::Skipped,
            rows_returned: None,
            rows_affected: None,
            duration_ms: 0,
            message: None,
        },
    ];

    let report = summarize_report(
        "Deploy postgres://root:hunter2-not-a-real-password@prod/db".to_owned(),
        false,
        true,
        records,
    );
    assert_eq!(report.passed_count, 1);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.blocked_count, 1);
    assert_eq!(report.skipped_count, 1);
    assert_eq!(report.planned_count(), 0);
    assert_eq!(report.total_duration_ms, u64::MAX);
    assert!(report.has_failures());
    assert!(report.has_execution_failures());
    assert!(report.has_blocked_writes());
    assert!(!report.title.contains("hunter2-not-a-real-password"));
    assert!(report.title.contains("[redacted]"));
    assert!(
        report.cells[1]
            .message
            .as_deref()
            .is_some_and(|m| !m.contains("not-a-real-password-one") && m.contains("[redacted]"))
    );
}

#[test]
fn renderers_emit_strictly_plain_ascii_and_valid_json_markdown() {
    let report = summarize_report(
        "Audit \u{2014} \u{1f680} Check".to_owned(),
        false,
        false,
        vec![CellExecutionRecord {
            index: 0,
            name: "cell|pipe".to_owned(),
            heading: Some("H\u{00e9}ader\x1b[31m\u{202e}".to_owned()),
            is_write: false,
            status: CellStatus::Passed,
            rows_returned: Some(3),
            rows_affected: Some(1),
            duration_ms: 42,
            message: Some("ok | PGPASSWORD=not-a-real-password-two".to_owned()),
        }],
    );

    let table = report.render_table();
    assert!(table.is_ascii(), "table must be plain ASCII: {table}");
    assert!(table.contains("Runbook: Audit"));
    assert!(table.contains("3 ret, 1 aff"));
    assert!(!table.contains("not-a-real-password-two"));
    assert!(!table.contains('\x1b'));

    let plain = report.render_plain();
    assert!(plain.is_ascii(), "plain must be plain ASCII: {plain}");
    assert!(plain.contains("cell index=0 name=cell|pipe"));
    assert!(plain.contains("status=passed"));
    assert!(!plain.contains("not-a-real-password-two"));

    let json = report.render_json();
    assert!(json.is_ascii(), "json must be plain ASCII: {json}");
    let roundtrip: RunbookReport = serde_json::from_str(&json).expect("valid JSON roundtrip");
    assert_eq!(roundtrip, report);

    let markdown = report.render_markdown();
    assert!(
        markdown.is_ascii(),
        "markdown must be plain ASCII: {markdown}"
    );
    assert!(markdown.contains("cell\\|pipe"));
    assert!(markdown.contains("\\| [redacted]") || markdown.contains("\\| PGPASSWORD=[redacted]"));
    assert!(!markdown.contains("not-a-real-password-two"));

    let empty = summarize_report("Empty".to_owned(), true, false, Vec::new());
    assert!(empty.render_table().contains("No cells in runbook."));
    assert!(empty.render_markdown().contains("_No cells in runbook._"));
    assert!(empty.render_plain().is_ascii());
    assert!(empty.render_json().is_ascii());
}
