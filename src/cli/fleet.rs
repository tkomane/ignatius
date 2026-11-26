//! Named-profile entry point for a read-only PostgreSQL fleet comparison.

use std::io::Write;
use std::time::Duration;

use clap::Args;
use futures_util::StreamExt;

use super::{CliTelemetryFormat, ConnectionOptions, auth_providers, resolve_target_and_profile};
use crate::ExitCode;
use crate::config::{self, Config, Paths};
use crate::connection::{ConnectionTarget, DEFAULT_PORT, EnvSnapshot, Environment, cloud};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::fleet::{FleetReport, Issue, MAX_TARGETS, Observation, TargetProbe};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const STATEMENT_TIMEOUT: Duration = Duration::from_secs(2);

/// Named PostgreSQL profiles to compare using metadata reads.
#[derive(Debug, Args, Clone)]
pub struct FleetArgs {
    /// Baseline profile, for example @development.
    #[arg(long, value_name = "@PROFILE")]
    baseline: String,
    /// Candidate profile. Repeat up to three times.
    #[arg(long, required = true, value_name = "@PROFILE")]
    candidate: Vec<String>,
    /// Limit schema snapshots to this PostgreSQL schema.
    #[arg(long, value_name = "SCHEMA")]
    schema: Option<String>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = CliTelemetryFormat::Table)]
    format: CliTelemetryFormat,
}

fn named_profile(value: &str) -> Result<&str, Diagnostic> {
    value
        .strip_prefix('@')
        .filter(|name| {
            (1..=64).contains(&name.len())
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        })
        .ok_or_else(|| usage("give fleet targets as @PROFILE using 1-64 ASCII slug characters"))
}

fn usage(message: &str) -> Diagnostic {
    Diagnostic::new(DiagnosticKind::Usage, message, "selecting fleet profiles")
}

fn select_profiles(args: &FleetArgs) -> Result<Vec<&str>, Diagnostic> {
    if !(1..MAX_TARGETS).contains(&args.candidate.len()) {
        return Err(usage(
            "fleet requires one baseline and one to three candidates",
        ));
    }
    if args.schema.as_ref().is_some_and(|schema| {
        schema.trim().is_empty() || schema.len() > 63 || schema.chars().any(char::is_control)
    }) {
        return Err(usage(
            "schema must be a non-empty PostgreSQL identifier of at most 63 bytes",
        ));
    }
    let profiles = std::iter::once(&args.baseline)
        .chain(&args.candidate)
        .map(|name| named_profile(name))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, profile) in profiles.iter().enumerate() {
        if profiles[..index].contains(profile) {
            return Err(usage(
                "fleet baseline and candidates must be distinct profiles",
            ));
        }
    }
    Ok(profiles)
}

fn resolve_profile(
    name: &str,
    config: &Config,
    env: &EnvSnapshot,
) -> Result<ConnectionTarget, Issue> {
    let profile = config.profiles.get(name).ok_or(Issue::Configuration)?;
    if [&profile.host, &profile.dbname, &profile.user]
        .iter()
        .any(|field| field.as_ref().is_none_or(|value| value.trim().is_empty()))
        || profile.port == Some(0)
    {
        return Err(Issue::Configuration);
    }
    let options = ConnectionOptions {
        profile: Some(name.to_owned()),
        read_only: true,
        ..ConnectionOptions::default()
    };
    let (_, mut args) =
        resolve_target_and_profile(None, &options, config).map_err(|_| Issue::Configuration)?;
    args.port.get_or_insert(DEFAULT_PORT);
    crate::connection::resolve(None, &args, env, &config.connection)
        .map_err(|error| Issue::from_diagnostic(&error))
}

const fn environment_label(environment: &Environment) -> &'static str {
    match environment {
        Environment::Unclassified => "unclassified",
        Environment::Local => "local",
        Environment::Development => "development",
        Environment::Test => "test",
        Environment::Staging => "staging",
        Environment::Production => "production",
        Environment::Custom(_) => "custom",
    }
}

async fn probe(
    mut target: ConnectionTarget,
    providers: &std::collections::BTreeMap<String, cloud::Provider>,
    schema: Option<&str>,
) -> Result<Observation, Issue> {
    target.connect_timeout = target.connect_timeout.min(PROBE_TIMEOUT);
    let sslmode = target.sslmode.as_str();
    let operation = async {
        let target = cloud::authenticate(target, providers)
            .await
            .map_err(|error| Issue::from_diagnostic(&error))?;
        let session = crate::postgres::session::connect(&target, STATEMENT_TIMEOUT)
            .await
            .map_err(|error| Issue::from_diagnostic(&error))?;
        session.fleet_probe(sslmode, schema).await
    };
    tokio::time::timeout(PROBE_TIMEOUT, operation)
        .await
        .map_err(|_| Issue::Timeout)?
}

pub(super) fn run(
    args: &FleetArgs,
    paths: &Paths,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode, Diagnostic> {
    let profiles = select_profiles(args)?;
    let config = config::load(paths)?.config;
    let providers = auth_providers(&config)?;
    let mut env = EnvSnapshot::from_process();
    // A single-target shell setting must not re-route the fleet or send a
    // shared identity to every profile. Passfile entries are matched per target.
    for key in [
        "PGSERVICE",
        "PGSERVICEFILE",
        "PGPASSWORD",
        "PGSSLMODE",
        "PGTARGETSESSIONATTRS",
        "PGSSLCERT",
        "PGSSLKEY",
    ] {
        env.values.remove(key);
    }
    let connections = profiles
        .into_iter()
        .map(|name| (name, resolve_profile(name, &config, &env)))
        .collect::<Vec<_>>();
    let schema = args.schema.as_deref();
    let probes = super::runtime()?.block_on(async {
        futures_util::stream::iter(connections)
            .map(|(name, target)| {
                let providers = &providers;
                async move {
                    let environment = target.as_ref().map_or("unclassified", |target| {
                        environment_label(&target.environment)
                    });
                    let result = match target {
                        Ok(target) => probe(target, providers, schema).await,
                        Err(reason) => Err(reason),
                    };
                    TargetProbe {
                        profile: name.to_owned(),
                        environment,
                        result,
                    }
                }
            })
            .buffered(MAX_TARGETS)
            .collect::<Vec<_>>()
            .await
    });
    let Some((baseline, candidates)) = probes.split_first() else {
        return Err(usage("fleet requires a baseline profile"));
    };
    let report = FleetReport::build(baseline, candidates);
    let rendered = match args.format {
        CliTelemetryFormat::Table => report.render_table(),
        CliTelemetryFormat::Plain => report.render_plain(),
        CliTelemetryFormat::Json => serde_json::to_string_pretty(&report).map_err(|_| {
            Diagnostic::new(
                DiagnosticKind::Internal,
                "could not encode the fleet report",
                "formatting JSON",
            )
        })?,
    };
    super::write_report_output(rendered, None, false, "Fleet report", out, err)?;
    Ok(if report.has_findings() {
        ExitCode::Findings
    } else {
        ExitCode::Success
    })
}
