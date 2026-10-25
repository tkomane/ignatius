# Guided-discovery contracts

These are user-visible contracts for first-frame guidance, contextual hints,
the general command palette, and empty-state recovery. Existing SQL, result,
history, export, and connection contracts remain authoritative.

## First frame

- A fresh frame names the connection state in words.
- An unusable connection does not advertise a runnable query.
- A usable connection with an empty buffer says where to type, how to run, and
  how to discover more actions.
- Guidance is inline and does not require a modal dismissal.

## Contextual hints

- The footer changes when focus changes between Editor, Results, and Objects.
- At most six comfortable-density or four compact-density hints are shown,
  superseding the original five-action bound under Feature 025's 2026-10-25
  presentation clarification.
- Each shown key is resolved by the active keymap, including configured
  replacements.
- Hints include action words and retain meaning without colour or Unicode.
- A busy Results pane prioritises cancellation; an empty or unusable pane names
  the prerequisite instead of implying progress.

## Command palette

- The general overlay is titled `Command palette` and states how to search and
  leave it.
- Entries are grouped by user intent and include actual keys or prerequisites.
- Search matches labels and details for run, inspect, help, objects, save, and
  result intents whenever those actions are applicable.
- Opening, searching, and dismissing emit no effect. Choosing an entry delegates
  to the existing reducer action.
- A search with no applicable match explains the missing prerequisite or gives a
  concrete next action; it never presents a blank unexplained surface.

## Empty and blocked states

- Editor, Results, Objects, no-match filters, no saved queries, failed
  transactions, and unusable connections each retain a readable recovery path.
- Failed transactions direct the user to rollback before suggesting another
  query.
- No result state is distinct from running, no rows, and filtered-empty.
- ASCII, no-colour, narrow, reduced-motion, and plain-adjacent output retains
  the same state and action meaning.

## Non-goals and safety

- Discovery never runs SQL, reloads metadata, writes files, records history,
  copies to the clipboard, contacts a cloud service, or persists onboarding
  state.
- It does not add mouse-only capability or alter scripted output.

## T042 scrollable help - 2026-10-25

Help is a local reading surface. Every active direct binding and chord, its
full key label and its complete description must be reachable without
horizontal clipping. Reuse display-width wrapping and density-aware overlay
geometry; long keys must not collide with action text. Reserve a fixed rail
with active scroll/close/palette keys and current position. At 40x8, keep a
readable body and the rail; at smaller/zero geometry retain a safe close route.
Use a usize wrapped-row offset, clamped by the same geometry in rendering and
the reducer. New help starts at the top; actual resize resets it. Tick and
unrelated async results preserve the offset.

Up/Down, PageUp/PageDown, Home/End and Ctrl+Home/Ctrl+End navigate only help.
Text, paste, deletion, undo/redo, focus and pane reshaping must not mutate the
covered editor/panes. Pasted text is ignored with the existing fixed feedback.
Escape, Enter and the active help key close help without editing or changing
an underlying plan/error/filter. Palette opening remains local and may cover
help; dismissing that palette returns to the same help position. The palette
is the searchable route to commands; no second search state is added here.
Quit/Cancel and other explicit bound commands retain their existing actions,
closing help before dispatch so it cannot keep covering the resulting state.
No navigation or ordinary typing produces a database/file/clipboard effect.

Match render/input precedence. Protected prompts/reviews, prefix, connection
details and palette keep their existing higher ownership. Help sits above
completion, filters, inspector and definition; opening help from an inspector
must display help and preserve the inspector underneath. Closing it restores
that owner. Do not bypass W04 sending gates or alter modal priority elsewhere.
The chord prefix also opens above help without closing it; dismissing the
prefix returns to help, and pane-resizing chords retain their modal no-op rule.
At narrow widths the rail may teach just the complete page-scroll keys; first/
last keys remain listed and functional in the body. Long valid control remaps
must retain a body at 40x8 rather than replacing the binding list with refusal.
Cancelling an unchosen palette preserves its help/inspector parent. A real
selection closes help before normal dispatch; direct object insertion or saved-
query opening reveals Editor and dismisses covered inspector/definition/
completion/filter input without clearing filter criteria. Selecting the Help
toggle from an already open help parent closes it rather than reopening it.
Use the runtime's active keymap snapshot for shared metrics, preserving the
existing default-only fallback for models without a snapshot. Displayed custom
key labels are never replaced with assumed defaults.
Space-key bindings must say `Space`, including `Ctrl+Space`, rather than an
invisible trailing character. This changes display labels only, not input keys.

Add failing-first real-buffer traversal and reducer cases for every entry,
long remaps, Unicode, both densities, dark/light/high-contrast and ASCII/no-colour,
40x8 and smaller, resize, nested owners, paste and no hidden SQL mutation.
Inspect actual styled buffers. Rendered-cell evidence remains separate from
native terminal appearance and hand-use acceptance.
