# Research: Explicit retained-result refresh

## Existing execution boundary

The reducer already retains the SQL template associated with the last completed
execution in `Model::last_sql`. Ordinary runs pass through one `run` and
`start` path, which applies connection, transaction, production, named-parameter
and history rules before returning an execution effect. Reusing that path keeps
refresh behaviour aligned with ordinary queries and avoids a second executor
contract.

## Design decisions

### Retained source, not editor source

Refresh reads `last_sql`, never the current editor buffer. This is the only
source that can truthfully claim to be the query behind the displayed result.
The action is therefore independent of editor edits, cursor movement and
formatting. The editor remains editable before and after refresh.

### Exactly one read-classified statement

The existing statement splitter is the right boundary for this feature because
it understands PostgreSQL strings, quoted identifiers, comments and dollar
bodies. Refresh requires exactly one returned statement and uses the existing
advisory classifier to require `Impact::Read`. Multi-statement input is refused
even when all statements appear to read, because replaying a submitted buffer
would be ambiguous. The classifier remains advisory and PostgreSQL remains the
authority for permissions and server behaviour.

### Reuse the named-parameter prompt

The retained source may contain named placeholders. Refresh sends no value from
the previous execution because the existing model keeps only the template and
prompts again. Empty text remains a valid value. The template is the history
and diagnostic source; prompted values stay behind the existing secret type.

### Explicit shortcut and palette entry

`F6` is the advertised configurable direct action because traditional terminals
can encode `Ctrl+Shift+R` as the same control byte as `Ctrl+R`. The enhanced
`Ctrl+Shift+R` alias remains available where the terminal preserves modifiers.
Neither key displaces `Ctrl+R` for running the editor buffer or consumes a
printable editor character. The command palette and contextual Results hints
use the action's name and the active keymap snapshot, so configuration cannot
make the shown key stale.

### No special refresh executor

No refresh effect, timer, cache or background watcher is needed. Once eligible,
the action enters the existing execution lifecycle. The result pane already
retains its last result while a new job is in flight, and a completed job uses
the ordinary selection, filter, grid, timing, cancellation, history and error
rules. A response is one explicit outcome and there is no automatic follow-up.

## Alternatives considered

- **Refresh from the editor**: rejected because the editor may have changed and
  would no longer identify the displayed result.
- **Replay every statement in the last buffer**: rejected because a retained
  result from a multi-statement submission does not identify which statements
  are safe to repeat.
- **Add a refresh timer or refresh on focus**: rejected by the no-silent-replay
  constitution principle and because it would make database work happen without
  a deliberate action.
- **Add a new runtime effect**: rejected because it would duplicate the existing
  parameter, cancellation, history and execution boundary.
- **Reuse `Ctrl+R`**: rejected because it would make the familiar Run buffer
  shortcut context-dependent and would make the direct action hard to explain.
