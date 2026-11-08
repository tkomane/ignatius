# Ignatius

[![CI](https://github.com/tkomane/ignatius/actions/workflows/ci.yml/badge.svg)](https://github.com/tkomane/ignatius/actions/workflows/ci.yml)

```
 ___   ___  _  _    _    _____  ___  _   _  ___
|_ _| / __|| \| |  /_\  |_   _||_ _|| | | |/ __|
 | | | (_ || .` | / _ \   | |   | | | |_| |\__ \
|___| \___||_|\_|/_/ \_\  |_|  |___| \___/ |___/
------------------------------------------------
the PostgreSQL client you can point at production
```

A keyboard-first, mouse-fluent terminal workbench and scriptable CLI for PostgreSQL built in Rust (`ratatui` + `tokio-postgres`). Inspired by modern developer terminal tools like OpenCode, Lazygit, Harlequin, Neo-tree, Telescope.nvim, Blink.cmp, and Trouble.nvim, Ignatius delivers a calm, truthful, GUI-grade experience right inside your terminal. No telemetry, no user accounts, and no cloud intermediaries in the loop.

![Ignatius Workbench Overview](docs/assets/screenshots/workbench-overview.png)

## Why Ignatius?

PostgreSQL clients have traditionally forced an uncomfortable choice: use `psql` for safety and complete compatibility while enduring a wall of scrolling text, or reach for a multi-database GUI client that treats PostgreSQL as just another dialect and obscures whether your TLS connection actually negotiated the encryption you asked for.

Ignatius ends that compromise. Everything a developer expects from a modern visual workbench is brought into a single, compact, dependency-light terminal binary that feels natural under pressure.

| Pillar | How Ignatius Delivers It |
| --- | --- |
| **4-Tier Surface Architecture & Adaptive Themes** | Deep TokyoNight-inspired `dark`, Catppuccin Latte-inspired `light`, `high-contrast`, user `[theme.tokens]` `custom` overrides, and automatic `OSC 11` terminal background luminance detection (`ui.theme = "auto"`), degrading cleanly through `TrueColor`, `Indexed256`, and `Basic16`. |
| **Truthful Results Grid** | `NULL` (`[null]`), empty string (`""`), and literal `"NULL"` (`"NULL"`) are always visually and copy-distinct. JSON/JSONB values, arrays, numeric right-alignment, sort indicators, row numbers, and horizontal/vertical scrolling work out of the box. |
| **Safety by Construction** | Production profiles (`environment = "prod"` or `read_only = true`) visibly change the header pill (`[PROD]` / `READ-ONLY`), guard destructive statements (`DROP`, `TRUNCATE`, `DELETE`/`UPDATE` without `WHERE`) behind a confirmation modal, and require an explicit reviewed `UPDATE` modal (`Ctrl+K u` on a cell) with primary-key targeting and `Enter` to execute. |
| **Schema-Aware Editor & Overlays** | Multi-line SQL editor with active `CursorLine` & `CursorLineNr`, 1 MiB bounded bracketed paste, schema-aware `Ctrl+Space` completion popup, 80%x75% Command Palette (`Ctrl+P`) with 40%/60% list-and-SQL preview split, interactive `EXPLAIN` / `EXPLAIN ANALYZE` Plan Explorer (`Ctrl+K l` / `Ctrl+K a`), and layered PostgreSQL diagnostics (`SQLSTATE`, statement line/column, token pinning, likely cause, and next step). |
| **Non-Interactive CLI & Streaming Export** | Scriptable `ignatius query` with `--format table|csv|tsv|json|jsonl|sql`, `--file`, `--var key=value`, `--tx single|none`, and constant-memory streaming export via `--output <path>`. |

---

## Visual Tour

### Three-Pane Workbench & Neo-tree Schema Sidebar

![Ignatius Three-Pane Workbench](docs/assets/screenshots/workbench-overview.png)

The primary interface splits into three intuitive, resizable surfaces:
- **Neo-tree Style Schema Sidebar (Left)**: Hierarchical database object browser showing schemas, tables, views, materialized views, foreign tables, partitions, and columns with data types. Double-click or press `Enter` to expand nodes, inspect definitions (`Ctrl+K d`), or view dependency trees (`Ctrl+K y`).
- **SQL Editor (Top-Right)**: Syntax-highlighted multi-line editor with active `CursorLine` and `CursorLineNr` gutters. Supports word-level undo/redo (`Ctrl+Z` / `Ctrl+Y`), bracketed paste bounded at 1 MiB, and deterministic formatting (`Ctrl+Shift+F` or `Ctrl+K q`).
- **Results Grid (Bottom-Right)**: Elevated Harlequin-style data table with row numbers, sort indicators, grid view controls (`Ctrl+K g`), local filtering (`/`), expanded row view (`Ctrl+K x`), and full cell inspection (`Enter` or `Ctrl+K i`).
- **Split Resizing & Zoom**: Drag split borders with the mouse or use chords (`Ctrl+K ,` / `Ctrl+K .` for pane splits, `Ctrl+K [` / `Ctrl+K ]` for result columns) to adjust layout ratios. Press `Ctrl+K z` to toggle full-screen zoom on any focused pane.

### Schema-Aware SQL Completion (Blink.cmp Style)

![Schema-Aware SQL Completion](docs/assets/screenshots/completion-menu.png)

Ignatius features an asynchronous, catalog-backed autocomplete popup modeled after Blink.cmp:
- **Intelligent Context Detection**: Automatically triggers on typing or via `Ctrl+Space`. Intelligently suggests SQL keywords, schemas, tables, views, columns, CTEs, and table aliases without issuing blocking database queries.
- **Fuzzy Subsequence Matching**: Highlights matched characters in real-time, displays explicit object type badges (`table`, `view`, `column`, `keyword`), and inserts canonical quoted identifiers when needed.

### 60% SQL Preview Command Palette (Telescope Style)

![Command Palette with 60% SQL Preview](docs/assets/screenshots/palette-preview.png)

Pressing `Ctrl+P` opens a Telescope-inspired command palette occupying 80% width and 75% height of the screen:
- **40%/60% Split**: The left pane presents fuzzy-searchable commands, saved queries, connection profiles, and catalog objects with a solid selection bar.
- **Syntax-Highlighted Live Preview**: The 60% right pane renders immediate, syntax-highlighted SQL for saved queries, table definitions, foreign key relationships, and connection trust boundaries before any action is executed.

### Interactive EXPLAIN / EXPLAIN ANALYZE Plan Explorer

![Interactive Query Plan Explorer](docs/assets/screenshots/plan-explorer.png)

Understand PostgreSQL execution plans without leaving the terminal:
- **Estimated Plans (`Ctrl+K l`)**: Runs `EXPLAIN (FORMAT JSON)` without executing the statement, visualizing plan nodes in a tree with estimated startup and total costs.
- **Measured Execution (`Ctrl+K a`)**: Runs `EXPLAIN ANALYZE` in a transaction, measuring actual timings, loops, buffer hits, and row counts.
- **Cost Attribution**: Automatically identifies and highlights the most expensive node in the query plan, surfacing scan types, filter predicates, and index conditions clearly.

### Layered Error Diagnostics & Token Pinning (Trouble.nvim Style)

![Layered PostgreSQL Error Diagnostics](docs/assets/screenshots/error-diagnostics.png)

When PostgreSQL returns an error, Ignatius replaces cryptic backtraces with a Trouble.nvim-style structured breakdown:
- **Multi-Span Context**: Clearly separates the headline from the operational context (`While: executing buffer`), database context (`Object context: relation "public"."users"`), and catalog analysis.
- **Precise Location & Underline**: Maps PostgreSQL server error offsets back to the exact statement number, editor line, and character column, placing an explicit caret indicator directly beneath the erroneous token.
- **Actionable Guidance**: Suggests a probable reason (`Likely cause:`) and safe next steps (`Next:`). Pressing `Ctrl+D` expands the technical section revealing raw `SQLSTATE` codes and server fields.

### Deliberate, Reviewed Cell-to-UPDATE & Write Guard

![Reviewed Cell-to-UPDATE Modal](docs/assets/screenshots/reviewed-update.png)

Ignatius makes quick data edits safe, transparent, and auditable:
- **Targeted Cell Editing**: Selecting a cell in a single-table result with a primary key and pressing `Ctrl+K u` generates a safe, parameterized `UPDATE` statement.
- **Explicit Modal Review**: The modal displays the proposed SQL, new value, and primary key `WHERE` predicates in full. Pressing `Enter` executes the single update, while `Esc` cancels without side-effects.
- **Production Protection**: The write guard blocks ad-hoc cell updates in production profiles (`environment = "prod"`) and read-only sessions (`--read-only`), preventing accidental mutations.

### Adaptive Light, Dark & High-Contrast Themes

![Ignatius Light Theme](docs/assets/screenshots/workbench-light.png)

Built on a 4-tier painted surface system (`Surface`, `SurfaceAlt`, `SurfacePane`, `SurfaceOverlay`):
- **Built-in Palettes**: TokyoNight-inspired `dark`, Catppuccin Latte-inspired `light`, and WCAG AAA `high-contrast`.
- **Runtime Theme Cycling**: Open the Command Palette (`Ctrl+P` -> "Switch theme") or configure `[keys] switch-theme = "..."` to instantly cycle themes without reconnecting or re-running SQL.
- **Terminal Integration**: Supports automatic background luminance detection via `OSC 11` queries (`ui.theme = "auto"`), and user-defined color overrides in `[theme.tokens]`.

---

## Quickstart: Try Ignatius in 60 Seconds

Ignatius includes a self-contained local PostgreSQL container playground pre-seeded with realistic ecommerce, analytics, and type-coverage fixtures.

### 1. Start the Playground
```bash
cargo xtask db up
```
This spins up disposable local PostgreSQL containers:
- Plain connection: `postgres://ignatius_test:ignatius_test_pw@127.0.0.1:55432/ignatius_demo?sslmode=disable`
- TLS connection: `postgres://ignatius_test:ignatius_test_pw@localhost:55433/ignatius_demo?sslmode=verify-full`
Pre-seeded schemas include `orders`, `recent_orders`, `rendering_cases`, `type_coverage`, and `reporting.order_totals`.

### 2. Launch the Workbench
Run directly from source or install the binary:
```bash
# Option A: Run directly via xtask
cargo xtask run

# Option B: Install to ~/.local/bin/ignatius and launch
cargo xtask install
ignatius "postgres://ignatius_test:ignatius_test_pw@127.0.0.1:55432/ignatius_demo?sslmode=disable"
```

### 3. Immediate Actions to Try
- Press `Ctrl+R` to execute the pre-loaded query buffer.
- Press `Tab` to navigate focus from Editor to the Results grid.
- Navigate cells with the arrow keys (`Up`, `Down`, `Left`, `Right`). Press `Enter` (or `Ctrl+K i`) to inspect any value in the Cell Detail viewer.
- On a single-table result row with a primary key, press `Ctrl+K u` to preview a reviewed `UPDATE` statement.
- Press `Ctrl+P` to open the Command Palette with live 60% SQL preview.
- In the Command Palette (`Ctrl+P`), select **Switch theme** to cycle between dark, light, and high-contrast themes.
- Press `Ctrl+K l` to inspect the visual EXPLAIN query plan.

### 4. Tear Down
```bash
cargo xtask db down
```
Stops the containers and discards all synthetic playground data.

---

## Connecting to PostgreSQL

### URIs and Command-Line Flags
Connect directly with standard PostgreSQL connection strings or individual flags:
```bash
# Standard connection URI
ignatius "postgres://app_user:secret@db.internal:5432/analytics?sslmode=verify-full"

# Individual connection flags
ignatius --host db.internal --port 5432 --user app_user --dbname analytics --sslmode verify-full

# Open in read-only mode (server-enforced)
ignatius --read-only "postgres://app_user@db.internal:5432/analytics"

# Connect using a named profile
ignatius connect @production-orders
```

### Configuration Profiles (`config.toml`)
Connection profiles and client preferences live in `~/.config/ignatius/config.toml` (Linux/macOS) or `%APPDATA%\ignatius\config.toml` (Windows):

```toml
[ui]
theme = "auto"              # "dark", "light", "high-contrast", or "auto" (OSC 11)
mouse = true                # enable mouse clicks, drags, and scrolling
density = "comfortable"     # "comfortable" or "compact"
reduced_motion = false      # disable animations for accessibility
glyphs = "nerd-font"        # "unicode", "ascii", or "nerd-font"

[profiles.local-dev]
host = "127.0.0.1"
port = 55432
dbname = "ignatius_demo"
user = "ignatius_test"
sslmode = "disable"
environment = "dev"

[profiles.staging-analytics]
host = "staging-db.internal"
port = 5432
dbname = "analytics"
user = "analyst"
sslmode = "require"
environment = "staging"

[profiles.production-orders]
host = "orders-primary.internal"
port = 5432
dbname = "orders"
user = "readonly_admin"
sslmode = "verify-full"
environment = "prod"        # triggers [PROD] header badge and write guards
read_only = true            # enforces PostgreSQL session read-only transaction mode
```

### Cloud Identity Authentication (No Stored Passwords)
Authenticate to cloud-managed PostgreSQL services using short-lived tokens from official cloud CLIs:

```toml
[profiles.azure-prod]
host = "orders.postgres.database.azure.com"
dbname = "orders"
user = "engineer@company.com"
sslmode = "verify-full"
environment = "prod"
auth = "entra"              # fetches token via `az account get-access-token`

[profiles.aws-prod]
host = "analytics.cluster-xyz.us-east-1.rds.amazonaws.com"
dbname = "analytics"
user = "db_iam_user"
sslmode = "verify-full"
environment = "prod"
auth = "aws"                # fetches token via AWS RDS IAM authentication

[profiles.gcp-prod]
host = "10.20.30.40"
dbname = "warehouse"
user = "sa-query@project.iam"
sslmode = "verify-full"
environment = "prod"
auth = "gcp"                # fetches token via `gcloud auth print-access-token`
```

### Credential Safety Invariants
- **Zero Plaintext Secrets**: Passwords and bearer tokens are wrapped in `secrecy::SecretString` and zeroized on drop.
- **Single Redaction Pipeline**: All log messages, diagnostic reports, and CLI output pass through an audited redaction module.
- **Standard Secrets Resolution**: Credentials resolve securely from connection strings, `.pgpass`, environment variables (`PGPASSWORD`), or an interactive masked prompt. Passwords are never written to disk or recorded in statement history.

---

## Keyboard Shortcuts, Chords & Mouse Fluency

Ignatius is designed for fluid, full-keyboard operation with zero Vim-modal traps, backed by complete mouse gestures.

### Global Navigation
| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Cycle focus between Sidebar, Editor, and Results panes |
| `Ctrl+P` | Open Command Palette with 60% live preview |
| `Ctrl+B` | Toggle Neo-tree Schema Sidebar visibility |
| `Ctrl+C` | Cancel currently running statement (honest cancellation) |
| `Ctrl+Q` | Quit Ignatius |
| `F1` / `Ctrl+G` | Open or dismiss comprehensive interactive Help |
| `Esc` | Dismiss current modal, overlay, search, or error |

### SQL Editor Keys
| Key | Action |
| --- | --- |
| `Ctrl+R` / `F5` | Execute full SQL buffer |
| `Ctrl+T` / `F9` | Execute SQL statement under cursor |
| `Ctrl+Space` | Trigger catalog-backed SQL autocomplete popup |
| `Ctrl+Shift+F` | Format SQL buffer deterministically |
| `Ctrl+Z` / `Ctrl+Y` | Word-level Undo / Redo |
| `Ctrl+Left` / `Ctrl+Right` | Move cursor word by word |
| `Home` / `End` | Move cursor to start / end of line |

### Results Grid & Sidebar Keys
| Key | Action |
| --- | --- |
| `Up` / `Down` / `Left` / `Right` | Navigate cells in Results grid or nodes in Sidebar |
| `PageUp` / `PageDown` / `Home` / `End` | Page or jump to start/end of the focused pane |
| `Enter` / `Ctrl+K i` | Open full Cell Detail inspector (or expand/insert selected sidebar object) |
| `Ctrl+K c` | Copy selected cell value via reviewed OSC 52 clipboard prompt |
| `Ctrl+K u` | Generate a reviewed `UPDATE` statement from selected cell |
| `Ctrl+K g` | Open result grid controls (sort, freeze first column, column visibility, type header) |
| `Ctrl+K x` | Toggle expanded vertical record view for the selected row |
| `/` or `Ctrl+K f` | Filter results grid rows when Results has focus, or filter object tree |
| `F6` / `Ctrl+Shift+R` | Explicitly refresh retained result set by re-running its SQL |

### Two-Key Chord Menu (`Ctrl+K`)
Pressing `Ctrl+K` displays a persistent on-screen chord guide. Press the second key to trigger the action:

| After `Ctrl+K` | Description |
| --- | --- |
| `,` | Shrink the sidebar or editor share |
| `.` | Grow the sidebar or editor share |
| `z` | Zoom the focused pane or restore the layout |
| `b` | Show or hide the object tree |
| `p` | Open the command palette |
| `c` | Copy the selected result value |
| `u` | Generate a reviewed UPDATE from the selected cell |
| `q` | Format the SQL buffer |
| `n` | Choose a connection |
| `g` | Open result grid controls |
| `[` | Narrow the selected result column by 2 cells |
| `]` | Widen the selected result column by 2 cells |
| `l` | Show the estimated query plan |
| `a` | Measure the query plan (executes the statement) |
| `f` | Filter the object tree |
| `r` | Reload the object tree |
| `h` | Show or hide help |
| `x` | Expand the selected row down the screen |
| `i` | Inspect the selected cell |
| `s` | Search the statements that have run |
| `v` | Pause or resume recording statements |
| `d` | Show the selected object's definition |
| `y` | Show what depends on the selected object |
| `w` | Save the buffer as a named query |
| `o` | Open a saved query |
| `e` | Write the rows on screen to a file |

### Mouse Fluency
Mouse events translate directly into the same actions as keyboard commands:
| Target Region | Gesture | Action |
| --- | --- | --- |
| Any pane interior | Left-click | Focus pane |
| Sidebar row | Left-click | Focus tree and select row |
| Sidebar row | Double-click | Toggle expand/collapse or open definition |
| Results cell | Left-click | Select cell |
| Results cell | Double-click | Open Cell Detail inspector |
| Results header cell | Left-click | Cycle column sort (Asc -> Desc -> Server) |
| Results column rule | Left-drag | Resize column width interactively |
| Vertical split bar | Left-drag | Resize sidebar width |
| Horizontal split bar | Left-drag | Resize editor / results split height |
| Overlay / Palette row | Double-click | Accept item |
| Any pane | Scroll wheel | Scroll content vertically by 3 rows |
| Results grid | Shift + Scroll | Scroll columns horizontally |

---

## Non-Interactive CLI, Scripting & Streaming Export

Ignatius functions as a Unix-friendly command-line query engine and data exporter.

### Query Execution & Output Formatting
```bash
# Formats: table, csv, tsv, json, jsonl (ndjson), and sql (INSERT statements)
ignatius query "$DATABASE_URL" -c "SELECT id, name, email FROM users" --format json

# Execute a query file with parameterized variable substitution
ignatius query "$DATABASE_URL" -f monthly_report.sql --var month=2026-11 --var min_orders=5 --format table

# Generate SQL INSERT statements targeting a specific table name
ignatius query "$DATABASE_URL" -c "SELECT * FROM legacy_users" --format sql --table target_users
```

### Constant-Memory Streaming Export
For large result sets, `--output` streams rows directly to disk without loading the entire result set into process memory:
```bash
ignatius query "$DATABASE_URL" -c "SELECT * FROM large_audit_log" --format csv --output audit_export.csv
```
If an export is cancelled or interrupted, Ignatius writes partial progress to a `.partial` file, cleans up the destination path, and exits with code `9` (`export-interrupted`), ensuring partial files are never mistaken for completed exports.

### CLI Management Commands
```bash
# Diagnostics and environment validation
ignatius doctor

# List, inspect, and test named connection profiles
ignatius profile list
ignatius profile show production-orders
ignatius profile test staging-analytics

# Manage local interactive statement history
ignatius history list
ignatius history clear --yes

# Manage and run saved queries
ignatius saved list
ignatius saved run active_user_counts "$DATABASE_URL"

# Shell completion generator
ignatius completions zsh > ~/.zfunc/_ignatius
```

### Process Exit Codes
Scripts can reliably inspect exit codes to determine exact failure modes:
| Code | Identifier | Meaning |
| --- | --- | --- |
| `0` | `success` | Command completed successfully; all statements succeeded |
| `2` | `usage` | Invalid CLI arguments, missing parameters, or conflicting flags |
| `3` | `config` | Configuration file is invalid, missing, or unparseable |
| `4` | `connection` | Network, socket, DNS, or TCP connection failure |
| `5` | `authentication` | Server reached but credentials/token rejected |
| `6` | `tls` | TLS negotiation failed or policy could not be satisfied |
| `7` | `query` | PostgreSQL returned an error for a statement |
| `8` | `cancelled` | Query was cancelled by the user or server |
| `9` | `export-interrupted` | File export was interrupted; output preserved as `.partial` |
| `70` | `internal` | Unexpected client error or internal defect |
| `130` | `sigint` | Process terminated by standard SIGINT signal |

---

## Accessibility, Plain Mode & Design Principles

### Screen Readers and Plain Mode
```bash
ignatius --plain connect "$DATABASE_URL"
```
In `--plain` mode, Ignatius disables the alternate screen buffer, raw mode, and all ANSI escape sequences. Output flows as linear text compatible with `TERM=dumb`, pipes, and screen readers (such as VoiceOver and NVDA). Every prompt communicates database state, production classification, and transaction status explicitly in plain words.

### Design Principles
- **No Emptiness Left Ambiguous**: A missing value is never rendered as blank space. Ignatius renders `[null]`, `""`, and `"NULL"` as visually distinct tokens across all displays.
- **Truthful Status Reporting**: Cancellation states read `Cancellation requested` until the PostgreSQL server sends confirmation. If a connection drops, Ignatius states `Connection lost - query outcome unknown`.
- **Verified TLS**: The TLS badge displays the cipher suite and TLS version reported by `pg_stat_ssl`, not merely the client's requested flags.

---

## Operations, Diagnostics, and Security

Ignatius is built for predictable operations and operator control. Core maintenance and recovery workflows are immediately accessible:

- **Installation & Builds:** Build and install locally via `cargo xtask install` (installs to `~/.local/bin/ignatius`) or run directly with `cargo xtask run`. Pre-compiled native release archives and reproducible checksums are documented in the [release operations guide](docs/operations/release.md).
- **Diagnostics and Health Checks:** Run `ignatius doctor` to validate your environment, active shell, configuration path, terminal color capabilities, and network/TLS socket reachability.
- **Rollback and Verification:** If a regression is detected, rollback instructions, previous artifact verification, and provenance signatures are maintained under [release operations](docs/operations/release.md).
- **History and Privacy Management:** Interactive statement history is stored strictly on the local machine in your configuration directory. Inspect saved history via `ignatius history list` or permanently purge all recorded queries via `ignatius history clear --yes`.
- **Uninstallation:** Remove the `ignatius` binary from your path (e.g., `rm ~/.local/bin/ignatius`) and remove the configuration directory (`~/.config/ignatius` on Linux/macOS or `%APPDATA%\ignatius` on Windows).

---

## Roadmap: Full pgAdmin Replacement Parity (M6)

Ignatius is expanding to provide 100% capability parity with pgAdmin 4, bringing full database design, administration, and diagnostic tooling into a unified, lightweight terminal workbench and scriptable CLI.

Milestone 6 defines 8 comprehensive vertical slices documented in the [Full pgAdmin Replacement Parity Architecture](docs/architecture/pgadmin-parity-plan.md) and the [Product Roadmap](docs/product/roadmap.md):

1. **`026-schema-er-diagrams` (Interactive Schema & ER Diagrams):** Terminal-native ER diagram canvas (`Ctrl+K m`) with orthogonal connectors, 1-hop / 2-hop neighbourhood filtering for dense schemas, and CLI export (`ignatius diagram --format mermaid|dot|svg|ascii`).
2. **`027-deep-catalog-and-ddl-generator` (Deep Catalog & DDL Generator):** Full `pg_catalog` inspection across indexes, constraints, triggers, sequences, enums, composite types, partitions, FDWs, and tablespaces, with deterministic `CREATE`/`ALTER`/`DROP` DDL generation (`ignatius ddl`).
3. **`028-data-import-export-and-backup` (Streaming COPY & Backup/Restore):** High-throughput streaming `COPY` import/export (CSV, TSV, JSONL, SQL, Parquet), interactive TUI column-mapping import wizard with transactional rollback, and subprocess-isolated `pg_dump`/`pg_restore` orchestration.
4. **`029-server-activity-and-locks-dashboard` (Server Activity & Locks Dashboard):** Real-time monitoring of `pg_stat_activity`, `pg_locks`, query age, wait events, and hierarchical blocking PID chains, with guarded backend cancellation and termination (`pg_cancel_backend` / `pg_terminate_backend`).
5. **`030-visual-schema-designer-and-migrations` (Visual Schema Designer & Migrations):** Form-driven TUI designer for tables, columns, indexes, and constraints with live DDL preview, alongside dual-environment schema diffing and transactional migration generation (`BEGIN; ... COMMIT;`).
6. **`031-roles-privileges-and-rls-auditor` (Roles, Privileges & RLS Auditor):** Visual browser for role inheritance trees, object privilege ACL matrices, and Row-Level Security (`RLS`) policy auditing with missing-policy detection and reviewed `GRANT`/`REVOKE`/`POLICY` SQL.
7. **`032-plpgsql-routine-runner-and-profiler` (PL/pgSQL Routine Runner & Profiler):** Signature discovery, typed parameter prompt harness, safe test execution within rollback savepoints, `RAISE NOTICE` diagnostic streams, and execution profiling.
8. **`033-listen-notify-and-maintenance-ops` (LISTEN/NOTIFY & Maintenance Ops):** Dedicated asynchronous pub/sub event monitor for `LISTEN`/`NOTIFY` channels, plus guided maintenance runner for `VACUUM (VERBOSE, ANALYZE)`, `ANALYZE`, and `REINDEX CONCURRENTLY` with live progress bars from `pg_stat_progress_*`.

---

## Documentation Index

- `docs/architecture/pgadmin-parity-plan.md` - Full pgAdmin replacement parity architecture (M6: Features 026-033)
- `docs/product/roadmap.md` - Milestone order, exit gates, and planned feature roadmap
- `docs/operations/release.md` - Release verification, installation, rollback, and trust procedures

- `docs/status.md` - Verified capabilities, testing matrices, and current progress
- `docs/support/compatibility.md` - Supported PostgreSQL versions, platforms, and terminals
- `docs/design/keymap.md` - Comprehensive keyboard binding and mouse gesture specification
- `docs/design/theme-tokens.md` - 4-tier surface architecture, contrast ratios, and theme tokens
- `docs/design/information-architecture.md` - Layout specifications, pane minimums, and responsive behavior
- `docs/security/threat-model.md` - Secret management, redaction, and trust boundaries
- `docs/operations/local-development.md` - Contributor guide, container fixtures, and verification suites

## License

Apache-2.0. See [LICENSE](LICENSE).
