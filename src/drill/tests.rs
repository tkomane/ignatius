//! Sidecar tests for the pure restore-drill model.

use super::*;

fn temp_path(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "ignatius-drill-unit-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

const LISTING: &str = "\
;
; Archive created at 2026-11-12 19:00:00 UTC
;     dbname: orders
;     TOC Entries: 7
;     Compression: none
;     Dump Version: 1.16-0
;     Format: CUSTOM
;
;
; Selected TOC Entries:
;
6; 2615 2200 SCHEMA - public postgres
215; 1259 16400 TABLE public orders postgres
216; 1259 16401 TABLE public customers postgres
217; 1259 16402 SEQUENCE public orders_id_seq postgres
218; 2606 16405 CONSTRAINT public orders_pkey postgres
219; 1259 16410 INDEX public orders_created_idx postgres
220; 0 0 TABLE DATA public orders postgres
221; 0 0 SEQUENCE SET public orders_id_seq postgres
222; 2606 16420 FK CONSTRAINT public orders_customer_fk postgres
";

#[test]
fn archive_format_is_detected_and_plain_sql_is_refused() {
    let directory = temp_path("dir");
    std::fs::create_dir_all(&directory).expect("create temp directory");
    assert_eq!(
        ArchiveFormat::detect(&directory),
        Ok(ArchiveFormat::Directory)
    );
    std::fs::remove_dir_all(&directory).expect("remove temp directory");

    let tar = temp_path("archive.tar");
    std::fs::write(&tar, b"not a real archive").expect("write tar placeholder");
    assert_eq!(ArchiveFormat::detect(&tar), Ok(ArchiveFormat::Tar));

    let custom = temp_path("archive.dump");
    std::fs::write(&custom, b"not a real archive").expect("write dump placeholder");
    assert_eq!(ArchiveFormat::detect(&custom), Ok(ArchiveFormat::Custom));

    let plain = temp_path("archive.sql");
    std::fs::write(&plain, b"SELECT 1;").expect("write sql placeholder");
    assert_eq!(ArchiveFormat::detect(&plain), Err(DrillError::PlainSql));

    let missing = temp_path("missing.dump");
    assert!(matches!(
        ArchiveFormat::detect(&missing),
        Err(DrillError::ArchiveMissing(_))
    ));

    let _ = std::fs::remove_file(&tar);
    let _ = std::fs::remove_file(&custom);
    let _ = std::fs::remove_file(&plain);
}

#[test]
fn the_default_scratch_name_is_source_plus_a_utc_timestamp() {
    assert_eq!(
        scratch_name("orders", "20261112T190000Z"),
        "orders_ignatius_drill_20261112T190000Z"
    );
    assert_ne!(
        scratch_name("orders", "20261112T190000Z"),
        scratch_name("orders", "20261112T190001Z")
    );
}

#[test]
fn long_scratch_names_stay_within_the_identifier_limit_and_remain_stable() {
    let source = "a".repeat(80);
    let first = scratch_name(&source, "20261112T190000Z");
    let second = scratch_name(&source, "20261112T190000Z");
    assert!(first.len() <= 63, "{} bytes", first.len());
    assert_eq!(first, second);
    assert!(first.contains(DRILL_SUFFIX));
}

#[test]
fn scratch_validation_refuses_the_source_system_and_empty_names() {
    assert_eq!(validate_scratch_name("orders", "orders_drill"), Ok(()));
    assert_eq!(
        validate_scratch_name("orders", "orders"),
        Err(DrillError::ScratchIsSource)
    );
    assert_eq!(
        validate_scratch_name("orders", "postgres"),
        Err(DrillError::ScratchIsSystem("postgres".to_owned()))
    );
    assert_eq!(
        validate_scratch_name("orders", "TEMPLATE0"),
        Err(DrillError::ScratchIsSystem("TEMPLATE0".to_owned()))
    );
    assert!(matches!(
        validate_scratch_name("orders", ""),
        Err(DrillError::InvalidScratchName(_))
    ));
    assert!(matches!(
        validate_scratch_name("orders", &"x".repeat(64)),
        Err(DrillError::InvalidScratchName(_))
    ));
}

#[test]
fn the_table_of_contents_is_counted_and_classified() {
    let summary = parse_toc(LISTING).expect("parse listing");
    assert_eq!(summary.entries, 9);
    let by_kind = |kind: &str| {
        summary
            .object_types
            .iter()
            .find(|entry| entry.kind == kind)
            .map(|entry| entry.count)
    };
    assert_eq!(by_kind("SCHEMA"), Some(1));
    assert_eq!(by_kind("TABLE"), Some(2));
    assert_eq!(by_kind("SEQUENCE"), Some(1));
    assert_eq!(by_kind("CONSTRAINT"), Some(1));
    assert_eq!(by_kind("INDEX"), Some(1));
    assert_eq!(by_kind("TABLE DATA"), Some(1));
    assert_eq!(by_kind("SEQUENCE SET"), Some(1));
    assert_eq!(by_kind("FK CONSTRAINT"), Some(1));
    assert!(summary.summary().starts_with("9 entries ("));
}

#[test]
fn an_empty_or_malformed_listing_is_refused() {
    assert_eq!(
        parse_toc(";\n; nothing here\n;\n"),
        Err(DrillError::EmptyArchive)
    );
    assert!(matches!(
        parse_toc("not a toc line\n"),
        Err(DrillError::CorruptToc(_))
    ));
    assert!(matches!(
        parse_toc("12; 2615\n"),
        Err(DrillError::CorruptToc(_))
    ));
}

#[test]
fn an_oversized_listing_is_refused() {
    let text = (0..MAX_TOC_LINES + 1)
        .map(|index| format!("{index}; 1259 {index} TABLE public t{index} postgres"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(parse_toc(&text), Err(DrillError::TocTooLarge(_))));
}

#[test]
fn the_plan_lists_inspect_create_restore_verify_and_drop_in_order() {
    let request = DrillRequest {
        source_database: "orders".to_owned(),
        scratch_database: "orders_ignatius_drill_20261112T190000Z".to_owned(),
        archive_path: "/backups/orders.dump".to_owned(),
        format: ArchiveFormat::Custom,
        keep: false,
        drop_on_failure: false,
    };
    let summary = parse_toc(LISTING).expect("parse listing");
    let steps = planned_steps(&request, &summary);
    let actions: Vec<&str> = steps.iter().map(|step| step.action).collect();
    assert_eq!(
        actions,
        vec!["inspect", "create", "restore", "verify", "drop"]
    );
    assert!(steps[1].detail.contains("TEMPLATE template0"));
    assert!(steps[2].detail.contains("--exit-on-error"));
    assert!(steps[2].detail.contains("PGPASSWORD=[redacted]"));
    assert!(!steps.iter().any(|step| step.detail.contains("hunter2")));
}

#[test]
fn retention_policy_keeps_failures_and_honours_keep() {
    assert!(should_drop(DrillOutcome::Passed, false, false));
    assert!(!should_drop(DrillOutcome::Passed, true, true));
    assert!(!should_drop(DrillOutcome::Failed, false, false));
    assert!(should_drop(DrillOutcome::Failed, false, true));
    assert!(!should_drop(DrillOutcome::Failed, true, true));
}

#[test]
fn a_passing_report_renders_table_and_json_without_secrets() {
    let report = DrillReport {
        source_database: "orders".to_owned(),
        scratch_database: "orders_ignatius_drill_20261112T190000Z".to_owned(),
        archive: "/backups/orders.dump".to_owned(),
        archive_format: "custom".to_owned(),
        toc_entries: 214,
        outcome: DrillOutcome::Passed,
        duration_ms: 9123,
        source_relations: Some(38),
        restored_relations: Some(38),
        restored_bytes: Some(12 * 1024 * 1024),
        scratch_kept: false,
        failure: None,
    };
    let table = report.render_table();
    assert!(table.contains("PASSED in 9123 ms"));
    assert!(table.contains("38 source, 38 restored"));
    assert!(table.contains("12.0 MiB"));
    assert!(table.contains("dropped"));
    assert!(!table.contains("hunter2"));

    let json: serde_json::Value = serde_json::from_str(&report.render_json()).expect("valid JSON");
    assert_eq!(json["outcome"], "passed");
    assert_eq!(json["toc_entries"], 214);
    assert_eq!(json["source_database"], "orders");
    assert_eq!(json["restored_relations"], 38);
    assert!(!report.render_json().contains("password"));
}

#[test]
fn a_failed_report_records_a_bounded_reason_and_keeps_the_scratch() {
    let reason = format!("{}\n{}", "x".repeat(3000), "second line");
    let report = DrillReport {
        source_database: "orders".to_owned(),
        scratch_database: "orders_drill".to_owned(),
        archive: "/backups/orders.dump".to_owned(),
        archive_format: "custom".to_owned(),
        toc_entries: 214,
        outcome: DrillOutcome::Failed,
        duration_ms: 50,
        source_relations: Some(38),
        restored_relations: None,
        restored_bytes: None,
        scratch_kept: true,
        failure: Some(bounded(&reason, MAX_FAILURE_CHARS)),
    };
    let table = report.render_table();
    assert!(table.contains("FAILED"));
    assert!(table.contains("kept for inspection"));
    assert!(!table.contains("second line"));
    let failure = report.failure.as_deref().expect("failure");
    assert!(failure.chars().count() <= MAX_FAILURE_CHARS);
}

#[test]
fn byte_formatting_uses_binary_units() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1024), "1.0 KiB");
    assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
}
