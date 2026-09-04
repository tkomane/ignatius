# Product acceptance

Planning targets set on **2026-09-04**. These operationalise the
[vision](vision.md) and [experience brief](experience-roadmap.md); they are not
measured results. Existing feature requirements and the constitution take
precedence. Record actual results in [status](../status.md), feature evidence
and the [compatibility matrix](../support/compatibility.md). These journeys
cover the intended integrated feature chain. Features observed only in the local
worktree remain unavailable on main until W01 integrates them with their specs.

## Outcomes that define the product

| ID | Outcome and beneficiary | Capability chain | Acceptance evidence | Delivery owners |
| --- | --- | --- | --- | --- |
| O1 | An infrastructure engineer can identify the target, protection and outcome before taking another action | Profiles/picker -> auth -> observed TLS/read-only facts -> production guard -> cancellation/recovery | No silent downgrade, replay, false completion or wrong-target action in the adversarial matrix; operator explains PROD, requested/observed TLS and transaction state correctly | W02, W03, W04, W05, W06 |
| O2 | A SQL-literate newcomer can work without memorising the client or schema | First frame -> object tree -> completion -> error at cursor -> local format/undo -> contextual help | First documented query within the existing 001 SC-001 two-minute target; at least four of five first-use participants complete the exploration journey within ten minutes without external docs or facilitator help | W08, W09 |
| O3 | A developer or analyst can inspect and extract the right data deliberately | Retained grid -> sort/filter/inspect -> explicit export/copy -> reviewed update -> explicit refresh | All scenarios retain source-row identity, distinguish NULL/empty/text NULL and state retained/returned/exported counts; no local view action emits SQL | W02, W03, W08, W09 |
| O4 | A user can recover from mistakes and interruptions without losing trust | Undo -> cancellation -> failed transaction remedy -> connection-loss wording -> atomic file recovery -> terminal restore | Existing 001 SC-002 cancellation target below ten seconds against the synthetic server; correct unknown outcomes, recoverable partial exports and usable shell after each supported exit path | W02, W05, W07, W10 |
| O5 | The same tool is dependable in scripts and private local workflows | CLI/stdout/stderr/exit codes -> plain mode -> parameter binding -> history controls -> safe diagnostics | Every documented exit code has a real producer; machine streams contain only their documented payloads; no credential, bound value, SQL or row data in prohibited persistence/diagnostic paths | W03, W04, W05, W08 |
| O6 | An operator can verify, install, recover and support the exact build | Source/build/target identity -> archive and trust evidence -> install -> rollback -> support route | All declared target rows have candidate-bound evidence, verified supply-chain records, a tested recovery procedure and an owner-reviewed release decision | W11, W12, W13, W14 |

The goal of being the best experience remains a hypothesis until people prefer
it in use. Passing these targets supports a bounded acceptance decision; five
participants do not establish market-wide superiority.

## Reference journeys

Use synthetic fixtures, an isolated configuration/data directory and a target
explicitly designated for testing. PROD labels may be tested on a disposable
server; do not use production data or privileges for these exercises.

| Journey | Steps | Required observation |
| --- | --- | --- |
| J1 First useful answer | Start with profiles, select the intended target, find an unfamiliar table, compose with completion, run and inspect a cell | User locates the actions without instruction, knows the connection state and understands the returned count |
| J2 Correct and reuse SQL | Introduce a syntax/constraint error, follow the location, edit, format, undo, use named parameters, save and reopen the template | No lost edit or stale caret, hidden prompt answers, one answer per name, template preserved in saved/history routes |
| J3 Investigate a wide result | Sort, filter, hide/resize columns, inspect a cell, expand a row, export a selected shape and optionally confirm copy | Selection always names the original row/column; local scope and counts remain visible; exported data has the declared scope |
| J4 Diagnose safely | Select a synthetic PROD profile, inspect trust facts, exercise a guarded write, request EXPLAIN then ANALYZE, cancel a long statement | User explains which operation executes SQL and which might have side effects; confirmation, refusal, cancellation and transaction wording match the server |
| J5 Review one change | On a writable synthetic non-production table, generate a cell UPDATE, cancel once, review again and confirm; explicitly refresh afterward | No write before final review, correct bound target, affected-row outcome, old snapshot identified, refresh prompts again and ignores later buffer edits |
| J6 Recover from disruption | Disconnect during execution; switch only from an eligible quiet state; interrupt an export; encounter invalid config; exit/resize/restart | No implicit retry, stale work, false success or terminal damage; partial files and repair instructions are accurate |
| J7 Automate and use plain mode | Run a saved query through CLI formats and a broken pipe; repeat the core query/error/cancel/export journey through `--plain` | Stream separation and exit codes hold; plain mode preserves meaningful text and never depends on the full-screen layout |
| J8 Install and recover | Independently verify a candidate, install beside a prior build, start, connect, trigger a controlled configuration failure and roll back | Exact identities and byte integrity, state preservation and usable prior binary/config pair; never claim a published-version upgrade when neither version was published |

Full-screen grid manipulation, the plan tree and OSC 52 are surface-specific.
Plain mode must support the underlying core jobs through documented text/SQL
and CLI routes. Do not advertise a TUI-only action as available in plain mode,
or claim screen-reader usability from the absence of escape bytes alone.

## Observation protocol

1. Recruit five SQL-literate people new to Ignatius when available, including
   infrastructure and application roles. The owner can provide an early pilot;
   one expert walkthrough does not replace the first-use sample.
2. Freeze the candidate source/build/target identity and the synthetic schema.
   Start each participant with a clean profile and normal defaults. Record OS,
   terminal version, shell, dimensions and presentation mode.
3. Give the task outcome, not key instructions. Record launch-to-first-result
   separately for the existing two-minute foundation target, with the local
   server already available and the documented setup steps. For the unaided
   J1 exploration journey, start timing at the first usable frame. Measure
   completion, elapsed time, help lookups, wrong actions,
   recoveries and whether assistance was needed. Stop after ten minutes for J1;
   an unfinished task is a failure, not missing data.
4. Ask the participant to explain the target, TLS, row scope and cancellation
   state. Record any materially wrong safety belief as a blocker regardless of
   their task time. Record short participant wording without SQL or data.
5. Repeat a familiar daily task with the participant's current client on an
   equivalent fixture, rotating the order to reduce learning effects. Record
   their preferred tool and reason. Do not claim a competitor comparison from
   imagined behaviour or a different task.
6. Run a follow-up daily-use pilot over five working sessions with the owner or
   a consenting tester. Record abandonment reasons and regressions locally.
   No telemetry or automatic uploads are introduced.
7. Convert observed obstacles into ranked, reproducible issues. Fix the
   blocking journey and repeat that scenario plus affected safety checks.
   Broader retesting follows only when the changed surface justifies it.

Store counts, timings, anonymised participant codes and safe observations in a
dated report. Screenshots and terminal captures are optional, synthetic-only
and manually reviewed. Do not collect identities, connection strings, real
queries, result values, clipboard contents or provider output for research.

## Performance and resource evidence

The foundation already requires first paint before network completion and no
database work blocking input/rendering. Test those conditions with a delayed
or unavailable server; fast localhost success alone cannot prove them.

The following are proposed engineering thresholds for W10 to baseline and put
into the relevant feature plan before adding a timing gate:

| Scenario | Proposed target | Measurement boundary |
| --- | --- | --- |
| Local input, palette and completion over an already loaded catalogue | p95 input-to-frame latency at or below 100 ms | At least 100 actions after warm-up, fixed synthetic data, target OS/terminal and release build recorded; exclude provider/database latency |
| Local grid interaction | Same 100 ms p95 target at the configured retained-row cap | Include sort/filter/resize combinations and wide Unicode cells; report catalogue/result dimensions |
| Slow database operation | Input, cancel intent and redraw remain responsive | Run a delayed query and metadata load; separate user-intent latency from server-confirmed cancellation |
| Streaming export growth | Additional rows do not create proportional retained memory growth | Compare 200,000 and 2,000,000 fixed-width synthetic rows; record peak RSS and byte/row counts, explain driver/OS variance |
| Malformed or large input | A documented bound/refusal or recoverable degraded state | Exercise formatter 1 MiB, plan 1 MiB/500 nodes, clipboard 1 MiB, 64 parameter names, update 16 KiB, configured result cap and one oversized value/notice |

The limits above come from the feature specifications inspected in the local
planning worktree; W01 must bring those contracts with their implementation. No whole-
process memory ceiling is currently established for an oversized single value
or notice. W10 must measure and specify that boundary instead of calling the
row cap a complete memory guarantee. Record hardware, toolchain, server,
sample count, median/p95 and raw safe measurements. A missed target prompts
profiling and a scoped plan; never loosen a safety invariant to hit it.

## Acceptance decision

A candidate must pass the constitutional correctness, privacy, terminal and
release gates for every claimed surface. Failed or missing evidence is listed
by platform, provider and source revision. Usability failures create work in
W09; performance/resource gaps create work in W10. A capability whose support
scope changes requires its specification and compatibility entry to change
together. The roadmap itself does not authorize reducing accepted scope.
