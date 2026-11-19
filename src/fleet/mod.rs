//! Compare bounded PostgreSQL fleet observations without database I/O.

use crate::snapshot::{SchemaSnapshot, diff_snapshots};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub(crate) const MAX_TARGETS: usize = 4;
pub(crate) const MAX_EXTENSIONS: usize = 256;
pub(crate) const SETTINGS: [&str; 8] = [
    "effective_cache_size",
    "jit",
    "max_connections",
    "max_parallel_workers_per_gather",
    "random_page_cost",
    "seq_page_cost",
    "shared_buffers",
    "work_mem",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Issue {
    Configuration,
    Authentication,
    Connection,
    Tls,
    Timeout,
    Catalog,
    PermissionDenied,
    SchemaMissing,
    ReadOnlyUnconfirmed,
    UnsupportedVersion,
    Bounded,
    InvalidCatalogValue,
}

impl Issue {
    pub(crate) fn from_diagnostic(diagnostic: &crate::diagnostics::Diagnostic) -> Self {
        use crate::diagnostics::DiagnosticKind as Kind;
        match diagnostic.kind {
            Kind::Usage | Kind::Config => Self::Configuration,
            Kind::Authentication => Self::Authentication,
            Kind::Connection => Self::Connection,
            Kind::Tls => Self::Tls,
            Kind::Cancelled => Self::Timeout,
            _ if diagnostic
                .technical
                .iter()
                .any(|field| field.label == "SQLSTATE" && field.value == "42501") =>
            {
                Self::PermissionDenied
            }
            _ => Self::Catalog,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Authentication => "authentication",
            Self::Connection => "connection",
            Self::Tls => "tls",
            Self::Timeout => "timeout",
            Self::Catalog => "catalog",
            Self::PermissionDenied => "permission-denied",
            Self::SchemaMissing => "schema-missing",
            Self::ReadOnlyUnconfirmed => "read-only-unconfirmed",
            Self::UnsupportedVersion => "unsupported-version",
            Self::Bounded => "bounded",
            Self::InvalidCatalogValue => "invalid-catalog-value",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TlsObservation {
    Disabled,
    NotNegotiated,
    Active,
    Unknown,
}

impl TlsObservation {
    const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NotNegotiated => "not-negotiated",
            Self::Active => "active",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Health {
    pub(crate) server_version_num: i32,
    pub(crate) in_recovery: bool,
    pub(crate) read_only: bool,
    pub(crate) tls: TlsObservation,
    pub(crate) sslmode: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Setting {
    pub(crate) value: String,
    pub(crate) unit: Option<String>,
}

pub(crate) struct Observation {
    pub(crate) health: Health,
    pub(crate) schema: Result<SchemaSnapshot, Issue>,
    pub(crate) extensions: Result<BTreeMap<String, String>, Issue>,
    pub(crate) settings: Result<BTreeMap<String, Setting>, Issue>,
}

pub(crate) struct TargetProbe {
    pub(crate) profile: String,
    pub(crate) environment: &'static str,
    pub(crate) result: Result<Observation, Issue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum TargetStatus {
    Complete,
    Partial,
    Unavailable,
}

impl TargetStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Serialize)]
struct Target {
    profile: String,
    environment: &'static str,
    status: TargetStatus,
    health: Option<Health>,
    issue: Option<Issue>,
}

impl From<&TargetProbe> for Target {
    fn from(probe: &TargetProbe) -> Self {
        let (status, health, issue) = match &probe.result {
            Err(reason) => (TargetStatus::Unavailable, None, Some(*reason)),
            Ok(observation) => {
                let complete = observation.schema.as_ref().is_ok_and(|s| !s.truncated)
                    && observation.extensions.is_ok()
                    && observation.settings.is_ok()
                    && observation.health.tls != TlsObservation::Unknown;
                let status = if complete {
                    TargetStatus::Complete
                } else {
                    TargetStatus::Partial
                };
                (status, Some(observation.health), None)
            }
        };
        Self {
            profile: probe.profile.clone(),
            environment: probe.environment,
            status,
            health,
            issue,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ComparisonStatus {
    Match,
    Drift,
    Unavailable,
}

impl ComparisonStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Drift => "drift",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Serialize)]
struct Dimension<T> {
    status: ComparisonStatus,
    detail: Option<T>,
    baseline_issue: Option<Issue>,
    candidate_issue: Option<Issue>,
}

impl<T> Dimension<T> {
    fn compare<B>(
        baseline: Result<&B, Issue>,
        candidate: Result<&B, Issue>,
        compare: impl FnOnce(&B, &B) -> (bool, T),
    ) -> Self {
        match (baseline, candidate) {
            (Ok(baseline), Ok(candidate)) => {
                let (matches, detail) = compare(baseline, candidate);
                let status = if matches {
                    ComparisonStatus::Match
                } else {
                    ComparisonStatus::Drift
                };
                Self {
                    status,
                    detail: Some(detail),
                    baseline_issue: None,
                    candidate_issue: None,
                }
            }
            (baseline, candidate) => Self {
                status: ComparisonStatus::Unavailable,
                detail: None,
                baseline_issue: baseline.err(),
                candidate_issue: candidate.err(),
            },
        }
    }

    fn render_issues(&self, output: &mut String) {
        if let Some(issue) = self.baseline_issue {
            let _ = write!(output, " baseline_issue={}", issue.label());
        }
        if let Some(issue) = self.candidate_issue {
            let _ = write!(output, " candidate_issue={}", issue.label());
        }
    }
}

#[derive(Serialize)]
struct SchemaDifference {
    baseline_fingerprint: String,
    candidate_fingerprint: String,
    breaking: usize,
    warning: usize,
    info: usize,
}

#[derive(Serialize)]
struct Change<T> {
    name: String,
    baseline: Option<T>,
    candidate: Option<T>,
}

fn map_changes<T: Clone + PartialEq>(
    baseline: &BTreeMap<String, T>,
    candidate: &BTreeMap<String, T>,
) -> (bool, Vec<Change<T>>) {
    let keys: BTreeSet<&String> = baseline.keys().chain(candidate.keys()).collect();
    let changes: Vec<_> = keys
        .into_iter()
        .filter_map(|name| {
            let before = baseline.get(name);
            let after = candidate.get(name);
            (before != after).then(|| Change {
                name: name.clone(),
                baseline: before.cloned(),
                candidate: after.cloned(),
            })
        })
        .collect();
    (changes.is_empty(), changes)
}

fn compared_schema(
    baseline: &SchemaSnapshot,
    candidate: &SchemaSnapshot,
) -> (bool, SchemaDifference) {
    let (breaking, warning, info, matches) =
        if baseline.fingerprint_sha256 == candidate.fingerprint_sha256 {
            (0, 0, 0, true)
        } else {
            let diff = diff_snapshots(baseline, candidate);
            (
                diff.breaking_count,
                diff.warning_count,
                diff.info_count,
                false,
            )
        };
    (
        matches,
        SchemaDifference {
            baseline_fingerprint: baseline.fingerprint_sha256.clone(),
            candidate_fingerprint: candidate.fingerprint_sha256.clone(),
            breaking,
            warning,
            info,
        },
    )
}

fn observation_section<T>(
    probe: &TargetProbe,
    field: impl FnOnce(&Observation) -> &Result<T, Issue>,
) -> Result<&T, Issue> {
    let observation = probe.result.as_ref().map_err(|issue| *issue)?;
    field(observation).as_ref().map_err(|issue| *issue)
}

fn schema_section(probe: &TargetProbe) -> Result<&SchemaSnapshot, Issue> {
    let schema = observation_section(probe, |observation| &observation.schema)?;
    if schema.truncated {
        return Err(Issue::Bounded);
    }
    Ok(schema)
}

#[derive(Serialize)]
struct Comparison {
    candidate: String,
    schema: Dimension<SchemaDifference>,
    extensions: Dimension<Vec<Change<String>>>,
    settings: Dimension<Vec<Change<Setting>>>,
}

impl Comparison {
    fn new(baseline: &TargetProbe, candidate: &TargetProbe) -> Self {
        Self {
            candidate: candidate.profile.clone(),
            schema: Dimension::compare(
                schema_section(baseline),
                schema_section(candidate),
                compared_schema,
            ),
            extensions: Dimension::compare(
                observation_section(baseline, |o| &o.extensions),
                observation_section(candidate, |o| &o.extensions),
                map_changes,
            ),
            settings: Dimension::compare(
                observation_section(baseline, |o| &o.settings),
                observation_section(candidate, |o| &o.settings),
                map_changes,
            ),
        }
    }

    fn has_drift(&self) -> bool {
        [
            self.schema.status,
            self.extensions.status,
            self.settings.status,
        ]
        .contains(&ComparisonStatus::Drift)
    }

    fn render(&self, output: &mut String) {
        let _ = write!(
            output,
            "candidate={} schema={}",
            self.candidate,
            self.schema.status.label()
        );
        self.schema.render_issues(output);
        if let Some(diff) = &self.schema.detail {
            let _ = write!(
                output,
                " baseline={} candidate_fingerprint={} breaking={} warning={} info={}",
                diff.baseline_fingerprint,
                diff.candidate_fingerprint,
                diff.breaking,
                diff.warning,
                diff.info,
            );
        }
        output.push('\n');

        let _ = write!(
            output,
            "candidate={} extensions={}",
            self.candidate,
            self.extensions.status.label()
        );
        self.extensions.render_issues(output);
        output.push('\n');
        for change in self.extensions.detail.iter().flatten() {
            let _ = writeln!(
                output,
                "  {} {} -> {}",
                change.name,
                change.baseline.as_deref().unwrap_or("absent"),
                change.candidate.as_deref().unwrap_or("absent")
            );
        }

        let _ = write!(
            output,
            "candidate={} settings={}",
            self.candidate,
            self.settings.status.label()
        );
        self.settings.render_issues(output);
        output.push('\n');
        for change in self.settings.detail.iter().flatten() {
            let _ = writeln!(
                output,
                "  {} {} -> {}",
                change.name,
                setting_label(change.baseline.as_ref()),
                setting_label(change.candidate.as_ref()),
            );
        }
    }
}

fn setting_label(setting: Option<&Setting>) -> String {
    setting.map_or_else(
        || "absent".to_owned(),
        |setting| match &setting.unit {
            Some(unit) => format!("{} {unit}", setting.value),
            None => setting.value.clone(),
        },
    )
}

#[derive(Serialize)]
pub(crate) struct FleetReport {
    format_version: u32,
    baseline: String,
    complete: bool,
    drift: bool,
    targets: Vec<Target>,
    comparisons: Vec<Comparison>,
}

impl FleetReport {
    pub(crate) fn build(baseline: &TargetProbe, candidates: &[TargetProbe]) -> Self {
        let targets: Vec<_> = std::iter::once(baseline)
            .chain(candidates)
            .map(Target::from)
            .collect();
        let comparisons: Vec<_> = candidates
            .iter()
            .map(|candidate| Comparison::new(baseline, candidate))
            .collect();
        Self {
            format_version: 1,
            baseline: baseline.profile.clone(),
            complete: targets
                .iter()
                .all(|target| target.status == TargetStatus::Complete),
            drift: comparisons.iter().any(Comparison::has_drift),
            targets,
            comparisons,
        }
    }

    pub(crate) const fn has_findings(&self) -> bool {
        !self.complete || self.drift
    }

    pub(crate) fn render_plain(&self) -> String {
        let mut output = format!(
            "baseline={} complete={} drift={}\n",
            self.baseline, self.complete, self.drift
        );
        for target in &self.targets {
            let _ = write!(
                output,
                "profile={} status={} environment={}",
                target.profile,
                target.status.label(),
                target.environment
            );
            if let Some(health) = &target.health {
                let _ = write!(
                    output,
                    " server_version_num={} in_recovery={} read_only={} tls={} sslmode={}",
                    health.server_version_num,
                    health.in_recovery,
                    health.read_only,
                    health.tls.label(),
                    health.sslmode
                );
            }
            if let Some(issue) = target.issue {
                let _ = write!(output, " issue={}", issue.label());
            }
            output.push('\n');
        }
        for comparison in &self.comparisons {
            comparison.render(&mut output);
        }
        output
    }

    pub(crate) fn render_table(&self) -> String {
        let mut output = format!(
            "PostgreSQL fleet | baseline @{} | complete {} | drift {}\n\n",
            self.baseline, self.complete, self.drift
        );
        let profile_width = self
            .targets
            .iter()
            .map(|target| target.profile.len())
            .max()
            .unwrap_or(7)
            .max(7);
        let _ = writeln!(
            output,
            "{:profile_width$}  {:<11}  {:<11}  {:<8}  {:<14}  {:<9}  Detail",
            "Profile", "Status", "Environment", "Version", "TLS", "Read-only"
        );
        for target in &self.targets {
            let (version, tls, read_only, detail) = match &target.health {
                Some(health) => (
                    health.server_version_num.to_string(),
                    health.tls.label(),
                    if health.read_only { "yes" } else { "no" },
                    format!(
                        "sslmode={} in_recovery={}",
                        health.sslmode, health.in_recovery
                    ),
                ),
                None => (
                    "-".to_owned(),
                    "-",
                    "-",
                    target.issue.map_or("-", Issue::label).to_owned(),
                ),
            };
            let _ = writeln!(
                output,
                "{:profile_width$}  {:<11}  {:<11}  {version:<8}  {tls:<14}  {read_only:<9}  {detail}",
                target.profile,
                target.status.label(),
                target.environment
            );
        }
        output.push('\n');
        for comparison in &self.comparisons {
            comparison.render(&mut output);
        }
        output
    }
}

#[cfg(test)]
mod tests;
