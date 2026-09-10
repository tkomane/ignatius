//! Safe, ephemeral connection choices for the interactive picker.
//!
//! The configuration file remains the authority for named connections. This
//! module copies only the fields that are safe and useful to show, so the
//! application model never needs to carry a whole profile or its unknown
//! values. A selected row carries its name back to the runtime, which resolves
//! the real target through the existing connection boundary.

use crate::config::schema::Profile;
use crate::connection::cloud::{Provider, ProviderPresentation};
use std::collections::BTreeMap;

/// Maximum number of Unicode scalar values in one row's derived detail.
const MAX_DETAIL_CHARS: usize = 240;

/// Safe display data for one configured connection profile.
///
/// This is deliberately not a clone of [`Profile`]. In particular, the
/// flattened unknown-field map is not copied, because it may contain a value
/// that the profile validator must refuse, including a password-shaped field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProfileSummary {
    /// The profile map key.
    pub name: String,
    /// Optional human description.
    pub description: Option<String>,
    /// Safe location hint, without resolving environment or service files.
    pub location: String,
    /// Safe database hint.
    pub database: String,
    /// Safe role hint.
    pub user: String,
    /// Configured TLS mode, or an explicit statement that the target default
    /// will decide it.
    pub transport: String,
    /// Configured classification, or the default unclassified state.
    pub environment: String,
    /// Whether the profile asks PostgreSQL to make the session read-only.
    pub read_only: bool,
    /// Provider name only, never a token or command output.
    pub auth: Option<String>,
    /// Safe facts used by the connection trust surface after a switch.
    pub provider_presentation: Option<ProviderPresentation>,
    /// A safe validation headline when the profile needs attention.
    pub validation: Option<String>,
}

impl ConnectionProfileSummary {
    /// Builds a safe summary from one profile and the already-known providers.
    #[must_use]
    pub fn from_profile(
        name: &str,
        profile: &Profile,
        providers: &BTreeMap<String, Provider>,
    ) -> Self {
        let validation = profile.validate(name).err().map(|error| error.headline);
        let auth = profile.auth.clone();
        let provider_presentation = auth
            .as_deref()
            .and_then(|provider| providers.get(provider))
            .map(Provider::presentation);

        Self {
            // The name is also the resolver key, so preserve it exactly. The
            // renderer owns viewport clipping and display sanitisation.
            name: name.to_owned(),
            description: profile.description.as_deref().map(bounded),
            location: bounded(&location(profile)),
            database: bounded(&profile.dbname.as_deref().map_or_else(
                || "database from environment/defaults".to_owned(),
                |value| value.to_owned(),
            )),
            user: bounded(&profile.user.as_deref().map_or_else(
                || "role from environment/defaults".to_owned(),
                |value| value.to_owned(),
            )),
            transport: bounded(&profile.sslmode.as_deref().map_or_else(
                || "TLS from target defaults".to_owned(),
                |value| format!("TLS {value}"),
            )),
            environment: bounded(&profile.environment.as_deref().map_or_else(
                || "environment unclassified".to_owned(),
                |value| format!("environment {value}"),
            )),
            read_only: profile.read_only,
            auth,
            provider_presentation,
            validation: validation.map(|value| bounded(&value)),
        }
    }

    /// The single detail line shown beside the profile name.
    #[must_use]
    pub fn detail(&self) -> String {
        let mut parts = vec![
            self.location.clone(),
            self.database.clone(),
            self.user.clone(),
            self.transport.clone(),
            self.environment.clone(),
        ];
        if self.read_only {
            parts.push("read-only".to_owned());
        }
        if let Some(provider) = &self.auth {
            parts.push(format!("provider {provider}"));
        }
        if let Some(description) = &self.description
            && !description.trim().is_empty()
        {
            parts.push(format!("description {description}"));
        }
        if let Some(validation) = &self.validation {
            parts.push(format!("configuration needs attention: {validation}"));
        }
        bounded(&parts.join(" | "))
    }
}

/// Builds summaries in the configuration map's deterministic order.
#[must_use]
pub fn summaries(
    profiles: &BTreeMap<String, Profile>,
    providers: &BTreeMap<String, Provider>,
) -> Vec<ConnectionProfileSummary> {
    profiles
        .iter()
        .map(|(name, profile)| ConnectionProfileSummary::from_profile(name, profile, providers))
        .collect()
}

/// Describes a profile's configured host and port without claiming resolution.
fn location(profile: &Profile) -> String {
    match (profile.host.as_deref(), profile.port) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => format!("{host}:port from defaults"),
        (None, Some(port)) => format!("host from environment/defaults:{port}"),
        (None, None) => "host and port from environment/defaults".to_owned(),
    }
}

/// Bounds display-derived text without changing the source configuration.
fn bounded(text: &str) -> String {
    let mut value: String = text.chars().take(MAX_DETAIL_CHARS).collect();
    if text.chars().count() > MAX_DETAIL_CHARS {
        value.push_str("...");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn profile(text: &str) -> Profile {
        toml::from_str(text).expect("profile")
    }

    #[test]
    fn summaries_show_safe_hints_and_keep_configuration_order() {
        let profiles = BTreeMap::from([
            (
                "orders-prod".to_owned(),
                profile(
                    r#"host = "db.example.net"
port = 6432
dbname = "orders"
user = "app"
sslmode = "verify-full"
environment = "production"
read-only = true
description = "careful reads"
auth = "entra"
"#,
                ),
            ),
            (
                "orders-dev".to_owned(),
                profile("host = \"127.0.0.1\"\ndbname = \"orders\"\n"),
            ),
        ]);
        let providers = BTreeMap::new();
        let result = summaries(&profiles, &providers);

        assert_eq!(
            result
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["orders-dev", "orders-prod"]
        );
        let production = &result[1];
        assert!(production.detail().contains("db.example.net:6432"));
        assert!(production.detail().contains("TLS verify-full"));
        assert!(production.detail().contains("environment production"));
        assert!(production.detail().contains("read-only"));
        assert!(production.detail().contains("description careful reads"));
    }

    #[test]
    fn missing_fields_are_words_about_unresolved_defaults() {
        let profiles = BTreeMap::from([("partial".to_owned(), profile(""))]);
        let result = summaries(&profiles, &BTreeMap::new());
        let detail = result[0].detail();
        assert!(detail.contains("host and port from environment/defaults"));
        assert!(detail.contains("database from environment/defaults"));
        assert!(detail.contains("role from environment/defaults"));
        assert!(detail.contains("TLS from target defaults"));
        assert!(detail.contains("environment unclassified"));
    }

    #[test]
    fn unknown_values_are_not_copied_even_when_they_are_secret_shaped() {
        let profile = profile(
            r#"host = "safe-host"
password = "hunter2"
"#,
        );
        let mut profiles = BTreeMap::new();
        profiles.insert("suspicious-profile".to_owned(), profile);
        let summary = summaries(&profiles, &BTreeMap::new()).remove(0);
        let debug = format!("{summary:?}");
        assert!(summary.validation.is_some());
        assert!(!debug.contains("hunter2"));
        assert!(!summary.detail().contains("hunter2"));
        assert!(summary.detail().contains("configuration needs attention"));
    }

    #[test]
    fn provider_summaries_keep_the_name_but_not_the_argument_vector() {
        let profile = Profile {
            auth: Some("custom".to_owned()),
            ..Profile::default()
        };
        let providers = BTreeMap::from([(
            "custom".to_owned(),
            Provider {
                name: "custom".to_owned(),
                command: vec![
                    "secret-tool".to_owned(),
                    "--token".to_owned(),
                    "hunter2".to_owned(),
                ],
                extract: crate::connection::cloud::Extract::Raw,
                timeout: std::time::Duration::from_secs(5),
                remedy: "sign in again".to_owned(),
            },
        )]);
        let summary =
            ConnectionProfileSummary::from_profile("custom-profile", &profile, &providers);

        assert_eq!(summary.auth.as_deref(), Some("custom"));
        let presentation = summary
            .provider_presentation
            .as_ref()
            .expect("provider facts");
        assert_eq!(presentation.command_name, "secret-tool");
        assert!(presentation.invocation.contains("arguments hidden"));
        let debug = format!("{summary:?}");
        assert!(!debug.contains("--token"));
        assert!(!debug.contains("hunter2"));
    }

    #[test]
    fn control_text_is_bounded_in_derived_details() {
        let description = "x".repeat(MAX_DETAIL_CHARS + 50);
        let profile = Profile {
            description: Some(description),
            ..Profile::default()
        };
        let summary = ConnectionProfileSummary::from_profile("profile", &profile, &BTreeMap::new());
        assert!(summary.detail().chars().count() <= MAX_DETAIL_CHARS + 3);
    }
}
