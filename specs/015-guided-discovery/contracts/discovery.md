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
- At most five contextual hints are shown.
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
