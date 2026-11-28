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
    assert_eq!(production.auth.as_deref(), Some("entra"));
    assert!(production.provider_presentation.is_none());
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
        crate::connection::cloud::Provider {
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
    let presentations = crate::connection::cloud::presentation_registry(&providers);
    let summary =
        ConnectionProfileSummary::from_profile("custom-profile", &profile, &presentations);

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
fn configured_override_summary_uses_the_selected_provider_definition() {
    let config: crate::config::Config = toml::from_str(
        r#"[auth.providers.entra]
command = ["fixture-provider", "hidden-fixture-argument"]
json-field = "fixtureToken"
remedy = "open the fixture identity tool"
"#,
    )
    .expect("auth provider configuration");
    let configured = crate::cli::auth_providers(&config).expect("validated provider definitions");
    let presentations = crate::connection::cloud::presentation_registry(&configured);
    let profile = profile(r#"auth = "entra""#);
    let summary = ConnectionProfileSummary::from_profile("override", &profile, &presentations);
    let view = summary
        .provider_presentation
        .as_ref()
        .expect("provider facts");

    assert_eq!(summary.auth.as_deref(), Some("entra"));
    assert_eq!(view.name, "entra");
    assert_eq!(view.display_name, r#"Configured provider "entra""#);
    assert_eq!(view.command_name, "fixture-provider");
    assert_eq!(
        view.invocation,
        "configured provider command (arguments hidden)"
    );
    assert_eq!(view.extraction, r#"JSON field "fixtureToken""#);
    assert_eq!(
        view.lifetime,
        "short-lived credential, lifetime defined by the provider"
    );
    assert_eq!(view.remedy, "open the fixture identity tool");
    assert!(!format!("{summary:?}").contains("hidden-fixture-argument"));
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

#[test]
fn target_session_attrs_is_shown_in_picker_only_when_not_any() {
    let standby = profile("host = \"replica.example.net\"\ntarget-session-attrs = \"standby\"\n");
    let summary = ConnectionProfileSummary::from_profile("replica", &standby, &BTreeMap::new());
    assert_eq!(summary.target_session_attrs.as_deref(), Some("standby"));
    assert!(summary.detail().contains("target-session-attrs standby"));

    let any = profile("host = \"any.example.net\"\ntarget-session-attrs = \"ANY\"\n");
    let any_summary = ConnectionProfileSummary::from_profile("any", &any, &BTreeMap::new());
    assert!(any_summary.target_session_attrs.is_none());
    assert!(!any_summary.detail().contains("target-session-attrs"));

    let invalid = profile("host = \"bad.example.net\"\ntarget-session-attrs = \"bogus\"\n");
    let bad_summary = ConnectionProfileSummary::from_profile("bad", &invalid, &BTreeMap::new());
    assert!(bad_summary.target_session_attrs.is_none());
    assert!(
        bad_summary
            .validation
            .as_deref()
            .is_some_and(|v| v.contains("target-session-attrs"))
    );
    assert!(
        bad_summary
            .detail()
            .contains("configuration needs attention")
    );
}

#[test]
fn session_timeouts_are_shown_in_picker_detail_when_configured() {
    let timed = profile(
        "host = \"db.example.net\"\nstatement-timeout = \"30s\"\nlock-timeout = \"500ms\"\n\
         idle-in-transaction-session-timeout = \"5m\"\n",
    );
    let summary = ConnectionProfileSummary::from_profile("timed", &timed, &BTreeMap::new());
    assert_eq!(summary.statement_timeout.as_deref(), Some("30s"));
    assert_eq!(summary.lock_timeout.as_deref(), Some("500ms"));
    assert_eq!(
        summary.idle_in_transaction_session_timeout.as_deref(),
        Some("5m")
    );
    assert!(summary.detail().contains("statement-timeout 30s"));
    assert!(summary.detail().contains("lock-timeout 500ms"));
    assert!(summary.detail().contains("idle-in-tx-timeout 5m"));

    let disabled = profile("host = \"db.example.net\"\nstatement-timeout = \"0\"\n");
    let disabled_summary =
        ConnectionProfileSummary::from_profile("disabled", &disabled, &BTreeMap::new());
    assert_eq!(
        disabled_summary.statement_timeout.as_deref(),
        Some("disabled")
    );
    assert!(disabled_summary.lock_timeout.is_none());
    assert!(
        disabled_summary
            .detail()
            .contains("statement-timeout disabled")
    );
    assert!(!disabled_summary.detail().contains("lock-timeout"));

    let invalid = profile("host = \"bad.example.net\"\nstatement-timeout = \"bogus\"\n");
    let bad_summary = ConnectionProfileSummary::from_profile("bad", &invalid, &BTreeMap::new());
    assert!(bad_summary.statement_timeout.is_none());
    assert!(
        bad_summary
            .validation
            .as_deref()
            .is_some_and(|v| v.contains("statement-timeout"))
    );

    let scoped = profile("host = \"db.example.net\"\nsearch-path = \"analytics, public\"\n");
    let scoped_summary =
        ConnectionProfileSummary::from_profile("scoped", &scoped, &BTreeMap::new());
    assert_eq!(
        scoped_summary.search_path.as_deref(),
        Some("analytics, public")
    );
    assert!(
        scoped_summary
            .detail()
            .contains("search-path analytics, public")
    );
}

#[test]
fn multi_host_profile_summary_shows_candidate_hosts_and_validates_empty_segments() {
    let multi =
        profile("host = \"db1.example.net,db2.example.net\"\nport = 6432\ndbname = \"orders\"\n");
    let summary = ConnectionProfileSummary::from_profile("ha", &multi, &BTreeMap::new());
    assert!(summary.validation.is_none(), "{:?}", summary.validation);
    assert_eq!(
        summary.location,
        "db1.example.net:6432,db2.example.net:6432 (2 hosts)"
    );
    assert!(summary.detail().contains("2 hosts"));

    let no_port = profile("host = \"db1.example.net,db2.example.net\"\n");
    let no_port_summary =
        ConnectionProfileSummary::from_profile("ha-default", &no_port, &BTreeMap::new());
    assert_eq!(
        no_port_summary.location,
        "db1.example.net,db2.example.net (2 hosts, port from defaults)"
    );

    let invalid = profile("host = \"db1.example.net,,db2.example.net\"\n");
    let invalid_summary =
        ConnectionProfileSummary::from_profile("ha-bad", &invalid, &BTreeMap::new());
    assert!(
        invalid_summary.validation.is_some(),
        "empty host segment must fail validation"
    );
    assert!(
        invalid_summary
            .detail()
            .contains("configuration needs attention")
    );
}
