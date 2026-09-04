# The experience roadmap

**Written 2026-08-16**, from the owner's brief: the differentiator is that this
is the PostgreSQL client with the best experience anyone has used, that it
demands the least expertise and the least remembering, and that it brings as
much of a graphical client's help as a terminal can carry.

This document is the argument for what to build and in what order. Each item
becomes a specification of its own before any of it becomes code.

**Execution planning updated 2026-09-04:** the ordered experience through
Feature 024 is now implemented locally, with evidence boundaries below. Use
the [product roadmap](roadmap.md) for current milestone order and the
[agent delivery plan](agent-delivery-plan.md) for assignments. This document
retains the product rationale and original sequencing; it is not a separate
instruction to reimplement those features. [Product acceptance](acceptance.md)
defines how to test the promised reduction in expertise and remembering.

## Current delivery

As of 2026-09-03, Feature 017, the connection trust surface, is implemented and
pushed to `main` in commit `9f80bef`. It makes the existing cloud-auth route
legible before a query: the command palette opens a read-only view of the target,
environment, server posture, observed TLS state, provider command summary,
credential lifetime caveat and provider remedy. Entra, AWS, Google Cloud and
configured providers share the same display contract, while tokens and provider
commands are never refreshed or executed by the view.

This is a cross-cutting trust and discoverability slice, not a replacement for
the ordered experience work below. Feature 012 schema-aware completion and
Feature 013 errors that point at the problem are now implemented locally.
Feature 014, a result grid that can be worked, is now implemented locally as
well. Live cloud-account evidence is still separate from the local
implementation and is recorded in the compatibility matrix when available.

As of 2026-09-04, Feature 012 is implemented in ancestor commit `3168e7c`. The
full-screen editor and plain mode share
one lexer-backed engine and one catalogue snapshot with bounded visible lists, with cursor-scoped
objects, aliases, schemas, columns, CTEs, quoted acceptance, truthful loading
and stale states, and ASCII/no-colour wording. Focused reducer, renderer, CLI,
and PostgreSQL 18.4 evidence is recorded locally; the locked full verifier
passes all available gates, while the Unix-socket integration and cross-platform
hand checks remain separate evidence gates.

As of 2026-09-04, Feature 013 is implemented on the local
`codex/013-errors-at-cursor` branch. Server character positions map to the right
statement and UTF-8-safe editor offset, valid locations move focus without
changing SQL or undo history, and edits invalidate old markers. Constraint
diagnostics promote server-supplied object fields, use the current completion
snapshot only for a confirmed match, and state catalogue limitations. Plain and
JSON output retain the same facts without control sequences or result data on a
failed stdout stream. Focused tests and the locked verifier pass against
disposable PostgreSQL 18.4; Unix-socket, platform hand, and screen-reader checks
remain separate evidence gates.

As of 2026-09-04, Feature 014 is implemented in the local working tree. Results
can be sorted locally over retained rows with stable source-row identity, and a
searchable result-grid palette controls visible columns, bounded widths,
server-described type labels, and a frozen first column. `Ctrl+K g` is
contextual to Results, reset restores the automatic view, and new executions
clear result-specific shape while retaining only the documented reading
preferences. The existing filter, inspector, expanded row, plain output, JSON,
streaming export, history, and editor contracts remain separate from this view.
Focused and live plain/TLS PostgreSQL evidence pass and are recorded in
`docs/status.md`. Unix-socket, platform hand, screen-reader, live-resize, and
Warp renderer checks remain separate evidence gates.

Feature 015 guided discovery is now implemented in the local working tree. The
first frame, empty panes and blocked states explain a safe next action; the
footer follows focus with no more than five contextual hints; and the general
palette searches by intent while retaining prerequisite guidance. Discovery uses
the active configured keymap and is presentation-only, so opening or searching
does not run SQL, reload metadata, write files, record history, or persist a
tutorial. Focused automated evidence is recorded in `docs/status.md`; manual
terminal and screen-reader evidence remain separate.

As of 2026-09-04, Feature 018 closes the first copy boundary locally. `Ctrl+K c`
and the inspector or palette can ask for one retained text cell, preserve its
source-row identity through filtering and sorting, and send it only after an
explicit confirmation. OSC 52 is opt-in, bounded at 1 MiB of UTF-8 bytes, and
write-only: success reports bytes and characters sent while leaving terminal
acceptance unconfirmed. Plain, JSON, NDJSON, history, export, and SQL editing
remain separate. Focused automated evidence is recorded in `docs/status.md`;
hand use in actual terminal, SSH, and multiplexer paths remains separate.

As of 2026-09-04, Feature 019 makes the editor locally legible in one action.
`Ctrl+Shift+F`, `Ctrl+K q`, the palette and plain `\format` share one bounded,
deterministic formatter. Major clauses, predicates and eligible lists gain
readable layout while strings, quoted identifiers, dollar bodies and comments
remain exact. A changed buffer is one undoable edit with logical cursor
recovery; malformed, comment-only and over-limit input stays untouched. This
is an editor-only transformation: it does not contact PostgreSQL, add history,
write files or mix interaction text into result output. Focused and full locked
automated evidence is recorded in `docs/status.md`; manual terminal and
screen-reader evidence remain separate.

As of 2026-09-04, Feature 020 makes implicit startup a choice when named
connections exist. The first frame opens a searchable picker with a default
route and safe profile summaries, while explicit targets and route options keep
the existing path. Enter resolves only after a selection and sends only the
profile name across the reducer/runtime boundary; passwords, tokens and unknown
profile fields never reach the model. `Ctrl+K n` supports quiet-session
switching, which clears old server facts and rejects late work without touching
the SQL buffer or local result-reading preferences. Focused automated evidence
and the locked verifier are green with the semantic Unix-socket gate explicitly
skipped; terminal, screen-reader and cross-platform hand checks remain
separate.

As of 2026-09-04, Feature 021 is implemented in the local working tree. The
locked verifier passes with the semantic Unix-socket gate explicitly skipped.
`Ctrl+K e` now behaves like a small save-as flow: it names
the retained and filtered row scope, lets the user search for CSV, TSV, JSON,
NDJSON or Markdown, and only then asks for a path. The filename extension is
not consulted. The scripted route also accepts explicit
`--format insert --insert-table TABLE`, quotes identifiers, preserves NULL and
escapes text without type guessing, while refusing ambiguous columns, NUL text
and headerless output. Generated SQL remains reviewable data and is never
executed by the client.

As of 2026-09-04, Feature 022 is implemented in the local working tree. A
bounded lexer-backed scanner recognizes safe `:name` placeholders outside
strings, quoted identifiers, dollar bodies and comments, and prompts once per
distinct name in first-use order. The full-screen and plain clients mask typed
values, accept empty text, preserve the template in history and diagnostics,
and offer `--param-env NAME=VARIABLE` for non-interactive runs. Binding uses
escaped PostgreSQL literal text over the existing simple-query route, refuses
NUL and never logs or persists the values. The locked verifier, live PostgreSQL
checks and a macOS pseudo-terminal check pass; the semantic Unix-socket gate
and manual terminal, screen-reader and cross-platform hand checks remain
separate.

As of 2026-09-04, Feature 023 is implemented in the local working tree. With
Results focused, `Ctrl+K u` and the command palette accept only a conservative
single-table direct `SELECT`, resolve its live primary-key metadata in the
session search path, and refuse production-classified or server read-only
sessions before replacement input. The value prompt is separate from the
review prompt; the latter shows the exact bound `UPDATE` and Enter emits one
parameterized write. Esc sends nothing, stale result identity is rejected, and
the source result is never rerun automatically. Focused, live PostgreSQL and
locked-verifier evidence is recorded in `docs/status.md`; terminal,
screen-reader and cross-platform hand checks remain separate.

As of 2026-09-04, Feature 024 is implemented in the local working tree. With
Results focused, `F6`, the command palette and the contextual footer can request
one explicit refresh of the retained source. `Ctrl+Shift+R` is also accepted by
terminals with enhanced modifier reporting. The action ignores
later editor edits, accepts only one read-classified statement, prompts again
for named parameters, shows a visible in-flight and outcome state, and never
replays on focus, filtering, sorting, reconnect or failure. Focused checks and
the locked verifier pass, including plain and TLS PostgreSQL 18.4 evidence; the
semantic Unix-socket gate and terminal, screen-reader and cross-platform hand
checks remain separate evidence gates.

## What a graphical client is actually giving people

Not beauty. Four things, and they are worth naming separately because they cost
different amounts to reproduce in a terminal:

1. **It knows the schema, so the person does not have to.** Every table name,
   column name and function is offered rather than recalled. This is the largest
   single difference between writing SQL in DataGrip and writing it in `psql`,
   and it is worth more than everything else on this list put together.
2. **It puts the answer where the question was.** A syntax error is underlined
   in the statement, not printed as a character offset. A plan is a tree with the
   expensive node marked, not 40 lines of text.
3. **It makes state visible without being asked.** Which server, which database,
   which transaction, how many rows, how long, what type this column is. In a
   terminal client most of that is a command you have to know to run.
4. **It is discoverable.** Someone who has never opened it can find things by
   looking. In a terminal, capability that nobody can find does not exist - a
   lesson this project learned expensively when five documented chords turned
   out to be bound to nothing.

What a terminal gives back is speed, an SSH session, scriptability, and no mouse
requirement. The aim is to keep all of that and take the four above.

## What constrains the answer

The constitution is not decoration here; three principles decide real questions.

- **Delight must remain truthful (I).** A completion list that offers a column
  that does not exist, or that quietly goes stale after a migration, is worse
  than no completion list. Anything derived from the catalogue says how fresh
  it is.
- **Accessible meaning survives styling loss (VI).** Every icon has a word.
  Every colour has a word. `--plain` and a screen reader get the same
  capability, expressed differently, and the ASCII glyph tier is not a
  degradation to be tolerated but a tier to be designed.
- **Keyboard-first, never keyboard-only (V).** Mouse support is welcome and is
  never load-bearing. No workflow may assume Vim.

## The original order, and why

### 1. Completion that knows the schema - Feature 012

The largest gap and the one people notice in the first minute. Keywords,
schemas, tables, views, functions, and columns scoped to the tables actually in
the statement, including through aliases. Offered as you type, accepted with one
key, dismissed with another, never inserting anything you did not choose.

It is first because everything else on this list is an improvement to a session
that is already going, and this one changes whether the session gets going at
all. The catalogue it needs is already loaded for the object tree, over a
connection that already exists.

The hard part is not the list. It is knowing what is in scope at the cursor,
which means understanding the statement well enough to know that `o.` after
`FROM orders o` means the columns of `orders`. The lexer that colours the buffer
already tokenises it; this needs a little more structure than that and much less
than a parser.

### 2. Errors that point at the problem - Feature 013 - implemented locally

PostgreSQL returns the character position of a syntax error and the client
now puts the cursor there and marks the token. A line and column in words keep
the answer useful without colour, while an unchanged buffer is the only one that
receives a marker. This removes a small tax paid on every mistake.

The same applies to a failed constraint: the server names the constraint, and
the client promotes the supplied table and column. The catalogue can add a type
only when the current snapshot matches; loading, stale, unavailable, and
non-matching states remain explicit.

### 3. A result grid you can actually work - Feature 014

Sort by a column without rewriting the query and re-running it. Hide columns
that are in the way through a searchable chooser. Widen one within safe bounds.
See each available server type without asking, with unavailable metadata named
as unavailable. Freeze the first visible column so a wide table stays readable,
even in a narrow or ASCII terminal.

This is where a graphical client feels effortless and a terminal client feels
like a printout. The controls are palette-first and keyboard reachable with
`Ctrl+K g`, while the grid states local order, shown-column count, type
availability, frozen-column state, and the retained-row scope in words. It is
all local work on rows already retained, so it needs no query rewrite or rerun.

### 4. Finding your way without being told - Feature 015

The discoverability item, and the one that decides whether "least expertise" is
true. A first run that explains itself. A hint line that changes with focus and
says what the useful keys are here. A palette that is the answer to "what can I
do" rather than a list of commands you already knew. An empty state that
suggests the next thing rather than sitting blank.

The implementation keeps this local and bounded: the first frame and empty
states are state-specific, the footer shows at most five complete pairs, and the
palette searches labels and intent details while explaining prerequisites for
unavailable actions. The active configured keymap is the only source for every
advertised key.

It is fourth rather than first only because the three above give it something
worth finding.

### 5. A plan you can read - Feature 016

`EXPLAIN` and `EXPLAIN ANALYZE` rendered as a tree, with estimated against
actual rows, time per node, and the expensive node marked in words as well as
colour. Sequential scans on large tables and misestimates by an order of
magnitude are the two things people are looking for, and both can be pointed at
directly.

The local implementation keeps the distinction explicit. `Ctrl+K l` reads a
structured estimate for the statement under the cursor and never executes it;
`Ctrl+K a` warns that analysis executes in the current session, may have side
effects, and is not automatically rolled back before asking for confirmation.
The Results pane then provides estimated cost units, row width, observed startup
and total time, per-loop rows and loops where supplied, a tenfold estimate
mismatch label, and an attention marker whose basis is stated as estimated cost
or measured time. Selection, expansion, collapse and dismissal are local, and a
bounded 500-node, 1 MiB view keeps a large or malformed response recoverable.

### 6. The rest, in no fixed order

- **Format the buffer.** One key, PostgreSQL-aware, idempotent, bounded and
  reversible. Formatting is local and conservative: protected SQL bytes are
  retained exactly, the cursor follows its token, one Undo restores the prior
  buffer, and plain `\format` leaves the statement pending for an explicit
  terminator.
- **A connection picker**, so starting is choosing rather than typing a URI.
  Feature 020 now covers the local picker and quiet-session switch boundary;
  live terminal and platform evidence remain separate.
- **More export shapes**: Feature 021 now covers an intentional format-first
  save flow for CSV, TSV, JSON, NDJSON and Markdown, plus explicit CLI `INSERT`
  statements. Feature 018 covers the first copy route through opt-in OSC 52; a
  native clipboard route remains deferred until a concrete platform need
  justifies it.
- **Parameters prompted**: Feature 022 now covers named placeholders, masked
  prompts, secret environment mappings and literal-safe binding without
  changing the template or query history.
- **Editing a cell to generate an `UPDATE`.** Feature 023 now covers the first
  conservative slice: direct single-table projections with live primary-key
  identity, a separate replacement and review boundary, exact bound SQL, one
  parameterized write, stale-state refusal and no automatic rerun. Production
  and server read-only sessions remain refused.
- **Refreshing a retained result.** Feature 024 now covers one deliberate
  refresh from the source behind a completed result. It is intentionally narrow:
  one read-classified statement, a usable connection, named-parameter
  re-prompting, an explicit outcome, and no passive replay.

## What is deliberately not on the list

- **Entity-relationship diagrams and schema comparison.** Real features of real
  products, and neither survives contact with an 80-column terminal.
- **A second database.** Principle II. The optimisation for PostgreSQL is the
  product.
- **Anything that uploads a query, a result or a schema.** Principle III, and it
  is not negotiable for a convenience.

## How this is judged

Not by feature count. By whether someone who knows SQL and has never seen this
can connect, find a table, write a working query against it and get the rows out
without reading documentation - and by whether someone who does that daily stops
noticing the tool. Both are hand-verification questions, and neither is answered
by a test suite.
