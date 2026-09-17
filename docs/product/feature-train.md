# The feature train

**Written 2026-09-18 at the owner's direction.** This document is the
product plan beyond feature 025: a sequenced train of chartered features
for future OpenCode agent sessions to specify and implement one at a time.
It complements the [roadmap](roadmap.md) and the
[delivery plan](agent-delivery-plan.md); it does not replace them, and the
W01-W16 verification packages keep their own authority and their own open
state.

Two rules govern how this document is used:

- **A charter is not a specification.** Each feature below gets its full
  Spec Kit artifact set (spec, clarify, plan, contracts, tasks) in the
  session that starts it, exactly as feature 025 did, and not before. The
  constitution's thin-vertical-slice rule and the delivery plan's "new
  behaviour requires its own spec, plan and tasks before code" both forbid
  batch-authoring specifications for work that is quarters away; a plan
  written today against source that 025's extraction is actively reshaping
  would be stale before it was read.
- **Numbers are assigned by the allocator, never by this document.**
  `.specify/scripts/bash/create-new-feature.sh` claims the next number
  when a feature starts, and `.specify/feature.json` is another worker's
  active selector until 025 completes. Charters here are named by slug
  candidate only. In particular, 025's own task list allows its quiet
  chrome and palette preview stories to detach into a follow-up feature;
  if that happens, that spin-out boards the train first and takes the
  next number.

## The product thesis, restated for planning

Ignatius wins when one person, under real pressure, reaches for it instead
of splitting their trust between `psql` and something prettier. The owner's
own working context defines the bar: an infrastructure architect running
Azure Database for PostgreSQL and on-premises estates, who values loud
production markers and quiet defaults, evidence over confidence, secrets
that never surface, keyboard-first speed with a real pointer, and chrome
that is earned rather than ambient. Every charter below is tested against
that person and the personas and outcomes in [acceptance](acceptance.md):
the infrastructure engineer with production access (O1, O4), the
SQL-literate newcomer (O2), the developer or analyst extracting data
deliberately (O3, O5), and the operator who has to verify and support the
build (O6).

What this product will not become is as binding as what it will: no second
database engine, no telemetry or upload of any kind, no ER diagrams that
cannot survive an 80-column terminal, no feature that quietly executes SQL
on the user's behalf. Depth on PostgreSQL is the product.

## The train

Order within a phase is the recommended boarding order; phases gate on the
milestones and external blockers named in Sequencing below.

### Phase A - the daily driver (after 025)

#### Charter: gui-grade-spinouts (conditional)

- **Problem**: 025's analysis gate may detach the quiet-chrome and
  palette-preview stories (US8, US9) to keep 025 shippable.
- **Boards only if** that detachment happens; the charter is the 025 spec
  text for those stories, unchanged, plus their existing contracts.
- **Single-writer impact**: none (that is why they detach cleanly).
- **Evidence gates**: the 025 parity and meaning tests extended to the
  detached surfaces.

#### Charter: copy-bulk-data

- **Problem and observed need**: the client exports retained rows well,
  but it cannot load data at all, and it cannot export beyond the retained
  cap without a scripted `query`. `COPY` is the reason people keep `psql`
  installed. Named assumption recorded per W16 discipline: the owner's
  migration and seeding jobs are the observed need; sample size one.
- **Scope in**: client-side `COPY` in both directions (the `\copy`
  analogue): file to table and table or query to file, format options
  (csv, text, binary pass-through), header handling, delimiter and NULL
  options, progress reporting with honest byte and row counts,
  cancellation that states what was and was not written, and the existing
  atomic `.partial` discipline extended to imports (staging table or
  explicit non-atomic warning; decided at specification time).
- **Scope out**: server-side file access (`COPY ... TO '/path'` runs only
  with an explicit server-path confirmation), scheduling, transformation.
- **Constitutional constraints**: an import is a write and gets the full
  production guard; progress is time and volume, never a guessed
  percentage of an unknown total; a partial import is reported as partial.
- **Personas and outcomes**: O3, O5; journeys J3, J7.
- **Dependencies**: none beyond 025's merge. **Single-writer impact**:
  effects and executor (message.rs, update.rs, interactive.rs) for the
  streaming lifecycle; grid untouched.
- **Evidence gates**: live round-trip against the disposable fixtures at
  multiple sizes, interrupted-import recovery, and a cross-check that
  `psql \copy` accepts the exported files.

#### Charter: transaction-workbench

- **Problem and observed need**: the client reports server transaction
  state truthfully but offers no first-class way to work a transaction:
  begin, savepoint, roll back to a point, or understand a failed state
  beyond the marker. The owner's reviewed-write habit (023) stops at
  single statements.
- **Scope in**: explicit begin, commit, rollback and savepoint actions
  with state-aware availability and footer hints; a visible transaction
  timeline (statements since begin, from local session history); failed
  transaction recovery guidance naming rollback as the way out; DDL and
  lock-wait awareness in wording; production confirmation extended to
  commit when the transaction contains writes.
- **Scope out**: autocommit-off as a default (explicit opt-in only),
  prepared transactions, any automatic rollback or retry.
- **Constitutional constraints**: the server's reported state remains the
  only truth; nothing is replayed; the timeline is local bookkeeping and
  says so.
- **Personas and outcomes**: O1, O4; journeys J4, J5, J6.
- **Dependencies**: none. **Single-writer impact**: reducer and model for
  transaction intent state; keymap and palette additions follow the 025
  same-change keymap rule.
- **Evidence gates**: live tests across begin, savepoint, failed-state,
  rollback and commit paths, including the concurrency wording agreed in
  W03.

#### Charter: session-observatory

- **Problem and observed need**: "point at production" ends today at the
  query surface. The person responsible for the database needs to see what
  the server is doing: activity, locks, blocking chains, and their own
  session's footprint. This is the operator journey (J4) with no current
  surface.
- **Scope in**: a read-only activity view over `pg_stat_activity` with
  deliberate refresh (the 024 refresh vocabulary, never a timer by
  default), lock and blocking-chain presentation in words, filtering by
  database, user and state, and cancel-own-query wired to the existing
  cancellation truthfulness.
- **Scope in, guarded**: `pg_cancel_backend` and `pg_terminate_backend`
  against other sessions. Terminate is this product's first action that is
  destructive to someone else's work: it takes the production-grade
  confirmation regardless of environment classification, names the target
  session's user, application and query start time before asking, and the
  specification must model the `pg_signal_backend` role reality (the
  server refuses without it; the client never pretends the guard is its
  own permission system).
- **Scope out**: any automatic polling default, historical retention,
  cross-server aggregation.
- **Personas and outcomes**: O1, O4; journey J4.
- **Dependencies**: benefits from 025's threshold-gated chrome; not
  blocked by it. **Single-writer impact**: a new pane or palette purpose;
  model and reducer growth routed through the integrator.
- **Evidence gates**: live blocking-chain fixture (two sessions, one
  lock), refused terminate without the role, and honest wording when the
  server refuses.

#### Charter: explain-analyze-depth

- **Problem and observed need**: feature 016 reads a plan; it cannot yet
  compare one, and `EXPLAIN ANALYZE` on a write is a foot-gun the product
  already guards with the analyzed-plan confirmation flow recorded in the
  W02 reducer work. The owner's tuning sessions need the next layer.
- **Scope in**: `EXPLAIN (ANALYZE, BUFFERS)` behind the existing
  analyzed-plan confirmation (extended, not reinvented), buffer and
  timing presentation in the existing tree, a local plan history for the
  session, and side-by-side comparison of two retained plans with the
  changed nodes named in words.
- **Scope out**: plan persistence across sessions, auto-explain server
  configuration, optimizer hinting.
- **Constitutional constraints**: analyze executes the statement and the
  interface never lets that surprise anyone; comparison is local and
  value-free in history and diagnostics.
- **Personas and outcomes**: O3; journey J4.
- **Dependencies**: none. **Single-writer impact**: plan model state;
  moderate reducer growth.
- **Evidence gates**: live analyze on reads, refused-then-confirmed
  analyze on writes inside an explicit transaction with rollback, and a
  comparison fixture with a forced plan change (an index created between
  runs).

### Phase B - depth and discovery

#### Charter: schema-intelligence

- **Problem**: the object tree names things; it does not yet answer the
  questions a person asks next: how big, how used, what depends on this
  index, which constraints and defaults apply, what the sequence is at.
- **Scope in**: table and index size and usage statistics from the
  standard catalogs, presented with the existing advisory honesty (stats
  are the server's counters, staleness stated); constraint, default,
  sequence, trigger and extension detail in the definition surface;
  column-level statistics where the catalogs offer them.
- **Scope out**: bloat estimation heuristics presented as facts, missing
  index recommendations, any DDL generation beyond the existing
  description surfaces.
- **Personas and outcomes**: O2, O3; journeys J1, J4.
- **Dependencies**: none. **Single-writer impact**: minimal; extends tree
  and definition surfaces.
- **Evidence gates**: live catalog checks across the supported PostgreSQL
  matrix, since the statistics views drift between major versions and
  `docs/support/compatibility.md` must say which versions carry which
  columns.

#### Charter: worksheets

- **Problem and named assumption**: real investigation is a sequence of
  statements whose results want to be read together. Today one result
  replaces another. The assumption to validate at specification time: the
  owner's investigation flow (several probes, compare, refine) is common
  enough to justify the model change. This is the largest-risk charter on
  the train and its clarify step must be taken seriously.
- **Scope in, candidate**: a worksheet mode where each executed statement
  retains its result in a scrollable session transcript, with the
  existing grid tools available per result; worksheets are ordinary `.sql`
  files plus never-persisted local result state.
- **Scope out, firm**: persisted results (constitution III), notebook
  file formats, remote sharing, markdown cells.
- **Personas and outcomes**: O2, O3; journeys J2, J3.
- **Dependencies**: 025's widget extraction makes the render feasible;
  memory bounds need W10's resource work. **Single-writer impact**: high;
  the retained-result model becomes plural. Specify only when a session
  can afford the integrator focus.
- **Evidence gates**: the W10 resource bounds extended to plural results;
  identity rules (which result a refresh or cell action targets) proven in
  the reducer suite.

#### Charter: search-everywhere

- **Problem**: discovery exists per surface (palette purposes, tree
  filter, history search). The person who does not know where a thing
  lives needs one entry point.
- **Scope in**: a unified palette purpose that searches actions, objects,
  columns, saved queries and history together, grouped by kind, using the
  existing safe summaries and the 025 preview pane; per-kind scoping
  prefixes.
- **Scope out**: content search inside result sets (the grid filter owns
  that), fuzzy ranking claims beyond the existing ordering rules.
- **Personas and outcomes**: O2; journey J1.
- **Dependencies**: 025 palette geometry. **Single-writer impact**: low.
- **Evidence gates**: reachability and meaning tests in the existing
  discovery suite; no execution effects from search.

### Phase C - reach and release

#### Charter: packaging-distribution

- **Problem**: the release train (W11-W14) proves a candidate; nobody can
  yet install one with their package manager. Homebrew, winget, and a
  Debian package are the owner's platforms' native routes.
- **Scope in**: packaging definitions and their drift checks, shell
  completions and man pages shipped in the packages, and the documented
  rollback route per manager; all gated behind W14's owner release
  decision, publishing nothing by itself.
- **Scope out**: auto-update (constitution III), a version-check network
  call by default, curl-pipe-sh installers.
- **Personas and outcomes**: O6; journey J8.
- **Dependencies**: W11-W13 complete; W14 owner gate. **Single-writer
  impact**: CI and release contracts only.
- **Evidence gates**: install, run, `doctor`, and uninstall proven per
  manager on the target platforms, recorded per the release evidence
  rules.

#### Charter: terminal-craft

- **Problem**: the delight backlog. Small, bounded refinements that each
  research pass has named and deferred, kept together so they ride one
  session instead of leaking into every other feature's scope.
- **Scope in, current backlog**: scroll-margin centering for the grid
  cursor; a picker-in-picker for switching palette scope without closing
  it; OSC 8 hyperlinks where a URL is already shown as text; a small
  curated set of additional built-in themes under the existing contrast
  tests; the README's colored identity assets (a freeze-generated SVG
  still and a VHS tape of the real boot reveal, regenerable from committed
  sources) once the 025 banner exists to record.
- **Scope out**: anything requiring a new dependency without its own
  decision record; motion beyond the documented set.
- **Personas and outcomes**: O2; the daily-use half of the acceptance
  judgement.
- **Dependencies**: 025 complete. **Single-writer impact**: low.
- **Evidence gates**: the existing parity, meaning and reduced-motion
  suites extended to each item.

## Sequencing

- **Milestones**: Phase A serves M2 (proven daily experience) and must
  interleave with W08 and W09: hand verification in real terminals and
  the observed first-use and daily-use protocols are what turn these
  features from implemented into proven, and the acceptance document's
  warning stands - five participants support a bounded decision, not a
  superiority claim. Phase C's packaging charter belongs to M3/M4 and
  waits on the release train.
- **External blockers, unchanged and named**: live cloud-provider
  verification needs accounts (W06); native Windows verification needs a
  Windows host (W07); the trademark search and domain for the product
  name remain outstanding before any publication (W13).
- **Verification interleave**: after each Phase A feature merges, the
  next session runs the focused W05 matrix refresh for the surfaces it
  touched before boarding the next charter. A feature train that outruns
  its evidence recreates the exact risk the roadmap's risk table names.

## How a future session boards the train

1. Orient exactly as `AGENTS.md` says; read this document last so current
   state wins over the plan.
2. Confirm the previous feature is merged, its evidence recorded, and
   `.specify/feature.json` is not another session's live selector.
3. Pick the next charter (or the spin-out, if it exists). Confirm its
   observed-need note still holds with the owner; a charter is a
   hypothesis, and striking one is cheaper than building it.
4. Run the full Spec Kit cycle for that one feature: specify, clarify
   with the owner where material, plan with a walked constitution check,
   pinned-value contracts for anything a budget worker could guess,
   tasks decomposed for the orchestration rules with pre-filled
   assignment blocks, then analyze.
5. Implement per `docs/operations/agent-orchestration.md` and the
   playbook: worker caps, single-writer integrator lane, independent
   review on a different model family, serialized verification, evidence
   recorded before boxes are checked, quota stops as checkpoints.
6. Land through the pull-request gate under the git authority the owner
   granted on 2026-09-18 (branch, commit, push, PR, auto-merge behind
   green checks; tags and releases stay owner-only).
7. Update `docs/status.md`, this document's charter state, and the
   roadmap linkage in the same integrator change.

### Session prompt template

Paste into a fresh OpenCode session, filling the charter name:

```text
Read AGENTS.md, .specify/memory/constitution.md and docs/status.md, then
git log --oneline | head -20 and git status. Read
docs/product/feature-train.md and confirm with the owner that the
"<charter-name>" charter is the next boarding and its observed-need note
still holds. Confirm the previous feature's evidence is recorded and
.specify/feature.json is free.

Then run the full Spec Kit cycle for that one charter: specify (the
allocator assigns the number), clarify material ambiguity with the owner,
plan with a walked constitution check, pinned-value contracts for every
value a worker could guess wrong, and tasks decomposed per
docs/operations/agent-orchestration.md with pre-filled playbook
assignment blocks, the single-writer rules, and a checkable parallelism
predicate. Analyze, then implement under the standing orchestration and
review rules, and land through the pull-request gate under the owner's
2026-09-18 git authority. A missing pinned value means stop and ask. A
quota stop is a checkpoint. Record focused, full-suite, live and
unverified evidence separately before checking any box.
```

## Deferred integrations

This document lands while feature 025 is mid-implementation, so the
shared, integrator-owned files were deliberately not touched. The next
integrator after 025 merges should, in one change: add the feature-train
pointer to `docs/product/roadmap.md`'s planning section, record the
train's existence and the 2026-09-18 owner direction in `docs/status.md`,
map the Phase A charters into the delivery plan's package table, and
consider an architecture decision record for the 025 theming and painted
surface decisions, which currently live only in design documents and the
025 contracts.
