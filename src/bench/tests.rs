//! Sidecar unit tests for the pure query benchmark statistics, sparklines,
//! and A/B regression comparison engine.

use super::*;

fn samples_from_ms(durations: &[f64]) -> Vec<BenchSample> {
    durations
        .iter()
        .enumerate()
        .map(|(i, &duration_ms)| BenchSample {
            iteration: i + 1,
            duration_ms,
            rows_returned: 3,
        })
        .collect()
}

#[test]
fn bench_config_defaults_and_clamping_enforce_bounds() {
    let def = BenchConfig::default();
    assert_eq!(def.warmup_runs, DEFAULT_WARMUP_RUNS);
    assert_eq!(def.measured_runs, DEFAULT_MEASURED_RUNS);
    assert!(def.rollback);

    let high = BenchConfig::new(99, 5000, false);
    assert_eq!(high.warmup_runs, MAX_WARMUP_RUNS);
    assert_eq!(high.measured_runs, MAX_MEASURED_RUNS);
    assert!(!high.rollback);

    let low = BenchConfig {
        warmup_runs: 0,
        measured_runs: 0,
        rollback: true,
    }
    .clamped();
    assert_eq!(low.warmup_runs, 0);
    assert_eq!(low.measured_runs, 1);
    assert!(low.rollback);

    let built = BenchConfig::default()
        .with_warmup_runs(50)
        .with_measured_runs(0)
        .with_rollback(false);
    assert_eq!(built.warmup_runs, MAX_WARMUP_RUNS);
    assert_eq!(built.measured_runs, 1);
    assert!(!built.rollback);
}

#[test]
fn compute_bench_stats_calculates_welford_stddev_linear_percentiles_and_qps() {
    let samples = samples_from_ms(&[10.0, 20.0, 30.0, 40.0, 50.0]);
    let cfg = BenchConfig::new(2, 5, true);
    let stats = compute_bench_stats("range_scan", "SELECT * FROM orders;", &cfg, &samples);

    assert_eq!(stats.label, "range_scan");
    assert_eq!(stats.sql_preview, "SELECT * FROM orders;");
    assert_eq!(stats.warmup_runs, 2);
    assert_eq!(stats.measured_runs, 5);
    assert!(stats.rollback);
    assert_eq!(stats.total_rows, 15);
    assert!((stats.min_ms - 10.0).abs() < 1e-9);
    assert!((stats.max_ms - 50.0).abs() < 1e-9);
    assert!((stats.mean_ms - 30.0).abs() < 1e-9);

    // Population variance = ((20^2 + 10^2 + 0 + 10^2 + 20^2) / 5) = 200.0
    let expected_stddev = 200.0_f64.sqrt();
    assert!(
        (stats.stddev_ms - expected_stddev).abs() < 1e-9,
        "stddev={} expected={expected_stddev}",
        stats.stddev_ms
    );

    // Linear interpolation across 5 elements (indices 0..=4):
    // p50 -> pos = 2.0 -> 30.0
    // p95 -> pos = 3.8 -> 40.0 + 0.8 * 10.0 = 48.0
    // p99 -> pos = 3.96 -> 40.0 + 0.96 * 10.0 = 49.6
    assert!((stats.p50_ms - 30.0).abs() < 1e-9);
    assert!((stats.p95_ms - 48.0).abs() < 1e-9);
    assert!((stats.p99_ms - 49.6).abs() < 1e-9);

    // QPS = 1000.0 / 30.0
    assert!((stats.qps - (1000.0 / 30.0)).abs() < 1e-9);
}

#[test]
fn compute_bench_stats_handles_single_and_empty_samples_cleanly() {
    let cfg = BenchConfig::default();

    let empty = compute_bench_stats("", "", &cfg, &[]);
    assert_eq!(empty.label, "benchmark");
    assert_eq!(empty.measured_runs, 0);
    assert_eq!(empty.mean_ms, 0.0);
    assert_eq!(empty.qps, 0.0);
    assert_eq!(empty.sparkline_ascii, " ".repeat(BENCH_SPARKLINE_BUCKETS));
    assert_eq!(empty.sparkline_unicode, " ".repeat(BENCH_SPARKLINE_BUCKETS));

    let single_samples = [BenchSample {
        iteration: 1,
        duration_ms: 4.0,
        rows_returned: 7,
    }];
    let single = compute_bench_stats("", "SELECT 42", &cfg, &single_samples);
    assert_eq!(single.label, "SELECT 42");
    assert_eq!(single.measured_runs, 1);
    assert!((single.min_ms - 4.0).abs() < 1e-9);
    assert!((single.mean_ms - 4.0).abs() < 1e-9);
    assert!((single.stddev_ms - 0.0).abs() < 1e-9);
    assert!((single.p50_ms - 4.0).abs() < 1e-9);
    assert!((single.p95_ms - 4.0).abs() < 1e-9);
    assert!((single.p99_ms - 4.0).abs() < 1e-9);
    assert!((single.max_ms - 4.0).abs() < 1e-9);
    assert!((single.qps - 250.0).abs() < 1e-9);
    assert_eq!(single.sparkline_ascii, "=".repeat(BENCH_SPARKLINE_BUCKETS));
    assert_eq!(
        single.sparkline_unicode,
        "\u{2584}".repeat(BENCH_SPARKLINE_BUCKETS)
    );
}

#[test]
fn sparklines_produce_12_buckets_across_iteration_latencies() {
    let cfg = BenchConfig::new(0, 12, true);
    let exact_12 = samples_from_ms(&[
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
    ]);
    let stats_12 = compute_bench_stats("ramp", "SELECT 1", &cfg, &exact_12);
    assert_eq!(stats_12.sparkline_ascii.len(), BENCH_SPARKLINE_BUCKETS);
    assert!(stats_12.sparkline_ascii.is_ascii());
    assert!(
        stats_12
            .sparkline_ascii
            .chars()
            .all(|c| " .:-=+*#%".contains(c))
    );
    assert_eq!(stats_12.sparkline_ascii.chars().next(), Some('.'));
    assert_eq!(stats_12.sparkline_ascii.chars().last(), Some('%'));
    assert_eq!(
        stats_12.sparkline_unicode.chars().count(),
        BENCH_SPARKLINE_BUCKETS
    );
    assert!(stats_12.sparkline_unicode.chars().all(|c| {
        " \u{2581}\u{2582}\u{2583}\u{2584}\u{2585}\u{2586}\u{2587}\u{2588}".contains(c)
    }));
    assert_eq!(stats_12.sparkline_unicode.chars().next(), Some('\u{2581}'));
    assert_eq!(stats_12.sparkline_unicode.chars().last(), Some('\u{2588}'));

    // 36 samples bucketed into 12 time windows with a clear middle spike.
    let mut spike = vec![2.0; 36];
    spike[15] = 50.0;
    spike[16] = 50.0;
    spike[17] = 50.0;
    let stats_36 = compute_bench_stats(
        "spike",
        "SELECT 1",
        &BenchConfig::new(0, 36, true),
        &samples_from_ms(&spike),
    );
    assert_eq!(stats_36.sparkline_ascii.len(), BENCH_SPARKLINE_BUCKETS);
    let chars: Vec<char> = stats_36.sparkline_ascii.chars().collect();
    assert_eq!(chars[0], '.');
    assert_eq!(chars[5], '%');
    assert_eq!(chars[11], '.');
}

#[test]
fn compare_benchmarks_evaluates_faster_slower_unchanged_and_threshold_gate() {
    let cfg = BenchConfig::default();
    let base = compute_bench_stats(
        "baseline",
        "SELECT * FROM orders",
        &cfg,
        &samples_from_ms(&[10.0, 10.0, 10.0, 10.0, 10.0]),
    );

    // 1. Single-query report has no comparison and never fails gate.
    let single = compare_benchmarks(base.clone(), None, Some(10.0));
    assert!(single.candidate.is_none());
    assert!(single.comparison.is_none());
    assert!(!single.failed_gate);

    // 2. Faster candidate (2.0x speedup: 10ms -> 5ms).
    let faster_cand = compute_bench_stats(
        "indexed",
        "SELECT * FROM orders WHERE id = 1",
        &cfg,
        &samples_from_ms(&[5.0, 5.0, 5.0, 5.0, 5.0]),
    );
    let faster_report = compare_benchmarks(base.clone(), Some(faster_cand), Some(10.0));
    let cmp_fast = faster_report.comparison.as_ref().expect("comparison");
    assert_eq!(cmp_fast.verdict, BenchVerdict::Faster);
    assert!(!cmp_fast.regression_detected);
    assert!(!faster_report.failed_gate);
    assert!((cmp_fast.speedup_ratio - 2.0).abs() < 1e-9);
    assert!((cmp_fast.p50_delta_ms - (-5.0)).abs() < 1e-9);
    assert!((cmp_fast.p50_delta_pct - (-50.0)).abs() < 1e-9);

    // 3. Slower candidate within 20% gate (+10%) -> Slower verdict, gate passes.
    let slightly_slower = compute_bench_stats(
        "slight",
        "SELECT * FROM orders",
        &cfg,
        &samples_from_ms(&[11.0, 11.0, 11.0, 11.0, 11.0]),
    );
    let pass_report = compare_benchmarks(base.clone(), Some(slightly_slower), Some(20.0));
    let cmp_pass = pass_report.comparison.as_ref().expect("comparison");
    assert_eq!(cmp_pass.verdict, BenchVerdict::Slower);
    assert!(!cmp_pass.regression_detected);
    assert!(!pass_report.failed_gate);

    // 4. Slower candidate exceeding 10% gate (+50%) -> regression_detected + failed_gate.
    let regressed = compute_bench_stats(
        "seq_scan",
        "SELECT * FROM orders",
        &cfg,
        &samples_from_ms(&[15.0, 15.0, 15.0, 15.0, 15.0]),
    );
    let fail_report = compare_benchmarks(base.clone(), Some(regressed), Some(10.0));
    let cmp_fail = fail_report.comparison.as_ref().expect("comparison");
    assert_eq!(cmp_fail.verdict, BenchVerdict::Slower);
    assert!(cmp_fail.regression_detected);
    assert!(fail_report.failed_gate);
    assert!((cmp_fail.p50_delta_ms - 5.0).abs() < 1e-9);
    assert!((cmp_fail.p50_delta_pct - 50.0).abs() < 1e-9);

    // 5. Tail p95 regression alone breaches gate even when p50 is flat.
    let tail_spike = compute_bench_stats(
        "tail_spike",
        "SELECT * FROM orders",
        &cfg,
        &samples_from_ms(&[10.0, 10.0, 10.0, 10.0, 20.0]),
    );
    let tail_report = compare_benchmarks(base.clone(), Some(tail_spike), Some(15.0));
    let cmp_tail = tail_report.comparison.as_ref().expect("comparison");
    assert_eq!(cmp_tail.verdict, BenchVerdict::Slower);
    assert!(cmp_tail.regression_detected);
    assert!(tail_report.failed_gate);

    // 6. Unchanged within noise band.
    let steady = compute_bench_stats(
        "steady",
        "SELECT * FROM orders",
        &cfg,
        &samples_from_ms(&[10.05, 10.05, 10.05, 10.05, 10.05]),
    );
    let unchanged_report = compare_benchmarks(base, Some(steady), Some(10.0));
    let cmp_same = unchanged_report.comparison.as_ref().expect("comparison");
    assert_eq!(cmp_same.verdict, BenchVerdict::Unchanged);
    assert!(!cmp_same.regression_detected);
    assert!(!unchanged_report.failed_gate);
}

#[test]
fn redacts_credentials_and_renders_strictly_plain_ascii_and_roundtrip_json() {
    let cfg = BenchConfig::new(1, 3, true);
    let base = compute_bench_stats(
        "conn postgres://bench_user:topsecret99@db.internal:5432/prod \u{2014} alpha",
        "SELECT dblink_connect('host=db password=supersecret_pw')",
        &cfg,
        &samples_from_ms(&[12.0, 14.0, 13.0]),
    );
    assert!(!base.label.contains("topsecret99"));
    assert!(!base.sql_preview.contains("supersecret_pw"));
    assert!(base.label.contains("[redacted]"));
    assert!(base.sql_preview.contains("[redacted]"));
    assert!(base.label.is_ascii());
    assert!(base.sql_preview.is_ascii());

    let cand = compute_bench_stats("cand", "SELECT 1", &cfg, &samples_from_ms(&[6.0, 7.0, 6.5]));
    let report = compare_benchmarks(base, Some(cand), Some(10.0));

    let table = report.render_table();
    let plain = report.render_plain();
    let json = report.render_json();

    assert!(table.is_ascii(), "table must be strictly plain ASCII");
    assert!(plain.is_ascii(), "plain must be strictly plain ASCII");
    assert!(json.is_ascii(), "json must be strictly plain ASCII");
    assert!(!table.contains("topsecret99"));
    assert!(!plain.contains("supersecret_pw"));
    assert!(table.contains("QUERY BENCHMARK COMPARISON"));
    assert!(table.contains("Gate: PASS (threshold=10.0%)"));
    assert!(plain.contains("mode=compare"));
    assert!(plain.contains("verdict=Faster"));
    assert!(plain.contains("failed_gate=false"));

    let decoded: BenchReport = serde_json::from_str(&json).expect("valid JSON roundtrip");
    assert_eq!(decoded, report);

    let single = BenchReport::single(report.baseline.clone());
    let single_table = single.render_table();
    let single_plain = single.render_plain();
    assert!(single_table.is_ascii());
    assert!(single_plain.is_ascii());
    assert!(single_table.contains("QUERY BENCHMARK REPORT"));
    assert!(single_plain.contains("mode=single"));
}

#[test]
fn handles_non_finite_durations_zero_baselines_long_labels_and_un_gated_regressions() {
    let cfg = BenchConfig::default();
    let long_label = "x".repeat(MAX_BENCH_LABEL_CHARS + 30);
    let long_sql = format!("SELECT {}", "a, ".repeat(60));
    let dirty_samples = [
        BenchSample {
            iteration: 1,
            duration_ms: -2.5,
            rows_returned: 1,
        },
        BenchSample {
            iteration: 2,
            duration_ms: f64::NAN,
            rows_returned: 1,
        },
        BenchSample {
            iteration: 3,
            duration_ms: f64::INFINITY,
            rows_returned: 1,
        },
    ];
    let zero_base = compute_bench_stats(&long_label, &long_sql, &cfg, &dirty_samples);
    assert_eq!(zero_base.label.chars().count(), MAX_BENCH_LABEL_CHARS);
    assert!(zero_base.label.ends_with("..."));
    assert_eq!(zero_base.sql_preview.chars().count(), MAX_SQL_PREVIEW_CHARS);
    assert!(zero_base.sql_preview.ends_with("..."));
    assert_eq!(zero_base.min_ms, 0.0);
    assert_eq!(zero_base.mean_ms, 0.0);
    assert_eq!(zero_base.qps, 0.0);

    let positive_cand = compute_bench_stats("pos", "SELECT 1", &cfg, &samples_from_ms(&[5.0, 5.0]));
    let zero_cmp = compare_benchmarks(zero_base, Some(positive_cand), Some(f64::NAN));
    let cmp = zero_cmp.comparison.as_ref().expect("comparison");
    assert!(cmp.speedup_ratio.is_finite());
    assert_eq!(cmp.p50_delta_pct, 0.0);
    assert!(zero_cmp.threshold_pct.is_none());
    assert!(!zero_cmp.failed_gate);
    assert!(serde_json::from_str::<BenchReport>(&zero_cmp.render_json()).is_ok());

    // Without threshold_pct, a slower candidate flags regression_detected=true
    // and verdict=Slower while keeping failed_gate=false.
    let slow_base = compute_bench_stats(
        "base",
        "SELECT 1",
        &cfg,
        &samples_from_ms(&[10.0; DEFAULT_MEASURED_RUNS]),
    );
    assert_eq!(slow_base.sparkline_ascii.len(), BENCH_SPARKLINE_BUCKETS);
    let slow_cand = compute_bench_stats(
        "cand",
        "SELECT 2",
        &cfg,
        &samples_from_ms(&[25.0; DEFAULT_MEASURED_RUNS]),
    );
    let ungated = compare_benchmarks(slow_base, Some(slow_cand), None);
    let ungated_cmp = ungated.comparison.as_ref().expect("comparison");
    assert_eq!(ungated_cmp.verdict, BenchVerdict::Slower);
    assert_eq!(ungated_cmp.verdict.label(), "Slower");
    assert!(ungated_cmp.regression_detected);
    assert!(!ungated.failed_gate);
    assert!(ungated.render_table().contains("Gate: NONE"));
}
