# Tasks: psql Meta-Commands and Bounded Watch Mode (050)

- [x] T001: Author `specs/050-psql-metacommands-and-watch-mode/{spec.md,plan.md,tasks.md}` defining meta-command surface, watch bounds, and lexical safety rules.
- [x] T002: Implement `src/metacommand/mod.rs` pure lexer, quote/comment-aware `\watch` scanner, pattern escaper, catalog SQL generator, `help_result()`, and refusal diagnostics.
- [x] T003: Add `src/metacommand/tests.rs` sidecar unit tests for all commands, schema-qualified patterns, quote injection safety, `\x`/`\timing`, `\watch` intervals, companion SQL, and `\!` refusal.
- [x] T004: Register `pub mod metacommand;` in `src/lib.rs` and wire dispatch, expanded/timing toggles, and bounded watch state into `src/app/`.
- [x] T005: Wire meta-command rendering and watch status indicators in `src/ui/` and interactive execution.
- [x] T006: Add end-to-end tests against live PostgreSQL covering `\d`, `\dt`, `\l+`, `\conninfo`, and bounded `\watch`.
- [x] T007: Update README, keymap/help docs, and compatibility documentation for supported meta-commands.
- [x] T008: Run `cargo xtask verify` and record verification evidence in `docs/status.md`.
