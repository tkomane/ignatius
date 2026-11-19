//! Pure query benchmark statistics, 12-bucket latency sparklines, and A/B
//! regression comparison engine for Feature `056-query-benchmarks-and-regression-suite`.
//!
//! This module contains zero network, filesystem, process, or terminal I/O.
//! Callers measure query iterations externally (for example via
//! [`crate::postgres::bench::run_benchmark`]) and feed raw [`BenchSample`]
//! slices into [`compute_bench_stats`] and [`compare_benchmarks`] to obtain
//! bounded statistics and strictly plain-ASCII table, plain-text, and JSON
//! reports.

#[cfg(test)]
mod tests;

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::diagnostics::redaction::redact_text;
use crate::query::value::sanitize_for_display;

/// Default number of unmeasured warmup iterations executed before sampling.
pub const DEFAULT_WARMUP_RUNS: usize = 2;

/// Maximum allowed number of warmup iterations.
pub const MAX_WARMUP_RUNS: usize = 20;

/// Default number of measured benchmark iterations.
pub const DEFAULT_MEASURED_RUNS: usize = 10;

/// Maximum allowed number of measured benchmark iterations.
pub const MAX_MEASURED_RUNS: usize = 200;

/// Fixed bucket count for per-iteration latency sparklines.
pub const BENCH_SPARKLINE_BUCKETS: usize = 12;

/// Maximum character length retained for a normalized benchmark label.
pub const MAX_BENCH_LABEL_CHARS: usize = 120;

/// Maximum character length retained for a normalized SQL preview.
pub const MAX_SQL_PREVIEW_CHARS: usize = 120;

const ASCII_SPARKLINE_RAMP: [char; 9] = [' ', '.', ':', '-', '=', '+', '*', '#', '%'];
const UNICODE_SPARKLINE_RAMP: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
    '\u{2588}',
];

const LATENCY_EPSILON_MS: f64 = 1e-6;
const MATERIAL_CHANGE_PCT: f64 = 1.0;

/// Bounded execution parameters for one query benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchConfig {
    /// Number of unrecorded warmup iterations clamped to `0..=MAX_WARMUP_RUNS`.
    pub warmup_runs: usize,
    /// Number of recorded measured iterations clamped to `1..=MAX_MEASURED_RUNS`.
    pub measured_runs: usize,
    /// When `true`, wraps every iteration in `BEGIN` / unconditional `ROLLBACK`.
    pub rollback: bool,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            warmup_runs: DEFAULT_WARMUP_RUNS,
            measured_runs: DEFAULT_MEASURED_RUNS,
            rollback: true,
        }
    }
}

impl BenchConfig {
    /// Builds a bounded configuration, clamping `warmup_runs` to
    /// `0..=MAX_WARMUP_RUNS` and `measured_runs` to `1..=MAX_MEASURED_RUNS`.
    #[must_use]
    pub const fn new(warmup_runs: usize, measured_runs: usize, rollback: bool) -> Self {
        Self {
            warmup_runs: clamp_warmup(warmup_runs),
            measured_runs: clamp_measured(measured_runs),
            rollback,
        }
    }

    /// Returns a copy of this configuration with iteration bounds enforced.
    #[must_use]
    pub const fn clamped(self) -> Self {
        Self::new(self.warmup_runs, self.measured_runs, self.rollback)
    }

    /// Updates `warmup_runs`, clamping to `0..=MAX_WARMUP_RUNS`.
    #[must_use]
    pub const fn with_warmup_runs(mut self, warmup_runs: usize) -> Self {
        self.warmup_runs = clamp_warmup(warmup_runs);
        self
    }

    /// Updates `measured_runs`, clamping to `1..=MAX_MEASURED_RUNS`.
    #[must_use]
    pub const fn with_measured_runs(mut self, measured_runs: usize) -> Self {
        self.measured_runs = clamp_measured(measured_runs);
        self
    }

    /// Updates rollback-isolation mode.
    #[must_use]
    pub const fn with_rollback(mut self, rollback: bool) -> Self {
        self.rollback = rollback;
        self
    }
}

const fn clamp_warmup(runs: usize) -> usize {
    if runs > MAX_WARMUP_RUNS {
        MAX_WARMUP_RUNS
    } else {
        runs
    }
}

const fn clamp_measured(runs: usize) -> usize {
    if runs < 1 {
        1
    } else if runs > MAX_MEASURED_RUNS {
        MAX_MEASURED_RUNS
    } else {
        runs
    }
}

/// One measured benchmark iteration observation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BenchSample {
    /// One-based iteration index.
    pub iteration: usize,
    /// Elapsed wall-clock execution time in milliseconds.
    pub duration_ms: f64,
    /// Number of rows returned (for queries) or affected (for DML).
    pub rows_returned: u64,
}

/// Aggregated latency, throughput, and sparkline statistics for one SQL query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchStats {
    /// Sanitized, credential-redacted label bounded to [`MAX_BENCH_LABEL_CHARS`].
    pub label: String,
    /// Sanitized, credential-redacted SQL preview bounded to [`MAX_SQL_PREVIEW_CHARS`].
    pub sql_preview: String,
    /// Effective warmup iterations run before measurement.
    pub warmup_runs: usize,
    /// Number of measured samples included in these statistics.
    pub measured_runs: usize,
    /// Minimum observed iteration latency in milliseconds.
    pub min_ms: f64,
    /// Arithmetic mean latency in milliseconds (Welford algorithm).
    pub mean_ms: f64,
    /// Population standard deviation in milliseconds (Welford algorithm).
    pub stddev_ms: f64,
    /// Linear-interpolated 50th percentile (median) latency in milliseconds.
    pub p50_ms: f64,
    /// Linear-interpolated 95th percentile latency in milliseconds.
    pub p95_ms: f64,
    /// Linear-interpolated 99th percentile latency in milliseconds.
    pub p99_ms: f64,
    /// Maximum observed iteration latency in milliseconds.
    pub max_ms: f64,
    /// Mean sustained throughput in queries per second (`1000.0 / mean_ms`).
    pub qps: f64,
    /// Sum of rows returned or affected across all measured samples.
    pub total_rows: u64,
    /// Whether iterations were isolated inside `BEGIN` / `ROLLBACK`.
    pub rollback: bool,
    /// Raw measured samples in iteration order.
    pub samples: Vec<BenchSample>,
    /// Strictly plain-ASCII 12-bucket sparkline across per-iteration latencies.
    pub sparkline_ascii: String,
    /// Opt-in Unicode 12-bucket block sparkline across per-iteration latencies.
    pub sparkline_unicode: String,
}

/// Directional latency outcome comparing candidate against baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchVerdict {
    /// Candidate is materially faster than baseline.
    Faster,
    /// Candidate is materially slower than baseline or breached the threshold gate.
    Slower,
    /// Candidate and baseline latencies are within the materiality tolerance.
    Unchanged,
}

impl BenchVerdict {
    /// Plain ASCII label for tables, scripts, and status badges.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Faster => "Faster",
            Self::Slower => "Slower",
            Self::Unchanged => "Unchanged",
        }
    }
}

/// Quantitative A/B comparison between a baseline and candidate benchmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchComparison {
    /// Relative p50 speedup (`baseline.p50_ms / candidate.p50_ms`; `> 1.0` means faster).
    pub speedup_ratio: f64,
    /// Signed median latency delta (`candidate.p50_ms - baseline.p50_ms`).
    pub p50_delta_ms: f64,
    /// Percentage change in p50 latency relative to baseline.
    pub p50_delta_pct: f64,
    /// Signed 95th-percentile latency delta (`candidate.p95_ms - baseline.p95_ms`).
    pub p95_delta_ms: f64,
    /// Percentage change in p95 latency relative to baseline.
    pub p95_delta_pct: f64,
    /// Whether latency regressed beyond `threshold_pct` (or materially when unset).
    pub regression_detected: bool,
    /// Overall directional verdict.
    pub verdict: BenchVerdict,
}

/// Complete single-query or A/B benchmark report with regression gate status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchReport {
    /// Primary (or baseline) benchmark statistics.
    pub baseline: BenchStats,
    /// Optional candidate benchmark statistics when running an A/B comparison.
    pub candidate: Option<BenchStats>,
    /// Derived comparison deltas and verdict when `candidate` is `Some`.
    pub comparison: Option<BenchComparison>,
    /// Configured regression threshold percentage, when gating is active.
    pub threshold_pct: Option<f64>,
    /// `true` when `threshold_pct` is configured and a regression breached it.
    pub failed_gate: bool,
}

/// Computes bounded summary statistics, linear percentiles, and 12-bucket
/// per-iteration sparklines from raw [`BenchSample`] measurements.
#[must_use]
pub fn compute_bench_stats(
    label: &str,
    sql: &str,
    config: &BenchConfig,
    samples: &[BenchSample],
) -> BenchStats {
    let effective = config.clamped();
    let sql_preview = sanitize_bounded(sql, MAX_SQL_PREVIEW_CHARS, "");
    let fallback_label = if sql_preview.is_empty() {
        "benchmark"
    } else {
        sql_preview.as_str()
    };
    let clean_label = sanitize_bounded(label, MAX_BENCH_LABEL_CHARS, fallback_label);

    let normalized_samples: Vec<BenchSample> = samples
        .iter()
        .map(|s| BenchSample {
            iteration: s.iteration,
            duration_ms: sanitize_duration_ms(s.duration_ms),
            rows_returned: s.rows_returned,
        })
        .collect();

    let durations: Vec<f64> = normalized_samples.iter().map(|s| s.duration_ms).collect();
    let total_rows = normalized_samples
        .iter()
        .map(|s| s.rows_returned)
        .fold(0u64, u64::saturating_add);
    let measured_runs = normalized_samples.len();

    let (min_ms, mean_ms, stddev_ms, p50_ms, p95_ms, p99_ms, max_ms, qps) = if durations.is_empty()
    {
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    } else {
        let mut count = 0usize;
        let mut mean = 0.0f64;
        let mut m2 = 0.0f64;
        for &val in &durations {
            count += 1;
            let delta = val - mean;
            mean += delta / (count as f64);
            let delta2 = val - mean;
            m2 += delta * delta2;
        }
        let mean_val = normalize_zero(mean);
        let variance = (m2 / (count as f64)).max(0.0);
        let stddev_val = normalize_zero(variance.sqrt());

        let mut sorted = durations.clone();
        sorted.sort_by(f64::total_cmp);

        let min_val = normalize_zero(sorted[0]);
        let max_val = normalize_zero(sorted[sorted.len() - 1]);
        let p50_val = normalize_zero(percentile_linear(&sorted, 0.50));
        let p95_val = normalize_zero(percentile_linear(&sorted, 0.95));
        let p99_val = normalize_zero(percentile_linear(&sorted, 0.99));

        let qps_val = if mean_val > 1e-12 {
            let raw = 1000.0 / mean_val;
            if raw.is_finite() { raw } else { 0.0 }
        } else {
            0.0
        };

        (
            min_val, mean_val, stddev_val, p50_val, p95_val, p99_val, max_val, qps_val,
        )
    };

    let (sparkline_ascii, sparkline_unicode) = build_bench_sparklines(&durations);

    BenchStats {
        label: clean_label,
        sql_preview,
        warmup_runs: effective.warmup_runs,
        measured_runs,
        min_ms,
        mean_ms,
        stddev_ms,
        p50_ms,
        p95_ms,
        p99_ms,
        max_ms,
        qps,
        total_rows,
        rollback: effective.rollback,
        samples: normalized_samples,
        sparkline_ascii,
        sparkline_unicode,
    }
}

/// Builds a [`BenchReport`] for either a single benchmark (`candidate == None`)
/// or an A/B regression comparison (`candidate == Some(...)`), evaluating an
/// optional maximum allowed latency regression percentage `threshold_pct`.
#[must_use]
pub fn compare_benchmarks(
    baseline: BenchStats,
    candidate: Option<BenchStats>,
    threshold_pct: Option<f64>,
) -> BenchReport {
    let normalized_threshold = normalize_threshold(threshold_pct);
    let Some(cand) = candidate else {
        return BenchReport {
            baseline,
            candidate: None,
            comparison: None,
            threshold_pct: normalized_threshold,
            failed_gate: false,
        };
    };

    let speedup_ratio = compute_speedup_ratio(&baseline, &cand);
    let p50_delta_ms = normalize_zero(cand.p50_ms - baseline.p50_ms);
    let p50_delta_pct = compute_delta_pct(baseline.p50_ms, p50_delta_ms);
    let p95_delta_ms = normalize_zero(cand.p95_ms - baseline.p95_ms);
    let p95_delta_pct = compute_delta_pct(baseline.p95_ms, p95_delta_ms);

    let regression_detected = match normalized_threshold {
        Some(limit) => {
            (p50_delta_pct > limit && p50_delta_ms > LATENCY_EPSILON_MS)
                || (p95_delta_pct > limit && p95_delta_ms > LATENCY_EPSILON_MS)
        }
        None => {
            (p50_delta_pct > MATERIAL_CHANGE_PCT && p50_delta_ms > LATENCY_EPSILON_MS)
                || (p95_delta_pct > MATERIAL_CHANGE_PCT
                    && p95_delta_ms > LATENCY_EPSILON_MS
                    && p50_delta_pct >= -MATERIAL_CHANGE_PCT)
        }
    };

    let verdict = if regression_detected {
        BenchVerdict::Slower
    } else if (p50_delta_pct < -MATERIAL_CHANGE_PCT && p50_delta_ms < -LATENCY_EPSILON_MS)
        || (p95_delta_pct < -MATERIAL_CHANGE_PCT
            && p95_delta_ms < -LATENCY_EPSILON_MS
            && p50_delta_pct <= MATERIAL_CHANGE_PCT)
    {
        BenchVerdict::Faster
    } else if (p50_delta_pct > MATERIAL_CHANGE_PCT && p50_delta_ms > LATENCY_EPSILON_MS)
        || (p95_delta_pct > MATERIAL_CHANGE_PCT
            && p95_delta_ms > LATENCY_EPSILON_MS
            && p50_delta_pct >= -MATERIAL_CHANGE_PCT)
    {
        BenchVerdict::Slower
    } else {
        BenchVerdict::Unchanged
    };

    let failed_gate = normalized_threshold.is_some() && regression_detected;

    BenchReport {
        baseline,
        candidate: Some(cand),
        comparison: Some(BenchComparison {
            speedup_ratio,
            p50_delta_ms,
            p50_delta_pct,
            p95_delta_ms,
            p95_delta_pct,
            regression_detected,
            verdict,
        }),
        threshold_pct: normalized_threshold,
        failed_gate,
    }
}

impl BenchReport {
    /// Wraps a single benchmark summary into a [`BenchReport`].
    #[must_use]
    pub fn single(baseline: BenchStats) -> Self {
        compare_benchmarks(baseline, None, None)
    }

    /// Renders an aligned, strictly plain-ASCII table of benchmark statistics
    /// and any A/B regression comparison.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        match (&self.candidate, &self.comparison) {
            (Some(cand), Some(cmp)) => {
                let gate_label = match self.threshold_pct {
                    Some(limit) if self.failed_gate => format!("FAIL (threshold={limit:.1}%)"),
                    Some(limit) => format!("PASS (threshold={limit:.1}%)"),
                    None => "NONE".to_owned(),
                };
                out.push_str("QUERY BENCHMARK COMPARISON\n");
                let _ = writeln!(
                    out,
                    "Verdict   : {} | Speedup: {:.2}x | Gate: {gate_label}",
                    cmp.verdict.label(),
                    cmp.speedup_ratio,
                );
                let _ = writeln!(
                    out,
                    "Baseline  : {} ({})",
                    sanitize_ascii(&self.baseline.label),
                    sanitize_ascii(&self.baseline.sql_preview),
                );
                let _ = writeln!(
                    out,
                    "Candidate : {} ({})",
                    sanitize_ascii(&cand.label),
                    sanitize_ascii(&cand.sql_preview),
                );
                let _ = writeln!(
                    out,
                    "Base Runs : warmup={} | measured={} | rollback={} | spark=[{}]",
                    self.baseline.warmup_runs,
                    self.baseline.measured_runs,
                    yes_no(self.baseline.rollback),
                    sanitize_ascii(&self.baseline.sparkline_ascii),
                );
                let _ = writeln!(
                    out,
                    "Cand Runs : warmup={} | measured={} | rollback={} | spark=[{}]",
                    cand.warmup_runs,
                    cand.measured_runs,
                    yes_no(cand.rollback),
                    sanitize_ascii(&cand.sparkline_ascii),
                );
                out.push('\n');

                let _ = writeln!(
                    out,
                    "{:<18}  {:>12}  {:>12}  {:>24}",
                    "METRIC", "BASELINE", "CANDIDATE", "DELTA (B - A)"
                );
                let _ = writeln!(out, "{}", "-".repeat(72));

                write_cmp_row(
                    &mut out,
                    "Min (ms)",
                    &fmt_ms(self.baseline.min_ms),
                    &fmt_ms(cand.min_ms),
                    &fmt_delta_ms(cand.min_ms - self.baseline.min_ms, None),
                );
                write_cmp_row(
                    &mut out,
                    "Mean (ms)",
                    &fmt_ms(self.baseline.mean_ms),
                    &fmt_ms(cand.mean_ms),
                    &fmt_delta_ms(cand.mean_ms - self.baseline.mean_ms, None),
                );
                write_cmp_row(
                    &mut out,
                    "Stddev (ms)",
                    &fmt_ms(self.baseline.stddev_ms),
                    &fmt_ms(cand.stddev_ms),
                    &fmt_delta_ms(cand.stddev_ms - self.baseline.stddev_ms, None),
                );
                write_cmp_row(
                    &mut out,
                    "p50 (ms)",
                    &fmt_ms(self.baseline.p50_ms),
                    &fmt_ms(cand.p50_ms),
                    &fmt_delta_ms(cmp.p50_delta_ms, Some(cmp.p50_delta_pct)),
                );
                write_cmp_row(
                    &mut out,
                    "p95 (ms)",
                    &fmt_ms(self.baseline.p95_ms),
                    &fmt_ms(cand.p95_ms),
                    &fmt_delta_ms(cmp.p95_delta_ms, Some(cmp.p95_delta_pct)),
                );
                write_cmp_row(
                    &mut out,
                    "p99 (ms)",
                    &fmt_ms(self.baseline.p99_ms),
                    &fmt_ms(cand.p99_ms),
                    &fmt_delta_ms(cand.p99_ms - self.baseline.p99_ms, None),
                );
                write_cmp_row(
                    &mut out,
                    "Max (ms)",
                    &fmt_ms(self.baseline.max_ms),
                    &fmt_ms(cand.max_ms),
                    &fmt_delta_ms(cand.max_ms - self.baseline.max_ms, None),
                );
                write_cmp_row(
                    &mut out,
                    "Throughput (qps)",
                    &format!("{:.2}", self.baseline.qps),
                    &format!("{:.2}", cand.qps),
                    &format!("{:+.2}", cand.qps - self.baseline.qps),
                );
                write_cmp_row(
                    &mut out,
                    "Total Rows",
                    &self.baseline.total_rows.to_string(),
                    &cand.total_rows.to_string(),
                    &format!(
                        "{:+}",
                        signed_row_delta(self.baseline.total_rows, cand.total_rows)
                    ),
                );
            }
            _ => {
                out.push_str("QUERY BENCHMARK REPORT\n");
                let _ = writeln!(out, "Baseline  : {}", sanitize_ascii(&self.baseline.label));
                let _ = writeln!(
                    out,
                    "SQL       : {}",
                    sanitize_ascii(&self.baseline.sql_preview)
                );
                let _ = writeln!(
                    out,
                    "Runs      : warmup={} | measured={} | rollback={} | total_rows={}",
                    self.baseline.warmup_runs,
                    self.baseline.measured_runs,
                    yes_no(self.baseline.rollback),
                    self.baseline.total_rows,
                );
                let _ = writeln!(
                    out,
                    "Sparkline : [{}] ({BENCH_SPARKLINE_BUCKETS} buckets)",
                    sanitize_ascii(&self.baseline.sparkline_ascii),
                );
                out.push('\n');

                let _ = writeln!(out, "{:<20}  {:>14}", "METRIC", "VALUE");
                let _ = writeln!(out, "{}", "-".repeat(36));
                write_single_row(&mut out, "Min (ms)", &fmt_ms(self.baseline.min_ms));
                write_single_row(&mut out, "Mean (ms)", &fmt_ms(self.baseline.mean_ms));
                write_single_row(&mut out, "Stddev (ms)", &fmt_ms(self.baseline.stddev_ms));
                write_single_row(&mut out, "p50 (ms)", &fmt_ms(self.baseline.p50_ms));
                write_single_row(&mut out, "p95 (ms)", &fmt_ms(self.baseline.p95_ms));
                write_single_row(&mut out, "p99 (ms)", &fmt_ms(self.baseline.p99_ms));
                write_single_row(&mut out, "Max (ms)", &fmt_ms(self.baseline.max_ms));
                write_single_row(
                    &mut out,
                    "Throughput (qps)",
                    &format!("{:.2}", self.baseline.qps),
                );
                write_single_row(
                    &mut out,
                    "Total Rows",
                    &self.baseline.total_rows.to_string(),
                );
            }
        }
        out
    }

    /// Renders a strictly plain-ASCII line-oriented summary suitable for scripts.
    #[must_use]
    pub fn render_plain(&self) -> String {
        let mut out = String::new();
        let mode = if self.candidate.is_some() {
            "compare"
        } else {
            "single"
        };
        let _ = writeln!(out, "mode={mode}");
        write_plain_stats(&mut out, "baseline", &self.baseline);
        if let Some(cand) = &self.candidate {
            write_plain_stats(&mut out, "candidate", cand);
        }
        if let Some(cmp) = &self.comparison {
            let _ = writeln!(
                out,
                "comparison verdict={} speedup_ratio={:.4} p50_delta_ms={:+.3} p50_delta_pct={:+.2} p95_delta_ms={:+.3} p95_delta_pct={:+.2} regression_detected={}",
                cmp.verdict.label(),
                cmp.speedup_ratio,
                cmp.p50_delta_ms,
                cmp.p50_delta_pct,
                cmp.p95_delta_ms,
                cmp.p95_delta_pct,
                cmp.regression_detected,
            );
        }
        let threshold_str = self
            .threshold_pct
            .map_or_else(|| "none".to_owned(), |t| format!("{t:.2}"));
        let _ = writeln!(
            out,
            "gate threshold_pct={threshold_str} failed_gate={}",
            self.failed_gate
        );
        out
    }

    /// Renders a strictly plain-ASCII pretty JSON representation of the report.
    #[must_use]
    pub fn render_json(&self) -> String {
        let raw = serde_json::to_string_pretty(self).unwrap_or_else(|_| {
            "{\"error\":\"benchmark report could not be serialized\"}".to_owned()
        });
        escape_json_to_ascii(&raw)
    }
}

fn write_plain_stats(out: &mut String, role: &str, stats: &BenchStats) {
    let _ = writeln!(
        out,
        "{role} label={:?} warmup={} measured={} rollback={} min_ms={:.3} mean_ms={:.3} stddev_ms={:.3} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3} qps={:.2} total_rows={} sparkline={:?}",
        sanitize_ascii(&stats.label),
        stats.warmup_runs,
        stats.measured_runs,
        stats.rollback,
        stats.min_ms,
        stats.mean_ms,
        stats.stddev_ms,
        stats.p50_ms,
        stats.p95_ms,
        stats.p99_ms,
        stats.max_ms,
        stats.qps,
        stats.total_rows,
        sanitize_ascii(&stats.sparkline_ascii),
    );
}

fn build_bench_sparklines(durations: &[f64]) -> (String, String) {
    if durations.is_empty() {
        let spaces = " ".repeat(BENCH_SPARKLINE_BUCKETS);
        return (spaces.clone(), spaces);
    }

    let mut buckets = [0.0f64; BENCH_SPARKLINE_BUCKETS];
    let n = durations.len();
    if n <= BENCH_SPARKLINE_BUCKETS {
        for (b, slot) in buckets.iter_mut().enumerate() {
            let idx = (b * n) / BENCH_SPARKLINE_BUCKETS;
            *slot = durations[idx.min(n - 1)];
        }
    } else {
        for (b, slot) in buckets.iter_mut().enumerate() {
            let start = (b * n) / BENCH_SPARKLINE_BUCKETS;
            let end = ((b + 1) * n) / BENCH_SPARKLINE_BUCKETS;
            let slice = &durations[start..end.max(start + 1)];
            let sum: f64 = slice.iter().sum();
            *slot = sum / (slice.len() as f64);
        }
    }

    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for &val in &buckets {
        if val < min_v {
            min_v = val;
        }
        if val > max_v {
            max_v = val;
        }
    }
    let span = max_v - min_v;

    let mut ascii = String::with_capacity(BENCH_SPARKLINE_BUCKETS);
    let mut unicode = String::with_capacity(BENCH_SPARKLINE_BUCKETS * 3);

    for &val in &buckets {
        let ramp_idx = if !span.is_finite() || span <= 1e-12 {
            4
        } else {
            let norm = ((val - min_v) / span).clamp(0.0, 1.0);
            1 + ((norm * 7.0).round() as usize).min(7)
        };
        ascii.push(ASCII_SPARKLINE_RAMP[ramp_idx]);
        unicode.push(UNICODE_SPARKLINE_RAMP[ramp_idx]);
    }

    (ascii, unicode)
}

fn percentile_linear(sorted: &[f64], quantile: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = quantile.clamp(0.0, 1.0) * ((sorted.len() - 1) as f64);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - (lo as f64);
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

fn compute_speedup_ratio(baseline: &BenchStats, candidate: &BenchStats) -> f64 {
    if baseline.p50_ms > 1e-12 && candidate.p50_ms > 1e-12 {
        let ratio = baseline.p50_ms / candidate.p50_ms;
        if ratio.is_finite() {
            return ratio;
        }
    }
    if baseline.mean_ms > 1e-12 && candidate.mean_ms > 1e-12 {
        let ratio = baseline.mean_ms / candidate.mean_ms;
        if ratio.is_finite() {
            return ratio;
        }
    }
    1.0
}

fn compute_delta_pct(baseline: f64, delta: f64) -> f64 {
    if !baseline.is_finite() || !delta.is_finite() || baseline <= LATENCY_EPSILON_MS {
        0.0
    } else {
        normalize_zero((delta / baseline) * 100.0)
    }
}

fn normalize_threshold(threshold_pct: Option<f64>) -> Option<f64> {
    threshold_pct
        .filter(|v| v.is_finite())
        .map(|v| normalize_zero(v.max(0.0)))
}

fn sanitize_duration_ms(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        normalize_zero(value)
    } else {
        0.0
    }
}

fn normalize_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn sanitize_bounded(input: &str, max_chars: usize, fallback: &str) -> String {
    let redacted = redact_text(input);
    let collapsed = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = sanitize_ascii(&collapsed);
    if clean.is_empty() {
        return truncate_ascii(fallback, max_chars);
    }
    truncate_ascii(&clean, max_chars)
}

fn sanitize_ascii(input: &str) -> String {
    let safe = sanitize_for_display(input);
    let mut out = String::with_capacity(safe.len());
    for ch in safe.chars() {
        if ch.is_ascii() && !ch.is_ascii_control() {
            out.push(ch);
        } else {
            let _ = write!(out, "\\u{{{:04X}}}", ch as u32);
        }
    }
    out
}

fn truncate_ascii(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_owned();
    }
    if max_len <= 3 {
        return input.chars().take(max_len).collect();
    }
    let mut out: String = input.chars().take(max_len - 3).collect();
    out.push_str("...");
    out
}

fn escape_json_to_ascii(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let code = ch as u32;
            if code <= 0xFFFF {
                let _ = write!(out, "\\u{code:04x}");
            } else {
                let adjusted = code - 0x1_0000;
                let high = 0xD800 + (adjusted >> 10);
                let low = 0xDC00 + (adjusted & 0x3FF);
                let _ = write!(out, "\\u{high:04x}\\u{low:04x}");
            }
        }
    }
    out
}

fn write_single_row(out: &mut String, metric: &str, value: &str) {
    let _ = writeln!(out, "{:<20}  {:>14}", truncate_ascii(metric, 20), value);
}

fn write_cmp_row(out: &mut String, metric: &str, a: &str, b: &str, delta: &str) {
    let _ = writeln!(
        out,
        "{:<18}  {:>12}  {:>12}  {:>24}",
        truncate_ascii(metric, 18),
        a,
        b,
        delta,
    );
}

fn fmt_ms(ms: f64) -> String {
    format!("{:.3}", normalize_zero(ms))
}

fn fmt_delta_ms(delta: f64, pct: Option<f64>) -> String {
    let d = normalize_zero(delta);
    match pct {
        Some(p) => format!("{d:+.3} ({:+.1}%)", normalize_zero(p)),
        None => format!("{d:+.3}"),
    }
}

fn signed_row_delta(baseline: u64, candidate: u64) -> i64 {
    let a = i64::try_from(baseline).unwrap_or(i64::MAX);
    let b = i64::try_from(candidate).unwrap_or(i64::MAX);
    b.saturating_sub(a)
}

const fn yes_no(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}
