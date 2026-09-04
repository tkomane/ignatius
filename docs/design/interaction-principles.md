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

**Say what is true, including when that is "unknown".** `Cancellation requested`
is a different state from `Query cancelled by server`, because between them the
statement may still be running. `TLS state unknown` is a real answer.

**Progressive disclosure.** An error shows four short lines by default and hides
the server's technical fields behind one key, with a count so you know they exist.

**Trust is inspectable.** The connection header answers the glance question; the
palette's connection details surface answers the careful question. It names the
target, server posture, TLS state, and credential route without refreshing,
executing, or exposing a token. Inspection is read-only, dismissible with one
key, and safe to open before a consequential query.

**Density with room to breathe.** Every element earns its space. The header
carries connection, environment, posture, transport and timing because a user
running statements against a real database needs all five without opening a
panel. It carries nothing else.

**Feedback is immediate and honest.** Pressing cancel changes the status
immediately, because the request was made immediately. It does not claim the
outcome, because the outcome is not known yet.

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

## Motion

There is almost none, and what exists can be turned off with
`ui.reduced-motion`. Progress is text, because text is readable by a screen
reader, survives a slow terminal, and does not imply precision that is not there.
