//! Manual-theme startup/refusal contracts through the actual binary.

use std::path::Path;
use std::process::{Command, Output};

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
    command.env_clear();
    for key in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SystemRoot",
        "WINDIR",
        "TMP",
        "TEMP",
        "TMPDIR",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("IGNATIUS_CONFIG_DIR", dir)
        .env("IGNATIUS_DATA_DIR", dir.join("absent-data"))
        .env("PGPASSFILE", dir.join("absent-passfile"))
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run isolated client")
}

#[test]
fn manual_theme_refusals_are_exact_before_terminal_connection_or_migration_writes() {
    let cases = [
        (
            "[ui]\ntheme = \"custom\"\n",
            "ui.theme = \"custom\" requires a [theme] table.",
        ),
        (
            "[ui]\ntheme = \"auto\"\n",
            "ui.theme = \"auto\" is not available in this build. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ),
        (
            "[ui]\ntheme = \"Dark\"\n",
            "ui.theme = \"Dark\" is not a theme. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ),
        (
            "[ui]\ntheme = 7\n",
            "ui.theme must be a string. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ),
        ("theme = 7\n", "theme must be a table."),
        (
            "[theme]\nextends = \"custom\"\n",
            "[theme] extends = \"custom\" is not a built-in theme. Use \"dark\", \"light\" or \"high-contrast\".",
        ),
        (
            "[theme]\nunknown = \"#FFFFFF\"\n",
            "[theme] has no token named \"unknown\". Token names are listed in docs/design/theme-tokens.md.",
        ),
        (
            "[theme]\ntext = 7\n",
            "[theme] text must be a colour string. Use \"#RRGGBB\" hex, for example \"#F38BA8\".",
        ),
        (
            "[theme]\ntext = \"red\"\n",
            "[theme] text = \"red\" is not a colour. Use \"#RRGGBB\" hex, for example \"#F38BA8\".",
        ),
        (
            "[theme]\nsurface = \"#000000\"\ntext = \"#000000\"\n",
            "[theme] text on surface is 1.0:1; this pair must reach 4.5:1.",
        ),
    ];
    for version in [0, 1] {
        for (body, detail) in cases {
            let dir = tempfile::tempdir().unwrap();
            let source = format!("schema_version = {version}\n{body}");
            std::fs::write(dir.path().join("config.toml"), &source).unwrap();
            for args in [
                vec![],
                vec!["connect"],
                vec!["config", "validate"],
                vec!["config", "show"],
                vec!["config", "migrate"],
                vec!["config", "migrate", "--dry-run"],
                vec![
                    "query",
                    "postgres://app@127.0.0.1:1/never-contact",
                    "-c",
                    "SELECT 1",
                ],
            ] {
                let output = run(dir.path(), &args);
                assert_eq!(output.status.code(), Some(3), "{body} {args:?}: {output:?}");
                assert!(
                    output.stdout.is_empty(),
                    "refusal polluted stdout: {output:?}"
                );
                assert_eq!(
                    String::from_utf8(output.stderr).unwrap(),
                    format!("configuration error: {detail}\n"),
                    "{args:?}"
                );
                assert_eq!(
                    std::fs::read_to_string(dir.path().join("config.toml")).unwrap(),
                    source
                );
                assert_eq!(
                    std::fs::read_dir(dir.path()).unwrap().count(),
                    1,
                    "refusal created state or backup"
                );
            }
        }
    }
}

#[test]
fn manual_theme_valid_custom_roundtrips_without_serializing_the_cache() {
    for selection in ["dark", "light", "high-contrast", "custom"] {
        let dir = tempfile::tempdir().unwrap();
        let source = format!(
            "schema_version = 1\n[ui]\ntheme = \"{selection}\"\n[theme]\nextends = \"light\"\n"
        );
        std::fs::write(dir.path().join("config.toml"), &source).unwrap();
        let output = run(dir.path(), &["config", "validate"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let output = run(dir.path(), &["config", "show"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let config: toml::Value =
            toml::from_str(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
        assert_eq!(config["ui"]["theme"].as_str(), Some(selection));
        assert_eq!(
            config["theme"].as_table().unwrap().len(),
            1,
            "only the explicit extends is serialized"
        );
        assert_eq!(config["theme"]["extends"].as_str(), Some("light"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.toml")).unwrap(),
            source
        );
    }
}

#[test]
fn manual_theme_invalid_configuration_remains_a_failed_doctor_report() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "[ui]\ntheme = \"custom\"\n").unwrap();
    let output = run(dir.path(), &["doctor", "--json"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(output.stderr.is_empty(), "doctor keeps its report envelope");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let check = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "configuration")
        .unwrap();
    assert_eq!(check["status"], "FAIL");
    assert!(
        check["detail"]
            .as_str()
            .unwrap()
            .contains("requires a [theme] table")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains('\u{1b}'));
}

#[test]
fn manual_theme_reflection_is_redacted_escaped_and_bounded_in_the_actual_cli() {
    let dir = tempfile::tempdir().unwrap();
    let marker = ["synthetic", "theme", "credential"].join("-");
    let raw = format!(
        "postgres://user:{marker}@invalid/db\n\u{1b}[31mé{}",
        "x".repeat(150)
    );
    let source = toml::to_string(&toml::toml! { ui = { theme = raw } }).unwrap();
    std::fs::write(dir.path().join("config.toml"), source).unwrap();
    let output = run(dir.path(), &["config", "validate"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.lines().count(),
        1,
        "one physical diagnostic line: {stderr:?}"
    );
    assert!(stderr.is_ascii());
    assert!(!stderr.contains(&marker));
    assert!(!stderr.contains('\u{1b}'));
    assert!(stderr.contains("[truncated]"));
    assert!(stderr.len() < 250);
    assert!(stderr.starts_with("configuration error: ui.theme = \""));
}
