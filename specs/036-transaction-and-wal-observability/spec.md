# Feature Specification: Transaction & WAL Observability

**Feature Branch**: `feat/036-transaction-and-wal-observability`
**Created**: 2026-11-12
**Status**: In Progress
**Authority**: Roadmap Milestone 7 (M7: Enterprise Observability & Query Performance Forensics)

## User Scenarios & Testing

### User Story 1 - Real-Time Checkpointer & Background Writer Health (Priority: P1)

A database administrator or SRE troubleshooting sudden I/O saturation and transaction commit latency spikes opens the Transaction & WAL Observability cockpit (`Ctrl+K &` or `ignatius wal-stats --section checkpointer`) to inspect checkpoint cadence:
1. Checkpoint Distribution: Ratio of scheduled (`checkpoint_timed`) to forced/requested (`checkpoint_req`) checkpoints. When requested checkpoints dominate, PostgreSQL is exhausting `max_wal_size` prematurely, forcing synchronous disk flushing.
2. I/O Durations: Total and average write time (`checkpoint_write_time`) versus sync/fsync time (`checkpoint_sync_time`). High sync times expose slow storage arrays or saturated write caches.
3. Buffer Writes Attribution: Distinguishes buffers written by checkpointer, background writer (`buffers_clean`), and client backend processes (`buffers_backend`). Backend writes indicate checkpointer starvation, directly blocking transactional queries.
4. Engine Adaptation: Gracefully probes `pg_stat_checkpointer` on PostgreSQL 17+ and falls back seamlessly to `pg_stat_bgwriter` on PostgreSQL 14-16.

**Independent Test**: Connect to a running PostgreSQL 14-18 instance, execute checkpoint diagnostics via `ignatius wal-stats --section checkpointer --format json`, and assert that checkpoint timing, counts, and buffer attributions are accurately extracted.

---

### User Story 2 - WAL Generation Rates & Archiver Health (Priority: P1)

An engineer sizing storage, monitoring replication throughput, or troubleshooting archive delays needs to measure write-ahead log (WAL) generation velocity without installing external agents:
1. WAL Activity: Current log sequence number (`pg_current_wal_lsn()` on primary or `pg_last_wal_replay_lsn()` on standby), total WAL records generated, and WAL bytes written.
2. Full Page Writes: Tracks `wal_fpi` (full page images) indicating cache churn after checkpoints.
3. Archiver Telemetry: Inspects `pg_stat_archiver` for total archived segments, last archived WAL, failed archivals, and last failed WAL segment name.

---

### User Story 3 - Commit/Rollback Ratios, Conflicts & Temp File Spills (Priority: P1)

An application developer monitoring production deployment stability wants to identify transaction errors, deadlocks, and disk spills:
1. Transaction Rates: Calculates transactions committed (`xact_commit`) versus rolled back (`xact_rollback`).
2. Rollback Anomaly Warning: Flags rollback ratios exceeding 5% as warnings and > 20% as critical danger indicators.
3. Deadlocks & Conflicts: Surfaces deadlocks (`deadlocks`) and query cancellation conflicts (`conflicts`).
4. Temp File I/O: Tracks temporary file count (`temp_files`) and disk spill volume (`temp_bytes`) caused by hash joins or sorts exceeding `work_mem`.

---

### User Story 4 - Transaction ID (XID) Wraparound & Freeze Forensics (Priority: P1)

A site reliability engineer auditing a multi-terabyte database needs early warning against catastrophic PostgreSQL shutdown caused by transaction ID wraparound:
1. Computes the transaction ID age of `datfrozenxid` for each database relative to `autovacuum_freeze_max_age` (typically 200 million transactions) and the 2-billion wraparound ceiling.
2. Surfaces percentage of wraparound horizon consumed (`% to emergency freeze`).
3. Surfaces the oldest table `relfrozenxid` requiring vacuum freezing.

---

### User Story 5 - Streaming Replication Lag & Standby Telemetry (Priority: P2)

When running primary-replica topographies, an engineer inspects active streaming connections:
1. Queries `pg_stat_replication` for client IP, application name, replication state (`streaming`, `catchup`), and sync state (`sync`, `async`, `quorum`).
2. Calculates byte lag (sent vs written vs flushed vs replayed) and latency in milliseconds.

---

### User Story 6 - Field Industrial TUI Cockpit & CLI (Priority: P1)

Within the interactive workbench:
1. Keymap chord `Ctrl+K &` (`wal-observability`) opens the cockpit.
2. Dieter Rams / Teenage Engineering hardware status capsules:
   `[COMMITS/SEC: 1,420]`, `[ROLLBACK: 0.1%]`, `[WAL: 4.2 MB/s]`, `[CHECKPOINT: TIMED 98%]`, `[XID AGE: 12% SAFE]`, `[STANDBYS: 2]`.
3. Dual-pane layout across tabs:
   - `[1] Checkpoints & WAL`
   - `[2] Transactions & XID Freeze`
   - `[3] Standbys & Replication`
4. Non-interactive scriptable CLI subcommand `ignatius wal-stats` supporting `--format table|json|plain`, `--section all|checkpointer|wal|transactions|freeze|replication`, `--output <path>`, and `--force`.
