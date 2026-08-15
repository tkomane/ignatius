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
| `Enter` | Insert a line break in the editor | No |
| `Backspace` | Delete the character before the cursor | No |
| `Enter` | Use what is selected: a line break, an object's name, or a confirmation | No |
| `/` | Filter the object tree, when it has focus | No |
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

## What a key means depends on where you are

`Enter` is the clearest case. In the editor it inserts a line break. On an object
in the tree it puts that object's quoted name where SQL is written, because that
is what you wanted it for. On a schema or a group it opens them, because a
container has no name worth pasting. In the palette it confirms.

The arrow keys always navigate: right opens a node, left closes it. That
separation means `Enter` never has to guess between "use this" and "open this".

## Modes

Three states intercept keys before the editor sees them, peeled by `Esc` in this
order: a pending chord, the palette, then the object filter. Help and the current
error are peeled after those. The order is fixed and tested, because a key that
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

Not yet. The structure is ready: bindings are data with descriptions and a
conflict checker that returns findings rather than panicking, so configuration
can be validated with a clear message. Wiring it to the configuration file is
Feature 007.
