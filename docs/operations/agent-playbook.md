# Agent playbook

Operational handoff for the [product roadmap](../product/roadmap.md).
The [constitution](../../.specify/memory/constitution.md) governs changes;
[status](../status.md) is the live resumption point; the selected feature's
`spec.md` and `tasks.md` govern implementation. This document defines how an
agent takes an assignment through those authorities without creating another
completion ledger.

## Start an assignment

1. Read `AGENTS.md`, the constitution, `docs/status.md`, recent Git history,
   `git status` and the selected full feature slug. Read its plan/tasks and
   relevant ADRs, compatibility rows, security boundaries and user journeys.
2. Locate the work package in
   [agent-delivery-plan.md](../product/agent-delivery-plan.md). State the outcome,
   acceptance scenario and write scope. A branch name or `.specify/feature.json`
   is a routing hint; confirm that it names the intended feature before using
   Spec Kit commands. Never change another worker's active selector.
3. Read every file before editing it. Identify pre-existing diffs and the
   source revision used by existing evidence. Obtain an isolated checkout when
   independent work needs one; include dependent local work explicitly rather
   than assuming remote main contains it.
4. If the spec/task package is incomplete, prepare it before implementation.
   Use specify -> clarify material ambiguity -> plan -> tasks -> analyse ->
   implement -> verify -> record status. Close evidence gaps with evidence;
   do not invent a new feature just to rename an old unchecked task.
5. Choose the smallest complete user journey that advances the assigned
   outcome. Include loading, empty, refusal, error, cancellation, recovery,
   narrow/plain/ASCII and machine-output boundaries where applicable.

## Assignment and return contract

Use this compact block in the task handoff or review description. Put durable
decisions and results in the repository, not only in the conversation.

```text
Package / full feature slug / outcome IDs:
Source revision and pre-existing work:
Problem and expected user-visible result:
Owned paths and shared-file coordinator:
Dependencies and explicit non-goals:
Acceptance scenarios / existing requirement and task IDs:
Commands and environment needed:
External actions already authorized, if any:

Return:
  Changed paths and why:
  Focused evidence:
  Full-suite evidence:
  Live platform / terminal / server / build evidence:
  Skips, failures and unverified claims:
  Updated task/status/compatibility references:
  Remaining decisions and exact next action:
```

An assignment is ready when its acceptance and write boundaries are concrete,
its dependencies are present and its specification has no unresolved security
or data-semantics ambiguity. A feature is implemented when its behaviour and
required checks exist; it is platform-verified only for named live evidence;
it is release-ready only under the release contracts. Use those distinctions
in every return.

## Coordinate shared work

Multiple agents are an optional execution arrangement, not an instruction to
spawn them automatically. Follow the session's delegation policy. One agent
can execute the same work packages sequentially.

| Shared area | Coordination rule |
| --- | --- |
| `app/model.rs`, `app/message.rs`, `app/update.rs`, `cli/interactive.rs`, `ui/layout.rs` | One integrator owns the combined transition/API changes; independently developed patches agree on message/effect contracts first |
| Query helpers, connection adapters, terminal test harnesses | Can be isolated once inputs/outputs and fixtures are agreed; do not edit another lane's shared reducer or runtime silently |
| `Cargo.toml`, lockfile, toolchain, CI and release schemas | One change owner at a time; preserve pins, minimum Rust and release contracts |
| `docs/status.md`, roadmap, compatibility, changelog, release catalogue | Integrator publishes the combined local record after reviewing each lane's evidence; reread immediately before editing |
| Existing feature task files | Feature owner updates its checkbox evidence; shared scenario results are linked, not independently relabelled |

Keep independent work in separate branches/worktrees when useful and identify
the integration base explicitly. Do not reset, stash away, force-push or stage
unrelated user work to solve a collision. When two changes conflict, preserve
both intended behaviours and run the affected combined scenarios. Git metadata
or filesystem permission restrictions are environment boundaries, not reasons
to modify the repository through an unreviewed workaround.

## Architecture review checklist

Before implementation, answer these in the feature plan:

- Which intent becomes which message/effect? What source/job/connection
  identity rejects an obsolete reply or confirmation?
- What is pure and what performs I/O? Does the operation touch the database,
  filesystem, process environment, provider program or terminal clipboard?
- What data crosses each trust boundary, how is it bounded, and where does it
  become display-safe? Are any raw secrets or expanded SQL copies displayable?
- What are the exact PostgreSQL, TLS and transaction semantics? What remains
  unknown after cancellation or connection loss? Is any retry/replay possible?
- How is the action discovered with the active keymap, and what carries its
  meaning without colour, Unicode or a full-screen renderer?
- What proves the user outcome: pure tests, a runtime contract, real database,
  real terminal, participant observation, or exact packaged bytes?
- What is the recovery path, and which compatibility, security, release-note
  or ADR record must change with the behaviour?

The component and [runtime](../architecture/runtime-state.md) documents give
the existing boundaries. Do not add a database abstraction, state store,
second redactor or native adapter for organisational convenience.

## Verification and evidence

For application changes, the authoritative local command is:

```bash
cargo --locked xtask db up
cargo --locked xtask verify
```

The fixtures contain synthetic credentials. Never put real credentials in
commands, test fixtures, screenshots or saved evidence. The verifier discovers
plain/TLS fixtures; a real Unix socket separately requires the configured
`IGNATIUS_TEST_PG_SOCKET_URI` fixture. Follow
[local development](local-development.md) and [verification](verification.md)
for the actual environment. Teardown only disposable resources started for the
assignment; do not remove a shared running fixture owned by another worker.

Run focused checks that prove the changed claim first, then the required full
gate. For documentation-only planning changes, check path/anchor validity,
requirement and dependency coverage, contradictions, whitespace and any
existing documentation contracts affected; report the application suite as
not rerun when it was not. New tests should prove a failure boundary, not mirror
the implementation or merely assert that a document contains its own words.

For every evidence item record:

| Field | Required content |
| --- | --- |
| Claim | Requirement, task, outcome or scenario being proved |
| Source | Commit plus dirty-worktree qualification, or immutable candidate identity |
| Environment | OS/architecture, terminal/shell when relevant, toolchain, server/provider version and fixture identity |
| Method | Exact command or human steps, synthetic inputs, expected observation |
| Result | Pass, fail, skip or unverified; counts and safe observations |
| Scope | What this result proves and what it cannot prove |
| Retention | Safe repository reference or approved evidence location; expiry if temporary |

A green workflow is evidence only after its actual relevant jobs and steps are
inspected. A test that returned early is a skip even if the harness prints OK.
WSL cannot prove native Windows behaviour, a pty cannot prove Warp rendering,
and a synthetic token cannot prove Entra/AWS/GCP authentication.

## Decisions and external actions

Make routine reversible implementation decisions inside the accepted scope.
For an unresolved architecture choice, first prepare alternatives, consequences,
the proposed spec/ADR change and a recommended choice. Ask only for the
material decision that remains; continue independent work while it is pending.

Owner authority is required for the repository's protected actions: push,
publish, tag, sign or release, plus destructive actions, production changes,
new paid resources and outbound messages. Existing explicit session authority
persists; do not request it again. Prepare and verify the exact diff, candidate
or external configuration before seeking approval so the decision is concrete.
An instruction to plan a cloud test does not authorize obtaining a production
credential or creating the test infrastructure.

Do not reopen ADR-0011's credential store or ADR-0012's libpq decision without
the evidence they require. Release trust choices still pending in ADR-0008
must stay explicit. Never weaken a constitutional gate to meet a schedule.

## Stop and hand off cleanly

Before ending, update the selected tasks and `docs/status.md` with the exact
state, evidence boundaries and next action. If blocked, identify the unavailable
platform/account/decision and continue an independent package where possible.
Do not rerun or restart another agent's process because a poll timed out.

Before calling work complete, inspect each required deliverable and its
authoritative evidence. Report focused tests, full suite, live evidence and
unverified work separately. State what was changed locally and what, if
anything, was actually committed, pushed or distributed.
