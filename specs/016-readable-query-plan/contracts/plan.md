# Plan interaction contract

This is a terminal and reducer contract, not a new machine-output interface.

## Entry points

- `Explain plan` is discoverable from the command palette and the keymap chord
  popup. It acts on the one statement under the cursor.
- `Analyze plan` is discoverable beside it and always opens a confirmation
  before any database effect.
- Both entries explain their prerequisite when there is no usable connection,
  no statement, a busy query, a failed transaction, or an already-wrapped
  EXPLAIN statement.

## Request and completion

1. The reducer creates a job identity and returns a plan-specific effect.
2. The runtime asks the current PostgreSQL session for structured EXPLAIN data.
3. The reducer accepts only the completion matching the active plan job.
4. A successful completion opens the plan in Results and preserves ordinary
   retained-result state.
5. A failed, malformed, truncated, cancelled, or connection-lost response is
   labelled with a truthful next action and is never presented as a successful
   plan.

## Visible node contract

Every visible node has:

- operation name;
- relation or index context when supplied;
- estimated rows and startup/total cost when supplied;
- observed rows, time, and loops only for analyzed output;
- an explicit `not supplied` or `not measured` label for absent values;
- a text expense or estimate-mismatch label when derived;
- selected styling plus a text selection cue in ASCII and no-colour modes.

## Safety and machine boundary

- Plain planning does not execute the target statement.
- Analysis says that it executes the target, may have side effects, and is not
  automatically rolled back. Confirmation is explicit and one-shot.
- Navigation, expansion, collapse, and dismissal are local only.
- The plan request is absent from user history and all non-interactive output
  contracts. Normal query, JSON, NDJSON, streaming export, and plain paths are
  unchanged.
- All server text passes through the existing redaction and terminal-safe
  display helpers.
