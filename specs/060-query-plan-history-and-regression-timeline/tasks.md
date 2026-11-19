# Tasks - Query plan history and regression timeline

- [x] T001 Implement FR-6001 and FR-6002 through the CLI and PostgreSQL session.
- [x] T002 Implement FR-6003 and FR-6005 with the versioned local catalog and
  prove validation, retention, privacy, and concurrent writer behavior.
- [x] T003 Implement FR-6004 and prove offline show, forget and comparability.
- [x] T004 Prove the live before/after-index journey and server-enforced
  refusal. Run focused and full gates and record which PostgreSQL versions ran.
- [x] T005 Update the user-facing command, compatibility, privacy and release
  note documentation and record implementation status.

Four focused unit tests cover comparability, ordering, lock contention,
corruption and retention. Three CLI contracts passed against PostgreSQL
18.7 plain and verify-full TLS on macOS arm64. The estimate and measurement
journey also passed against PostgreSQL 14.25, 15.20, 16.16 and 17.12. The
full verifier and existing benchmark and plan-diff contracts passed.
