//! Local observations of one read query across schema and plan revisions.

pub(crate) mod store;

use crate::bench::BenchStats;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::plandiff::PlanSnapshot;
use crate::snapshot::sha256_hex;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

pub(crate) const MAX_SERIES: usize = 32;
pub(crate) const MAX_OBSERVATIONS: usize = 64;
pub(crate) const MAX_SQL_BYTES: usize = 64 * 1024;
const P95_REGRESSION_PCT: f64 = 20.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Measurement {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub explain_ms: f64,
    pub shared_read_blocks: u64,
    pub temp_written_blocks: u64,
    pub wal_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub unix_seconds: i64,
    pub server_version_num: i32,
    pub schema_sha256: String,
    pub plan_sha256: String,
    pub settings_sha256: String,
    pub total_cost: f64,
    pub measurement: Option<Measurement>,
}

impl Observation {
    pub(crate) fn from_capture(
        server_version_num: i32,
        schema_sha256: String,
        plan: &PlanSnapshot,
        bench: Option<&BenchStats>,
        unix_seconds: i64,
    ) -> Result<Self, Diagnostic> {
        let shape: Vec<_> = plan
            .nodes
            .iter()
            .map(|node| {
                (
                    &node.path,
                    &node.node_type,
                    &node.relation_name,
                    &node.index_name,
                )
            })
            .collect();
        let measurement = if let Some(bench) = bench {
            let explain_ms = plan.execution_time_ms.ok_or_else(|| {
                Diagnostic::new(
                    DiagnosticKind::Query,
                    "PostgreSQL did not report an execution time",
                    "capturing measured plan history",
                )
            })?;
            Some(Measurement {
                p50_ms: bench.p50_ms,
                p95_ms: bench.p95_ms,
                explain_ms,
                shared_read_blocks: plan.shared_read_blocks,
                temp_written_blocks: plan.temp_written_blocks,
                wal_bytes: plan.wal_bytes,
            })
        } else {
            None
        };
        Ok(Self {
            unix_seconds,
            server_version_num,
            schema_sha256,
            plan_sha256: digest(&shape)?,
            settings_sha256: digest(&plan.settings)?,
            total_cost: plan.total_cost,
            measurement,
        })
    }

    fn valid(&self) -> bool {
        self.server_version_num > 0
            && DateTime::<Utc>::from_timestamp(self.unix_seconds, 0).is_some()
            && [
                &self.schema_sha256,
                &self.plan_sha256,
                &self.settings_sha256,
            ]
            .into_iter()
            .all(|value| valid_digest(value))
            && valid_metric(self.total_cost)
            && self.measurement.as_ref().is_none_or(|value| {
                [value.p50_ms, value.p95_ms, value.explain_ms]
                    .into_iter()
                    .all(valid_metric)
                    && value.p95_ms >= value.p50_ms
            })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Series {
    pub binding: String,
    pub forgotten_observations: u64,
    pub observations: Vec<Observation>,
}

impl Series {
    fn valid(&self) -> bool {
        valid_digest(&self.binding)
            && (1..=MAX_OBSERVATIONS).contains(&self.observations.len())
            && self.observations.iter().all(Observation::valid)
    }
}

#[derive(Debug, Default, Serialize)]
struct Changes {
    schema_changed: Option<bool>,
    plan_changed: Option<bool>,
    settings_changed: Option<bool>,
    comparable: bool,
    cost_delta_pct: Option<f64>,
    p95_delta_pct: Option<f64>,
    p95_regression: Option<bool>,
}

impl Changes {
    fn between(previous: Option<&Observation>, current: &Observation) -> Self {
        let Some(previous) = previous else {
            return Self::default();
        };
        let comparable = previous.server_version_num == current.server_version_num
            && previous.measurement.is_some() == current.measurement.is_some();
        let p95_delta_pct = previous
            .measurement
            .as_ref()
            .zip(current.measurement.as_ref())
            .filter(|_| comparable)
            .and_then(|(prev, now)| delta_pct(prev.p95_ms, now.p95_ms));
        Self {
            schema_changed: Some(previous.schema_sha256 != current.schema_sha256),
            plan_changed: Some(previous.plan_sha256 != current.plan_sha256),
            settings_changed: Some(previous.settings_sha256 != current.settings_sha256),
            comparable,
            cost_delta_pct: comparable
                .then(|| delta_pct(previous.total_cost, current.total_cost))
                .flatten(),
            p95_delta_pct,
            p95_regression: p95_delta_pct.map(|value| value > P95_REGRESSION_PCT),
        }
    }
}

#[derive(Debug, Serialize)]
struct TimelineRow<'a> {
    #[serde(flatten)]
    observation: &'a Observation,
    changes: Changes,
}

#[derive(Debug, Serialize)]
pub(crate) struct Timeline<'a> {
    name: &'a str,
    forgotten_observations: u64,
    observations: Vec<TimelineRow<'a>>,
}

impl<'a> Timeline<'a> {
    pub(crate) fn new(name: &'a str, series: Option<&'a Series>) -> Self {
        let observations = series.map_or(&[][..], |value| value.observations.as_slice());
        let baselines = std::iter::once(None).chain(observations.iter().map(Some));
        Self {
            name,
            forgotten_observations: series.map_or(0, |value| value.forgotten_observations),
            observations: baselines
                .zip(observations)
                .map(|(previous, observation)| TimelineRow {
                    observation,
                    changes: Changes::between(previous, observation),
                })
                .collect(),
        }
    }

    pub(crate) fn render_text(&self) -> String {
        let mut output = format!(
            "Plan history {} ({} retained, {} aged out)\n",
            self.name,
            self.observations.len(),
            self.forgotten_observations
        );
        if self.observations.is_empty() {
            output.push_str("No observations. Use plan-history capture to add one.\n");
        }
        for row in &self.observations {
            let entry = row.observation;
            let change = &row.changes;
            let time = DateTime::<Utc>::from_timestamp(entry.unix_seconds, 0)
                .map_or_else(|| "unknown time".to_owned(), |date| date.to_rfc3339());
            let _ = writeln!(
                output,
                "{time}  PostgreSQL {}.{}  {}  cost {:.2} ({})",
                entry.server_version_num / 10_000,
                entry.server_version_num % 10_000,
                if entry.measurement.is_some() {
                    "measured"
                } else {
                    "estimate"
                },
                entry.total_cost,
                format_delta(change.cost_delta_pct),
            );
            let _ = writeln!(
                output,
                "  Schema {}  shape {}  settings {}",
                format_change(change.schema_changed, &entry.schema_sha256),
                format_change(change.plan_changed, &entry.plan_sha256),
                format_change(change.settings_changed, &entry.settings_sha256),
            );
            if let Some(measurement) = &entry.measurement {
                let _ = writeln!(
                    output,
                    "  p50 {:.3} ms  p95 {:.3} ms ({}){}  EXPLAIN {:.3} ms\n  shared reads {}  temp blocks written {}  WAL {} B",
                    measurement.p50_ms,
                    measurement.p95_ms,
                    format_delta(change.p95_delta_pct),
                    if change.p95_regression == Some(true) {
                        " [regression >20%]"
                    } else {
                        ""
                    },
                    measurement.explain_ms,
                    measurement.shared_read_blocks,
                    measurement.temp_written_blocks,
                    measurement.wal_bytes,
                );
            }
            if change.schema_changed.is_some() && !change.comparable {
                output.push_str(
                    "  Deltas unavailable across server version or capture mode changes.\n",
                );
            }
        }
        output
    }
}

pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String, Diagnostic> {
    let bytes = serde_json::to_vec(value).map_err(|_| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            "could not encode plan history identity",
            "hashing the capture identity",
        )
    })?;
    Ok(sha256_hex(&bytes))
}

pub(crate) fn valid_series_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}

fn valid_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_metric(metric: f64) -> bool {
    metric.is_finite() && metric >= 0.0
}

fn delta_pct(previous: f64, current: f64) -> Option<f64> {
    let delta = (current / previous - 1.0) * 100.0;
    delta.is_finite().then_some(delta)
}

fn format_delta(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:+.1}%"))
}

fn format_change(changed: Option<bool>, hash: &str) -> String {
    let state = match changed {
        None => "first",
        Some(false) => "same",
        Some(true) => "changed",
    };
    format!("{state} {}", &hash[..12])
}

#[cfg(test)]
mod tests;
