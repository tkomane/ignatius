# Keymap

The authority is `src/ui/keymap.rs`; this document mirrors it. Bindings are data,
checked for conflicts by a test, and listed in help at runtime.

| Key | Action | Advertised |
| --- | --- | --- |
| `Ctrl+R` | Run the whole buffer | Yes |
| `Ctrl+T` | Run the statement at the cursor | Yes |
| `Ctrl+C` | Cancel the running statement | Yes |
| `Ctrl+Q` | Quit | Yes |
| `F1` | Show or hide help | Yes |
| `Ctrl+P` | Open the command palette | Yes |
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

## What a key means depends on where you are

`Enter` is the clearest case. In the editor it inserts a line break. On an object
in the tree it puts that object's quoted name where SQL is written, because that
is what you wanted it for. On a cell in the results it opens the inspector, which
is the only thing there is to do to a value. On a schema or a group it opens them, because a
container has no name worth pasting. In the palette it confirms.

The arrow keys always navigate: right opens a node, left closes it. That
separation means `Enter` never has to guess between "use this" and "open this".

## Modes

Four states intercept keys before the editor sees them, peeled by `Esc` in this
order: a pending chord, the palette, the object filter, then the cell inspector.
Help and the current error are peeled after those. The inspector is the one that
does not swallow everything: typing goes nowhere, but a key that already means
something closes it and does that thing, so `Ctrl+R` still runs. The order is fixed and tested, because a key that
does something different depending on invisible state is how an interface becomes
untrustworthy.

## Why these

**`Ctrl+C` cancels rather than quits.** In a database client the statement is the
thing you urgently need to stop, and `Ctrl+C` is the reflex for that everywhere
else. Quitting is `Ctrl+Q`, which is deliberate rather than reflexive.

**Chords for execution, not function keys.** Function keys are routinely claimed
before a terminal program ever sees them: an operating system shortcut, an
assistant, a screen-recording tool. When that happens the user gets someone
else's feature instead of theirs, and there is nothing this program can do about
it from inside the terminal. So the advertised keys are `Ctrl+R` and `Ctrl+T`,
which no platform claims by default.

`F5` and `F9` remain bound for muscle memory from other database tools, and
`Ctrl+Enter` is bound for terminals that can distinguish it. Where a terminal
cannot, `Ctrl+Enter` arrives as a plain `Enter` and the binding simply never
fires, which is why plain `Enter` still inserts a line break.

**Help has two keys** for the same reason: `F1`, and `Ctrl+G` when something has
taken `F1`.

**`Esc` peels one layer at a time.** Help closes first, and the error stays,
because dismissing an error you have not read is the more expensive mistake.

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
`run-buffer`, `run-statement`, `cancel`, `quit`, `toggle-help`, `focus-next`,
`toggle-error-detail`, `dismiss`, `toggle-sidebar`, `open-palette`,
`begin-prefix`, `start-filter`, `reload-objects`, `show-definition`,
`show-dependencies`, `save-query`, `open-query`, `export-rows`, `open-history`,
`toggle-history-recording`,
`toggle-expanded-row`, `toggle-inspector`, `undo`, `redo`, `delete-forward`,
`delete-word-left`, `move-line-start`, `move-line-end`, `move-buffer-start`,
`move-buffer-end`.

Movement keys are not bindable. They mean different things in each pane, and
rebinding them one at a time would produce an interface nobody could describe.
