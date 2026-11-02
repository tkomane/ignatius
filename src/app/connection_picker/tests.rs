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
    let summary = ConnectionProfileSummary::from_profile("custom-profile", &profile, &providers);

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
