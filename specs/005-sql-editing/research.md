# Research: Writing the SQL

**Status**: Repository-grounded design research. It records constraints and
does not authorise changes to Claude's active source paths.

## Decision 1: Keep the editor pure

**Decision**: Keep text editing, cursor movement and history in `Editor` and
keep execution in the existing reducer/effect boundary.

**Rationale**: A pure buffer can prove UTF-8 boundaries, undo and movement
without a terminal or database. It also preserves the constitution's
unidirectional state model.

**Rejected**: Letting the widget own cursor or history state would create a
second source of truth and make scripted or reducer tests incomplete.

## Decision 2: Use byte offsets with boundary invariants

**Decision**: Store the cursor as a byte offset, but move only through
`String` character boundaries.

**Rationale**: Statement selection already uses byte offsets, while Rust's
boundary checks prevent splitting a multi-byte value. Every public movement and
edit operation must preserve the invariant.

## Decision 3: Use bounded snapshots for history

**Decision**: Store whole text and cursor snapshots, capped at 200 undo steps.

**Rationale**: SQL buffers are small enough that snapshots are easier to review
and restore than a diff engine. A cap prevents a long session from growing
without bound. Typing runs may coalesce, but movement must end a coalescing run
so edits on opposite sides of a move remain independently undoable.

## Decision 4: Treat Enter as a context-dependent action

**Decision**: Keep one terminal Enter action and route it by focus. In the
editor it must call `insert_newline()`, in the tree it opens or inserts an
object, and in the palette it confirms a command.

**Current gap**: `src/app/update.rs` currently calls `insert('\n')` in the
editor branch. The source follow-up must wire the existing indentation-aware
method before FR-506 can be checked.

## Decision 5: Keep syntax highlighting separate

**Decision**: Syntax colouring is not part of this feature's contract. The
editor must preserve exact source text and statement boundaries regardless of
presentation. Claude's active `src/query/*` work is tracked as a separate
follow-up slice.

## Open evidence

- Focused keymap coverage for the new movement, deletion and history bindings.
- A reducer test for Enter indentation and undo grouping across movement.
- A real terminal test for the eight-line write, correction and run scenario.
- A full verifier run after Claude's current source work is stable.
