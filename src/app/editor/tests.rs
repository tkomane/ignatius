use super::*;

fn typed(text: &str) -> Editor {
    let mut editor = Editor::default();
    for ch in text.chars() {
        if ch == '\n' {
            editor.insert_newline();
        } else {
            editor.insert(ch);
        }
    }
    editor
}

#[test]
fn completion_replacement_is_one_undoable_edit() {
    let mut editor = Editor::with_text("SELECT ord");
    editor.replace_range(7..10, "\"orders\"");
    assert_eq!(editor.text(), "SELECT \"orders\"");
    assert_eq!(editor.cursor(), "SELECT \"orders\"".len());
    assert!(editor.is_modified());

    editor.undo();
    assert_eq!(editor.text(), "SELECT ord");
    assert_eq!(editor.cursor(), "SELECT ord".len());
    assert!(!editor.can_undo());
}

#[test]
fn completion_replacement_preserves_utf8_boundaries() {
    let mut editor = Editor::with_text("SELECT café");
    let start = "SELECT ".len();
    editor.replace_range(start..editor.text().len(), "\"café\"");
    assert_eq!(editor.text(), "SELECT \"café\"");
    assert_eq!(editor.cursor(), editor.text().len());
}

#[test]
fn formatted_buffer_replacement_is_one_edit_with_cursor_recovery() {
    let source = "select café,total from orders where total>0;";
    let source_cursor = source.find("total>0").expect("token") + "total".len();
    let mut editor = Editor::with_text(source);
    editor.set_cursor(source_cursor);
    let formatted = crate::query::format_sql(source, source_cursor).expect("format");

    editor.replace_range(0..source.len(), &formatted.text);
    editor.set_cursor(formatted.cursor);
    assert!(editor.is_modified());
    assert!(editor.can_undo());
    assert!(editor.text().is_char_boundary(editor.cursor()));

    editor.undo();
    assert_eq!(editor.text(), source);
    assert_eq!(editor.cursor(), source_cursor);
    assert!(!editor.can_undo());
    editor.redo();
    assert_eq!(editor.text(), formatted.text);
    assert_eq!(editor.cursor(), formatted.cursor);
}

#[test]
fn formatted_cursor_fixtures_stay_on_ascii_and_unicode_character_boundaries() {
    for (source, token, inside) in [
        ("select total from orders;", "total", "tot"),
        ("select café from orders;", "café", "ca"),
    ] {
        let source_cursor = source.find(token).expect("token") + inside.len();
        let formatted = crate::query::format_sql(source, source_cursor).expect("format");
        let mut editor = Editor::with_text(source);
        editor.set_cursor(source_cursor);
        editor.replace_range(0..source.len(), &formatted.text);
        editor.set_cursor(formatted.cursor);
        assert!(editor.text().is_char_boundary(editor.cursor()));
        assert!(editor.cursor() <= editor.text().len());
    }
}

#[test]
fn a_noop_format_replacement_does_not_change_revision_or_history() {
    let source = "SELECT 1\nFROM orders;";
    let mut editor = Editor::with_text(source);
    editor.set_cursor(4);
    let revision = editor.revision();
    let cursor = editor.cursor();
    editor.replace_range(0..source.len(), source);
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.cursor(), cursor);
    assert!(!editor.can_undo());
    assert!(!editor.is_modified());
}

#[test]
fn text_revision_changes_for_edits_but_not_cursor_positioning() {
    let mut editor = Editor::with_text("SELECT café");
    let initial = editor.revision();
    editor.set_cursor("SELECT ".len() + "caf".len());
    assert_eq!(editor.revision(), initial);
    assert!(!editor.is_modified());
    assert!(!editor.can_undo());

    editor.insert('!');
    assert_eq!(editor.revision(), initial + 1);
    editor.undo();
    assert_eq!(editor.revision(), initial + 2);
    editor.redo();
    assert_eq!(editor.revision(), initial + 3);
}

#[test]
fn cursor_setter_clamps_invalid_offsets_without_splitting_utf8() {
    let mut editor = Editor::with_text("café");
    editor.set_cursor(4);
    assert_eq!(
        editor.cursor(),
        3,
        "the caret backs up to a character boundary"
    );
    editor.set_cursor(999);
    assert_eq!(editor.cursor(), editor.text().len());
}

#[test]
fn vertical_movement_keeps_the_column_it_started_from() {
    // The behaviour every editor has and every text field lacks: passing
    // through a short line must not forget where the cursor was.
    let mut editor = Editor::with_text("SELECT customer_id\nFROM x\nWHERE total > 100");
    editor.move_buffer_start();
    for _ in 0..15 {
        editor.move_right();
    }
    assert_eq!(editor.position(), (1, 16));

    editor.move_down();
    assert_eq!(editor.position(), (2, 7), "the short line clamps");
    editor.move_down();
    assert_eq!(
        editor.position(),
        (3, 16),
        "and the column comes back on a line long enough for it"
    );

    editor.move_up();
    editor.move_up();
    assert_eq!(editor.position(), (1, 16));
}

#[test]
fn vertical_movement_stops_at_the_ends_rather_than_doing_nothing() {
    let mut editor = Editor::with_text("one\ntwo");
    editor.move_buffer_start();
    editor.move_right();
    editor.move_up();
    assert_eq!(editor.cursor(), 0, "up on the first line goes to the start");

    editor.move_buffer_end();
    editor.move_left();
    editor.move_down();
    assert_eq!(
        editor.cursor(),
        editor.text().len(),
        "down on the last line goes to the end"
    );
}

#[test]
fn line_and_buffer_keys_land_where_they_say() {
    let mut editor = Editor::with_text("SELECT 1\nFROM orders\nWHERE id = 2");
    editor.move_buffer_start();
    editor.move_down();
    editor.move_line_end();
    assert_eq!(editor.position(), (2, 12));
    editor.move_line_start();
    assert_eq!(editor.position(), (2, 1));
    editor.move_buffer_end();
    assert_eq!(editor.position(), (3, 13));
    editor.move_buffer_start();
    assert_eq!(editor.position(), (1, 1));
}

#[test]
fn word_movement_treats_identifiers_and_punctuation_as_separate_words() {
    let mut editor = Editor::with_text("SELECT orders.customer_id FROM t");
    editor.move_buffer_start();

    editor.move_word_right();
    assert_eq!(editor.position().1, 7, "past SELECT");
    editor.move_word_right();
    assert_eq!(editor.position().1, 14, "past orders");
    editor.move_word_right();
    assert_eq!(editor.position().1, 15, "past the dot on its own");
    editor.move_word_right();
    assert_eq!(editor.position().1, 26, "past customer_id");

    editor.move_word_left();
    assert_eq!(editor.position().1, 15, "back to the start of customer_id");
    editor.move_word_left();
    assert_eq!(editor.position().1, 14);
    editor.move_word_left();
    assert_eq!(editor.position().1, 8, "back to the start of orders");
}

#[test]
fn word_movement_stops_at_the_ends() {
    let mut editor = Editor::with_text("word");
    editor.move_buffer_start();
    editor.move_word_left();
    assert_eq!(editor.cursor(), 0);
    editor.move_buffer_end();
    editor.move_word_right();
    assert_eq!(editor.cursor(), 4);

    let mut empty = Editor::default();
    empty.move_word_left();
    empty.move_word_right();
    empty.move_up();
    empty.move_down();
    assert_eq!(empty.cursor(), 0, "an empty buffer survives every key");
}

#[test]
fn a_new_line_keeps_the_indentation_of_the_one_it_left() {
    let mut editor = typed("SELECT 1\n    FROM orders");
    editor.insert_newline();
    editor.insert('W');
    assert_eq!(
        editor.text(),
        "SELECT 1\n    FROM orders\n    W",
        "indented SQL that loses its indent is SQL people stop indenting"
    );

    // Splitting a line in the middle copies only the indentation before the
    // cursor, so no leading space is invented.
    let mut editor = Editor::with_text("  abc");
    editor.move_buffer_start();
    editor.move_right();
    editor.insert_newline();
    assert_eq!(
        editor.text(),
        " \n  abc",
        "only the indent before the cursor is copied; the rest of the line \
         keeps its own characters"
    );
}

#[test]
fn deleting_forwards_backwards_and_by_word_all_respect_characters() {
    let mut editor = Editor::with_text("SELECT héllo 日本;");
    editor.move_buffer_end();
    editor.delete_word_left();
    assert_eq!(editor.text(), "SELECT héllo 日本");
    editor.delete_word_left();
    assert_eq!(editor.text(), "SELECT héllo ");
    assert!(editor.text().is_char_boundary(editor.cursor()));

    editor.move_buffer_start();
    editor.delete_forward();
    assert_eq!(editor.text(), "ELECT héllo ");
    for _ in 0..50 {
        editor.delete_forward();
    }
    assert!(editor.text().is_empty());
    editor.delete_forward();
    editor.backspace();
    assert!(editor.text().is_empty(), "both ends are no-ops, not panics");
}

#[test]
fn undo_takes_back_a_word_at_a_time_and_redo_puts_it_back() {
    let mut editor = typed("SELECT one two");
    assert_eq!(editor.text(), "SELECT one two");

    editor.undo();
    assert_eq!(editor.text(), "SELECT one ", "a word is one step");
    editor.undo();
    assert_eq!(editor.text(), "SELECT ");
    assert!(editor.can_redo());

    editor.redo();
    assert_eq!(editor.text(), "SELECT one ");
    editor.redo();
    assert_eq!(editor.text(), "SELECT one two");
    assert!(!editor.can_redo());

    // Typing after an undo makes the redo path unreachable, as everywhere.
    editor.undo();
    editor.insert('!');
    assert!(!editor.can_redo());
}

#[test]
fn undo_covers_deletions_and_a_replaced_buffer() {
    let mut editor = Editor::with_text("SELECT 1;");
    editor.set_text("DROP TABLE orders;");
    assert_eq!(editor.text(), "DROP TABLE orders;");
    editor.undo();
    assert_eq!(
        editor.text(),
        "SELECT 1;",
        "text arriving from elsewhere is exactly what someone wants back"
    );

    editor.move_buffer_end();
    editor.delete_word_left();
    editor.backspace();
    assert_eq!(editor.text(), "SELECT ");
    editor.undo();
    editor.undo();
    assert_eq!(editor.text(), "SELECT 1;");
}

#[test]
fn undo_on_an_untouched_buffer_does_nothing_at_all() {
    let mut editor = Editor::with_text("SELECT 1");
    assert!(!editor.can_undo());
    editor.undo();
    editor.redo();
    assert_eq!(editor.text(), "SELECT 1");
    assert_eq!(editor.cursor(), 8);
}

#[test]
fn the_history_is_bounded_so_a_long_session_cannot_grow_without_end() {
    let mut editor = Editor::default();
    for index in 0..(HISTORY_LIMIT * 2) {
        // Whitespace ends each run, so every character is its own step.
        editor.insert(char::from_digit((index % 10) as u32, 10).unwrap_or('0'));
        editor.insert(' ');
    }
    assert!(editor.past.len() <= HISTORY_LIMIT);
    // The recent past still works, which is the part anyone reaches for.
    let before = editor.text().to_owned();
    editor.undo();
    assert_ne!(editor.text(), before);
}

#[test]
fn multibyte_text_is_never_split_by_any_movement() {
    let mut editor = typed("SELECT 'héllo 日本';");
    assert_eq!(editor.text(), "SELECT 'héllo 日本';");
    for _ in 0..40 {
        editor.move_left();
        assert!(editor.text().is_char_boundary(editor.cursor()));
    }
    for _ in 0..40 {
        editor.move_right();
        assert!(editor.text().is_char_boundary(editor.cursor()));
    }
    editor.move_buffer_start();
    editor.move_word_right();
    assert!(editor.text().is_char_boundary(editor.cursor()));
}

#[test]
fn a_page_moves_by_a_screenful_and_stops_at_the_end() {
    let mut editor = Editor::with_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
    editor.move_buffer_start();
    editor.move_page_down(4);
    assert_eq!(editor.position().0, 5);
    editor.move_page_up(4);
    assert_eq!(editor.position().0, 1);
    editor.move_page_down(100);
    assert_eq!(editor.position().0, 10, "it stops at the last line");
}

#[test]
fn loading_text_is_not_an_edit_but_typing_is() {
    let mut editor = Editor::with_text("SELECT 1");
    assert!(!editor.is_modified());
    editor.set_text("SELECT 2");
    assert!(!editor.is_modified(), "the buffer matches what it holds");
    editor.insert('!');
    assert!(editor.is_modified());
}

#[test]
fn moving_the_cursor_ends_the_undo_run() {
    // Typing in one place, going somewhere else, and typing there is two
    // changes. Before this was fixed they shared one undo step, so a single
    // undo took back work in a place the cursor was no longer near.
    let mut editor = typed("SELECT");
    editor.move_buffer_start();
    for ch in "x".chars() {
        editor.insert(ch);
    }

    editor.undo();
    assert_eq!(editor.text(), "SELECT", "only the second run comes back");

    editor.undo();
    assert_eq!(editor.text(), "", "and the first run is a step of its own");
}

#[test]
fn every_movement_ends_the_undo_run_not_just_the_arrow_keys() {
    // One test per key, because the run is ended in each method and an
    // added movement that forgets to do it would otherwise go unnoticed.
    type Movement = fn(&mut Editor);
    let movements: Vec<(&str, Movement)> = vec![
        ("left", |e| e.move_left()),
        ("right", |e| e.move_right()),
        ("up", |e| e.move_up()),
        ("down", |e| e.move_down()),
        ("line start", |e| e.move_line_start()),
        ("line end", |e| e.move_line_end()),
        ("buffer start", |e| e.move_buffer_start()),
        ("buffer end", |e| e.move_buffer_end()),
        ("word left", |e| e.move_word_left()),
        ("word right", |e| e.move_word_right()),
        ("page up", |e| e.move_page_up(4)),
        ("page down", |e| e.move_page_down(4)),
    ];

    // No trailing space or newline: a run that ends mid-word is the only one
    // the next insert would coalesce into, so this can actually fail. The
    // first version of this test typed a trailing newline, which starts a
    // fresh undo step by itself, and it passed with the fix taken out.
    let source = "SELECT one\nFROM two\nWHERE three\nAND four";

    for (name, movement) in movements {
        let mut editor = typed(source);
        movement(&mut editor);
        editor.insert('x');
        editor.undo();
        assert_eq!(
            editor.text(),
            source,
            "moving by {name} did not end the undo run, so one undo took back \
             the typing before the move as well"
        );
    }
}

#[test]
fn a_crlf_and_cr_paste_normalises_to_lf_and_inserts_at_the_caret() {
    let mut editor = Editor::with_text("SELECT ");
    assert!(editor.insert_paste("one\r\ntwo\rthree"));
    assert_eq!(editor.text(), "SELECT one\ntwo\nthree");
    assert_eq!(editor.cursor(), editor.text().len());
    assert!(editor.is_modified());
}

#[test]
fn a_paste_is_one_undoable_edit_that_restores_the_prior_text_and_cursor() {
    let mut editor = Editor::with_text("SELECT 1;\nFROM t;");
    editor.set_cursor("SELECT 1;".len());
    let before_text = editor.text().to_owned();
    let before_cursor = editor.cursor();
    let before_revision = editor.revision();

    assert!(editor.insert_paste(" WHERE x\r\n"));

    assert_eq!(editor.revision(), before_revision + 1);
    assert!(editor.can_undo());
    editor.undo();
    assert_eq!(editor.text(), before_text);
    assert_eq!(editor.cursor(), before_cursor);
    assert!(!editor.can_undo());
}

#[test]
fn a_paste_at_the_limit_is_accepted_and_one_byte_more_is_refused() {
    let accepted = "a".repeat(PASTE_LIMIT_BYTES);
    let mut editor = Editor::with_text("SELECT ");
    let revision = editor.revision();
    assert!(editor.insert_paste(&accepted));
    assert_eq!(editor.text().len(), "SELECT ".len() + PASTE_LIMIT_BYTES);
    assert_eq!(editor.revision(), revision + 1);
    assert!(editor.is_modified());

    let refused_text = "b".repeat(PASTE_LIMIT_BYTES + 1);
    let mut refused = Editor::with_text("SELECT ");
    let before_text = refused.text().to_owned();
    let before_revision = refused.revision();
    let before_cursor = refused.cursor();
    assert!(!refused.insert_paste(&refused_text));
    assert_eq!(refused.text(), before_text);
    assert_eq!(refused.revision(), before_revision);
    assert_eq!(refused.cursor(), before_cursor);
    assert!(!refused.can_undo());
    assert!(!refused.is_modified());
}

#[test]
fn an_empty_paste_changes_nothing() {
    let mut editor = Editor::with_text("SELECT 1");
    let before_text = editor.text().to_owned();
    let before_revision = editor.revision();
    let before_cursor = editor.cursor();

    assert!(!editor.insert_paste(""));

    assert_eq!(editor.text(), before_text);
    assert_eq!(editor.revision(), before_revision);
    assert_eq!(editor.cursor(), before_cursor);
    assert!(!editor.can_undo());
    assert!(!editor.is_modified());
}
