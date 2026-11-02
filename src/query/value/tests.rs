use super::*;

#[test]
fn escape_sequences_from_the_database_cannot_reach_the_terminal() {
    // A value crafted to clear the screen and repaint a fake status line.
    let hostile = "\x1b[2J\x1b[H\x1b[31mDROP TABLE orders\x1b[0m";
    let safe = sanitize_for_display(hostile);
    assert!(!safe.contains('\x1b'), "ESC survived: {safe:?}");
    assert!(safe.starts_with("\\x1B[2J"), "{safe}");
    // The readable text is preserved so the value is still inspectable.
    assert!(safe.contains("DROP TABLE orders"));
}

#[test]
fn every_control_character_is_escaped() {
    for code in (0u32..0x20).chain(std::iter::once(0x7f)) {
        let ch = char::from_u32(code).expect("valid char");
        let safe = sanitize_for_display(&ch.to_string());
        assert!(
            !safe.chars().any(|c| (c as u32) < 0x20 || c as u32 == 0x7f),
            "U+{code:04X} survived as {safe:?}"
        );
    }
}

#[test]
fn c1_controls_and_bidi_overrides_are_escaped() {
    let safe = sanitize_for_display("\u{9b}[31m");
    assert!(safe.starts_with("\\u{009B}"), "{safe}");

    let spoof = "user\u{202e}txt.exe";
    let safe = sanitize_for_display(spoof);
    assert!(safe.contains("\\u{202E}"), "{safe}");
    assert!(!safe.contains('\u{202e}'));
}

#[test]
fn ordinary_text_including_unicode_is_left_alone() {
    for value in ["plain", "héllo wörld", "日本語", "emoji 🐘", "R 1,245.00"] {
        assert_eq!(sanitize_for_display(value), value);
    }
}

#[test]
fn whitespace_is_made_visible_rather_than_breaking_alignment() {
    assert_eq!(sanitize_for_display("a\tb\nc\rd"), "a\\tb\\nc\\rd");
}

#[test]
fn null_is_distinguishable_from_empty_string_and_from_the_text_null() {
    let null = Cell::Null;
    let empty = Cell::Text(String::new());
    let literal = Cell::Text("NULL".into());

    assert_eq!(null.display(), NULL_MARKER);
    assert_eq!(empty.display(), "");
    assert_eq!(literal.display(), "NULL");
    assert_ne!(null.display(), empty.display());
    assert_ne!(null.display(), literal.display());

    // Where the display could still be ambiguous, the description is not.
    assert_eq!(null.describe(), "SQL NULL");
    assert_eq!(empty.describe(), "empty string, 0 characters");
    assert_eq!(literal.describe(), "text, 4 characters");
    assert!(null.raw().is_none());
}

#[test]
fn export_preserves_the_server_value_exactly() {
    let cell = Cell::Text("1.100".into());
    assert_eq!(
        cell.export(""),
        "1.100",
        "numeric text must not be reformatted"
    );
    assert_eq!(Cell::Null.export(""), "");
    assert_eq!(Cell::Null.export("\\N"), "\\N");
    // Export is not display: escaping here would corrupt the exported data.
    assert_eq!(Cell::Text("a\tb".into()).export(""), "a\tb");
}

#[test]
fn retained_values_have_a_byte_bound_but_streaming_cells_do_not() {
    let value = "x".repeat(MAX_RETAINED_VALUE_BYTES + 1);
    assert_eq!(
        Cell::from_option_bounded(Some(&value)),
        Err(MAX_RETAINED_VALUE_BYTES + 1)
    );
    assert_eq!(
        Cell::from_option_bounded(Some("x")),
        Ok(Cell::Text("x".into()))
    );
    assert_eq!(Cell::from_option_bounded(None), Ok(Cell::Null));
}

#[test]
fn width_counts_terminal_cells_not_characters() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("日本語"), 6, "wide characters take two cells");
    assert_eq!(
        display_width("e\u{301}"),
        1,
        "combining accents add no width"
    );
}

#[test]
fn truncation_respects_wide_characters_and_never_splits_one() {
    assert_eq!(truncate_to_width("abcdef", 10, true), "abcdef");
    assert_eq!(truncate_to_width("abcdef", 4, true), "abc\u{2026}");
    assert_eq!(truncate_to_width("abcdef", 4, false), "a...");

    let wide = truncate_to_width("日本語です", 5, true);
    assert!(
        display_width(&wide) <= 5,
        "{wide} was {} cells",
        display_width(&wide)
    );
    assert!(wide.ends_with('\u{2026}'));

    // Degenerate widths must not panic.
    assert_eq!(display_width(&truncate_to_width("abcdef", 1, true)), 1);
    assert_eq!(truncate_to_width("abcdef", 0, true), "");
}

#[test]
fn wrapping_fills_each_line_to_the_width_and_never_splits_a_character() {
    assert_eq!(wrap_to_width("abcdef", 3), vec!["abc", "def"]);
    assert_eq!(wrap_to_width("abcdefg", 3), vec!["abc", "def", "g"]);
    assert_eq!(wrap_to_width("ab", 10), vec!["ab"]);

    // A wide character that does not fit in the remainder moves down whole.
    let lines = wrap_to_width("a日本", 3);
    assert_eq!(lines, vec!["a日", "本"]);
    for line in &lines {
        assert!(display_width(line) <= 3, "{line} overflows");
    }
}

#[test]
fn wrapping_keeps_the_line_breaks_the_value_already_had() {
    // The value's own shape is a fact about the data. An empty line in the
    // middle of a JSON document is part of it.
    assert_eq!(wrap_to_width("one\ntwo", 10), vec!["one", "two"]);
    assert_eq!(wrap_to_width("one\n\ntwo", 10), vec!["one", "", "two"]);
    assert_eq!(
        wrap_to_width("", 10),
        vec![""],
        "an empty value is one line"
    );
    assert_eq!(
        wrap_to_width("abcdef\ngh", 3),
        vec!["abc", "def", "gh"],
        "wrapping applies within each line"
    );
}

#[test]
fn wrapping_to_no_width_yields_nothing_rather_than_looping() {
    assert!(wrap_to_width("anything", 0).is_empty());
    assert_eq!(wrap_to_width("abc", 1), vec!["a", "b", "c"]);
    // A wide character in a one-column pane cannot fit. It goes on a line of
    // its own and overflows by one cell rather than disappearing: a value
    // that is not shown is worse than a value that is clipped by the frame.
    assert_eq!(wrap_to_width("日", 1), vec!["日"]);
}

#[test]
fn padding_aligns_by_display_width() {
    assert_eq!(pad_to_width("ab", 5), "ab   ");
    assert_eq!(display_width(&pad_to_width("日本", 6)), 6);
    assert_eq!(
        pad_to_width("toolong", 3),
        "toolong",
        "padding never truncates"
    );
}

#[test]
fn sanitised_output_is_stable_under_repetition() {
    let value = "\x1b[31mred\x1b[0m";
    let once = sanitize_for_display(value);
    assert_eq!(sanitize_for_display(&once), once);
}
