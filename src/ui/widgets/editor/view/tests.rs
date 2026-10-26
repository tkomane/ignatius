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

#[test]
fn production_projection_retains_only_visible_units_plus_caret() {
    let text = "x".repeat(1_048_576);
    let capacity = 64;
    let line = LineView::project(&text, 0, capacity, Some(text.len() - 1));
    assert!(
        line.units.len() <= capacity + 1,
        "off-screen source units must not all remain allocated: {}",
        line.units.len()
    );
}

#[test]
fn projected_units_keep_whole_provenance_width_and_far_caret() {
    let text = "界\tX".repeat(1000);
    for (offset, capacity) in [(1, 1), (2, 3), (100, 0), (4998, 4)] {
        let line = LineView::project(&text, offset, capacity, Some(text.len() - 1));
        assert!(line.units.len() <= capacity + 1);
        assert_eq!(line.width, 5000);
        assert_eq!(line.end_column, 3001);
        for unit in &line.units {
            assert!(
                unit.source.contains(&(text.len() - 1))
                    || (unit.width > 0
                        && unit.cell < offset + capacity
                        && offset < unit.cell + unit.width)
            );
        }
    }
}

#[test]
fn zero_width_offscreen_units_are_not_retained_unless_they_own_caret() {
    let text = "\u{301}".repeat(70_000);
    let none = LineView::project(&text, 0, 1, None);
    assert!(none.units.is_empty());
    let caret = LineView::project(&text, 0, 1, Some(2));
    assert_eq!(caret.units.len(), 1);
    assert_eq!(caret.units[0].source, 0..text.len());
}

#[test]
fn unchanged_printable_ascii_matches_public_renderer_graphemes() {
    let text = (b' '..=b'~').map(char::from).collect::<String>();
    let span = Span::raw(&text);
    let expected = span.styled_graphemes(Style::default()).collect::<Vec<_>>();
    let actual = LineView::project(&text, 0, usize::MAX, None);
    assert_eq!(actual.units.len(), expected.len());
    for (byte, (unit, grapheme)) in actual.units.iter().zip(expected).enumerate() {
        assert_eq!(unit.text, grapheme.symbol);
        assert_eq!(unit.source, byte..byte + 1);
        assert_eq!((unit.column, unit.cell, unit.width), (byte + 1, byte, 1));
    }
}

#[test]
fn real_controls_del_and_literal_escapes_preserve_sanitizer_boundaries() {
    let controls = (0u8..=31)
        .chain(std::iter::once(127))
        .map(char::from)
        .collect::<String>();
    let actual = LineView::project(&controls, 0, usize::MAX, None);
    assert_eq!(actual.units.len(), 33);
    for (byte, unit) in actual.units.iter().enumerate() {
        assert_eq!(unit.source, byte..byte + 1);
        assert_eq!(unit.text, sanitize_for_display(&controls[byte..byte + 1]));
        assert!(unit.width > 1);
    }
    let literal = r"\t\x7F\u{009B}";
    let actual = LineView::project(literal, 0, usize::MAX, None);
    assert_eq!(actual.units.len(), literal.len());
    assert!(actual.units.iter().all(|unit| unit.width == 1));
    assert_eq!(
        actual
            .units
            .iter()
            .map(|unit| unit.text.as_ref())
            .collect::<String>(),
        literal
    );
}
