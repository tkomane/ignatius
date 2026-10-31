use super::*;
use crate::config::ThemeChoice;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::layout::Presentation;
use crate::ui::theme::{ColorDepth, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn presentation(
    theme_choice: ThemeChoice,
    color: bool,
    tier: GlyphTier,
    depth: ColorDepth,
) -> Presentation {
    Presentation::new(
        Theme::new(theme_choice, color).with_depth(depth),
        Glyphs::new(tier),
        false,
    )
}

fn line_to_string(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn reduced_motion_starts_at_final_with_zero_animation_frames() {
    let state_reduced = BannerState::new(true);
    assert!(state_reduced.is_final());
    assert_eq!(state_reduced.tick(), BANNER_TOTAL_TICKS);

    let state_animated = BannerState::new(false);
    assert!(!state_animated.is_final());
    assert_eq!(state_animated.tick(), 0);
}

#[test]
fn advance_reaches_final_at_tick_12_and_finish_skips_immediately() {
    let mut state = BannerState::new(false);
    for expected_tick in 0..BANNER_TOTAL_TICKS {
        assert_eq!(state.tick(), expected_tick);
        assert!(!state.is_final());
        state.advance();
    }
    assert_eq!(state.tick(), BANNER_TOTAL_TICKS);
    assert!(state.is_final());

    // Advance once more - stays at final.
    state.advance();
    assert_eq!(state.tick(), BANNER_TOTAL_TICKS);
    assert!(state.is_final());

    // Finish skips immediately.
    let mut intermediate = BannerState::Revealing { tick: 3 };
    intermediate.finish();
    assert!(intermediate.is_final());
    assert_eq!(intermediate.tick(), BANNER_TOTAL_TICKS);
}

#[test]
fn front_col_eases_monotonically_to_width() {
    for width in [59, 48, 22, 8] {
        let mut prev = 0;
        for tick in 0..=BANNER_TOTAL_TICKS {
            let state = if tick == BANNER_TOTAL_TICKS {
                BannerState::Final
            } else {
                BannerState::Revealing { tick }
            };
            let front = front_col(width, state);
            assert!(
                front >= prev,
                "front_col must be monotonically non-decreasing: tick {tick}, prev {prev}, front {front}"
            );
            assert!(front <= width, "front_col must not exceed width {width}");
            prev = front;
        }
        assert_eq!(front_col(width, BannerState::Revealing { tick: 0 }), 0);
        assert_eq!(front_col(width, BannerState::Final), width);
    }

    // Exact pinned easing checks for tick 0, 6, 12 per T052.
    assert_eq!(front_col(48, BannerState::Revealing { tick: 0 }), 0);
    assert_eq!(front_col(48, BannerState::Revealing { tick: 6 }), 36);
    assert_eq!(front_col(48, BannerState::Final), 48);

    assert_eq!(front_col(59, BannerState::Revealing { tick: 0 }), 0);
    assert_eq!(front_col(59, BannerState::Revealing { tick: 6 }), 44);
    assert_eq!(front_col(59, BannerState::Final), 59);
}

#[test]
fn too_small_layout_suppresses_banner() {
    let p = presentation(
        ThemeChoice::Dark,
        true,
        GlyphTier::Unicode,
        ColorDepth::TrueColor,
    );
    assert!(render_banner_lines(BannerState::Final, &p, 39).is_empty());
    assert!(render_banner_lines(BannerState::Final, &p, 10).is_empty());

    let mut buf = Buffer::empty(Rect::new(0, 0, 39, 7));
    render_banner(
        BannerState::Final,
        Some("step"),
        &p,
        Rect::new(0, 0, 39, 7),
        &mut buf,
    );
    // Buffer should remain empty/untouched
    for cell in buf.content() {
        assert_eq!(cell.symbol(), " ");
    }

    let mut buf2 = Buffer::empty(Rect::new(0, 0, 80, 7));
    render_banner(
        BannerState::Final,
        Some("step"),
        &p,
        Rect::new(0, 0, 80, 7),
        &mut buf2,
    );
    for cell in buf2.content() {
        assert_eq!(cell.symbol(), " ");
    }
}

#[test]
fn wordmark_and_monogram_selection_across_tiers() {
    let p_uni = presentation(
        ThemeChoice::Dark,
        true,
        GlyphTier::Unicode,
        ColorDepth::TrueColor,
    );
    let p_ascii = presentation(
        ThemeChoice::Dark,
        true,
        GlyphTier::Ascii,
        ColorDepth::TrueColor,
    );

    // Wordmark >= 64
    let wm_uni = render_banner_lines(BannerState::Final, &p_uni, 64);
    assert_eq!(wm_uni.len(), 8);
    for (i, expected) in UNICODE_WORDMARK_LINES.iter().enumerate() {
        assert_eq!(&line_to_string(&wm_uni[i]), expected);
    }
    assert_eq!(&line_to_string(&wm_uni[6]), UNICODE_WORDMARK_UNDERLINE);
    assert_eq!(&line_to_string(&wm_uni[7]), UNICODE_WORDMARK_TAGLINE);

    let wm_ascii = render_banner_lines(BannerState::Final, &p_ascii, 64);
    assert_eq!(wm_ascii.len(), 6);
    for (i, expected) in ASCII_WORDMARK_LINES.iter().enumerate() {
        assert_eq!(&line_to_string(&wm_ascii[i]), expected);
    }
    assert_eq!(&line_to_string(&wm_ascii[4]), ASCII_WORDMARK_UNDERLINE);
    assert_eq!(&line_to_string(&wm_ascii[5]), ASCII_WORDMARK_TAGLINE);

    // Monogram < 64
    let mono_uni = render_banner_lines(BannerState::Final, &p_uni, 60);
    assert_eq!(mono_uni.len(), 6);
    for (i, expected) in UNICODE_MONOGRAM_LINES.iter().enumerate() {
        assert_eq!(&line_to_string(&mono_uni[i]), expected);
    }

    let mono_ascii = render_banner_lines(BannerState::Final, &p_ascii, 60);
    assert_eq!(mono_ascii.len(), 1);
    assert_eq!(&line_to_string(&mono_ascii[0]), ASCII_MONOGRAM);
}

#[test]
fn deterministic_sweep_decode_edge_underline_and_tagline() {
    let p_uni = presentation(
        ThemeChoice::Dark,
        true,
        GlyphTier::Unicode,
        ColorDepth::TrueColor,
    );
    const WIDTH: usize = 59;

    for tick in 0..BANNER_TOTAL_TICKS {
        let state = BannerState::Revealing { tick };
        let lines = render_banner_lines(state, &p_uni, 80);
        assert_eq!(lines.len(), 8);

        let front = front_col(WIDTH, state);

        // Letterforms
        for row in 0..6 {
            let art_chars: Vec<char> = UNICODE_WORDMARK_LINES[row].chars().collect();
            let rendered_chars: Vec<char> = line_to_string(&lines[row]).chars().collect();
            assert_eq!(rendered_chars.len(), WIDTH);

            for col in 0..WIDTH {
                let r_char = rendered_chars[col];
                let f_char = art_chars[col];

                if col + 3 < front {
                    // Left of decode edge: final art character
                    assert_eq!(
                        r_char, f_char,
                        "col {col} row {row} tick {tick} should be final"
                    );
                } else if col < front {
                    // Inside 3-column decode edge
                    if f_char != ' ' {
                        assert!(
                            UNICODE_DECODE_POOL.contains(&r_char),
                            "col {col} row {row} tick {tick} char '{r_char}' must come from unicode decode pool"
                        );
                        let expected = UNICODE_DECODE_POOL
                            [(col + row + usize::from(tick)) % UNICODE_DECODE_POOL.len()];
                        assert_eq!(r_char, expected);
                    } else {
                        assert_eq!(r_char, ' ');
                    }
                } else {
                    // Past front: blank
                    assert_eq!(
                        r_char, ' ',
                        "col {col} row {row} tick {tick} must be blank past front"
                    );
                }
            }
        }

        // Underline row (row 6)
        let underline_chars: Vec<char> = line_to_string(&lines[6]).chars().collect();
        let ul_len = UNICODE_WORDMARK_UNDERLINE.chars().count();
        assert_eq!(underline_chars.len(), ul_len);
        for (col, &ch) in underline_chars.iter().enumerate().take(ul_len) {
            if col < front {
                assert_eq!(ch, '▔');
            } else {
                assert_eq!(ch, ' ');
            }
        }

        // Tagline row (row 7) is blank during revealing
        let tagline_str = line_to_string(&lines[7]);
        assert!(
            tagline_str.trim().is_empty(),
            "tagline must be blank on tick {tick}"
        );
    }

    // On final frame, tagline appears
    let final_lines = render_banner_lines(BannerState::Final, &p_uni, 80);
    assert_eq!(&line_to_string(&final_lines[7]), UNICODE_WORDMARK_TAGLINE);

    // ASCII sweep test
    let p_ascii = presentation(
        ThemeChoice::Dark,
        true,
        GlyphTier::Ascii,
        ColorDepth::TrueColor,
    );
    for tick in 0..BANNER_TOTAL_TICKS {
        let state = BannerState::Revealing { tick };
        let lines = render_banner_lines(state, &p_ascii, 80);
        assert_eq!(lines.len(), 6);
        let front = front_col(48, state);

        for row in 0..4 {
            let art_chars: Vec<char> = ASCII_WORDMARK_LINES[row].chars().collect();
            let rendered_chars: Vec<char> = line_to_string(&lines[row]).chars().collect();

            for (col, &r_char) in rendered_chars.iter().enumerate() {
                let f_char = art_chars.get(col).copied().unwrap_or(' ');
                if col + 3 < front {
                    assert_eq!(r_char, f_char);
                } else if col < front {
                    if f_char != ' ' {
                        assert!(
                            ASCII_DECODE_POOL.contains(&r_char),
                            "col {col} row {row} tick {tick} char '{r_char}' must come from ascii decode pool"
                        );
                    } else {
                        assert_eq!(r_char, ' ');
                    }
                } else {
                    assert_eq!(r_char, ' ');
                }
            }
        }

        // Underline uses '-'
        let ul_chars: Vec<char> = line_to_string(&lines[4]).chars().collect();
        for (col, &ch) in ul_chars.iter().enumerate().take(48) {
            if col < front {
                assert_eq!(ch, '-');
            } else {
                assert_eq!(ch, ' ');
            }
        }

        let tagline_str = line_to_string(&lines[5]);
        assert!(tagline_str.trim().is_empty());
    }
}

#[test]
fn banner_ramp_styling_across_color_depths() {
    for depth in [
        ColorDepth::TrueColor,
        ColorDepth::Indexed256,
        ColorDepth::Basic16,
        ColorDepth::None,
    ] {
        let p = presentation(ThemeChoice::Dark, true, GlyphTier::Unicode, depth);
        let lines = render_banner_lines(BannerState::Final, &p, 80);
        let row0 = &lines[0];
        for span in &row0.spans {
            if span.content.trim().is_empty() {
                continue;
            }
            match depth {
                ColorDepth::TrueColor => {
                    assert!(matches!(span.style.fg, Some(Color::Rgb(..))));
                }
                ColorDepth::Indexed256 => {
                    if let Some(Color::Indexed(idx)) = span.style.fg {
                        assert!(idx >= 16);
                    } else {
                        panic!(
                            "Expected Indexed color for Indexed256, got {:?}",
                            span.style.fg
                        );
                    }
                }
                ColorDepth::Basic16 => {
                    if let Some(Color::Indexed(idx)) = span.style.fg {
                        assert!(idx <= 15);
                    } else {
                        panic!(
                            "Expected Indexed color <= 15 for Basic16, got {:?}",
                            span.style.fg
                        );
                    }
                }
                ColorDepth::None => {
                    assert_eq!(span.style.fg, None);
                }
            }
        }
    }
}

#[test]
fn step_line_rendered_centered_below_banner() {
    let p = presentation(
        ThemeChoice::Dark,
        true,
        GlyphTier::Unicode,
        ColorDepth::TrueColor,
    );
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);

    let step_text = "Connecting to localhost:5432...";
    render_banner(BannerState::Final, Some(step_text), &p, area, &mut buf);

    // Banner is 8 rows high: (24 - 8) / 2 = 8 start_y.
    // Step line should be at y = 8 + 8 = 16.
    let step_y = 16;
    let expected_x = (80 - step_text.len() as u16) / 2;

    let mut found_str = String::new();
    for x in expected_x..expected_x + step_text.len() as u16 {
        found_str.push_str(buf[(x, step_y)].symbol());
    }
    assert_eq!(found_str, step_text);
}

#[test]
fn plain_mode_and_machine_output_emit_zero_banner_bytes() {
    use crate::cli::output::{Format, OutputOptions, write_execution};
    use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
    use crate::query::value::Cell;
    use std::time::Duration;

    let help = crate::cli::plain::help_text();
    for forbidden in [
        UNICODE_WORDMARK_LINES[0],
        UNICODE_MONOGRAM_LINES[0],
        ASCII_WORDMARK_LINES[0],
        UNICODE_WORDMARK_UNDERLINE,
        ASCII_WORDMARK_UNDERLINE,
    ] {
        assert!(
            !help.contains(forbidden),
            "plain help must never contain banner art: {forbidden}"
        );
    }

    let mut set = ResultSet::new(vec!["id".into()], 10);
    set.push(vec![Cell::Text("1".into())]);
    let execution = Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(1),
            elapsed: Duration::from_millis(2),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(2),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    };

    for format in [
        Format::Table,
        Format::Csv,
        Format::Tsv,
        Format::Json,
        Format::Ndjson,
        Format::Markdown,
    ] {
        let mut out = Vec::new();
        write_execution(
            &mut out,
            &execution,
            &OutputOptions {
                format,
                ..OutputOptions::default()
            },
        )
        .expect("write machine output");
        let rendered = String::from_utf8(out).expect("utf-8 output");
        for forbidden in [
            UNICODE_WORDMARK_LINES[0],
            UNICODE_MONOGRAM_LINES[0],
            ASCII_WORDMARK_LINES[0],
            UNICODE_WORDMARK_UNDERLINE,
            ASCII_WORDMARK_UNDERLINE,
            ASCII_WORDMARK_TAGLINE,
        ] {
            assert!(
                !rendered.contains(forbidden),
                "machine output ({format:?}) must never contain banner bytes"
            );
        }
    }
}

#[test]
fn connecting_shell_at_sixty_four_and_eighty_columns_renders_wordmark_above_step_line() {
    use crate::app::model::{ConnectingStep, ConnectionState, Model};
    use crate::ui::keymap::Keymap;
    use crate::ui::layout::render_to_string;

    for width in [64u16, 80u16, 120u16] {
        let mut model = Model::new(100);
        model.connection = ConnectionState::Connecting {
            step: ConnectingStep::ServerHandshake,
            target: "app@localhost:5432/orders".into(),
        };
        model.banner = BannerState::Final;
        let p = presentation(
            ThemeChoice::Dark,
            true,
            GlyphTier::Unicode,
            ColorDepth::TrueColor,
        );
        let rendered = render_to_string(&model, &Keymap::new(), &p, width, 30);
        assert!(
            rendered.contains(UNICODE_WORDMARK_LINES[0]),
            "width {width} must render the 59-column unicode wordmark:\n{rendered}"
        );
        assert!(
            rendered.contains("the PostgreSQL client you can point at production"),
            "width {width} must render the tagline on Final:\n{rendered}"
        );
        assert!(
            rendered.contains("server handshake"),
            "width {width} must keep the connecting step line visible:\n{rendered}"
        );
    }
}
