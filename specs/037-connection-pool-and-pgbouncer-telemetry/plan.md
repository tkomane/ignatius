# Implementation Plan: Connection Pool & PgBouncer Telemetry (037)

## Architectural Strategy
Feature 037 completes Milestone 7 by establishing end-to-end connection pool observability without introducing external dependencies or fragile runtime scraping.

### 1. Domain Module (`src/pool/mod.rs` & `tests.rs`)
- Implement `PgBouncerPool`, `ServerConnectionStats`, `ClientWaitMetric`, `PoolReport`, `PoolSection`.
- Derive Serde `Serialize` and `Deserialize`.
- Implement tabular, JSON, and plain text renderers.
- Write sidecar unit tests in `src/pool/tests.rs`.

### 2. Catalog & Extraction Primitives (`src/postgres/metadata.rs` & `src/postgres/session.rs`)
- In `src/postgres/metadata.rs`: Implement `connection_pool_telemetry(&Client) -> Result<PoolReport, Diagnostic>`.
  - Read `max_connections` and `superuser_reserved_connections` via `SHOW`.
  - Query `pg_stat_activity` grouped by `datname`, `usename`, and `state`.
  - Query waiting connections (`wait_event_type IS NOT NULL`).
  - Calculate headroom and saturation percentage.
- In `src/postgres/session.rs`: Add `Session::connection_pool_telemetry`.

### 3. Keymap & Action Registration
- `src/ui/keymap.rs`: Register chord `*` -> `Action::ConnectionPool` and configurable action `connection-pool`.
- Update `docs/design/keymap.md` and `README.md`.

### 4. TEA Elm Architecture
- `src/app/message.rs`: Add `Action::ConnectionPool`, `Message::ConnectionPoolLoaded`, `Effect::LoadConnectionPool`.
- `src/app/model.rs`: Add `ConnectionPoolState` and fields to `Model`.
- `src/app/update.rs`: Implement `connection_pool_open` and `connection_pool_action`.
- `src/cli/interactive.rs`: Handle `Effect::LoadConnectionPool` by spawning async worker.

### 5. Field Industrial TUI Overlay
- `src/ui/widgets/overlays.rs`: Implement `render_connection_pool`.
- `src/ui/layout.rs`: Call `render_connection_pool` when `model.connection_pool.is_some()`.

### 6. CLI Subcommand
- `src/cli/mod.rs`: Add `PoolStats(PoolStatsArgs)` and implement `run_pool_stats`.

### 7. Contract Testing & Verification
- `tests/pool_telemetry_contract.rs`: Test help output, file protection without `--force`, and live outputs against test database.
- Run `cargo fmt` and full `cargo xtask verify`.
- Update `docs/status.md` and `CHANGELOG.md`.
