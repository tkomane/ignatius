# Feature Specification: Saved Query Collections, Tag Metadata, Offline CLI, and Roadmap Alignment

**Feature Branch**: `feat/m22-query-collections` (PR 3A) and `feat/m22-docs-readme` (PR 3B)
**Created**: 2026-11-27
**Status**: In Progress (PR 3A active; PR 3B pending)
**Milestone**: M22 - Saved Query Collections and Roadmap Alignment

## Problem Statement

Two local workflow and documentation gaps remain as saved SQL libraries and shipped capabilities expand:

1. **Saved queries live only as a flat directory without organization, metadata, or offline CLI inspection.** Operators with dozens of saved `.sql` files cannot group queries into single-level collections (`collection/query-name`), attach plain SQL header tags/descriptions (`-- @tag:`, `-- @description:`), preview full bounded SQL with collection and tag metadata in the TUI palette, or list and inspect saved queries from the offline CLI (`ignatius queries list` / `show`).
2. **Roadmap, status, and README documentation need synchronization across M20-M22.** Once PR 3A lands, `docs/status.md`, `docs/product/roadmap.md`, and `README.md` must accurately reflect the verified M20-M22 builds without unverified human or cloud claims.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Unreadable, symlinked, non-regular, or oversized query files are never silently trusted or passed through; diagnostics report the exact reason and safe recovery action.
- **Principle III (Local-first and private by default).** Saved queries remain plain `.sql` files in the user's `queries_dir` with optional SQL `-- @tag:` / `-- @description:` comments readable by `psql` or any text editor, and `ignatius queries` runs strictly offline with zero database or network calls.
- **Principle V (Keyboard-first, never keyboard-only).** Saved Queries palette filtering matches query names, collection names, `#tag` labels, descriptions, and SQL previews via keyboard without extra steps.
- **Principle VI (Accessible meaning survives styling loss).** Collection origins, `#tag` tags, descriptions, and SQL previews render with explicit text labels and `Theme`/`Token` styles that survive `Basic16`, `NO_COLOR`, and ASCII modes.
- **Principle VII (Cross-platform behaviour is a release criterion).** Path validation rejects `\` separators, `.`, `..`, `:`, control characters, and Windows-reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`) across both collection and query-stem segments on every OS.

## User Scenarios & Testing

### User Story 1 - Single-Level Saved Query Collections and Header Metadata (Priority: P1, Slice 1 / PR 3A)

An operator can organize saved `.sql` queries either directly in the queries root (`monthly-revenue`) or inside a single-level collection directory (`billing/monthly-revenue`). Optional plain SQL comments at the top of a file (`-- @tag: finance, monthly` or `-- @tags: ...`, and `-- @description: ...`) populate structured metadata while keeping the file 100% valid PostgreSQL SQL. Path traversal, multi-level nesting, symlinks (on either the collection directory or the `.sql` leaf), non-regular files, Windows-reserved device names, and files exceeding 1 MiB are rejected or safely bounded.

### User Story 2 - TUI Saved Queries Palette Search and Rich Preview (Priority: P1, Slice 1 / PR 3A)

When opening Saved Queries (`Ctrl+K o` / `OpenQuery`), the palette displays flat and collection-qualified entries, lets the user filter accurately by query name, collection name, `#tag`, description, or SQL text, and displays collection origin, `#tag` tags, description, and a bounded multi-line syntax-highlighted SQL preview in the right-hand preview pane.

### User Story 3 - Offline `ignatius queries` CLI (Priority: P1, Slice 1 / PR 3A)

An operator or script can inspect saved queries completely offline via `ignatius queries list [--collection <NAME>] [--tag <TAG>] [--format text|json]` and `ignatius queries show <NAME>`, respecting `--config-dir` (`paths.queries_dir`), keeping clean SQL or JSON on stdout, and emitting `DiagnosticKind::Usage` errors to stderr for invalid, symlinked, oversized, or missing names.

### User Story 4 - Roadmap, Status, and README Alignment (Priority: P2, Slice 2 / PR 3B)

Project documentation (`docs/status.md`, `docs/product/roadmap.md`, `README.md`, and Feature 068 tasks) is synchronized with the verified M20-M22 deliverables using exact observed verification receipts and direct "PostgreSQL workbench for your terminal" framing, while keeping owner-gated manual tasks open.

## Functional Requirements

### Slice 1 - Saved Query Collections, Tag Metadata, Palette Search/Preview, and Offline CLI (PR 3A)

- **FR-001**: Extend `SavedQuery` (`src/queries.rs`) to `{ name, collection: Option<String>, tags: Vec<String>, description: Option<String>, path, summary, preview }` while preserving 100% backwards compatibility for flat `.sql` files without header metadata.
- **FR-002**: Enforce strict single-level path security in `Library::path_for`, `save`, `load`, and `list`: allow at most one `/` with non-empty segments before and after `/`; validate both collection and query-stem segments against `\` separators, `.`, `..`, `:`, control characters, and `RESERVED_ON_WINDOWS`; verify via `symlink_metadata` that any collection directory is a real directory (never a symlink) and any `.sql` leaf is a real regular file (never a symlink or directory); and bound reads at 1 MiB (`MAX_QUERY_FILE_BYTES`).
- **FR-003**: Parse leading SQL `-- @tag:` / `-- @tags:` and `-- @description:` header comments into `SavedQuery.tags` and `SavedQuery.description`, extract a bounded multi-line SQL `preview`, scan root and 1-level real subdirectories in `Library::list()` sorted deterministically by `name`, and expose `Library::list_filtered(collection: Option<&str>, tag: Option<&str>)`.
- **FR-004**: Update `Message::QueriesListed` (`src/app/update.rs`), `Palette` (`src/app/palette.rs`), and `src/ui/widgets/palette.rs` (preserving existing `Palette` / `PaletteEntry` public API and tests) so Saved Queries entries display collection origin, `#tag` tags, description, and bounded multi-line SQL preview in the right-hand preview pane and filter accurately on collection names and `#tag` queries.
- **FR-005**: Add `Command::Queries { action: QueriesAction }` in `src/cli/mod.rs` providing offline `ignatius queries list [--collection <NAME>] [--tag <TAG>] [--format text|json]` and `ignatius queries show <NAME>` respecting `--config-dir` (`paths.queries_dir`) with zero database or network access.

### Slice 2 - Documentation, Roadmap, and README Synchronization (PR 3B)

- **FR-006**: Update `docs/status.md`, `docs/product/roadmap.md`, and `README.md` with accurate M20-M22 verification summaries, keeping owner-gated tasks (`001` T051/T052, `007` T032/T033, `008` T033/T035) open and zero fabricated claims.

## Non-Functional Requirements

- **NFR-001 (UI Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Repository Hygiene)**: Sidecar `tests.rs` files only, `AGENTS.md == CLAUDE.md`, zero Unicode em/en dashes (`\u{2014}` / `\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify` and `cargo --locked xtask security`.
