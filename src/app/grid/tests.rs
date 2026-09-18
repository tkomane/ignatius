use super::*;
use crate::query::value::Cell;

fn set(rows: &[&[Option<&str>]]) -> ResultSet {
    let mut result = ResultSet::new(vec!["id".into(), "value".into()], 20);
    for row in rows {
        result.push(
            row.iter()
                .map(|cell| cell.map_or(Cell::Null, |value| Cell::Text(value.to_owned())))
                .collect(),
        );
    }
    result
}

#[test]
fn result_state_defaults_to_truthful_reading_preferences() {
    let state = ResultGridState::new();
    assert!(state.show_types);
    assert!(!state.freeze_first);
    assert!(state.sort.is_none());
    assert!(state.visible_columns(0).is_empty());
}

#[test]
fn reset_for_result_clears_view_but_keeps_session_preferences() {
    let mut state = ResultGridState::new();
    state.show_types = false;
    state.freeze_first = true;
    state.sort = Some(SortSpec {
        column: 1,
        direction: SortDirection::Descending,
    });
    state.hidden_columns.insert(0);
    state.widths.insert(1, 60);
    state.horizontal_start = 4;
    state.note = Some("old result".into());

    state.reset_for_result(&["new".into()]);

    assert_eq!(state.columns_signature, vec!["new"]);
    assert!(!state.show_types);
    assert!(state.freeze_first);
    assert!(state.sort.is_none());
    assert!(state.hidden_columns.is_empty());
    assert!(state.widths.is_empty());
    assert_eq!(state.horizontal_start, 0);
    assert!(state.note.is_none());
}

#[test]
fn the_last_visible_column_cannot_be_hidden() {
    let mut state = ResultGridState::new();
    assert!(state.toggle_column(1, 2));
    assert!(!state.toggle_column(0, 2));
    assert_eq!(state.visible_columns(2), vec![0]);
    assert!(
        state
            .note
            .as_deref()
            .is_some_and(|note| note.contains("one result column"))
    );
    assert!(state.toggle_column(1, 2), "restore the hidden neighbour");
    assert_eq!(state.visible_columns(2), vec![0, 1]);
}

#[test]
fn duplicate_labels_are_still_independent_source_columns() {
    let mut state = ResultGridState::new();
    assert!(state.toggle_column(0, 3));
    assert_eq!(state.visible_columns(3), vec![1, 2]);
    assert!(!state.hidden_columns.contains(&1));
    assert!(state.toggle_column(1, 3));
    assert_eq!(state.visible_columns(3), vec![2]);
}

#[test]
fn widths_are_clamped_and_step_from_the_current_display_width() {
    let mut state = ResultGridState::new();
    assert_eq!(state.adjust_width(0, 10, 4), 14);
    assert_eq!(state.adjust_width(0, 14, -100), MIN_COLUMN_WIDTH);
    assert_eq!(state.adjust_width(0, 3, 1000), MAX_COLUMN_WIDTH);
    assert_eq!(state.width_for(0, 12), MAX_COLUMN_WIDTH);
}

#[test]
fn column_width_bounds_and_step_are_the_contract_values() {
    // contracts/grid.md: minimum 4, maximum 80, inclusive, and a keyboard step
    // of 2 cells per press.
    assert_eq!(MIN_COLUMN_WIDTH, 4);
    assert_eq!(MAX_COLUMN_WIDTH, 80);
    assert_eq!(COLUMN_WIDTH_STEP, 2);

    let step = isize::try_from(COLUMN_WIDTH_STEP).expect("a small step");
    let mut state = ResultGridState::new();
    assert_eq!(state.adjust_width(0, 10, step), 12);
    assert_eq!(state.adjust_width(0, 12, -step), 10);
    // An inherited automatic width below the minimum is lifted to the bound
    // before a step is applied.
    assert_eq!(state.adjust_width(1, 1, step), 6);
    assert_eq!(state.adjust_width(2, 1, -step), MIN_COLUMN_WIDTH);
    assert_eq!(
        state.adjust_width(3, MAX_COLUMN_WIDTH, step),
        MAX_COLUMN_WIDTH
    );
}

#[test]
fn automatic_widths_never_go_below_the_minimum() {
    let result = set(&[&[Some("1"), Some("x")]]);
    for index in 0..result.columns.len() {
        assert!(
            automatic_column_width(&result, index) >= MIN_COLUMN_WIDTH,
            "automatic width for column {index} fell below the bound"
        );
    }
}

#[test]
fn sorting_uses_decimal_values_without_floating_point_loss() {
    let result = set(&[
        &[Some("a"), Some("10")],
        &[Some("b"), Some("2")],
        &[Some("c"), Some("9007199254740993")],
        &[Some("d"), Some("9007199254740992")],
    ]);
    let mut state = ResultGridState::new();
    state.toggle_sort(1);
    assert_eq!(
        state.sort_rows(&(0..4).collect::<Vec<_>>(), &result),
        vec![1, 0, 3, 2]
    );
}

#[test]
fn sorting_keeps_null_last_in_both_directions_and_is_stable_for_equals() {
    let result = set(&[
        &[Some("first"), Some("2")],
        &[Some("second"), Some("2.00")],
        &[Some("empty"), Some("")],
        &[Some("null"), None],
    ]);
    let mut state = ResultGridState::new();
    state.toggle_sort(1);
    assert_eq!(
        state.sort_rows(&(0..4).collect::<Vec<_>>(), &result),
        vec![2, 0, 1, 3]
    );
    state.toggle_sort(1);
    assert_eq!(
        state.sort_rows(&(0..4).collect::<Vec<_>>(), &result),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn sorting_cycles_back_to_server_order() {
    let result = set(&[&[Some("first"), Some("b")], &[Some("second"), Some("a")]]);
    let mut state = ResultGridState::new();
    let source = vec![0, 1];
    state.toggle_sort(1);
    assert_eq!(state.sort_rows(&source, &result), vec![1, 0]);
    state.toggle_sort(1);
    state.toggle_sort(1);
    assert!(state.sort.is_none());
    assert_eq!(state.sort_rows(&source, &result), source);
}

#[test]
fn sort_wording_names_the_column_and_replaces_a_different_columns_sort() {
    let columns = vec!["id".to_owned(), "amount".to_owned()];
    let mut state = ResultGridState::new();
    assert_eq!(
        state.sort_label(&columns),
        None,
        "server order has no sort line"
    );
    state.toggle_sort(1);
    assert_eq!(
        state.sort_label(&columns).as_deref(),
        Some("sorted by amount ascending")
    );
    state.toggle_sort(1);
    assert_eq!(
        state.sort_label(&columns).as_deref(),
        Some("sorted by amount descending")
    );
    state.toggle_sort(1);
    assert_eq!(
        state.sort_label(&columns),
        None,
        "the cycle ends in server order"
    );

    state.toggle_sort(1);
    state.toggle_sort(0);
    assert_eq!(
        state.sort_label(&columns).as_deref(),
        Some("sorted by id ascending"),
        "a different column starts ascending and replaces the previous sort"
    );
    assert_eq!(state.sort.map(|sort| sort.column), Some(0));
}

#[test]
fn viewport_position_label_pins_the_four_shapes() {
    assert_eq!(
        viewport_position_label(120, 160, 1_248, 1_248, 1_248, false, false).as_deref(),
        Some("rows 120-160 of 1,248 retained")
    );
    assert_eq!(
        viewport_position_label(120, 160, 10_000, 10_000, 200_000, true, false).as_deref(),
        Some("rows 120-160 of 10,000 retained, of 200,000 returned")
    );
    assert_eq!(
        viewport_position_label(3, 42, 1_032, 1_248, 1_248, false, true).as_deref(),
        Some("rows 3-42 matching 1,032 of 1,248 retained")
    );
    // A filtered, truncated result keeps both facts rather than hiding one.
    assert_eq!(
        viewport_position_label(3, 42, 1_032, 1_248, 200_000, true, true).as_deref(),
        Some("rows 3-42 matching 1,032 of 1,248 retained, of 200,000 returned")
    );
    // Nothing rendered says nothing; it never invents `rows 0-0`.
    assert_eq!(viewport_position_label(1, 0, 0, 0, 0, false, false), None);
    assert_eq!(viewport_position_label(0, 0, 0, 0, 0, false, false), None);
}

#[test]
fn selected_column_window_stays_reachable_and_can_freeze_the_first() {
    let visible = vec![0, 1, 2, 3, 4, 5];
    let widths = vec![8; 6];
    let window = column_window(&visible, 4, &widths, 25, false, 0);
    assert!(window.contains(&4));
    let frozen = column_window(&visible, 4, &widths, 25, true, 0);
    assert_eq!(frozen[0], 0);
    assert!(frozen.contains(&4));
}

#[test]
fn a_narrow_frozen_window_keeps_both_identity_and_selected_column_known() {
    let window = column_window(&[0, 1, 2], 2, &[20, 20, 20], 5, true, 0);
    assert_eq!(window, vec![0, 2]);
}

#[test]
fn a_horizontal_position_cannot_hide_the_selected_column() {
    let visible = vec![0, 1, 2];
    let widths = vec![8; 3];
    assert_eq!(
        column_window(&visible, 2, &widths, usize::MAX, false, usize::MAX),
        visible,
        "an invalid saved offset falls back to a reachable window"
    );
    assert_eq!(
        column_window(&visible, 2, &widths, 12, true, usize::MAX),
        vec![0, 2],
        "freezing keeps identity and the selected source column visible"
    );
}

#[test]
fn reset_view_keeps_reading_preferences_but_clears_result_shape() {
    let mut state = ResultGridState::new();
    state.show_types = false;
    state.freeze_first = true;
    state.toggle_sort(0);
    state.toggle_column(1, 3);
    let _ = state.adjust_width(2, 10, 4);
    state.horizontal_start = 2;
    state.reset_view();

    assert!(state.sort.is_none());
    assert!(state.hidden_columns.is_empty());
    assert!(state.widths.is_empty());
    assert_eq!(state.horizontal_start, 0);
    assert!(!state.show_types);
    assert!(state.freeze_first);
}

#[test]
fn text_sort_handles_case_and_short_rows_without_panicking() {
    let mut result = ResultSet::new(vec!["id".into(), "label".into()], 20);
    result.push(vec![Cell::Text("one".into()), Cell::Text("b".into())]);
    result.push(vec![Cell::Text("two".into()), Cell::Text("A".into())]);
    result.push(vec![Cell::Text("three".into())]);

    let mut state = ResultGridState::new();
    state.toggle_sort(1);
    assert_eq!(
        state.sort_rows(&[0, 1, 2], &result),
        vec![1, 0, 2],
        "case-folded text comes first and a missing cell is last"
    );
}

#[test]
fn decimal_parser_handles_signs_fractions_and_exponents() {
    let values = ["-2", "-1.5", "0", ".5", "1e2", "1.00", "10"];
    let mut keys: Vec<_> = values
        .iter()
        .filter_map(|value| DecimalKey::parse(value))
        .collect();
    keys.sort();
    let rendered: Vec<(bool, String, i64)> = keys
        .into_iter()
        .map(|key| (key.negative, key.digits, key.scale))
        .collect();
    assert_eq!(rendered.len(), values.len());
    assert!(DecimalKey::parse("NaN").is_none());
    assert!(DecimalKey::parse("1e999999999999999999999").is_none());
}
