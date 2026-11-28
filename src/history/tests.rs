use super::*;
use std::time::Duration;

#[test]
fn pruning_preserves_an_earlier_temporary_and_keeps_the_latest_entries() {
    let dir = tempfile::tempdir().expect("temporary root");
    let history = history(&dir, 1);
    history.record(&entry("SELECT 1")).expect("first entry");
    let stale = history.path().with_extension("jsonl.new");
    std::fs::write(&stale, "earlier state").expect("stale temporary");
    history.record(&entry("SELECT 2")).expect("second entry");
    assert_eq!(std::fs::read_to_string(stale).unwrap(), "earlier state");
    assert_eq!(history.entries().unwrap()[0].sql, "SELECT 2");
}

#[cfg(unix)]
#[test]
fn pruning_never_follows_a_stale_temporary_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().expect("temporary root");
    let history = history(&dir, 1);
    history.record(&entry("SELECT 1")).expect("first entry");
    let outside = dir.path().join("outside");
    std::fs::write(&outside, "untouched").expect("outside file");
    let stale = history.path().with_extension("jsonl.new");
    symlink(&outside, &stale).expect("stale link");
    history.record(&entry("SELECT 2")).expect("second entry");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "untouched");
    assert!(std::fs::symlink_metadata(stale).unwrap().is_symlink());
    assert_eq!(history.entries().unwrap()[0].sql, "SELECT 2");
}

fn entry(sql: &str) -> Entry {
    Entry::now(
        "app@localhost:5432/orders",
        "orders",
        "local",
        sql,
        Outcome::Succeeded,
        Duration::from_millis(12),
    )
}

fn history(dir: &tempfile::TempDir, max: usize) -> History {
    History::at(&dir.path().join("history.jsonl"), max)
}

#[test]
fn a_statement_is_written_and_read_back_exactly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);
    assert_eq!(
        history.record(&entry("SELECT 1;")).expect("record"),
        Recorded::Written
    );
    history.record(&entry("SELECT 2;")).expect("record");

    let entries = history.entries().expect("read");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].sql, "SELECT 1;", "oldest first");
    assert_eq!(entries[1].sql, "SELECT 2;");
    assert_eq!(entries[0].outcome, Outcome::Succeeded);
    assert_eq!(entries[0].database, "orders");

    let recent = history.recent(1).expect("read");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].sql, "SELECT 2;", "newest first");
}

#[test]
fn a_statement_that_mentions_a_credential_never_reaches_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);

    for sql in [
        "ALTER ROLE app PASSWORD 'hunter2'",
        "CREATE USER x WITH PASSWORD 'p'",
        "SELECT * FROM users WHERE api_key = 'k'",
        "update settings set secret = 'shh'",
        "SELECT token FROM sessions",
    ] {
        assert_eq!(
            history.record(&entry(sql)).expect("record"),
            Recorded::LooksLikeACredential,
            "{sql}"
        );
    }

    assert!(
        history.entries().expect("read").is_empty(),
        "nothing was written"
    );
    assert!(
        !history.path().exists(),
        "the file was not even created for a statement that cannot be kept"
    );

    // The check is on substrings, so a column that merely contains one of
    // the words is refused too. That over-refusal is the intended direction:
    // a retype costs a moment, a leaked password costs more.
    for sql in [
        "UPDATE users SET password_hash = 'x'",
        "SELECT access_token FROM sessions",
        "SELECT tokens_used FROM usage",
    ] {
        assert_eq!(
            history.record(&entry(sql)).expect("record"),
            Recorded::LooksLikeACredential,
            "{sql}"
        );
    }

    // An ordinary statement is still recorded.
    assert_eq!(
        history
            .record(&entry("SELECT count(*) FROM orders"))
            .expect("record"),
        Recorded::Written
    );
}

#[test]
fn the_connection_target_is_redacted_before_it_is_written() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);
    let entry = Entry::now(
        "postgres://app:hunter2@db.example.net:5432/orders",
        "orders",
        "production",
        "SELECT 1",
        Outcome::Succeeded,
        Duration::from_millis(1),
    );
    history.record(&entry).expect("record");

    let text = std::fs::read_to_string(history.path()).expect("read");
    assert!(
        !text.contains("hunter2"),
        "a password reached the file: {text}"
    );
    assert!(text.contains("db.example.net"), "{text}");
}

#[test]
fn history_that_is_off_or_paused_writes_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut history = history(&dir, 100);

    history.pause();
    assert!(!history.is_recording());
    assert_eq!(
        history.record(&entry("SELECT 1")).expect("r"),
        Recorded::Paused
    );

    history.resume();
    assert!(history.is_recording());
    history.record(&entry("SELECT 1")).expect("r");

    let off = History {
        enabled: false,
        ..history.clone()
    };
    assert_eq!(
        off.record(&entry("SELECT 2")).expect("r"),
        Recorded::Disabled
    );
    assert!(!off.is_recording());

    assert_eq!(history.entries().expect("read").len(), 1);
}

#[test]
fn the_file_is_kept_to_its_configured_length_newest_first() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 3);
    for index in 0..10 {
        history
            .record(&entry(&format!("SELECT {index}")))
            .expect("r");
    }
    let entries = history.entries().expect("read");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].sql, "SELECT 7");
    assert_eq!(entries[2].sql, "SELECT 9");
    assert!(
        !history.path().with_extension("jsonl.new").exists(),
        "the temporary file is not left behind"
    );
}

#[test]
fn clearing_removes_everything_and_is_safe_to_repeat() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);
    history.record(&entry("SELECT 1")).expect("r");
    history.clear().expect("clear");
    assert!(history.entries().expect("read").is_empty());
    assert!(!history.path().exists());
    history.clear().expect("clearing nothing is not an error");
}

#[test]
fn recording_past_the_configured_max_keeps_only_the_newest() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 3);
    for statement in [
        "SELECT 1;",
        "SELECT 2;",
        "SELECT 3;",
        "SELECT 4;",
        "SELECT 5;",
    ] {
        history.record(&entry(statement)).expect("record");
    }
    let entries = history.entries().expect("read");
    assert_eq!(entries.len(), 3, "the file stays bounded");
    assert_eq!(entries[0].sql, "SELECT 3;", "oldest first of what survived");
    assert_eq!(entries[2].sql, "SELECT 5;");
}

#[test]
fn a_damaged_line_does_not_lose_the_rest_of_the_history() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);
    history.record(&entry("SELECT 1")).expect("r");
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(history.path())
            .expect("open");
        writeln!(file, "{{not json").expect("write");
    }
    history.record(&entry("SELECT 2")).expect("r");

    let entries = history.entries().expect("read");
    assert_eq!(entries.len(), 2, "the readable entries survive");
}

#[test]
fn reading_a_history_that_does_not_exist_yet_is_not_an_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);
    assert!(history.entries().expect("read").is_empty());
    assert!(history.recent(10).expect("read").is_empty());
}

#[cfg(unix)]
#[test]
fn the_file_is_readable_only_by_its_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 2);
    history.record(&entry("SELECT 1")).expect("r");
    let mode = std::fs::metadata(history.path())
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "history is the owner's alone");

    // Pruning rewrites the file, which must not widen it.
    for index in 0..5 {
        history
            .record(&entry(&format!("SELECT {index}")))
            .expect("r");
    }
    let mode = std::fs::metadata(history.path())
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "rewriting kept the permissions");
}

#[test]
fn an_entry_reads_the_way_a_listing_needs_it() {
    let entry = entry("SELECT\n  1,\n  2");
    assert_eq!(entry.one_line(), "SELECT 1, 2");
    assert!(entry.when().len() >= 19, "{}", entry.when());
    assert_eq!(Outcome::Succeeded.label(), "ok");
    assert_eq!(
        Outcome::from_status(&crate::query::ExecutionStatus::ConnectionLost),
        Outcome::Unknown
    );

    // A value crafted to drive the terminal is escaped in a listing.
    let hostile = entry.sql.clone() + "\u{1b}[2J";
    let entry = Entry {
        sql: hostile,
        ..entry
    };
    assert!(!entry.one_line().contains('\u{1b}'));
}

#[test]
fn only_a_reason_the_user_needs_is_said_out_loud() {
    assert!(Recorded::LooksLikeACredential.note().is_some());
    assert!(Recorded::Written.note().is_none());
    assert!(
        Recorded::Paused.note().is_none(),
        "the interface shows a paused session; it need not repeat itself"
    );
    assert!(Recorded::Written.was_written());
    assert!(!Recorded::Disabled.was_written());
}

fn dated_entry(
    at: &str,
    target: &str,
    database: &str,
    environment: &str,
    sql: &str,
    outcome: Outcome,
    elapsed_ms: u64,
) -> Entry {
    Entry {
        at: at.to_owned(),
        target: target.to_owned(),
        database: database.to_owned(),
        environment: environment.to_owned(),
        sql: sql.to_owned(),
        outcome,
        elapsed_ms,
    }
}

#[test]
fn search_filters_newest_first_enforces_cap_and_redacts_defense_in_depth() {
    let dir = tempfile::tempdir().expect("temp dir");
    let history = history(&dir, 100);
    // Write raw JSONL lines directly so we can exercise defense-in-depth read filtering.
    let lines = [
        serde_json::to_string(&dated_entry(
            "2026-10-01T10:00:00Z",
            "postgres://app:hunter2@localhost:5432/orders",
            "orders",
            "local",
            "SELECT * FROM orders;",
            Outcome::Succeeded,
            15,
        ))
        .unwrap(),
        serde_json::to_string(&dated_entry(
            "2026-10-02T12:00:00Z",
            "app@localhost:5432/billing",
            "billing",
            "staging",
            "SELECT * FROM invoices WHERE status = 'late';",
            Outcome::Failed,
            120,
        ))
        .unwrap(),
        serde_json::to_string(&dated_entry(
            "2026-10-03T14:00:00Z",
            "app@localhost:5432/billing",
            "billing",
            "staging",
            "ALTER ROLE x PASSWORD 'secret';",
            Outcome::Succeeded,
            5,
        ))
        .unwrap(),
        serde_json::to_string(&dated_entry(
            "2026-10-04T16:00:00Z",
            "app@localhost:5432/billing",
            "billing",
            "production",
            "SELECT count(*) FROM invoices;",
            Outcome::Succeeded,
            40,
        ))
        .unwrap(),
    ];
    std::fs::write(history.path(), lines.join("\n") + "\n").expect("write history");

    // Credential-mentioning entry written externally is omitted on read.
    let all = history.search(&HistoryFilter::default()).expect("search");
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].sql, "SELECT count(*) FROM invoices;");
    assert!(
        !all[2].target.contains("hunter2"),
        "target must be re-redacted on read: {}",
        all[2].target
    );

    let filtered = history
        .search(&HistoryFilter {
            query: Some("invoices".to_owned()),
            database: Some("BILLING".to_owned()),
            environment: Some("staging".to_owned()),
            outcome: Some(Outcome::Failed),
            since: Some(parse_since_timestamp("2026-10-02").expect("since")),
            limit: 10,
        })
        .expect("filtered");
    assert_eq!(filtered.len(), 1);
    assert_eq!(
        filtered[0].sql,
        "SELECT * FROM invoices WHERE status = 'late';"
    );

    let limited = history
        .search(&HistoryFilter {
            limit: 1,
            ..HistoryFilter::default()
        })
        .expect("limited");
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].sql, "SELECT count(*) FROM invoices;");
}

#[test]
fn stats_and_export_formats_handle_latencies_csv_quoting_and_redaction() {
    let entries = vec![
        dated_entry(
            "2026-10-01T10:00:00Z",
            "postgres://app:hunter2@localhost/orders",
            "orders",
            "local",
            "SELECT 1, 'a, \"b\"';",
            Outcome::Succeeded,
            10,
        ),
        dated_entry(
            "2026-10-02T10:00:00Z",
            "app@localhost/orders",
            "orders",
            "local",
            "SELECT * FROM deadlocked;",
            Outcome::Failed,
            50,
        ),
        dated_entry(
            "2026-10-03T10:00:00Z",
            "app@localhost/billing",
            "billing",
            "staging",
            "SELECT pg_sleep(5);",
            Outcome::Cancelled,
            200,
        ),
        dated_entry(
            "2026-10-04T10:00:00Z",
            "app@localhost/billing",
            "billing",
            "staging",
            "SELECT api_key FROM secrets;",
            Outcome::Succeeded,
            9999,
        ),
    ];

    let stats = HistoryStats::compute(&entries);
    assert_eq!(stats.total, 3);
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.cancelled, 1);
    assert_eq!(stats.by_database.get("orders"), Some(&2));
    assert_eq!(stats.by_environment.get("staging"), Some(&1));
    assert_eq!(stats.median_elapsed_ms, Some(50));
    assert_eq!(stats.p95_elapsed_ms, Some(200));
    assert_eq!(stats.max_elapsed_ms, Some(200));

    let csv = String::from_utf8(
        format_history_export(&entries, HistoryExportFormat::Csv).expect("csv export"),
    )
    .expect("utf8");
    assert!(!csv.contains("hunter2"), "{csv}");
    assert!(!csv.contains("api_key"), "{csv}");
    assert!(csv.contains("\"SELECT 1, 'a, \"\"b\"\"';\""), "{csv}");

    let sql = String::from_utf8(
        format_history_export(&entries, HistoryExportFormat::Sql).expect("sql export"),
    )
    .expect("utf8");
    assert!(sql.contains("Ignatius local history export"), "{sql}");
    assert!(sql.contains("SELECT * FROM deadlocked;"), "{sql}");
    assert!(!sql.contains("api_key"), "{sql}");

    let jsonl = String::from_utf8(
        format_history_export(&entries, HistoryExportFormat::Jsonl).expect("jsonl export"),
    )
    .expect("utf8");
    assert_eq!(jsonl.lines().count(), 3);
    assert!(!jsonl.contains("hunter2"), "{jsonl}");
}

#[test]
fn parses_status_export_format_and_since_timestamps() {
    assert_eq!(Outcome::parse("OK").unwrap(), Outcome::Succeeded);
    assert_eq!(Outcome::parse("succeeded").unwrap(), Outcome::Succeeded);
    assert_eq!(Outcome::parse("failed").unwrap(), Outcome::Failed);
    assert_eq!(Outcome::parse("canceled").unwrap(), Outcome::Cancelled);
    assert_eq!(Outcome::parse("unknown").unwrap(), Outcome::Unknown);
    assert!(Outcome::parse("bogus").is_err());

    assert_eq!(
        HistoryExportFormat::parse("csv").unwrap(),
        HistoryExportFormat::Csv
    );
    assert_eq!(
        HistoryExportFormat::parse("jsonl").unwrap(),
        HistoryExportFormat::Jsonl
    );
    assert_eq!(
        HistoryExportFormat::parse("SQL").unwrap(),
        HistoryExportFormat::Sql
    );
    assert!(HistoryExportFormat::parse("parquet").is_err());

    assert!(parse_since_timestamp("2026-11-28").is_ok());
    assert!(parse_since_timestamp("2026-11-28T09:30:00Z").is_ok());
    assert!(parse_since_timestamp("not-a-date").is_err());
}
