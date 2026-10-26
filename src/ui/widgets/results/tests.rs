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

    // 20 rows of area less two borders is 18; the five prelude lines leave 13
    // rendered rows starting at the top.
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("rows 1-13 of 30 retained"), "{text}");

    // Moving the selection one row past the fold scrolls the viewport.
    model.selected_row = 20;
    let text = buffer_to_string(&render(&model, &presentation, area));
    assert!(text.contains("rows 9-21 of 30 retained"), "{text}");
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
        text.contains("rows 1-13 of 30 retained, of 200,000 returned"),
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
