# Feature Specification: Connection Pool & PgBouncer Telemetry (037)

## Overview
Milestone 7 (Enterprise Observability & Query Performance Forensics) culminates with Feature `037-connection-pool-and-pgbouncer-telemetry`. Connection exhaustion and pooling contention are among the most catastrophic production failure modes in PostgreSQL environments. Ignatius provides real-time visibility into connection saturation, headroom before exhaustion, PgBouncer pool allocations, client wait queues, and connection state distributions.

## Functional Requirements
1. **Pure Domain Model (`src/pool/mod.rs` & `tests.rs`)**:
   - `PgBouncerPool`: Per-pool allocation metrics (database, user, active client connections, waiting client connections, active/idle/used server connections, max wait time, average query duration).
   - `ServerConnectionStats`: Server-wide connection metrics (active, idle, idle in transaction, fastpath, disabled), configured `max_connections`, `superuser_reserved_connections`, calculated headroom count, and saturation percentage.
   - `ClientWaitMetric`: Individual client connections blocked waiting for query execution, transaction locks, or pool allocations.
   - `PoolReport`: Complete aggregated connection pooling and server exhaustion diagnostic snapshot.
   - `PoolSection`: Section filtering (`All`, `Pools`, `Server`, `Wait`).
   - Pure tabular, JSON, and plain text formatting engines.
2. **PostgreSQL & PgBouncer Extractors (`src/postgres/metadata.rs` & `session.rs`)**:
   - Catalog queries extracting `pg_stat_activity` state counts and connection parameters (`max_connections`, `superuser_reserved_connections`).
   - PgBouncer integration probing `SHOW POOLS` / `SHOW STATS` when available, falling back gracefully to catalog activity analysis when connecting directly to PostgreSQL.
   - Session method `Session::connection_pool_telemetry`.
3. **Keymap & TEA Integration**:
   - Registered chord `Ctrl+K *` and configurable action `connection-pool`.
   - TEA Elm loop integration: `Action::ConnectionPool`, `Message::ConnectionPoolLoaded`, `Effect::LoadConnectionPool`.
   - `ConnectionPoolState` in `Model`.
4. **Field Industrial TUI Overlay (`src/ui/widgets/overlays.rs` & `layout.rs`)**:
   - Hardware status capsules: `[HEADROOM]`, `[SATURATION]`, `[ACTIVE]`, `[IDLE]`, `[WAIT QUEUE]`, `[POOLS]`.
   - 3 Diagnostic Tabs:
     - `[1] Pool Allocations & Queues`: PgBouncer pools or database/user connection allocations with client waiting queues.
     - `[2] Server Connections & Saturation`: Connection state breakdown with visual saturation percentage bar.
     - `[3] Client Wait Forensics`: Detailed connection inspector for active and waiting queries.
   - Hotkey controls: `[1]`/`[2]`/`[3]` tabs, `[r]` refresh, `Esc` close.
5. **Headless Scriptable CLI Subcommand (`ignatius pool-stats`)**:
   - Flags: `--section all|pools|server|wait`, `--format table|json|plain`, `--output <path>`, `--force`.
6. **Verification Battery**:
   - Contract tests in `tests/pool_telemetry_contract.rs`.
   - Documentation synchronization in `README.md`, `docs/design/keymap.md`, `docs/status.md`, `CHANGELOG.md`.
