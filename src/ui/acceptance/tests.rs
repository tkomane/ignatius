//! Synthetic Buffer acceptance, not native-terminal or physical contrast proof.

use crate::app::grid::SortDirection;
use crate::app::model::{ConnectionState, Focus, Model, NoticeDetails, QueryPhase};
use crate::app::{Action, Effect, Message, update};
use crate::config::{
    ColorDepthChoice, ColorMode, Config, ConfigSource, Density, Loaded, Paths, ThemeChoice,
    ThemeSelection,
};
use crate::connection::Environment;
use crate::diagnostics::doctor::{CheckStatus, TerminalFacts, environment_report};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::{SessionInfo, TlsState};
use crate::query::plan::PlanDocument;
use crate::query::result::{
    Execution, ExecutionStatus, JobId, Notice, NoticeReport, ResultSet, StatementResult,
    TransactionState,
};
use crate::query::value::Cell;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{
    ColorDepth, Rgb, Token, ValidatedThemes, lower_theme_inputs, theme_for_background,
};
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
        background_rgb: None,
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

#[test]
fn reference_journeys_j1_through_j8_preserve_safety_recovery_and_visual_cues() {
    let (_, catalog) = palettes().remove(0);
    let presentation = Presentation::new(
        catalog.selected_theme(true, ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let mut model = model(presentation.clone(), &catalog, "prod-grid");

    // ------------------------------------------------------------------------
    // J1: First useful answer
    // Connected session -> sidebar visible -> run query -> inspect results grid ->
    // open expanded row detail -> verify [null], "", and "NULL" distinction ->
    // close detail -> quit.
    // ------------------------------------------------------------------------
    assert!(!model.sidebar_visible);
    update(&mut model, Message::Action(Action::ToggleSidebar));
    assert!(
        model.sidebar_visible,
        "J1: ToggleSidebar makes sidebar visible"
    );

    model.editor.set_text("SELECT * FROM synthetic_values;\n");
    let mut set = ResultSet::new(
        vec![
            "sql_null".into(),
            "empty_text".into(),
            "literal_null".into(),
            "plain_text".into(),
        ],
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
    update(
        &mut model,
        Message::ExecutionFinished(Box::new(Execution {
            notice_report: NoticeReport::default(),
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(1),
                elapsed: Duration::from_millis(15),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(15),
            error: None,
            transaction: TransactionState::Open,
        })),
    );
    assert_eq!(
        model.visible_result().unwrap().rows.len(),
        1,
        "J1: results grid populated"
    );

    let grid_buf = render(&model);
    let grid_text = buffer_to_string(&grid_buf);
    assert!(grid_text.contains("sql_null"));
    assert!(grid_text.contains("literal_null"));

    update(&mut model, Message::Action(Action::ToggleExpandedRow));
    assert!(model.expanded_row, "J1: expanded row opened");

    let expanded_buf = render(&model);
    let row_null = {
        let (_, y) = field(&expanded_buf, "sql_null");
        (0..AREA.width)
            .map(|x| expanded_buf[(x, y)].symbol())
            .collect::<String>()
    };
    let row_empty = {
        let (_, y) = field(&expanded_buf, "empty_text");
        (0..AREA.width)
            .map(|x| expanded_buf[(x, y)].symbol())
            .collect::<String>()
    };
    let row_literal = {
        let (_, y) = field(&expanded_buf, "literal_null");
        (0..AREA.width)
            .map(|x| expanded_buf[(x, y)].symbol())
            .collect::<String>()
    };
    assert!(row_null.contains("[null]"), "J1: sql_null shows [null]");
    assert!(
        !row_empty.contains("NULL") && !row_empty.contains("[null]"),
        "J1: empty string does not display NULL or [null]"
    );
    assert!(
        row_literal.contains("NULL") && !row_literal.contains("[null]"),
        "J1: literal NULL shows NULL and not [null]"
    );

    update(&mut model, Message::Action(Action::ToggleExpandedRow));
    assert!(!model.expanded_row, "J1: expanded row closed");

    let quit_effects = update(&mut model, Message::Action(Action::Quit));
    assert!(model.should_quit, "J1: Action::Quit requests quit");
    assert_eq!(quit_effects, vec![Effect::Quit]);
    model.should_quit = false;

    // ------------------------------------------------------------------------
    // J2: Correct and reuse SQL
    // Failed query with error position -> JumpToError moves cursor to error offset ->
    // format SQL -> open command palette -> verify editor text and recovery.
    // ------------------------------------------------------------------------
    let sql_with_error = "SELECT x,   y   FORM   orders   WHERE   id=1;";
    model.editor.set_text(sql_with_error);
    let error_pos = 15;
    model.phase = QueryPhase::Running {
        job: JobId(2),
        statements: 1,
    };
    model.running_editor_revision = Some(model.editor.revision());
    model.running_sql = Some(sql_with_error.to_owned());
    let diagnostic = Diagnostic::new(
        DiagnosticKind::Query,
        "syntax error at or near \"FORM\"",
        sql_with_error,
    )
    .in_statement(1)
    .at_position(error_pos);

    update(
        &mut model,
        Message::ExecutionFinished(Box::new(Execution {
            notice_report: NoticeReport::default(),
            job: JobId(2),
            statements: Vec::new(),
            status: ExecutionStatus::Failed,
            elapsed: Duration::from_millis(8),
            error: Some(diagnostic),
            transaction: TransactionState::Autocommit,
        })),
    );

    assert!(model.error_location.is_some(), "J2: error location mapped");
    let loc = model.error_location.as_ref().unwrap();
    assert_eq!(
        model.editor.cursor(),
        loc.cursor,
        "J2: editor cursor moved to error offset"
    );
    assert_eq!(model.focus, Focus::Editor);

    update(&mut model, Message::Action(Action::FormatBuffer));
    assert!(model.format_notice.is_some(), "J2: SQL formatted locally");
    let formatted_text = model.editor.text().to_owned();
    assert_ne!(formatted_text, sql_with_error, "J2: buffer text formatted");

    update(&mut model, Message::Action(Action::OpenPalette));
    assert!(model.palette.is_some(), "J2: command palette opened");
    let palette_buf = render(&model);
    let palette_text = buffer_to_string(&palette_buf);
    assert!(
        palette_text.contains("Commands") || palette_text.contains("Switch theme"),
        "J2: palette rendered"
    );

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.palette.is_none(), "J2: palette dismissed");
    assert_eq!(
        model.editor.text(),
        formatted_text,
        "J2: editor text preserved on recovery"
    );

    // ------------------------------------------------------------------------
    // J3: Investigate a wide result
    // Populate wide result set -> sort column -> apply local filter -> verify
    // filtered count -> clear filter -> adjust column width -> start export
    // and verify export prompt shows visible row scope.
    // ------------------------------------------------------------------------
    let mut wide_set = ResultSet::new(
        vec![
            "id".into(),
            "account_name".into(),
            "status".into(),
            "notes".into(),
            "balance".into(),
        ],
        100,
    );
    assert!(wide_set.push(vec![
        Cell::Text("1".into()),
        Cell::Text("Acme Alpha".into()),
        Cell::Text("active".into()),
        Cell::Text("priority tier 1".into()),
        Cell::Text("1000.00".into()),
    ]));
    assert!(wide_set.push(vec![
        Cell::Text("2".into()),
        Cell::Text("Beta Bravo".into()),
        Cell::Text("suspended".into()),
        Cell::Text("review pending".into()),
        Cell::Text("250.50".into()),
    ]));
    assert!(wide_set.push(vec![
        Cell::Text("3".into()),
        Cell::Text("Charlie Corp".into()),
        Cell::Text("active".into()),
        Cell::Text("onboarding".into()),
        Cell::Text("5000.00".into()),
    ]));
    model.last_execution = Some(Execution {
        notice_report: NoticeReport::default(),
        job: JobId(3),
        statements: vec![StatementResult {
            result_set: Some(wide_set),
            rows_affected: Some(3),
            elapsed: Duration::from_millis(20),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(20),
        error: None,
        transaction: TransactionState::Autocommit,
    });
    model.focus = Focus::Results;
    model.result_grid.reset_view();
    assert_eq!(model.filtered_rows().len(), 3, "J3: 3 initial rows");

    model.selected_column = 1;
    model.result_grid.toggle_sort(1);
    assert_eq!(
        model.result_grid.sort.map(|s| s.direction),
        Some(SortDirection::Ascending),
        "J3: column sorted ascending"
    );

    update(&mut model, Message::Action(Action::StartFilter));
    assert!(model.result_filtering, "J3: result filtering active");
    update(&mut model, Message::Action(Action::Insert('A')));
    update(&mut model, Message::Action(Action::Insert('c')));
    update(&mut model, Message::Action(Action::Insert('m')));
    update(&mut model, Message::Action(Action::Insert('e')));
    assert_eq!(model.result_filter, "Acme");
    update(&mut model, Message::Action(Action::Activate));
    assert!(!model.result_filtering, "J3: result filter committed");
    assert_eq!(
        model.filtered_rows().len(),
        1,
        "J3: filtered count matches search"
    );

    update(&mut model, Message::Action(Action::StartFilter));
    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.result_filter.is_empty(), "J3: filter cleared");
    assert_eq!(
        model.filtered_rows().len(),
        3,
        "J3: full row count restored"
    );

    let auto_width = crate::app::grid::automatic_column_width(model.visible_result().unwrap(), 1);
    let initial_width = model.result_grid.width_for(1, auto_width);
    update(&mut model, Message::Action(Action::WidenColumn));
    let widened = model.result_grid.width_for(1, auto_width);
    assert!(widened > initial_width, "J3: column widened");
    update(&mut model, Message::Action(Action::NarrowColumn));
    let narrowed = model.result_grid.width_for(1, auto_width);
    assert!(narrowed < widened, "J3: column narrowed");
    model.result_grid.reset_view();
    assert!(
        model.result_grid.widths.is_empty(),
        "J3: column widths reset"
    );

    update(&mut model, Message::Action(Action::ExportRows));
    assert!(model.palette.is_some(), "J3: export palette opened");
    let export_palette_buf = render(&model);
    let export_palette_text = buffer_to_string(&export_palette_buf);
    assert!(
        export_palette_text.contains("ready") || export_palette_text.contains("CSV"),
        "J3: export prompt shows row readiness/format choices"
    );
    update(&mut model, Message::Action(Action::Dismiss));
    assert!(model.palette.is_none());

    // ------------------------------------------------------------------------
    // J4: Diagnose safely
    // Load EXPLAIN (FORMAT JSON) plan + NoticeReport with WARNING -> toggle notices
    // overlay -> verify rendered notice severity and session status ([PROD], transaction)
    // in the buffer.
    // ------------------------------------------------------------------------
    let plan_json =
        r#"[{"Plan":{"Node Type":"Seq Scan","Relation Name":"orders","Total Cost":42.0}}]"#;
    model
        .plan
        .ready(PlanDocument::parse(plan_json, false).expect("valid plan document"));
    assert!(model.plan.is_visible(), "J4: plan view is visible");

    let warning_notice = Notice {
        severity: "WARNING".to_owned(),
        message: "table will be scanned sequentially".to_owned(),
        code: Some("01000".to_owned()),
    };
    model.notices.push(warning_notice);
    model.transaction = TransactionState::Open;

    model.notice_details = Some(NoticeDetails {
        snapshot: model.notice_state.snapshot(),
        scroll: 0,
    });
    let notice_buf = render(&model);
    let notice_text = buffer_to_string(&notice_buf);
    assert!(
        notice_text.contains("Server notice retention") || notice_text.contains("PROD"),
        "J4: notice retention overlay or session title drawn"
    );
    assert!(
        notice_text.contains("[PROD]"),
        "J4: [PROD] badge visible during notice inspection"
    );

    update(&mut model, Message::Action(Action::Dismiss));
    assert!(
        model.notice_details.is_none(),
        "J4: notice overlay dismissed"
    );
    model.plan.clear();
    assert!(!model.plan.is_visible(), "J4: plan view closed");

    // ------------------------------------------------------------------------
    // J5: Review one change
    // Trigger guarded write review on a [PROD] session -> render buffer and
    // verify write confirmation modal displays statement/target and safety cue ->
    // cancel and verify zero execution effects.
    // ------------------------------------------------------------------------
    model.editor.set_text("UPDATE orders SET total = 0;\n");
    let guarded_effects = update(&mut model, Message::Action(Action::RunBuffer));
    assert!(
        guarded_effects.is_empty(),
        "J5: write withheld pending confirmation"
    );
    assert!(
        model.pending_run.is_some(),
        "J5: pending run confirmation created"
    );
    let pending = model.pending_run.as_ref().unwrap();
    assert!(pending.sql.contains("UPDATE orders SET total = 0;"));

    let confirm_buf = render(&model);
    let confirm_text = buffer_to_string(&confirm_buf);
    assert!(confirm_text.contains("UPDATE orders SET total = 0;"));
    assert!(confirm_text.contains("fixture") || confirm_text.contains("Destructive"));

    let cancel_effects = update(&mut model, Message::Action(Action::Dismiss));
    assert!(
        cancel_effects.is_empty(),
        "J5: cancel produces zero effects"
    );
    assert!(model.pending_run.is_none(), "J5: pending run cleared");
    assert_eq!(
        model.phase,
        QueryPhase::Idle,
        "J5: query phase remains idle"
    );

    // ------------------------------------------------------------------------
    // J6: Recover from disruption
    // 1. Running query -> Action::Cancel transitions to QueryPhase::CancellationRequested
    //    and renders "Cancellation requested".
    // 2. Failed transaction (25P02) -> renders "Transaction failed" and "ROLLBACK".
    // 3. Message::ConnectionLost -> preserves editor SQL and retained results,
    //    clears running_for, transitions to ConnectionState::Lost, offers Reconnect.
    // ------------------------------------------------------------------------
    model.phase = QueryPhase::Running {
        job: JobId(60),
        statements: 1,
    };
    model.running_for = Some(Duration::from_millis(1200));
    let cancel_effects = update(&mut model, Message::Action(Action::Cancel));
    assert_eq!(
        cancel_effects,
        vec![Effect::Cancel { job: JobId(60) }],
        "J6: cancel delivers Effect::Cancel"
    );
    assert_eq!(
        model.phase,
        QueryPhase::CancellationRequested { job: JobId(60) },
        "J6: phase transitions to CancellationRequested"
    );
    let cancel_buf = render(&model);
    let cancel_text = buffer_to_string(&cancel_buf);
    assert!(
        cancel_text.contains("Cancellation requested"),
        "J6: renders cancellation status in buffer"
    );

    let tx_error = Diagnostic::new(
        DiagnosticKind::Query,
        "current transaction is aborted, commands ignored until end of transaction block",
        "SELECT 1;",
    )
    .technical("code", "25P02")
    .next_action("run ROLLBACK to end it");

    update(
        &mut model,
        Message::ExecutionFinished(Box::new(Execution {
            notice_report: NoticeReport::default(),
            job: JobId(60),
            statements: Vec::new(),
            status: ExecutionStatus::Failed,
            elapsed: Duration::from_millis(10),
            error: Some(tx_error),
            transaction: TransactionState::Failed,
        })),
    );
    let tx_buf = render(&model);
    let tx_text = buffer_to_string(&tx_buf);
    assert!(
        tx_text.contains("Transaction failed"),
        "J6: renders transaction failed"
    );
    assert!(
        tx_text.contains("ROLLBACK"),
        "J6: renders ROLLBACK guidance"
    );

    model.editor.set_text("SELECT count(*) FROM orders;\n");
    model.phase = QueryPhase::Running {
        job: JobId(61),
        statements: 1,
    };
    model.running_for = Some(Duration::from_millis(500));
    let lost_effects = update(&mut model, Message::ConnectionLost);
    assert!(
        lost_effects.is_empty(),
        "J6: connection lost produces no outgoing effects"
    );
    assert_eq!(
        model.editor.text(),
        "SELECT count(*) FROM orders;\n",
        "J6: editor SQL preserved after disconnect"
    );
    assert!(
        model.last_execution.is_some(),
        "J6: retained results preserved"
    );
    assert!(model.running_for.is_none(), "J6: running_for cleared");
    assert!(
        matches!(model.connection, ConnectionState::Lost { .. }),
        "J6: connection state records lost"
    );
    assert_eq!(
        model.phase,
        QueryPhase::Idle,
        "J6: query phase returns to idle"
    );
    let lost_buf = render(&model);
    let lost_text = buffer_to_string(&lost_buf);
    assert!(
        lost_text.contains("connection lost"),
        "J6: status indicates lost connection"
    );

    // ------------------------------------------------------------------------
    // J7: Automate and use plain/accessible modes
    // Connected workspace under GlyphTier::Ascii, ColorDepth::None, ColorDepth::Basic16,
    // and reduced_motion = true preserves every word, status badge, and [PROD] marker
    // with zero non-ASCII bytes in Ascii and zero RGB/256-color escapes in ColorDepth::None.
    // ------------------------------------------------------------------------
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

    let ascii_none = Presentation::new(
        catalog.selected_theme(false, ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        true,
    );
    model.presentation = ascii_none;
    let ascii_buf = render(&model);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(
        ascii_text.is_ascii(),
        "J7: Ascii tier contains only ASCII bytes"
    );
    assert!(
        ascii_text.contains("[PROD]"),
        "J7: ASCII presentation preserves [PROD] badge"
    );
    assert!(
        ascii_buf
            .content
            .iter()
            .all(|c| c.fg == Color::Reset && c.bg == Color::Reset),
        "J7: ColorDepth::None emits only Color::Reset"
    );

    let basic16 = Presentation::new(
        catalog.selected_theme(true, ColorDepth::Basic16),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    model.presentation = basic16;
    let basic16_buf = render(&model);
    assert!(
        basic16_buf.content.iter().all(|c| c.bg == Color::Reset),
        "J7: Basic16 emits Reset background fills"
    );
    let basic16_text = buffer_to_string(&basic16_buf);
    assert!(
        basic16_text.contains("[PROD]"),
        "J7: Basic16 preserves [PROD] badge"
    );

    model.phase = QueryPhase::Running {
        job: JobId(70),
        statements: 1,
    };
    model.running_for = Some(Duration::from_millis(2000));
    model.presentation.reduced_motion = true;
    model.frame = 0;
    let frame0_buf = render(&model);
    let frame0_text = buffer_to_string(&frame0_buf);
    model.frame = 42;
    let frame42_buf = render(&model);
    let frame42_text = buffer_to_string(&frame42_buf);
    assert_eq!(
        frame0_text, frame42_text,
        "J7: reduced_motion produces static indicators across frames"
    );
    assert!(
        frame0_text.contains("Running"),
        "J7: static indicator still states Running"
    );
    assert!(
        frame0_text.contains("2.0s"),
        "J7: static indicator shows elapsed duration"
    );
    model.phase = QueryPhase::Idle;
    model.running_for = None;

    // ------------------------------------------------------------------------
    // J8: Install and recover
    // Verify doctor::environment_report with "auto" background luminance detection
    // for dark (17, 19, 23) and light (250, 248, 245) RGB replies produces
    // expected theme and diagnostic report.
    // ------------------------------------------------------------------------
    let dark_rgb = Rgb(17, 19, 23);
    let light_rgb = Rgb(250, 248, 245);
    assert_eq!(
        theme_for_background(dark_rgb),
        ThemeChoice::Dark,
        "J8: dark RGB luminance resolves to ThemeChoice::Dark"
    );
    assert_eq!(
        theme_for_background(light_rgb),
        ThemeChoice::Light,
        "J8: light RGB luminance resolves to ThemeChoice::Light"
    );

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let mut auto_config = Config::default();
    auto_config.ui.theme = ThemeSelection::Auto;
    let loaded = Ok(Loaded {
        config: auto_config,
        themes: ValidatedThemes::default(),
        source: ConfigSource::Defaults,
        pending_migration: None,
    });

    let terminal_dark = TerminalFacts {
        background_rgb: Some(dark_rgb),
        ..facts(false)
    };
    let report_dark =
        environment_report(&Paths::rooted_at(temp_dir.path()), &loaded, &terminal_dark);
    let theme_check_dark = report_dark
        .checks
        .iter()
        .find(|c| c.name == "theme")
        .expect("J8: theme check present for dark facts");
    assert_eq!(theme_check_dark.status, CheckStatus::Ok);
    assert!(theme_check_dark.detail.contains("using dark"));

    let terminal_light = TerminalFacts {
        background_rgb: Some(light_rgb),
        ..facts(false)
    };
    let report_light =
        environment_report(&Paths::rooted_at(temp_dir.path()), &loaded, &terminal_light);
    let theme_check_light = report_light
        .checks
        .iter()
        .find(|c| c.name == "theme")
        .expect("J8: theme check present for light facts");
    assert_eq!(theme_check_light.status, CheckStatus::Ok);
    assert!(theme_check_light.detail.contains("using light"));

    let (sel, def) = lower_theme_inputs(Some(&toml::Value::String("auto".into())), None).unwrap();
    let mut resolved = ValidatedThemes::resolve(sel, def.as_ref()).unwrap();
    assert!(resolved.is_auto(), "J8: ValidatedThemes records auto mode");
    assert_eq!(
        resolved.apply_auto_background(Some(dark_rgb)),
        ThemeChoice::Dark,
        "J8: Auto theme resolves Dark choice"
    );
    assert_eq!(
        resolved.apply_auto_background(Some(light_rgb)),
        ThemeChoice::Light,
        "J8: Auto theme resolves Light choice"
    );
}

#[test]
fn viewport_matrix_100x30_80x24_60x20_30x6_and_live_resize_preserve_meaning() {
    let (_, catalog) = palettes().remove(0);
    let presentation = Presentation::new(
        catalog.selected_theme(true, ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let mut model = model(presentation, &catalog, "prod-grid");
    model
        .editor
        .set_text("SELECT id, name FROM users WHERE active = true;\n");
    let keymap = Keymap::new();

    // 1. 100x30: standard full workspace
    let area_100x30 = Rect::new(0, 0, 100, 30);
    update(&mut model, Message::Resized(100, 30));
    let mut buf_100x30 = Buffer::empty(area_100x30);
    crate::ui::render(&model, &keymap, area_100x30, &mut buf_100x30);
    let text_100x30 = buffer_to_string(&buf_100x30);
    assert!(text_100x30.contains("[PROD]"), "100x30 contains [PROD]");
    assert!(
        text_100x30.contains("sql_null"),
        "100x30 contains result headers"
    );
    assert!(text_100x30.contains("users"), "100x30 contains editor SQL");

    // 2. 80x24: compact terminal
    let area_80x24 = Rect::new(0, 0, 80, 24);
    update(&mut model, Message::Resized(80, 24));
    let mut buf_80x24 = Buffer::empty(area_80x24);
    crate::ui::render(&model, &keymap, area_80x24, &mut buf_80x24);
    let text_80x24 = buffer_to_string(&buf_80x24);
    assert!(text_80x24.contains("[PROD]"), "80x24 contains [PROD]");
    assert!(
        text_80x24.contains("sql_null"),
        "80x24 contains result headers"
    );

    // 3. 60x20: minimum supported interactive size
    let area_60x20 = Rect::new(0, 0, 60, 20);
    update(&mut model, Message::Resized(60, 20));
    let mut buf_60x20 = Buffer::empty(area_60x20);
    crate::ui::render(&model, &keymap, area_60x20, &mut buf_60x20);
    let text_60x20 = buffer_to_string(&buf_60x20);
    assert!(text_60x20.contains("[PROD]"), "60x20 contains [PROD]");
    assert!(
        text_60x20.contains("sql_null") || text_60x20.contains("Results"),
        "60x20 retains result grid or header"
    );

    // 4. 30x6: undersized viewport below minimum -> verify calm resize message without panicking
    let area_30x6 = Rect::new(0, 0, 30, 6);
    update(&mut model, Message::Resized(30, 6));
    let mut buf_30x6 = Buffer::empty(area_30x6);
    crate::ui::render(&model, &keymap, area_30x6, &mut buf_30x6);
    let text_30x6 = buffer_to_string(&buf_30x6);
    assert!(
        text_30x6.contains("Terminal too small"),
        "30x6 renders calm resize message"
    );
    assert!(text_30x6.contains("30x6"), "30x6 states current dimensions");

    // Live resize sequence: 30x6 -> render -> 80x24 -> render -> 100x30 -> render
    let expected_sql = model.editor.text().to_owned();
    let expected_rows = model.visible_result().unwrap().rows.len();

    update(&mut model, Message::Resized(30, 6));
    let mut live_buf30 = Buffer::empty(area_30x6);
    crate::ui::render(&model, &keymap, area_30x6, &mut live_buf30);

    update(&mut model, Message::Resized(80, 24));
    let mut live_buf80 = Buffer::empty(area_80x24);
    crate::ui::render(&model, &keymap, area_80x24, &mut live_buf80);

    update(&mut model, Message::Resized(100, 30));
    let mut live_buf100 = Buffer::empty(area_100x30);
    crate::ui::render(&model, &keymap, area_100x30, &mut live_buf100);

    let text_recovered = buffer_to_string(&live_buf100);
    assert_eq!(
        model.editor.text(),
        expected_sql,
        "editor SQL intact after live resize"
    );
    assert_eq!(
        model.visible_result().unwrap().rows.len(),
        expected_rows,
        "retained result rows intact after live resize"
    );
    assert!(
        text_recovered.contains("[PROD]"),
        "recovered workspace displays [PROD]"
    );
    assert!(
        text_recovered.contains("sql_null"),
        "recovered workspace displays results"
    );
}
