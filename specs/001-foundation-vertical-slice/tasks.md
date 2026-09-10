# Tasks: Foundation and proven vertical slice

**Input**: [spec.md](./spec.md), [plan.md](./plan.md)

Checkboxes are the durable record of what is actually done. A box is ticked only
when the behaviour exists and a test or recorded evidence proves it.

## Phase 1: Setup

- [x] T001 Initialise the repository with Spec Kit 0.16.4 scaffolding
- [x] T002 Create the Cargo package, pin the toolchain, commit the lockfile
- [x] T003 Configure crate lints: deny unsafe, warn on missing Debug, clippy all
- [x] T004 Capture release identity in `build.rs` as four distinct facts
- [x] T005 Add `docker/compose.yaml` and synthetic fixtures for a disposable server

## Phase 2: Foundational

- [x] T006 [P] `branding.rs`: one home for the product name and release identity
- [x] T007 [P] `exit_code.rs`: the stable exit-code contract, with tests
- [x] T008 `diagnostics/redaction.rs`: one redaction implementation, adversarial tests
- [x] T009 `diagnostics/diagnostic.rs`: layered diagnostics that redact on construction
- [x] T010 `diagnostics/logging.rs`: opt-in logging, size-bounded, no SQL or row values
- [x] T011 `platform/`: file permissions, socket support, platform naming
- [x] T012 [P] `config/paths.rs`: XDG-style resolution with explicit overrides
- [x] T013 [P] `config/schema.rs`: versioned schema, unknown keys rejected
- [x] T014 `config/store.rs`: atomic writes, validation, backed-up migration

## Phase 3: User Story 1 - Run a query and see the answer (P1)

- [x] T015 `connection/target.rs`: precedence, URI and keyword parsing, classification
- [x] T016 `postgres/tls.rs`: policy per sslmode, `verify-ca` refused, honest TLS state
- [x] T017 `postgres/session.rs`: connect, stream via simple query protocol, bootstrap facts
- [x] T018 `query/statements.rs`: PostgreSQL-aware statement boundaries
- [x] T019 `query/value.rs`: terminal-safe rendering, NULL distinction, width handling
- [x] T020 `query/result.rs`: bounded result sets with visible truncation
- [x] T021 `ui/theme.rs`: semantic tokens, three palettes, contrast tests
- [x] T022 `ui/terminal.rs`: RAII guard, panic hook, capability detection
- [x] T023 `ui/keymap.rs`: bindings as data, conflict detection, hints
- [x] T024 `ui/layout.rs`: full, compact and too-small layouts
- [x] T025 `app/`: model, messages, pure reducer, effects
- [x] T026 `cli/interactive.rs`: event loop, input thread, effect executor
- [x] T027 Integration test: `SELECT 1` renders with columns, count and timing
- [x] T028 Evidence: run the client in a pty and capture a rendered frame

## Phase 4: User Story 2 - Understand a failure (P1)

- [x] T029 `postgres/error.rs`: map driver and server errors to layered diagnostics
- [x] T030 Plain-language cause and action for the SQLSTATEs people actually hit
- [x] T031 Render the server's error position against the SQL buffer
- [x] T032 Integration test: missing relation reports SQLSTATE 42P01 and a position
- [x] T033 Integration test: wrong password is authentication, and never echoed
- [x] T034 Integration test: failed transaction names ROLLBACK as the recovery

## Phase 5: User Story 3 - Cancel a running statement (P2)

- [x] T035 `CancelHandle` on a second connection, with truthful wording
- [x] T036 Reducer: `CancellationRequested` state, second request suppressed
- [x] T037 Integration test: `pg_sleep(30)` cancelled, server confirms 57014

## Phase 6: User Story 4 - Use it from a script (P2)

- [x] T038 `cli/output.rs`: table, CSV, TSV, JSON, NDJSON, Markdown
- [x] T039 NDJSON refuses multiple result sets rather than guessing
- [x] T040 `cli/mod.rs`: command tree, global flags, presentation resolution
- [x] T041 `doctor` with human and JSON output, and a staged connection probe
- [x] T042 `config paths|show|validate|migrate` and generated completions
- [x] T043 CLI contract tests as a subprocess: exit codes and stream separation
- [x] T044 Broken pipe exits zero; failed queries write nothing to stdout

## Phase 7: Polish and cross-cutting

- [x] T045 Constitution, ADRs 0001-0008, landscape research
- [x] T046 Feature 001 spec, plan, tasks, traceability
- [x] T047 Repository documentation: README, CLAUDE.md, SECURITY, contributing
- [x] T048 Threat model and data-handling documentation
- [x] T049 Operations documentation: local development, verification, release
- [x] T050 CI workflow for macOS, Windows and Linux
- [ ] T051 Verify on Windows 11 in Windows Terminal with PowerShell 7 (CI builds and tests it; nobody has used it)
- [ ] T052 Verify on Linux, including the Unix socket path (automated socket evidence exists; full-screen Linux hand use remains unverified)
- [x] T053 Extend the CI server matrix to PostgreSQL 14 through 18
- [x] T054 Subprocess-level evidence for exit codes 5, 6 and 8
- [x] T055 Automated terminal-restoration test in CI (pty on Unix)
- [ ] T055a The same on Windows, which needs ConPTY
- [x] T056 Private repository with CI on three platforms and three server versions
- [x] T057 Advertised keys that no operating system claims by default
- [x] T058 Visual system: glyph tiers, icons, capsules, striping, motion

## Dependencies

Phase 1 and 2 block everything. Within Phase 3, T015 to T020 are independent of
each other and all block T026. User stories 2, 3 and 4 depend only on Phase 3 and
are independent of one another.

## Status

Phases 1 to 7 are complete except the three open items above, which require
platform hand evidence or the Windows ConPTY harness. They are
carried in `docs/status.md` as the next actions.

Partial T052 evidence on 2026-08-16: the focused
`tests/postgres_socket_integration.rs` check passed inside a disposable Linux
container against a real PostgreSQL Unix socket, executing `SELECT 1` through
the production session path. The same pinned Linux container also passed 497
library tests, all 38 CLI contract tests and all 3 Unix pty restoration tests.
T052 remains unchecked because this is automated Linux evidence, not hand use
of the full-screen client in a Linux terminal. The shared CI workflow now has
the socket fixture: its execution passed at `52872ac` in
[run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318),
rechecked on 2026-09-04. That historical source result does not verify newer
local features or complete the hand-use task.
