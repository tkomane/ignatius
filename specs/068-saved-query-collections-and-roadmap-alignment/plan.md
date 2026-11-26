# Implementation Plan: Saved Query Collections, Tag Metadata, Offline CLI, and Roadmap Alignment

**Spec**: [spec.md](spec.md)

## Delivery Slices

Feature 068 (Milestone M22) is delivered across two pull requests:

### PR 3A - Saved Query Collections, Tag Metadata, TUI Search/Preview, and Offline CLI (`feat/m22-query-collections`)

1. **`src/queries.rs` & `src/queries/tests.rs`**:
   - Extend `SavedQuery` with `collection: Option<String>`, `tags: Vec<String>`, `description: Option<String>`, and `preview: String` (bounded multi-line SQL preview).
   - Enforce single-level collection validation (`collection/query-name`, at most one `/`, non-empty segments) in `resolve_target` / `path_for`, checking both collection and query stem against `\`, `.`, `..`, `:`, control characters, and `RESERVED_ON_WINDOWS`.
   - Enforce `symlink_metadata` non-symlink real directory checks on collection folders and non-symlink regular file checks on `.sql` leaves, plus a 1 MiB (`MAX_QUERY_FILE_BYTES = 1_048_576`) bounded read cap across `load` and `list`.
   - Parse optional SQL header comments (`-- @tag:`, `-- @tags:`, `-- @description:`) at the top of `.sql` files while keeping `.sql` 100% valid PostgreSQL SQL.
   - Scan root `.sql` files and 1-level real non-symlink subdirectories in `Library::list()`, sorted by `name`, and add `Library::list_filtered(collection: Option<&str>, tag: Option<&str>)`.
2. **`src/app/palette.rs`, `src/app/update.rs`, `src/ui/widgets/palette.rs`, `src/app/update/tests.rs`, `src/app/palette/tests.rs`, `src/ui/widgets/palette/tests.rs`**:
   - Preserve existing `Palette` and `PaletteEntry` public constructors/fields and existing tests, adding `Palette::from_saved_queries` for `Message::QueriesListed`.
   - Rank matches across query name, collection name, `#tag` tags, description, and SQL preview; render collection origin, `#tag` tags, description, and multi-line syntax-highlighted SQL preview in the right-hand preview pane using `Theme`/`Token` only.
3. **`src/cli/mod.rs`, `src/cli/tests.rs`, `tests/cli_contract.rs`, `tests/resource_bounds_contract.rs`**:
   - Add `Command::Queries { action: QueriesAction }` for offline `ignatius queries list [--collection <NAME>] [--tag <TAG>] [--format text|json]` and `ignatius queries show <NAME>` respecting `--config-dir` (`paths.queries_dir`).
   - Update resource-bounds contract and add comprehensive unit + subprocess CLI contract tests.

### PR 3B - Documentation, Roadmap, and README Alignment (`feat/m22-docs-readme`)

1. **`docs/status.md`, `docs/product/roadmap.md`, `README.md`**:
   - Synchronize M20-M22 feature records and CLI/workbench documentation with verified test evidence and direct "PostgreSQL workbench for your terminal" framing while keeping owner-gated manual tasks open.

## Verification Gates

- `cargo --locked xtask verify` (formatting, clippy, docs, unit tests, UI/CLI/doc/resource-bounds contracts, PostgreSQL integration tests).
- `cargo --locked xtask security` (sidecar test hygiene, `AGENTS.md == CLAUDE.md`, zero governed em/en dashes, policy checks).
