# Research: Copy a result value out

## Decision: Use the accepted OSC 52 write-only route

ADR-0013 accepts OSC 52 as the v1 copy transport because it works without a
platform clipboard dependency, including over SSH and in WSL under Windows
Terminal. The implementation writes the sequence to the already-held
interactive terminal and never reads the clipboard back.

Rationale: the product's most important target is a terminal session that may
not have a Linux display, while the terminal itself can bridge the request to a
host clipboard. A native clipboard crate would add platform dependencies and
would not solve the WSL case.

Alternative considered: a system clipboard crate such as `arboard`. Deferred
because it requires a platform display or native integration, has different
failure modes per operating system, and does not fit the SSH/WSL target. It can
be reconsidered as a separate transport behind the same safety contract.

Source: [ADR-0013: Copy through OSC 52, and only when asked for](../../docs/architecture/decisions/0013-copying-a-value-out.md), accepted 2026-08-16.

## Decision: Keep OSC 52 disabled unless the operator opts in

The default configuration is `[clipboard] osc52 = false`. An explicit
`[clipboard] osc52 = true` is required before the action can stage a copy. When
disabled, the action remains discoverable and reports the exact setting needed,
but emits no terminal bytes.

Rationale: the result value deliberately leaves the process through the
terminal, and may pass through SSH or a multiplexer. A safe default must require
the operator to accept that exposure rather than silently enabling a terminal
side effect.

Alternative considered: enable OSC 52 by default because many modern terminals
support it. Rejected because terminal support is not universal and the data
exposure is more important than reducing setup friction.

## Decision: Confirm with metadata, never with the value

The pending candidate stores only the result job identity, source row, source
column, UTF-8 byte count, and character count. The confirmation shows the
location and counts, explains terminal-path exposure, and says acceptance is
unconfirmed. At Enter, the reducer re-derives the value from the retained
result and validates the identity again.

Rationale: this avoids a second raw-value store and prevents the confirmation
overlay, Debug output, or effect assertions from becoming accidental data
surfaces. Re-derivation makes a result change while the prompt is open a stale
candidate instead of copying a value the operator is no longer looking at.

Alternative considered: copy the `String` into the model when opening the
confirmation. Rejected because it duplicates potentially sensitive data and
weakens stale-result detection.

## Decision: Bound raw UTF-8 payloads at 1 MiB

The client refuses any selected text whose UTF-8 byte length exceeds 1 MiB,
before constructing or writing an OSC 52 sequence. It does not attempt partial
copy. The refusal names the client limit and points to explicit export.

Rationale: terminal OSC 52 limits vary and often fail by truncating or ignoring
large sequences. A bounded client-side refusal keeps memory and terminal work
predictable and avoids claiming that a partial value was copied.

Alternative considered: rely on each terminal's limit or truncate to a
terminal-safe prefix. Rejected because the client cannot discover the terminal
limit and truncation would change the requested value.

## Decision: Encode the value with standard base64 and fixed OSC framing

The transport uses the write form `ESC ] 52 ; c ; <base64> BEL`. The result's
raw UTF-8 bytes are base64 encoded before writing, so control bytes and text
that resembles OSC syntax cannot terminate or alter the sequence. The encoder
is a small standard-library implementation to avoid a new dependency. The
payload type has redacted Debug output and is consumed at the write boundary.

Rationale: base64 is portable across terminal implementations and makes the
security property directly testable: only fixed framing may contain ESC or BEL,
and decoding the payload must reproduce the exact original bytes.

Alternative considered: quote or escape selected text directly inside OSC 52.
Rejected because terminal quoting rules are less uniform and would require
proving that every control byte is neutralized.

## Decision: Report bytes written, not clipboard acceptance

After writing and flushing, the runtime sends a completion containing byte and
character counts only. The UI says that the terminal sequence was sent and
clipboard acceptance is unconfirmed. Write or flush failure becomes a safe
diagnostic with an export-oriented next action. There is no read-back, polling,
automatic retry, or clear operation.

Rationale: OSC 52 is a one-way terminal protocol from the client's perspective.
The client knows whether its own I/O succeeded, but not whether the emulator
accepted, displayed, retained, or cleared the value.

## Decision: Keep the action outside machine output and database effects

Copy is an interactive-only action. It does not enter statement history, plain
output, JSON, NDJSON, exports, SQL text, or PostgreSQL execution. A copy from a
filtered or sorted grid uses the existing displayed-row to source-row mapping;
the retained result remains the only value source.

Rationale: copying is an intentional user data transfer, not a query result
format. Keeping it at the interactive reducer/runtime boundary preserves the
existing scriptable CLI and database semantics.
