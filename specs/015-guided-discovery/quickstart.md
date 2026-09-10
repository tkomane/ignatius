# Quickstart: Guided discovery

Run these checks from the repository root. They exercise the feature without a
database; the full verifier remains the authoritative repository gate when the
disposable database fixtures are available.

## Focused checks

```bash
cargo fmt --all --check
cargo check --locked
cargo test --locked app::discovery --lib
cargo test --locked app::palette --lib
cargo test --locked app::update --lib
cargo test --locked ui::keymap --lib
cargo test --locked ui::layout --lib
cargo test --locked --test documentation_matches_the_build
git diff --check
```

Expected outcomes:

- Discovery context and empty-state tests pass for disconnected, connected,
  busy, empty, filtered, blocked, and populated states.
- Hints change with focus, use active keys, stay at or below five entries, and
  retain meaning in ASCII and no-colour rendering.
- Command-palette searches find applicable intent and never emit an effect just
  by opening, searching, or dismissing.
- Existing palette, keymap, renderer, documentation, script, and export tests
  remain green.

## Full repository evidence

```bash
cargo xtask db up
cargo --locked xtask verify
cargo xtask db down
cargo xtask db status
```

Record formatting, lints, API documentation, workspace tests, documentation
tests, database-backed tests, and Unix-socket or platform skips separately in
`docs/status.md`. A skip is not a pass.

## Manual smoke path

At a supported terminal size:

1. Start with an empty editor and read the first-frame next action.
2. Press Tab through Editor, Results, and Objects and confirm the footer follows
   the focused pane.
3. Press `Ctrl+P`, search `run`, `help`, and `result`, then dismiss with Escape.
4. Enter a harmless query only when connected, then repeat the Results and
   empty-filter states.
5. Repeat with ASCII and no-colour presentation and note any platform-specific
   terminal behaviour separately from automated evidence.
