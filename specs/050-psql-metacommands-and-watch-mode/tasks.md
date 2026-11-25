# Tasks: psql Meta-Commands and Bounded Watch Mode (050)

- [x] T001: Author `specs/050-psql-metacommands-and-watch-mode/{spec.md,plan.md,tasks.md}` defining meta-command surface, watch bounds, and lexical safety rules.
- [x] T002: Implement `src/metacommand/mod.rs` pure lexer, quote/comment-aware `\watch` scanner, pattern escaper, catalog SQL generator, `help_result()`, and refusal diagnostics.
- [x] T003: Add `src/metacommand/tests.rs` sidecar unit tests for all commands, schema-qualified patterns, quote injection safety, `\x`/`\timing`, `\watch` intervals, companion SQL, and `\!` refusal.
- [x] T004: Register `pub mod metacommand;` in `src/lib.rs` and wire dispatch, expanded/timing toggles, and bounded watch state into `src/app/`.
- [x] T005: Wire meta-command rendering and watch status indicators in `src/ui/` and interactive execution.
- [x] T006: Add end-to-end tests against live PostgreSQL covering `\d`, `\dt`, `\l+`, `\conninfo`, and bounded `\watch`.
- [x] T007: Update README, keymap/help docs, and compatibility documentation for supported meta-commands.
- [x] T008: Run `cargo xtask verify` and record verification evidence in `docs/status.md`.

- [x] T009 Stop the prior connection's watch and clear its result SQL on an
  accepted profile switch. Verify the old timer, bare `\watch`, a new
  query and a refused switch in the reducer. Check the normal interactive
  route using disposable local profiles.

Two reducer regressions failed on `e8fd6ff` and the rejected-switch control
passed. The fix passed all three. An owned macOS arm64 PTY and PostgreSQL
18.7 confirmed the first watch repeated, ended on the second profile, and
bare `\watch` needed a query from that profile before repeating it.
[Wave 80](../../docs/status.md#wave-80---retire-watch-state-on-profile-switch---25-november-2026)
