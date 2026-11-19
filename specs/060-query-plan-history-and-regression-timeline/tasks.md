# Tasks - Query plan history and regression timeline

- [x] T001 Implement FR-6001 and FR-6002 through the CLI and PostgreSQL session.
- [x] T002 Implement FR-6003 and FR-6005 with the versioned local catalog and
  prove validation, retention, privacy, and concurrent writer behavior.
- [x] T003 Implement FR-6004 and prove offline show, forget and comparability.
- [ ] T004 Prove the live before/after-index journey and server-enforced
  refusal. Run focused and full gates and record which PostgreSQL versions ran.
- [ ] T005 Update the user-facing command, compatibility, privacy and release
  note documentation and record implementation status.

Focused checks passed locally on macOS arm64. Three unit tests cover
comparability, lock contention, corruption and retention. The two CLI
contracts passed against PostgreSQL 18.7 over plain TCP and verify-full
TLS. The existing plan-diff contract passed against plain PostgreSQL 18.7.
