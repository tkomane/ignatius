# Terminal compatibility

## One abstraction

All output and input go through Crossterm. Win32 console calls are never mixed
with virtual terminal sequences, because mixing the two is where Windows terminal
bugs come from. Reference: <https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences>

## What is turned on, and turned off again

| Mode | Default | Sequence |
| --- | --- | --- |
| Alternate screen | On | `ESC[?1049h` / `ESC[?1049l` |
| Bracketed paste | On | `ESC[?2004h` / `ESC[?2004l` |
| Cursor visibility | Hidden | `ESC[?25l` / `ESC[?25h` |
| Mouse capture | **Off** | `ESC[?1000h` / `ESC[?1000l` |
| Raw mode | On | Crossterm |

Mouse capture is off by default because capturing the mouse takes away the
terminal's own text selection, which people rely on to copy a value. Turning it
on is a configuration choice and removes no keyboard capability.

Restoration happens in exactly the reverse order, with the alternate screen left
last so the restored cursor and paste state apply to the user's own screen. This
is enforced by a test that asserts the ordering, and confirmed by a captured pty
session in `docs/operations/verification.md`.

## Exit paths that must restore

| Path | Mechanism | Verified |
| --- | --- | --- |
| Normal quit | RAII guard, plus an explicit restore before any final error | Yes, pty capture |
| Error during the session | RAII guard | Unit test |
| Panic | Panic hook installed before the terminal is taken | Unit test of the sequences |
| SIGTERM | Signal watcher turns it into an ordinary quit | Unit test of the sequences |
| Windows console close | Ctrl+C handler turns it into a quit | Not verified |

## Warp

Warp supports alternate-screen applications, configurable mouse and scroll
reporting, and progressive Kitty keyboard protocol handling. Reference:
<https://docs.warp.dev/terminal/more-features/full-screen-apps>

The consequence that matters is key release events: with the Kitty protocol,
terminals send both press and release. Only presses are acted on, so a statement
runs once. Everything else is standard terminal behaviour, and richer input is
treated as progressive enhancement rather than assumed.

## Detection, and overriding it

Detection is deliberately shallow, because confident wrong answers are worse than
admitted uncertainty:

- **Is it a terminal**: asked of stdout directly.
- **Size**: read from the terminal, and absent when there is none.
- **Unicode**: inferred from `LC_ALL`, `LC_CTYPE` or `LANG` containing UTF-8, and
  assumed on Windows where UTF-8 is the modern default.
- **Colour**: `NO_COLOR` and `TERM=dumb` always win over configuration.

Every one of these can be overridden: `--ascii`, `--plain`, `--color`, and the
`ui` section of the configuration. `doctor` reports what was detected so a user
can see why the interface chose what it chose.

## Sizes

| Size | Behaviour |
| --- | --- |
| 80x24 and larger | Full layout |
| Smaller than 80x24, at least 40x8 | One pane at a time |
| Smaller than 40x8 | A message with the current size, the minimum, and `ignatius query` |

Resizes are ordinary messages into the reducer, so a burst of them changes only
the recorded size and cannot corrupt any other state.
