//! Named-profile fleet CLI and disposable PostgreSQL acceptance.
#![allow(clippy::print_stderr)]

use ignatius::config::{Config, schema::Profile};
use ignatius::connection::{EnvSnapshot, parse_connection_string};
use serde_json::Value;
use std::fmt::Write as _;
use std::process::{Command, Output};
use tempfile::TempDir;

fn binary(root: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    command.env("IGNATIUS_CONFIG_DIR", root.path());
    command.env("IGNATIUS_DATA_DIR", root.path().join("data"));
    command.env_remove("IGNATIUS_LOG");
    for key in EnvSnapshot::from_process().values.keys() {
        command.env_remove(key);
    }
    command.env("PGPASSFILE", root.path().join("pgpass"));
    command
}

fn run(root: &TempDir, args: &[&str]) -> Output {
    binary(root).args(args).output().expect("run fleet")
}

fn write_config(root: &TempDir, config: &Config) {
    std::fs::write(
        root.path().join("config.toml"),
        toml::to_string(config).expect("encode test config"),
    )
    .expect("write test config");
}

#[test]
fn refuses_uris_duplicates_and_more_than_three_candidates() {
    let root = tempfile::tempdir().unwrap();
    let invalid_route = [
        "postgres",
        "://some-user:",
        "private-example",
        "@some-host/db",
    ]
    .concat();
    for arguments in [
        vec![
            "fleet",
            "--baseline",
            &invalid_route,
            "--candidate",
            "@test",
        ],
        vec!["fleet", "--baseline", "@test", "--candidate", "@test"],
        vec![
            "fleet",
            "--baseline",
            "@one",
            "--candidate",
            "@two",
            "--candidate",
            "@three",
            "--candidate",
            "@four",
            "--candidate",
            "@five",
        ],
    ] {
        let output = run(&root, &arguments);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-example"));
    }
}

#[test]
fn never_resolves_incomplete_profiles_through_environment_or_pgservice() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.profiles.insert(
        "unfinished".to_owned(),
        Profile {
            dbname: Some("configured_database".to_owned()),
            ..Profile::default()
        },
    );
    write_config(&root, &config);
    let output = binary(&root)
        .env("PGHOST", "localhost")
        .env("PGPORT", "1")
        .env("PGUSER", "fixture")
        .env("PGSERVICE", "no-such-service")
        .args([
            "fleet",
            "--baseline",
            "@unfinished",
            "--candidate",
            "@missing",
            "--format",
            "json",
        ])
        .output()
        .expect("run fleet");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["targets"][0]["profile"], "unfinished");
    assert_eq!(json["targets"][0]["issue"], "configuration");
    assert_eq!(json["targets"][1]["profile"], "missing");
    assert_eq!(json["targets"][1]["issue"], "configuration");
    assert!(json["targets"][0]["health"].is_null());
    assert_eq!(json["comparisons"][0]["schema"]["status"], "unavailable");
}

fn fixture_uri(variable: &str) -> Option<String> {
    let value = std::env::var(variable)
        .ok()
        .filter(|value| !value.is_empty());
    if value.is_none() {
        eprintln!("skipping fleet fixture: {variable} is not set");
    }
    value
}

fn passfile_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace(':', "\\:")
}

fn fixture(root: &TempDir, uris: &[(&str, &str, &str)]) -> Command {
    let mut config = Config::default();
    let mut command = binary(root);
    if uris.iter().any(|&(_, _, mode)| mode == "verify-full") {
        command.env(
            "PGSSLROOTCERT",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docker/tls/generated/ca.crt"),
        );
    }
    let mut passwords = String::new();
    for &(profile_name, uri, mode) in uris {
        let parts = parse_connection_string(uri).expect("parse synthetic fixture target");
        for (parameter, variable) in [
            ("sslrootcert", "PGSSLROOTCERT"),
            ("sslcert", "PGSSLCERT"),
            ("sslkey", "PGSSLKEY"),
        ] {
            if let Some(value) = parts.get(parameter) {
                command.env(variable, value);
            }
        }
        let host = &parts["host"];
        let port = parts["port"].parse::<u16>().expect("fixture port");
        let database = &parts["dbname"];
        let user = &parts["user"];
        if let Some(password) = parts.get("password") {
            writeln!(
                passwords,
                "{}:{port}:{}:{}:{}",
                passfile_value(host),
                passfile_value(database),
                passfile_value(user),
                passfile_value(password),
            )
            .expect("format the synthetic passfile in memory");
        }
        config.profiles.insert(
            profile_name.to_owned(),
            Profile {
                host: Some(host.clone()),
                port: Some(port),
                dbname: Some(database.clone()),
                user: Some(user.clone()),
                sslmode: Some(mode.to_owned()),
                environment: Some("test".to_owned()),
                ..Profile::default()
            },
        );
    }
    let passfile = root.path().join("pgpass");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write as _;
    options
        .open(passfile)
        .unwrap()
        .write_all(passwords.as_bytes())
        .unwrap();
    write_config(root, &config);
    command
}

#[test]
fn postgres_plain_proves_equal_read_only_catalogs_and_preserves_available_sections() {
    let Some(uri) = fixture_uri("IGNATIUS_TEST_PG_URI") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let mut command = fixture(
        &root,
        &[
            ("baseline", &uri, "disable"),
            ("candidate", &uri, "disable"),
        ],
    );
    command
        .env("PGSERVICE", "ignored-fixture-service")
        .env("PGPASSWORD", "synthetic-ambient-password-must-not-be-used");
    let args = [
        "fleet",
        "--baseline",
        "@baseline",
        "--candidate",
        "@candidate",
    ];
    let output = command
        .args(args)
        .args(["--schema", "public", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["format_version"], 1);
    assert_eq!(json["complete"], true);
    assert_eq!(json["targets"][0]["health"]["read_only"], true);
    assert_eq!(json["targets"][0]["health"]["tls"], "disabled");
    assert_eq!(json["comparisons"][0]["schema"]["status"], "match");
    assert_eq!(
        json["comparisons"][0]["schema"]["detail"]["baseline_fingerprint"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(json["comparisons"][0]["extensions"]["status"], "match");
    assert_eq!(json["comparisons"][0]["settings"]["status"], "match");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("standard delivery"));

    let output = run(
        &root,
        &[
            "fleet",
            "--baseline",
            "@baseline",
            "--candidate",
            "@candidate",
            "--candidate",
            "@absent",
            "--schema",
            "nonexistent_fleet_fixture",
            "--format",
            "plain",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let plain = String::from_utf8(output.stdout).unwrap();
    assert!(plain.contains("profile=baseline status=partial"));
    assert!(plain.contains("candidate=candidate schema=unavailable baseline_issue=schema-missing"));
    assert!(plain.contains("candidate=candidate settings=match"));
    assert!(plain.contains("profile=absent status=unavailable"));
    assert!(!root.path().join("data").exists());
}

#[test]
fn postgres_tls_proves_negotiated_transport_with_read_only_enabled() {
    let Some(uri) = fixture_uri("IGNATIUS_TEST_PG_TLS_URI") else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let output = fixture(
        &root,
        &[
            ("tls-base", &uri, "verify-full"),
            ("tls-peer", &uri, "verify-full"),
        ],
    )
    .args([
        "fleet",
        "--baseline",
        "@tls-base",
        "--candidate",
        "@tls-peer",
        "--format",
        "json",
    ])
    .output()
    .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    for target in json["targets"].as_array().unwrap() {
        assert_eq!(target["health"]["read_only"], true);
        assert_eq!(target["health"]["tls"], "active");
        assert_eq!(target["health"]["sslmode"], "verify-full");
    }
    assert!(output.stderr.is_empty());
}

#[test]
fn postgres_fleet_compares_plain_and_tls_servers_in_one_run() {
    let (Some(plain), Some(tls)) = (
        fixture_uri("IGNATIUS_TEST_PG_URI"),
        fixture_uri("IGNATIUS_TEST_PG_TLS_URI"),
    ) else {
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let output = fixture(
        &root,
        &[("plain", &plain, "disable"), ("tls", &tls, "verify-full")],
    )
    .args([
        "fleet",
        "--baseline",
        "@plain",
        "--candidate",
        "@tls",
        "--format",
        "json",
    ])
    .output()
    .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["complete"], true, "{json}");
    assert_eq!(json["targets"][0]["health"]["tls"], "disabled");
    assert_eq!(json["targets"][1]["health"]["tls"], "active");
    assert_eq!(
        output.status.code(),
        Some(i32::from(json["drift"].as_bool().unwrap())),
    );
    assert!(output.stderr.is_empty());
}
