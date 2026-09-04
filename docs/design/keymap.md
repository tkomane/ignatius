# Keymap

The authority is `src/ui/keymap.rs`; this document mirrors it. Bindings are data,
checked for conflicts by a test, and listed in help at runtime.

The first-frame copy, contextual footer and command palette use a read-only
snapshot of the active keymap. The footer shows at most five complete
action/key pairs, selected for the current focus and state. A configured key
therefore replaces the default everywhere discovery names that action; the
command palette remains the route to actions that do not fit in the footer.

| Key | Action | Advertised |
| --- | --- | --- |
| `Ctrl+R` | Run the whole buffer | Yes |
| `Ctrl+T` | Run the statement at the cursor | Yes |
| `F6` | Refresh the retained result | Yes |
| `Ctrl+Shift+R` | Refresh the retained result in terminals with enhanced modifier reporting | No |
| `Ctrl+Shift+F` | Format the SQL buffer | Yes |
| `Ctrl+C` | Cancel the running statement | Yes |
| `Ctrl+Q` | Quit | Yes |
| `F1` | Show or hide help | Yes |
| `Ctrl+P` | Open the command palette | Yes |
| `Ctrl+Space` | Show schema completion | Yes |
| `Ctrl+B` | Show or hide the object tree | No |
| `Ctrl+K` | Start a chord and list what follows it | No |
| `Ctrl+Enter` | Run the whole buffer, where the terminal can distinguish it | No |
| `F5` | Run the whole buffer | No |
| `F9` | Run the statement at the cursor | No |
| `Ctrl+G` | Show or hide help, when F1 is unavailable | No |
| `Tab` | Move focus between panes | No |
| `Esc` | Dismiss help, then the current error | No |
| `Ctrl+D` | Expand or collapse error detail | No |
| `Up` `Down` `Left` `Right` | Move within the focused pane | No |
| `Enter` | Insert a line break in the editor, keeping its indentation | No |
| `Home` `End` | Start and end of the line | No |
| `Ctrl+Home` `Ctrl+End` | Start and end of the buffer, or the first and last row | No |
| `Ctrl+Left` `Ctrl+Right` | Move a word | No |
| `Alt+Left` `Alt+Right` | Move a word, where the terminal sends Alt for it | No |
| `PageUp` `PageDown` | Move by what the pane can show | No |
| `Delete` | Delete the character after the cursor | No |
| `Ctrl+W` `Alt+Backspace` | Delete the word before the cursor | No |
| `Ctrl+Z` | Undo | No |
| `Ctrl+Y` | Redo | No |
| `Backspace` | Delete the character before the cursor | No |
| `Enter` | Use what is selected: a line break, an object's name, or a confirmation | No |
| `/` | Filter what is in front of you: the rows when the results have focus, the object tree otherwise | No |
| Any printable key | Type into the editor | No |

## Chords

`Ctrl+K` opens a popup listing every key that can follow it. Nothing is on a
timer: a popup that disappears while being read is worse than one that waits.

| After `Ctrl+K` | Does |
| --- | --- |
| `b` | Show or hide the object tree |
| `p` | Open the command palette |
| `n` | Choose a connection |
| `g` | Open result grid controls when Results has focus |
| `l` | Show the estimated query plan |
| `a` | Measure the query plan (executes the statement) |
| `f` | Filter the object tree |
| `r` | Reload the object tree |
| `h` | Show or hide help |
| `x` | Expand the selected row down the screen |
| `i` | Inspect the selected cell |
| `s` | Search the statements that have run |
| `d` | Show the selected object's definition |
| `y` | Show what depends on the selected object |
| `w` | Save the buffer as a named query |
| `o` | Open a saved query |
| `e` | Write the rows on screen to a file |
| `v` | Pause or resume recording statements |
| `c` | Copy the selected result value |
| `u` | Generate a reviewed UPDATE from the selected cell |
| `q` | Format the SQL buffer |

When Results has focus, `Ctrl+K c` asks before sending the selected text cell
through the terminal. A NULL, disabled transport, stale selection, or value
over the copy limit is explained without sending anything. The printable `c`
remains editor input everywhere outside the chord.

When Results has focus, `Ctrl+K u` starts the guarded cell-to-`UPDATE` flow. The
first slice accepts only a direct single-table `SELECT` with a primary key in
the projection. It resolves the relation in the session's search path, refuses
production-classified and server read-only sessions, asks for a literal
replacement, then shows the exact bound statement. Enter on that review sends
one parameterized update; Esc sends nothing, and the prior result is never
rerun automatically. Printable `u` remains editor input outside the chord.

With the editor focused, `Ctrl+Shift+F` or `Ctrl+K q` formats the complete SQL
buffer locally. It changes only whitespace and layout outside strings, quoted
identifiers, dollar bodies, and comments; it is deterministic and one Undo
returns the whole previous buffer. An ambiguous or over-limit buffer stays
untouched and says what to correct. The printable `f` remains editor input.

With Results focused, `F6` refreshes a completed retained result only when its
source is exactly one read-classified statement and the connection is usable.
It sends the retained source, never the current editor buffer, so edits made
after the result do not change what is refreshed. Named parameters are prompted
again with the existing masked prompt. `Ctrl+Shift+R` is an additional direct
alias for terminals that preserve enhanced modifier reporting; traditional
terminals can encode it as `Ctrl+R`, so `F6` is the portable key shown in the
footer. The action is also in the command palette and the contextual Results
footer; it never runs on focus, filter, sorting, layout, reconnect or any other
passive transition. A failure states the actual outcome and does not retry.

When named profiles exist and no target or connection option was typed, the
first interactive frame opens the same picker. It lists a default route first,
then profiles in deterministic configuration order, with safe location, database, role, TLS,
environment, read-only and provider hints. Search is local and Enter is the
only point that starts resolution. Esc leaves the editor and connection state
unchanged. `Ctrl+K n` opens it again after a quiet session; a running query or
plan remains in control until it finishes.

Choosing a profile changes the connection, not the SQL buffer. The old server
facts, result, object tree and completion snapshot are cleared while local
reading preferences remain. The runtime receives only the profile name and
resolves credentials outside the application model. Passwords, tokens and
unknown profile fields never appear in the picker.

## Editing keys

Word movement is bound twice, with `Ctrl` and with `Alt`, because terminals do
not agree: Windows Terminal and most Linux emulators send `Ctrl` for that
gesture and macOS terminals conventionally send `Alt`. Binding both means the
key works where the user is rather than where the author was.

`Ctrl+Z` is undo rather than suspend. Raw mode turns off the terminal's own
signal handling, so the key arrives here as a key; a database client in the
middle of a statement is not something to suspend by reflex anyway.

One undo step is a word and the space after it. Character-at-a-time undo is
tedious and whole-buffer undo is frightening, and the word is the unit people
actually think in.

`Ctrl+Space` opens completion explicitly at the cursor. When automatic
completion is enabled, typing and deleting can also open the same local menu;
the menu never changes the buffer until Enter accepts the visible selection.
Esc dismisses it without changing the buffer, and Ctrl+Z undoes an accepted
replacement in one step.

`Ctrl+Shift+F` formats the complete editor buffer as one local edit. The footer
reports whether it changed, did nothing, or refused safely, and `Ctrl+Z` undoes
the complete formatting edit. `Ctrl+K q` reaches the same action when a
terminal or personal preference makes the direct shortcut inconvenient.

When Results has focus, `Ctrl+K g` opens the same searchable palette over the
current retained result. It lists local sorting, visible-column selection,
bounded width controls, type-label visibility, frozen-first-column layout, and
reset. Choosing a column opens a second searchable palette; duplicate labels
remain separate because each entry carries its source position. These actions
never rerun SQL.

With the editor focused, `Ctrl+K l` asks PostgreSQL for a structured estimated
plan for the statement under the cursor. `Ctrl+K a` opens a confirmation before
`EXPLAIN ANALYZE` executes that statement. The plan is read in the Results pane;
arrow keys move through it, Enter expands or collapses a branch, and Esc returns
to the prior ordinary result without a rerun. The plan actions are chords so
`l` and `a` remain ordinary printable editor input everywhere else.

## What a key means depends on where you are

`Enter` is the clearest case. In the editor it inserts a line break. On an object
in the tree it puts that object's quoted name where SQL is written, because that
is what you wanted it for. On a cell in the results it opens the inspector, which
is the only thing there is to do to a value. On a schema or a group it opens them, because a
container has no name worth pasting. In the palette it confirms.

The arrow keys always navigate: right opens a node, left closes it. That
separation means `Enter` never has to guess between "use this" and "open this".
In Results, left and right move through shown source columns, so hiding a
column cannot make the selected value silently change identity.

## Modes

The pending chord, palette, completion, filters, inspector, copy confirmation,
refresh parameter prompt, cell-update value prompt and cell-update review
intercept keys before the editor sees them. They are peeled by `Esc` in the
order they were opened. A cell update has two deliberate confirmations:
entering a replacement only prepares the review, and the review is the only
point that can emit the write effect.
Help and the current error are peeled after those. The inspector is the one that
does not swallow everything: typing goes nowhere, but a key that already means
something closes it and does that thing, so `Ctrl+R` still runs. The order is fixed and tested, because a key that
does something different depending on invisible state is how an interface becomes
untrustworthy.

## Why these

**`Ctrl+C` cancels rather than quits.** In a database client the statement is the
thing you urgently need to stop, and `Ctrl+C` is the reflex for that everywhere
else. Quitting is `Ctrl+Q`, which is deliberate rather than reflexive.

**Chords for execution, with a direct refresh key.** Function keys are routinely
claimed before a terminal program ever sees them: an operating system shortcut,
an assistant, a screen-recording tool. When that happens the user gets someone
else's feature instead of theirs, and there is nothing this program can do about
it from inside the terminal. Execution therefore advertises `Ctrl+R` and
`Ctrl+T`, which no platform claims by default. Refresh advertises `F6` because
traditional terminals can collapse `Ctrl+Shift+R` into `Ctrl+R`; the shifted
control form remains an extra alias where enhanced modifier reporting exists.

`F5` and `F9` remain bound for muscle memory from other database tools, and
`Ctrl+Enter` is bound for terminals that can distinguish it. Where a terminal
cannot, `Ctrl+Enter` arrives as a plain `Enter` and the binding simply never
fires, which is why plain `Enter` still inserts a line break.

**Help has two keys** for the same reason: `F1`, and `Ctrl+G` when something has
taken `F1`.

**`Esc` peels one layer at a time.** Help closes first, and the error stays,
because dismissing an error you have not read is the more expensive mistake.

**`Ctrl+K g` is contextual.** The result controls only appear when Results has
focus and a retained result exists. Their state is written in words - local
order, shown-column count, type availability, frozen-column state - so the
same workflow remains legible without colour or Unicode.

**The command palette answers a question.** It identifies itself as `Command
palette`, searches labels and intent details, groups applicable actions, and
keeps a prerequisite beside an unavailable chord. Opening, searching and
dismissing it do not execute a query or change the editor.

**No modal editing.** There is no normal mode to be trapped in. Printable keys
type. Anyone who has never used Vim can use this on the first try.

## Key release events

Terminals that speak the Kitty keyboard protocol, including Warp, can send press
and release events. Only presses are acted on. Without that, every statement
would run twice.

## Configurable bindings

A `[keys]` table in `config.toml` replaces the built-in binding for an action:

```toml
[keys]
run-buffer = ["f2", "ctrl+r"]   # several keys, or one
quit = "ctrl+x"
```

Naming an action removes its defaults entirely, so what the file says is what
the keyboard does for that action. Everything else keeps its defaults.

**Three things are errors, not things to ignore**: an action name this build does
not know, a key it cannot read, and two actions on one key. The file's whole
purpose is to say what the keyboard does, so a line in it that quietly does
nothing would be worse than no file. Each is reported with what to do about it,
before the terminal is taken, and exits 3.

Keys are written case-insensitively as optional `ctrl+`, `alt+` and `shift+`
modifiers followed by one key: a single character, `f1` to `f24`, or one of
`esc`, `tab`, `enter`, `backspace`, `delete`, `insert`, `home`, `end`, `pageup`,
`pagedown`, `up`, `down`, `left`, `right`, `space`.

The bindable action names are the authority in `src/ui/keymap.rs`:
`run-buffer`, `run-statement`, `refresh-result`, `cancel`, `quit`, `toggle-help`,
`focus-next`, `format-buffer`,
`toggle-error-detail`, `dismiss`, `toggle-sidebar`, `open-palette`,
`open-result-grid`,
`begin-prefix`, `start-filter`, `reload-objects`, `complete`, `show-definition`,
`show-dependencies`, `save-query`, `open-query`, `export-rows`, `open-history`,
`toggle-history-recording`,
`toggle-expanded-row`, `toggle-inspector`, `undo`, `redo`, `delete-forward`,
`delete-word-left`, `move-line-start`, `move-line-end`, `move-buffer-start`,
`move-buffer-end`.

Movement keys are not bindable. They mean different things in each pane, and
rebinding them one at a time would produce an interface nobody could describe.
