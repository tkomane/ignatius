use super::*;

#[test]
fn display_units_keep_renderer_clusters_and_complete_control_escapes() {
    let line = LineView::new("界e\u{301}👩\u{200d}💻\t\x1b");
    assert_eq!(
        line.units
            .iter()
            .map(|u| u.text.as_ref())
            .collect::<Vec<_>>(),
        ["界", "e\u{301}", "👩\u{200d}💻", "\\t", "\\x1B"]
    );
    assert_eq!(
        line.units.iter().map(|u| u.width).collect::<Vec<_>>(),
        [2, 1, 2, 2, 4]
    );
    assert_eq!(
        line.units.iter().map(|u| u.column).collect::<Vec<_>>(),
        [1, 2, 4, 7, 8]
    );
    assert!(
        line.units
            .iter()
            .all(|u| !u.text.chars().any(char::is_control))
    );
}

#[test]
fn partial_wide_and_escape_units_become_exact_intersection_blanks() {
    let line = LineView::new("界\tX");
    assert_eq!(line.units[0].clipped(1, 3), Some((0, " ".into())));
    assert_eq!(line.units[1].clipped(1, 2), Some((1, " ".into())));
    assert_eq!(line.units[1].clipped(1, 3), Some((1, "\\t".into())));
    assert_eq!(line.column_at(0), 1);
    assert_eq!(line.column_at(1), 1);
    assert_eq!(line.column_at(2), 2);
    assert_eq!(line.column_at(3), 2);
    assert_eq!(line.column_at(5), 4);
}

#[test]
fn interior_scalar_cursor_selects_whole_combining_and_zwj_cluster() {
    for (text, cursor) in [("e\u{301}X", 1), ("👩\u{200d}💻X", "👩".len())] {
        let line = LineView::new(text);
        let view = Viewport::new(&line, cursor, 8);
        assert_eq!(view.offset, 0);
        assert_eq!(view.caret_cell, 0);
        assert_eq!(view.caret_column, 1);
        assert!(!view.blank_caret);
    }
}

#[test]
fn tiny_capacity_zero_width_and_eol_use_source_mapped_blank() {
    for (text, cursor, column) in [("界X", 0, 1), ("\tX", 0, 1), ("\u{301}", 0, 1), ("X", 1, 2)] {
        let line = LineView::new(text);
        let view = Viewport::new(&line, cursor, 1);
        assert!(view.blank_caret);
        assert_eq!(view.caret_cell, 0);
        assert_eq!(view.caret_column, column);
    }
}

#[test]
fn offset_beyond_u16_recomputes_for_home_and_resize() {
    let text = "x".repeat(70_000);
    let line = LineView::new(&text);
    assert_eq!(Viewport::new(&line, text.len(), 10).offset, 69_991);
    assert_eq!(Viewport::new(&line, 0, 10).offset, 0);
    assert_eq!(Viewport::new(&line, 6, 10).offset, 0);
    assert_eq!(Viewport::new(&line, 6, 5).offset, 2);
    assert_eq!(Viewport::new(&line, text.len(), 70_001).offset, 0);
}

#[test]
fn caret_line_tracks_utf8_relative_offset_and_empty_trailing_line() {
    let mut editor = Editor::with_text("界\n\nabc");
    editor.set_cursor("界\n".len());
    assert_eq!(caret_line(&editor), ("", 0));
    editor.set_cursor(editor.text().len());
    assert_eq!(caret_line(&editor), ("abc", 3));
}
