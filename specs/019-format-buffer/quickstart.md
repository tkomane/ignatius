# Quickstart: Format the SQL buffer

## Focused automated checks

```bash
cargo fmt --all -- --check
cargo test --locked query::format --lib
cargo test --locked app::editor --lib
cargo test --locked app::update --lib
cargo test --locked app::discovery --lib
cargo test --locked ui::keymap --lib
cargo test --locked ui::layout --lib
cargo test --locked cli::plain --lib
cargo test --locked --test documentation_matches_the_build
cargo test --locked --test cli_contract
```

The focused tests must prove protected-byte preservation, idempotence, cursor
mapping, one-step undo/redo, refusal paths, shortcut/chord/palette discovery,
value-free rendering, plain-mode `\\format`, and no query or machine-output
side effects.

## Manual interactive check

1. Start the disposable fixture with `cargo xtask db up` only when a connected
   session is needed, then open the full-screen client in a real terminal.
2. Put a compact query such as `select o.id,o.total from orders o where
   o.total>0 and o.status='ready';` in the editor.
3. Invoke `Ctrl+Shift+F`, then repeat it. Confirm the second invocation reports
   that the buffer is already formatted.
4. Move into a token, format, press Undo once, and Redo once. Confirm the token
   and complete text return exactly.
5. Repeat in compact, narrow, ASCII, no-colour, and reduced-motion modes. Check
   that status words remain visible and no protected text is echoed in refusal
   messages.
6. In plain mode, type a multi-line statement without its semicolon, enter
   `\\format`, and confirm the formatted preview is on the message stream while
   the statement remains pending.

## Release evidence boundary

Record the terminal name, operating system, terminal size, glyph and colour
tier, and whether the manual interaction was completed. Automated renderer
tests are not a substitute for hand terminal or screen-reader evidence.

After focused checks, run the repository authority:

```bash
cargo --locked xtask verify
cargo xtask db down
cargo xtask db status
```

Report every pass, skip, failure, live PostgreSQL version, and unverified
platform or terminal assumption separately. Do not commit, push, tag, sign,
release, or publish without owner authorization.
