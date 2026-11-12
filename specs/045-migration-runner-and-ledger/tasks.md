# Tasks: Migration Runner and Ledger (045)

- [x] T001: Implement `src/migrate/` discovery, version parsing, SHA-256 checksums, planning and renderers.
- [x] T002: Add `src/migrate/tests.rs` for order, duplicates, drift, missing files, transaction-control refusal, bounds and output.
- [x] T003: Add ledger constants, records query and transactional apply to `Session` with advisory locking and rollback.
- [x] T004: Add `ignatius migrate status|plan|apply|verify` CLI with formats, `--to`, dry-run and production guard.
- [x] T005: Add `tests/migration_contract.rs` for live apply, re-apply, drift, failure rollback, dry-run and exit codes.
- [x] T006: Document the authoring contract and CLI surfaces.
- [x] T007: Run focused gates and the full `cargo xtask verify` battery.
- [x] T008: Update `docs/status.md` and `CHANGELOG.md` with delivery evidence.
