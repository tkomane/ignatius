# Agent delivery plan

**Planned 2026-09-04.** This is the assignment catalogue for the
[roadmap](roadmap.md). Read the [baseline](planning-baseline.md),
[architecture](../architecture/containers-and-components.md) and
[agent playbook](../operations/agent-playbook.md) first. Outcome IDs and journeys
refer to [product acceptance](acceptance.md).

Work-package IDs (`W01`-`W16`) are coordination identifiers, not feature
numbers. They group existing work and identify new specification work. Existing
`specs/<full-slug>/tasks.md` files remain authoritative for implementation
completion. New behaviour requires its own spec, plan and tasks before code.
The coordinator records package progress and the next handoff in `docs/status.md`.
Nothing below is already complete merely because it is planned.

## Scheduling and ownership

| Package | Priority | Accountable role | Prerequisites | Exit enables |
| --- | --- | --- | --- | --- |
| W01 Reconcile and integrate the current feature chain | P0 | Integration lead | Current worktree and owner of existing edits identified | A reproducible baseline for all acceptance work |
| W02 Preserve session and result identity across features | P0 | Runtime engineer | W01 baseline | O1/O3/O4 correctness |
| W03 Verify query, parameter and reviewed-write semantics | P0 | PostgreSQL engineer | W01 baseline; coordinate W02 shared files | O1/O3/O5 correctness |
| W04 Prove runtime privacy across every route | P0 | Security reviewer | W01 baseline; repeat after W02/W03/W06 changes | O1/O5 privacy; candidate privacy scenarios |
| W05 Refresh database and protocol evidence | P0 | PostgreSQL verifier | Stable W02/W03 changes | Candidate-source PostgreSQL evidence |
| W06 Make cloud authentication verifiable on each platform | P0 | Auth/platform engineer | W01; explicit controlled account access for live checks | Per-provider support facts |
| W07 Automate terminal restoration on all primary targets | P0 | Terminal engineer | W01; Windows-native host for ConPTY | Foundation T055a and lifecycle evidence |
| W08 Verify terminals, accessibility and core journeys by hand | P0 | Accessibility/terminal verifier | Stable relevant W02-W07 behaviour | Platform and plain-mode acceptance |
| W09 Prove the daily experience and remove observed friction | P1 | Product/UX engineer | W01 pilot baseline; final pass after W08 | Measured O2/O3 experience |
| W10 Bound resources and prove local-state recovery | P1 | Reliability engineer | W01 baseline; stable code before final measurement | Performance and recovery evidence |
| W11 Resolve the release trust mechanism | P0 before release | Release engineer, owner decides trust choices | ADR-0008 and Feature 008 contracts | Approved mechanism implemented with dry validation |
| W12 Assemble and verify the exact release candidate | P0 before release | Release engineer | W02-W11 accepted, W13 support prerequisites | Feature 008 T033 evidence |
| W13 Prepare support, naming and repository governance | P0 before public distribution | Maintainer | Current repository and intended distribution audience | Reviewable operational release decision |
| W14 Obtain the release decision and execute the authorized scope | Owner gate | Maintainer/release owner | W12 and W13 | Feature 008 T035 and any separately authorized distribution |
| W15 Maintain the supported release | After distribution | Maintainer | W14 for published-version work | Repeatable patch/recovery/support process |
| W16 Expand only where observed jobs justify it | Later | Product owner with feature engineer | W09 evidence and stable release baseline | Next specified vertical slice |

P0 denotes a correctness or release dependency, not permission to preempt a
different agent's files. W11 and W13 preparation can begin while W01 runs.
After W01, W06 and W07 can use isolated platform scopes while the integration
lead sequences W02/W03. W04 is independently reviewed, but its final evidence
must follow the last relevant implementation change. W08-W10 can gather early
feedback; their acceptance run must identify the final candidate they cover.

There is no calendar promise without assigned people, available platforms and
provider accounts. Estimate each scoped feature after the first failing
reproduction or baseline measurement. The critical path is integration ->
correctness/platform/experience evidence -> candidate trust -> owner decision.
An unavailable account blocks its provider row, not unrelated local work.

## W01 - Reconcile and integrate the current feature chain

**Result:** one reproducible development baseline that contains the intended
experience and has an honest evidence record. Owns integration and status;
does not authorize discarding pre-existing work or publishing it.

1. Recheck branch, HEAD, remote main, modified/untracked paths and active
   `.specify/feature.json`. Preserve existing edits. Identify the owner of
   shared implementation files before any consolidation.
2. Recheck `024-retained-result-refresh/tasks.md` T001-T021 and the recorded
   2026-09-04 completion evidence. They completed during the planning pass;
   do not restart the implementation. Carry forward only evidence matching
   the source under review and retain terminal/socket gaps. Re-run affected
   focused checks when integration changes the covered behaviour.
3. Review the combined dependency chain: 012 -> 013, 012/004 -> 014,
   014/015 -> 018/021, 005/013 -> 019, 009/011/017 -> 020,
   005/006 -> 022, 014/022 -> 023, and retained source/022/023 -> 024.
   These are integration relationships, not claims that all files can be split
   into independent commits without inspection.
4. Prepare reviewable, coherent commits or patches by behaviour, preserving
   every side of a shared-file change. Reconcile feature headers, checkboxes,
   changelog, compatibility and key/documentation contracts. Do not reset the
   worktree to make a branch look tidy.
5. Record `cargo --locked xtask verify` against the integrated source and
   skips separately. Reuse the recorded run only if its implementation inputs
   remain unchanged; otherwise run it with disposable PostgreSQL. Obtain fresh
   candidate-source hosted evidence only when
   push/workflow authority exists. Historical main CI does not close this step.

**Handoff:** exact source/worktree identity, scoped diff, feature task links,
focused/full/live results, remaining gates and next work package. **Exit:**
the inherited implementation status is reconciled; all included behaviours have
traceability; the next engineer can reproduce the same baseline. Hosted
integration remains explicitly pending if publication of source is not authorized.

## W02 - Preserve session and result identity across features

**Outcomes:** O1, O3, O4. **Primary files:** `src/app/model.rs`,
`src/app/message.rs`, `src/app/update.rs`, `src/cli/interactive.rs`, and the pure
grid/completion/plan modules. The integration lead serializes these shared files.

Use Features 012, 014, 016, 020, 023 and 024 as the requirements. Build a
cross-feature transition matrix for: query in flight; cancellation requested;
connection loss; quiet connection switch; old metadata/plan completions;
sorted/filtered rows; pending copy/update/parameter prompts; refresh followed
by a new editor buffer. Reproduce failures through messages and effects, then
through the runtime boundary where scheduling is material.

Assert source job, row and column identity rather than current view position.
An old connection generation must not restore old facts. A pending confirmation
must be invalidated when its source changes. Local sort, filter, inspection,
formatting, palette discovery and plan navigation must not execute SQL.
Refresh is one explicit new request; it neither changes the displayed source
to the edited buffer nor reuses prior parameter answers.

**Exit:** meaningful interleaving tests and applicable runtime evidence cover
the matrix, all affected existing contracts pass, and each discovered behaviour
change has a scoped spec/task update. Do not rewrite the state architecture
solely to make its types more uniform.

## W03 - Verify query, parameter and reviewed-write semantics

**Outcomes:** O1, O3, O5. **Primary files:** `src/query/parameters.rs`,
`src/query/update.rs`, `src/query/statements.rs`, `src/postgres/session.rs`,
`src/postgres/metadata.rs`, CLI/plain adapters and PostgreSQL contracts.

Follow Features 001, 016, 022-024. Prove protected SQL regions, quoting,
repeated names, empty values, NUL refusal, error-position mapping after binding,
transaction state and no secret values in history. The current binding route
is escaped text at the simple-query boundary; do not describe it as extended-
protocol prepared statements.

For generated UPDATE, prove live relation and ordered primary-key metadata,
privilege and production/read-only refusals, exact reviewed statement, one
execution, and an accurate zero/one/multiple-row outcome. Exercise row changes
or deletion between review and send, composite/quoted keys, views, joins,
ambiguous projection and stale selection. Document the accepted concurrency
semantics: review does not lock a row or prove the old value is unchanged.
If optimistic conflict detection is required, specify it as new behaviour
before adding predicates or changing transactions.

For EXPLAIN, ANALYZE and refresh, prove the effect boundary and state why
classification is advisory: a read-looking function may have side effects.
Verify the `standard_conforming_strings` assumption and its existing refusal
or compatibility boundary; never silently bind under incompatible semantics.
No auto-rollback, write retry or implicit SELECT replay may be introduced.

**Exit:** focused adversarial tests plus live synthetic PostgreSQL evidence
for changed semantics, with the locked verifier and compatibility documentation
updated. A schema/key/privilege test alone does not prove review-to-execution
identity or concurrent-update behaviour.

## W04 - Prove runtime privacy across every route

**Outcomes:** O1, O5. **Primary files:** existing redaction/logging, history,
prompt and diagnostic modules; focused subprocess tests; security docs.

Audit raw secrets and SQL through success, failure, cancel, timeout and panic
paths, including provider stdout/stderr, parameter expansion, generated UPDATE
review, refresh and clipboard confirmation. Use unique synthetic sentinels and
isolated files. Inspect emitted stderr, debug formatting, configured logs,
history and support identity output; distinguish intentional query/result output
and the explicit review surface from prohibited disclosure.

Keep one redaction implementation. Verify dependency logging remains excluded
even at supported debug levels; full SQL leaked through a historical hosted
artifact, so local unit tests alone cannot close the release privacy gate.
Prove provider values are absent from child arguments/environment, and that
plain prompts do not echo values. Treat best-effort zeroing as best effort.

**Exit:** source audit findings resolved or explicitly blocking, focused
privacy tests and safe local runtime evidence recorded. W12 must repeat the
privacy scenarios against each exact packaged candidate, replacing the failed
historical artifact's evidence rather than relabelling it.

## W05 - Refresh database and protocol evidence

**Outcomes:** O1, O4, O5. **Primary files:** PostgreSQL tests, disposable fixture
configuration, CI and compatibility evidence. Coordinate workflow changes with W12.

Run the current source against the repository's PostgreSQL 14-18 matrix with
plain and TLS fixtures, plus the real Linux socket fixture. Cover type/NULL
rendering, hostile identifiers, multistatement errors, SQLSTATE, notices,
cancellation, failed transactions, read-only enforcement, TLS identity failures,
completion catalogue permissions and the object session's read-only fallback.
Include the new parameter/update/refresh contracts where applicable.

Record actual server versions and fixture identities. Keep native macOS and
Windows database runtime evidence distinct from Linux CI; W08 supplies those
target journeys. A harness returning early because a URI is absent is a skip.
Follow the supported window in compatibility; upstream-window changes are
reviewed policy changes, not automatic removal of old coverage.

**Exit:** candidate-source matrix jobs and real socket query evidence, with
every missing combination named. The success of `52872ac` is the starting
baseline, not the acceptance result for later code.

## W06 - Make cloud authentication verifiable on each platform

**Outcomes:** O1, O5. **Primary files:** `src/connection/cloud.rs`, platform
adapter, cloud identity tests, Feature 011/017 specs and compatibility rows.

First reproduce Windows-native launch behaviour for installed Azure, Google
Cloud and AWS tools. `az`/`gcloud` script-launcher concerns are recorded
suspicions, not established defects. Add synthetic portable subprocess
coverage for discovery, timeout, non-zero exit, malformed output and redaction.
Verify current vendor CLI contracts from official documentation before choosing
a repair. Do not silently add a general shell or interpolate credentials into
commands to make one platform pass. A shell-boundary exception needs an
explicit spec/threat-model decision; the closed libpq migration is irrelevant.

Then, with explicitly authorized controlled accounts and databases, run Entra,
AWS and GCP separately. Capture safe tool/version, OS, provider, server version,
observed TLS, authenticated role, read query outcome and remedy facts. Confirm
unencrypted targets refuse before acquisition, failed providers do not fall
back to password prompts, and the trust view does not acquire/refresh tokens.
Do not create cloud resources, grants, charges or production changes under a
read-only verification assignment. An absent account stays a dated evidence gap.

**Exit:** documented native launch policy, automated subprocess evidence and
one truthful support row per provider/platform tested. A synthetic TLS token
proves the mechanism, never a vendor integration by itself.

## W07 - Automate terminal restoration on all primary targets

**Outcomes:** O4, O6. **Primary files:** `tests/terminal_restore.rs`, existing
terminal tests/helpers, `src/ui/terminal.rs`, CI; verify actual filenames before
editing. Owns 001 T055a and related REL-001 evidence.

Add a Windows-native ConPTY acceptance harness with a narrowly scoped test
boundary. Exercise normal quit, startup failure after acquisition, cancellation,
supported signal/control-event handling and panic restoration where supported.
Extend Unix process evidence for uncovered exit paths using the same assertions:
cursor restored, raw/input modes restored, paste/mouse modes undone and alternate
screen left in the correct order. Preserve crate-wide unsafe denial; a test
helper is not permission to introduce production FFI.

**Exit:** executable target-specific tests, failure output that identifies the
missing restoration step, and live native Windows plus Unix process results.
W08 still verifies real terminal renderers and human shell usability. A Windows
compile or startup-refusal test does not satisfy T055a.

## W08 - Verify terminals, accessibility and core journeys by hand

**Outcomes:** O1-O6. **Primary artifacts:** terminal compatibility, keymap,
plain-mode evidence, product journey records and related feature task lists.

Execute the applicable [acceptance journeys](acceptance.md) in macOS/Warp,
Windows 11/Windows Terminal with PowerShell, Linux, and WSL in Windows Terminal.
Native Windows and WSL are separate rows. Record versions, build identity and
server version. Include a real database query and verified TLS on the primary
native targets, not only opening the screen.

Cover 100x30, 80x24, 60x20 and 30x6 meaning, live resize, dark/light/high-contrast,
ASCII/no-colour, reduced motion, configurable keys, keyboard-only operation and
mouse-disabled operation. Check whether advertised modified keys reach the
program and that the palette/chord alternatives remain discoverable.

Use VoiceOver and NVDA with `--plain` for connect/query/error/cancel/export and
recovery. Verify focus/order/wording with a person, not only a byte scan. Test
opt-in OSC 52 with a synthetic cell on terminal, WSL, SSH and a representative
multiplexer path; record actual acceptance or refusal without clipboard reads.

**Exit:** scenario-level observations and resolved blockers, with 001
T051/T052 and plain-mode T032-T034 updated only where their exact scope is met.
Shared scenario evidence may support several features, but each affected
feature must link to it. Missing access leaves that row open.

## W09 - Prove the daily experience and remove observed friction

**Outcomes:** O2, O3. **Primary artifacts:** acceptance observations, journeys,
discovery/keymap/interaction specs; UI code only through a specified fix.

Run the first-use and daily-use protocol in `acceptance.md`. Prioritize wrong
safety beliefs, blocked tasks, lost edits, unfindable actions and unnecessary
repetition before decorative polish. Trace each observation to a journey and
feature. Propose the smallest complete vertical slice that removes the cause,
including empty/refusal/error and accessible states.

Compare a changed interaction on the same fixture and terminal with its
baseline; retain useful branding, visual meaning and calm feedback. New motion
must respect reduced-motion and plain/ASCII modes. Do not add onboarding state,
telemetry or extra confirmations without a specified user benefit.

**Exit:** measured acceptance targets met or explicitly failed, daily-use
abandonment reasons addressed, and repeat evidence for changed journeys. Record
the actual sample size; an agent's opinion is not user research.

## W10 - Bound resources and prove local-state recovery

**Outcomes:** O3-O5. **Primary files:** result/export/format/plan code,
configuration/history/query stores and targeted reliability tests.

Baseline the measurement scenarios in `acceptance.md`. Inspect the largest
single value, notice, metadata snapshot and provider output as well as row
counts; one capped collection does not prove bounded process memory. Specify
any missing byte/time bounds, refusal wording and cancel/recovery path before
implementation. Profile measured bottlenecks before choosing dependencies or
moving code between modules.

Exercise invalid configuration/schema versions, unknown settings, interrupted
atomic writes, full/unwritable destinations, filename collisions, path traversal,
symlink boundaries, history trimming/clear and saved-query recovery. Confirm
an export's visible scope, partial status and counts after interruption.
Measure cancellation and responsiveness while metadata/query/export work runs.

**Exit:** reproducible measurements, documented resource limits and residual
risks, implemented scoped fixes where needed, and targeted failure/recovery
evidence. Never use destructive cleanup of the user's files as a test fixture.

## W11 - Resolve the release trust mechanism

**Outcome:** O6. **Primary artifacts:** ADR-0008, Feature 008 plan/tasks,
`release-evidence/` contracts, release workflow design.

Prepare the decision package before asking the owner: inventory format and
generator with lockfile/input binding; signing mechanism and verification
identity; custody, expiry, rotation/revocation and outage behaviour; provenance
format, builder identity and independent verifier; retention location and
privacy allowlist. Compare viable choices using current primary documentation,
required permissions, operational burden and costs if any. Do not invent a
provider choice or treat checksums as signatures.

Separate non-publishing build/readiness permissions from any future signing or
distribution job. Define the minimum rights and review exact workflow changes
before requesting credentials or owner authority. Keep the current read-only
workflow contract until an explicit decision changes it.

**Exit:** concrete owner-reviewed trust decision recorded in ADR-0008 and a
scoped implementation/task plan, followed by implementation of the approved
inventory generator, trust adapters and independent verification commands.
Test missing/expired/mismatched evidence and provider outages with dry fixtures;
keep live credentials outside source/configuration and keep live actions gated.
Unconfigured choices continue to block readiness. Research and dry validation
can proceed without acquiring keys or enabling a live release route. Exact
candidate signatures and provenance are collected under W12's authority.

## W12 - Assemble and verify the exact release candidate

**Outcome:** O6. **Authority:** Feature 008 T033; release operations guide,
schemas and the existing `xtask release` command surface.

Freeze an approved source revision and product version from their authorities.
Follow the canonical release quickstart for target builds, repeat-byte
comparison, manifest/checksum verification, inventory and independently verified
signature/provenance binding. Materialise only allowlisted evidence under the
canonical path; reject mixed attempts, stale targets, missing files and invalid
or expired trust evidence. Tagging/signing/hosted dispatch require their own
existing owner authority; preparing the inputs does not supply it.

Run J8 and the W04 privacy probes on each exact archive for macOS Apple silicon,
Windows x86_64 and Linux x86_64 GNU targets declared by the release workflow.
Do not label an older musl handoff binary as the current GNU candidate. Record
install, identity, real query/TLS, invalid config, preservation and rollback
facts per target. Bind W05/W08-W10 evidence to that source, rerunning affected
scenarios after changes. Keep historical failed artifacts and newer corrective
evidence distinguishable.

**Exit:** `cargo --locked xtask release validate <record-path>` validates the
record contract and independent checks prove its technical evidence. Run
`cargo --locked xtask release check <record-path>` separately: it enforces
readiness, including the controlled owner-authorization reference. An outstanding
owner gate remains an explicit blocker for W14, and T033 remains open until
its complete scope is met. A deliberately blocked rehearsal proves enforcement,
not readiness. Do not require an already-authorized release before presenting
the concrete candidate for the final owner decision.

## W13 - Prepare support, naming and repository governance

**Outcome:** O6. **Primary artifacts:** `SECURITY.md`, contribution/support and
release documentation, landscape naming record, repository governance proposal.

Prepare a private vulnerability-reporting route, a privacy-safe bug template
using the existing identity/doctor fields, triage severity and escalation
ownership, and supported platform/server claims. Make install, uninstall,
rollback, history removal and configuration repair discoverable. Reconcile
README, changelog and release catalogue with the included features.

Before public distribution, obtain the outstanding naming/trademark and domain
decision using appropriately qualified review; this plan neither clears a
trademark nor purchases a domain. Read current repository rules and propose
enforceable review/check protection for the owner's chosen hosting plan. Local
hooks are useful, but cannot be described as server enforcement. No paid plan,
repository visibility change, report transmission or new public contact is
authorized by this preparation assignment.

**Exit:** concrete, tested local documentation and a reviewable list of the
external configuration/owner actions still needed. Enable and verify those
external routes only under explicit authority before claiming they exist.

## W14 - Obtain the release decision and execute the authorized scope

**Outcome:** O6. **Authority:** Feature 008 T035 and explicit owner decision.

Present the exact candidate identity, included capabilities, evidence matrix,
remaining limitations, trust-verification commands, install/rollback routes and
proposed distribution destination. Record whether the owner accepts, defers or
rejects that concrete candidate. Do not substitute permission to push source,
run CI or prepare a package for permission to tag, sign or publish it.

If authorized, record the controlled authorization reference and rerun the
readiness check. Resolve all remaining blockers and close T033 only with its
complete evidence before distribution. Execute only the authorized scope,
independently download/read back the
published bytes and metadata, verify signatures/provenance/digests and smoke
the supported installation routes. Update the catalogue, changelog, support
matrix and status from those facts. If rejected or blocked, retain the candidate
and the unresolved gate; no release claim or checkbox is earned by waiting.

**Exit:** a recorded release decision and, only if authorized and successful,
verified distribution. The package version remains governed by `Cargo.toml`;
roadmap milestones do not assign or automatically advance version numbers.

## W15 - Maintain the supported release

**Outcome:** sustained O1-O6. Reuse CI dependency, licence, secret and server
matrix controls. Review scheduled results, supported PostgreSQL versions,
toolchain compatibility and credential-provider CLI changes. Triage security or
data-loss reports before new features; preserve exact affected build identity.

For every patch, reproduce the defect, specify the changed contract, keep the
fix narrow, run the affected/full gates and repeat the relevant packaged-runtime
evidence. Practise rollback and trust-key revocation with synthetic or dry
evidence before an incident. Publish only through W14's explicit release route.

**Exit for each maintenance cycle:** source/evidence/support state agrees,
reported failures have a disposition, and the next cycle has an assigned
maintainer. No silent updater or telemetry is added.

## W16 - Expand only where observed jobs justify it

This is a conditional backlog, not a commitment or a release prerequisite.
Rank candidates by frequency of a demonstrated blocked job, safety benefit,
implementation/review cost and ongoing support burden. Do not use feature count
or an invented market comparison as the priority score.

| Candidate | Evidence required to open a feature | Architectural constraint |
| --- | --- | --- |
| Deeper PostgreSQL workflows such as COPY or additional type/plan support | A repeated task cannot be completed through current documented routes | PostgreSQL semantics, bounded streaming, explicit side effects and CLI contracts |
| Safer concurrent cell editing | Real need to detect a changed row between read/review/write | Specify conflict detection and transaction semantics; never silently retry writes |
| Additional connection/security options or pooler support | A named target requires a currently refused/unsupported capability | No silent TLS or authentication weakening; current vendor/server evidence |
| Native clipboard or credential storage | Observed daily friction justifies reopening ADR-0013 or ADR-0011 | Explicit transfer/storage consent, removal/recovery, platform tests and threat model |
| Direct cloud token acquisition | Vendor CLI setup is a measured adoption blocker | New network/cache/browser trust design before implementation; preserve local core |
| Additional architectures/distribution channels | Identified users cannot install the current targets | Full build/runtime/rollback/signing/support cost, not compile-only expansion |
| Session/workspace convenience | Measured navigation or repetition problem after W09 | No automatic query replay, result persistence or hidden connection switching |

Generic databases, an AI upload path, accounts/sync, migrations, BI and a plugin
marketplace remain outside the vision. Reopening a constitutional constraint
requires the constitution's explicit amendment process, not a roadmap task.
