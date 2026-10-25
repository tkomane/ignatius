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
- [x] W07-061 Keep the Unix PTY deadline attached to its owned `Child`, with
  polling, termination and reaping before return. Remove the detached delayed
  numeric-PID kill, which could outlive a completed test and target a reused
  PID. Keep output capture bounded by child completion without pipe backpressure
  or detached reader/watchdog threads; preserve all three normal-quit assertions
  and profile isolation. Prove normal completion and a deliberately hung owned
  child, then run the four existing PTY contracts before expanding abnormal-exit
  coverage. This harness repair does not establish panic/error/Windows evidence.
- [x] W07-059 Isolate the Unix PTY harness before expanding exit coverage:
  allocate unique temporary configuration and data roots, remove inherited
  logging and PostgreSQL connection environment, and keep those roots alive
  through the child lifetime. Prove the actual child receives that isolation
  without reading the developer's real history. Retain the existing live PTY
  render/normal-quit checks; do not count this as Windows or abnormal-exit proof.
- [x] W07-060 Close the Unix startup registration window in REL-001. Register
  SIGTERM under the runtime before acquiring the terminal; pass the registered
  receiver into the existing watcher and preserve Windows/query cancellation.
  First reproduce the old asynchronous-registration race in an isolated
  self-exec child whose current-thread runtime has not been polled, then prove
  buffered delivery through the production preparation/watcher path. Inject a
  registration failure through a private callback seam and prove acquisition
  is never called. Use no production trigger flag, unsafe FFI or parent-process
  signal. Keep actual steady-state PTY restoration evidence separate.
- [x] W07-062 Prove error-return and panic restoration in owned isolated Unix
  PTY children through the actual `TerminalGuard` and panic hook, with default
  and non-default mouse/paste/screen options. Assert restoration precedes panic
  details and matches the acquired options. Reproduce and repair any default-
  option assumption before expanding mouse capture. Remove unsupported panic
  claims that no data was written or that arbitrary details are secret-free;
  report failed restoration attempts truthfully. Keep the existing previous-
  hook/payload contract and all query behavior unchanged. Use no production
  panic flag, unsafe FFI, real profile, database write or signal to an unowned
  PID. Record exactly which modes/paths were observed; Windows stays separate.
- [x] W07-063 Observe the production guard/hook during an uncaught panic in
  a standalone owned Unix library-helper process, outside libtest and without
  catch_unwind. Check all eight acquired mode combinations, before/after actual
  PTY flags and restoration before previous/default hook details. Follow the
  bounded `plan.md` probe; distinguish this from a full product crash, abort,
  Windows or native visual acceptance.
- [ ] W07-064 Implement the owned Windows ConPTY observation slice under
  `contracts/windows-conpty-observation.md`: establish native mode/cursor/buffer
  capability and no-guard/known-unrestored/hung-child controls before expanding
  the actual guard normal/error/uncaught-panic option matrix. Use test-only
  native SDK interop and the selected safe Rust library helper, owned jobs/
  deadlines/drains and exact source/compiler/runner evidence. Preserve product
  behavior and query/privacy contracts. Require actual Windows runtime and
  independent review; leave unobservable paste/buffer modes and full T055a,
  T051 hand use and release acceptance explicit.
- [x] W10-007 Baseline oversized-value and notice memory at clean local
  `24db9c5` using the bounded experiment in `plan.md`, actual optimized binary
  identity, isolated state and the existing synthetic PostgreSQL fixture.
  Separate retained-row behavior, pre-retention driver allocation, notice-count
  growth and complete streaming output; record peak RSS, byte/row counts,
  medians/ranges and failures without inventing a whole-process ceiling.
- [ ] W10-008 Use W10-007 evidence to specify distinct operational bounds for
  notice retention, aggregate retained-result bytes and incoming frame/value
  allocation, including truthful refusal, unknown-outcome and recovery wording.
  Identify a real slow-export-sink measurement seam; a slow server is not a slow
  sink. Add each implementation task only after its relevant complete contract
  agrees. A bounded notice slice alone cannot close the complete W10 memory
  requirement.
- [x] W10-009 Repair the measured silent buffered-query truncation route:
  CSV/TSV/NDJSON/Markdown/INSERT must report retained versus received counts on
  stderr without changing data bytes, success exit codes or automatically
  rerunning SQL. Table/JSON keep their existing in-band counts; streaming file
  exports remain complete and separate. Follow the exact `plan.md` policy,
  add failing-first focused/live contracts and correct the CLI's claim that
  streamed values never pass through memory.
- [x] W10-011 Measure real consumer backpressure through the actual
  Session/StreamWriter/Export library path at clean `d8a3ac3`, using the exact
  pacing/workload/stop contract in `plan.md`. Compare three fresh children per
  unpaced/paced small-row/large-value case, complete file bytes/digests/counts
  and peak RSS; preserve the synthetic fixture and remove only owned payloads.
  Keep this distinct from top-level CLI cancellation, TLS and process ceilings.
- [x] W10-012 Implement REL-005's complete bounded notice slice under
  `contracts/notice-retention.md`: cumulative admission and sticky privacy,
  precision-safe guard/report/checkpoint lifecycle, report-carrying query/stream/
  plan/connect outcomes, unchanged CLI stdout and single JSON diagnostics,
  plain delivery and bounded persistent TUI pending status/explicit reader.
  Preserve protected input/SQL/transaction/cancellation semantics. Watch focused
  boundaries fail first, retain existing sidecar tests, obtain independent peer
  review, inspect rendered output and pass required combined gates. Do not
  relabel this as a frame/aggregate-result/process-memory ceiling.
  Local source: `d65024b`; peer/rendered review and full 1,370-case gate passed.
  Published head `fda16f8` passed hosted Linux/macOS/Windows and Rust1.90 gates,
  PG14/18 plain+verify-full notices/runtime/session paths and the real socket
  contract. Native hand-use, broader W05 matrix and other resource routes remain
  separate. External model review was owner-skipped; native peer review passed.
- [x] W10-013 After W10-012 source integration, implement REL-002's complete
  execution-wide retained-data allowance under `contracts/retained-result-data.md`:
  checked whole-description/whole-row admission, cumulative prefix closure,
  truthful typed description unavailability, compatible known data/output,
  explicit new unavailable JSON/status case and pre-effect retained-data action
  refusals. Preserve notice/SQL/transaction/cancellation/stream contracts; obtain
  focused/live/peer/full/rendered and affected server evidence before closure.
  Final milestone03 passed native peer review, canonical full 1,431 outer
  cases/1,127 library and 112 styled-buffer capture with eight contact sheets
  inspected. Milestone02's unchanged core/output passed 12 plain/verify-full
  cases and 24 CLI health children on PG18.6. Affected-major/hosted acceptance
  passed at clean `54056c2`: all PostgreSQL 14-18 plain/verify-full routes,
  60 new fixture-backed cases within 1,180 selected invocations, pinned Rust
  1.97.1, all platform/MSRV jobs, real Linux socket and CodeQL. The macOS
  full-gate socket case is still a semantic skip. Native hand-use, incoming
  allocation and whole-process bounds remain separate from this closed slice.
- [x] T056 Private repository with CI on three platforms and three server versions
- [x] T057 Advertised keys that no operating system claims by default
- [x] T058 Visual system: glyph tiers, icons, capsules, striping, motion

## Dependencies

Phase 1 and 2 block everything. Within Phase 3, T015 to T020 are independent of
each other and all block T026. User stories 2, 3 and 4 depend only on Phase 3 and
are independent of one another.

## Status

The original platform acceptance items T051, T052 and T055a remain open for
hand evidence or the Windows ConPTY harness. The W07 follow-ups above track
additional lifecycle findings without relabelling that platform acceptance.
Current assignments and evidence are carried in `docs/status.md`.

W07-062 evidence on 2026-10-25: the original mouse-enabled panic case failed
because cleanup reached the prior hook before disabling mouse capture. The
repair passed 13 ordinary units, two PTY drivers and one inert parent helper;
the drivers exercised 16 actual owned macOS PTY children, all eight acquired
mouse/paste/screen combinations for error return and caught panic. Actual
canonical/echo/signal/raw state and output ordering were observed before the
previous hook; the original payload survived. Logs/manifest/patch:
`/private/tmp/ignatius-w07-062-w2iPKH/`. The lead reviewed/applied the two-file
patch; focused repeats and the combined full gate passed, recorded in
verification. This proves guard/hook behavior, not a full uncaught product
crash, forced OS-device failure, native visual acceptance or Windows behavior.

W07-063 evidence on 2026-10-25: eight standalone Rust main processes outside
libtest/catch_unwind linked the production guard/hook and all exited 101 after
restoring acquired modes before previous/default panic output. Actual raw,
canonical, echo and signal flags were checked. Immediate Darwin flags differed
only by PENDIN; normal same-PTY line input cleared it and exact full flags then
matched, without reset. Root inspected helpers and all transcript digests.
Evidence: `/private/tmp/ignatius-w07-063-u329k7er/`; manifest SHA-256
`883b1467c09cdac27e1e7f95659c96d1254a3f1c62e96c246838985e94bfba4b`.
Application source equals clean `d8a3ac3`; build base `74eb155` had only the
recorded planning plan/tasks diff. This is library-main uncaught-unwind proof,
not a full injected product crash, abort, native hand use or Windows.

W10-007 evidence on 2026-10-25: clean `24db9c5`, optimized local release,
Homebrew Rust 1.99.1, macOS 27.0.1 arm64, Apple M2 Max/32 GiB and PostgreSQL
18.6. Twenty-seven measured CLI children (three per workload) and 28
authenticated version/count checks completed without fixture changes or stop
thresholds. Maximum observed RSS was 44,744,704 bytes, including a complete
8,388,609-byte CSV export. The lead reviewed the helpers, manifest, all six
artifact digests and removal of only the three owned export payloads.
Evidence: `/private/tmp/ignatius-w10-007-AFKZrb/evidence/`; detailed results and
limits are in verification. These observed peaks establish no strict process,
frame, notice or aggregate-result ceiling. W10-008 remains open.

W10-009 integration progress: the three-file patch was reviewed/applied, after
nine expected baseline failures and 87 private focused passes. Root then passed
33 CLI units plus 14 live contracts on each plain and verified-TLS PostgreSQL
18.6 route (61 integrated focused passes, zero skips). Source was modified
`871db73`; evidence and patch identities are in verification. The combined
full gate then passed after W10-010 and help integration: 1,315 harness cases/
1,027 library on macOS 27.0.1 arm64/Homebrew Rust 1.99.1, PostgreSQL 18.6
plain/TLS; the socket boundary remains a semantic skip. Final log:
`/private/tmp/ignatius-verify-2026-10-25-help-and-export-final.log`.
That additional export
row/flush accounting repair is specified in Feature 021 FR-2112/T023-T025;
notice and aggregate-memory contracts remain separate W10-008 work.

W10-011 evidence on 2026-10-25: exact clean `d8a3ac3`, optimized production
library/helper, macOS 27.0.1 arm64/Homebrew Rust 1.99.1. Twelve fresh export
children and 24 before/after PostgreSQL 18.6/orders=3 checks completed once,
without retries/stops/skips. All complete bytes/digests/four row counters
matched; only the 12 verified owned CSVs (100,663,302 bytes) were removed.
Paced small/large medians were 8.031/8.109 s and 6.797/40.812 MiB peak RSS.
Root inspected sources before execution and verified all 38 final artifact
hashes, raw derived statistics, expected digests and empty outputs afterwards.
Evidence: `/private/tmp/ignatius-w10-011-d8a3ac3-JVcSVr/`; report SHA-256
`e6f760dacb2c42e12fa6b67ca74eef9be2f43523f9b0665f4f4711f252930b51`.
This is real library-writer backpressure above BufWriter with complete data,
not top-level CLI cancellation, disk bandwidth, TLS or a process-memory ceiling.

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

Fresh partial T052 evidence on 2026-10-25: clean security source `f99fe75`
passed the actual PostgreSQL 18.6 socket contract and all three existing normal-
quit PTY restoration contracts in a Debian 13.6 Linux arm64 container using
Rust 1.97.1, with zero skips. A separate actual Rust 1.90.0 locked/offline
workspace/all-targets/all-features check also passed. Exact source, build,
fixture and log identities are in
[verification](../../docs/operations/verification.md#frozen-postgresql-and-linux-acceptance---2026-10-25).
T052 remains open for Linux full-screen hand use, and T055a remains open for
native Windows ConPTY. This does not verify subsequent UI changes.

W07-059 evidence on 2026-10-25: a real synthetic child-environment probe first
failed with inherited logging and all 18 supported PostgreSQL environment
routes. After isolation, that probe and all three actual macOS normal-quit PTY
contracts passed (four total, zero skips). Root working tree was modified;
Homebrew Rust 1.99.1 and the existing SSD cache were used. The unique temporary
configuration/data roots outlive the child, and an explicitly absent private
PGPASSFILE blocks personal fallback. The regression changes only child-command
environment, never process-global state or a real profile. Red/green logs:
`/private/tmp/ignatius-w07-059-20261025-emhifp/`. The full integrated repeat is
recorded separately after ongoing pane work. Panic/signal/raw-mode and Windows
coverage are not inferred from this test.

W07-060 evidence on 2026-10-25: an isolated self-exec regression first died by
SIGTERM before its current-thread runtime was ever polled. The repair passed
21 interactive sidecar tests, four PTY contracts and warnings-denied Clippy in
its isolated modified clone. The lead reviewed/applied the two-file patch;
the integrated locked verifier then passed 1,208 harness cases/22 summaries,
including PostgreSQL 18.6 plain/TLS, with the macOS socket case still a semantic
skip. Log: `/private/tmp/ignatius-verify-2026-10-25-signal-startup.log`.

The actual integrated binary reported `4a7717b` modified, Homebrew Rust 1.99.1
and aarch64-apple-darwin. A separate owned-PTY SIGTERM run after first paint
exited zero, restored cursor/paste/alternate-screen order and canonical/echo/
signal modes. Full stty matched after a normal recovery-line read cleared
Darwin PENDIN state; no reset or forced cleanup was used. Evidence:
`/private/tmp/ignatius-rel001-integrated-20261025-w3FUso/`. The production
pre-registration ordering is proven by the deterministic pre-poll child and
failure callback; the PTY repeat proves steady-state restoration. Native
Windows, panic/error process paths and a latest-source Linux/MSRV repeat remain
separate open boundaries.

W07-061 evidence on 2026-10-25: all six Unix terminal harness checks passed
on macOS arm64 with Homebrew Rust 1.99.1 at the modified integration tree.
The three actual PTY normal-quit/drawing/order checks and profile-isolation
probe remain intact. An owned sleeping child was terminated and reaped at its
50 ms deadline; a normal child completed with exactly 130,000 stdout bytes and
the expected stderr, above ordinary pipe capacity. The parent captures output
in private temporary files, polls its owned child and joins its input writer;
no detached delayed numeric-PID signal survives a completed test. Native peer
review found no blocker for the fixed test commands. Log:
`/private/tmp/ignatius-w07-owned-deadline-2026-10-25.log`. Output is bounded by
the deadline, not a hard byte quota. This is harness/normal-quit proof, not
panic/error process, Windows, native visual or arbitrary process-tree proof.
