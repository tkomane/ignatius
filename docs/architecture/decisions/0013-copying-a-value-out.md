# ADR-0013: Copy through OSC 52, and only when asked for

- Status: Accepted on 2026-08-16.
- Date: 2026-08-16
- Related: ADR-0002 (ratatui and crossterm), `specs/004-result-inspection/spec.md`

## Context

Copying a value out of the result pane is the last unbuilt part of Feature 004.
It was gated rather than built because it is the one action in the product that
moves a customer's data out of this process on purpose, and the two ways to do
it fail differently.

**The system clipboard**, through a crate such as `arboard`, talks to the
platform: NSPasteboard on macOS, the Win32 clipboard on Windows, and X11 or
Wayland on Linux.

**OSC 52** writes the value into the terminal as an escape sequence and asks the
terminal to put it on the clipboard. It needs nothing installed and works over
SSH, because the sequence travels with the session.

## What decided it

The environment this is actually used in. The owner's target is WSL under
Windows Terminal, and that is the case the two options separate on:

- A Linux clipboard crate needs an X11 or Wayland display. A plain WSL shell has
  neither, so the system-clipboard route is not degraded there - it is absent,
  and the copy key would simply not work on the platform it is most wanted on.
- Windows Terminal implements OSC 52 for writing to the clipboard, enabled by
  default, with a profile setting to turn it off. Reading the clipboard was
  deliberately never implemented, on the grounds that handing an application the
  clipboard without the user knowing is a security hole. That asymmetry is
  exactly the right one for this product: write is what a copy key needs, and
  read is what it must never have.

Checked against Microsoft's terminal repository and release notes on 2026-08-16;
sources at the end.

## Decision

1. **Copy is implemented with OSC 52 and no new dependency.**
2. **It is off unless configuration turns it on.** `[clipboard] osc52 = false` is
   the default. The reason is not caution for its own sake: the sequence carries
   the value through the terminal emulator, which may log it, and over SSH it
   crosses every hop in between. A person who wants that should have said so.
3. **The refusal is informative.** With copy off, the key says the value was not
   copied, says the setting that would allow it, and says what the setting means
   - not "copy failed".
4. **Nothing is ever read back.** Ignatius never queries the clipboard, so the
   sequence used is the write half of OSC 52 only.
5. **The confirmation states the size**, because a large value silently
   truncated by a terminal's own limit would be the interface lying about what
   happened, which principle I forbids.
6. **A clipboard crate is deferred, not rejected.** If someone runs this on a
   Linux desktop with a display and wants the native route, that is a second
   implementation behind the same key and the same setting, decided when a user
   exists rather than now.

## Consequences

- Copy works identically over SSH and in WSL, which is where terminal clients
  are used, and does not work in a terminal that has the sequence disabled -
  which the interface has to say rather than fail silently on.
- `docs/security/data-handling.md` gains a row: a value the user asked for
  leaves the process through the terminal, opt-in, never automatically.
- No new dependency, and the binary stays self-contained, which matters for the
  same reason it mattered in ADR-0012.

## Sources

Checked 2026-08-16:

- [Support xterm VT sequence (OSC 52) for setting the clipboard - microsoft/terminal #2946](https://github.com/microsoft/terminal/issues/2946)
- [Add support for OSC 52 (copy-to-clipboard) - microsoft/terminal b24579d](https://github.com/microsoft/terminal/commit/b24579d2b04bcbf177c513e4d1885d12511b3aee)
- [Windows Terminal Preview 1.23 release notes](https://devblogs.microsoft.com/commandline/windows-terminal-preview-1-23-release/)
