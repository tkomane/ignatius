# Tasks: W10 local-state hardening

**Input:** existing [specification](spec.md), [hardening plan](plan.md),
foundation REL-004 and history FR-604/605, SEC-604.

- [x] W10-001 Reproduce saved-query destination-link and predictable temporary
  overwrite failures using only isolated test directories.
- [x] W10-002 Add one exclusive, owner-only, synced atomic replacement helper
  and route configuration, history trimming and saved-query saves through it.
- [x] W10-003 Refuse linked/non-regular query entries consistently, preserving
  normal replacement and outside target bytes; move touched inline unit tests
  into sidecar `tests.rs` modules.
- [x] W10-004 Prove successful replacement, stale-entry preservation, failed
  rename recovery, missing-parent handling and Unix permissions.
- [x] W10-005 Update security and release documentation, run focused checks and
  the full verifier, and record exact evidence and remaining platform limits.
- [ ] W10-006 Harden migration-backup reservation against dangling links and
  concurrent filename claims; preserve the exact old configuration in a new
  private backup before replacement. This pre-existing follow-up was identified
  during peer review and is not covered by the replacement-helper evidence.

Focused evidence on 2026-10-25: the three original temporary-file regressions
and saved-query link regression failed before their fixes. After integration,
configuration 13, history 14, query 10 and atomic-helper 6 tests passed on macOS
arm64 with Rust 1.99.1. A native peer review found a valid long-name regression;
its new test failed before the bounded temporary-basename repair and passed
after it. Independent different-family review, the final full gate and native
Windows/Linux execution remain separate gates in `docs/status.md`.
