# Feature Specification: Persistent Scratchpad and Snippet Library (Feature 042)

## 1. Overview
In daily database development and administration, engineers frequently reuse standardized SQL patterns:
- Monitoring queries (e.g. table sizes, cache hit rates, lock trees, active connections).
- Routine administrative statements (e.g. terminating hung queries, analyzing vacuum progress, rebuilding bloated indexes).
- Common application templates (e.g. keyset pagination, upsert with `ON CONFLICT`, recursive CTEs, audit trail triggers).
- Ad-hoc persistent scratchpads: temporary drafting spaces saved across application restarts without cluttering formal saved queries.

Feature `042-persistent-scratchpad-and-snippet-library` introduces a persistent snippet and scratchpad manager to Ignatius:
1. **Persistent Storage**: Snippets and scratchpads are stored as human-readable `.sql` files with metadata headers or structured metadata under `<data_dir>/snippets/` and `<data_dir>/scratchpads/`.
2. **Curated Standard Library**: Ships with built-in, production-tested PostgreSQL snippets categorized by domain (`admin`, `performance`, `patterns`, `schemas`).
3. **Interactive TUI Cockpit (`Ctrl+K ;`)**: Dieter Rams Field Industrial snippet browser with category/tag filtering, instant search, syntax-highlighted SQL preview, and one-key insertion into the active editor buffer or opening as a new editor tab.
4. **Variable Substitution**: Supports placeholder markers (e.g., `{{table_name}}`, `{{column_name}}`) that prompt or populate intelligently during insertion.
5. **Headless Scriptable CLI (`ignatius snippet`)**: Manage and inspect snippets non-interactively.

## 2. User Scenarios & Capabilities

### Scenario 1: Interactive Snippet Browser & Instant Insertion
- An engineer writing a query in the SQL editor wants to calculate cache hit ratios.
- The engineer presses `Ctrl+K ;` (`snippet-library`).
- The Field Industrial Snippet Library overlay opens, showing:
  - Header capsules: `[SNIPPETS: 14]`, `[CATEGORY: PERFORMANCE]`, `[STORAGE: LOCAL]`.
  - Left pane: Fuzzy-filterable list of snippets with category badge and description.
  - Right pane: Syntax-highlighted live SQL preview with parameter markers.
- The engineer presses `Enter`: the snippet is inserted directly into the active editor buffer at the current cursor position, and the overlay dismisses.
- If the engineer presses `Ctrl+O` or `n`, the snippet opens in a brand new editor buffer tab!

### Scenario 2: Save Active Editor as Reusable Snippet / Scratchpad
- An engineer drafts a complex reporting query in the editor and wants to save it as a reusable snippet.
- In the Snippet Library overlay, pressing `a` (`Add from Editor`) captures the current buffer text, prompting for snippet name, category, and description.
- The snippet is validated against path traversal rules and immediately saved to disk under `<data_dir>/snippets/<name>.sql`.

### Scenario 3: Persistent Scratchpads Across Restarts
- Scratchpad tabs can be marked as persistent scratchpads.
- Saved under `<data_dir>/scratchpads/<name>.sql`, they automatically persist and reload across Ignatius workbench sessions.

### Scenario 4: Non-Interactive CLI Parity
- Engineers can list, display, and export snippets via CLI:
  ```bash
  ignatius snippet list
  ignatius snippet show cache-hit-ratio
  ignatius snippet add my-audit --file audit.sql --category admin --description "Audit trail generator"
  ```

## 3. Visual & Aesthetic Tenets (Dieter Rams / Field Industrial)
- **Status Capsules**:
  - `[SNIPPETS: 14/100]`
  - `[CATEGORY: ALL | ADMIN | PERFORMANCE | PATTERNS]`
  - `[SCOPE: BUILT-IN | USER]`
- **Phosphor Amber Accent**: Selected snippet in list highlighted with `Token::Focus` bold styling.
- **Preview Pane**: 60% right pane displaying formatted SQL, description, tags, and insertion instructions.

## 4. Keymap & Action Bindings
- `Ctrl+K ;`: `Action::SnippetLibrary` (`snippet-library`) - Open snippet and scratchpad library.
- Inside Overlay:
  - `Up` / `Down` / `k` / `j`: Navigate snippet list.
  - `Enter`: Insert snippet into currently active editor buffer at cursor.
  - `n` or `Ctrl+O`: Open snippet in a new editor tab buffer.
  - `Tab`: Cycle category filter (`ALL` -> `ADMIN` -> `PERFORMANCE` -> `PATTERNS` -> `USER`).
  - `a`: Add current editor buffer as a new user snippet.
  - `d`: Delete selected user snippet (with confirmation).
  - `Esc`: Close overlay without side effects.
