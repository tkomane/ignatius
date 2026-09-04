# Interaction principles

The emotional target is calm confidence. The user should feel oriented, in
control, and slightly delighted. Nothing here is decoration for its own sake.

## The rules

**One obvious primary action.** At any moment there is one thing the interface is
inviting you to do, and it says what key does it. The empty results pane says
"Press Ctrl+R to run the buffer" rather than sitting blank.

**Every state is a word.** Colour reinforces; it never carries meaning alone. The
production marker is `[PROD]`. Read-only posture is `[read-only]`. A NULL is
`[null]`. Turn colour off and nothing is lost.

**Discovery follows the state.** The first frame, footer and command palette
describe what is useful now, using the active keymap rather than a remembered
default. An empty or blocked pane names one safe next action, and never implies
that an unavailable query, connection, or result can be used.

**Say what is true, including when that is "unknown".** `Cancellation requested`
is a different state from `Query cancelled by server`, because between them the
statement may still be running. `TLS state unknown` is a real answer.

**Errors point, but never guess.** A server position moves the caret only when it
maps to the unchanged submitted statement. The error line uses an explicit `!`
marker, the token is emphasized when identifiable, and statement, line, and
column are written in words. Missing, stale, or rewritten positions say why no
marker was applied.

**Database facts stay facts.** Constraint schema, relation, column, and
constraint names come from PostgreSQL. A catalogue-derived type appears only
after a current snapshot matches the supplied relation and column; otherwise
the interface names the limitation.

**Progressive disclosure.** An error shows four short lines by default and hides
the server's technical fields behind one key, with a count so you know they exist.

**Trust is inspectable.** The connection header answers the glance question; the
palette's connection details surface answers the careful question. It names the
target, server posture, TLS state, and credential route without refreshing,
executing, or exposing a token. Inspection is read-only, dismissible with one
key, and safe to open before a consequential query.

**Starting is a choice when choices exist.** If named profiles are configured and
the user has not already supplied a route, the first frame offers a searchable
default row and safe profile summaries instead of making someone type a URI.
The list never resolves a target or credential while it is being read. Enter
selects one name; the runtime performs the existing resolution and authentication
path afterward. Switching clears old server facts while preserving the SQL
buffer and local reading preferences.

**Density with room to breathe.** Every element earns its space. The header
carries connection, environment, posture, transport and timing because a user
running statements against a real database needs all five without opening a
panel. It carries nothing else.

**Feedback is immediate and honest.** Pressing cancel changes the status
immediately, because the request was made immediately. It does not claim the
outcome, because the outcome is not known yet.

**A result view never pretends to be a second query.** Sorting, hiding, sizing,
and freezing operate only on retained rows and source columns already in the
screen. The grid says when order is local, how many rows are retained, and when
type metadata is unavailable. Reset returns to the server order without
rewriting SQL or issuing another request.

**Refreshing is explicit and source-bound.** `Ctrl+Shift+R` refreshes only a
completed retained result whose source is one read-classified statement. It
uses the retained template rather than the editable buffer, prompts again for
named parameters, states the in-flight and actual outcome, and never reruns on
focus, filtering, sorting, reconnect or failure.

**Saving is an intentional shape choice.** `Ctrl+K e` opens a searchable
format palette before asking for a path. It names the retained rows that will
leave the screen, keeps the choice independent of the filename extension, and
does no file work until the destination is accepted. SQL `INSERT` output stays
in the explicit scripted route, where its table and literal rules can be
reviewed before execution.

**Selection has an identity.** A selected row is a source record, not merely a
screen position. Filtering, sorting, hiding a column, and the expanded or
inspector views preserve that identity or state why it cannot be shown.

**Moving a value out is intentional.** Copying a retained text cell requires an
explicit confirmation and states the row, column, and UTF-8 size without
showing the value. OSC 52 is disabled by default; when enabled, the client
writes and flushes a terminal sequence but never reads or clears the clipboard.
Success therefore says that terminal acceptance is unconfirmed, and the
terminal, SSH path, or multiplexer exposure remains visible to the user.

**Formatting is a local, reversible edit.** `Ctrl+Shift+F`, `Ctrl+K q`, the
palette, and plain `\\format` expose the same deterministic operation. It
changes whitespace and layout only outside protected SQL regions, never runs a
query, and reports whether it applied, had nothing to do, or refused safely.
One Undo restores the complete prior buffer and cursor; a changed buffer also
invalidates any old server error location.

**A plan separates belief from observation.** Plain `EXPLAIN` says estimate
only, keeps planner cost units separate from milliseconds, and never executes
the target statement. `EXPLAIN ANALYZE` says that it executes, may have side
effects, and is not automatically rolled back before it asks for confirmation.
Actual rows, startup and total time, and loops appear only when PostgreSQL
supplied them; a tenfold row deviation is labelled as an estimate mismatch.

**Plan exploration is local.** The selected operation, expanded branches and
bounded tree window are in-memory view state. Movement, expansion, collapse and
dismissal do not rerun SQL, alter the editor, add history, or change the
ordinary retained result underneath the plan. Selection and the way out remain
written in ASCII and no-colour modes.

**Nothing modal for anything harmless.** Confirmations are reserved for actions
that deserve them. A confirmation people dismiss reflexively is worse than none,
because it trains the reflex.

## What is deliberately avoided

- ASCII art on launch, gradients made of glyphs, spinners without text.
- A border around everything. Borders separate panes and nothing else.
- Unexplained abbreviations. `read-write` rather than `rw`.
- A permanent wall of shortcuts. Six hints in the footer; the rest in help.
- Vim assumptions. Every action has a named key and appears in help.
- Copying another client's visual identity.
- A tutorial that blocks the first useful action. Guidance is in the frame and
  can be dismissed or bypassed without persisted onboarding state.
- A wide result that silently becomes a clipped printout. The visible-column
  chooser, bounded widths, type labels, and frozen first column are one
  recoverable view with a reset path.

## Motion

There is almost none, and what exists can be turned off with
`ui.reduced-motion`. Progress is text, because text is readable by a screen
reader, survives a slow terminal, and does not imply precision that is not there.
