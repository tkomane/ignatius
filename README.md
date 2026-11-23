<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="docs/assets/banner-light.svg" />
  <img src="docs/assets/banner-dark.svg" alt="Ignatius - PostgreSQL terminal workbench in Rust: TLS verify-full, read-only first, Atelier and Paper editions" width="96%" />
</picture>

### Calm confidence for real databases: `psql` honesty inside an editorial terminal workbench.

[![CI](https://github.com/tkomane/ignatius/actions/workflows/ci.yml/badge.svg)](https://github.com/tkomane/ignatius/actions/workflows/ci.yml)
[![Rust 2024](https://img.shields.io/badge/rust-2024%20edition-9A3115?labelColor=1B1917&logo=rust&logoColor=FAF5EC)](Cargo.toml)
[![PostgreSQL](https://img.shields.io/badge/postgresql-14%20|%2015%20|%2016%20|%2017%20|%2018-25486B?labelColor=1B1917&logo=postgresql&logoColor=FAF5EC)](docs/support/compatibility.md)
[![Capabilities](https://img.shields.io/badge/capabilities-M6--M16%20delivered-18593B?labelColor=1B1917&logo=checkmarx&logoColor=FAF5EC)](docs/product/roadmap.md)
[![Design](https://img.shields.io/badge/themes-atelier%20|%20paper%20|%20material%203%20|%20field%20industrial-714304?labelColor=1B1917&logo=color&logoColor=FAF5EC)](docs/design/theme-tokens.md)
[![Telemetry](https://img.shields.io/badge/telemetry-zero%20%2F%20private-144F63?labelColor=1B1917)](docs/security/data-handling.md)
[![License](https://img.shields.io/badge/license-Apache--2.0-4B443C?labelColor=1B1917)](LICENSE)

*Editorial obsidian & rag-paper craft, Material Design 3 elevation, and Dieter Rams tactile restraint.*<br>
*Fast, deterministic, built strictly in Rust (`ratatui` + `tokio-postgres`). Zero telemetry, zero accounts, zero cloud dependencies.*

<p align="center">
  <a href="#vision"><strong>Vision</strong></a> &bull;
  <a href="#editorial-craft--seven-built-in-themes"><strong>Themes &amp; Craft</strong></a> &bull;
  <a href="#visual-tour"><strong>Visual Tour</strong></a> &bull;
  <a href="#quickstart-try-ignatius-in-60-seconds"><strong>Quickstart</strong></a> &bull;
  <a href="#keyboard-shortcuts-chords--mouse-fluency"><strong>Keymap &amp; Chords</strong></a> &bull;
  <a href="#non-interactive-cli-scripting--streaming-export"><strong>CLI Suite</strong></a> &bull;
  <a href="#capabilities-m6-m16"><strong>Capabilities</strong></a>
</p>

<br />

<img src="docs/assets/screenshots/workbench-atelier.png" alt="Ignatius workbench in Atelier Editorial Dark with terracotta focus, obsidian-espresso surfaces, PostgreSQL/Codicon glyphs, italic comments and null markers, object tree, SQL editor, and results grid" width="96%" />
<br />
<sub><em>Atelier Editorial Dark (Feature 062): obsidian-espresso surfaces, terracotta keycaps, PostgreSQL/Codicon glyphs, italic comments &amp; <code>[null]</code>, whisper-quiet tree guides, and 2x Studio window chrome.</em></sub>

</div>

---

## Vision

There are two kinds of PostgreSQL client, and neither is the one you want when the database is real.

`psql` is the reference implementation: complete, scriptable, and trustworthy. It is also a line-oriented REPL where wide results collapse into text walls, where only your shell prompt reminds you which cluster a `DELETE` will hit, and where a mistyped column returns an error with nowhere to look next. Multi-database GUIs feel pleasant for browsing, yet treat PostgreSQL semantics superficially and rarely tell you what transport security your connection actually negotiated versus what you merely asked for.

So engineers use one client for casual exploration and fall back to `psql` for production, leaving them least fluent in the tool they use under the highest pressure.

**Ignatius ends that split.** Built in one compact Rust binary (`ratatui` + `tokio-postgres`), the client you explore schemas with daily is the exact engine you automate in CI and point at production with calm confidence: oriented, in control, and slightly delighted; never noisy, cryptic, brittle, or smug.

### What Sets Ignatius Apart

| Honesty Guarantee | How Ignatius Enforces It |
| --- | --- |
| **Transport truth from the server** | Connection badges reflect `pg_stat_ssl` (`SSLv3`/TLS version and cipher suite) read back from PostgreSQL, so `require` and `verify-full` are never blurred together. |
| **Options that fail closed** | Unhonored security settings such as `sslrootcert` fail loudly before entering the terminal instead of silently downgrading protection. |
| **Declared production posture** | `environment = "prod"` (`[PROD]`) and `read-only = true` (`[READ ONLY]`) are explicit operator declarations that guard destructive mutations and reviewed updates even with `NO_COLOR=1` and `--ascii`. |
| **Uncertainty is stated verbatim** | Cancellations show `Cancellation requested` until PostgreSQL confirms (`57014`); dropped sockets report `Connection lost - query outcome unknown`; `NULL` (`[null]`), empty string (`""`), and literal `"NULL"` never collide visually. |
| **One binary, TUI + CI CLI** | Interactive SQL workspaces, diagnostics, benchmarks, snapshots, and runbooks share the same PostgreSQL runtime, redaction pipeline, and exit codes (`0`-`130`) as scriptable `ignatius` subcommands. |

### What Ignatius Will Never Become

Ignatius stays sharp by refusing feature sprawl recorded in [the product vision](docs/product/vision.md):
- Not a generic multi-database client that flattens PostgreSQL types, locks, WAL, and replication into lowest-common-denominator SQL.
- Never an AI wrapper that ships your schemas, queries, or row values to external endpoints.
- No cloud accounts, workspace sync services, BI dashboard builders, or third-party plugin marketplaces.
- Deep administration workflows (`026`-`061`) stay SQL-first, bounded, previewable, and explicitly guarded.

---

## Editorial Craft & Seven Built-In Themes

Terminal tools do not need visual clutter to feel refined. Ignatius pairs **Apple HIG clarity and typographic hierarchy**, **Google Material Design 3 4-tier tonal elevation**, and **Dieter Rams / print editorial discipline**, codified in [the Ignatius Constitution](.specify/memory/constitution.md) (Principle XI):

- **4-Tier Painted Surfaces**: Every built-in palette maps distinct elevations (`Surface` outer frame, `SurfacePane` interiors, `SurfaceAlt` rails/headers/zebra stripes/recessed modal pills, `SurfaceOverlay` dialogs/elevated keycaps) using `Clear`-then-block overlay painting.
- **Tactile Keycaps & Non-Dimmed Modal Badges**:
  - Header identity badges, footer shortcut rails (cleanly right-aligned when space permits), and empty-state starter keys lift onto `SurfaceOverlay` via `Theme::keycap(token)` cleanly over `SurfaceAlt` bars.
  - Secondary overlay capsules (`[WARMUP: 2]`, `[TABLES: 1]`, `[CELLS: 2]`) rest recessed on `SurfaceAlt` inside `SurfaceOverlay` modals via `Theme::recessed_pill(token)` without dim-modifier wash-out, guarded by width-aware `render_capsule_bar` wrapping so pills never clip on tight viewports.
- **Literary Typography & Micro-Rules**: Split multi-span pane headers lead with focused accent markers and calm muted metadata, SQL comments (`SyntaxComment`) and `[null]` markers set in literary italic (`TrueColor`/`Indexed256`), tree indentation guides recede smoothly into `Border` tone behind semantic icons, the Spotlight command palette paints a full-width `SurfaceAlt` search strip (`list_row_0_y` invariant intact), and modal tab strips separate from content with hairline dividers.
- **PostgreSQL & Codicon Symbolism (`--glyphs nerd-font`)**: Opt-in Nerd Font iconography pairs the PostgreSQL elephant brand emblem with crisp single-cell Codicon and Material Design symbols across object trees, locks, diagnostics, and read/write postures alongside `CommitMono Nerd Font`, while `unicode` and `--ascii` keep every screen 100% self-explanatory without icon reliance.
- **Adaptive & Accessible Everywhere**: Automatic `OSC 11` terminal background detection (`ui.theme = "auto"`), full `[theme]` custom extension support, `Basic16` (surface fills left transparent cleanly), `NO_COLOR` (zero ANSI color sequences), and `--ascii` parity. Every non-border token satisfies `>= 4.5:1` contrast across all four surface tiers (`>= 7.0:1` in `high-contrast`).

| Preset | Character | Surface Sequence (`Base` -> `Pane` -> `Alt` -> `Overlay`) | Primary Accents |
| --- | --- | --- | --- |
| `atelier` | Editorial warm obsidian-espresso dark | `#141210` -> `#1B1917` -> `#24211E` -> `#2E2A26` | Terracotta `#E6825C`, champagne `#D8BE9A`, antique gold `#E6BD6A`, eucalyptus `#8EC5BC` |
| `paper` | Archival rag-paper editorial light | `#EAE3D5` -> `#F6F1E7` -> `#E4DCCD` -> `#DBD2C1` | Sienna `#9A3115`, Prussian slate `#25486B`, forest ink `#18593B` |
| `material` | Google Material Design 3 Tonal Dark | `#111318` -> `#191C22` -> `#22262E` -> `#2B303A` | Material blue `#8AB4F8`, cyan `#78D9EC`, sage `#81C995` |
| `field-industrial` | Dieter Rams / Teenage Engineering matte hardware | `#0E1117` -> `#151A23` -> `#1C2330` -> `#242D3D` | Safety orange `#FF6A00`, phosphor amber `#FBBF24`, sage `#10B981` |
| `dark` | TokyoNight-inspired cool midnight studio | `#11131D` -> `#1B1E2E` -> `#22263A` -> `#292E42` | Cobalt `#7AA2F7`, teal `#73DACA`, amber `#E0AF68` |
| `light` | Catppuccin Latte-inspired crisp daylight | `#E6E9EF` -> `#F4F6F9` -> `#E2E6F0` -> `#DCE0E8` | Royal blue `#1E66F5`, sapphire `#209FB5`, pine `#40A02B` |
| `high-contrast` | Pure WCAG AAA black & high-luminance ink | `#000000` -> `#0B0E14` -> `#161B26` -> `#1E2430` | Crisp sky `#80BFFF`, gold `#FFD466`, coral `#FF8A8A` |

Runtime theme switching via **Command Palette (`Ctrl+P` -> Switch theme)** or `[keys] switch-theme` cycles cleanly across `dark -> light -> high-contrast -> field-industrial -> material -> atelier -> paper -> custom?` without reconnecting.

<table>
  <tr>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/workbench-paper.png" alt="Archival Rag Paper workbench with Sienna focus and Prussian accents" />
      <br />
      <sub><strong>Archival Rag Paper (<code>paper</code>)</strong> - Warm editorial rag-paper light surfaces with Sienna keycaps and Prussian selection.</sub>
    </td>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/workbench-material.png" alt="Material 3 Tonal workbench with structured Results empty state" />
      <br />
      <sub><strong>Material 3 Tonal (<code>material</code>)</strong> - Calm neutral slate elevations and structured Results empty-state guidance.</sub>
    </td>
  </tr>
  <tr>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/workbench-field-industrial.png" alt="Field Industrial workbench with populated results grid" />
      <br />
      <sub><strong>Field Industrial (<code>field-industrial</code>)</strong> - Matte instrument chrome with safety-orange focus and high-legibility rows.</sub>
    </td>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/workbench-light.png" alt="Ignatius Light Theme workbench" />
      <br />
      <sub><strong>Light (<code>light</code>)</strong> - Cool daylight palette with crisp <code>[PROD]</code> capsules and full-width column headers.</sub>
    </td>
  </tr>
</table>

---

## Visual Tour

Images below are 2x HiDPI captures rendered directly from the Ignatius TUI buffer over synthetic database fixtures.

### Three-Pane Workbench (`workbench-overview.png`)

<p align="center">
  <img src="docs/assets/screenshots/workbench-overview.png" alt="Ignatius Three-Pane Workbench in Classic Dark" width="92%" />
</p>

- **Neo-tree Style Schema Sidebar (Left)**: Browse schemas, tables, views, materialized views, partitions, columns with ellipsis-safe type summaries, constraints, and triggers (`Enter` expands or opens definitions with `Ctrl+K d` and dependency graphs with `Ctrl+K y`).
- **Multi-Tab Split SQL Editor (Top-Right)**: Syntax highlighting, Focus/Muted `CursorLineNr` gutters, up to 16 concurrent buffers (`Ctrl+K =` / `Ctrl+K }` / `Ctrl+K {` / `Ctrl+K -`), vertical/horizontal split panes (`Ctrl+K \`), built-in & custom scratchpad snippets (`Ctrl+K ;`), atomic crash recovery (`Ctrl+K r` / `Ctrl+K d`), 1 MiB bounded bracketed paste, and deterministic SQL formatting (`Ctrl+Shift+F` or `Ctrl+K q`).
- **Truthful Results Grid (Bottom-Right)**: Full-width column header fill, row-number gutter, sort indicators, local filtering (`/`), frozen columns & visibility controls (`Ctrl+K g`), expanded vertical rows (`Ctrl+K x`), and structured JSONB/array/PostGIS/pgvector inspection (`Enter` or `Ctrl+K i`). Interactive split-border dragging and pane zoom (`Ctrl+K z`) keep focus fluid.

### Schema-Aware Completion & 60% Spotlight Command Palette

<table>
  <tr>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/completion-menu.png" alt="Schema-Aware SQL Completion Popup" />
      <br />
      <sub><strong>Blink.cmp-Style Completion (<code>Ctrl+Space</code>)</strong> - Asynchronous catalog completions with fuzzy matching and hairline dividers.</sub>
    </td>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/palette-preview.png" alt="Spotlight Command Palette with 60% Live SQL Preview" />
      <br />
      <sub><strong>Spotlight Command Palette (<code>Ctrl+P</code>)</strong> - Full-width recessed search strip with 40%/60% list and live SQL preview.</sub>
    </td>
  </tr>
</table>

### Query Plan Explorer & Actionable Error Diagnostics

<table>
  <tr>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/plan-explorer.png" alt="Interactive EXPLAIN ANALYZE Query Plan Explorer" />
      <br />
      <sub><strong>Plan Explorer (<code>Ctrl+K l</code> / <code>Ctrl+K a</code>)</strong> - Interactive <code>EXPLAIN</code> / <code>EXPLAIN ANALYZE</code> tree surfacing hottest nodes and estimate mismatches.</sub>
    </td>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/error-diagnostics.png" alt="Structured Query Error Diagnostics" />
      <br />
      <sub><strong>Layered Diagnostics</strong> - Exact SQL token context (<code>SELECT id, custmer_id...</code>), probable cause, next action, and expandable <code>SQLSTATE</code>.</sub>
    </td>
  </tr>
</table>

### Guarded Cell-to-UPDATE Review

<p align="center">
  <img src="docs/assets/screenshots/reviewed-update.png" alt="Reviewed Cell-to-UPDATE Modal" width="88%" />
</p>

Pressing `Ctrl+K u` on an eligible primary-key cell synthesizes an explicit parameterized `UPDATE` preview showing exact predicates before any query leaves the client. Production profiles (`environment = "prod"`) and `--read-only` sessions enforce write guards by default.

### Benchmarks, Schema Drift Guard & SQL Runbooks (M14)

<table>
  <tr>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/query-bench.png" alt="Query benchmark overlay in Material 3 Tonal" />
      <br />
      <sub><strong>Query Benchmarks (<code>Ctrl+K '</code>)</strong> - Warmup-separated A/B timing with <code>p50</code>/<code>p95</code>/<code>p99</code>, QPS, sparklines, and rollback safety.</sub>
    </td>
    <td align="center" width="50%">
      <img src="docs/assets/screenshots/schema-snapshot.png" alt="Schema snapshot drift overlay in Field Industrial" />
      <br />
      <sub><strong>Schema Drift Guard (<code>Ctrl+K &quot;</code>)</strong> - Zero-row <code>pg_catalog</code> SHA-256 baseline pinning and <code>Breaking</code>/<code>Warning</code>/<code>Info</code> diff gates.</sub>
    </td>
  </tr>
</table>

<p align="center">
  <img src="docs/assets/screenshots/sql-runbook.png" alt="SQL runbook overlay with write-guard dry-run planner" width="88%" />
  <br />
  <sub><strong>SQL Runbooks (<code>Ctrl+K ?</code>)</strong> - Markdown and <code>-- @cell</code> notebooks with <code>classify_all</code> write guards and dry-run safety previews.</sub>
</p>

---

## Quickstart: Try Ignatius in 60 Seconds

Ignatius ships with disposable PostgreSQL containers pre-seeded with realistic ecommerce, analytics, and rendering fixtures (`public.orders`, `public.recent_orders`, `public.rendering_cases`, `public.type_coverage`, and `reporting.order_totals`).

### 1. Start the Playground
```bash
cargo xtask db up
```
Starts disposable PostgreSQL on port `55432` (plain) and `55433` (verified TLS).

### 2. Launch the Workbench
Run directly from source against the plain fixture:
```bash
cargo xtask run
```

Or install locally (`~/.local/bin/ignatius`) and launch against either plain or verified TLS:
```bash
cargo xtask install

# Plain fixture
PGPASSWORD="$(sed -n 's/^POSTGRES_PASSWORD=//p' docker/dev.env)" \
  ignatius connect 'postgres://ignatius_test@127.0.0.1:55432/ignatius_demo?sslmode=disable'

# Verified TLS fixture
PGPASSWORD="$(sed -n 's/^POSTGRES_PASSWORD=//p' docker/dev.env)" \
  PGSSLROOTCERT="$PWD/docker/tls/generated/ca.crt" \
  ignatius connect 'postgres://ignatius_test@localhost:55433/ignatius_demo?sslmode=verify-full'
```

### 3. Immediate Actions to Try
- Run `SELECT order_id, customer_id, total, note FROM public.orders ORDER BY order_id;` with `Ctrl+R`.
- Press `Tab` to switch panes; navigate Results cells with arrow keys and open Cell Detail with `Enter` (`Ctrl+K i`).
- Press `Ctrl+K u` on an eligible row to preview a guarded `UPDATE`, or `Ctrl+K l` to inspect the query plan.
- Press `Ctrl+P` for the 60% SQL preview Command Palette, and select **Switch theme** to cycle across `dark`, `light`, `high-contrast`, `field-industrial`, `material`, `atelier`, and `paper`.

### 4. Tear Down
```bash
cargo xtask db down
```

---

## Connecting to PostgreSQL

### URIs and Command-Line Flags
```bash
# Standard connection URI
ignatius connect "postgres://app_user:secret@db.internal:5432/analytics?sslmode=verify-full"

# Individual connection flags and server-enforced read-only mode
ignatius connect --host db.internal --port 5432 --username app_user --dbname analytics --sslmode verify-full --read-only

# Named profile
ignatius connect @production-orders
```

### Configuration Profiles (`config.toml`)
Profiles and UI preferences live in `~/.config/ignatius/config.toml` (Linux/macOS) or `%APPDATA%\ignatius\config.toml` (Windows):

```toml
[ui]
theme = "auto"              # "dark", "light", "high-contrast", "field-industrial", "material", "atelier", "paper", "custom", or "auto" (OSC 11)
mouse = true                # enable mouse clicks, drags, and scrolling
density = "comfortable"     # "comfortable" or "compact"
reduced-motion = false      # set true to disable animations
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
read-only = true            # enforces PostgreSQL session read-only transaction mode
```

### Cloud Identity Authentication & Secret Safety
Short-lived cloud token providers work without stored passwords via `[profiles.<name>]`:
- `auth = "entra"` (Azure Database for PostgreSQL via `az account get-access-token`)
- `auth = "aws"` (AWS RDS IAM token via AWS CLI)
- `auth = "gcp"` (Google Cloud SQL via `gcloud auth print-access-token`)

All passwords and tokens are wrapped in `secrecy::SecretString`, zeroized on drop, passed through a single audited redaction pipeline, and never written to disk or statement history. Credentials resolve from connection strings, `.pgpass`, `PGPASSWORD`, or interactive masked prompts.

---

## Keyboard Shortcuts, Chords & Mouse Fluency

Zero Vim-modal traps: printable keys type cleanly while `Tab`, `Ctrl+*`, function keys, and two-key `Ctrl+K` chords drive every capability.

### Global Navigation, Editor & Results Keys
| Context | Key | Action |
| --- | --- | --- |
| Global | `Tab` / `Shift+Tab` | Cycle focus between Sidebar, Editor, and Results panes |
| Global | `Ctrl+P` | Open Spotlight Command Palette with 60% live preview |
| Global | `Ctrl+B` | Toggle Neo-tree Schema Sidebar |
| Global | `Ctrl+C` / `Ctrl+Q` | Cancel active query cleanly / quit Ignatius |
| Global | `F1` / `Ctrl+G` / `Esc` | Toggle Help / dismiss active overlay or prompt |
| Editor | `Ctrl+R` (`F5`) / `Ctrl+T` (`F9`) | Execute full SQL buffer / statement under cursor |
| Editor | `Ctrl+Space` / `Ctrl+Shift+F` | SQL autocomplete popup / deterministic formatting |
| Editor | `Ctrl+Z` / `Ctrl+Y` | Word-level Undo / Redo (`Ctrl+Left`/`Right`, `Home`/`End`) |
| Results / Tree | `Up`/`Down`/`Left`/`Right` | Navigate grid cells or sidebar tree (`PageUp`/`PageDown`/`Home`/`End`) |
| Results / Tree | `Enter` (`Ctrl+K i`) / `/` (`Ctrl+K f`) | Inspect cell or expand object / filter grid or tree |
| Results | `F6` / `Ctrl+Shift+R` | Refresh retained result set by re-running its SQL |

### Two-Key Chord Menu (`Ctrl+K`)
Pressing `Ctrl+K` displays an on-screen chord guide; press the second key to execute:

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
| `m` | Open the schema ER diagram |
| `j` | Open the server activity and locks dashboard |
| `t` | Open the roles, privileges, and RLS auditor |
| `k` | Open the schema diff and migration designer |
| `<` | Open the data import wizard |
| `(` | Open the routine runner and profiler |
| `>` | Open maintenance operations and pub-sub monitor |
| `%` | Open query performance and stat forensics |
| `^` | Open index advisor and bloat forensics |
| `&` | Open transaction, WAL, and checkpointer observability |
| `*` | Open connection pool and pgbouncer telemetry |
| `)` | Open data masking and anonymization rules |
| `|` | Open cross-database copy and sync pipelines |
| `+` | Open synthetic test fixture generator |
| `=` | Create new SQL editor buffer |
| `-` | Close current SQL editor buffer |
| `}` | Switch to next SQL editor buffer |
| `{` | Switch to previous SQL editor buffer |
| `\` | Toggle editor buffer split mode |
| `;` | Open snippet and scratchpad library |
| `!` | Lint the active SQL buffer |
| `#` | Open logical replication and slots manager |
| `@` | Open server config, HBA, extensions, and FDWs |
| `:` | Stop active psql watch loop |
| `_` | Open live data browser and batch CRUD |
| `/` | Open extended EXPLAIN and A/B plan diff |
| `$` | Open result profiler and HTML/Parquet export |
| `'` | Run query latency benchmark and A/B suite |
| `"` | Inspect schema snapshot and drift guard |
| `?` | Open interactive SQL Markdown runbook |

When crash-recovery buffers are pending on startup, contextual `Ctrl+K r` restores them and `Ctrl+K d` discards the journal cleanly.

### Mouse Fluency
| Target Region | Gesture | Action |
| --- | --- | --- |
| Any pane interior / row | Left-click / Double-click | Focus pane, select row/cell, or open Cell Detail / definition |
| Results header / column rule | Left-click / Left-drag | Cycle sort (`Asc` -> `Desc` -> `Server`) / resize column width |
| Vertical or horizontal split | Left-drag | Resize sidebar width or editor/results height |
| Overlay / Palette / Panes | Double-click / Scroll / Shift+Scroll | Accept item, scroll vertically (3 rows), scroll grid horizontally |

---

## Non-Interactive CLI, Scripting & Streaming Export

Every core capability works headlessly for scripts, pipelines, and CI gates:

### Query Execution & Streaming Export
```bash
# Formats: table, csv, tsv, json, ndjson, markdown, and insert
ignatius query "$DATABASE_URL" -c "SELECT id, name, email FROM users" --format json

# Named environment parameter binding without SQL interpolation
IGNATIUS_MONTH=2026-11 ignatius query "$DATABASE_URL" -f monthly.sql --param-env month=IGNATIUS_MONTH

# Stream rows directly to disk (.partial safety on interruption -> exit 9)
ignatius query "$DATABASE_URL" -c "SELECT * FROM large_audit_log" --format csv --output audit_export.csv

# psql backslash meta-commands over CLI or TUI
ignatius query "$DATABASE_URL" -c '\dt' --format json
```

### Operational & CI Automation Commands
```bash
# Diagnostics, configuration, history, snippets & crash recovery
ignatius doctor
ignatius config check
ignatius connect --check @production-orders
ignatius history list
ignatius history clear --yes
ignatius snippet list
ignatius snippet show cache-hit-ratio
ignatius recovery status
ignatius recovery restore --format sql > recovered.sql
ignatius recovery discard

# Offline SQL lint (13 lexical rules -> table, json, sarif), forward-only migrations & data dictionary
ignatius lint migrations/ --fail-on error --format sarif --output lint.sarif
ignatius migrate status "$DATABASE_URL" --dir migrations
ignatius migrate apply "$DATABASE_URL" --dir migrations --dry-run
ignatius docs "$DATABASE_URL" --schema public --format markdown --output docs/data-dictionary.md

# Growth audit, partition lifecycle & scratch restore drills (M11)
ignatius growth "$DATABASE_URL" --top 20 --fail-on critical
ignatius partition status "$DATABASE_URL" --schema billing
ignatius partition plan "$DATABASE_URL" --table billing.invoices --period monthly --start 2026-12-01 --count 3
ignatius restore-drill "$DATABASE_URL" --backup /backups/orders.dump --dry-run

# Catalog ER diagrams, deterministic DDL, streaming COPY, backup/restore & schema diff (M6)
ignatius diagram "$DATABASE_URL" --schema public --format mermaid --output schema.mmd
ignatius ddl "$DATABASE_URL" --schema public --output schema.sql
ignatius import "$DATABASE_URL" orders data.csv --delimiter ',' --header
ignatius export "$DATABASE_URL" -c "SELECT * FROM orders" --format csv --output orders.csv
ignatius backup "$DATABASE_URL" backup.dump --format custom
ignatius restore "$DATABASE_URL" backup.dump --clean
ignatius diff "$DATABASE_URL" --source-schema public --target-schema reporting

# Activity/locks, security/RLS audit, routines, pub/sub & maintenance (M6)
ignatius activity "$DATABASE_URL" --locks
ignatius audit "$DATABASE_URL" --section rls --schema public
ignatius run-routine "$DATABASE_URL" process_order -a 42 --rollback
ignatius profile-routine "$DATABASE_URL" calculate_tax --format json
ignatius listen "$DATABASE_URL" order_events --timeout 30 --format jsonl
ignatius notify "$DATABASE_URL" order_events "order #1042 placed"
ignatius maintain "$DATABASE_URL" vacuum --target orders --analyze

# Query stats, index/bloat forensics, WAL/checkpointer & pool telemetry (M7)
ignatius profile-queries "$DATABASE_URL" --sort mean --limit 20 --format table
ignatius index-advisor "$DATABASE_URL" --format table
ignatius wal-stats "$DATABASE_URL" --format table
ignatius pool-stats "$DATABASE_URL" --format table

# Data masking, topological cross-DB sync, synthetic fixtures & replication/cluster catalogs (M8, M12)
ignatius mask-rules "$DATABASE_URL" --schema public
ignatius sync --source "$STAGING_URL" --target "$DEV_URL" --dry-run
ignatius seed "$DATABASE_URL" --schema public --rows 25 --format sql
ignatius replication "$DATABASE_URL" --section all --format table
ignatius cluster "$DATABASE_URL" --section settings --modified-only --format json

# Keyset browser, EXPLAIN BUFFERS/WAL plan diff, HTML/Parquet analytics, benchmarks, snapshots, runbooks & fleet (M13-M16)
ignatius browse "$DATABASE_URL" --table public.orders --page-size 50 --format table
ignatius plan-diff "$DATABASE_URL" --baseline "SELECT * FROM orders" --candidate "SELECT id FROM orders WHERE id = 1" --analyze
ignatius analytics "$DATABASE_URL" -c "SELECT * FROM orders" --format parquet -o orders.parquet
ignatius bench "$DATABASE_URL" -c "SELECT * FROM public.orders" --warmup 2 --runs 10 --format table
ignatius snapshot capture "$DATABASE_URL" --schema public -o schema.snapshot.json
ignatius snapshot check "$DATABASE_URL" --against schema.snapshot.json --schema public --fail-on breaking
ignatius runbook --file incident.md --dry-run --format table
ignatius plan-history show --series orders-plan --format json
ignatius fleet --baseline @app-dev --candidate @app-staging --schema public --format json
ignatius completion zsh > ~/.zfunc/_ignatius
```

### Exit Codes
| Code | Identifier | Meaning |
| --- | --- | --- |
| `0` | `success` | Command completed cleanly; all statements succeeded |
| `1` | `findings` | Configured threshold reached (`lint`, `growth`, `bench`, `snapshot`, or `fleet` drift) |
| `2` | `usage` | Invalid CLI arguments, flags, or unsupported terminal combination |
| `3` | `config` | Configuration file invalid, missing, or unparseable |
| `4` | `connection` | Network, socket, DNS, or TCP failure |
| `5` | `authentication` | Server reached; credentials or token rejected |
| `6` | `tls` | TLS handshake or certificate verification policy failed |
| `7` | `query` | PostgreSQL returned a statement error |
| `8` | `cancelled` | Statement cancelled cleanly by user or server |
| `9` | `export-interrupted` | Export interrupted; partial data preserved at `.partial` |
| `70` | `internal` | Unexpected client invariant violation |
| `130` | `sigint` | Terminated via standard `SIGINT` |

---

## Accessibility, Plain Mode & Operations

- **Linear Screen-Reader & Pipe Mode (`--plain`)**: `ignatius --plain connect "$DATABASE_URL"` disables alternate screen, raw mode, and all ANSI escape sequences so output streams cleanly for VoiceOver, NVDA, `TERM=dumb`, and pipelines while stating environment, production identity, and transaction state explicitly in words.
- **Installation & Local Builds**: Run from source via `cargo xtask run` or install via `cargo xtask install` (`~/.local/bin/ignatius`). Native release archive verification, reproducible checksums, and ADR-0008 release signing/publication posture live in [release operations](docs/operations/release.md).
- **Local State & Removal**: Interactive history and crash-recovery journals live strictly under local XDG/AppData directories (`ignatius history clear --yes`, `ignatius recovery discard`). Uninstall by removing `~/.local/bin/ignatius` and `~/.config/ignatius`.

---

## Capabilities (`M6`-`M16`)

### Milestone 6: Full pgAdmin 4 Replacement Parity (`026`-`033`)
Implemented across TUI overlays and scriptable CLI routes under [pgadmin-parity-plan.md](docs/architecture/pgadmin-parity-plan.md):
- `026-schema-er-diagrams` (`Ctrl+K m`, `ignatius diagram` - Mermaid, DOT, SVG, ASCII, Unicode)
- `027-deep-catalog-and-ddl-generator` (`Ctrl+K d`, `ignatius ddl` - `CREATE`/`ALTER`/`DROP` DDL across `pg_catalog` with `Toggle DROP` and `Load in Editor`)
- `028-data-import-export-and-backup` (`Ctrl+K <`, `ignatius import`, `export`, `backup`, `restore` - streaming `COPY` & guarded `pg_dump`/`pg_restore`)
- `029-server-activity-and-locks-dashboard` (`Ctrl+K j`, `ignatius activity` - `pg_stat_activity`, blocker trees, guarded signals)
- `030-visual-schema-designer-and-migrations` (`Ctrl+K k`, `ignatius diff` - table/column/index/constraint designer & transactional schema diff)
- `031-roles-privileges-and-rls-auditor` (`Ctrl+K t`, `ignatius audit` - role trees, ACLs, RLS policy auditor)
- `032-plpgsql-routine-runner-and-profiler` (`Ctrl+K (`, `ignatius run-routine`, `profile-routine` - savepoint-isolated test harness & profiler)
- `033-listen-notify-and-maintenance-ops` (`Ctrl+K >`, `ignatius listen`, `notify`, `maintain` - async pub/sub monitor & guarded vacuum/reindex progress)

### Milestones 7-16 Overview (`034`-`062`)
| Milestone | Features | Highlights |
| --- | --- | --- |
| **M7 Observability & Perf** | `034`-`037` | `pg_stat_statements` forensics (`Ctrl+K %`), index/bloat/HypoPG advisor (`Ctrl+K ^`), WAL/XID wraparound/replication lag (`Ctrl+K &`), PgBouncer & pool saturation (`Ctrl+K *`) |
| **M8 Masking, Sync & Seed** | `038`-`040` | PII heuristic discovery & masked views (`Ctrl+K )`), topological streaming `COPY` cross-DB sync (`Ctrl+K \|`), deterministic PRNG referential test seeding (`Ctrl+K +`) |
| **M9 Multi-Buffer & Recovery** | `041`-`043` | Up to 16 tabs & split SQL panes (`Ctrl+K =`/`\`/`-`), built-in/custom SQL snippet library (`Ctrl+K ;`), atomic session crash recovery (`Ctrl+K r`/`d`) |
| **M10 CI Workflow Automation** | `044`-`046` | 13 offline SQL safety lint rules with SARIF (`Ctrl+K !`, `ignatius lint`), checksummed forward-only migrations (`ignatius migrate`), deterministic Markdown/JSON/Mermaid data dictionaries (`ignatius docs`) |
| **M11 Operational Lifecycle** | `047`-`049` | Isolated scratch backup restore drills (`ignatius restore-drill`), reviewed range-partition DDL (`ignatius partition`), heap/TOAST/sequence wraparound growth audits (`ignatius growth`) |
| **M12 `psql` & Cluster Parity** | `050`-`052` | Interactive `\d`/`\dt`/`\watch` (`Ctrl+K :`), logical replication & WAL slot retention (`Ctrl+K #`), `pg_settings`/`pg_hba_file_rules`/extensions/FDW catalog (`Ctrl+K @`) |
| **M13 CRUD, Plan Diff & Analytics** | `053`-`055` | Keyset data browser with reviewed multi-row CRUD (`Ctrl+K _`), `EXPLAIN (BUFFERS, WAL)` A/B plan diff (`Ctrl+K /`), zero-query sparklines plus HTML/Parquet export (`Ctrl+K $`/`e`) |
| **M14 Bench, Snapshots & Runbooks** | `056`-`058` | Rollback-isolated A/B query latency suite (`Ctrl+K '`), SHA-256 `pg_catalog` drift gates (`Ctrl+K "`), write-guarded Markdown & `-- @cell` notebooks (`Ctrl+K ?`) |
| **M15 Types, History & Fleet** | `059`-`061` | Structured JSONB/array/range/PostGIS/pgvector cell tree & reviewed edits, digest-only `ignatius plan-history`, read-only cross-environment `ignatius fleet` comparisons |
| **M16 Editorial TUI & Themes** | `062` | Curated obsidian-espresso `atelier` & rag-paper `paper` palettes (`>= 4.5:1` across 4 tiers), `Theme::keycap` & `recessed_pill` hierarchy, Spotlight search strip, hairline rules, 14 2x HiDPI renders |

---

## Documentation Index

- [docs/product/vision.md](docs/product/vision.md) - Product problem, honesty guarantees, and explicit non-goals
- [docs/product/roadmap.md](docs/product/roadmap.md) - Milestone sequence, exit criteria, and feature train
- [docs/architecture/pgadmin-parity-plan.md](docs/architecture/pgadmin-parity-plan.md) - M6 pgAdmin 4 parity architecture (`026`-`033`)
- [docs/design/theme-tokens.md](docs/design/theme-tokens.md) - 4-tier surface elevations, contrast thresholds, and custom themes
- [docs/design/keymap.md](docs/design/keymap.md) - Complete keybindings, chords, and mouse gestures
- [docs/design/information-architecture.md](docs/design/information-architecture.md) - Pane geometry, compact responsive behavior, and layouts
- [docs/support/compatibility.md](docs/support/compatibility.md) - Supported PostgreSQL versions, terminals, platforms, and open acceptance gates
- [docs/security/threat-model.md](docs/security/threat-model.md) - Secret handling, redaction boundaries, and security assumptions
- [docs/operations/release.md](docs/operations/release.md) - Release candidate checklist, checksums, and ADR-0008 readiness gates
- [docs/operations/local-development.md](docs/operations/local-development.md) - Contributor setup, container fixtures, and test commands
- [docs/status.md](docs/status.md) - Current implementation state and verification ledger

## License

Apache-2.0. See [LICENSE](LICENSE).
