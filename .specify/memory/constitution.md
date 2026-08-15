# Ignatius Constitution

Ignatius is a local-first, keyboard-first PostgreSQL client for the terminal.

This constitution governs every specification, plan, task and change in this
repository. It is short on purpose: a principle that does not reject an
attractive shortcut is decoration, so each one below has been written to be
usable as a reason to say no.

When a change conflicts with a principle, the change loses. Amending a principle
is allowed, but it takes an explicit commit that says what changed and why, and a
matching update to the specifications it affects.

## Core Principles

### I. Delight must remain truthful

Friendly copy, colour, motion and automation exist to make real state easier to
read. They may never disguise uncertainty, data loss, incomplete verification, a
side effect, a degraded connection, or an unsafe default.

Concretely: the interface says `Cancellation requested` until the server confirms
the cancellation, and `Connection lost - query outcome unknown` when that is the
truth. A partially written export is reported as partial. A statement is never
described as having succeeded when what is known is that a connection dropped.

### II. PostgreSQL correctness before generic abstraction

Ignatius is optimised deliberately for PostgreSQL. It respects PostgreSQL types,
SQLSTATE, notices, transaction states, schemas, search paths, server versions,
cancellation, COPY, EXPLAIN, extensions, roles, permissions and connection
conventions.

No lowest-common-denominator database abstraction may be introduced in version 1.
Where a PostgreSQL behaviour cannot yet be supported, it is named in
`docs/support/compatibility.md` rather than approximated.

### III. Local-first and private by default

The core product works with no account and no cloud service. By default there is
no telemetry, no update check, no query upload, no result upload, and no sync.

Result rows are never persisted unless the user explicitly exports them. Query
history is transparent, controllable, scoped, and erasable. Adding any network
call that is not a database connection requires a specification and a threat
model update first.

### IV. Safe by default, with expert escape hatches

Database privileges are the primary control and Ignatius never pretends
otherwise. On top of them it adds comprehensible environment cues and guardrails
for destructive or production operations, while stating plainly that heuristic
SQL classification is advisory and not a security boundary.

Ignatius never silently replays a statement, retries a write, downgrades TLS, or
changes transaction semantics. A parameter that would weaken confidentiality or
identity checking is refused rather than ignored.

### V. Keyboard-first, never keyboard-only

Every core workflow can be completed with the keyboard, with visible focus.
Mouse support is progressive enhancement and disabling it removes no capability.

Commands are discoverable through contextual hints, searchable help, a command
palette, and configurable keymaps with conflict validation. No workflow may
assume the user already knows Vim.

### VI. Accessible meaning survives styling loss

Colour, Unicode glyphs, box drawing and icons are supplementary. Explicit text,
layout, focus and status wording must still carry the meaning in no-colour,
ASCII, narrow-terminal, low-contrast and plain modes.

A production connection is marked with the word `PROD`, not a red border. A NULL
is distinguishable from an empty string and from the text `NULL` without colour.

### VII. Cross-platform behaviour is a release criterion

macOS in Warp and Windows 11 in Windows Terminal are first-class targets, and
Linux is supported and validated in CI. Terminal restoration, input handling,
paths, credential storage, line endings, packaging, completions and documentation
are tested on all three before a release, not after it.

### VIII. One source of truth for product state

Specifications, version, release notes, configuration schema, migrations,
generated completions and compatibility claims each have exactly one authority.
Generated artefacts are reproducible and checked for drift.

Product version, source revision, build identity and target triple are four
distinct facts. None may be inferred from another, and a rebuild never invents a
new product version.

### IX. Evidence over confidence

A requirement is not done because the code looks right. Each feature carries
proportionate automated checks, terminal-level evidence where the claim is about
the terminal, container-backed evidence where the claim is about PostgreSQL, and
an explicit definition of done.

Reports separate focused tests, full-suite results, live evidence, and unverified
assumptions. An environment limitation is reported as an environment limitation,
never as a passing test.

### X. Recoverability is part of quality

The terminal is restored after normal exit, error, cancellation, panic and
supported signals. Local state writes are atomic and migratable. Users have a
documented recovery route for corrupted configuration, an unavailable keyring, a
broken connection, an interrupted export, and an incompatible terminal.

## Additional Constraints

- **Architecture.** A testable unidirectional state model: input and async events
  become messages, a pure reducer produces the next model and a list of effects,
  and an executor performs them. UI widgets render state and emit intent; they do
  not open connections or write files. Stale async results are rejected by job
  identity. Global mutable state is avoided.
- **Secrets.** No secret appears in source, committed configuration, logs,
  diagnostics, screenshots, process arguments or test fixtures. Secrets use
  dedicated types that do not print themselves, and all displayable text passes
  through one redaction implementation.
- **Dependencies.** Prefer few, mature, well-licensed dependencies with a clear
  reason each. The lockfile is committed and the toolchain is pinned.
- **Prose.** Documentation and user-facing copy use plain language, explicit
  labels and visible counts. Use a normal hyphen, never an em dash. Dates that
  matter are absolute.

## Development Workflow

- Work proceeds as thin vertical slices, each specified before it is built:
  specify, clarify where ambiguity is material, plan, generate tasks, analyse for
  cross-artefact conflicts, implement, verify, and record status.
- `docs/status.md` is kept current, and always before context runs short. A
  decision that exists only in a chat log does not exist.
- Every change that alters user-visible behaviour updates the documentation and
  the release notes in the same commit.
- The pull-request gates in `docs/operations/verification.md` are the minimum:
  formatting, compilation on every primary target, clippy with warnings denied,
  unit and snapshot tests, representative PostgreSQL integration tests,
  dependency advisory and licence checks, secret scanning, and a terminal
  restoration check.
- Commits are small and imperative and explain why. Nothing is published, pushed,
  signed or released without the owner's explicit authority.

## Governance

This constitution supersedes convenience, habit and personal preference. Where a
principle and a deadline conflict, the deadline is renegotiated.

Amendments require a commit that states the principle changed, the reason, and
the specifications updated to match. Complexity that a principle would reject
must either be removed or justified in writing in the relevant plan, in a
`Complexity Tracking` section, and referenced from the affected specification.

Operational guidance for working in this repository lives in `CLAUDE.md`.

**Version**: 1.0.0 | **Ratified**: 2026-08-15 | **Last Amended**: 2026-08-15
