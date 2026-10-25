//! Synthetic Buffer acceptance, not native-terminal or physical contrast proof.

use crate::app::model::{ConnectionState, Focus, Model, QueryPhase};
use crate::app::{Action, Message, update};
use crate::config::{ColorDepthChoice, ColorMode, Density};
use crate::connection::Environment;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::{SessionInfo, TlsState};
use crate::query::result::{
    Execution, ExecutionStatus, JobId, NoticeReport, ResultSet, StatementResult, TransactionState,
};
use crate::query::value::Cell;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Token, ValidatedThemes, lower_theme_inputs};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{BufWriter, Write};
use std::time::Duration;

const AREA: Rect = Rect::new(0, 0, 120, 40);
const SCENES: [&str; 7] = [
    "prod-grid",
    "expanded-null",
    "expanded-empty",
    "expanded-literal-null",
    "error-recovery",
    "help",
    "palette",
];

fn palettes() -> Vec<(&'static str, ValidatedThemes)> {
    let mut inverted =
        "[ui]\ntheme = \"custom\"\n[theme]\nextends = \"high-contrast\"\n".to_owned();
    for token in Token::ALL {
        let background = matches!(
            token,
            Token::Surface | Token::SurfaceAlt | Token::SurfacePane | Token::SurfaceOverlay
        );
        inverted.push_str(&format!(
            "{} = \"{}\"\n",
            token.key(),
            if background { "#FFFFFF" } else { "#000000" }
        ));
    }
    [
        ("dark", "[ui]\ntheme = \"dark\"".to_owned()),
        ("light", "[ui]\ntheme = \"light\"".to_owned()),
        (
            "high-contrast",
            "[ui]\ntheme = \"high-contrast\"".to_owned(),
        ),
        (
            "inherited-custom",
            "[ui]\ntheme = \"custom\"\n[theme]\nextends = \"dark\"".to_owned(),
        ),
        ("inverted-custom", inverted),
    ]
    .into_iter()
    .map(|(name, text)| {
        let value: toml::Value = toml::from_str(&text).unwrap();
        let (selection, definition) = lower_theme_inputs(
            value.get("ui").and_then(|ui| ui.get("theme")),
            value.get("theme"),
        )
        .unwrap();
        (
            name,
            ValidatedThemes::resolve(selection, definition.as_ref()).unwrap(),
        )
    })
    .collect()
}

fn facts(no_color: bool) -> crate::diagnostics::doctor::TerminalFacts {
    crate::diagnostics::doctor::TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color,
        colorterm: Some("truecolor".into()),
        size: Some((AREA.width, AREA.height)),
        unicode: true,
        color_depth: ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
    }
}

fn model(presentation: Presentation, catalog: &ValidatedThemes, scene: &str) -> Model {
    let mut model = Model::new(100);
    model.presentation = presentation;
    model.theme_catalog = catalog.clone();
    model.size = (AREA.width, AREA.height);
    model.focus = Focus::Results;
    model.sidebar_visible = false;
    model.keymap_snapshot = Keymap::new().snapshot();
    model.connection = ConnectionState::Connected(Box::new(SessionInfo {
        target: "reader@localhost:5432/fixture".into(),
        database: "fixture".into(),
        user: "reader".into(),
        server_version: "18.6".into(),
        backend_pid: "42".into(),
        search_path: "public".into(),
        read_only: false,
        tls: TlsState::Disabled,
        environment: Environment::Production,
    }));
    model.editor.set_text(
        "-- Synthetic presentation fixture\nSELECT * FROM synthetic_values WHERE note = 'NULL';\n",
    );
    let mut set = ResultSet::new(
        ["sql_null", "empty_text", "literal_null", "plain_text"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        100,
    );
    assert!(set.push(vec![
        Cell::Null,
        Cell::Text(String::new()),
        Cell::Text("NULL".into()),
        Cell::Text("value".into()),
    ]));
    model.phase = QueryPhase::Running {
        job: JobId(1),
        statements: 1,
    };
    let failed = scene == "error-recovery";
    update(
        &mut model,
        Message::ExecutionFinished(Box::new(Execution {
            notice_report: NoticeReport::default(),
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(1),
                elapsed: Duration::from_millis(12),
                notices: Vec::new(),
            }],
            status: if failed {
                ExecutionStatus::Failed
            } else {
                ExecutionStatus::Succeeded
            },
            elapsed: Duration::from_millis(12),
            error: failed.then(|| {
                Diagnostic::new(
                    DiagnosticKind::Query,
                    "current transaction is aborted",
                    "running the synthetic statement",
                )
                .likely_cause("an earlier statement failed")
                .next_action("run ROLLBACK to end it")
            }),
            transaction: if failed {
                TransactionState::Failed
            } else {
                TransactionState::Open
            },
        })),
    );
    if scene.starts_with("expanded-") {
        model.expanded_row = true;
        model.selected_column = match scene {
            "expanded-null" => 0,
            "expanded-empty" => 1,
            "expanded-literal-null" => 2,
            _ => unreachable!(),
        };
    }
    if scene == "help" {
        assert!(update(&mut model, Message::Action(Action::ToggleHelp)).is_empty());
    } else if scene == "palette" {
        assert!(update(&mut model, Message::Action(Action::OpenPalette)).is_empty());
        model.palette.as_mut().unwrap().query = "Switch theme".into();
    }
    model
}

fn render(model: &Model) -> Buffer {
    let mut buf = Buffer::empty(AREA);
    crate::ui::render(model, &Keymap::new(), AREA, &mut buf);
    buf
}

fn field(buf: &Buffer, name: &str) -> (u16, u16) {
    for y in AREA.y..AREA.bottom() {
        let mut line = String::new();
        let mut starts = Vec::with_capacity(usize::from(AREA.width));
        for x in AREA.x..AREA.right() {
            starts.push((line.len(), x));
            line.push_str(buf[(x, y)].symbol());
        }
        if let Some(byte) = line.find(name)
            && let Some((_, x)) = starts.into_iter().find(|(start, _)| *start == byte)
        {
            return (x, y);
        }
    }
    panic!("missing field {name}: {}", buffer_to_string(buf));
}

fn assert_scene(buf: &Buffer, model: &Model, scene: &str) {
    let text = buffer_to_string(buf);
    assert!(text.contains("PROD"), "production words missing: {scene}");
    let (x, y) = field(buf, "[PROD]");
    let capsule = model
        .presentation
        .theme
        .capsule(Token::EnvironmentProduction);
    if let Some(fg) = capsule.fg {
        assert_eq!(buf[(x, y)].fg, fg, "production capsule ink");
    }
    if let Some(bg) = capsule.bg {
        assert_eq!(buf[(x, y)].bg, bg, "production capsule fill");
    }
    assert!(buf[(x, y)].modifier.contains(capsule.add_modifier));
    if scene.starts_with("expanded-") {
        for name in ["sql_null", "empty_text", "literal_null", "plain_text"] {
            field(buf, name);
        }
        let row = |name: &str| {
            let (_, y) = field(buf, name);
            (0..AREA.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row("sql_null").contains("[null]"));
        assert!(!row("empty_text").contains("NULL"));
        assert!(!row("empty_text").contains("[null]"));
        assert!(row("literal_null").contains("NULL"));
        assert!(!row("literal_null").contains("[null]"));
        let name = match scene {
            "expanded-null" => "sql_null",
            "expanded-empty" => "empty_text",
            "expanded-literal-null" => "literal_null",
            _ => unreachable!(),
        };
        let (x, y) = field(buf, name);
        let selected = &buf[(x, y)];
        let selection = model.presentation.theme.style(Token::Selection);
        let header = model.presentation.theme.style(Token::Header);
        assert_eq!(
            selected.fg,
            selection.fg.or(header.fg).unwrap_or(Color::Reset)
        );
        assert!(selected.modifier.contains(header.add_modifier));
        assert!(selected.modifier.contains(selection.add_modifier));
        if let Some(bg) = selection.bg {
            assert_eq!(selected.bg, bg);
        }
        if scene != "expanded-empty" {
            let value = if scene == "expanded-null" {
                "[null]"
            } else {
                "NULL"
            };
            let value_x = (x..AREA.right())
                .find(|value_x| {
                    (*value_x..AREA.right())
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                        .starts_with(value)
                })
                .unwrap();
            let actual = &buf[(value_x, y)];
            let semantic = model.presentation.theme.style(if scene == "expanded-null" {
                Token::NullValue
            } else {
                Token::Text
            });
            assert_eq!(
                actual.fg,
                selection.fg.or(semantic.fg).unwrap_or(Color::Reset)
            );
            assert!(actual.modifier.contains(semantic.add_modifier));
            assert!(actual.modifier.contains(selection.add_modifier));
        }
    } else if scene == "error-recovery" {
        for words in [
            "Transaction failed",
            "current transaction is aborted",
            "ROLLBACK",
        ] {
            assert!(text.contains(words), "missing {words}: {text}");
        }
    } else if scene == "palette" {
        assert!(text.contains("Switch theme"), "{text}");
        let (_, y) = field(buf, "Session");
        let selection = model.presentation.theme.style(Token::Selection);
        for (word, token) in [
            ("Session", Token::Muted),
            ("Switch theme", Token::Text),
            ("Cycle theme", Token::Muted),
        ] {
            let x = (0..AREA.width)
                .find(|x| {
                    (*x..AREA.width)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                        .starts_with(word)
                })
                .unwrap();
            let semantic = if token == Token::Text {
                // The selected label already uses Selection, including its
                // deliberate Reset foreground at Basic16 and colour off.
                selection
            } else {
                model.presentation.theme.style(token)
            };
            let actual = &buf[(x, y)];
            assert_eq!(
                actual.fg,
                selection.fg.or(semantic.fg).unwrap_or(Color::Reset),
                "selected palette {word} ink"
            );
            if let Some(bg) = selection.bg {
                assert_eq!(actual.bg, bg, "selected palette {word} fill");
            }
            assert!(actual.modifier.contains(semantic.add_modifier));
            assert!(actual.modifier.contains(selection.add_modifier));
        }
    } else if scene == "help" {
        assert!(text.contains("Help"), "{text}");
    } else {
        assert!(
            text.contains("literal_null") && text.contains("NULL"),
            "{text}"
        );
    }
}

fn capture(writer: &mut impl Write, buf: &Buffer, metadata: serde_json::Value) {
    let mut runs = Vec::new();
    for y in 0..AREA.height {
        let mut x = 0;
        while x < AREA.width {
            let first = &buf[(x, y)];
            let start = x;
            let mut symbols = Vec::new();
            while x < AREA.width {
                let cell = &buf[(x, y)];
                if cell.fg != first.fg || cell.bg != first.bg || cell.modifier != first.modifier {
                    break;
                }
                symbols.push(cell.symbol().to_owned());
                x += 1;
            }
            runs.push(json!({"x":start,"y":y,"fg":format!("{:?}",first.fg),"bg":format!("{:?}",first.bg),"modifiers":first.modifier.bits(),"modifier_names":format!("{:?}",first.modifier),"symbols":symbols}));
        }
    }
    serde_json::to_writer(&mut *writer, &json!({"kind":"SyntheticRatatuiBuffer","width":AREA.width,"height":AREA.height,"metadata":metadata,"runs":runs})).unwrap();
    writeln!(writer).unwrap();
}

#[test]
fn manual_theme_actual_buffers_preserve_words_geometry_selection_and_colour_off() {
    let capture_path = std::env::var_os("IGNATIUS_THEME_CAPTURE_DIR").map(std::path::PathBuf::from);
    let mut writer = capture_path.map(|path| {
        std::fs::create_dir_all(&path).unwrap();
        BufWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.join("buffers.jsonl"))
                .unwrap(),
        )
    });
    let mut expected = BTreeMap::<String, Vec<String>>::new();
    let mut frames = 0;
    for (palette, catalog) in palettes() {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Indexed256,
            ColorDepth::Basic16,
            ColorDepth::None,
        ] {
            for glyphs in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
                for density in [Density::Comfortable, Density::Compact] {
                    for gate in ["colour-on", "colour-never", "no-color-facts"] {
                        let color = crate::ui::terminal::should_use_color(
                            if gate == "colour-never" {
                                ColorMode::Never
                            } else {
                                ColorMode::Always
                            },
                            &facts(gate == "no-color-facts"),
                        );
                        let presentation = Presentation::new(
                            catalog.selected_theme(color, depth),
                            Glyphs::new(glyphs),
                            true,
                        )
                        .with_density(density);
                        for scene in SCENES {
                            let model = model(presentation.clone(), &catalog, scene);
                            let buf = render(&model);
                            assert_scene(&buf, &model, scene);
                            let symbols: Vec<String> = buf
                                .content
                                .iter()
                                .map(|cell| cell.symbol().to_owned())
                                .collect();
                            let key = format!("{glyphs:?}/{density:?}/{scene}");
                            let original = expected.entry(key).or_insert_with(|| symbols.clone());
                            assert_eq!(
                                &symbols, original,
                                "palette/depth/colour changed actual content or geometry: {palette}/{depth:?}/{gate}"
                            );
                            if !color || depth == ColorDepth::None {
                                assert!(
                                    buf.content
                                        .iter()
                                        .all(|cell| cell.fg == Color::Reset
                                            && cell.bg == Color::Reset)
                                );
                            }
                            if depth == ColorDepth::Basic16 {
                                assert!(buf.content.iter().all(|cell| cell.bg == Color::Reset));
                            }
                            if glyphs == GlyphTier::Ascii {
                                assert!(buffer_to_string(&buf).is_ascii());
                            }
                            if let Some(writer) = writer.as_mut() {
                                capture(
                                    writer,
                                    &buf,
                                    json!({"palette":palette,"depth":format!("{depth:?}"),"glyphs":format!("{glyphs:?}"),"density":format!("{density:?}"),"gate":gate,"scene":scene,"resolved_color":color}),
                                );
                            }
                            frames += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(frames, 2520);
    if let Some(mut writer) = writer {
        writer.flush().unwrap();
    }
}

#[test]
fn manual_theme_no_color_facts_win_over_all_real_depth_flags_and_custom_palettes() {
    for (_, catalog) in palettes() {
        for flag in [
            ColorDepthChoice::TrueColor,
            ColorDepthChoice::Indexed256,
            ColorDepthChoice::Basic16,
        ] {
            let facts = facts(true);
            let color = crate::ui::terminal::should_use_color(ColorMode::Always, &facts);
            let (depth, _) =
                crate::cli::resolve_depth(Some(flag), ColorDepthChoice::Auto, color, &facts);
            assert!(!color);
            assert_eq!(depth, ColorDepth::None);
            let theme = catalog.selected_theme(color, depth);
            for token in Token::ALL {
                let style = theme.style(*token);
                assert!(style.fg.is_none() && style.bg.is_none());
            }
            assert!(
                theme
                    .style(Token::Selection)
                    .add_modifier
                    .contains(Modifier::REVERSED)
            );
        }
    }
}
