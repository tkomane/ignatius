# Tasks: Migration Runner and Ledger (045)

- [ ] T001: Implement `src/migrate/` discovery, version parsing, SHA-256 checksums, planning and renderers.
- [ ] T002: Add `src/migrate/tests.rs` for order, duplicates, drift, missing files, transaction-control refusal, bounds and output.
- [ ] T003: Add ledger constants, records query and transactional apply to `Session` with advisory locking and rollback.
- [ ] T004: Add `ignatius migrate status|plan|apply|verify` CLI with formats, `--to`, dry-run and production guard.
- [ ] T005: Add `tests/migration_contract.rs` for live apply, re-apply, drift, failure rollback, dry-run and exit codes.
- [ ] T006: Add `Action::Migrations`, model state, discovery, keymap `Ctrl+K :` and confirmation reducer.
- [ ] T007: Add the migrations overlay renderer with pending/applied/drift states and tests.
- [ ] T008: Document the authoring contract, keymap and CLI surfaces.
- [ ] T009: Run focused gates and the full `cargo xtask verify` battery.
- [ ] T010: Update `docs/status.md` and `CHANGELOG.md` with delivery evidence.
