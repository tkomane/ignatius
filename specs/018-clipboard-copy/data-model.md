# Data model: Copy a result value out

## Clipboard preference

The persisted, non-secret operator preference controlling OSC 52 writes.

| Field | Meaning | Validation/default |
| --- | --- | --- |
| `osc52` | Whether an explicitly confirmed copy may be written through the terminal | Boolean; defaults to `false` when omitted |

The preference is nested under `[clipboard]` in the existing schema-version-1
configuration. Unknown fields remain rejected by the existing configuration
parser. No credential, result value, terminal capability, or clipboard content
is stored.

## Copy candidate

An ephemeral confirmation identity for one displayed result cell.

| Field | Meaning | Validation |
| --- | --- | --- |
| `result_job` | Job identity of the retained result | Must equal the current retained result job at confirmation |
| `source_row` | Row in the retained result before display filtering/sorting | Must be a valid source row |
| `column` | Source column index | Must be a valid source column containing text |
| `bytes` | UTF-8 byte count known at candidate creation | Must equal the current cell's raw byte length |
| `characters` | Unicode scalar-value count for display | Must equal the current cell's character count |

The candidate never stores the raw value. The current result grid maps the
displayed selection to `source_row`, so the same identity is used whether the
view is filtered, sorted, or opened in the inspector.

## Copyable cell

The value selected for transport from the retained result.

| Cell state | Copy behavior |
| --- | --- |
| `Text(value)` where `value.len() <= 1 MiB` | Eligible; empty text remains a valid zero-byte payload |
| `Text(value)` where `value.len() > 1 MiB` | Refused before transport; suggest explicit export |
| `Null` | Refused with a distinct SQL NULL explanation; never converted to text |
| Missing source row/column or empty filtered view | No payload; explain that no copyable cell is selected |

Counts are computed from the raw cell text, not terminal-sanitised display
text. The payload therefore preserves the original UTF-8 bytes even when the
visible rendering replaces control characters.

## Clipboard payload

An in-memory, single-use transport value created only after confirmation and a
fresh candidate check.

| Property | Meaning |
| --- | --- |
| raw bytes | Exact UTF-8 bytes of the selected `Text` cell |
| size bound | At most 1 MiB |
| debug representation | Redacted; never displays raw bytes or text |
| lifetime | From reducer effect creation through one runtime write, then dropped |

The payload is not persisted, logged, added to history, or retained in the
model after the effect is consumed.

## Clipboard outcome

The user-visible result of an attempted action. It carries no raw value.

| Outcome | Terminal write | User-facing meaning and next action |
| --- | --- | --- |
| Disabled | None | Set `[clipboard] osc52 = true` if the operator accepts terminal-path exposure |
| NULL | None | SQL NULL has no text value; select a text cell or use export |
| Too large | None | Value exceeds the 1 MiB client bound; use explicit export |
| Stale | None | Result changed or candidate is no longer valid; select the current cell and retry |
| Cancelled | None | Confirmation dismissed; result and selection remain |
| Sent, acceptance unconfirmed | One write and flush | Sequence was sent; terminal/clipboard acceptance cannot be verified |
| Write failed | Partial or no observable terminal write | I/O failed; inspect terminal output or use explicit export, with no automatic retry |

## State transitions

```text
eligible cell
    │ Copy selected value
    ├── disabled ───────> Disabled notice
    ├── NULL ────────────> NULL notice
    ├── oversized ───────> Too-large notice
    └── enabled ─────────> Confirmation(candidate)
                               │
                               ├── Escape/Cancel ─────> Cancelled / retained result
                               ├── stale at Enter ────> Stale notice / no write
                               └── valid Enter ───────> Effect(payload)
                                                            │
                                                            ├── I/O error ──> Write-failed diagnostic
                                                            └── flushed ────> Sent, acceptance unconfirmed
```

Only the valid Enter transition creates a payload or terminal effect. A fresh
result or a new query clears any pending candidate and the previous copy notice
as part of the existing job lifecycle.
