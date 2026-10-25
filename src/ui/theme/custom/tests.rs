use super::*;
use ratatui::style::Modifier;

fn lower(text: &str) -> Result<(ThemeSelection, Option<ThemeDefinition>), ThemeRefusal> {
    let value: toml::Value = toml::from_str(text).expect("synthetic TOML fixture");
    lower_theme_inputs(
        value.get("ui").and_then(|ui| ui.get("theme")),
        value.get("theme"),
    )
}

#[test]
fn canonical_tokens_and_hex_are_exact_and_exhaustive() {
    assert_eq!(Token::ALL.len(), 24);
    for (i, token) in Token::ALL.iter().enumerate() {
        assert_eq!(token.index(), i);
        assert_eq!(Token::from_key(token.key()), Some(*token));
        assert!(Token::from_key(&token.key().to_uppercase()).is_none());
    }
    assert_eq!(Rgb::from_hex("#aBcD09"), Some(Rgb(0xab, 0xcd, 9)));
    assert_eq!(Rgb(0xab, 0xcd, 9).hex(), "#ABCD09");
    for bad in [
        "red",
        "#123",
        " #ABCDEF",
        "#ABCDEG",
        "#000000\n",
        "#１２３４５６",
        "1234567",
    ] {
        assert!(Rgb::from_hex(bad).is_none());
    }
}

#[test]
fn defaults_absence_empty_definition_and_inheritance_are_distinct() {
    assert_eq!(lower("").unwrap(), (ThemeSelection::Dark, None));
    let (selection, definition) = lower("[theme]\n").unwrap();
    assert_eq!(selection, ThemeSelection::Dark);
    let definition = definition.unwrap();
    assert_eq!(definition.base(), ThemeChoice::Dark);
    for token in Token::ALL {
        assert_eq!(definition.override_for(*token), None);
    }
    let catalog = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition)).unwrap();
    let active = catalog.selected_theme(true, ColorDepth::TrueColor);
    assert_eq!(active.selection(), ThemeSelection::Custom);
    for token in Token::ALL {
        assert_eq!(
            active.rgb(*token),
            Theme::new(ThemeChoice::Dark, true).rgb(*token)
        );
    }
}

#[test]
fn exact_refusal_examples_and_wrong_types_are_safe_lines() {
    for (input, expected) in [
        (
            "[theme]\ndanger2=\"#FFFFFF\"",
            "configuration error: [theme] has no token named \"danger2\". Token names are listed in docs/design/theme-tokens.md.",
        ),
        (
            "[theme]\ndanger=\"red\"",
            "configuration error: [theme] danger = \"red\" is not a colour. Use \"#RRGGBB\" hex, for example \"#F38BA8\".",
        ),
        (
            "[theme]\nextends=\"solarized\"",
            "configuration error: [theme] extends = \"solarized\" is not a built-in theme. Use \"dark\", \"light\" or \"high-contrast\".",
        ),
        (
            "[ui]\ntheme=\"custom\"",
            "configuration error: ui.theme = \"custom\" requires a [theme] table.",
        ),
        (
            "[ui]\ntheme=1",
            "configuration error: ui.theme must be a string. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ),
        ("theme=1", "configuration error: theme must be a table."),
        (
            "[theme]\nextends=1",
            "configuration error: [theme] extends must be a string. Use \"dark\", \"light\" or \"high-contrast\".",
        ),
        (
            "[theme]\ndanger=1",
            "configuration error: [theme] danger must be a colour string. Use \"#RRGGBB\" hex, for example \"#F38BA8\".",
        ),
    ] {
        let error = lower(input).unwrap_err();
        assert_eq!(error.line(), expected);
        assert_eq!(error.to_string(), expected);
        assert_eq!(error.line().lines().count(), 1);
    }
    assert_eq!(
        ThemeRefusal::contrast(Token::Muted, Token::SurfacePane, 2.19, 4.5).line(),
        "configuration error: [theme] muted on surface-pane is 2.1:1; this pair must reach 4.5:1."
    );
    assert!(
        ThemeRefusal::contrast(Token::Muted, Token::SurfacePane, 4.49999, 4.5)
            .line()
            .contains("is 4.4:1")
    );
}

#[test]
fn refusal_order_is_selection_then_shape_base_unknown_and_token_order() {
    assert!(
        lower("theme=1\n[ui]\ntheme=\"custom\"")
            .unwrap_err()
            .line()
            .contains("theme must be a table")
    );
    assert!(
        lower("theme=1\n[ui]\ntheme=\"auto\"")
            .unwrap_err()
            .line()
            .contains("not available in this build")
    );
    assert!(
        lower("[theme]\nextends=1\naaa=1")
            .unwrap_err()
            .line()
            .contains("extends must be a string")
    );
    assert!(
        lower("[theme]\nzzz=1\naaa=1\ntext=1")
            .unwrap_err()
            .line()
            .contains("\"aaa\"")
    );
    assert!(
        lower("[theme]\ndanger=1\ntext=1")
            .unwrap_err()
            .line()
            .contains("[theme] text must be")
    );
}

#[test]
fn reflected_input_is_redacted_then_escaped_and_bounded_without_split_units() {
    let hostile = "password=synthetic-private\n\r\t\u{1b}\u{202e}\"\\é";
    let safe = reflected(hostile);
    assert!(!safe.contains("synthetic-private"));
    assert!(safe.contains("[redacted]"));
    assert!(safe.is_ascii());
    assert!(!safe.chars().any(char::is_control));
    assert_eq!(
        reflected("\"\\\n\r\t\u{1b}\u{202e}é"),
        "\\\"\\\\\\n\\r\\t\\u{001B}\\u{202E}\\u{00E9}"
    );
    assert_eq!(reflected(&"a".repeat(80)), "a".repeat(80));
    assert_eq!(
        reflected(&format!("{}é", "a".repeat(79))),
        format!("{}... [truncated]", "a".repeat(79))
    );
}

#[test]
fn serde_uses_the_same_grammar_and_serializes_definition_not_cache() {
    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        selection: ThemeSelection,
        theme: ThemeDefinition,
    }
    let source = "selection=\"custom\"\n[theme]\nextends=\"light\"\ndanger=\"#A81812\"";
    let parsed: Wrapper = toml::from_str(source).unwrap();
    assert_eq!(parsed.selection, ThemeSelection::Custom);
    let serialized = toml::to_string(&parsed).unwrap();
    assert!(serialized.contains("extends = \"light\""));
    assert!(serialized.contains("danger = \"#A81812\""));
    assert!(!serialized.contains("surface"));
    let again: Wrapper = toml::from_str(&serialized).unwrap();
    assert_eq!(again.theme, parsed.theme);
    assert!(toml::from_str::<Wrapper>("selection=\"dark\"\n[theme]\ndanger=1").is_err());
}

#[test]
fn catalog_cycles_three_or_four_and_rejects_mismatched_custom_without_mutation() {
    let base = ValidatedThemes::default();
    let mut active = base.selected_theme(false, ColorDepth::Basic16);
    for expected in [
        ThemeSelection::Light,
        ThemeSelection::HighContrast,
        ThemeSelection::Dark,
    ] {
        active = base.cycle(&active).unwrap();
        assert_eq!(active.selection(), expected);
        assert!(!active.color);
        assert_eq!(active.depth, ColorDepth::Basic16);
    }
    let definition = ThemeDefinition::new(ThemeChoice::Dark);
    let catalog =
        ValidatedThemes::resolve(ThemeSelection::HighContrast, Some(&definition)).unwrap();
    let before = catalog.selected_theme(true, ColorDepth::Indexed256);
    let custom = catalog.cycle(&before).unwrap();
    assert_eq!(custom.selection(), ThemeSelection::Custom);
    assert!(base.cycle(&custom).is_err());
    assert_eq!(custom.selection(), ThemeSelection::Custom);
    assert_eq!(
        catalog.cycle(&custom).unwrap().selection(),
        ThemeSelection::Dark
    );
    let other = ValidatedThemes::resolve(
        ThemeSelection::Dark,
        Some(&definition.with_override(Token::Danger, Rgb(255, 160, 160))),
    )
    .unwrap();
    assert!(other.cycle(&custom).is_err());
}

#[test]
fn inactive_custom_is_fully_validated_and_normal_border_cannot_disappear() {
    let bad = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Text, Rgb(0, 0, 0));
    assert!(ValidatedThemes::resolve(ThemeSelection::Light, Some(&bad)).is_err());
    let dark = Theme::new(ThemeChoice::Dark, true);
    let border = ThemeDefinition::new(ThemeChoice::Dark)
        .with_override(Token::Border, dark.rgb(Token::Surface));
    assert_eq!(
        ValidatedThemes::resolve(ThemeSelection::Dark, Some(&border))
            .unwrap_err()
            .line(),
        "configuration error: [theme] border must differ from surface."
    );
}

#[test]
fn validated_custom_cannot_reintroduce_paint_under_any_colour_off_gate() {
    let definition = ThemeDefinition::new(ThemeChoice::Dark);
    let catalog = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition)).unwrap();
    for (color, depth) in [
        (false, ColorDepth::TrueColor),
        (false, ColorDepth::Indexed256),
        (true, ColorDepth::None),
    ] {
        let theme = catalog.selected_theme(color, depth);
        for token in Token::ALL {
            let style = theme.style(*token);
            assert!(style.fg.is_none() && style.bg.is_none());
            assert!(theme.color(*token).is_none());
        }
        assert!(
            theme
                .style(Token::Selection)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(theme.stripe().bg.is_none());
        assert!(theme.capsule(Token::EnvironmentProduction).bg.is_none());
    }
}

#[test]
fn every_override_lowers_in_canonical_slot_and_every_matrix_role_is_validated() {
    for token in Token::ALL {
        let mut table = toml::map::Map::new();
        table.insert(token.key().into(), toml::Value::String("#aBCd09".into()));
        let (_, definition) = lower_theme_inputs(None, Some(&toml::Value::Table(table))).unwrap();
        let definition = definition.unwrap();
        assert_eq!(definition.override_for(*token), Some(Rgb(0xab, 0xcd, 9)));
        for other in Token::ALL {
            if other != token {
                assert_eq!(definition.override_for(*other), None);
            }
        }
    }
    let backgrounds = [
        Token::Surface,
        Token::SurfaceAlt,
        Token::SurfacePane,
        Token::SurfaceOverlay,
    ];
    for base in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
    ] {
        let theme = Theme::new(base, true);
        for token in Token::ALL {
            let color = if backgrounds.contains(token) {
                theme.rgb(Token::Text)
            } else {
                theme.rgb(Token::Surface)
            };
            let definition = ThemeDefinition::new(base).with_override(*token, color);
            assert!(
                ValidatedThemes::resolve(ThemeSelection::Dark, Some(&definition)).is_err(),
                "{base:?}/{token:?} escaped full resolution"
            );
        }
    }
}

#[test]
fn selected_and_actual_production_fill_thresholds_do_not_preimplement_other_capsules() {
    let theme = Theme::new(ThemeChoice::Dark, true);
    let gray = (0..=255)
        .map(|v| Rgb(v, v, v))
        .find(|rgb| {
            [
                Token::Surface,
                Token::SurfaceAlt,
                Token::SurfacePane,
                Token::SurfaceOverlay,
            ]
            .iter()
            .all(|bg| rgb.contrast(theme.rgb(*bg)) >= 3.0)
                && rgb.contrast(theme.rgb(Token::Surface)) < 4.5
        })
        .expect("semantic-only contrasting gray");
    for (token, pair) in [
        (Token::Selection, "surface on selection"),
        (
            Token::EnvironmentProduction,
            "surface on environment-production",
        ),
    ] {
        let definition = ThemeDefinition::new(ThemeChoice::Dark).with_override(token, gray);
        let error = ValidatedThemes::resolve(ThemeSelection::Dark, Some(&definition)).unwrap_err();
        assert!(error.line().contains(pair));
        assert!(error.line().contains("must reach 4.5:1"));
    }
    let definition = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Info, gray);
    assert!(ValidatedThemes::resolve(ThemeSelection::Dark, Some(&definition)).is_ok());
}

#[test]
fn border_is_decorative_only_for_normal_bases_and_high_contrast_is_inherited() {
    let border = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Border, Rgb(0, 0, 0));
    assert!(ValidatedThemes::resolve(ThemeSelection::Dark, Some(&border)).is_ok());
    let border =
        ThemeDefinition::new(ThemeChoice::HighContrast).with_override(Token::Border, Rgb(0, 0, 0));
    assert!(
        ValidatedThemes::resolve(ThemeSelection::Dark, Some(&border))
            .unwrap_err()
            .line()
            .contains("must reach 7.0:1")
    );
    let muted = ThemeDefinition::new(ThemeChoice::HighContrast)
        .with_override(Token::Muted, Rgb(128, 128, 128));
    assert!(
        ValidatedThemes::resolve(ThemeSelection::Light, Some(&muted))
            .unwrap_err()
            .line()
            .contains("must reach 7.0:1")
    );
}

#[test]
fn hostile_but_valid_inverted_custom_resolves_all_tokens_once_and_survives_depths() {
    let mut definition = ThemeDefinition::new(ThemeChoice::HighContrast);
    for token in Token::ALL {
        let rgb = if [
            Token::Surface,
            Token::SurfaceAlt,
            Token::SurfacePane,
            Token::SurfaceOverlay,
        ]
        .contains(token)
        {
            Rgb(255, 255, 255)
        } else {
            Rgb(0, 0, 0)
        };
        definition = definition.with_override(*token, rgb);
    }
    let catalog = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition)).unwrap();
    for depth in [
        ColorDepth::TrueColor,
        ColorDepth::Indexed256,
        ColorDepth::Basic16,
        ColorDepth::None,
    ] {
        for color in [true, false] {
            let theme = catalog.selected_theme(color, depth);
            assert_eq!(theme.selection(), ThemeSelection::Custom);
            assert_eq!(theme.rgb(Token::Surface), Rgb(255, 255, 255));
            assert_eq!(theme.rgb(Token::Text), Rgb(0, 0, 0));
            let style = theme.style(Token::Selection);
            if !color || depth == ColorDepth::None {
                assert!(style.fg.is_none() && style.bg.is_none());
            }
            if depth == ColorDepth::Basic16 {
                assert!(style.bg.is_none());
                assert!(style.add_modifier.contains(Modifier::REVERSED));
            }
        }
    }
    definition = definition.with_override(Token::Text, Rgb(255, 255, 255));
    assert!(ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition)).is_err());
    assert_eq!(
        catalog
            .selected_theme(true, ColorDepth::TrueColor)
            .rgb(Token::Text),
        Rgb(0, 0, 0)
    );
}

#[test]
fn lower_grammar_does_not_trim_alias_or_echo_nonstrings_and_covers_all_wrong_shapes() {
    for value in [
        toml::Value::Integer(7),
        toml::Value::Float(1.5),
        toml::Value::Datetime("1979-05-27T07:32:00Z".parse().unwrap()),
        toml::Value::Boolean(true),
        toml::Value::Array(Vec::new()),
        toml::Value::Table(toml::map::Map::new()),
    ] {
        assert_eq!(
            lower_theme_inputs(Some(&value), None).unwrap_err().line(),
            "configuration error: ui.theme must be a string. Use \"dark\", \"light\", \"high-contrast\" or \"custom\"."
        );
    }
    for bad in ["DARK", " dark", "dark ", "high_contrast", ""] {
        assert!(
            lower_theme_inputs(Some(&toml::Value::String(bad.into())), None)
                .unwrap_err()
                .line()
                .contains("is not a theme")
        );
    }
    for base in ["auto", "custom", "Dark", " dark"] {
        let mut table = toml::map::Map::new();
        table.insert("extends".into(), toml::Value::String(base.into()));
        assert!(
            lower_theme_inputs(None, Some(&toml::Value::Table(table)))
                .unwrap_err()
                .line()
                .contains("is not a built-in theme")
        );
    }
}

#[test]
fn reflected_controls_and_unicode_are_complete_ascii_escapes_at_budget_edges() {
    for ch in (0u32..=0x9f)
        .filter_map(char::from_u32)
        .filter(|ch| ch.is_control())
    {
        let value = reflected(&ch.to_string());
        assert!(value.is_ascii());
        assert!(!value.chars().any(char::is_control));
        assert!(value.starts_with('\\'));
    }
    for ch in [
        '\u{200e}',
        '\u{200f}',
        '\u{202a}',
        '\u{202e}',
        '\u{2066}',
        '\u{2069}',
        '\u{1f600}',
    ] {
        assert_eq!(
            reflected(&ch.to_string()),
            format!("\\u{{{:04X}}}", ch as u32)
        );
    }
    assert_eq!(
        reflected(&format!("{}é", "a".repeat(72))),
        format!("{}\\u{{00E9}}", "a".repeat(72))
    );
    assert_eq!(
        reflected(&format!("{}éx", "a".repeat(72))),
        format!("{}\\u{{00E9}}... [truncated]", "a".repeat(72))
    );
}

#[test]
fn selection_refusals_have_the_exact_manual_unknown_and_auto_lines() {
    for (selection, expected) in [
        (
            "other",
            "configuration error: ui.theme = \"other\" is not a theme. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ),
        (
            "auto",
            "configuration error: ui.theme = \"auto\" is not available in this build. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ),
    ] {
        assert_eq!(
            lower_theme_inputs(Some(&toml::Value::String(selection.into())), None)
                .unwrap_err()
                .line(),
            expected
        );
    }
}

#[test]
fn every_reflection_route_uses_the_existing_redactor_before_safe_formatting() {
    let secret = ["synthetic", "credential", "fixture"].join("-");
    for input in [
        format!("postgres://user:{secret}@invalid/db"),
        format!("password={secret}"),
        format!("Authorization: Bearer {secret}"),
        format!("Bearer {secret}"),
    ] {
        let value = toml::Value::String(input.clone());
        let selection = lower_theme_inputs(Some(&value), None).unwrap_err();
        let mut key = toml::map::Map::new();
        key.insert(input.clone(), toml::Value::Integer(1));
        let unknown = lower_theme_inputs(None, Some(&toml::Value::Table(key))).unwrap_err();
        let mut base = toml::map::Map::new();
        base.insert("extends".into(), value.clone());
        let base = lower_theme_inputs(None, Some(&toml::Value::Table(base))).unwrap_err();
        let mut color = toml::map::Map::new();
        color.insert("danger".into(), value);
        let color = lower_theme_inputs(None, Some(&toml::Value::Table(color))).unwrap_err();
        for error in [selection, unknown, base, color] {
            assert!(
                !error.line().contains(&secret),
                "refusal leaked a synthetic credential"
            );
            assert!(error.line().contains("[redacted]"));
            assert!(error.line().is_ascii());
            assert!(!error.line().chars().any(char::is_control));
        }
    }
}
