//! Sidecar tests for the pure growth audit model.

use super::*;

fn relation(name: &str, total: u64) -> RelationGrowth {
    RelationGrowth {
        schema: "public".to_owned(),
        table: name.to_owned(),
        partitioned: false,
        total_bytes: total,
        heap_bytes: total / 2,
        index_bytes: total / 4,
        toast_bytes: total - (total / 2) - (total / 4),
        estimated_rows: Some(1000),
        live_tuples: 1000,
        dead_tuples: 0,
        inserts: 0,
        updates: 0,
        deletes: 0,
        stats_age_hours: Some(48.0),
        last_vacuum: None,
        last_analyze: None,
    }
}

fn sequence(name: &str, last: Option<i64>, max: i64, increment: i64) -> SequenceHeadroom {
    SequenceHeadroom {
        schema: "public".to_owned(),
        name: name.to_owned(),
        data_type: "bigint".to_owned(),
        last_value: last,
        min_value: 1,
        max_value: max,
        increment,
        cyclic: false,
    }
}

#[test]
fn dead_ratio_uses_live_and_dead_tuples_and_handles_an_empty_table() {
    let mut entry = relation("orders", 100);
    entry.live_tuples = 80;
    entry.dead_tuples = 20;
    assert_eq!(entry.dead_ratio(), Some(0.2));

    entry.live_tuples = 0;
    entry.dead_tuples = 0;
    assert_eq!(entry.dead_ratio(), None);

    entry.live_tuples = 0;
    entry.dead_tuples = 10;
    assert_eq!(entry.dead_ratio(), Some(1.0));
}

#[test]
fn rows_per_day_is_net_growth_over_the_statistics_window() {
    let mut entry = relation("orders", 100);
    entry.inserts = 1000;
    entry.deletes = 400;
    entry.stats_age_hours = Some(48.0);
    assert_eq!(entry.rows_per_day(), Some(300.0));
}

#[test]
fn a_short_or_missing_window_does_not_produce_a_rate() {
    let mut entry = relation("orders", 100);
    entry.inserts = 1000;
    entry.deletes = 0;
    entry.stats_age_hours = Some(0.5);
    assert_eq!(entry.rows_per_day(), None);
    entry.stats_age_hours = None;
    assert_eq!(entry.rows_per_day(), None);
}

#[test]
fn sequence_headroom_counts_remaining_increments() {
    let seq = sequence("orders_id_seq", Some(9500), 10000, 1);
    assert_eq!(seq.remaining(), Some(500));
    assert!(seq.is_critical(), "5 percent remaining is critical");
    assert_eq!(seq.state_label(), "CRITICAL");

    let seq = sequence("orders_id_seq", Some(5000), 10000, 1);
    assert_eq!(seq.remaining(), Some(5000));
    assert!(!seq.is_warning());
    assert_eq!(seq.state_label(), "OK");
}

#[test]
fn a_thousand_values_left_is_critical_even_when_the_fraction_is_wider() {
    let seq = sequence("wide_seq", Some(1_000_000_000), 1_000_001_000, 1);
    assert_eq!(seq.remaining(), Some(1000));
    assert!(seq.is_critical());
}

#[test]
fn twenty_percent_remaining_is_a_warning_but_not_critical() {
    let seq = sequence("orders_id_seq", Some(8001), 10000, 1);
    assert!(seq.is_warning());
    assert!(!seq.is_critical());
    assert_eq!(seq.state_label(), "WARNING");
}

#[test]
fn a_descending_sequence_measures_remaining_values_towards_its_minimum() {
    let mut seq = sequence("countdown", Some(250), 1000, -1);
    seq.min_value = 1;
    assert_eq!(seq.remaining(), Some(249));
    let used = seq.used_fraction().expect("fraction");
    assert!(
        (used - 0.75).abs() < 0.01,
        "unexpected used fraction: {used}"
    );
}

#[test]
fn cyclic_and_unreadable_sequences_are_never_exhausted() {
    let mut cyclic = sequence("cycle_seq", Some(99), 100, 1);
    cyclic.cyclic = true;
    assert_eq!(cyclic.remaining(), None);
    assert!(!cyclic.is_critical());
    assert_eq!(cyclic.state_label(), "CYCLIC");

    let unreadable = sequence("locked_seq", None, 100, 1);
    assert!(!unreadable.is_critical());
    assert_eq!(unreadable.state_label(), "UNAVAILABLE");
    assert_eq!(unreadable.used_fraction(), None);
}

#[test]
fn vacuum_debt_requires_both_the_ratio_and_the_absolute_floor() {
    let options = GrowthOptions::default();
    let mut below_floor = relation("orders", 100);
    below_floor.live_tuples = 100;
    below_floor.dead_tuples = 9999;
    let report = GrowthSnapshot {
        relations: vec![below_floor],
        ..GrowthSnapshot::empty()
    }
    .evaluate(&options);
    assert!(report.findings.is_empty());

    let mut entry = relation("orders", 100);
    entry.live_tuples = 40_000;
    entry.dead_tuples = 10_000;
    let report = GrowthSnapshot {
        relations: vec![entry],
        ..GrowthSnapshot::empty()
    }
    .evaluate(&options);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, "vacuum-debt");
    assert_eq!(report.findings[0].severity, "WARNING");
}

#[test]
fn a_large_unpartitioned_relation_is_a_partitioning_candidate() {
    let mut entry = relation("events", 2 * 1024 * 1024);
    entry.inserts = 1000;
    entry.stats_age_hours = Some(24.0);
    let options = GrowthOptions {
        large_table_bytes: 1024 * 1024,
        ..GrowthOptions::default()
    };
    let report = GrowthSnapshot {
        relations: vec![entry],
        ..GrowthSnapshot::empty()
    }
    .evaluate(&options);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, "partitioning-candidate");
    assert!(report.findings[0].message.contains("1000 rows/day"));
}

#[test]
fn a_partitioned_parent_is_not_a_candidate_and_findings_sort_by_severity() {
    let mut parent = relation("events", 10 * 1024 * 1024);
    parent.partitioned = true;
    let mut child = relation("events_2026_11", 10 * 1024 * 1024);
    child.dead_tuples = 50;
    child.live_tuples = 50;
    let seq = sequence("events_id_seq", Some(99_999), 100_000, 1);
    let options = GrowthOptions {
        large_table_bytes: 1,
        ..GrowthOptions::default()
    };
    let report = GrowthSnapshot {
        relations: vec![parent, child],
        sequences: vec![seq],
        ..GrowthSnapshot::empty()
    }
    .evaluate(&options);
    assert_eq!(report.findings[0].code, "sequence-exhaustion");
    assert_eq!(report.findings[0].severity, "CRITICAL");
    assert!(report.findings.iter().all(|f| f.object != "public.events"));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.object == "public.events_2026_11")
    );
}

#[test]
fn relations_and_sequences_are_ordered_deterministically() {
    let small = relation("a_small", 10);
    let large = relation("z_large", 1000);
    let healthy = sequence("a_healthy", Some(10), 1000, 1);
    let nearly_full = sequence("z_full", Some(995), 1000, 1);
    let report = GrowthSnapshot {
        relations: vec![small, large],
        sequences: vec![healthy, nearly_full],
        ..GrowthSnapshot::empty()
    }
    .evaluate(&GrowthOptions::default());
    assert_eq!(report.relations[0].table, "z_large");
    assert_eq!(report.sequences[0].name, "z_full");
}

#[test]
fn an_empty_snapshot_renders_without_failing() {
    let options = GrowthOptions::default();
    let report = GrowthSnapshot::empty().evaluate(&options);
    let table = report.render_table(&options);
    assert!(table.contains("No user relations found."));
    assert!(table.contains("No user sequences found."));
    assert!(table.contains("No retention or growth findings."));
    assert!(!table.contains("NaN"));
    let json = report.render_json(&options);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["relations"].as_array().map(Vec::len), Some(0));
    assert_eq!(parsed["findings"].as_array().map(Vec::len), Some(0));
}

#[test]
fn json_reports_the_window_estimates_and_computed_rates() {
    let mut entry = relation("orders", 4096);
    entry.dead_tuples = 25;
    entry.live_tuples = 75;
    entry.inserts = 240;
    entry.deletes = 40;
    entry.stats_age_hours = Some(24.0);
    let seq = sequence("orders_id_seq", Some(850_000), 1_000_000, 1);
    let options = GrowthOptions {
        top: 250,
        ..GrowthOptions::default()
    }
    .bounded();
    let report = GrowthSnapshot {
        relations: vec![entry],
        sequences: vec![seq],
        stats_reset: Some("2026-11-11T00:00:00Z".to_owned()),
        stats_age_hours: Some(24.0),
        ..GrowthSnapshot::empty()
    }
    .evaluate(&options);
    let json = report.render_json(&options);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["stats_reset"], "2026-11-11T00:00:00Z");
    assert_eq!(parsed["top"], 200);
    assert_eq!(parsed["relations"][0]["dead_ratio"], 0.25);
    assert_eq!(parsed["relations"][0]["rows_per_day"], 200.0);
    assert_eq!(parsed["sequences"][0]["state"], "WARNING");
    assert!(!json.contains("NaN"));
    assert!(!json.contains("inf"));
}

#[test]
fn fail_on_thresholds_select_the_expected_findings() {
    let mut entry = relation("orders", 100);
    entry.live_tuples = 10_000;
    entry.dead_tuples = 10_000;
    let report = GrowthSnapshot {
        relations: vec![entry],
        ..GrowthSnapshot::empty()
    }
    .evaluate(&GrowthOptions::default());
    assert!(report.has_at_or_above(GrowthSeverity::Warning));
    assert!(!report.has_at_or_above(GrowthSeverity::Critical));
    assert_eq!(report.count_at(GrowthSeverity::Warning), 1);
    assert_eq!(GrowthFailOn::Never.threshold(), None);
    assert_eq!(
        GrowthFailOn::Critical.threshold(),
        Some(GrowthSeverity::Critical)
    );
}

#[test]
fn byte_formatting_uses_binary_units() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1023), "1023 B");
    assert_eq!(format_bytes(1024), "1.0 KiB");
    assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
    assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
}

#[test]
fn the_retention_bounds_are_enforced() {
    let options = GrowthOptions {
        top: 0,
        large_table_bytes: 0,
        ..GrowthOptions::default()
    }
    .bounded();
    assert_eq!(options.top, 1);
    assert_eq!(options.large_table_bytes, 1);
    let options = GrowthOptions {
        top: 10_000,
        ..GrowthOptions::default()
    }
    .bounded();
    assert_eq!(options.top, MAX_RELATIONS);
}
