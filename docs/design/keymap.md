# Keymap

The authority is `src/ui/keymap.rs`; this document mirrors it. Bindings are data,
checked for conflicts by a test, and listed in help at runtime.

| Key | Action | In footer |
| --- | --- | --- |
| `F5` | Run the whole buffer | Yes |
| `F9` | Run the statement at the cursor | Yes |
| `Ctrl+C` | Cancel the running statement | Yes |
| `Ctrl+Q` | Quit | Yes |
| `F1` | Show or hide help | Yes |
| `Tab` | Move focus between panes | No |
| `Esc` | Dismiss help, then the current error | No |
| `Ctrl+D` | Expand or collapse error detail | No |
| `Up` `Down` `Left` `Right` | Move within the focused pane | No |
| `Enter` | Insert a line break in the editor | No |
| `Backspace` | Delete the character before the cursor | No |
| Any printable key | Type into the editor | No |

## Why these

**`Ctrl+C` cancels rather than quits.** In a database client the statement is the
thing you urgently need to stop, and `Ctrl+C` is the reflex for that everywhere
else. Quitting is `Ctrl+Q`, which is deliberate rather than reflexive.

**Function keys for execution.** They do not collide with typing SQL, they need
no chord, and they work identically in Warp and Windows Terminal. `F5` matching
other database tools is a bonus, not the reason.

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
