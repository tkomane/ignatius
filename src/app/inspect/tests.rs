use super::*;

fn set() -> ResultSet {
    let mut set = ResultSet::new(vec!["id".into(), "note".into(), "amount".into()], 10);
    set.push(vec![
        Cell::Text("1".into()),
        Cell::Text("a fairly long note that will not fit".into()),
        Cell::Null,
    ]);
    set.push(vec![
        Cell::Text("2".into()),
        Cell::Text(String::new()),
        Cell::Text("NULL".into()),
    ]);
    set
}

#[test]
fn a_cell_view_is_derived_from_the_selection_and_never_stored() {
    let set = set();
    let view = CellView::build(&set, 0, 1, 10).expect("a cell");
    assert_eq!(view.column, "note");
    assert_eq!(view.position, (2, 3));
    assert_eq!(view.row, (1, 2));
    assert_eq!(view.description, "text, 36 characters");
    assert!(view.lines.len() > 1, "a long value wraps");
    for line in &view.lines {
        assert!(value::display_width(line) <= 10);
    }
    assert_eq!(view.lines.concat(), "a fairly long note that will not fit");
}

#[test]
fn a_selection_that_outlived_its_result_yields_nothing_rather_than_a_panic() {
    let set = set();
    assert!(CellView::build(&set, 99, 0, 20).is_none());
    assert!(CellView::build(&set, 0, 99, 20).is_none());

    let empty = ResultSet::new(vec!["id".into()], 10);
    assert!(CellView::build(&empty, 0, 0, 20).is_none());

    let no_columns = ResultSet::new(Vec::new(), 10);
    assert!(CellView::build(&no_columns, 0, 0, 20).is_none());
}

#[test]
fn null_an_empty_string_and_the_text_null_are_told_apart_in_words() {
    // This is the promise made in the NULL_MARKER documentation: where the
    // grid can only show a marker, the inspector says which it is.
    let set = set();
    let null = CellView::build(&set, 0, 2, 20).expect("a cell");
    let empty = CellView::build(&set, 1, 1, 20).expect("a cell");
    let literal = CellView::build(&set, 1, 2, 20).expect("a cell");

    assert_eq!(null.description, "SQL NULL");
    assert!(null.is_null);
    assert!(null.lines.is_empty(), "a NULL has no text to show");

    assert_eq!(empty.description, "empty string, 0 characters");
    assert!(!empty.is_null);
    assert_eq!(empty.lines, vec![""]);

    assert_eq!(literal.description, "text, 4 characters");
    assert_eq!(literal.lines, vec!["NULL"]);

    assert_ne!(null.description, empty.description);
    assert_ne!(null.description, literal.description);
}

#[test]
fn a_value_that_would_drive_the_terminal_is_shown_as_text() {
    let mut set = ResultSet::new(vec!["payload".into()], 10);
    set.push(vec![Cell::Text("\x1b[2J\x1b[Hgone".into())]);
    let view = CellView::build(&set, 0, 0, 40).expect("a cell");
    assert!(
        !view.lines.iter().any(|line| line.contains('\x1b')),
        "an escape reached the inspector: {:?}",
        view.lines
    );
    assert!(view.lines.concat().contains("\\x1B[2J"));

    // A column name is server-controlled too.
    let mut named = ResultSet::new(vec!["\x1b[31mred".into()], 10);
    named.push(vec![Cell::Text("v".into())]);
    let view = CellView::build(&named, 0, 0, 40).expect("a cell");
    assert!(!view.column.contains('\x1b'), "{}", view.column);
}

#[test]
fn counts_describe_the_value_stored_not_the_way_it_was_wrapped() {
    let mut set = ResultSet::new(vec!["doc".into()], 10);
    set.push(vec![Cell::Text("aaaa\nbb".into())]);

    // Four characters, a newline, two characters: seven, whatever the pane
    // does with them. The wrapped line count is a fact about the pane.
    let narrow = CellView::build(&set, 0, 0, 2).expect("a cell");
    let wide = CellView::build(&set, 0, 0, 80).expect("a cell");
    assert_eq!(narrow.description, "text, 7 characters");
    assert_eq!(wide.description, narrow.description);
    assert_eq!(wide.lines, vec!["aaaa", "bb"], "the value's own lines");
    assert_eq!(narrow.lines, vec!["aa", "aa", "bb"]);

    // The grid escapes the newline, because there it would break the row.
    // The inspector uses it, and neither one lets it reach the terminal.
    assert_eq!(Cell::Text("aaaa\nbb".into()).display(), "aaaa\\nbb");
    assert!(!wide.lines.iter().any(|line| line.contains('\n')));
}

#[test]
fn scrolling_stops_at_both_ends_and_states_where_it_is() {
    let mut set = ResultSet::new(vec!["doc".into()], 10);
    set.push(vec![Cell::Text("abcdefghij".into())]);
    let view = CellView::build(&set, 0, 0, 2).expect("a cell");
    assert_eq!(view.lines.len(), 5);

    let mut inspector = Inspector::new();
    assert_eq!(inspector.scroll(), 0);
    inspector.scroll_up();
    assert_eq!(inspector.scroll(), 0, "the top is the top");

    for _ in 0..10 {
        inspector.scroll_down(view.lines.len(), 2);
    }
    assert_eq!(
        inspector.scroll(),
        3,
        "scrolling stops with the last line on screen"
    );

    let (visible, above, below) = view.window(inspector.scroll(), 2);
    assert_eq!(visible, ["gh", "ij"]);
    assert_eq!(above, 3);
    assert_eq!(below, 0);

    let label = view.scroll_label(0, 2).expect("a position");
    assert_eq!(label, "showing 1-2 of 5 wrapped lines (3 below)");
    assert!(
        view.scroll_label(0, 5).is_none(),
        "a value that fits says nothing about scrolling"
    );
    assert!(view.scroll_label(0, 0).is_none());
}

#[test]
fn a_window_never_indexes_past_the_value() {
    let mut set = ResultSet::new(vec!["doc".into()], 10);
    set.push(vec![Cell::Text("abc".into())]);
    let view = CellView::build(&set, 0, 0, 10).expect("a cell");
    let (visible, _, below) = view.window(99, 3);
    assert_eq!(visible, ["abc"], "a stale offset still shows the value");
    assert_eq!(below, 0);
    assert_eq!(view.window(0, 0).0.len(), 0);
}

#[test]
fn an_expanded_row_puts_one_column_on_each_line_with_the_names_aligned() {
    let set = set();
    let fields = expand_row(&set, 0, 40, true);
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name, "id    ");
    assert_eq!(fields[1].name, "note  ");
    assert_eq!(fields[2].name, "amount");
    assert_eq!(fields[0].value, "1");
    assert!(fields[2].is_null);
    assert_eq!(fields[2].value, crate::query::value::NULL_MARKER);

    let widths: Vec<usize> = fields
        .iter()
        .map(|f| value::display_width(&f.name))
        .collect();
    assert!(
        widths.windows(2).all(|w| w[0] == w[1]),
        "names are padded to a common width so values line up"
    );
}

#[test]
fn an_expanded_value_too_wide_for_the_pane_is_marked_as_shortened() {
    let set = set();
    let fields = expand_row(&set, 0, 20, true);
    let note = &fields[1];
    assert!(note.truncated, "it does not fit and says so");
    assert!(note.value.ends_with('\u{2026}'));
    assert!(!fields[0].truncated, "a value that fits is not marked");

    // In ASCII the marker is still there, in ASCII.
    let ascii = expand_row(&set, 0, 20, false);
    assert!(ascii[1].value.ends_with("..."));
}

#[test]
fn an_expanded_view_can_hide_columns_without_shifting_source_values() {
    let set = set();
    let fields = expand_row_columns(&set, 0, 80, true, &[0, 2]);

    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name.trim(), "id");
    assert_eq!(fields[0].value, "1");
    assert_eq!(fields[1].name.trim(), "amount");
    assert_eq!(fields[1].value, crate::query::value::NULL_MARKER);
    assert!(fields[1].is_null);
}

#[test]
fn expanding_a_row_that_is_not_there_yields_nothing() {
    let set = set();
    assert!(expand_row(&set, 99, 40, true).is_empty());
    assert!(expand_row(&ResultSet::new(vec!["a".into()], 10), 0, 40, true).is_empty());
}

#[test]
fn an_extremely_narrow_pane_still_shows_something_of_every_column() {
    // One cell of value per line is the floor. Showing no value at all would
    // be worse than showing one character of it.
    let set = set();
    for width in [0usize, 1, 2, 8] {
        let fields = expand_row(&set, 0, width, true);
        assert_eq!(fields.len(), 3, "width {width}");
        for field in fields {
            assert!(
                !field.value.is_empty(),
                "width {width} showed nothing for {}",
                field.name
            );
        }
    }
}
