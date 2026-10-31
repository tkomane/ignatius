# Terminal compatibility

## One abstraction

All output and input go through Crossterm. Win32 console calls are never mixed
with virtual terminal sequences, because mixing the two is where Windows terminal
bugs come from. Reference: <https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences>

## What is turned on, and turned off again

| Mode | Default | Note | Sequence |
| --- | --- | --- | --- |
| Alternate screen | On | Active during session | `ESC[?1049h` / `ESC[?1049l` |
| Bracketed paste | On | Active during session | `ESC[?2004h` / `ESC[?2004l` |
| Cursor visibility | Hidden | Restored on exit | `ESC[?25l` / `ESC[?25h` |
| Mouse capture | **On** | By default; disable with ui.mouse = false | `ESC[?1000h` / `ESC[?1000l` |
| Raw mode | On | Active during session | Crossterm |

Mouse capture defaults to on because pointer interactions and mouse verbs are a
first-class path in the full-screen client. Turning mouse capture on takes away
the terminal's own default text selection, but holding Shift while clicking or
dragging restores terminal-native text selection in supported terminals. In-app
copying of retained text cells remains the explicit, opt-in OSC 52 route
(`[clipboard] osc52 = true`). Mouse capture can be disabled entirely by setting
`ui.mouse = false` in `config.toml`, and disabling it removes no interactive
capability: every action remains reachable by keyboard.

**Mouse capture is the one mode that is not an escape sequence everywhere.** On
Windows, Crossterm enables and disables it through the console API rather than by
writing `ESC[?1000h`. The call is made and undone exactly as on Unix; only its
observability differs, which is why the ordering test asserts the sequence order
on Unix and the remaining modes everywhere. This was found by the Windows CI
runner, not by reasoning about it.

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

### Colour depth detection precedence

Terminal colour depth is determined through a strict five-tier precedence:

1. **None**: Forced when `NO_COLOR` is present, `TERM=dumb`, `--plain` is
   passed, or `ui.color = "never"` is configured. All colour emission is
   disabled, resolving tokens to styling modifiers and text.
2. **Explicit overrides**: The `--color-depth` command-line flag takes precedence
   over configuration, followed by `ui.color-depth` in `config.toml` (`truecolor`,
   `256`, or `16`). An explicit setting of `auto` defers to environment
   detection below.
3. **TrueColor**: Detected when the `COLORTERM` environment variable equals
   `truecolor` or `24bit`. Emits direct 24-bit RGB escape sequences.
4. **Indexed256**: Detected when `TERM` contains `256color`. Uses the deterministic
   6x6x6 color cube and greyscale ramp, never emitting indices 0-15.
5. **Basic16**: The universal fallback when none of the above match. Maps tokens
   through the fixed 16-colour ANSI table.

### OSC 11 background luminance probing

Automatic terminal background luminance detection via OSC 11 (`ESC]11;?ESC\`) is
architecturally designed (D-2505) but remains an open implementation item
(`T032`). Because unconfirmed terminal queries can cause escape-sequence
echoing or hang on unsupported emulators, automatic probing is not active, and
configuring `ui.theme = "auto"` is explicitly refused during configuration
validation with a message naming the supported choices (`dark`, `light`,
`high-contrast`, `custom`).

## Sizes

| Size | Behaviour |
| --- | --- |
| 80x24 and larger | Full layout |
| Smaller than 80x24, at least 40x8 | One pane at a time |
| Smaller than 40x8 | A message with the current size, the minimum, and `ignatius query` |

Resizes are ordinary messages into the reducer, so a burst of them changes only
the recorded size and cannot corrupt any other state.
