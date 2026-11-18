# Tasks: Server Config, HBA, and Infrastructure Catalog (052)

- [x] T001: Author `specs/052-server-config-hba-and-infra-catalog/spec.md`, `plan.md`, and `tasks.md`.
- [x] T002: Implement `src/cluster/mod.rs` pure domain types, bounds, HBA risk classification, filtering, summary counters, ASCII/JSON renderers, option redaction, and reviewed SQL generators.
- [x] T003: Implement `src/cluster/tests.rs` sidecar unit tests covering HBA classification, bounds, filtering, rendering, SQL quoting, and secret masking.
- [x] T004: Implement `src/postgres/cluster.rs` bounded read-only catalog extractors compatible with PostgreSQL 14 through 18 with graceful `42501`/`42P01` HBA fallback and guarded tablespace sizing.
- [x] T005: Coordinator wiring in `src/lib.rs`, `src/postgres/mod.rs`, `src/postgres/session.rs`, `src/cli/mod.rs`, and `src/app/*`.
- [x] T006: Run full `cargo xtask verify` and live PostgreSQL integration tests once wired.
