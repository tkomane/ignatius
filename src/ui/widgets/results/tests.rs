//! Focused tests for the Results widget.
//!
//! These carry the grid contract's pinned wording shapes, the local sort state,
//! the column-width bounds, and the rule that NULL, an empty string and the
//! literal text `NULL` stay distinguishable by words at every colour depth.

use super::render_results;
use crate::app::grid;
use crate::app::model::{Focus, Model};
use crate::config::ThemeChoice;
use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
use crate::query::value::Cell;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use std::time::Duration;

fn presentation_at(depth: ColorDepth) -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(depth),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

fn model_with(columns: &[&str], rows: &[Vec<Cell>]) -> Model {
    let mut set = ResultSet::new(columns.iter().map(|name| (*name).to_owned()).collect(), 100);
    for row in rows {
        assert!(set.push(row.clone()));
    }
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.focus = Focus::Results;
    model.last_execution = Some(Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(rows.len() as u64),
            elapsed: Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    });
    model
}

fn render(model: &Model, presentation: &Presentation, area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    render_results(model, &Keymap::new(), presentation, area, &mut buf);
    buf
}

fn text_rows(count: usize) -> Vec<Vec<Cell>> {
    (0..count)
        .map(|index| vec![Cell::Text(format!("r{index}"))])
        .collect()
}

#[test]
fn a_pane_with_no_room_for_a_data_row_states_no_range() {
    let model = model_with(&["value"], &text_rows(30));
    let presentation = presentation_at(ColorDepth::TrueColor);
    // Four inner rows cannot carry the five prelude rows and a data row, so the
    // range must be absent rather than claiming a row that was never drawn.
    let buf = render(&model, &presentation, Rect::new(0, 0, 60, 6));
    let text = buffer_to_string(&buf);
    assert!(
        !text.contains("rows 1-"),
        "a range was stated for a pane with no data row: {text}"
    );
    assert!(!text.contains("rows 0-0"), "{text}");
}

#[test]
fn the_viewport_range_line_states_the_rows_actually_rendered() {
    let mut model = model_with(&["value"], &text_rows(30));
    let area = Rect::new(0, 0, 60, 20);
    let presentation = presentation_at(ColorDepth::TrueColor);

    // 20 rows of area less two borders is 18; five prelude lines plus the
    // 1-line quick-stats footer leave 12 rendered rows starting at the top.
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("rows 1-12 of 30 retained"), "{text}");

    // Moving the selection one row past the fold scrolls the viewport.
    model.selected_row = 20;
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("rows 10-21 of 30 retained"), "{text}");

    // Hiding quick-stats restores the 13th row.
    model.selected_row = 0;
    model.result_grid.show_quick_stats = false;
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("rows 1-13 of 30 retained"), "{text}");
}

#[test]
fn a_truncated_result_states_what_the_server_returned() {
    let mut model = model_with(&["value"], &text_rows(30));
    if let Some(set) = model
        .last_execution
        .as_mut()
        .and_then(|execution| execution.statements[0].result_set.as_mut())
    {
        set.rows_seen = 200_000;
        set.cap = 30;
    }
    let text = buffer_to_string(&render(
        &model,
        &presentation_at(ColorDepth::TrueColor),
        Rect::new(0, 0, 80, 20),
    ));
    assert!(
        text.contains("rows 1-12 of 30 retained, of 200,000 returned"),
        "{text}"
    );
}

#[test]
fn a_filtered_result_names_the_match_count_without_hiding_retention() {
    let mut model = model_with(&["value"], &text_rows(30));
    model.result_filter = "r1".into();
    let matching = model.filtered_rows().len();
    assert!(
        matching > 0 && matching < 30,
        "the fixture narrows the result"
    );
    let text = buffer_to_string(&render(
        &model,
        &presentation_at(ColorDepth::TrueColor),
        Rect::new(0, 0, 80, 20),
    ));
    assert!(
        text.contains(&format!(
            "rows 1-{matching} matching {matching} of 30 retained"
        )),
        "{text}"
    );
}

#[test]
fn structured_filter_unknown_column_renders_warning_without_silently_falling_back() {
    let mut model = model_with(
        &["status", "amount"],
        &[vec![Cell::Text("paid".into()), Cell::Text("100".into())]],
    );
    model.result_filter = "missing_col:=paid".into();
    assert!(model.filtered_rows().is_empty());
    let text = buffer_to_string(&render(
        &model,
        &presentation_at(ColorDepth::TrueColor),
        Rect::new(0, 0, 120, 20),
    ));
    assert!(
        text.contains("unknown filter column: missing_col"),
        "{text}"
    );
    assert!(
        text.contains("Unknown filter column (missing_col)"),
        "{text}"
    );
}

#[test]
fn an_empty_result_keeps_its_wording_and_never_says_rows_0_0() {
    let model = model_with(&["value"], &[]);
    let text = buffer_to_string(&render(
        &model,
        &presentation_at(ColorDepth::TrueColor),
        Rect::new(0, 0, 80, 20),
    ));
    assert!(text.contains("No rows returned"), "{text}");
    assert!(!text.contains("rows 0-0"), "{text}");
    assert!(!text.contains("0 retained"), "{text}");
}

#[test]
fn the_sort_state_is_stated_in_words() {
    let mut model = model_with(
        &["id", "amount"],
        &[
            vec![Cell::Text("1".into()), Cell::Text("30".into())],
            vec![Cell::Text("2".into()), Cell::Text("10".into())],
        ],
    );
    model.selected_column = 1;
    let area = Rect::new(0, 0, 80, 20);
    let presentation = presentation_at(ColorDepth::TrueColor);

    model.result_grid.toggle_sort(1);
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("sorted by amount ascending"), "{text}");

    model.result_grid.toggle_sort(1);
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("sorted by amount descending"), "{text}");

    model.result_grid.toggle_sort(1);
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("order: server"), "{text}");
    assert!(!text.contains("sorted by"), "{text}");
}

#[test]
fn rendered_column_widths_respect_the_bounds_before_padding() {
    let model = model_with(
        &["col1", "col2"],
        &[vec![Cell::Text("x".into()), Cell::Text("y".into())]],
    );
    let area = Rect::new(0, 0, 60, 20);
    let presentation = presentation_at(ColorDepth::TrueColor);

    let mut wide = model.clone();
    wide.result_grid
        .widths
        .insert(0, grid::MAX_COLUMN_WIDTH + 40);
    let text = buffer_to_string(&render(&wide, &presentation, area));
    assert!(text.contains("col1"), "{text}");
    assert!(
        !text.contains("col2"),
        "a maximum-width column leaves no room for its neighbour: {text}"
    );

    let mut narrow = model.clone();
    narrow.result_grid.widths.insert(0, 1);
    let text = buffer_to_string(&render(&narrow, &presentation, area));
    assert!(text.contains("col1"), "{text}");
    assert!(
        text.contains("col2"),
        "a minimum-width column leaves room for its neighbour: {text}"
    );
}

#[test]
fn null_empty_and_literal_null_stay_distinct_on_stripes_at_every_depth() {
    let area = Rect::new(0, 0, 60, 20);
    for (depth, stripes_are_painted) in [
        (ColorDepth::TrueColor, true),
        (ColorDepth::Indexed256, true),
        (ColorDepth::Basic16, false),
        (ColorDepth::None, false),
    ] {
        let presentation = presentation_at(depth);
        let stripe = presentation.theme.surface(Token::SurfaceAlt).bg;

        for (value, kind) in [
            (Cell::Null, "null"),
            (Cell::Text(String::new()), "empty"),
            (Cell::Text("NULL".into()), "literal"),
        ] {
            let model = model_with(
                &["marker", "value"],
                &[
                    vec![Cell::Text("header".into()), Cell::Text("first".into())],
                    vec![Cell::Text("stripe-row".into()), value],
                ],
            );
            let buf = render(&model, &presentation, area);
            let text = buffer_to_string(&buf);
            let summary = format!("{depth:?} {kind}");
            let line = text
                .lines()
                .find(|line| line.contains("stripe-row"))
                .unwrap_or_else(|| panic!("{summary}: the striped result row is missing: {text}"));

            // Words carry the distinction, not colour.
            assert_eq!(
                line.contains("[null]"),
                kind == "null",
                "{summary}: {line:?}"
            );
            assert_eq!(
                line.contains("NULL"),
                kind == "literal",
                "{summary}: {line:?}"
            );

            // Stripes are reinforcement only: painted where the depth paints,
            // and dropped rather than faked where it must not.
            let y = text
                .lines()
                .position(|line| line.contains("stripe-row"))
                .expect("the striped row") as u16;
            let painted = stripe.map_or(0, |colour| {
                (area.x..area.x + area.width)
                    .filter(|x| buf[(*x, y)].bg == colour)
                    .count()
            });
            if stripes_are_painted {
                assert!(painted > 0, "{summary}: the striped row carries no stripe");
            } else {
                assert_eq!(
                    painted, 0,
                    "{summary}: a stripe was faked where it is dropped"
                );
            }
        }
    }
}

fn expanded_presentations(color: bool) -> Vec<Presentation> {
    let mut presentations = Vec::new();
    for choice in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
    ] {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Indexed256,
            ColorDepth::Basic16,
            ColorDepth::None,
        ] {
            for glyphs in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
                for density in [
                    crate::config::Density::Comfortable,
                    crate::config::Density::Compact,
                ] {
                    presentations.push(
                        Presentation::new(
                            Theme::new(choice, color).with_depth(depth),
                            Glyphs::new(glyphs),
                            false,
                        )
                        .with_density(density),
                    );
                }
            }
        }
    }
    presentations
}

fn expanded_selection_model() -> Model {
    let mut model = model_with(
        &[
            "sql_null",
            "empty_text",
            "literal_null",
            "plain_text",
            "control",
        ],
        &[vec![
            Cell::Null,
            Cell::Text(String::new()),
            Cell::Text("NULL".into()),
            Cell::Text("value".into()),
            Cell::Text("control-value".into()),
        ]],
    );
    model.expanded_row = true;
    model
}

fn expanded_field_row(buf: &Buffer, name: &str) -> u16 {
    (0..buf.area.height)
        .find(|y| {
            let text: String = (0..buf.area.width).map(|x| buf[(x, *y)].symbol()).collect();
            text.contains(name)
        })
        .unwrap_or_else(|| panic!("missing expanded field {name}: {}", buffer_to_string(buf)))
}

fn expanded_text_column(buf: &Buffer, y: u16, text: &str) -> u16 {
    (0..buf.area.width)
        .find(|x| {
            let suffix: String = (*x..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            suffix.starts_with(text)
        })
        .unwrap_or_else(|| {
            panic!(
                "missing {text:?} in expanded row {y}: {}",
                buffer_to_string(buf)
            )
        })
}

#[test]
fn selected_expanded_fields_use_active_ink_and_keep_modifiers_across_presentations() {
    for presentation in expanded_presentations(true) {
        let summary = format!(
            "{:?}/{:?}/{:?}/{:?}",
            presentation.theme.selection(),
            presentation.color_depth,
            presentation.glyphs.tier,
            presentation.density
        );
        let mut model = expanded_selection_model();
        model.selected_column = 4;
        let ordinary = render(&model, &presentation, Rect::new(0, 0, 80, 12));
        for (column, name, value, value_token) in [
            (0, "sql_null", "[null]", Token::NullValue),
            (1, "empty_text", "", Token::Text),
            (2, "literal_null", "NULL", Token::Text),
            (3, "plain_text", "value", Token::Text),
        ] {
            model.selected_column = column;
            let selected = render(&model, &presentation, ordinary.area);
            let y = expanded_field_row(&selected, name);
            assert_eq!(y, expanded_field_row(&ordinary, name), "{summary}");
            let arrow = if presentation.glyphs.is_ascii() {
                ">"
            } else {
                "▸"
            };
            let mut samples = vec![
                (expanded_text_column(&selected, y, name), Token::Header),
                (expanded_text_column(&selected, y, arrow), Token::Focus),
            ];
            if !value.is_empty() {
                samples.push((expanded_text_column(&selected, y, value), value_token));
            }
            let selection = presentation.theme.style(Token::Selection);
            for (x, token) in samples {
                let old = &ordinary[(x, y)];
                let actual = &selected[(x, y)];
                let token_style = presentation.theme.style(token);
                assert_eq!(
                    old.fg,
                    token_style.fg.unwrap_or(Color::Reset),
                    "ordinary {summary}/{name}/{token:?}"
                );
                assert!(
                    old.modifier.contains(token_style.add_modifier),
                    "ordinary {summary}/{name}/{token:?}"
                );
                assert!(
                    !old.modifier.contains(Modifier::REVERSED),
                    "ordinary {summary}/{name}/{token:?}"
                );
                assert_eq!(
                    actual.fg,
                    selection.fg.unwrap_or(old.fg),
                    "selected ink {summary}/{name}/{token:?} at({x},{y})"
                );
                assert_eq!(
                    actual.bg,
                    selection.bg.unwrap_or(old.bg),
                    "selected fill {summary}/{name}/{token:?} at({x},{y})"
                );
                assert_eq!(
                    actual.modifier,
                    old.modifier | selection.add_modifier,
                    "selected modifiers {summary}/{name}/{token:?}"
                );
            }
            if matches!(
                presentation.color_depth,
                ColorDepth::Basic16 | ColorDepth::None
            ) {
                assert!(
                    selected.content.iter().all(|cell| cell.bg == Color::Reset),
                    "painted background at {summary}"
                );
            }
        }
    }
}

#[test]
fn selected_expanded_fields_with_colour_disabled_emit_no_colours() {
    for presentation in expanded_presentations(false) {
        let mut model = expanded_selection_model();
        for column in [0, 3] {
            model.selected_column = column;
            let buf = render(&model, &presentation, Rect::new(0, 0, 80, 12));
            assert!(
                buf.content
                    .iter()
                    .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
            );
            let name = if column == 0 {
                "sql_null"
            } else {
                "plain_text"
            };
            let y = expanded_field_row(&buf, name);
            let x = expanded_text_column(&buf, y, name);
            assert!(
                buf[(x, y)]
                    .modifier
                    .contains(Modifier::REVERSED | Modifier::BOLD)
            );
            let value = if column == 0 { "[null]" } else { "value" };
            let x = expanded_text_column(&buf, y, value);
            assert!(buf[(x, y)].modifier.contains(Modifier::REVERSED));
            assert_eq!(buf[(x, y)].modifier.contains(Modifier::DIM), column == 0);
        }
    }
}

#[test]
fn expanded_selection_preserves_null_empty_and_literal_null_words_across_presentations() {
    for presentation in expanded_presentations(true) {
        let mut model = expanded_selection_model();
        for column in 0..4 {
            model.selected_column = column;
            let buf = render(&model, &presentation, Rect::new(0, 0, 80, 12));
            let row = |name| {
                let y = expanded_field_row(&buf, name);
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            };
            assert!(row("sql_null").contains("[null]"));
            let empty = row("empty_text");
            assert!(
                !empty.contains("[null]") && !empty.contains("NULL") && !empty.contains("value")
            );
            assert!(row("literal_null").contains("NULL"));
            assert!(!row("literal_null").contains("[null]"));
            assert!(row("plain_text").contains("value"));
        }
    }
}

#[test]
fn header_row_elevated_with_stripe_surface_at_depths_that_paint() {
    let model = model_with(
        &["id", "name"],
        &[vec![Cell::Text("1".into()), Cell::Text("alice".into())]],
    );
    for depth in [ColorDepth::TrueColor, ColorDepth::Indexed256] {
        let presentation = presentation_at(depth);
        let area = Rect::new(0, 0, 60, 10);
        let buf = render(&model, &presentation, area);
        let header_row_y = (0..area.height)
            .find(|&y| {
                let row_str = (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>();
                row_str.contains("id") && row_str.contains("name")
            })
            .expect("header row must exist");
        let stripe_bg = presentation
            .theme
            .stripe()
            .bg
            .expect("stripe background must exist");
        let selection_bg = presentation
            .theme
            .color(Token::Selection)
            .expect("selection color");
        let id_x = (0..area.width)
            .find(|&x| buf[(x, header_row_y)].symbol() == "i")
            .expect("column id must be present");
        let name_x = (0..area.width)
            .find(|&x| buf[(x, header_row_y)].symbol() == "n")
            .expect("column name must be present");
        assert_eq!(
            buf[(id_x, header_row_y)].bg,
            selection_bg,
            "selected column header at depth {depth:?} must have selection background"
        );
        assert_eq!(
            buf[(name_x, header_row_y)].bg,
            stripe_bg,
            "unselected column header at depth {depth:?} must have stripe background"
        );
        assert_eq!(
            buf[(1, header_row_y)].bg,
            stripe_bg,
            "gutter cell at depth {depth:?} must have stripe background"
        );
    }
}

#[test]
fn styled_toolbar_line_delimiters_and_accents() {
    let theme = Theme::new(ThemeChoice::Dark, true);
    let order_text = "rows 1..10 of 100 | natural database order | selected column 1: id";
    let line = super::styled_toolbar_line(order_text, Token::Info, Some("selected column"), &theme);
    let reconstructed: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(reconstructed, order_text);

    let border_style = theme.style(Token::Border);
    let focus_style = theme.style(Token::Focus);
    let info_style = theme.style(Token::Info);

    let delimiter_spans: Vec<_> = line
        .spans
        .iter()
        .filter(|s| s.content.as_ref() == " | ")
        .collect();
    assert_eq!(delimiter_spans.len(), 2);
    for d in delimiter_spans {
        assert_eq!(d.style, border_style);
    }

    let accent_span = line
        .spans
        .iter()
        .find(|s| s.content.as_ref().starts_with("selected column"))
        .expect("accent span");
    assert_eq!(accent_span.style, focus_style);

    let state_text = "shown 2/2 columns | types: hidden | freeze first: off | g grid controls";
    let state_line = super::styled_toolbar_line(state_text, Token::Muted, None, &theme);
    let reconstructed_state: String = state_line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(reconstructed_state, state_text);

    let controls_span = state_line
        .spans
        .iter()
        .find(|s| s.content.as_ref().contains("controls"))
        .expect("controls span");
    assert_eq!(controls_span.style, info_style);
}

#[test]
fn error_view_renders_multi_span_diagnostic_labels_and_preserves_words() {
    use crate::diagnostics::{Diagnostic, DiagnosticKind, ObjectContext};
    use crate::query::error_location::ErrorLocation;

    let presentation = presentation_at(ColorDepth::TrueColor);
    let mut model = Model::new(10);
    let mut error = Diagnostic::new(DiagnosticKind::Query, "syntax error", "executing buffer");
    error.object = Some(ObjectContext {
        schema: Some("public".into()),
        table: Some("users".into()),
        column: None,
        constraint: None,
    });
    model.error_location = Some(ErrorLocation {
        cursor: 10,
        token: None,
        line: 1,
        column: 5,
        statement_number: 1,
        character: 10,
    });
    error.likely_cause = Some("missing semicolon".into());
    error.next_action = Some("add semicolon at end".into());

    let area = Rect::new(0, 0, 80, 20);
    let mut buf = Buffer::empty(area);
    super::render_error(&model, &error, &presentation, area, &mut buf);

    let text = buffer_to_string(&buf);
    assert!(text.contains("While: executing buffer"));
    assert!(text.contains("Object context: relation \"public\".\"users\""));
    assert!(text.contains("Catalogue: "));
    assert!(text.contains("Location: statement 1, line 1, column 5"));
    assert!(text.contains("Likely cause: missing semicolon"));
    assert!(text.contains("Next: add semicolon at end"));

    let find_word_cell = |word: &str| -> (u16, u16) {
        for y in 0..area.height {
            let row_str: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(pos) = row_str.find(word) {
                return (pos as u16, y);
            }
        }
        panic!("could not find {word} in buffer: {text}");
    };

    // "While: " -> Token::Header (Bold)
    let (while_x, while_y) = find_word_cell("While:");
    assert!(
        buf[(while_x, while_y)].modifier.contains(Modifier::BOLD),
        "While: must be Header (bold)"
    );

    // "Object context: " -> Token::Header (Bold)
    let (obj_x, obj_y) = find_word_cell("Object context:");
    assert!(
        buf[(obj_x, obj_y)].modifier.contains(Modifier::BOLD),
        "Object context: must be Header (bold)"
    );

    // "Location: " -> Token::Header (Bold)
    let (loc_x, loc_y) = find_word_cell("Location:");
    assert!(
        buf[(loc_x, loc_y)].modifier.contains(Modifier::BOLD),
        "Location: must be Header (bold)"
    );

    // "Next: " -> Token::Header (Bold)
    let (next_x, next_y) = find_word_cell("Next:");
    assert!(
        buf[(next_x, next_y)].modifier.contains(Modifier::BOLD),
        "Next: must be Header (bold)"
    );

    // "Catalogue: " -> Token::Header (Bold), like every other fact label
    let header_fg = presentation.theme.color(Token::Header).unwrap();
    let (cat_x, cat_y) = find_word_cell("Catalogue:");
    assert_eq!(
        buf[(cat_x, cat_y)].fg,
        header_fg,
        "Catalogue: must have Header fg"
    );
    assert!(buf[(cat_x, cat_y)].modifier.contains(Modifier::BOLD));

    // "Likely cause: " -> Token::Warning
    let warn_fg = presentation.theme.color(Token::Warning).unwrap();
    let (cause_x, cause_y) = find_word_cell("Likely cause:");
    assert_eq!(
        buf[(cause_x, cause_y)].fg,
        warn_fg,
        "Likely cause: must have Warning fg"
    );
}

#[test]
fn structured_cell_detail_renders_selection_and_path_in_ascii() {
    let mut model = model_with(
        &["payload"],
        &[vec![Cell::Text(r#"{"x":[null,2]}"#.to_owned())]],
    );
    let set = model.last_execution.as_mut().unwrap().statements[0]
        .result_set
        .as_mut()
        .unwrap();
    set.set_structured_columns(vec![Some(crate::structured::Column {
        kind: crate::structured::Kind::Jsonb,
        partial_edit: true,
        origin: None,
    })]);
    let mut inspector = crate::app::inspect::Inspector::new();
    inspector.move_node(true, 2, 4);
    let area = Rect::new(0, 0, 60, 16);
    let presentation = Presentation::new(
        Theme::new(ThemeChoice::Material, true).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut buf = Buffer::empty(area);
    super::render_inspector(&model, inspector, &presentation, area, &mut buf);
    let actual = buffer_to_string(&buf);
    assert!(actual.is_ascii(), "{actual}");
    assert!(actual.contains("Cell Detail"), "{actual}");
    assert!(actual.contains("[0]  null"), "{actual}");
    assert!(actual.contains(r#"Path $ / "x" / [0]"#), "{actual}");
    assert!(!actual.contains("e Edit"), "{actual}");

    inspector.toggle_raw();
    let mut buf = Buffer::empty(area);
    super::render_inspector(&model, inspector, &presentation, area, &mut buf);
    let actual = buffer_to_string(&buf);
    assert!(actual.contains("Server text"), "{actual}");
    assert!(actual.contains(r#"{"x":[null,2]}"#), "{actual}");
}

#[test]
fn structured_cell_detail_draws_border_guides_semantic_tokens_and_striped_path() {
    let mut model = model_with(
        &["payload"],
        &[vec![Cell::Text(r#"{"x":[null,"ok"]}"#.into())]],
    );
    let set = model.last_execution.as_mut().unwrap().statements[0]
        .result_set
        .as_mut()
        .unwrap();
    set.set_structured_columns(vec![Some(crate::structured::Column {
        kind: crate::structured::Kind::Jsonb,
        partial_edit: true,
        origin: None,
    })]);
    let mut inspector = crate::app::inspect::Inspector::new();
    // Move selection to `"ok"` so `x` (depth 1) and `[0]` (`null`, depth 2) are unselected.
    inspector.move_node(true, 3, 4);

    let area = Rect::new(0, 0, 76, 14);
    let truecolor = presentation_at(ColorDepth::TrueColor);
    let mut buf = Buffer::empty(area);
    super::render_inspector(&model, inspector, &truecolor, area, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("Cell Detail"), "{rendered}");

    let cell_of = |needle: &str| -> (u16, u16) {
        for y in 0..area.height {
            let line: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, y);
            }
        }
        panic!("{needle:?} missing:\n{rendered}");
    };

    // Split title puts Cell Detail in bold Focus ink.
    let title_pos = cell_of("Cell Detail");
    assert_eq!(
        buf[title_pos].fg,
        truecolor.theme.color(Token::Focus).unwrap()
    );
    assert!(buf[title_pos].modifier.contains(Modifier::BOLD));

    // Nested unselected row `│ │ [0]  null` uses Border guide, Info key, and NullValue ink.
    let null_key = cell_of("[0]");
    assert_eq!(
        buf[null_key].fg,
        truecolor.theme.color(Token::Info).unwrap()
    );
    let guide_x = null_key.0 - 2;
    assert_eq!(buf[(guide_x, null_key.1)].symbol(), "\u{2502}");
    assert_eq!(
        buf[(guide_x, null_key.1)].fg,
        truecolor.theme.color(Token::Border).unwrap()
    );
    let null_val = cell_of("null");
    assert_eq!(
        buf[null_val].fg,
        truecolor.theme.color(Token::NullValue).unwrap()
    );

    // Path footer paints SurfaceAlt strip across the entire inner modal width.
    let box_area = crate::ui::layout::inspector_box(area, &truecolor);
    let path_pos = cell_of("Path ");
    let stripe_bg = truecolor.theme.surface(Token::SurfaceAlt).bg.unwrap();
    for x in box_area.x + 1..box_area.right() - 1 {
        assert_eq!(
            buf[(x, path_pos.1)].bg,
            stripe_bg,
            "Path strip column {x} should carry SurfaceAlt background"
        );
    }

    // Basic16 and None drop the SurfaceAlt background cleanly while preserving words.
    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = presentation_at(depth);
        let low_box = crate::ui::layout::inspector_box(area, &low);
        let mut lbuf = Buffer::empty(area);
        super::render_inspector(&model, inspector, &low, area, &mut lbuf);
        assert!(buffer_to_string(&lbuf).contains("Path $"));
        assert_eq!(lbuf[(low_box.x + 2, path_pos.1)].bg, Color::Reset);
    }
}

#[test]
fn results_header_row_fills_grid_width_gutter_uses_muted_and_empty_hints_stay_crisp() {
    let model = model_with(
        &["id", "note"],
        &[
            vec![Cell::Text("1".into()), Cell::Text("ok".into())],
            vec![Cell::Text("2".into()), Cell::Text("no".into())],
        ],
    );
    let area = Rect::new(0, 0, 60, 14);

    let pres = presentation_at(ColorDepth::TrueColor);
    let buf = render(&model, &pres, area);
    let stripe_bg = pres.theme.stripe().bg.unwrap();
    let muted_fg = pres.theme.color(Token::Muted).unwrap();
    let text = buffer_to_string(&buf);
    assert!(text.lines().next().unwrap().contains('\u{258d}'), "{text}");
    assert!(text.contains("Results"), "{text}");

    // Column header row is at inside.y + 3 = y 4: active column `id` carries
    // Selection bg while gutter `#`, unselected `note` column, and trailing full-width
    // fill across grid_width carry SurfaceAlt stripe_bg.
    let col_header_y = 4;
    let header_line: String = (0..area.width)
        .map(|x| buf[(x, col_header_y)].symbol())
        .collect();
    let hash_x = header_line[..header_line.find('#').unwrap()]
        .chars()
        .count() as u16;
    let id_x = header_line[..header_line.find("id").unwrap()]
        .chars()
        .count() as u16;
    let note_x = header_line[..header_line.find("note").unwrap()]
        .chars()
        .count() as u16;
    let selection_bg = pres.theme.style(Token::Selection).bg.unwrap();

    assert_eq!(buf[(hash_x, col_header_y)].bg, stripe_bg);
    assert_eq!(buf[(id_x, col_header_y)].bg, selection_bg);
    assert_eq!(buf[(note_x, col_header_y)].bg, stripe_bg);
    // The numeric `id` column right-aligns its name, so its cell starts
    // after the gutter `#` and its padding cell rather than at the text.
    for x in (1..=hash_x + 1).chain((note_x - 2)..59) {
        assert_eq!(
            buf[(x, col_header_y)].bg,
            stripe_bg,
            "column header cell x={x} must carry SurfaceAlt stripe across full grid_width"
        );
    }
    // Unselected row 2 (inside.y + 6 = y 7) keeps SurfaceAlt gutter and Muted line number.
    assert_eq!(buf[(hash_x, 7)].bg, stripe_bg);
    assert_eq!(buf[(hash_x, 7)].fg, muted_fg);

    // Basic16 & ColorDepth::None / ASCII stay unpainted and strictly ASCII.
    for (tier, depth) in [
        (GlyphTier::Ascii, ColorDepth::Basic16),
        (GlyphTier::Ascii, ColorDepth::None),
    ] {
        let low = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(tier),
            false,
        );
        let low_buf = render(&model, &low, area);
        assert_eq!(low_buf[(50, col_header_y)].bg, Color::Reset);
        assert!(buffer_to_string(&low_buf).is_ascii());
    }

    // Empty state renders hairline rule and Theme::keycap hints cleanly in 40x8 ASCII.
    let empty_model = Model::new(100);
    let ascii_none = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let empty_buf = render(&empty_model, &ascii_none, Rect::new(0, 0, 40, 8));
    let empty_text = buffer_to_string(&empty_buf);
    assert!(empty_text.is_ascii(), "{empty_text:?}");
    assert!(empty_text.contains("Connection required."), "{empty_text}");
    assert!(empty_text.contains("---"), "{empty_text}");
}

#[test]
fn grid_rule_places_cross_junctions_under_every_column_divider() {
    let model = model_with(
        &["id", "customer", "notes"],
        &[vec![
            Cell::Text("1".into()),
            Cell::Text("Ada".into()),
            Cell::Text("tier 1".into()),
        ]],
    );
    let area = Rect::new(0, 0, 80, 12);
    let pres = Presentation::new(
        Theme::new(ThemeChoice::Atelier, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    let buf = render(&model, &pres, area);

    let header_y = (0..area.height)
        .find(|&y| {
            let line: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
            line.contains('#') && line.contains("customer")
        })
        .expect("grid header row");
    let rule_y = header_y + 1;
    let divider_xs: Vec<u16> = (0..area.width)
        .filter(|&x| buf[(x, header_y)].symbol() == "\u{2502}")
        .collect();
    assert_eq!(divider_xs.len(), 2, "expected 2 column dividers");
    for x in divider_xs {
        assert_eq!(
            buf[(x, rule_y)].symbol(),
            "\u{253c}",
            "expected cross junction below column divider at x={x}"
        );
        assert_eq!(
            buf[(x, rule_y)].fg,
            pres.theme.color(Token::Border).unwrap()
        );
    }

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let abuf = render(&model, &ascii, area);
    let aheader_y = (0..area.height)
        .find(|&y| {
            let line: String = (0..area.width).map(|x| abuf[(x, y)].symbol()).collect();
            line.contains('#') && line.contains("customer")
        })
        .expect("ascii grid header row");
    assert!(buffer_to_string(&abuf).is_ascii());
    for x in (1..(area.width - 1)).filter(|&x| abuf[(x, aheader_y)].symbol() == "|") {
        assert_eq!(abuf[(x, aheader_y + 1)].symbol(), "+");
    }
}

#[test]
fn render_error_preserves_caret_column_and_styles_code_strip() {
    use crate::diagnostics::{Diagnostic, DiagnosticKind};
    use crate::query::error_location::ErrorLocation;

    let mut model = Model::new(100);
    let sql = "SELECT id, custmer_id FROM public.orders;";
    model.editor.set_text(sql);
    let mut error = Diagnostic::new(
        DiagnosticKind::Query,
        "column \"custmer_id\" does not exist",
        "executing buffer",
    );
    error.technical.push(crate::diagnostics::TechnicalField {
        label: "code".into(),
        value: "42703".into(),
    });
    model.error_location = Some(ErrorLocation {
        cursor: 11,
        token: Some(11..21),
        line: 1,
        column: 12,
        statement_number: 1,
        character: 12,
    });

    let area = Rect::new(0, 0, 80, 18);
    let pres = Presentation::new(
        Theme::new(ThemeChoice::Atelier, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    let mut buf = Buffer::empty(area);
    super::render_error(&model, &error, &pres, area, &mut buf);

    let find_line = |needle: &str| -> (u16, String) {
        for y in 0..area.height {
            let row: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
            if row.contains(needle) {
                return (y, row);
            }
        }
        panic!("missing {needle:?} in:\n{}", buffer_to_string(&buf));
    };

    let (sql_y, sql_row) = find_line("custmer_id FROM");
    let (caret_y, caret_row) = find_line("^");
    assert_eq!(
        caret_y,
        sql_y + 1,
        "caret must sit directly beneath SQL row"
    );
    let token_col = sql_row[..sql_row.find("custmer_id").unwrap()]
        .chars()
        .count();
    let caret_col = caret_row[..caret_row.find('^').unwrap()].chars().count();
    assert_eq!(
        caret_col, token_col,
        "caret column ({caret_col}) must align with custmer_id ({token_col}):\n{sql_row}\n{caret_row}"
    );
    assert_eq!(
        buf[(token_col as u16, sql_y)].bg,
        pres.theme.surface(Token::SurfaceAlt).bg.unwrap(),
        "SQL marker strip must use SurfaceAlt"
    );
    assert_eq!(
        buf[(token_col as u16, sql_y)].fg,
        pres.theme.color(Token::Danger).unwrap(),
        "offending token must highlight with Token::Danger"
    );
    assert!(
        buf[(token_col as u16, sql_y)]
            .modifier
            .contains(Modifier::UNDERLINED)
    );

    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut lbuf = Buffer::empty(area);
        super::render_error(&model, &error, &low, area, &mut lbuf);
        let ltext = buffer_to_string(&lbuf);
        assert!(ltext.is_ascii(), "{depth:?}: {ltext:?}");
        assert_eq!(lbuf[(token_col as u16, sql_y)].bg, Color::Reset);
    }
}

/// The two toolbar lines, read from the painted buffer: the rows directly
/// under the pane's top border.
fn toolbar_lines(buf: &Buffer, area: Rect) -> (String, String) {
    let row = |y: u16| -> String {
        (area.x + 1..area.x + area.width - 1)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_owned()
    };
    (row(area.y + 1), row(area.y + 2))
}

fn sortable_model() -> Model {
    let mut model = model_with(
        &["id", "amount"],
        &[
            vec![Cell::Text("1".into()), Cell::Text("30".into())],
            vec![Cell::Text("2".into()), Cell::Text("10".into())],
        ],
    );
    model.selected_column = 1;
    model
}

#[test]
fn a_local_sort_is_stated_once_with_its_retained_rows_qualifier() {
    let mut model = sortable_model();
    model.result_grid.toggle_sort(1);
    let presentation = presentation_at(ColorDepth::TrueColor);
    for width in [80u16, 120, 160] {
        let area = Rect::new(0, 0, width, 20);
        let (order, _) = toolbar_lines(&render(&model, &presentation, area), area);
        assert!(
            order.contains("sorted by amount ascending (local, retained rows only)"),
            "width {width}: {order:?}"
        );
        assert_eq!(order.matches("sorted").count(), 1, "{order:?}");
        assert!(!order.contains("order: local"), "{order:?}");
    }
    // At the minimum usable width the sort still says it is local and over
    // retained rows only, in whole words.
    let area = Rect::new(0, 0, 40, 20);
    let (order, _) = toolbar_lines(&render(&model, &presentation, area), area);
    assert_eq!(order, "local sort, retained rows only", "{order:?}");
}

#[test]
fn a_narrow_toolbar_drops_whole_segments_instead_of_cutting_words() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    for sorted in [false, true] {
        let mut model = sortable_model();
        if sorted {
            model.result_grid.toggle_sort(1);
        }
        for width in 20u16..=140 {
            let area = Rect::new(0, 0, width, 20);
            let (order, state) = toolbar_lines(&render(&model, &presentation, area), area);
            // A local sort is stated in some form in every pane that can
            // hold the shortest statement of it.
            if sorted && usize::from(width) >= 2 + "local sort, retained only".len() {
                assert!(order.contains("local"), "width {width}: {order:?}");
            }
            for line in [&order, &state] {
                assert!(
                    !line.contains('\u{2026}') && !line.contains("..."),
                    "width {width}, sorted {sorted}: a segment was cut: {line:?}"
                );
                let stripped = line
                    .strip_suffix("1/2 \u{00b7} amount")
                    .map(str::trim_end)
                    .unwrap_or(line.as_str());
                for segment in stripped.split(" | ").filter(|s| !s.is_empty()) {
                    assert!(
                        [
                            "rows 1-2 of 2 retained",
                            "order: server",
                            "sorted by amount ascending (local, retained rows only)",
                            "local sort, retained rows only",
                            "local sort, retained only",
                            "selected column 2: amount",
                            "2 columns",
                            "types: unavailable",
                            "Ctrl+K g grid controls",
                        ]
                        .contains(&segment),
                        "width {width}, sorted {sorted}: {segment:?} is not a whole segment"
                    );
                }
            }
        }
    }
}

#[test]
fn the_state_line_names_only_what_differs_from_a_plain_grid() {
    let mut model = sortable_model();
    let presentation = presentation_at(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 120, 20);

    let (_, state) = toolbar_lines(&render(&model, &presentation, area), area);
    assert_eq!(
        state,
        "2 columns | types: unavailable | Ctrl+K g grid controls"
    );

    // An unfocused pane does not teach a route that needs focus first.
    model.focus = Focus::Editor;
    let (_, state) = toolbar_lines(&render(&model, &presentation, area), area);
    assert_eq!(state, "2 columns | types: unavailable");

    model.result_grid.freeze_first = true;
    assert!(model.result_grid.toggle_column(0, 2));
    let (_, state) = toolbar_lines(&render(&model, &presentation, area), area);
    assert_eq!(
        state,
        "shown 1/2 columns | types: unavailable | freeze first: on"
    );

    let single = model_with(&["only"], &text_rows(2));
    let (_, state) = toolbar_lines(&render(&single, &presentation, area), area);
    assert!(state.starts_with("1 column | "), "{state:?}");
}

/// The column header row of a rendered grid: the first row whose gutter
/// carries `#`.
fn header_row(buf: &Buffer, area: Rect) -> u16 {
    (area.y..area.y + area.height)
        .find(|&y| {
            (area.x..area.x + area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_start_matches(['\u{2502}', '\u{2503}', '|', ' '])
                .starts_with('#')
        })
        .expect("a column header row")
}

#[test]
fn an_unfocused_header_marks_the_selected_column_without_a_selection_fill() {
    let mut model = sortable_model();
    model.focus = Focus::Editor;
    let area = Rect::new(0, 0, 60, 14);
    for depth in [
        ColorDepth::TrueColor,
        ColorDepth::Indexed256,
        ColorDepth::Basic16,
        ColorDepth::None,
    ] {
        let presentation = presentation_at(depth);
        let theme = &presentation.theme;
        let buf = render(&model, &presentation, area);
        let y = header_row(&buf, area);
        let row: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
        let amount_x = row[..row.find("amount").expect("amount header")]
            .chars()
            .count() as u16;
        let cell = &buf[(amount_x, y)];
        let expected = theme.on_stripe(Token::Focus).add_modifier(Modifier::BOLD);
        assert_eq!(cell.fg, expected.fg.unwrap_or(Color::Reset), "{depth:?}");
        assert_eq!(cell.bg, expected.bg.unwrap_or(Color::Reset), "{depth:?}");
        assert!(cell.modifier.contains(Modifier::BOLD), "{depth:?}");
        assert!(
            !cell.modifier.contains(Modifier::REVERSED),
            "{depth:?}: no selection fill while unfocused"
        );
        if let Some(selection) = theme.style(Token::Selection).bg {
            assert_ne!(cell.bg, selection, "{depth:?}: no selection fill");
        }
    }

    // Focused, the selected header keeps its selection fill.
    model.focus = Focus::Results;
    let presentation = presentation_at(ColorDepth::TrueColor);
    let buf = render(&model, &presentation, area);
    let y = header_row(&buf, area);
    let row: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
    let amount_x = row[..row.find("amount").expect("amount header")]
        .chars()
        .count() as u16;
    assert_eq!(
        Some(buf[(amount_x, y)].bg),
        presentation.theme.style(Token::Selection).bg
    );
}

#[test]
fn a_header_sits_right_aligned_over_a_numeric_column() {
    let model = model_with(
        &["n", "label"],
        &[
            vec![Cell::Text("12345".into()), Cell::Text("x".into())],
            vec![Cell::Text("7".into()), Cell::Text("y".into())],
        ],
    );
    let area = Rect::new(0, 0, 60, 14);
    let buf = render(&model, &presentation_at(ColorDepth::None), area);
    let y = header_row(&buf, area);
    let header: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
    let value_row: String = (0..area.width).map(|x| buf[(x, y + 2)].symbol()).collect();
    // The last character of the header and of the widest value share a column.
    let header_end = header[..header.find("n ").expect("n header") + 1]
        .chars()
        .count();
    let value_end = value_row[..value_row.find("12345").expect("value") + 5]
        .chars()
        .count();
    assert_eq!(header_end, value_end, "{header}\n{value_row}");
    // A text column stays left-aligned: its name starts where its values do.
    let label_x = header.find("label").expect("label");
    let x_x = value_row.rfind(" x").expect("value x") + 1;
    assert_eq!(
        header[..label_x].chars().count(),
        value_row[..x_x].chars().count(),
        "{header}\n{value_row}"
    );
}

fn connected_empty(sql: &str) -> Model {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.focus = Focus::Results;
    model.connection =
        crate::app::model::ConnectionState::Connected(Box::new(crate::postgres::SessionInfo {
            target: "app@localhost:5432/orders".into(),
            database: "orders".into(),
            user: "app".into(),
            server_version: "18.0".into(),
            backend_pid: "1".into(),
            search_path: "public".into(),
            read_only: false,
            tls: crate::postgres::TlsState::Disabled,
            environment: crate::connection::Environment::Local,
            statement_timeout: None,
            lock_timeout: None,
            idle_in_transaction_session_timeout: None,
        }));
    model.editor.set_text(sql);
    model
}

#[test]
fn an_empty_results_pane_says_sql_is_ready_once_the_editor_has_some() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 24);
    let ready = buffer_to_string(&render(&connected_empty("SELECT 1;"), &presentation, area));
    assert!(ready.contains("SQL is ready to run."), "{ready}");
    assert!(ready.contains("Ctrl+R Run the buffer"), "{ready}");
    assert!(!ready.contains("Write SQL in the editor"), "{ready}");

    let blank = buffer_to_string(&render(&connected_empty("  \n"), &presentation, area));
    assert!(blank.contains("Write SQL in the editor"), "{blank}");
    assert!(!blank.contains("SQL is ready to run."), "{blank}");
}

#[test]
fn the_empty_results_state_stacks_from_the_top_with_text_hint_labels() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 24);
    let buf = render(&connected_empty("SELECT 1;"), &presentation, area);
    let rows: Vec<String> = buffer_to_string(&buf).lines().map(str::to_owned).collect();
    let find = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("missing {needle:?}: {rows:#?}"))
    };
    let headline = find("No query has run yet.");
    let palette = find("Command palette shows safe next actions.");
    // Two prose rows, a breathing row, the rule and two hint rows: the keys
    // sit right under the words, with the pane's spare height below them.
    assert!(
        palette - headline <= 5,
        "the hints drifted away from the prose: {rows:#?}"
    );
    assert!(
        rows[palette + 1..rows.len() - 1]
            .iter()
            .all(|row| row.trim_matches(['\u{2502}', '\u{2503}', ' ']).is_empty()),
        "{rows:#?}"
    );
    // Both hint labels are primary text, not dimmed.
    for label in ["Run the buffer", "Command palette shows"] {
        let y = find(label) as u16;
        let row = &rows[y as usize];
        let x = row[..row.find(label).expect("label")].chars().count() as u16;
        let cell = &buf[(x, y)];
        assert_eq!(
            Some(cell.fg),
            presentation.theme.style(Token::Text).fg,
            "{label}"
        );
        assert!(!cell.modifier.contains(Modifier::DIM), "{label}");
    }
}

fn labelled_error() -> crate::diagnostics::Diagnostic {
    use crate::diagnostics::{Diagnostic, DiagnosticKind, ObjectContext};
    let mut error = Diagnostic::new(DiagnosticKind::Query, "syntax error", "executing buffer")
        .technical("SQLSTATE", "42601")
        .technical("Severity", "ERROR");
    error.object = Some(ObjectContext {
        schema: Some("public".into()),
        table: Some("users".into()),
        column: None,
        constraint: None,
    });
    error.statement_number = Some(1);
    error.likely_cause = Some("missing semicolon".into());
    error.next_action = Some("add semicolon at end".into());
    error
}

fn error_text(model: &Model, presentation: &Presentation) -> Vec<String> {
    let area = Rect::new(0, 0, 90, 20);
    let mut buf = Buffer::empty(area);
    super::render_error(model, &labelled_error(), presentation, area, &mut buf);
    buffer_to_string(&buf).lines().map(str::to_owned).collect()
}

#[test]
fn every_error_label_starts_in_the_same_column_in_every_glyph_tier() {
    for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
        let presentation = Presentation::new(
            Theme::new(ThemeChoice::Dark, true),
            Glyphs::new(tier),
            false,
        );
        let rows = error_text(&Model::new(10), &presentation);
        let column = |label: &str| -> usize {
            let row = rows
                .iter()
                .find(|row| row.contains(label))
                .unwrap_or_else(|| panic!("{tier:?}: missing {label:?}: {rows:#?}"));
            row[..row.find(label).expect("label")].chars().count()
        };
        let expected = column("While:");
        for label in [
            "Object context:",
            "Catalogue:",
            "Statement:",
            "Likely cause:",
            "Next:",
            "2 technical field(s) hidden.",
        ] {
            assert_eq!(column(label), expected, "{tier:?}: {label} is out of line");
        }
    }
}

#[test]
fn the_error_detail_key_comes_from_the_keymap_and_offers_the_collapse() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let mut model = Model::new(10);
    let rows = error_text(&model, &presentation).join("\n");
    assert!(
        rows.contains("2 technical field(s) hidden. Ctrl+D to expand."),
        "{rows}"
    );

    let mut keys = std::collections::BTreeMap::new();
    keys.insert(
        "toggle-error-detail".to_owned(),
        crate::config::schema::KeySpec::One("f11".to_owned()),
    );
    model.keymap_snapshot = Keymap::from_config(&keys)
        .expect("a valid rebinding")
        .snapshot();
    let rows = error_text(&model, &presentation).join("\n");
    assert!(rows.contains("hidden. F11 to expand."), "{rows}");
    assert!(!rows.contains("Ctrl+D"), "{rows}");

    model.error_expanded = true;
    let rows = error_text(&model, &presentation).join("\n");
    assert!(rows.contains("42601"), "{rows}");
    assert!(rows.contains("F11 to collapse."), "{rows}");
    assert!(!rows.contains("to expand"), "{rows}");
}

// ------------------------------------------------------ result settle (063)

/// A completed result of job 1, settling as the interactive runtime would
/// start it, `elapsed` into the settle.
fn settling(elapsed: Duration) -> Model {
    let mut model = model_with(&["id"], &text_rows(2));
    model.motion.enable();
    model.motion.begin_result_settle(JobId(1));
    model.motion.advance(elapsed);
    model
}

/// The cell under the first character of the status word `Completed`.
fn status_cell(buf: &Buffer) -> ratatui::buffer::Cell {
    let title: String = (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect();
    let x = title[..title.find("Completed").expect("the status word")]
        .chars()
        .count() as u16;
    buf[(x, 0)].clone()
}

fn settle_frames(depth: ColorDepth) -> Vec<(Duration, Buffer)> {
    let presentation = presentation_at(depth);
    let area = Rect::new(0, 0, 80, 12);
    (0..=20u64)
        .map(|step| {
            let at = Duration::from_millis(step * 30);
            (at, render(&settling(at), &presentation, area))
        })
        .collect()
}

#[test]
fn a_settle_fades_only_the_status_word_ink_into_its_exact_resting_frame() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let theme = &presentation.theme;
    let area = Rect::new(0, 0, 80, 12);
    let resting = render(&model_with(&["id"], &text_rows(2)), &presentation, area);
    let frames = settle_frames(ColorDepth::TrueColor);

    let first = status_cell(&frames[0].1);
    assert_eq!(
        Some(first.fg),
        theme.color(Token::Success),
        "starts in success ink"
    );
    let settled = frames.last().expect("frames");
    assert!(settled.0 >= crate::ui::motion::TransitionKind::ResultSettle.duration());
    assert_eq!(settled.1, resting, "the settled frame is the resting frame");
    assert_eq!(
        Some(status_cell(&resting).fg),
        theme.mix(Token::Success, Token::Muted, 1.0),
        "the fade ends on exactly the resting ink"
    );

    let mut inks = Vec::new();
    for (at, frame) in &frames {
        assert_eq!(
            buffer_to_string(frame),
            buffer_to_string(&resting),
            "{at:?}: the words are identical in every frame"
        );
        let rule_y = (1..area.height)
            .find(|&y| resting[(1, y)].symbol() == "\u{2500}")
            .expect("column header hairline rule");
        for y in 0..area.height {
            for x in 0..area.width {
                let (cell, rest) = (&frame[(x, y)], &resting[(x, y)]);
                assert_eq!(cell.bg, rest.bg, "{at:?} ({x},{y}): never a background");
                assert_eq!(cell.modifier, rest.modifier, "{at:?} ({x},{y}): no pop");
                let is_row_gutter = y > rule_y && (1..=2).contains(&x);
                if y > 0 && y != rule_y && !is_row_gutter {
                    assert_eq!(
                        cell.fg, rest.fg,
                        "{at:?} ({x},{y}): only the title, header rule, and row gutter move"
                    );
                } else if y == rule_y && (x == 0 || x + 1 == area.width) {
                    assert_eq!(
                        cell.fg, rest.fg,
                        "{at:?} ({x},{y}): pane borders never move"
                    );
                }
            }
        }
        inks.push(status_cell(frame).fg);
    }
    inks.dedup();
    assert!(inks.len() > 3, "truecolour interpolates: {inks:?}");

    // The summary beside the word keeps its own dimmed muted ink throughout.
    let title: String = (0..area.width)
        .map(|x| frames[0].1[(x, 0)].symbol())
        .collect();
    let summary_x = title[..title.find("2 rows").expect("summary")]
        .chars()
        .count() as u16;
    assert_eq!(
        frames[0].1[(summary_x, 0)].style(),
        resting[(summary_x, 0)].style()
    );
    assert!(
        resting[(summary_x, 0)].modifier.contains(Modifier::DIM),
        "the summary stays quiet"
    );
    assert!(
        !status_cell(&resting).modifier.contains(Modifier::DIM),
        "the resting status word is not dimmed, so the fade has nothing to pop into"
    );
}

#[test]
fn a_settle_steps_at_the_midpoint_in_256_colours() {
    let theme = presentation_at(ColorDepth::Indexed256).theme;
    let frames = settle_frames(ColorDepth::Indexed256);
    for (at, frame) in &frames {
        let progress = crate::ui::motion::Transition::new(
            crate::ui::motion::TransitionKind::ResultSettle,
            Duration::ZERO,
        )
        .progress(*at);
        let expected = if progress < 0.5 {
            theme.color(Token::Success)
        } else {
            theme.color(Token::Muted)
        };
        assert_eq!(
            Some(status_cell(frame).fg),
            expected,
            "{at:?} at {progress}"
        );
    }
}

#[test]
fn a_settle_tints_only_its_own_completed_result() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 12);
    let resting = render(&model_with(&["id"], &text_rows(2)), &presentation, area);

    // Another job's settle never tints this result.
    let mut other = settling(Duration::ZERO);
    other.motion.begin_result_settle(JobId(2));
    assert_eq!(render(&other, &presentation, area), resting);

    // A refresh or a cancelled outcome under the same job keeps its colours.
    let mut refreshing = settling(Duration::ZERO);
    refreshing.running_refresh = true;
    let mut still = refreshing.clone();
    still.motion.settle();
    assert_eq!(
        render(&refreshing, &presentation, area),
        render(&still, &presentation, area)
    );

    // A cancelled outcome keeps its static colours even under a settle that
    // names its job.
    let mut cancelled = settling(Duration::ZERO);
    if let Some(execution) = cancelled.last_execution.as_mut() {
        execution.status = ExecutionStatus::Cancelled;
    }
    let mut cancelled_still = cancelled.clone();
    cancelled_still.motion.settle();
    let cancelled_frame = render(&cancelled, &presentation, area);
    assert!(buffer_to_string(&cancelled_frame).contains("Query cancelled by server"));
    assert_eq!(
        cancelled_frame,
        render(&cancelled_still, &presentation, area)
    );

    // No colour means no tint: the frame is the resting frame.
    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = presentation_at(depth);
        assert_eq!(
            render(&settling(Duration::ZERO), &low, area),
            render(&model_with(&["id"], &text_rows(2)), &low, area),
            "{depth:?}"
        );
    }
}

#[test]
fn every_settle_ink_keeps_text_contrast_on_the_pane_in_every_built_in_theme() {
    use crate::ui::theme::Rgb;
    for choice in [
        ThemeChoice::Dark,
        ThemeChoice::Light,
        ThemeChoice::HighContrast,
        ThemeChoice::FieldIndustrial,
        ThemeChoice::Material,
        ThemeChoice::Atelier,
        ThemeChoice::Paper,
    ] {
        let theme = Theme::new(choice, true);
        let pane = theme.rgb(Token::SurfacePane);
        let threshold = if choice == ThemeChoice::HighContrast {
            7.0
        } else {
            4.5
        };
        for step in 0..=20u8 {
            let amount = f32::from(step) / 20.0;
            let ink: Rgb = theme
                .rgb(Token::Success)
                .mix(theme.rgb(Token::Muted), amount);
            let contrast = ink.contrast(pane);
            assert!(
                contrast >= threshold,
                "{choice:?} at {amount}: {} on {} is {contrast:.2}:1",
                ink.hex(),
                pane.hex()
            );
        }
    }
}

// ---------------------------------------------------- heartbeat comet (063)

fn running(frame: u64, cancelling: bool) -> Model {
    let mut model = Model::new(100);
    model.sidebar_visible = false;
    model.focus = Focus::Results;
    model.phase = if cancelling {
        crate::app::model::QueryPhase::CancellationRequested { job: JobId(1) }
    } else {
        crate::app::model::QueryPhase::Running {
            job: JobId(1),
            statements: 1,
        }
    };
    model.running_for = Some(Duration::from_millis(1500));
    model.frame = frame;
    model
}

/// The heartbeat row's cells, found by its track glyph.
fn heartbeat_cells(buf: &Buffer) -> Vec<ratatui::buffer::Cell> {
    let area = buf.area;
    let (start_x, y) = (1..area.height)
        .find_map(|y| {
            (1..area.width.saturating_sub(super::HEARTBEAT_TRACK as u16))
                .find(|&start_x| {
                    (start_x..start_x + super::HEARTBEAT_TRACK as u16)
                        .all(|x| ["\u{2500}", "\u{2501}", "-", "="].contains(&buf[(x, y)].symbol()))
                })
                .map(|start_x| (start_x, y))
        })
        .expect("a heartbeat row");
    (start_x..start_x + super::HEARTBEAT_TRACK as u16)
        .map(|x| buf[(x, y)].clone())
        .collect()
}

#[test]
fn the_heartbeat_moves_one_cell_a_tick_back_and_forth_and_never_arrives() {
    let width = super::HEARTBEAT_TRACK;
    let mut previous = super::heartbeat_head(0, width).0;
    let mut seen = std::collections::BTreeSet::new();
    for frame in 1..=200u64 {
        let (head, _) = super::heartbeat_head(frame, width);
        assert!(head < width, "frame {frame}");
        assert_eq!(
            head.abs_diff(previous),
            1,
            "frame {frame}: linear, one cell a tick"
        );
        seen.insert(head);
        previous = head;
    }
    assert_eq!(seen.len(), width, "the head sweeps the whole track");
    // It is periodic, so nothing about it can read as a count or an outcome.
    let period = 2 * (width as u64 - 1);
    for frame in 0..period {
        assert_eq!(
            super::heartbeat_head(frame, width),
            super::heartbeat_head(frame + period, width)
        );
    }
}

#[test]
fn the_heartbeat_is_a_comet_with_a_fading_tail_at_truecolour() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let theme = &presentation.theme;
    let area = Rect::new(0, 0, 60, 12);
    let text = buffer_to_string(&render(&running(10, false), &presentation, area));
    assert!(text.contains("Running"), "{text}");
    assert!(text.contains("1.5 s"), "{text}");

    let cells = heartbeat_cells(&render(&running(10, false), &presentation, area));
    let heads: Vec<usize> = (0..cells.len())
        .filter(|&i| cells[i].symbol() == "\u{2501}")
        .collect();
    assert_eq!(
        heads,
        vec![5, 6, 7, 8, 9, 10],
        "a head and a five-cell phosphor tail behind it"
    );
    assert_eq!(Some(cells[10].fg), theme.color(Token::Info));
    let tail: Vec<_> = [9, 8, 7, 6, 5].iter().map(|&i| cells[i].fg).collect();
    assert_eq!(
        tail,
        [1.0 / 6.0, 2.0 / 6.0, 3.0 / 6.0, 4.0 / 6.0, 5.0 / 6.0]
            .iter()
            .map(|amount| theme.mix(Token::Info, Token::Border, *amount).expect("ink"))
            .collect::<Vec<_>>(),
        "the tail fades smoothly from the head into the track"
    );
    for cell in cells.iter().skip(11) {
        assert_eq!(cell.symbol(), "\u{2500}");
        assert_eq!(Some(cell.fg), theme.color(Token::Border));
    }

    // Moving left, the tail trails to the right of the head.
    let frame = super::HEARTBEAT_TRACK as u64 + 6;
    let (head, rightward) = super::heartbeat_head(frame, super::HEARTBEAT_TRACK);
    assert!(!rightward);
    let cells = heartbeat_cells(&render(&running(frame, false), &presentation, area));
    for distance in 1..=5 {
        assert_eq!(cells[head + distance].symbol(), "\u{2501}");
    }
    assert_eq!(cells[head - 1].symbol(), "\u{2500}");
}

#[test]
fn the_heartbeat_keeps_its_shape_and_drops_colour_honestly_at_lower_depths() {
    let area = Rect::new(0, 0, 60, 12);

    let indexed = presentation_at(ColorDepth::Indexed256);
    let cells = heartbeat_cells(&render(&running(10, false), &indexed, area));
    assert_eq!(Some(cells[10].fg), indexed.theme.color(Token::Info));
    for i in [7, 8, 9] {
        assert_eq!(cells[i].symbol(), "\u{2501}", "the same layout");
        assert_eq!(
            Some(cells[i].fg),
            indexed.theme.color(Token::Border),
            "no mix"
        );
    }

    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = presentation_at(depth);
        let cells = heartbeat_cells(&render(&running(10, false), &low, area));
        let heads: Vec<usize> = (0..cells.len())
            .filter(|&i| cells[i].symbol() == "\u{2501}")
            .collect();
        assert_eq!(heads, vec![10], "{depth:?}: the head alone, no tail");
        if depth == ColorDepth::Basic16 {
            assert!(cells[10].modifier.contains(Modifier::BOLD), "a bold head");
        } else {
            assert!(
                cells
                    .iter()
                    .all(|cell| cell.fg == Color::Reset && cell.modifier.is_empty()),
                "shape only"
            );
        }
    }

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, false),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    for frame in 0..60 {
        let cells = heartbeat_cells(&render(&running(frame, false), &ascii, area));
        let row: String = cells.iter().map(|cell| cell.symbol()).collect();
        assert_eq!(row.matches('=').count(), 1, "{row}");
        assert_eq!(
            row.matches('-').count(),
            super::HEARTBEAT_TRACK - 1,
            "{row}"
        );
    }
}

#[test]
fn a_pending_cancellation_keeps_the_heartbeat_moving_in_the_warning_ink() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 60, 12);
    let first = render(&running(4, true), &presentation, area);
    let next = render(&running(5, true), &presentation, area);
    assert_ne!(
        heartbeat_cells(&first),
        heartbeat_cells(&next),
        "it keeps moving"
    );
    let cells = heartbeat_cells(&first);
    assert_eq!(Some(cells[4].fg), presentation.theme.color(Token::Warning));
    assert!(buffer_to_string(&first).contains("Cancellation requested"));
}

#[test]
fn reduced_motion_keeps_the_words_and_draws_no_heartbeat() {
    let still = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let area = Rect::new(0, 0, 60, 12);
    let first = render(&running(3, false), &still, area);
    let later = render(&running(40, false), &still, area);
    assert_eq!(
        buffer_to_string(&first),
        buffer_to_string(&later),
        "the same words in every frame"
    );
    assert_eq!(first, later, "byte-identical across frames");
    let text = buffer_to_string(&first);
    assert!(text.contains("Running") && text.contains("1.5 s"), "{text}");
    let rows: Vec<&str> = text.lines().collect();
    let running_row = rows
        .iter()
        .position(|line| line.contains("Running"))
        .expect("running row");
    assert!(
        rows[running_row + 1].contains("asks the server to cancel"),
        "the words follow directly, with no heartbeat row: {text}"
    );
}

#[test]
fn a_custom_success_ink_too_faint_for_a_word_skips_the_settle() {
    use crate::ui::theme::{Rgb, ThemeDefinition, ThemeSelection, ValidatedThemes};
    // Admitted at 3:1 for badges, but under 4.5:1 on the pane the title sits on.
    let faint = Rgb(0x30, 0x8c, 0x50);
    let definition = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Success, faint);
    let theme = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition))
        .expect("a custom palette admitted at 3:1")
        .selected_theme(true, ColorDepth::TrueColor);
    assert!(faint.contrast(theme.rgb(Token::SurfacePane)) < 4.5);
    let presentation = Presentation::new(theme, Glyphs::new(GlyphTier::Unicode), false);
    let area = Rect::new(0, 0, 80, 12);
    let resting = render(&model_with(&["id"], &text_rows(2)), &presentation, area);
    for step in [0u64, 60, 200] {
        assert_eq!(
            render(&settling(Duration::from_millis(step)), &presentation, area),
            resting,
            "{step} ms: the faint ink is never painted"
        );
    }
    // A built-in palette still settles.
    let built_in = presentation_at(ColorDepth::TrueColor);
    assert_eq!(
        Some(status_cell(&render(&settling(Duration::ZERO), &built_in, area)).fg),
        built_in.theme.color(Token::Success)
    );
}

#[test]
fn a_settle_tints_the_column_header_hairline_rule_and_respects_faint_custom_themes() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let theme = &presentation.theme;
    let area = Rect::new(0, 0, 80, 12);
    let resting = render(&model_with(&["id"], &text_rows(2)), &presentation, area);
    let rule_y = (1..area.height)
        .find(|&y| resting[(1, y)].symbol() == "\u{2500}")
        .expect("column header rule row");

    let at_zero = render(&settling(Duration::ZERO), &presentation, area);
    assert_eq!(Some(at_zero[(1, rule_y)].fg), theme.color(Token::Success));
    assert_eq!(at_zero[(0, rule_y)].fg, resting[(0, rule_y)].fg);

    let mid = Duration::from_millis(240);
    let mid_model = settling(mid);
    let at_mid = render(&mid_model, &presentation, area);
    let (_, progress) = mid_model.motion().result_settle().expect("active settle");
    assert_eq!(
        Some(at_mid[(1, rule_y)].fg),
        theme.mix(Token::Success, Token::Border, progress)
    );

    let settled = render(&settling(Duration::from_millis(720)), &presentation, area);
    assert_eq!(settled[(1, rule_y)].fg, resting[(1, rule_y)].fg);

    // A low-contrast custom theme keeps the rule still.
    use crate::ui::theme::{Rgb, ThemeDefinition, ThemeSelection, ValidatedThemes};
    let faint = Rgb(0x30, 0x8c, 0x50);
    let definition = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Success, faint);
    let custom_theme = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition))
        .expect("custom palette passes 3:1 gate")
        .selected_theme(true, ColorDepth::TrueColor);
    let custom_pres = Presentation::new(custom_theme, Glyphs::new(GlyphTier::Unicode), false);
    let custom_resting = render(&model_with(&["id"], &text_rows(2)), &custom_pres, area);
    let custom_zero = render(&settling(Duration::ZERO), &custom_pres, area);
    assert_eq!(custom_zero[(1, rule_y)].fg, custom_resting[(1, rule_y)].fg);
}

#[test]
fn scrollbar_uses_hairline_glyphs_and_focus_contrast() {
    let rows: Vec<Vec<Cell>> = (0..40)
        .map(|i| vec![Cell::Text(format!("row-{i}"))])
        .collect();
    let mut focused_model = model_with(&["id"], &rows);
    focused_model.focus = Focus::Results;
    let mut unfocused_model = model_with(&["id"], &rows);
    unfocused_model.focus = Focus::Editor;

    let area = Rect::new(0, 0, 50, 10);
    let unicode = presentation_at(ColorDepth::TrueColor);
    let buf_focused = render(&focused_model, &unicode, area);
    let buf_unfocused = render(&unfocused_model, &unicode, area);

    let scroll_x = (1..area.width - 1)
        .find(|&x| {
            (1..area.height - 1)
                .any(|y| ["\u{2503}", "\u{2502}"].contains(&buf_focused[(x, y)].symbol()))
        })
        .expect("unicode scrollbar column");

    let thumb_y = (1..area.height - 1)
        .find(|&y| buf_focused[(scroll_x, y)].symbol() == "\u{2503}")
        .expect("unicode thumb cell");
    let track_y = (1..area.height - 1)
        .find(|&y| buf_focused[(scroll_x, y)].symbol() == "\u{2502}")
        .expect("unicode track cell");

    assert_eq!(
        Some(buf_focused[(scroll_x, thumb_y)].fg),
        unicode.theme.color(Token::Focus)
    );
    assert_eq!(
        Some(buf_focused[(scroll_x, track_y)].fg),
        unicode.theme.color(Token::Border)
    );
    assert_eq!(
        Some(buf_unfocused[(scroll_x, thumb_y)].fg),
        unicode.theme.color(Token::Muted)
    );
    assert_eq!(
        Some(buf_unfocused[(scroll_x, track_y)].fg),
        unicode.theme.color(Token::Border)
    );

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let buf_ascii = render(&focused_model, &ascii, area);
    assert_eq!(buf_ascii[(scroll_x, thumb_y)].symbol(), "#");
    assert_eq!(buf_ascii[(scroll_x, track_y)].symbol(), "|");
}

#[test]
fn inline_sort_chevron_renders_within_column_width_and_preserves_geometry() {
    let mut asc_model = model_with(
        &["id", "very_long_column_name_here"],
        &[vec![Cell::Text("1".into()), Cell::Text("ok".into())]],
    );
    asc_model.result_grid.sort = Some(grid::SortSpec {
        column: 0,
        direction: grid::SortDirection::Ascending,
    });
    let mut desc_model = asc_model.clone();
    desc_model.result_grid.sort = Some(grid::SortSpec {
        column: 0,
        direction: grid::SortDirection::Descending,
    });
    let mut unsorted_model = asc_model.clone();
    unsorted_model.result_grid.sort = None;

    let area = Rect::new(0, 0, 80, 10);
    let unicode = presentation_at(ColorDepth::TrueColor);
    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, true),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );

    let asc_text = buffer_to_string(&render(&asc_model, &unicode, area));
    let desc_text = buffer_to_string(&render(&desc_model, &unicode, area));
    assert!(asc_text.contains("id \u{25b4}"), "{asc_text}");
    assert!(desc_text.contains("id \u{25be}"), "{desc_text}");

    let asc_ascii = buffer_to_string(&render(&asc_model, &ascii, area));
    let desc_ascii = buffer_to_string(&render(&desc_model, &ascii, area));
    assert!(asc_ascii.contains("id ^"), "{asc_ascii}");
    assert!(desc_ascii.contains("id v"), "{desc_ascii}");

    // Column hit-testing geometry stays identical with or without sort.
    for x in 0..area.width {
        for y in 0..area.height {
            assert_eq!(
                crate::ui::widgets::results_region(&asc_model, area, x, y),
                crate::ui::widgets::results_region(&unsorted_model, area, x, y)
            );
        }
    }

    // Narrow manual column width omits chevron without truncating the column name.
    let mut tight_model = model_with(&["amount_cents"], &[vec![Cell::Text("100".into())]]);
    tight_model.result_grid.widths.insert(0, 12);
    tight_model.result_grid.sort = Some(grid::SortSpec {
        column: 0,
        direction: grid::SortDirection::Descending,
    });
    let tight_text = buffer_to_string(&render(&tight_model, &unicode, area));
    assert!(tight_text.contains("amount_cents"), "{tight_text}");
    assert!(!tight_text.contains('\u{25be}'), "{tight_text}");
}

#[test]
fn running_surface_is_inset_and_uses_focus_keycap_for_cancel() {
    let area = Rect::new(0, 0, 60, 12);
    let presentation = presentation_at(ColorDepth::TrueColor);
    let buf = render(&running(3, false), &presentation, area);
    let text = buffer_to_string(&buf);
    let rows: Vec<&str> = text.lines().collect();

    // Vertical inset leaves row immediately below border blank.
    assert!(
        rows[1]
            .trim_matches(['\u{2502}', '\u{2503}', ' '])
            .is_empty(),
        "{text}"
    );
    // Horizontal inset leaves x = 1 blank before the spinner (x = 2) and "Running" (x = 4).
    let running_y = rows
        .iter()
        .position(|line| line.contains("Running"))
        .expect("running row") as u16;
    assert_eq!(buf[(1, running_y)].symbol(), " ");
    assert_eq!(buf[(4, running_y)].symbol(), "R");

    // Cancel keycap uses Token::Focus keycap styling while preserving wording.
    assert!(text.contains("Ctrl+C asks the server to cancel."), "{text}");
    let cancel_y = rows
        .iter()
        .position(|line| line.contains("Ctrl+C"))
        .expect("cancel row") as u16;
    let ctrl_x = (1..area.width - 1)
        .find(|&x| buf[(x, cancel_y)].symbol() == "C")
        .expect("Ctrl+C cell");
    let expected_keycap = presentation.theme.keycap(Token::Focus);
    let keycap_cell = &buf[(ctrl_x, cancel_y)];
    assert_eq!(Some(keycap_cell.fg), expected_keycap.fg);
    assert_eq!(Some(keycap_cell.bg), expected_keycap.bg);
    assert_eq!(keycap_cell.modifier, expected_keycap.add_modifier);
}

#[test]
fn result_settle_waves_row_gutter_numbers_with_contrast_guard() {
    let presentation = presentation_at(ColorDepth::TrueColor);
    let theme = &presentation.theme;
    let area = Rect::new(0, 0, 80, 14);
    let mut base = model_with(&["id"], &text_rows(5));
    base.focus = Focus::Editor;
    let resting = render(&base, &presentation, area);

    let rule_y = (1..area.height)
        .find(|&y| resting[(1, y)].symbol() == "\u{2500}")
        .expect("header rule row");

    let mut at_zero_model = base.clone();
    at_zero_model.motion.enable();
    at_zero_model.motion.begin_result_settle(JobId(1));
    let at_zero = render(&at_zero_model, &presentation, area);
    assert_eq!(
        Some(at_zero[(2, rule_y + 1)].fg),
        theme.color(Token::Success),
        "row 0 gutter starts in Success ink"
    );
    assert_eq!(
        at_zero[(2, rule_y + 5)].fg,
        resting[(2, rule_y + 5)].fg,
        "bottom row waits for the vertical wave"
    );

    let mut mid_model = at_zero_model.clone();
    mid_model.motion.advance(Duration::from_millis(60));
    let at_mid = render(&mid_model, &presentation, area);
    assert_ne!(
        at_mid[(2, rule_y + 3)].fg,
        resting[(2, rule_y + 3)].fg,
        "mid-settle wave reaches middle row gutter"
    );

    // Custom low-contrast Success ink skips the gutter shimmer.
    use crate::ui::theme::{Rgb, ThemeDefinition, ThemeSelection, ValidatedThemes};
    let faint = Rgb(0x30, 0x8c, 0x50);
    let definition = ThemeDefinition::new(ThemeChoice::Dark).with_override(Token::Success, faint);
    let custom_theme = ValidatedThemes::resolve(ThemeSelection::Custom, Some(&definition))
        .expect("custom palette")
        .selected_theme(true, ColorDepth::TrueColor);
    let custom_pres = Presentation::new(custom_theme, Glyphs::new(GlyphTier::Unicode), false);
    assert_eq!(
        render(&at_zero_model, &custom_pres, area),
        render(&base, &custom_pres, area)
    );
}

#[test]
fn results_toolbar_renders_right_aligned_coordinate_cue_when_width_allows() {
    let mut model = sortable_model();
    model.selected_row = 1;
    model.selected_column = 1;
    let presentation = presentation_at(ColorDepth::TrueColor);
    let wide = Rect::new(0, 0, 120, 12);
    let buf = render(&model, &presentation, wide);
    let (order, _) = toolbar_lines(&buf, wide);
    assert!(
        order.ends_with("2/2 \u{00b7} amount"),
        "expected right-aligned coordinate cue: {order:?}"
    );
    let row_str: String = (0..wide.width).map(|x| buf[(x, 1)].symbol()).collect();
    let coord_x = row_str[..row_str.find("2/2").expect("coord")]
        .chars()
        .count() as u16;
    let col_x = row_str[..row_str.rfind("amount").expect("col name")]
        .chars()
        .count() as u16;
    assert_eq!(
        Some(buf[(coord_x, 1)].fg),
        presentation.theme.color(Token::Muted)
    );
    assert_eq!(
        Some(buf[(col_x, 1)].fg),
        presentation.theme.color(Token::Info)
    );
}

#[test]
fn quick_stats_bar_renders_numeric_and_text_summaries_with_ascii_and_unicode_parity() {
    let mut model = model_with(
        &["amount", "label"],
        &[
            vec![Cell::Text("10".into()), Cell::Text("alpha".into())],
            vec![Cell::Null, Cell::Text("beta".into())],
            vec![Cell::Text("30".into()), Cell::Text("alpha".into())],
        ],
    );
    let area = Rect::new(0, 0, 120, 14);
    for glyphs in [GlyphTier::Ascii, GlyphTier::Unicode] {
        let pres = Presentation::new(
            Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
            Glyphs::new(glyphs),
            false,
        );
        model.selected_column = 0;
        let numeric_text = buffer_to_string(&render(&model, &pres, area));
        assert!(
            numeric_text.contains(
                "amount: 3 rows | 2 non-null | 1 null | 2 distinct | min 10 .. 30 | sum 40 | avg 20"
            ),
            "{glyphs:?}: {numeric_text}"
        );

        model.selected_column = 1;
        let text_col = buffer_to_string(&render(&model, &pres, area));
        assert!(
            text_col.contains("label: 3 rows | 3 non-null | 0 null | 2 distinct"),
            "{glyphs:?}: {text_col}"
        );
        assert!(
            !text_col.contains("sum "),
            "{glyphs:?}: non-numeric column must not invent numeric stats: {text_col}"
        );
    }

    // Toggling show_quick_stats hides the bar cleanly.
    model.result_grid.show_quick_stats = false;
    let hidden = buffer_to_string(&render(
        &model,
        &presentation_at(ColorDepth::TrueColor),
        area,
    ));
    assert!(!hidden.contains("non-null"), "{hidden}");

    // Minimum-height pane with room for only one data row omits quick-stats so the row remains visible.
    model.result_grid.show_quick_stats = true;
    let tight = buffer_to_string(&render(
        &model,
        &presentation_at(ColorDepth::TrueColor),
        Rect::new(0, 0, 120, 8),
    ));
    assert!(tight.contains("alpha"), "{tight}");
    assert!(!tight.contains("non-null"), "{tight}");
}
