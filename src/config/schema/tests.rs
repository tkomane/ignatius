use super::*;

#[test]
fn defaults_are_valid_and_current() {
    let config = Config::default();
    assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(config.validate().is_empty(), "{:?}", config.validate());
    // Safe by default: mouse capture off keeps terminal selection working.
    assert!(!config.ui.mouse);
    assert!(
        !config.clipboard.osc52,
        "terminal clipboard transport is opt-in"
    );
}

#[test]
fn round_trips_through_toml_without_loss() {
    let config = Config::default();
    let text = toml::to_string_pretty(&config).expect("serialise");
    let parsed: Config = toml::from_str(&text).expect("parse");
    assert_eq!(config, parsed);
}

#[test]
fn unknown_keys_are_rejected_rather_than_silently_ignored() {
    let err = toml::from_str::<Config>("schema_version = 1\nthemes = \"dark\"\n")
        .expect_err("unknown key must fail");
    assert!(err.to_string().contains("themes"), "{err}");

    let err = toml::from_str::<Config>("schema_version = 1\n[ui]\ntheme = \"dark\"\nunknown = 1\n")
        .expect_err("unknown nested key must fail");
    assert!(err.to_string().contains("unknown"), "{err}");
}

#[test]
fn missing_schema_version_parses_as_version_zero_for_migration() {
    let config: Config = toml::from_str("[ui]\ntheme = \"light\"\n").expect("parse");
    assert_eq!(config.schema_version, 0);
    assert_eq!(config.ui.theme, ThemeChoice::Light);
    assert!(!config.clipboard.osc52);
}

#[test]
fn clipboard_setting_is_explicit_and_rejects_unknown_nested_fields() {
    let config: Config = toml::from_str("[clipboard]\nosc52 = true\n").expect("parse opt-in");
    assert!(config.clipboard.osc52);

    let err = toml::from_str::<Config>("[clipboard]\nunknown = true\n")
        .expect_err("unknown clipboard setting must fail");
    assert!(err.to_string().contains("unknown"), "{err}");
}

#[test]
fn validation_reports_every_problem_at_once() {
    let config = Config {
        schema_version: CURRENT_SCHEMA_VERSION,
        query: QueryConfig {
            max_buffered_rows: 0,
            statement_timeout_ms: 0,
        },
        connection: ConnectionConfig {
            connect_timeout_seconds: 0,
            application_name: "  ".into(),
        },
        ..Config::default()
    };
    let issues = config.validate();
    assert_eq!(issues.len(), 3, "{issues:#?}");
    for issue in &issues {
        assert!(
            !issue.suggestion.is_empty(),
            "every issue needs a next action"
        );
    }
}

#[test]
fn a_newer_schema_version_is_reported_not_guessed_at() {
    let config = Config {
        schema_version: CURRENT_SCHEMA_VERSION + 5,
        ..Config::default()
    };
    let issues = config.validate();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "schema-version");
}

#[test]
fn the_nerd_font_tier_round_trips_and_is_opt_in() {
    // Adding a tier must not invalidate a configuration written before it
    // existed, so this is an additive change with no migration.
    let config: Config = toml::from_str("[ui]\nglyphs = \"nerd-font\"\n").expect("parse");
    assert_eq!(config.ui.glyphs, GlyphMode::NerdFont);
    assert_eq!(UiConfig::default().glyphs, GlyphMode::Auto, "never assumed");
    let text = toml::to_string(&config).expect("serialise");
    assert!(text.contains("glyphs = \"nerd-font\""), "{text}");
}

#[test]
fn enum_values_are_kebab_case_in_the_file() {
    let text = toml::to_string(&Config {
        ui: UiConfig {
            theme: ThemeChoice::HighContrast,
            ..UiConfig::default()
        },
        ..Config::default()
    })
    .expect("serialise");
    assert!(text.contains("theme = \"high-contrast\""), "{text}");
    for accepted in ThemeChoice::ACCEPTED {
        assert!(!accepted.contains('_'), "documented values stay kebab-case");
    }
}

#[test]
fn the_color_depth_key_parses_each_pinned_value() {
    // The four values from `contracts/presentation.md`. Nothing else parses.
    for (text, expected) in [
        ("auto", ColorDepthChoice::Auto),
        ("truecolor", ColorDepthChoice::TrueColor),
        ("256", ColorDepthChoice::Indexed256),
        ("16", ColorDepthChoice::Basic16),
    ] {
        let config: Config =
            toml::from_str(&format!("[ui]\ncolor-depth = \"{text}\"\n")).expect("parse");
        assert_eq!(config.ui.color_depth, expected, "{text}");
    }
    assert_eq!(
        ColorDepthChoice::ACCEPTED,
        &["auto", "truecolor", "256", "16"],
        "the accepted values are pinned by the contract"
    );
}

#[test]
fn the_color_depth_key_refuses_a_value_outside_the_pinned_set() {
    for text in ["64", "true-color", "indexed-256", "24", "auto "] {
        let err = toml::from_str::<Config>(&format!("[ui]\ncolor-depth = \"{text}\"\n"))
            .expect_err("an unknown colour depth must be refused");
        assert!(
            err.to_string().contains(text),
            "the refusal repeats the refused value: {err}"
        );
    }
}

#[test]
fn color_depth_defaults_to_auto_and_the_key_may_be_missing() {
    assert_eq!(UiConfig::default().color_depth, ColorDepthChoice::Auto);
    // A configuration written before the key existed still parses.
    let config: Config = toml::from_str("[ui]\ntheme = \"dark\"\n").expect("parse");
    assert_eq!(config.ui.color_depth, ColorDepthChoice::Auto);
}
