use super::*;

#[test]
fn density_accepts_only_the_two_contract_values_and_round_trips() {
    for value in ["comfortable", "compact"] {
        let config: Config =
            toml::from_str(&format!("[ui]\ndensity = \"{value}\"\n")).expect("supported density");
        assert!(
            toml::to_string(&config)
                .unwrap()
                .contains(&format!("density = \"{value}\""))
        );
    }
    for value in ["dense", "Comfortable", "", "auto"] {
        assert!(toml::from_str::<Config>(&format!("[ui]\ndensity = \"{value}\"\n")).is_err());
    }
    assert!(toml::from_str::<Config>("[ui]\ndensity = 1\n").is_err());
}

#[test]
fn defaults_are_valid_and_current() {
    let config = Config::default();
    assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(config.validate().is_empty(), "{:?}", config.validate());
    // Mouse reporting is on by default.
    assert!(config.ui.mouse);
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
    assert_eq!(config.ui.theme, ThemeSelection::Light);
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
            theme: ThemeSelection::HighContrast,
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

#[test]
fn pending_defaults_keep_theme_absence_distinct_and_ordinary_defaults_unchanged() {
    let pending: PendingConfig = toml::from_str("").unwrap();
    let ordinary: Config = toml::from_str("").unwrap();
    assert!(pending.ui.theme.is_none());
    assert!(pending.theme.is_none());
    assert_eq!(pending.schema_version, ordinary.schema_version);
    assert_eq!(pending.ui.density, ordinary.ui.density);
    assert_eq!(pending.ui.color, ordinary.ui.color);
    assert_eq!(pending.ui.color_depth, ordinary.ui.color_depth);
    assert_eq!(pending.ui.glyphs, ordinary.ui.glyphs);
    assert_eq!(pending.ui.mouse, ordinary.ui.mouse);
    assert_eq!(pending.ui.reduced_motion, ordinary.ui.reduced_motion);
    assert_eq!(pending.ui.completion, ordinary.ui.completion);
    assert_eq!(pending.query, ordinary.query);
    assert_eq!(pending.connection, ordinary.connection);
    assert_eq!(pending.history, ordinary.history);
    assert_eq!(pending.clipboard, ordinary.clipboard);
    assert_eq!(pending.profiles, ordinary.profiles);
    assert_eq!(pending.keys, ordinary.keys);
    assert_eq!(pending.auth, ordinary.auth);
    assert_eq!(
        PendingConfig::default().schema_version,
        CURRENT_SCHEMA_VERSION
    );
}

#[test]
fn pending_selection_and_definition_capture_every_toml_value_shape() {
    for value in [
        "\"synthetic\"",
        "12",
        "1.25",
        "false",
        "1979-05-27T07:32:00Z",
        "[1, \"two\"]",
        "{ nested = [1, \"two\"] }",
    ] {
        let pending: PendingConfig =
            toml::from_str(&format!("theme = {value}\n[ui]\ntheme = {value}\n")).unwrap();
        let selection = &pending.ui.theme.as_ref().unwrap().0;
        let definition = &pending.theme.as_ref().unwrap().0;
        assert_eq!(selection, definition);
    }
    let empty: PendingConfig = toml::from_str("[theme]\n").unwrap();
    assert!(empty.theme.unwrap().0.as_table().unwrap().is_empty());
}

#[test]
fn pending_theme_debug_never_prints_raw_values_or_keys() {
    let pending: PendingConfig = toml::from_str("[ui]\ntheme = \"SYNTHETIC_SELECTION_SENTINEL\"\n[theme]\nSYNTHETIC_KEY_SENTINEL = \"SYNTHETIC_VALUE_SENTINEL\"\n").unwrap();
    let debug = format!("{pending:?}");
    assert!(debug.contains("DeferredThemeValue(<hidden>)"));
    assert!(!debug.contains("SYNTHETIC_"), "{debug}");
}

#[test]
fn pending_non_theme_types_unknowns_and_validation_remain_strict() {
    for source in [
        "ui = false\n",
        "query = false\n",
        "[ui]\nunknown = 1\n",
        "[query]\nmax-buffered-rows = \"wrong\"\n",
        "[connection]\nunknown = 1\n",
    ] {
        let old = toml::from_str::<Config>(source).unwrap_err();
        let pending = toml::from_str::<PendingConfig>(source).unwrap_err();
        assert_eq!(pending.message(), old.message());
        assert_eq!(pending.span(), old.span());
    }
    let source = "[query]\nmax-buffered-rows = 0\n[connection]\nconnect-timeout-seconds = 0\napplication-name = \" \"\n";
    let ordinary: Config = toml::from_str(source).unwrap();
    let pending: PendingConfig = toml::from_str(source).unwrap();
    assert_eq!(pending.validate(), ordinary.validate());
    assert_eq!(pending.validate().len(), 3);
    assert_eq!(
        toml::from_str::<PendingConfig>("ui = false\n")
            .unwrap_err()
            .message(),
        "invalid type: boolean `false`, expected struct UiConfig"
    );
}

#[test]
fn pending_syntax_and_theme_duplicate_errors_keep_the_original_message_and_span() {
    for source in [
        "[ui]\ntheme = \"dark\n",
        "[theme]\ndanger = \"#112233\"\ndanger = \"#445566\"\n",
        "[theme.nested]\nvalue = 1\nvalue = 2\n",
        "[theme]\nsyntax.comment = \"first\"\nsyntax.comment = \"second\"\n",
        "[theme]\nextends = \"dark\"\n[theme]\ndanger = \"#112233\"\n",
        "theme.danger = \"#112233\"\n[theme.danger]\nvalue = 1\n",
        "[theme]\nsyntax.comment = \"first\"\n[theme.syntax]\ncomment = \"second\"\n",
        "[ui]\ntheme = \"dark\"\ntheme = \"light\"\n",
    ] {
        let ordinary = toml::from_str::<Config>(source).unwrap_err();
        let pending = toml::from_str::<PendingConfig>(source).unwrap_err();
        assert_eq!(pending.message(), ordinary.message());
        assert_eq!(pending.span(), ordinary.span());
        assert_eq!(pending.to_string(), ordinary.to_string());
    }
}

#[test]
fn pending_wrong_theme_values_do_not_mask_the_next_non_theme_error() {
    let error = toml::from_str::<PendingConfig>("theme = []\n[ui]\ntheme = false\nunknown = 1\n")
        .unwrap_err();
    assert!(error.message().contains("unknown field `unknown`"));
    assert_eq!(error.span(), Some(30..37));
    let error = toml::from_str::<PendingConfig>(
        "[ui]\ntheme = false\n[query]\nmax-buffered-rows = \"wrong\"\n",
    )
    .unwrap_err();
    assert!(error.message().contains("expected usize"));
}

#[test]
fn typed_alias_keeps_the_definition_for_the_shared_resolver() {
    let definition: Config = toml::from_str("[theme]\n").unwrap();
    assert!(definition.theme.is_some());
    assert!(toml::from_str::<Config>("[ui]\ntheme = false\n").is_err());
    let normal = Config {
        ui: UiConfig {
            theme: ThemeSelection::Light,
            ..UiConfig::default()
        },
        ..Config::default()
    };
    assert!(normal.theme.is_none());
    assert_eq!(normal.ui.theme, ThemeSelection::Light);
}
