# Quickstart: GUI-grade experience

## Prerequisites

- Rust and Cargo from the pinned project toolchain.
- A disposable PostgreSQL instance for the existing integration checks
  (`cargo xtask db up`), started and torn down only by the integrator.
- A truecolor-capable terminal for the manual checks, plus one 256-colour
  session (`TERM=xterm-256color` without `COLORTERM`) for the depth check.

## Automated validation

From the repository root, per slice as named in `tasks.md`, then in full:

```text
cargo fmt --all -- --check
cargo test --locked --lib ui::widgets
cargo test --locked --lib ui::theme
cargo test --locked --lib ui::layout
cargo test --locked --lib app::editor::tests::paste
cargo test --locked --lib app::update::tests::mouse
cargo test --locked --lib app::update::tests::theme_switch
cargo test --locked --lib app::grid
cargo test --locked --lib connection::service::tests::connecting_steps
cargo test --locked --lib config
cargo test --locked --test documentation_matches_the_build
cargo --locked xtask verify
```

Expected outcomes:

- the widget extraction changes no rendered output: the pre-existing
  layout tests pass unchanged after moving;
- every frame at truecolor and 256 depth paints the three elevation
  surfaces; 16-colour and colour-off renders carry the same words with no
  painted surface;
- the extended parity matrix (three built-ins, a valid fixture theme, a
  hostile fixture theme, colour on and off, three glyph tiers) agrees on
  every meaning word;
- pasted text lands as one undoable edit, bounded and normalised, and a
  paste is never silently dropped;
- every mouse verb produces the same state as its keyboard route, and the
  full interaction set passes with `ui.mouse = false`;
- invalid `[theme]` tables are refused before the terminal is taken with
  the contract's exact wording and exit 3;
- splits, zoom and density obey the pinned bounds and tables;
- the locked verifier reports every gate separately from any semantic
  environment skip.

## Manual interactive check

1. Start the disposable database with `cargo xtask db up`.
2. Open the interactive client and confirm the whole frame is painted:
   no cell shows the terminal's own background around panes or overlays.
3. Watch the connecting display name its steps against a real target, and
   confirm the first connected frame has the tree's first level open,
   focus in the editor and the run hint visible.
4. Paste a multi-line query from another application; confirm it lands at
   the caret, one undo removes it, and nothing runs.
5. Run a query; click a header to sort, click and double-click cells,
   scroll with the wheel and with Shift held, drag a column edge, and
   repeat each by keyboard; confirm identical results and honest range
   wording such as `rows 120-160 of 1,248 retained`.
6. Drag the splits, use `Ctrl+K ,` / `Ctrl+K .` / `Ctrl+K z`, and confirm
   restore is exact.
7. Switch themes from the palette; set `ui.theme = "auto"` in a terminal
   that answers OSC 11 and one that does not; run `ignatius doctor` and
   confirm the answered and unanswered wording.
8. Define a `[theme]` with one bad value and confirm the exact refusal
   line and exit 3 before the terminal is taken.
9. Set `ui.mouse = false` and repeat step 5 by keyboard only.
10. Repeat a spot check with `--ascii --color never` and in a session
    with `TERM=xterm-256color` and no `COLORTERM`.

Manual terminal, screen-reader and cross-platform checks remain evidence
gates separate from these automated scenarios. A skipped gate is not a
pass.
