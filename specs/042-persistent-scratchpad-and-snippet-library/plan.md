# Implementation Plan: Persistent Scratchpad and Snippet Library (Feature 042)

## 1. Pure Domain Engine (`src/snippet/`)
- `src/snippet/mod.rs`:
  - `SnippetCategory`: `All`, `Admin`, `Performance`, `Patterns`, `User`.
  - `Snippet`:
    - `id: String`, `name: String`, `title: String`, `description: String`
    - `category: SnippetCategory`
    - `sql: String`
    - `is_builtin: bool`
    - `path: Option<PathBuf>`
  - `SnippetLibrary`:
    - `snippets: Vec<Snippet>`
    - `selected_idx: usize`
    - `category_filter: SnippetCategory`
    - Built-in curated catalog:
      - `cache-hit-ratio`: Buffer cache hit ratio across tables and indexes.
      - `table-sizes`: Top relation disk usage (table, indexes, total).
      - `active-locks`: Blocking locks and waiting transaction tree.
      - `index-bloat`: Estimated dead space and bloat percentage.
      - `kill-idle-tx`: Safe terminate statements for idle in transaction backends.
      - `keyset-pagination`: Deterministic cursor-based pagination query pattern.
      - `upsert-conflict`: `INSERT ... ON CONFLICT (id) DO UPDATE` pattern.
      - `audit-trigger`: Standard row-level temporal audit trail function and trigger.
    - File system discovery and saving:
      - Loading `.sql` files from `<data_dir>/snippets/`.
      - Validating names against security boundaries (no separators, no `..`, alphanumeric + hyphen/underscore).
      - Atomic write-file persistence.
- `src/snippet/tests.rs`:
  - Pure unit tests for built-in catalog loading, search, category filtering, user snippet addition, validation, and file persistence.

## 2. TEA Architecture Integration
- `src/app/model.rs`:
  - `SnippetLibraryState`: `library: SnippetLibrary`, `filter_text: String`, `prompt_mode: Option<SnippetPromptMode>`.
  - Add `pub snippet_library: Option<SnippetLibraryState>` to `Model`.
- `src/app/message.rs`:
  - Add `Action::SnippetLibrary` to `Action` and `Action::all()`.
- `src/ui/keymap.rs`:
  - Add `("snippet-library", Action::SnippetLibrary)` to `CONFIGURABLE`.
  - Add `(';', Action::SnippetLibrary, "Open snippet and scratchpad library")` to `CHORDS`.
  - Update `short_label`.
- `src/app/discovery.rs`:
  - Update `action_is_available`, `action_prerequisite`, and `palette_group` ("Files/History").
- `src/app/update.rs`:
  - Handle `Action::SnippetLibrary`.
  - Add `snippet_library_action`: navigation, category cycling, insertion into `model.editor` (and syncing to active buffer), opening in new buffer, adding user snippet, and dismissing.

## 3. Dieter Rams Field Industrial TUI Cockpit
- `src/ui/widgets/overlays.rs`:
  - Render `SnippetLibrary` overlay:
    - Top hardware capsules: `[SNIPPETS: n/total]`, `[CATEGORY: name]`, `[SCOPE: scope]`.
    - Split-pane layout: 40% snippet list with category pills on left; 60% syntax-highlighted SQL preview with description and usage notes on right.
    - Hotkeys bar: `[Enter]` insert, `[n]` new tab, `[Tab]` cycle category, `[a]` add from editor, `Esc` close.

## 4. Headless Scriptable CLI Subcommand
- `src/cli/mod.rs`:
  - Add `Snippet(SnippetArgs)` to `Command`.
  - Subcommands: `list`, `show <NAME>`, `add <NAME> --file <PATH>`.
  - Formats: `--format table|json|sql`.

## 5. Acceptance Contracts & Verification
- `tests/snippet_contract.rs`:
  - Test CLI subcommands (`ignatius snippet list`, `ignatius snippet show cache-hit-ratio`).
  - Test insertion into editor and opening in new tab.
- Update `README.md`, `docs/design/keymap.md`, `docs/status.md`, `CHANGELOG.md`, and interactive guide.
- Run `cargo xtask verify`.
