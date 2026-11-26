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
    ColorDepth, Rgb, Theme, Token, ValidatedThemes, lower_theme_inputs, theme_for_background,
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

fn buffer_runs(buf: &Buffer, area: Rect) -> Vec<serde_json::Value> {
    let mut runs = Vec::new();
    for y in 0..area.height {
        let mut x = 0;
        while x < area.width {
            let first = &buf[(x, y)];
            let start = x;
            let mut symbols = Vec::new();
            while x < area.width {
                let cell = &buf[(x, y)];
                if cell.fg != first.fg || cell.bg != first.bg || cell.modifier != first.modifier {
                    break;
                }
                symbols.push(cell.symbol().to_owned());
                x += 1;
            }
            runs.push(json!({
                "x": start,
                "y": y,
                "fg": format!("{:?}", first.fg),
                "bg": format!("{:?}", first.bg),
                "modifiers": first.modifier.bits(),
                "modifier_names": format!("{:?}", first.modifier),
                "symbols": symbols
            }));
        }
    }
    runs
}

fn capture(writer: &mut impl Write, buf: &Buffer, metadata: serde_json::Value) {
    let runs = buffer_runs(buf, AREA);
    serde_json::to_writer(
        &mut *writer,
        &json!({
            "kind": "SyntheticRatatuiBuffer",
            "width": AREA.width,
            "height": AREA.height,
            "metadata": metadata,
            "runs": runs
        }),
    )
    .unwrap();
    writeln!(writer).unwrap();
}

fn capture_showcase_scene(
    writer: &mut impl Write,
    buf: &Buffer,
    area: Rect,
    scene: &str,
    title: &str,
    subtitle: &str,
) {
    let runs = buffer_runs(buf, area);
    serde_json::to_writer(
        &mut *writer,
        &json!({
            "scene": scene,
            "title": title,
            "subtitle": subtitle,
            "width": area.width,
            "height": area.height,
            "runs": runs,
        }),
    )
    .unwrap();
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
    model.running_source = Some(model.editor_submission(None));
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
        frame0_text.contains("2.0 s"),
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

const SHOWCASE_AREA: Rect = Rect::new(0, 0, 124, 34);

/// The SQL every showcase workbench has just run.
const SHOWCASE_SQL: &str = "-- Inspect active customer orders and NULL distinction\n\
     SELECT id, customer, status, amount_cents, notes\n\
     FROM public.orders\n\
     WHERE status IN ('paid', 'pending')\n\
     ORDER BY id ASC;";

/// The same statement on one line, as the reviewed-update source.
const SHOWCASE_SOURCE_SQL: &str = "SELECT id, customer, status, amount_cents, notes FROM public.orders WHERE status IN ('paid', 'pending') ORDER BY id ASC;";

/// The columns of `public.orders`, in catalogue order.
const SHOWCASE_ORDER_COLUMNS: [(&str, &str, bool); 5] = [
    ("id", "integer", false),
    ("customer", "text", false),
    ("status", "text", false),
    ("amount_cents", "integer", false),
    ("notes", "text", true),
];

/// Synthetic rows in server order (`ORDER BY id`): every status is one the
/// statement's `WHERE` admits, and the ids skip the rows it filtered out.
/// The notes keep SQL NULL, an empty string and the literal text `NULL`
/// apart, and each of them sorts into the visible window by amount.
const SHOWCASE_ORDER_ROWS: [(&str, &str, &str, &str, Option<&str>); 14] = [
    (
        "1",
        "Ada Lovelace",
        "paid",
        "14500",
        Some("priority tier 1"),
    ),
    ("2", "Grace Hopper", "paid", "28900", None),
    ("4", "Edsger Dijkstra", "pending", "19200", Some("")),
    ("5", "Barbara Liskov", "paid", "51000", Some("NULL")),
    (
        "7",
        "Alan Turing",
        "paid",
        "128000",
        Some("invoice by email"),
    ),
    ("8", "Katherine Johnson", "pending", "7350", None),
    ("9", "Donald Knuth", "paid", "64200", Some("gift wrap")),
    (
        "11",
        "Margaret Hamilton",
        "paid",
        "89900",
        Some("split shipment"),
    ),
    ("12", "John Backus", "pending", "3999", None),
    (
        "14",
        "Frances Allen",
        "paid",
        "45600",
        Some("call before delivery"),
    ),
    (
        "15",
        "Ken Thompson",
        "pending",
        "18750",
        Some("net 30 terms"),
    ),
    ("17", "Radia Perlman", "paid", "33300", None),
    ("18", "Tony Hoare", "paid", "2100", Some("sample order")),
    (
        "20",
        "Shafi Goldwasser",
        "pending",
        "11000",
        Some("repeat customer"),
    ),
];

fn showcase_order_columns() -> Vec<crate::postgres::ColumnInfo> {
    SHOWCASE_ORDER_COLUMNS
        .into_iter()
        .map(|(name, data_type, nullable)| crate::postgres::ColumnInfo {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            primary_key: name == "id",
            default: None,
        })
        .collect()
}

/// The result of [`SHOWCASE_SQL`], in server order.
fn showcase_result_set() -> ResultSet {
    let mut set = ResultSet::new(
        SHOWCASE_ORDER_COLUMNS
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect(),
        100,
    );
    for (id, customer, status, amount, notes) in SHOWCASE_ORDER_ROWS {
        assert!(set.push(vec![
            Cell::Text(id.into()),
            Cell::Text(customer.into()),
            Cell::Text(status.into()),
            Cell::Text(amount.into()),
            notes.map_or(Cell::Null, |notes| Cell::Text(notes.into())),
        ]));
    }
    set
}

/// A successful execution of [`SHOWCASE_SQL`] as the session reports it.
fn showcase_execution(job: JobId, elapsed: Duration) -> Execution {
    Execution {
        notice_report: NoticeReport::default(),
        job,
        statements: vec![StatementResult {
            result_set: Some(showcase_result_set()),
            rows_affected: Some(SHOWCASE_ORDER_ROWS.len() as u64),
            elapsed,
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed,
        error: None,
        transaction: TransactionState::Autocommit,
    }
}

/// The completion catalogue snapshot, consistent with the Objects tree and
/// shaped the way `postgres::metadata::completion_catalog` builds it:
/// relations carry no detail, functions carry `returns ...`.
fn showcase_completion_catalog() -> crate::query::completion::CompletionCatalog {
    use crate::query::completion::{
        CatalogColumn, CatalogObject, CatalogObjectKind, CatalogRelation, CompletionCatalog,
    };

    let object = |kind, schema: &str, name: &str, detail: Option<&str>| CatalogObject {
        kind,
        schema: schema.into(),
        name: name.into(),
        readable: true,
        detail: detail.map(str::to_owned),
    };
    let relation = |schema: &str, name: &str, columns: &[(&str, &str)]| CatalogRelation {
        schema: schema.into(),
        name: name.into(),
        columns: columns
            .iter()
            .map(|(name, data_type)| CatalogColumn {
                name: (*name).into(),
                data_type: (*data_type).into(),
            })
            .collect(),
    };
    let order_columns = SHOWCASE_ORDER_COLUMNS
        .iter()
        .map(|(name, data_type, _)| (*name, *data_type))
        .collect::<Vec<_>>();
    CompletionCatalog {
        objects: vec![
            object(CatalogObjectKind::Table, "public", "customers", None),
            object(CatalogObjectKind::Table, "public", "orders", None),
            object(CatalogObjectKind::Table, "public", "rendering_cases", None),
            object(CatalogObjectKind::Table, "public", "type_coverage", None),
            object(CatalogObjectKind::View, "public", "recent_orders", None),
            object(CatalogObjectKind::View, "reporting", "order_totals", None),
            object(
                CatalogObjectKind::Function,
                "public",
                "customer_region",
                Some("returns text"),
            ),
            object(
                CatalogObjectKind::Function,
                "public",
                "first_order_date",
                Some("returns date"),
            ),
        ],
        relations: vec![
            relation(
                "public",
                "customers",
                &[("id", "integer"), ("name", "text"), ("region", "text")],
            ),
            relation("public", "orders", &order_columns),
            relation(
                "public",
                "rendering_cases",
                &[("label", "text"), ("value", "text")],
            ),
            relation(
                "public",
                "type_coverage",
                &[
                    ("as_int", "integer"),
                    ("as_json", "jsonb"),
                    ("as_ts", "timestamp with time zone"),
                ],
            ),
            relation(
                "public",
                "recent_orders",
                &[
                    ("id", "integer"),
                    ("customer", "text"),
                    ("amount_cents", "integer"),
                ],
            ),
            relation(
                "reporting",
                "order_totals",
                &[
                    ("region", "text"),
                    ("orders", "bigint"),
                    ("total_cents", "bigint"),
                ],
            ),
        ],
    }
}

fn showcase_base_model(theme_choice: ThemeChoice) -> Model {
    use crate::postgres::{ObjectKind, ObjectSummary, SchemaSummary};

    let mut model = Model::new(100);
    model.size = (SHOWCASE_AREA.width, SHOWCASE_AREA.height);
    model.sidebar_visible = true;
    model.focus = Focus::Editor;
    model.keymap_snapshot = Keymap::new().snapshot();
    model.presentation = Presentation::new(
        Theme::new(theme_choice, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    model.connection = ConnectionState::Connected(Box::new(SessionInfo {
        target: "ignatius_test@127.0.0.1:55432/ignatius_demo".into(),
        database: "ignatius_demo".into(),
        user: "ignatius_test".into(),
        server_version: "18.6".into(),
        backend_pid: "4821".into(),
        search_path: "\"$user\", public, reporting".into(),
        read_only: false,
        tls: TlsState::Active {
            version: "TLSv1.3".into(),
            cipher: "TLS_AES_256_GCM_SHA384".into(),
            mode: crate::connection::SslMode::VerifyFull,
        },
        environment: Environment::Production,
    }));

    let extra_tables = [
        ("audit_log", Some("append-only trail")),
        ("billing_cycles", None),
        ("carts", None),
        ("discounts", None),
        ("invoices", Some("settled ledger")),
        ("line_items", None),
        ("payments", None),
        ("payouts", None),
        ("refunds", None),
        ("sessions", None),
        ("shipments", None),
        ("subscriptions", None),
        ("tax_rates", None),
        ("transactions", None),
        ("user_roles", None),
        ("warehouses", None),
        ("webhooks", None),
    ];
    let schemas = vec![
        SchemaSummary {
            name: "public".into(),
            usable: true,
            counts: BTreeMap::from([
                (ObjectKind::Table, 4 + extra_tables.len() as i64),
                (ObjectKind::View, 1),
                (ObjectKind::Function, 2),
            ]),
        },
        SchemaSummary {
            name: "reporting".into(),
            usable: true,
            counts: BTreeMap::from([(ObjectKind::View, 1)]),
        },
    ];
    model.tree.set_schemas(schemas);
    model.tree.expand_schema(&[0]);

    // Expand the table group under public ([0, 0]). Production lists the
    // relations by name and gives each its comment as the detail.
    model.tree.selected = 1;
    let (req, path, _) = model.tree.expand_selected().expect("expand table group");
    let table = |name: &str, comment: Option<&str>| ObjectSummary {
        schema: "public".into(),
        name: name.into(),
        kind: ObjectKind::Table,
        readable: true,
        detail: comment.map(str::to_owned),
    };
    let mut table_summaries = vec![
        table("customers", None),
        table("orders", Some("sales ledger")),
        table("rendering_cases", None),
        table("type_coverage", None),
    ];
    for (name, comment) in extra_tables {
        table_summaries.push(table(name, comment));
    }
    model.tree.apply(
        req,
        &path,
        crate::app::tree::MetadataPayload::Objects(table_summaries),
    );

    // Expand the orders table ([0, 0, 1]): its columns and its index.
    model.tree.selected = 3;
    let (req_cols, path_cols, _) = model.tree.expand_selected().expect("expand orders table");
    model.tree.apply(
        req_cols,
        &path_cols,
        crate::app::tree::MetadataPayload::Relation {
            columns: showcase_order_columns(),
            indexes: vec![ObjectSummary {
                kind: ObjectKind::Index,
                schema: "public".into(),
                name: "orders_pkey".into(),
                readable: true,
                detail: Some("primary key".to_owned()),
            }],
        },
    );
    model.tree.selected = 3;

    // The completion snapshot is loaded with the tree, so neither the
    // completion menu nor a diagnostic says it is missing.
    model.completion.catalog = crate::app::completion::CatalogStatus::Ready {
        catalog: showcase_completion_catalog(),
        loaded_at: "2026-11-23 09:41:07 +02:00".into(),
    };

    model.editor.set_text(SHOWCASE_SQL);
    // Rest right after the closing parenthesis in `IN ('paid', 'pending')`
    // so SQL-aware delimiter matching highlights both parentheses visibly.
    let delim_cursor = SHOWCASE_SQL.find(')').map_or(62, |idx| idx + 1);
    model.editor.set_cursor(delim_cursor);

    model.last_execution = Some(showcase_execution(JobId(1), Duration::from_millis(240)));
    model.last_elapsed = Some(Duration::from_millis(240));
    model.result_grid.reset_view();
    // A local sort on the amount, largest first, rather than repeating the
    // server's ORDER BY id. The selected column is widened by one step so the
    // inline sort chevron (`▾`) renders next to `amount_cents`.
    model.result_grid.toggle_sort(3);
    model.result_grid.toggle_sort(3);
    let auto_amount = crate::app::grid::automatic_column_width(&showcase_result_set(), 3);
    let _ = model.result_grid.adjust_width(3, auto_amount, 2);
    model.selected_column = 3;

    model
}

fn render_showcase(model: &Model) -> Buffer {
    let mut buf = Buffer::empty(SHOWCASE_AREA);
    crate::ui::render(model, &Keymap::new(), SHOWCASE_AREA, &mut buf);
    buf
}

/// Runs the editor buffer through the reducer and returns the job it started.
fn showcase_run(model: &mut Model, sql: &str) -> JobId {
    let effects = update(model, Message::Action(Action::RunBuffer));
    let job = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Execute { job, sql: sent } => {
                assert_eq!(sent.trim(), sql.trim(), "the buffer is what runs");
                Some(*job)
            }
            _ => None,
        })
        .expect("running the buffer starts a job");
    assert!(model.phase.is_busy(), "the statement is in flight");
    job
}

fn showcase_development_model(theme_choice: ThemeChoice) -> Model {
    let mut model = showcase_base_model(theme_choice);
    let ConnectionState::Connected(session) = &mut model.connection else {
        unreachable!("showcase fixture is connected");
    };
    session.environment = Environment::Development;
    session.tls = TlsState::Disabled;
    model
}

fn check_showcase_scene(
    writer: Option<&mut impl Write>,
    model: &Model,
    scene: &str,
    title: &str,
    subtitle: &str,
    expected: &[&str],
) {
    let buf = render_showcase(model);
    let text = buffer_to_string(&buf);
    for item in expected {
        assert!(text.contains(item), "{scene} is missing {item:?}:\n{text}");
    }
    if let Some(writer) = writer {
        capture_showcase_scene(writer, &buf, SHOWCASE_AREA, scene, title, subtitle);
    }
}

fn enable_showcase_tabs(model: &mut Model) {
    model.buffers.buffers[0].title = "orders.sql".into();
    model.sync_active_buffer_from_editor();
    model.buffers.buffers.push(crate::buffer::EditorBuffer::new(
        2,
        "revenue.sql",
        Some(
            "SELECT region, sum(amount_cents) AS total_cents\n\
             FROM reporting.order_totals\n\
             GROUP BY region\n\
             ORDER BY total_cents DESC;\n"
                .into(),
        ),
    ));
    model.buffers.buffers.push(crate::buffer::EditorBuffer::new(
        3,
        "null_audit.sql",
        Some("SELECT label, value FROM public.rendering_cases WHERE value IS NULL;\n".into()),
    ));
    model.buffers.next_id = 4;
}

fn showcase_theme_scenes(writer: &mut Option<impl Write>) {
    let field = showcase_development_model(ThemeChoice::FieldIndustrial);
    check_showcase_scene(
        writer.as_mut(),
        &field,
        "workbench-field-industrial",
        "field-industrial",
        "The workbench in the field-industrial theme, with a populated result grid",
        &["[DEV]", "Ada Lovelace", "[null]"],
    );
    let mut material = showcase_development_model(ThemeChoice::Material);
    material.last_execution = None;
    material.last_elapsed = None;
    check_showcase_scene(
        writer.as_mut(),
        &material,
        "workbench-material",
        "material",
        "The workbench in the material theme before the first query",
        &["[DEV]", "No query has run yet.", "SQL is ready to run."],
    );
    let mut atelier = showcase_base_model(ThemeChoice::Atelier);
    enable_showcase_tabs(&mut atelier);
    check_showcase_scene(
        writer.as_mut(),
        &atelier,
        "workbench-atelier",
        "atelier",
        "The workbench in the atelier theme, a warm dark editorial palette",
        &[
            "[PROD]",
            "Ada Lovelace",
            "[null]",
            "amount_cents \u{25be}",
            "orders.sql",
            "revenue.sql",
        ],
    );
    let mut paper = showcase_development_model(ThemeChoice::Paper);
    enable_showcase_tabs(&mut paper);
    check_showcase_scene(
        writer.as_mut(),
        &paper,
        "workbench-paper",
        "paper",
        "The workbench in the paper theme, a warm light editorial palette",
        &[
            "[DEV]",
            "Ada Lovelace",
            "[null]",
            "amount_cents \u{25be}",
            "orders.sql",
            "revenue.sql",
        ],
    );
}

fn showcase_bench_scene(writer: Option<&mut impl Write>) {
    use crate::app::message::BenchTarget;
    use crate::bench::{BenchSample, compute_bench_stats};

    let mut model = showcase_development_model(ThemeChoice::Material);
    model.editor.set_text(
        "SELECT id FROM public.orders WHERE amount_cents >= 10000;\n\
         SELECT id FROM public.orders WHERE 10000 <= amount_cents;",
    );
    model.editor.set_cursor(0);
    let mut queued = update(&mut model, Message::Action(Action::QueryBench));
    assert_eq!(queued.len(), 1, "A runs first; B waits for A to report");
    let mut reported = 0;
    while let Some(effect) = queued.pop() {
        let Effect::RunQueryBench {
            request,
            target,
            sql,
            config,
        } = effect
        else {
            panic!("expected a benchmark effect");
        };
        let (name, durations) = match target {
            BenchTarget::Baseline => (
                "Baseline (A)",
                [3.1, 3.4, 3.7, 3.9, 4.5, 3.8, 4.1, 3.5, 3.4, 3.9],
            ),
            BenchTarget::Candidate => (
                "Candidate (B)",
                [2.1, 2.4, 2.8, 2.6, 3.2, 2.8, 3.0, 2.5, 2.4, 2.9],
            ),
        };
        let samples = durations
            .into_iter()
            .zip(1..)
            .map(|(duration_ms, iteration)| BenchSample {
                iteration,
                duration_ms,
                rows_returned: 12,
            })
            .collect::<Vec<_>>();
        let stats = compute_bench_stats(name, &sql, &config, &samples);
        queued = update(
            &mut model,
            Message::QueryBenchLoaded {
                request,
                target,
                result: Box::new(Ok(stats)),
            },
        );
        reported += 1;
    }
    assert_eq!(reported, 2, "A and B both ran, one after the other");
    check_showcase_scene(
        writer,
        &model,
        "query-bench",
        "Benchmark",
        "An A/B latency summary of two statements in rollback mode",
        &[
            "[BENCHMARK]",
            "[MODE: ROLLBACK]",
            "[FASTER]",
            "p95",
            "sparkline",
        ],
    );
}

fn showcase_snapshot_scene(writer: Option<&mut impl Write>) {
    use crate::snapshot::{
        ColumnSnapshot, ConstraintSnapshot, IndexSnapshot, SchemaSnapshot, TableSnapshot,
        ViewSnapshot, sha256_hex,
    };

    // The current catalogue is the one the Objects tree shows. The pinned
    // baseline is the same catalogue from before `legacy_note` became
    // `notes`, so the drift is one dropped and one added column.
    let mut model = showcase_development_model(ThemeChoice::FieldIndustrial);
    let table = |name: &str, columns: &[(&str, &str, bool)], key: Option<&str>| {
        let primary = key.map(|key| format!("{name}_pkey ({key})"));
        TableSnapshot {
            schema: "public".into(),
            name: name.into(),
            columns: columns
                .iter()
                .zip(1..)
                .map(|((column, data_type, nullable), ordinal)| ColumnSnapshot {
                    name: (*column).into(),
                    data_type: (*data_type).into(),
                    nullable: *nullable,
                    default_expr: None,
                    ordinal,
                })
                .collect(),
            constraints: key
                .map(|key| ConstraintSnapshot {
                    name: format!("{name}_pkey"),
                    kind: "PRIMARY KEY".into(),
                    definition: format!("PRIMARY KEY ({key})"),
                })
                .into_iter()
                .collect(),
            indexes: primary
                .map(|_| IndexSnapshot {
                    name: format!("{name}_pkey"),
                    is_unique: true,
                    is_primary: true,
                    definition: format!(
                        "CREATE UNIQUE INDEX {name}_pkey ON public.{name} USING btree ({})",
                        key.unwrap_or_default()
                    ),
                })
                .into_iter()
                .collect(),
            rls_enabled: false,
            rls_forced: false,
            policies: vec![],
        }
    };
    let view = |schema: &str, name: &str, definition: &str| ViewSnapshot {
        schema: schema.into(),
        name: name.into(),
        materialized: false,
        definition_sha256: sha256_hex(definition.as_bytes()),
    };
    let mut snapshot = SchemaSnapshot::new(None);
    snapshot.tables = vec![
        table(
            "customers",
            &[
                ("id", "integer", false),
                ("name", "text", false),
                ("region", "text", true),
            ],
            Some("id"),
        ),
        table("orders", &SHOWCASE_ORDER_COLUMNS, Some("id")),
        table(
            "rendering_cases",
            &[("label", "text", false), ("value", "text", true)],
            Some("label"),
        ),
        table(
            "type_coverage",
            &[
                ("as_int", "integer", true),
                ("as_json", "jsonb", true),
                ("as_ts", "timestamp with time zone", true),
            ],
            None,
        ),
    ];
    snapshot.views = vec![
        view(
            "public",
            "recent_orders",
            " SELECT id, customer, amount_cents FROM orders;",
        ),
        view(
            "reporting",
            "order_totals",
            " SELECT region, count(*) AS orders, sum(amount_cents) AS total_cents FROM orders;",
        ),
    ];
    let current = snapshot.clone().finalize();
    let notes = snapshot.tables[1]
        .columns
        .iter_mut()
        .find(|column| column.name == "notes")
        .expect("orders has notes");
    notes.name = "legacy_note".into();
    let baseline = snapshot.finalize();
    for (action, capture) in [
        (Action::SchemaSnapshot, baseline),
        (Action::Insert('r'), current),
    ] {
        let pin_baseline = action == Action::SchemaSnapshot;
        let effects = update(&mut model, Message::Action(action));
        let [Effect::CaptureSchemaSnapshot { request, .. }] = effects.as_slice() else {
            panic!("expected a snapshot effect");
        };
        update(
            &mut model,
            Message::SchemaSnapshotLoaded {
                request: *request,
                result: Box::new(Ok(capture)),
            },
        );
        if pin_baseline {
            update(&mut model, Message::Action(Action::Insert('b')));
        }
    }
    check_showcase_scene(
        writer,
        &model,
        "schema-snapshot",
        "Schema snapshot",
        "A catalogue fingerprint and its drift against a pinned baseline",
        &[
            "[SNAPSHOT v1]",
            "[SHA256:",
            "[BREAKING: 1]",
            "public.orders",
        ],
    );
}

fn showcase_runbook_scene(writer: Option<&mut impl Write>) {
    let mut model = showcase_development_model(ThemeChoice::Material);
    model.editor.set_text(
        "-- @cell check-orders\nSELECT count(*) FROM public.orders;\n\
         -- @cell annotate-order\nUPDATE public.orders SET notes = 'reviewed' WHERE id = 1;",
    );
    update(&mut model, Message::Action(Action::SqlRunbook));
    update(&mut model, Message::Action(Action::Insert('2')));
    check_showcase_scene(
        writer,
        &model,
        "sql-runbook",
        "Runbook",
        "A runbook dry run with the write cell blocked",
        &["[RUNBOOK]", "[WRITES: BLOCKED]", "planned=1", "blocked=1"],
    );
}

fn showcase_plan_document() -> PlanDocument {
    let plan_json = r#"[
  {
    "Plan": {
      "Node Type": "Merge Join",
      "Join Type": "Inner",
      "Merge Cond": "(o.customer = c.name)",
      "Startup Cost": 42.1,
      "Total Cost": 68.4,
      "Plan Rows": 420,
      "Plan Width": 64,
      "Actual Startup Time": 9.8,
      "Actual Total Time": 12.4,
      "Actual Rows": 420,
      "Actual Loops": 1,
      "Plans": [
        {
          "Node Type": "Sort",
          "Sort Key": ["o.customer"],
          "Startup Cost": 28.4,
          "Total Cost": 29.45,
          "Plan Rows": 420,
          "Plan Width": 48,
          "Actual Startup Time": 6.1,
          "Actual Total Time": 8.2,
          "Actual Rows": 420,
          "Actual Loops": 1,
          "Plans": [
            {
              "Node Type": "Seq Scan",
              "Relation Name": "orders",
              "Alias": "o",
              "Startup Cost": 0.0,
              "Total Cost": 25.5,
              "Plan Rows": 10,
              "Plan Width": 48,
              "Actual Startup Time": 0.01,
              "Actual Total Time": 1.4,
              "Actual Rows": 420,
              "Actual Loops": 1,
              "Filter": "(status = 'paid'::text)",
              "Rows Removed by Filter": 180
            }
          ]
        },
        {
          "Node Type": "Sort",
          "Sort Key": ["c.name"],
          "Startup Cost": 13.7,
          "Total Cost": 13.9,
          "Plan Rows": 40,
          "Plan Width": 32,
          "Actual Startup Time": 2.6,
          "Actual Total Time": 3.1,
          "Actual Rows": 40,
          "Actual Loops": 1,
          "Plans": [
            {
              "Node Type": "Seq Scan",
              "Relation Name": "customers",
              "Alias": "c",
              "Startup Cost": 0.0,
              "Total Cost": 12.4,
              "Plan Rows": 40,
              "Plan Width": 32,
              "Actual Startup Time": 0.01,
              "Actual Total Time": 2.3,
              "Actual Rows": 40,
              "Actual Loops": 1
            }
          ]
        }
      ]
    },
    "Planning Time": 0.38,
    "Execution Time": 12.9
  }
]"#;
    PlanDocument::parse(plan_json, true).expect("valid plan")
}

#[test]
fn workbench_showcase_scenes_preserve_elevated_chrome_and_overlays() {
    use crate::app::palette::{Palette, PaletteCommand, PaletteEntry};
    use crate::postgres::ObjectKind;
    use crate::query::completion::CompletionScope;

    let capture_path =
        std::env::var_os("IGNATIUS_SHOWCASE_CAPTURE_DIR").map(std::path::PathBuf::from);
    let mut writer = capture_path.map(|path| {
        std::fs::create_dir_all(&path).unwrap();
        BufWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path.join("showcase.jsonl"))
                .unwrap(),
        )
    });

    // 1. "workbench-overview"
    let model = showcase_base_model(ThemeChoice::Dark);
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(text.contains("Ignatius"), "overview contains product name");
    assert!(text.contains("[PROD]"), "overview contains [PROD]");
    assert!(text.contains("orders"), "overview contains orders");
    assert!(
        text.contains("Ada Lovelace"),
        "overview contains Ada Lovelace"
    );
    assert!(text.contains("[null]"), "overview contains [null]");
    assert!(
        text.contains("sorted by amount_cents descending"),
        "overview contains sorted by amount_cents descending"
    );
    assert!(
        text.contains("5 lines") && text.contains("[focused]"),
        "overview editor title shows line count and focused badge:\n{text}"
    );
    assert!(
        text.contains("1/14 \u{00b7} amount_cents"),
        "overview results toolbar shows coordinate cue:\n{text}"
    );
    assert!(
        text.contains('\u{2503}'),
        "overview renders hairline scrollbar thumb:\n{text}"
    );
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "workbench-overview",
            "Workbench",
            "The connected workbench with the Objects tree, the editor and a sorted result grid",
        );
    }

    // 2. "completion-menu": production completion at a real cursor, so the
    // candidates, their order and their details are exactly what the menu
    // offers after `FROM` in a statement that defines a CTE.
    let mut model = showcase_base_model(ThemeChoice::Dark);
    model.editor.set_text(
        "-- Pending orders by customer\n\
         WITH active_orders AS (\n\
         \x20   SELECT id, customer, amount_cents\n\
         \x20   FROM public.orders\n\
         \x20   WHERE status = 'pending'\n\
         )\n\
         SELECT customer, amount_cents\n\
         FROM ",
    );
    let end = model.editor.text().len();
    model.editor.set_cursor(end);
    assert!(update(&mut model, Message::Action(Action::Complete)).is_empty());
    let menu = model
        .completion
        .menu
        .as_ref()
        .expect("explicit completion opens");
    assert_eq!(menu.result.scope, CompletionScope::Relations);
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(
        text.contains("Complete"),
        "completion menu contains Complete title"
    );
    assert!(
        text.contains("orders"),
        "completion menu contains orders candidate"
    );
    assert!(
        text.contains("first_order_date"),
        "completion menu contains a function candidate"
    );
    assert!(
        text.contains("defined in this statement"),
        "completion menu describes the CTE as production does"
    );
    assert!(
        !text.contains("not loaded yet"),
        "completion menu does not deny the loaded catalogue"
    );
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "completion-menu",
            "Completion",
            "Completion at a relation position: a CTE, tables, views, functions and keywords",
        );
    }

    // 3. "palette-preview"
    let mut model = showcase_base_model(ThemeChoice::Dark);
    let mut entries = vec![
        PaletteEntry {
            label: "revenue_by_region".into(),
            detail: [
                "SELECT c.region,",
                "       count(*) AS orders,",
                "       sum(o.amount_cents) AS total_cents",
                "FROM public.orders o",
                "JOIN public.customers c ON c.name = o.customer",
                "WHERE o.status = 'paid'",
                "GROUP BY c.region",
                "ORDER BY total_cents DESC;",
            ]
            .join("\n"),
            group: "saved",
            command: PaletteCommand::Open("revenue_by_region".into()),
        },
        PaletteEntry {
            label: "active_customer_orders".into(),
            detail: [
                "SELECT id, customer, amount_cents",
                "FROM public.orders",
                "WHERE status IN ('paid', 'pending')",
                "ORDER BY id;",
            ]
            .join("\n"),
            group: "saved",
            command: PaletteCommand::Open("active_customer_orders".into()),
        },
        PaletteEntry {
            label: "null_audit_report".into(),
            detail: [
                "SELECT label, value",
                "FROM public.rendering_cases",
                "WHERE value IS NULL OR value IN ('', 'NULL');",
            ]
            .join("\n"),
            group: "saved",
            command: PaletteCommand::Open("null_audit_report".into()),
        },
    ];
    for (name, sql) in [
        (
            "top_customers_ltv",
            "SELECT customer, sum(amount_cents) AS lifetime_cents FROM public.orders GROUP BY 1 ORDER BY 2 DESC;",
        ),
        (
            "pending_order_aging",
            "SELECT id, customer, amount_cents FROM public.orders WHERE status = 'pending' ORDER BY id;",
        ),
        (
            "daily_settlement_rollup",
            "SELECT status, count(*), sum(amount_cents) FROM public.orders GROUP BY status;",
        ),
        (
            "table_bloat_estimate",
            "SELECT schemaname, relname, n_dead_tup FROM pg_stat_user_tables ORDER BY n_dead_tup DESC;",
        ),
        (
            "cache_hit_by_table",
            "SELECT relname, heap_blks_hit, heap_blks_read FROM pg_statio_user_tables ORDER BY heap_blks_read DESC;",
        ),
        (
            "long_running_locks",
            "SELECT pid, state, wait_event_type, query FROM pg_stat_activity WHERE state <> 'idle';",
        ),
        (
            "blocking_pid_chains",
            "SELECT pid, pg_blocking_pids(pid) AS blockers FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0;",
        ),
        (
            "unused_indexes_30d",
            "SELECT indexrelname, idx_scan, pg_relation_size(indexrelid) FROM pg_stat_user_indexes WHERE idx_scan = 0;",
        ),
        (
            "sequence_headroom",
            "SELECT schemaname, sequencename, last_value FROM pg_sequences ORDER BY last_value DESC NULLS LAST;",
        ),
        (
            "replication_slot_lag",
            "SELECT slot_name, active, pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn) AS lag_bytes FROM pg_replication_slots;",
        ),
        (
            "wal_generation_rate",
            "SELECT wal_records, wal_fpi, wal_bytes FROM pg_stat_wal;",
        ),
        (
            "rls_policy_audit",
            "SELECT schemaname, tablename, policyname, cmd FROM pg_policies ORDER BY tablename;",
        ),
        (
            "role_membership_tree",
            "SELECT r.rolname AS role, m.rolname AS member FROM pg_auth_members am JOIN pg_roles r ON r.oid = am.roleid JOIN pg_roles m ON m.oid = am.member;",
        ),
        (
            "stale_analyze_tables",
            "SELECT relname, last_autoanalyze, n_mod_since_analyze FROM pg_stat_user_tables ORDER BY n_mod_since_analyze DESC;",
        ),
        (
            "vacuum_progress_live",
            "SELECT pid, phase, heap_blks_scanned, heap_blks_total FROM pg_stat_progress_vacuum;",
        ),
        (
            "toast_table_sizes",
            "SELECT relname, pg_total_relation_size(reltoastrelid) AS toast_bytes FROM pg_class WHERE reltoastrelid <> 0;",
        ),
        (
            "unindexed_fk_check",
            "SELECT conrelid::regclass, conname FROM pg_constraint WHERE contype = 'f';",
        ),
        (
            "idle_in_transaction",
            "SELECT pid, usename, now() - xact_start AS age FROM pg_stat_activity WHERE state = 'idle in transaction';",
        ),
        (
            "partition_row_skew",
            "SELECT inhparent::regclass, inhrelid::regclass FROM pg_inherits ORDER BY 1, 2;",
        ),
        (
            "extension_inventory",
            "SELECT extname, extversion FROM pg_extension ORDER BY extname;",
        ),
        (
            "hba_auth_rules",
            "SELECT line_number, type, database, auth_method FROM pg_hba_file_rules;",
        ),
        (
            "modified_settings",
            "SELECT name, setting, unit, source FROM pg_settings WHERE source <> 'default' ORDER BY name;",
        ),
        (
            "checkpoint_pressure",
            "SELECT num_timed, num_requested, buffers_written FROM pg_stat_checkpointer;",
        ),
        (
            "type_coverage_sample",
            "SELECT as_int, as_json, as_ts FROM public.type_coverage;",
        ),
        (
            "order_totals_view",
            "SELECT region, orders, total_cents FROM reporting.order_totals ORDER BY total_cents DESC;",
        ),
        (
            "recent_orders_top",
            "SELECT id, customer, amount_cents FROM public.recent_orders ORDER BY id DESC LIMIT 25;",
        ),
        (
            "customer_region_lookup",
            "SELECT id, name, region, public.customer_region(name) FROM public.customers;",
        ),
        (
            "high_value_orders",
            "SELECT id, customer, amount_cents FROM public.orders WHERE amount_cents >= 50000;",
        ),
        (
            "missing_notes_audit",
            "SELECT id, customer, status FROM public.orders WHERE notes IS NULL;",
        ),
        (
            "empty_notes_audit",
            "SELECT id, customer, status FROM public.orders WHERE notes = '';",
        ),
        (
            "constraint_validation",
            "SELECT conname, convalidated FROM pg_constraint WHERE NOT convalidated;",
        ),
        (
            "dead_tuple_ratio",
            "SELECT relname, n_live_tup, n_dead_tup FROM pg_stat_user_tables WHERE n_live_tup + n_dead_tup > 0;",
        ),
        (
            "bgwriter_activity",
            "SELECT buffers_clean, maxwritten_clean, buffers_alloc FROM pg_stat_bgwriter;",
        ),
    ] {
        entries.push(PaletteEntry {
            label: name.into(),
            detail: sql.into(),
            group: "saved",
            command: PaletteCommand::Open(name.into()),
        });
    }
    model.palette = Some(Palette::over_saved_queries(entries));
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(
        text.contains("Saved queries"),
        "palette contains Saved queries title"
    );
    assert!(
        text.contains("revenue_by_region"),
        "palette contains selected entry file"
    );
    assert!(
        text.contains("SQL:") || text.contains("Preview"),
        "palette contains preview pane"
    );
    assert!(
        text.contains('\u{2503}'),
        "palette renders scroll thumb when list overflows viewport:\n{text}"
    );
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "palette-preview",
            "Saved queries",
            "The palette over saved queries, with the selected query's SQL previewed",
        );
    }

    // 4. "plan-explorer": the editor holds the statement the plan is for,
    // and the plan shows exclusive self-time bottleneck detection landing on
    // a non-root child node (`Sort`) alongside an unclipped `[estimate mismatch]`
    // badge and proportional 5-cell micro-bars on every row.
    let mut model = showcase_base_model(ThemeChoice::Dark);
    model.editor.set_text(
        [
            "-- Paid orders with their customer's region",
            "SELECT o.id, c.name, c.region, o.amount_cents",
            "FROM public.orders o",
            "JOIN public.customers c ON c.name = o.customer",
            "WHERE o.status = 'paid';",
        ]
        .join("\n"),
    );
    model.editor.set_cursor(50);
    model.focus = Focus::Results;
    let doc = showcase_plan_document();
    model.plan.ready(doc);
    model.plan.selected_path = vec![0, 0];
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(
        text.contains("EXPLAIN ANALYZE"),
        "plan explorer contains EXPLAIN ANALYZE"
    );
    assert!(
        text.contains("Nodes")
            && text.contains("Planning")
            && text.contains("Execution")
            && text.contains("Hotspot"),
        "plan explorer shows KPI strip:\n{text}"
    );
    assert!(
        text.contains("Attention: Sort (measured time)"),
        "plan explorer identifies non-root exclusive-time bottleneck:\n{text}"
    );
    assert!(
        text.contains("[attention: measured time]"),
        "plan explorer keeps attention badge unclipped:\n{text}"
    );
    assert!(
        text.contains("[estimate mismatch]"),
        "plan explorer contains unclipped estimate mismatch badge:\n{text}"
    );
    assert!(
        text.contains("self time 1.40 ms"),
        "plan detail reports exclusive self time:\n{text}"
    );
    assert!(
        text.contains('\u{2501}'),
        "plan explorer draws proportional micro-bars:\n{text}"
    );
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "plan-explorer",
            "Query plan",
            "An EXPLAIN ANALYZE plan tree with exclusive self-time micro-bars and mismatch markers",
        );
    }

    // 5. "error-diagnostics": the statement runs through the reducer and
    // fails the way `postgres::error::from_query_error` reports an undefined
    // column: the server's message, position, SQLSTATE and severity, and the
    // product's own advice for 42703. PostgreSQL attaches no schema, table or
    // column fields to 42703, so production shows no `Object context:` row
    // (and no `Catalogue:` row, which only follows it) for this error. That
    // row is covered with server-supplied fields in `results/tests.rs`.
    let mut model = showcase_base_model(ThemeChoice::Dark);
    let sql = "SELECT id, custmer_id, amount_cents FROM public.orders WHERE status = 'paid';";
    model.editor.set_text(sql);
    model.editor.set_cursor(sql.len());
    let job = showcase_run(&mut model, sql);
    let error = Diagnostic::new(
        DiagnosticKind::Query,
        "column \"custmer_id\" does not exist",
        "running statement 1",
    )
    .in_statement(1)
    .technical("SQLSTATE", "42703")
    .technical("Severity", "ERROR")
    .likely_cause("the column does not exist on that relation")
    .next_action("check the spelling, or inspect the table's columns")
    .at_position(12);
    update(
        &mut model,
        Message::ExecutionFinished(Box::new(Execution {
            notice_report: NoticeReport::default(),
            job,
            statements: Vec::new(),
            status: ExecutionStatus::Failed,
            elapsed: Duration::from_millis(15),
            error: Some(error),
            transaction: TransactionState::Autocommit,
        })),
    );
    // The technical fields are what the expand key shows.
    model.error_expanded = true;
    assert_eq!(model.focus, Focus::Editor, "the error location takes focus");
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(text.contains("While:"), "diagnostics contains While:");
    assert!(text.contains("Location:"), "diagnostics contains Location:");
    assert!(
        text.contains("Likely cause:"),
        "diagnostics contains Likely cause:"
    );
    assert!(text.contains("Next:"), "diagnostics contains Next:");
    assert!(text.contains("SQLSTATE"), "diagnostics labels the SQLSTATE");
    assert!(text.contains("42703"), "diagnostics shows the SQLSTATE");
    assert!(
        !text.contains("not loaded yet"),
        "diagnostics do not deny the loaded catalogue"
    );
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "error-diagnostics",
            "Query error",
            "A server error with its location marked in the statement and its SQLSTATE",
        );
    }

    // 6. "reviewed-update"
    let mut model = showcase_development_model(ThemeChoice::Dark);
    model.focus = Focus::Results;
    // The selected cell behind the review is the one being updated: Ada
    // Lovelace's status, wherever the local sort displays her row.
    model.selected_row = model
        .displayed_rows()
        .iter()
        .position(|row| *row == 0)
        .expect("source row 0 is displayed");
    model.selected_column = 2;
    let candidate = crate::app::model::UpdateCandidate {
        result_job: JobId(1),
        source_row: 0,
        result_column: 2,
        source_sql: SHOWCASE_SOURCE_SQL.into(),
    };
    let column_names = SHOWCASE_ORDER_COLUMNS
        .iter()
        .map(|(name, _, _)| (*name).to_owned())
        .collect::<Vec<_>>();
    let source = crate::query::parse_update_source(SHOWCASE_SOURCE_SQL, &column_names, 2)
        .expect("update source");
    let relation = crate::postgres::metadata::UpdateRelation {
        schema: "public".into(),
        relation: "orders".into(),
        kind: ObjectKind::Table,
        readable: true,
        writable: true,
        columns: showcase_order_columns(),
    };
    let (id, customer, status, amount, notes) = SHOWCASE_ORDER_ROWS[0];
    let plan = crate::query::plan_update(
        &source,
        "public",
        "orders",
        &column_names,
        &[
            Cell::Text(id.into()),
            Cell::Text(customer.into()),
            Cell::Text(status.into()),
            Cell::Text(amount.into()),
            notes.map_or(Cell::Null, |notes| Cell::Text(notes.into())),
        ],
        &column_names
            .iter()
            .map(|name| crate::query::UpdateColumn {
                name: name.clone(),
                primary_key: name == "id",
            })
            .collect::<Vec<_>>(),
        "delivered",
    )
    .expect("update plan");
    model.pending_update = Some(crate::app::model::PendingUpdate {
        candidate,
        relation,
        plan,
    });
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(text.contains("UPDATE"), "reviewed update contains UPDATE");
    assert!(text.contains("orders"), "reviewed update contains orders");
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "reviewed-update",
            "Reviewed update",
            "The review of a generated UPDATE, showing the exact statement before it runs",
        );
    }

    // 7. "workbench-light"
    let model = showcase_base_model(ThemeChoice::Light);
    let buf = render_showcase(&model);
    let text = buffer_to_string(&buf);
    assert!(
        text.contains("Ignatius"),
        "light workbench contains Ignatius"
    );
    assert!(text.contains("[PROD]"), "light workbench contains [PROD]");
    assert!(
        text.contains("Ada Lovelace"),
        "light workbench contains Ada Lovelace"
    );

    let light_pane_bg = model
        .presentation
        .theme
        .color(Token::SurfacePane)
        .expect("SurfacePane color");
    assert_eq!(light_pane_bg, Color::Rgb(0xf4, 0xf6, 0xf9));
    let has_pane_bg = (0..SHOWCASE_AREA.height)
        .any(|y| (0..SHOWCASE_AREA.width).any(|x| buf[(x, y)].bg == light_pane_bg));
    assert!(
        has_pane_bg,
        "light workbench buffer uses SurfacePane RGB as pane background"
    );
    if let Some(ref mut w) = writer {
        capture_showcase_scene(
            w,
            &buf,
            SHOWCASE_AREA,
            "workbench-light",
            "light",
            "The workbench in the light theme",
        );
    }

    showcase_theme_scenes(&mut writer);
    showcase_bench_scene(writer.as_mut());
    showcase_snapshot_scene(writer.as_mut());
    showcase_runbook_scene(writer.as_mut());
}

/// One animated showcase sequence: real buffers in order, each with how long
/// it stays on screen. Consecutive identical buffers merge into one longer
/// frame, which is the same picture held for the same time.
struct MotionSequence {
    name: &'static str,
    studio: &'static str,
    title: &'static str,
    subtitle: &'static str,
    frames: Vec<MotionFrameCapture>,
}

struct MotionFrameCapture {
    buf: Buffer,
    delay_ms: u32,
    poster: bool,
    /// While the theme curtain runs: how many body columns show the
    /// incoming theme, and the incoming and outgoing surface colours, so the
    /// renderer pads the window the way the curtain paints the panes.
    curtain: Option<CurtainPadding>,
}

/// How the renderer pads the window during one theme curtain frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CurtainPadding {
    column: u16,
    incoming: Color,
    outgoing: Color,
}

impl MotionSequence {
    const fn new(
        name: &'static str,
        studio: &'static str,
        title: &'static str,
        subtitle: &'static str,
    ) -> Self {
        Self {
            name,
            studio,
            title,
            subtitle,
            frames: Vec::new(),
        }
    }

    /// Renders `model` as it is now and shows it for `delay_ms`.
    fn push(&mut self, model: &Model, delay_ms: u32) -> Buffer {
        self.push_curtain(model, delay_ms, None)
    }

    fn push_curtain(
        &mut self,
        model: &Model,
        delay_ms: u32,
        curtain: Option<CurtainPadding>,
    ) -> Buffer {
        let buf = render_showcase(model);
        match self.frames.last_mut() {
            Some(last) if last.buf == buf && last.curtain == curtain => {
                last.delay_ms += delay_ms;
            }
            _ => self.frames.push(MotionFrameCapture {
                buf: buf.clone(),
                delay_ms,
                poster: false,
                curtain,
            }),
        }
        buf
    }

    /// Marks the latest frame as the still a reduced-motion reader sees.
    fn mark_poster(&mut self) {
        self.frames.last_mut().expect("a frame to mark").poster = true;
    }

    fn duration(&self) -> Duration {
        Duration::from_millis(
            self.frames
                .iter()
                .map(|frame| u64::from(frame.delay_ms))
                .sum(),
        )
    }

    fn check_bounds(&self) {
        assert!(
            self.frames.iter().filter(|frame| frame.poster).count() == 1,
            "{} has exactly one poster",
            self.name
        );
        for frame in &self.frames {
            assert!(
                (10..=10_000).contains(&frame.delay_ms),
                "{} frame delay {} ms is outside 10..=10000",
                self.name,
                frame.delay_ms
            );
        }
        assert!(
            self.duration() <= Duration::from_millis(6_000),
            "{} lasts {:?}, longer than the six-second budget",
            self.name,
            self.duration()
        );
    }

    fn write(&self, writer: &mut impl Write) {
        for (index, frame) in self.frames.iter().enumerate() {
            let mut line = json!({
                "sequence": self.name,
                "frame": index,
                "delay_ms": frame.delay_ms,
                "studio": self.studio,
                "scene": format!("{}-{index:03}", self.name),
                "title": self.title,
                "subtitle": self.subtitle,
                "width": SHOWCASE_AREA.width,
                "height": SHOWCASE_AREA.height,
                "runs": buffer_runs(&frame.buf, SHOWCASE_AREA),
            });
            if frame.poster {
                line["poster"] = json!(true);
            }
            if let Some(curtain) = frame.curtain {
                // The rails are the first and last rows; the body sweeps
                // between them.
                line["curtain_padding"] = json!({
                    "column": curtain.column,
                    "incoming": format!("{:?}", curtain.incoming),
                    "outgoing": format!("{:?}", curtain.outgoing),
                    "body_rows": [1, SHOWCASE_AREA.height - 1],
                });
            }
            serde_json::to_writer(&mut *writer, &line).unwrap();
            writeln!(writer).unwrap();
        }
    }
}

/// The first cell where `needle` starts in the showcase buffer.
fn showcase_find(buf: &Buffer, needle: &str) -> Option<(u16, u16)> {
    for y in 0..SHOWCASE_AREA.height {
        let mut line = String::new();
        let mut starts = Vec::new();
        for x in 0..SHOWCASE_AREA.width {
            starts.push((line.len(), x));
            line.push_str(buf[(x, y)].symbol());
        }
        if let Some(byte) = line.find(needle)
            && let Some((_, x)) = starts.into_iter().find(|(start, _)| *start == byte)
        {
            return Some((x, y));
        }
    }
    None
}

/// The symbols of one buffer row.
fn showcase_row(buf: &Buffer, y: u16) -> String {
    (0..SHOWCASE_AREA.width)
        .map(|x| buf[(x, y)].symbol())
        .collect()
}

/// The heartbeat row, which sits directly under the running status line.
fn showcase_heartbeat(buf: &Buffer, word: &str) -> (u16, String) {
    let (_, y) = showcase_find(buf, word).unwrap_or_else(|| {
        panic!(
            "missing {word:?} in a running frame:\n{}",
            buffer_to_string(buf)
        )
    });
    (y + 1, showcase_row(buf, y + 1))
}

/// Whether the heartbeat row carries the head in `token`'s ink.
fn heartbeat_head_ink(buf: &Buffer, y: u16, theme: &Theme, token: Token) -> bool {
    let ink = theme.style(token).fg.expect("a truecolour ink");
    (0..SHOWCASE_AREA.width).any(|x| buf[(x, y)].symbol() == "\u{2501}" && buf[(x, y)].fg == ink)
}

/// A statement that runs, ticks honestly and lands with the result settle.
fn motion_run_and_settle() -> MotionSequence {
    let mut sequence = MotionSequence::new(
        "run-and-settle",
        "atelier",
        "atelier",
        "A statement runs with a live clock and heartbeat, then its result settles",
    );
    let mut model = showcase_base_model(ThemeChoice::Atelier);
    model.last_execution = None;
    model.last_elapsed = None;
    model.result_grid.reset_view();
    model.selected_column = 0;
    model.motion.enable();
    assert!(model.allows_motion(), "truecolour Nerd glyphs allow motion");
    let idle = sequence.push(&model, 900);
    assert!(buffer_to_string(&idle).contains("No query has run yet."));

    // Running: every frame is one busy tick, and the clock beside `Running`
    // is the elapsed time the runtime reported for that tick.
    let job = showcase_run(&mut model, SHOWCASE_SQL);
    let tick = Duration::from_millis(90);
    let ticks = 12u32;
    let mut previous_heartbeat = None;
    for count in 0..=ticks {
        if count > 0 {
            update(
                &mut model,
                Message::Tick {
                    running_for: Some(tick * count),
                },
            );
        }
        let buf = sequence.push(&model, 90);
        let text = buffer_to_string(&buf);
        assert!(text.contains("Running"), "running frame {count}:\n{text}");
        let clock = crate::ui::widgets::live_seconds(tick * count);
        assert!(
            text.contains(&clock),
            "frame {count} shows {clock}:\n{text}"
        );
        let (y, heartbeat) = showcase_heartbeat(&buf, "Running");
        assert!(
            heartbeat_head_ink(&buf, y, &model.presentation.theme, Token::Info),
            "the running heartbeat head is in the information ink"
        );
        assert_ne!(
            previous_heartbeat.as_ref(),
            Some(&heartbeat),
            "the heartbeat moves on every tick"
        );
        previous_heartbeat = Some(heartbeat);
        assert!(
            !model.motion().is_active(),
            "a running statement is no transition"
        );
    }

    // The result lands with its settle: the words are final in the first
    // frame and only the status word's ink moves, from success to resting.
    update(
        &mut model,
        Message::ExecutionFinished(Box::new(showcase_execution(job, tick * ticks))),
    );
    assert!(
        model.motion().result_settle().is_some(),
        "a complete success settles"
    );
    let theme = model.presentation.theme.clone();
    let mut settle = Vec::new();
    while model.motion().is_active() {
        settle.push(sequence.push(&model, 33));
        update(
            &mut model,
            Message::MotionFrame {
                elapsed: crate::ui::motion::FRAME_INTERVAL,
            },
        );
    }
    let resting = sequence.push(&model, 2_000);
    sequence.mark_poster();
    let mut still = model.clone();
    still.motion.settle();
    assert_eq!(
        resting,
        render_showcase(&still),
        "the poster is the resting frame"
    );
    assert!(settle.len() >= 10, "the settle has {} frames", settle.len());
    let (x, y) = showcase_find(&resting, "Completed").expect("the status word");
    let success = theme
        .mix(Token::Success, Token::Muted, 0.0)
        .expect("a truecolour ink");
    assert_eq!(
        settle[0][(x, y)].fg,
        success,
        "the settle starts in success ink"
    );
    assert_eq!(
        resting[(x, y)].fg,
        crate::ui::widgets::results::status_word_style(&theme, None)
            .fg
            .expect("a resting ink"),
        "the settle ends in the resting ink"
    );
    for frame in &settle {
        assert_eq!(
            buffer_to_string(frame),
            buffer_to_string(&resting),
            "the words are final from the first settle frame"
        );
    }
    let text = buffer_to_string(&resting);
    assert!(text.contains("14 rows, 1080 ms"), "{text}");
    sequence
}

/// A statement that is cancelled, shown only in the states the product has.
fn motion_cancel_honestly() -> MotionSequence {
    let mut sequence = MotionSequence::new(
        "cancel-honestly",
        "dark",
        "dark",
        "A slow statement is cancelled; the server confirms before the client says so",
    );
    let mut model = showcase_base_model(ThemeChoice::Dark);
    model.last_execution = None;
    model.last_elapsed = None;
    model.result_grid.reset_view();
    model.selected_column = 0;
    let sql = [
        "-- A slow report, cancelled from the keyboard",
        "SELECT customer, sum(amount_cents) AS total_cents",
        "FROM public.orders",
        "CROSS JOIN generate_series(1, 2000000)",
        "GROUP BY customer;",
    ]
    .join("\n");
    model.editor.set_text(&sql);
    model.editor.set_cursor(sql.len());
    model.motion.enable();
    sequence.push(&model, 600);

    let job = showcase_run(&mut model, &sql);
    let tick = Duration::from_millis(90);
    let mut count = 0u32;
    let mut advance = |model: &mut Model, sequence: &mut MotionSequence, word: &str| {
        if count > 0 {
            update(
                model,
                Message::Tick {
                    running_for: Some(tick * count),
                },
            );
        }
        let buf = sequence.push(model, 90);
        let text = buffer_to_string(&buf);
        let clock = crate::ui::widgets::live_seconds(tick * count);
        assert!(text.contains(word), "{word} frame {count}:\n{text}");
        assert!(
            text.contains(&clock),
            "frame {count} shows {clock}:\n{text}"
        );
        count += 1;
        buf
    };
    for _ in 0..=8 {
        let buf = advance(&mut model, &mut sequence, "Running");
        let (y, _) = showcase_heartbeat(&buf, "Running");
        assert!(heartbeat_head_ink(
            &buf,
            y,
            &model.presentation.theme,
            Token::Info
        ));
        assert!(buffer_to_string(&buf).contains("asks the server to cancel."));
    }

    // Cancelling only asks. The heartbeat keeps moving in the warning ink,
    // because the statement may still be running.
    let effects = update(&mut model, Message::Action(Action::Cancel));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Cancel { job: cancelled } if *cancelled == job)),
        "cancelling asks the server to cancel this job: {effects:?}"
    );
    for frame in 0..8 {
        let buf = advance(&mut model, &mut sequence, "Cancellation requested");
        // The still a reduced-motion reader sees is the honest middle: asked,
        // not yet confirmed.
        if frame == 3 {
            sequence.mark_poster();
        }
        let (y, _) = showcase_heartbeat(&buf, "Cancellation requested");
        assert!(heartbeat_head_ink(
            &buf,
            y,
            &model.presentation.theme,
            Token::Warning
        ));
        assert!(
            buffer_to_string(&buf)
                .contains("The server has been asked to stop. It may still be running.")
        );
    }

    // Only the server's answer produces the cancelled state, with the
    // diagnostic `postgres::error::from_query_error` builds for 57014.
    update(&mut model, Message::CancellationDelivered(job));
    let error = Diagnostic::new(
        DiagnosticKind::Cancelled,
        "canceling statement due to user request",
        "running statement 1",
    )
    .in_statement(1)
    .technical("SQLSTATE", "57014")
    .technical("Severity", "ERROR")
    .likely_cause("the statement was cancelled")
    .next_action("nothing was retried; run it again when ready");
    update(
        &mut model,
        Message::ExecutionFinished(Box::new(Execution {
            notice_report: NoticeReport::default(),
            job,
            statements: Vec::new(),
            status: ExecutionStatus::Cancelled,
            elapsed: tick * count,
            error: Some(error),
            transaction: TransactionState::Autocommit,
        })),
    );
    assert!(!model.phase.is_busy());
    assert!(
        !model.motion().is_active(),
        "a cancellation is never faded toward calm"
    );
    let outcome = sequence.push(&model, 2_600);
    let text = buffer_to_string(&outcome);
    for words in [
        "Query cancelled by server",
        "canceling statement due to user request",
    ] {
        assert!(text.contains(words), "missing {words}:\n{text}");
    }
    sequence
}

/// Theme switches, each a real theme curtain stepped on the motion clock.
fn motion_theme_curtain() -> MotionSequence {
    let mut sequence = MotionSequence::new(
        "theme-curtain",
        "dark",
        "Switch theme",
        "Each theme switch sweeps the new palette across the panes behind a hard edge",
    );
    let mut model = showcase_base_model(ThemeChoice::Dark);
    model.motion.enable();
    let hold = 1_100;
    let first = sequence.push(&model, hold);
    // The colour the renderer pads the window with: row 1, column 0.
    let surface = |buf: &Buffer| buf[(0, 1)].bg;
    let rails = [0, SHOWCASE_AREA.height - 1];
    for next in [
        ThemeChoice::Atelier,
        ThemeChoice::Paper,
        ThemeChoice::Material,
        ThemeChoice::Dark,
    ] {
        let outgoing_buf = render_showcase(&model);
        let incoming = Theme::new(next, true).with_depth(ColorDepth::TrueColor);
        let mut settled = model.clone();
        settled.presentation.theme = incoming.clone();
        let incoming_buf = render_showcase(&settled);

        // Atelier to paper is one step of the real cycle, so the reducer's
        // own switch is checked against the route used for every step here.
        let mut via_reducer = (next == ThemeChoice::Paper).then(|| model.clone());

        // The same two steps `switch_theme_action` takes.
        let outgoing = std::mem::replace(&mut model.presentation.theme, incoming);
        assert!(model.allows_motion());
        model.motion.begin_theme_curtain(outgoing);
        if let Some(reducer) = via_reducer.as_mut() {
            assert!(update(reducer, Message::Action(Action::SwitchTheme)).is_empty());
            assert_eq!(reducer.presentation.theme, model.presentation.theme);
            assert_eq!(render_showcase(reducer), render_showcase(&model));
        }

        let mut frames = 0;
        while let Some((_, progress)) = model.motion().theme_curtain() {
            let column = (0..SHOWCASE_AREA.width)
                .filter(|column| {
                    crate::ui::motion::curtain_shows_incoming(
                        *column,
                        SHOWCASE_AREA.width,
                        progress,
                    )
                })
                .count() as u16;
            // Once every column shows the incoming theme the frame is the
            // settled one, padding included.
            let curtain = (column < SHOWCASE_AREA.width).then(|| CurtainPadding {
                column,
                incoming: surface(&incoming_buf),
                outgoing: surface(&outgoing_buf),
            });
            let buf = sequence.push_curtain(&model, 33, curtain);
            for y in 0..SHOWCASE_AREA.height {
                for x in 0..SHOWCASE_AREA.width {
                    let (cell, new, old) =
                        (&buf[(x, y)], &incoming_buf[(x, y)], &outgoing_buf[(x, y)]);
                    assert_eq!(
                        cell.symbol(),
                        new.symbol(),
                        "words never change mid-curtain"
                    );
                    if rails.contains(&y) {
                        assert_eq!(cell, new, "rails take the incoming theme in frame 0");
                    } else if column > 0
                        && column < SHOWCASE_AREA.width
                        && x == column - 1
                        && !cell.symbol().trim().is_empty()
                    {
                        assert_eq!(
                            Some(cell.fg),
                            model.presentation.theme.color(Token::Focus),
                            "truecolour seam cell ({x}, {y}) takes incoming focus ink"
                        );
                        assert_eq!(cell.bg, new.bg);
                        assert_eq!(cell.modifier, new.modifier);
                    } else {
                        let style = (cell.fg, cell.bg, cell.modifier);
                        assert!(
                            style == (new.fg, new.bg, new.modifier)
                                || style == (old.fg, old.bg, old.modifier),
                            "cell ({x}, {y}) is wholly in one theme"
                        );
                    }
                }
            }
            frames += 1;
            update(
                &mut model,
                Message::MotionFrame {
                    elapsed: crate::ui::motion::FRAME_INTERVAL,
                },
            );
        }
        assert!(frames >= 9, "the curtain has {frames} frames");
        assert_eq!(render_showcase(&model), incoming_buf, "the curtain settles");
        if next == ThemeChoice::Dark {
            assert_eq!(incoming_buf, first, "the loop closes on its first frame");
        } else {
            sequence.push(&model, hold);
        }
    }
    sequence.frames[0].poster = true;
    sequence
}

/// The bounded startup boot reveal sweeping the wordmark across while connecting.
fn motion_boot_reveal() -> MotionSequence {
    use crate::app::model::ConnectingStep;
    use crate::ui::widgets::banner::{
        BANNER_TOTAL_TICKS, BannerState, UNICODE_WORDMARK_LINES, unicode_wordmark_tagline,
    };

    let mut sequence = MotionSequence::new(
        "boot-reveal",
        "atelier",
        "Connecting",
        "The wordmark sweeps left to right while connecting and settles on the identity line",
    );
    let mut model = Model::new(100);
    model.size = (SHOWCASE_AREA.width, SHOWCASE_AREA.height);
    model.sidebar_visible = false;
    model.focus = Focus::Results;
    model.splits.zoomed = true;
    model.keymap_snapshot = Keymap::new().snapshot();
    model.presentation = Presentation::new(
        Theme::new(ThemeChoice::Atelier, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    model.motion.enable();
    let target = "ignatius_test@127.0.0.1:55432/ignatius_demo";

    for tick in 0..BANNER_TOTAL_TICKS {
        let step = match tick {
            0..=2 => ConnectingStep::ResolvingProfile,
            3..=5 => ConnectingStep::TlsHandshake,
            6..=9 => ConnectingStep::ServerHandshake,
            _ => ConnectingStep::LoadingCatalogue,
        };
        update(
            &mut model,
            Message::ConnectingStep {
                step,
                target: target.into(),
            },
        );
        assert_eq!(model.banner, BannerState::Revealing { tick });
        let buf = sequence.push(&model, 90);
        let text = buffer_to_string(&buf);
        assert!(
            text.contains(step.label()),
            "boot-reveal tick {tick} missing step label:\n{text}"
        );
        assert!(
            !text.contains(crate::branding::IDENTITY_LINE),
            "boot-reveal tick {tick} hides tagline until Final"
        );
        update(&mut model, Message::Tick { running_for: None });
    }

    assert_eq!(model.banner, BannerState::Final);
    let final_buf = sequence.push(&model, 2_000);
    sequence.mark_poster();
    let final_text = buffer_to_string(&final_buf);
    assert!(
        final_text.contains(UNICODE_WORDMARK_LINES[0]),
        "boot-reveal final frame renders wordmark:\n{final_text}"
    );
    assert!(
        final_text.contains(&unicode_wordmark_tagline()),
        "boot-reveal final frame renders identity tagline:\n{final_text}"
    );
    assert!(
        final_text.contains("loading catalogue"),
        "boot-reveal final frame renders connecting step:\n{final_text}"
    );

    let mut connected = showcase_base_model(ThemeChoice::Atelier);
    enable_showcase_tabs(&mut connected);
    sequence.push(&connected, 1_500);

    sequence
}

/// Format ripple down the editor statement gutter, followed by EXPLAIN ANALYZE plan micro-bar sweep.
fn motion_format_and_plan() -> MotionSequence {
    let mut sequence = MotionSequence::new(
        "format-and-plan",
        "atelier",
        "Format & plan",
        "Formatting ripples down the editor statement gutter, then EXPLAIN ANALYZE sweeps plan micro-bars",
    );
    let mut model = showcase_development_model(ThemeChoice::Atelier);
    enable_showcase_tabs(&mut model);
    model.focus = Focus::Editor;
    model.motion.enable();
    let unformatted = [
        "-- Paid orders with customer region",
        "select o.id, c.name, c.region, o.amount_cents from public.orders o join public.customers c on c.name = o.customer where o.status in ('paid', 'pending') order by o.amount_cents desc;",
    ]
    .join("\n");
    model.editor.set_text(&unformatted);
    model.editor.set_cursor(unformatted.len());
    sequence.push(&model, 550);

    // 1. Format the SQL buffer via the reducer and capture the gutter FormatRipple.
    assert!(update(&mut model, Message::Action(Action::FormatBuffer)).is_empty());
    assert!(
        model.motion().format_ripple().is_some(),
        "formatting changed SQL starts FormatRipple"
    );
    let mut ripple_frames = 0;
    while model.motion().is_active() {
        sequence.push(&model, 33);
        ripple_frames += 1;
        update(
            &mut model,
            Message::MotionFrame {
                elapsed: crate::ui::motion::FRAME_INTERVAL,
            },
        );
    }
    assert!(
        ripple_frames >= 8,
        "format ripple has {ripple_frames} frames"
    );
    let formatted_buf = sequence.push(&model, 500);
    let formatted_text = buffer_to_string(&formatted_buf);
    assert!(
        formatted_text.contains("select o.id")
            && formatted_text.contains("Formatted SQL: 2 lines to 10"),
        "formatted SQL appears in editor:\n{formatted_text}"
    );

    // 2. Load an EXPLAIN ANALYZE plan through the reducer and capture PlanSweep.
    let _ = update(&mut model, Message::Action(Action::AnalyzePlan));
    assert!(model.pending_plan.is_some());
    // Type the database name to confirm analyzed plan execution on the dev target.
    for ch in "ignatius_demo".chars() {
        update(&mut model, Message::Action(Action::Insert(ch)));
    }
    let effects = update(&mut model, Message::Action(Action::Activate));
    let [Effect::Explain { job, analyze, .. }] = effects.as_slice() else {
        panic!("expected Effect::Explain, got {effects:?}");
    };
    let (job, analyzed) = (*job, *analyze);
    update(
        &mut model,
        Message::PlanFinished(Box::new(crate::app::PlanExecution {
            job,
            analyzed,
            elapsed: Duration::from_millis(13),
            transaction: TransactionState::Autocommit,
            result: Ok(showcase_plan_document()),
            connection_lost: false,
            notice_report: NoticeReport::default(),
        })),
    );
    model.plan.selected_path = vec![0, 0];
    assert!(
        model.motion().plan_sweep().is_some(),
        "clean plan load starts PlanSweep"
    );
    let mut sweep_frames = Vec::new();
    while model.motion().is_active() {
        sweep_frames.push(sequence.push(&model, 33));
        update(
            &mut model,
            Message::MotionFrame {
                elapsed: crate::ui::motion::FRAME_INTERVAL,
            },
        );
    }
    assert!(
        sweep_frames.len() >= 8,
        "plan sweep has {} frames",
        sweep_frames.len()
    );
    let settled = sequence.push(&model, 1_800);
    sequence.mark_poster();
    let settled_text = buffer_to_string(&settled);
    assert!(
        settled_text.contains("Attention: Sort (measured time)")
            && settled_text.contains("Nodes")
            && settled_text.contains('\u{2501}'),
        "settled plan explorer shows KPI strip, bottleneck, and micro-bars:\n{settled_text}"
    );
    sequence
}

/// Browsing Theme entries in the floating Spotlight palette previews each palette live before switching.
fn motion_palette_themes() -> MotionSequence {
    use crate::app::message::Direction;

    let mut sequence = MotionSequence::new(
        "palette-themes",
        "atelier",
        "Theme previews",
        "Browsing Theme entries in the Spotlight palette previews each palette live before switching",
    );
    let mut model = showcase_base_model(ThemeChoice::Atelier);
    enable_showcase_tabs(&mut model);
    model.motion.enable();
    let surface = |buf: &Buffer| buf[(0, 1)].bg;

    let first = sequence.push(&model, 450);

    // Open the Spotlight palette and filter to "Theme: ".
    assert!(update(&mut model, Message::Action(Action::OpenPalette)).is_empty());
    for ch in "Theme: ".chars() {
        assert!(update(&mut model, Message::Action(Action::Insert(ch))).is_empty());
        sequence.push(&model, 60);
    }

    let move_to_theme = |model: &mut Model, target: &str| {
        let palette = model.palette.as_ref().expect("palette open");
        let target_idx = palette
            .matches()
            .iter()
            .position(|entry| entry.label == target)
            .unwrap_or_else(|| panic!("theme entry {target:?} not found in palette"));
        let current = palette.selected;
        if target_idx > current {
            for _ in 0..(target_idx - current) {
                update(model, Message::Action(Action::Move(Direction::Down)));
            }
        } else {
            for _ in 0..(current - target_idx) {
                update(model, Message::Action(Action::Move(Direction::Up)));
            }
        }
    };

    // Browse field-industrial -> material -> atelier -> paper live previews.
    move_to_theme(&mut model, "Theme: field-industrial");
    let field_preview = sequence.push(&model, 420);
    assert!(buffer_to_string(&field_preview).contains("Theme: field-industrial"));

    move_to_theme(&mut model, "Theme: material");
    let material_preview = sequence.push(&model, 420);
    assert!(buffer_to_string(&material_preview).contains("Theme: material"));

    move_to_theme(&mut model, "Theme: atelier");
    let atelier_preview = sequence.push(&model, 420);
    assert!(buffer_to_string(&atelier_preview).contains("Theme: atelier"));

    move_to_theme(&mut model, "Theme: paper");
    let paper_preview = sequence.push(&model, 650);
    sequence.mark_poster();
    let paper_preview_text = buffer_to_string(&paper_preview);
    assert!(
        paper_preview_text.contains("Theme: paper") && paper_preview_text.contains("Swatches:"),
        "palette shows live mini-workbench preview for paper:\n{paper_preview_text}"
    );

    // Confirm Theme: paper to trigger a live ThemeCurtain transition.
    let outgoing_buf = render_showcase(&model);
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    let mut settled_paper = model.clone();
    settled_paper.motion.settle();
    let incoming_paper_buf = render_showcase(&settled_paper);
    let mut curtain_frames = 0;
    while let Some((_, progress)) = model.motion().theme_curtain() {
        let column = (0..SHOWCASE_AREA.width)
            .filter(|col| {
                crate::ui::motion::curtain_shows_incoming(*col, SHOWCASE_AREA.width, progress)
            })
            .count() as u16;
        let curtain = (column < SHOWCASE_AREA.width).then(|| CurtainPadding {
            column,
            incoming: surface(&incoming_paper_buf),
            outgoing: surface(&outgoing_buf),
        });
        sequence.push_curtain(&model, 33, curtain);
        curtain_frames += 1;
        update(
            &mut model,
            Message::MotionFrame {
                elapsed: crate::ui::motion::FRAME_INTERVAL,
            },
        );
    }
    assert!(curtain_frames >= 8, "curtain has {curtain_frames} frames");
    sequence.push(&model, 800);

    // Switch back to atelier via palette + ThemeCurtain so the loop closes cleanly.
    assert!(update(&mut model, Message::Action(Action::OpenPalette)).is_empty());
    for ch in "Theme: atelier".chars() {
        assert!(update(&mut model, Message::Action(Action::Insert(ch))).is_empty());
    }
    let outgoing_paper = render_showcase(&model);
    assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
    while let Some((_, progress)) = model.motion().theme_curtain() {
        let column = (0..SHOWCASE_AREA.width)
            .filter(|col| {
                crate::ui::motion::curtain_shows_incoming(*col, SHOWCASE_AREA.width, progress)
            })
            .count() as u16;
        let curtain = (column < SHOWCASE_AREA.width).then(|| CurtainPadding {
            column,
            incoming: surface(&first),
            outgoing: surface(&outgoing_paper),
        });
        sequence.push_curtain(&model, 33, curtain);
        update(
            &mut model,
            Message::MotionFrame {
                elapsed: crate::ui::motion::FRAME_INTERVAL,
            },
        );
    }
    assert_eq!(
        render_showcase(&model),
        first,
        "palette-themes loop closes cleanly"
    );
    sequence.push(&model, 450);

    sequence
}

#[test]
fn motion_showcase_sequences_are_real_buffers_at_fixed_motion_times() {
    let sequences = [
        motion_boot_reveal(),
        motion_run_and_settle(),
        motion_cancel_honestly(),
        motion_theme_curtain(),
        motion_format_and_plan(),
        motion_palette_themes(),
    ];
    for sequence in &sequences {
        sequence.check_bounds();
    }
    if let Some(path) = std::env::var_os("IGNATIUS_SHOWCASE_CAPTURE_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        let mut writer = BufWriter::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path.join("motion.jsonl"))
                .unwrap(),
        );
        for sequence in &sequences {
            sequence.write(&mut writer);
        }
        writer.flush().unwrap();
    }
}
