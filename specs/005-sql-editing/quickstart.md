# 005 SQL editing validation guide

This guide distinguishes the current committed evidence from the open
acceptance work. It does not claim that Feature 005 is complete.

## Automated validation

From the repository root:

```text
cargo test --lib app::editor
cargo test --lib app::update::tests::editing_keys_reach_the_editor_and_nothing_else
cargo test --lib ui::layout::tests::the_editor_window_follows_the_cursor_through_a_long_buffer
cargo xtask verify
```

The full verifier needs both disposable PostgreSQL services for its integration
gate:

```text
cargo xtask db up
cargo xtask verify
```

Do not put a real password in a command or fixture. The repository launcher
prints a synthetic connection route and stores its password in `docker/dev.env`.

## Required focused scenarios before completion

1. Start with an indented multi-line buffer, press Enter through the reducer,
   and confirm the new line inherits only the indentation before the cursor.
2. Type a word, move the cursor, type another word, and undo once. The undo
   must not merge edits on opposite sides of the movement.
3. Resolve each new key binding and confirm the description agrees with the
   action in the keymap.
4. In a real pty, type an eight-line statement, move to line two, correct a
   word, return to the end and run it. Capture the line numbers and result.

## Evidence boundary

Unit and layout tests prove model and rendering invariants. Reducer tests prove
focus and action routing. A pty capture proves the actual terminal path. None
of these proves Windows or Linux hand use, screen-reader experience, or query
history privacy controls.
