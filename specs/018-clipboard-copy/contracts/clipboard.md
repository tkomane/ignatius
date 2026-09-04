# Clipboard interaction contract

This is an interactive terminal and reducer contract. It is not a new
machine-output or PostgreSQL protocol.

## Entry points

- `Copy selected value` is discoverable in the Results context and command
  palette when a retained text cell is selected.
- `Ctrl+K c` invokes the action. The printable `c` key remains SQL editor input.
- The same action is available with the inspector open and resolves the cell's
  retained source row and column.
- When no copyable cell exists, the action explains its prerequisite or gives a
  distinct NULL/oversize/disabled outcome rather than silently doing nothing.

## Configuration contract

```toml
[clipboard]
osc52 = false
```

The setting is backward compatible: an omitted section is equivalent to
`osc52 = false`. Only `true` permits a confirmed copy effect. The configuration
and UI state explain that terminal, SSH, multiplexer, and local applications
may observe or retain the value.

## Confirmation contract

1. The reducer validates focus, retained result, source cell, busy state, plan
   visibility, NULL state, size bound, and the opt-in setting.
2. For an enabled text cell, it opens a topmost confirmation without rendering
   the value. The overlay names the row and column, reports UTF-8 bytes and
   characters, states that the value will travel through the terminal path, and
   says acceptance is unconfirmed.
3. Escape and Cancel close the overlay without an effect. Enter rechecks the
   retained result job, source row, source column, and counts before creating
   the effect.
4. A stale candidate is rejected without transport. Typing and unrelated
   actions cannot leak through the confirmation overlay.

## Effect and runtime contract

- A valid confirmation produces exactly one `CopyValue` effect containing an
  in-memory redacted payload. It does not execute SQL, touch history, export
  rows, or alter result state.
- The runtime writes the OSC 52 set-clipboard sequence
  `ESC ] 52 ; c ; <base64(raw UTF-8)> BEL` to the existing interactive stdout
  and flushes it.
- The raw result never appears in the effect Debug representation or completion
  message. There is no clipboard read-back, polling, clear, retry, persistence,
  or telemetry.
- A flushed write produces a completion with byte and character counts and the
  phrase that terminal clipboard acceptance is unconfirmed.
- A write or flush failure produces a safe diagnostic with an actionable export
  or terminal-output next action and no raw value.

## Safety and compatibility contract

- NULL is never encoded as an empty string or literal `NULL`; empty text is a
  valid zero-byte value.
- The client refuses values above 1 MiB of raw UTF-8 bytes before transport and
  suggests explicit export.
- Base64 ensures result control bytes and OSC-looking text cannot become
  terminal syntax. Only fixed protocol framing contains ESC or BEL.
- Plain, JSON, NDJSON, history, SQL editing, database execution, and file export
  have no copy action or OSC 52 output.
- The current retained result remains the sole source of truth, including after
  filtering or sorting. A new result invalidates an open candidate.
