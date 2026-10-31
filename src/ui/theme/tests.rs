use super::*;

const READABLE: f64 = 4.5;
const LARGE_OR_SECONDARY: f64 = 3.0;
const HIGH_CONTRAST_MINIMUM: f64 = 7.0;

fn themes() -> [Theme; 3] {
    [
        Theme::new(ThemeChoice::Dark, true),
        Theme::new(ThemeChoice::Light, true),
        Theme::new(ThemeChoice::HighContrast, true),
    ]
}

/// The four painted backgrounds: base, stripe, pane and overlay. Text and
/// semantic colours have to stay readable on every one of them.
fn surfaces(theme: &Theme) -> [Rgb; 4] {
    [
        theme.rgb(Token::Surface),
        theme.rgb(Token::SurfaceAlt),
        theme.rgb(Token::SurfacePane),
        theme.rgb(Token::SurfaceOverlay),
    ]
}

const SURFACE_TOKENS: [Token; 4] = [
    Token::Surface,
    Token::SurfaceAlt,
    Token::SurfacePane,
    Token::SurfaceOverlay,
];

#[test]
fn every_theme_defines_every_token() {
    for theme in themes() {
        for token in Token::ALL {
            let rgb = theme.rgb(*token);
            // A surface token resolving to another surface would collapse an
            // elevation level; the other tokens must never equal the base.
            if !SURFACE_TOKENS.contains(token) {
                assert_ne!(
                    rgb,
                    theme.background(),
                    "{:?}/{token:?} is invisible against its own background",
                    theme.selection()
                );
            }
        }
    }
}

#[test]
fn primary_text_is_readable_on_its_surface() {
    for theme in themes() {
        for surface in surfaces(&theme) {
            let ratio = theme.rgb(Token::Text).contrast(surface);
            assert!(
                ratio >= READABLE,
                "{:?}: text contrast {ratio:.2} is below {READABLE}",
                theme.selection()
            );
        }
    }
}

#[test]
fn semantic_states_stay_legible_on_their_surface() {
    for theme in themes() {
        for surface in surfaces(&theme) {
            for token in [
                Token::Muted,
                Token::Success,
                Token::Warning,
                Token::Danger,
                Token::Info,
                Token::EnvironmentProduction,
                Token::TransactionFailed,
                Token::NullValue,
                Token::Header,
            ] {
                let ratio = theme.rgb(token).contrast(surface);
                assert!(
                    ratio >= LARGE_OR_SECONDARY,
                    "{:?}/{token:?} contrast {ratio:.2} is below {LARGE_OR_SECONDARY}",
                    theme.selection()
                );
            }
        }
    }
}

#[test]
fn text_stays_readable_on_a_striped_row() {
    // Striping alternates the background, so every contrast guarantee has to
    // hold against the alternate surface as well as the main one.
    for theme in themes() {
        let stripe = theme.rgb(Token::SurfaceAlt);
        assert!(
            theme.rgb(Token::Text).contrast(stripe) >= READABLE,
            "{:?}: text on a striped row is {:.2}",
            theme.selection(),
            theme.rgb(Token::Text).contrast(stripe)
        );
        for token in [
            Token::Muted,
            Token::NullValue,
            Token::Danger,
            Token::Success,
        ] {
            assert!(
                theme.rgb(token).contrast(stripe) >= LARGE_OR_SECONDARY,
                "{:?}/{token:?} on a striped row is {:.2}",
                theme.selection(),
                theme.rgb(token).contrast(stripe)
            );
        }
    }
}

#[test]
fn a_capsule_is_legible_and_falls_back_to_reversed_text() {
    for theme in themes() {
        let style = theme.capsule(Token::EnvironmentProduction);
        assert!(
            style.bg.is_some(),
            "{:?}: a capsule needs a fill",
            theme.selection()
        );
        assert!(style.fg.is_some());
        // The pair is token against surface, which the palette tests already
        // hold to a threshold; assert it here so the capsule cannot drift.
        let ratio = theme
            .rgb(Token::EnvironmentProduction)
            .contrast(theme.rgb(Token::Surface));
        assert!(
            ratio >= LARGE_OR_SECONDARY,
            "{:?}: capsule contrast {ratio:.2}",
            theme.selection()
        );
    }
    let plain = Theme::new(ThemeChoice::Dark, false);
    assert!(plain.capsule(Token::EnvironmentProduction).bg.is_none());
    assert!(
        plain
            .capsule(Token::EnvironmentProduction)
            .add_modifier
            .contains(Modifier::REVERSED),
        "without colour a capsule must still stand out"
    );
    assert!(
        plain.stripe().bg.is_none(),
        "striping is dropped, not faked"
    );
}

#[test]
fn the_high_contrast_theme_actually_earns_its_name() {
    let theme = Theme::new(ThemeChoice::HighContrast, true);
    for surface in surfaces(&theme) {
        for token in Token::ALL {
            if SURFACE_TOKENS.contains(token) {
                continue;
            }
            let ratio = theme.rgb(*token).contrast(surface);
            assert!(
                ratio >= HIGH_CONTRAST_MINIMUM,
                "{token:?} contrast {ratio:.2} is below {HIGH_CONTRAST_MINIMUM}"
            );
        }
    }
}

#[test]
fn the_light_theme_is_light_and_the_dark_theme_is_dark() {
    assert!(
        Theme::new(ThemeChoice::Light, true)
            .background()
            .luminance()
            > 0.7
    );
    assert!(Theme::new(ThemeChoice::Dark, true).background().luminance() < 0.05);
}

#[test]
fn disabling_colour_emits_no_colour_but_keeps_emphasis() {
    let theme = Theme::new(ThemeChoice::Dark, false);
    for token in Token::ALL {
        let style = theme.style(*token);
        assert!(style.fg.is_none(), "{token:?} emitted a foreground colour");
        assert!(style.bg.is_none(), "{token:?} emitted a background colour");
    }
    assert!(
        theme
            .style(Token::Focus)
            .add_modifier
            .contains(Modifier::BOLD),
        "focus must stay visible without colour"
    );
    assert!(
        theme
            .style(Token::Selection)
            .add_modifier
            .contains(Modifier::REVERSED),
        "selection must stay visible without colour"
    );
}

#[test]
fn contrast_maths_matches_known_values() {
    // Black on white is the WCAG maximum of 21:1.
    let ratio = Rgb(0, 0, 0).contrast(Rgb(255, 255, 255));
    assert!((ratio - 21.0).abs() < 0.01, "{ratio}");
    // A colour against itself is 1:1.
    assert!((Rgb(18, 21, 26).contrast(Rgb(18, 21, 26)) - 1.0).abs() < f64::EPSILON);
}

// ---- Colour depth: the quantizer ----------------------------------------

#[test]
fn the_depth_defaults_to_truecolor_and_the_builder_narrows_it() {
    assert_eq!(
        Theme::new(ThemeChoice::Dark, true).depth,
        ColorDepth::TrueColor
    );
    assert_eq!(
        Theme::new(ThemeChoice::Dark, true)
            .with_depth(ColorDepth::Basic16)
            .depth,
        ColorDepth::Basic16
    );
}

#[test]
fn quantizer_hits_exact_cube_candidates() {
    assert_eq!(quantize_256(Rgb(0, 0, 0)), 16);
    assert_eq!(quantize_256(Rgb(255, 255, 255)), 231);
    assert_eq!(quantize_256(Rgb(255, 0, 0)), 16 + 36 * 5);
    assert_eq!(quantize_256(Rgb(0, 95, 0)), 16 + 6);
}

#[test]
fn quantizer_hits_the_grey_ramp_when_that_is_nearest() {
    // 128 is an exact grey-ramp value: 8 + 12 * 10, index 232 + 12.
    assert_eq!(quantize_256(Rgb(128, 128, 128)), 244);
    // 100 is closer to the ramp's 98 than to the cube's 95.
    assert_eq!(quantize_256(Rgb(100, 100, 100)), 241);
    assert_eq!(quantize_256(Rgb(230, 233, 239)), 255);
}

#[test]
fn quantizer_ties_resolve_to_the_lower_index() {
    // 235 is exactly between the cube levels 215 (index 160) and 255 (196).
    assert_eq!(quantize_256(Rgb(235, 0, 0)), 160);
    // 13 is exactly between the grey levels 8 (index 232) and 18 (233).
    assert_eq!(quantize_256(Rgb(13, 13, 13)), 232);
    // 4 is closer to the cube's black (16) than to the nearest grey (232).
    assert_eq!(quantize_256(Rgb(4, 4, 4)), 16);
    // 175 is an exact cube level; the grey ramp is further away.
    assert_eq!(quantize_256(Rgb(175, 175, 175)), 16 + 36 * 3 + 6 * 3 + 3);
}

#[test]
fn quantizer_never_produces_a_redefinable_low_index() {
    for r in (0..=255).step_by(17) {
        for g in (0..=255).step_by(17) {
            for b in (0..=255).step_by(17) {
                let index = quantize_256(Rgb(r, g, b));
                assert!(
                    (16..=255).contains(&index),
                    "Rgb({r}, {g}, {b}) produced index {index}"
                );
            }
        }
    }
}

// ---- Colour depth: the fixed 16-colour table -----------------------------

/// The contract's family per token, written out by hand: `(normal, bright)`.
fn contract_family(token: Token) -> (u8, Option<u8>) {
    match token {
        Token::Danger | Token::EnvironmentProduction | Token::TransactionFailed => (1, Some(9)),
        Token::Success | Token::SyntaxLiteral => (2, Some(10)),
        Token::Warning | Token::TransactionActive | Token::SyntaxNumber => (3, Some(11)),
        Token::Focus | Token::Selection => (4, Some(12)),
        Token::SyntaxKeyword => (5, Some(13)),
        Token::Info | Token::SyntaxIdentifier => (6, Some(14)),
        Token::Text | Token::Header | Token::Border => (7, Some(15)),
        Token::Muted
        | Token::EnvironmentNonProduction
        | Token::NullValue
        | Token::SyntaxComment
        | Token::Surface
        | Token::SurfaceAlt
        | Token::SurfacePane
        | Token::SurfaceOverlay => (8, None),
    }
}

#[test]
fn basic_16_uses_the_contract_family_and_the_luminance_rule() {
    for theme in themes() {
        for token in Token::ALL {
            let (normal, bright) = contract_family(*token);
            let expected = match bright {
                Some(bright) if theme.rgb(*token).luminance() >= 0.5 => bright,
                _ => normal,
            };
            assert_eq!(
                basic_16(*token, &theme),
                expected,
                "{:?}/{token:?}",
                theme.selection()
            );
        }
    }
}

#[test]
fn basic_16_matches_hand_written_values() {
    let dark = Theme::new(ThemeChoice::Dark, true);
    let light = Theme::new(ThemeChoice::Light, true);
    let high = Theme::new(ThemeChoice::HighContrast, true);
    // text/grey: bright when the palette's text is light, normal when dark.
    assert_eq!(basic_16(Token::Text, &dark), 15);
    assert_eq!(basic_16(Token::Text, &light), 7);
    // red stays normal on all three palettes.
    assert_eq!(basic_16(Token::Danger, &dark), 1);
    assert_eq!(basic_16(Token::Danger, &light), 1);
    assert_eq!(basic_16(Token::Danger, &high), 1);
    // green and yellow are bright on the dark palette.
    assert_eq!(basic_16(Token::Success, &dark), 10);
    assert_eq!(basic_16(Token::Warning, &dark), 11);
    // mauve focus is normal at dark, bright on the high-contrast palette.
    assert_eq!(basic_16(Token::Focus, &dark), 4);
    assert_eq!(basic_16(Token::Focus, &high), 12);
    // muted never brightens.
    assert_eq!(basic_16(Token::Muted, &dark), 8);
    assert_eq!(basic_16(Token::Muted, &high), 8);
    // border is dark grey at dark and bright on the light palette.
    assert_eq!(basic_16(Token::Border, &dark), 7);
    assert_eq!(basic_16(Token::Border, &light), 15);
    // surface tokens were never foregrounds and always take muted.
    assert_eq!(basic_16(Token::SurfacePane, &light), 8);
}

// ---- Colour depth: depth-aware emission ----------------------------------

#[test]
fn truecolor_keeps_rgb_emission() {
    let theme = Theme::new(ThemeChoice::Dark, true);
    for token in Token::ALL {
        let style = theme.style(*token);
        if *token == Token::Selection {
            assert_eq!(style.bg, Some(theme.rgb(Token::Selection).into()));
            assert_eq!(style.fg, Some(theme.rgb(Token::Surface).into()));
        } else {
            assert_eq!(style.fg, Some(theme.rgb(*token).into()), "{token:?}");
            assert!(style.bg.is_none(), "{token:?}");
        }
    }
}

#[test]
fn indexed_256_emits_indexed_colours_from_the_quantizer() {
    let theme = Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::Indexed256);
    assert_eq!(theme.style(Token::Text).fg, Some(Color::Indexed(255)));
    assert_eq!(theme.style(Token::Danger).fg, Some(Color::Indexed(210)));
    let selection = theme.style(Token::Selection);
    assert_eq!(
        selection.bg,
        Some(Color::Indexed(quantize_256(theme.rgb(Token::Selection))))
    );
    assert_eq!(
        selection.fg,
        Some(Color::Indexed(quantize_256(theme.rgb(Token::Surface))))
    );
    assert_eq!(
        theme.surface(Token::SurfacePane).bg,
        Some(Color::Indexed(235))
    );
}

#[test]
fn basic_16_emits_family_foregrounds_and_paints_no_surface() {
    for choice in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
    ] {
        let theme = Theme::new(choice, true).with_depth(ColorDepth::Basic16);
        for token in Token::ALL {
            let style = theme.style(*token);
            assert!(
                style.bg.is_none(),
                "{choice:?}/{token:?} painted a background"
            );
            if *token == Token::Selection {
                assert!(style.fg.is_none(), "{choice:?}: selection needs reversed");
                assert!(style.add_modifier.contains(Modifier::REVERSED));
            } else {
                match style.fg {
                    Some(Color::Indexed(index)) => assert!(
                        (1..=15).contains(&index),
                        "{choice:?}/{token:?} emitted index {index}"
                    ),
                    other => panic!("{choice:?}/{token:?} emitted {other:?}"),
                }
            }
        }
        assert!(theme.stripe().bg.is_none(), "{choice:?}: stripe painted");
        assert!(theme.on_stripe(Token::Text).bg.is_none());
        for token in SURFACE_TOKENS {
            assert!(theme.style(token).bg.is_none(), "{choice:?}/{token:?}");
            assert!(theme.surface(token).bg.is_none(), "{choice:?}/{token:?}");
        }
        assert!(
            theme.capsule(Token::EnvironmentProduction).bg.is_none(),
            "{choice:?}: a capsule painted a surface"
        );
        assert!(
            theme
                .capsule(Token::EnvironmentProduction)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }
}

#[test]
fn depth_none_emits_no_colour_and_keeps_the_modifier_rules() {
    let theme = Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::None);
    for token in Token::ALL {
        let style = theme.style(*token);
        assert!(style.fg.is_none(), "{token:?} emitted a foreground");
        assert!(style.bg.is_none(), "{token:?} emitted a background");
        assert!(
            theme.color(*token).is_none(),
            "{token:?} converted a colour"
        );
    }
    assert!(
        theme
            .style(Token::Focus)
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert!(
        theme
            .style(Token::Selection)
            .add_modifier
            .contains(Modifier::REVERSED)
    );
    assert!(theme.stripe().bg.is_none());
    assert!(theme.capsule(Token::Danger).bg.is_none());
    assert!(theme.surface(Token::SurfacePane).bg.is_none());
}

fn manual_text_pair(choice: ThemeChoice, token: Token) {
    let theme = Theme::new(choice, true);
    for background in SURFACE_TOKENS {
        let ratio = theme.rgb(token).contrast(theme.rgb(background));
        assert!(
            ratio >= 4.5,
            "{choice:?}/{token:?} on {background:?}: {ratio:.8}:1, requires 4.5:1"
        );
    }
}

#[test]
fn manual_dark_null_text_clears_four_surfaces() {
    manual_text_pair(ThemeChoice::Dark, Token::NullValue);
}

#[test]
fn manual_dark_comment_text_clears_four_surfaces() {
    manual_text_pair(ThemeChoice::Dark, Token::SyntaxComment);
}

#[test]
fn manual_light_null_text_clears_four_surfaces() {
    manual_text_pair(ThemeChoice::Light, Token::NullValue);
}

#[test]
fn manual_light_comment_text_clears_four_surfaces() {
    manual_text_pair(ThemeChoice::Light, Token::SyntaxComment);
}

#[test]
fn manual_complete_builtin_matrix_clears_the_contract() {
    let text = [
        Token::Text,
        Token::Muted,
        Token::Header,
        Token::NullValue,
        Token::SyntaxKeyword,
        Token::SyntaxLiteral,
        Token::SyntaxNumber,
        Token::SyntaxComment,
        Token::SyntaxIdentifier,
    ];
    let semantic = [
        Token::Focus,
        Token::Success,
        Token::Warning,
        Token::Danger,
        Token::Info,
        Token::EnvironmentProduction,
        Token::EnvironmentNonProduction,
        Token::TransactionActive,
        Token::TransactionFailed,
    ];
    let mut failures = Vec::new();
    for theme in themes() {
        for token in Token::ALL {
            if SURFACE_TOKENS.contains(token) {
                continue;
            }
            let threshold = if theme.selection() == ThemeSelection::HighContrast {
                7.0
            } else if text.contains(token) {
                4.5
            } else if semantic.contains(token) {
                3.0
            } else {
                continue;
            };
            for background in SURFACE_TOKENS {
                let ratio = theme.rgb(*token).contrast(theme.rgb(background));
                if ratio < threshold {
                    failures.push(format!(
                        "{:?}/{token:?}/{background:?} {ratio:.8} < {threshold:.1}",
                        theme.selection()
                    ));
                }
            }
        }
        assert_ne!(theme.rgb(Token::Border), theme.rgb(Token::Surface));
        for fill in [Token::Selection, Token::EnvironmentProduction] {
            let threshold = if theme.selection() == ThemeSelection::HighContrast {
                7.0
            } else {
                4.5
            };
            let ratio = theme.rgb(Token::Surface).contrast(theme.rgb(fill));
            if ratio < threshold {
                failures.push(format!(
                    "{:?}/Surface/{fill:?} {ratio:.8} < {threshold:.1}",
                    theme.selection()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "resolved matrix failures: {failures:#?}"
    );
}

#[test]
fn banner_anchors_and_ramp_columns_clear_contrast_thresholds() {
    for theme in themes() {
        let (anchor_a, anchor_b) = theme.banner_anchors();
        let threshold = if theme.selection() == ThemeSelection::HighContrast {
            HIGH_CONTRAST_MINIMUM
        } else {
            READABLE
        };

        // Both anchor endpoints must meet the threshold against all surfaces.
        for surface in surfaces(&theme) {
            let ratio_a = anchor_a.contrast(surface);
            assert!(
                ratio_a >= threshold,
                "{:?}: banner anchor_a {:?} contrast {:.2} below {threshold} against {surface:?}",
                theme.selection(),
                anchor_a,
                ratio_a
            );
            let ratio_b = anchor_b.contrast(surface);
            assert!(
                ratio_b >= threshold,
                "{:?}: banner anchor_b {:?} contrast {:.2} below {threshold} against {surface:?}",
                theme.selection(),
                anchor_b,
                ratio_b
            );
        }

        // Every interpolated column across a 59-column ramp must clear the threshold.
        const WIDTH: usize = 59;
        for col in 0..WIDTH {
            let col_rgb = theme.banner_column_rgb(col, WIDTH);
            for surface in surfaces(&theme) {
                let ratio = col_rgb.contrast(surface);
                assert!(
                    ratio >= threshold,
                    "{:?}: banner col {col}/{WIDTH} {:?} contrast {:.2} below {threshold} against {surface:?}",
                    theme.selection(),
                    col_rgb,
                    ratio
                );
            }
        }
    }
}

#[test]
fn banner_ramp_honours_color_depths_and_disabled_color() {
    const WIDTH: usize = 59;
    let base_theme = Theme::new(ThemeChoice::Dark, true);

    // TrueColor emits Color::Rgb matching banner_column_rgb.
    let truecolor_theme = base_theme.clone().with_depth(ColorDepth::TrueColor);
    let ramp_tc = truecolor_theme.banner_ramp(WIDTH);
    assert_eq!(ramp_tc.len(), WIDTH);
    for (col, style) in ramp_tc.iter().enumerate() {
        let expected_rgb = truecolor_theme.banner_column_rgb(col, WIDTH);
        assert_eq!(style.fg, Some(expected_rgb.into()));
    }

    // Indexed256 emits Color::Indexed in 16..=255.
    let indexed_theme = base_theme.clone().with_depth(ColorDepth::Indexed256);
    let ramp_idx = indexed_theme.banner_ramp(WIDTH);
    assert_eq!(ramp_idx.len(), WIDTH);
    for (col, style) in ramp_idx.iter().enumerate() {
        let expected_rgb = indexed_theme.banner_column_rgb(col, WIDTH);
        let expected_idx = quantize_256(expected_rgb);
        assert_eq!(style.fg, Some(Color::Indexed(expected_idx)));
        assert!(expected_idx >= 16);
    }

    // Basic16 emits two flat bands in 0..=15.
    let basic_theme = base_theme.clone().with_depth(ColorDepth::Basic16);
    let ramp_16 = basic_theme.banner_ramp(WIDTH);
    assert_eq!(ramp_16.len(), WIDTH);
    let left_expected = ramp_16[0].fg;
    let right_expected = ramp_16[WIDTH - 1].fg;
    assert_ne!(left_expected, right_expected);
    for (col, style) in ramp_16.iter().enumerate() {
        if col < WIDTH / 2 {
            assert_eq!(style.fg, left_expected);
        } else {
            assert_eq!(style.fg, right_expected);
        }
        if let Some(Color::Indexed(idx)) = style.fg {
            assert!(idx <= 15);
        } else {
            panic!("Expected Color::Indexed for Basic16, got {:?}", style.fg);
        }
    }

    // ColorDepth::None emits plain text (no fg).
    let none_theme = base_theme.clone().with_depth(ColorDepth::None);
    let ramp_none = none_theme.banner_ramp(WIDTH);
    assert_eq!(ramp_none.len(), WIDTH);
    for style in &ramp_none {
        assert_eq!(style.fg, None);
        assert_eq!(style.bg, None);
    }

    // color = false emits plain text (no fg).
    let no_color_theme = Theme::new(ThemeChoice::Dark, false);
    let ramp_no_color = no_color_theme.banner_ramp(WIDTH);
    assert_eq!(ramp_no_color.len(), WIDTH);
    for style in &ramp_no_color {
        assert_eq!(style.fg, None);
        assert_eq!(style.bg, None);
    }
}
