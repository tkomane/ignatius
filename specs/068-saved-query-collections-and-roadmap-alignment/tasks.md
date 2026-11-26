# Tasks: Saved Query Collections, Tag Metadata, Offline CLI, and Roadmap Alignment

**Input**: Design documents from `/specs/068-saved-query-collections-and-roadmap-alignment/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 3A - Saved Query Collections, Tag Metadata, TUI Search/Preview, and Offline CLI (`feat/m22-query-collections`)

- [x] T001 Extend `SavedQuery` (`collection`, `tags`, `description`, `preview`) and enforce strict single-level collection path/symlink/Windows-reserved/1 MiB security boundaries in `src/queries.rs`.
- [x] T002 Parse `-- @tag:` / `-- @tags:` / `-- @description:` SQL header comments, scan root + 1-level real collection subdirectories in `Library::list()`, and implement `Library::list_filtered(collection, tag)` in `src/queries.rs` with sidecar tests in `src/queries/tests.rs`.
- [x] T003 Update `Palette`, `Message::QueriesListed`, and `src/ui/widgets/palette.rs` (`src/app/palette.rs`, `src/app/update.rs`, `src/ui/widgets/palette.rs`) to filter by collection/`#tag`/description and render collection origin, `#tag` tags, description, and bounded multi-line SQL preview using `Theme`/`Token` only, preserving existing `Palette` API and tests.
- [x] T004 Add offline `ignatius queries list [--collection <NAME>] [--tag <TAG>] [--format text|json]` and `ignatius queries show <NAME>` in `src/cli/mod.rs` respecting `--config-dir` (`paths.queries_dir`) with zero database or network calls.
- [x] T005 Add unit, reducer, widget, resource-bounds, and subprocess CLI contract tests across `src/queries/tests.rs`, `src/app/palette/tests.rs`, `src/app/update/tests.rs`, `src/ui/widgets/palette/tests.rs`, `src/cli/tests.rs`, `tests/cli_contract.rs`, and `tests/resource_bounds_contract.rs`, then verify with `cargo --locked xtask verify`.

## Phase 2: PR 3B - Documentation, Roadmap, and README Alignment (`feat/m22-docs-readme`)

- [ ] T006 Sync `docs/status.md`, `docs/product/roadmap.md`, and `README.md` with verified M20-M22 deliverables and direct "PostgreSQL workbench for your terminal" framing without closing owner-gated tasks (`001` T051/T052, `007` T032/T033, `008` T033/T035).
- [ ] T007 Verify documentation contracts and hygiene gates (`cargo --locked xtask verify` and `cargo --locked xtask security`).
