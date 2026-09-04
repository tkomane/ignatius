# Research: Guided discovery

## Decision: Derive guidance from the existing model

**Rationale**: Focus, connection state, query phase, editor contents, result
presence, filters, and object-tree state already live in `Model`. A pure derived
context keeps guidance truthful without introducing a second lifecycle or a
stale onboarding flag.

**Alternatives considered**: A persisted first-run tour was rejected because it
would add state the user must manage, complicate privacy, and make a later empty
state less helpful. A separate event-driven discovery cache was rejected because
it would duplicate model state and create another stale-result boundary.

## Decision: Let the active keymap remain the only key authority

**Rationale**: `Keymap` already validates configuration, resolves actions, and
provides human-readable key labels. Contextual hints should filter and order
those bindings, never copy hard-coded key strings into the UI. This also means a
configured replacement appears consistently in the footer and palette.

**Alternatives considered**: A feature-local map of friendly shortcuts was
rejected because it would drift from configuration and could advertise a key
that no longer invokes the action.

## Decision: Keep the hint rail bounded and focus-specific

**Rationale**: The current footer is global and can show actions that do not
apply to the pane under focus. A small ordered list per context gives the user a
next action without turning the footer into a second help screen. Five entries
is the maximum in the specification and the renderer truncates to the available
width while retaining the action words.

**Alternatives considered**: Showing every binding was rejected because it
creates noise and makes the next action harder to find. Hiding the footer was
rejected because focus would become visible without being actionable.

## Decision: Contextual palette entries are filtered, not disabled controls

**Rationale**: The existing palette has no execution model for disabled entries.
Filtering to actions that can safely do something now avoids a selectable no-op.
When a user searches for an unavailable capability, the empty state names the
missing prerequisite and a nearby global action remains available. Actions such
as help, focus, quit, and the palette itself remain discoverable whenever they
are meaningful.

**Alternatives considered**: Adding disabled entries was rejected for this slice
because it would require a new selection and activation contract, including how
disabled entries behave in ASCII and screen readers. Silently leaving all
existing entries was rejected by the feature requirement that the palette answer
what can be done here.

## Decision: Improve empty states in place and keep them non-modal

**Rationale**: The editor, results, objects, and existing error surfaces already
own the space where the user needs the next action. Adding a short heading,
state-specific sentence, and one or two real key paths there gives onboarding
without interrupting typing or requiring a dismissal decision.

**Alternatives considered**: A welcome dialog was rejected because it hides the
working surface and is hostile to repeated sessions, automation, and screen
readers. A separate tutorial command was deferred because the first frame must
already explain itself.

## Decision: No new dependency or I/O boundary

**Rationale**: The work is copy, ordering, and filtering over data already in
memory. Existing `ratatui`, `crossterm`, reducer, and renderer seams are enough.
No query, catalogue load, file, history, clipboard, subprocess, telemetry, or
cloud path is needed to discover a command.

**Alternatives considered**: A fuzzy-search dependency was rejected because the
existing palette scorer is already deterministic and sufficient; a help-file
loader was rejected because discovery must work before any extra file is read.
