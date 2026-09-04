# Contract: Explicit retained-result refresh

This is an interactive command contract. It applies to the full-screen client
only.

## Action

- **Name**: Refresh retained result
- **Direct key**: `F6` by default, configurable as `refresh-result`;
  `Ctrl+Shift+R` is an additional alias when the terminal preserves enhanced
  modifier reporting
- **Palette group**: Results
- **Availability**: Results focus, idle phase, no visible plan, usable
  connection, non-failed transaction, visible retained result and exactly one
  read-classified retained source statement

## Accepted flow

1. The user invokes the action.
2. The reducer reads the retained source template, not the editor.
3. If named parameters exist, the existing masked prompt asks for them again.
4. The existing execution effect is emitted once after all prompts and gates
   complete.
5. The Results surface reports running and then the actual result outcome.
6. The ordinary result lifecycle applies the new snapshot, counts, timing,
   local-view reset and history rules.

## Refused flow

If any availability or source rule fails, the reducer emits no execution effect,
does not alter editor text or retained result state, and provides a short
prerequisite or refusal. Unknown impact is not treated as read.

## Non-events

The following do not invoke refresh: startup, reconnect, resize, focus change,
filtering, sorting, column controls, inspector or expanded-row controls, plan
open/close, cell-update completion and a prior refresh outcome.

## Privacy and safety

Only the template crosses the ordinary execution and history boundary. Prompted
values use the existing secret binding type. The action is a convenience read,
not a permission bypass, and the advisory classifier is not a security control.
