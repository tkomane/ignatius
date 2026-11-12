# Tasks: Partition Lifecycle (048)

- [x] T001: Implement `src/partition/mod.rs` model, calendar periods, names, bounds, overlap and SQL rendering.
- [x] T002: Add `src/partition/tests.rs` sidecar tests for arithmetic, parsing, overlap, expiry and rendering.
- [x] T003: Add `src/postgres/partition.rs` bounded catalog snapshot and bound/key parsing.
- [x] T004: Add `ignatius partition status|plan` CLI with refusal diagnostics.
- [x] T005: Add `tests/partition_contract.rs` live acceptance tests including no-write proof.
- [x] T006: Document the lifecycle in README and `docs/design/partition-lifecycle.md`.
- [ ] T007: Run focused gates and the full `cargo xtask verify` battery.
- [ ] T008: Update `docs/status.md`, `CHANGELOG.md` and roadmap with delivery evidence.
