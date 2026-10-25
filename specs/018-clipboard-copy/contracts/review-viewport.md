# Contract: Review content before interactive sending

**Pinned 2026-10-25 for W04, Feature 018 T037-T040.** This repairs reproduced
clipping in existing confirmations. It introduces no transport, SQL capability,
credential route, configuration key or persistence.

## Scope and authority

The shared rules apply to the final OSC 52 copy confirmation, generated UPDATE
review, ordinary production-run confirmation, EXPLAIN ANALYZE confirmation and
each interactive named-parameter acceptance. They clarify Feature 018 FR-004,
Feature 023 FR-2308/2309, Feature 016 FR-006/007, Feature 022 FR-2204 and the
constitution's truthful/private/safe defaults. Existing typed production-name,
literal binding, stale-candidate, job/connection identity and one-effect rules
remain required. Plain/CLI routes retain their existing contracts.

## One readable review surface

- Keep required review content in a vertically scrollable body. Never discard
  its tail or silently replace an exact generated UPDATE with a preview.
- Preserve the existing family width limits and density's preferred outer
  clearance. Height may grow beyond the former fixed family height to fit the
  content, within the available terminal. Reduce clearance as already specified
  when interaction rows need it. Plan output remains a Results pane.
- Reserve a visible command rail, and a separate visible input row for typed
  production names or masked parameter values. These do not scroll with the
  body. Show review navigation, cancellation and the current acceptance state
  as complete key/action pairs. Use active keymap labels and width-aware
  wrapping; no clipped key may imply that acceptance is available.
  Reserve the greater rail height required by blocked and eligible wording, so
  changing that wording cannot remove a body row while granting acceptance.
  Use `Model::keymap_snapshot` for identical reducer/render admission metrics;
  preserve the existing default fallback only for models without a snapshot.
  At inner widths below 50 cells, use compact navigation wording with standard
  PgUp/PgDn abbreviations for the existing PageUp/PageDown keys (including their
  modifiers) and `top/end` for first/last. Preserve actual remapped keys. Measure
  this wording after the shared width is known so a default 40x8 parameter or
  typed-name review can retain its input and two body rows; verbosity alone
  must not force an enlarge refusal. Larger/remapped layouts retain the same
  stable-rail and no-clipping requirements.
- Reuse existing move/page/first/last actions for local review navigation.
  They never affect the editor, retained rows, hidden palette or database.
  Home/End resolve to line-start/line-end actions by default; Ctrl+Home/End
  resolve to buffer-start/buffer-end. Support these existing first/last review
  intents and teach the active binding rather than assuming a new action.
  Ordinary text/backspace still reaches only the owning input field. Paste
  retains its existing masked-input or ignored-confirmation behavior.
- Wrap by terminal display width, preserving all sanitized content and explicit
  line breaks. Renderer, navigation bounds and acceptance use one pure geometry
  and wrapping calculation. Do not use a line offset whose integer range can
  silently make a tail unreachable.
- Below 40 columns or 8 rows, or if the available rectangle cannot show the
  required input/command rail plus at least two body rows, refuse acceptance
  with a visible enlarge-or-cancel explanation where space permits. Escape,
  Cancel and Quit retain their current meanings even at zero-size geometry.

## Acceptance and lifecycle

- New candidates start at the first review row. Each new parameter name starts
  a fresh review at row zero. Typed or pasted values do not become review text.
- A body that fits is immediately eligible for the existing explicit acceptance.
  If it overflows, acceptance is blocked until the **current viewport** reaches
  the final wrapped row. The rail names the review action while blocked and
  the send/execute/accept action when eligible. Returning to an earlier page
  blocks acceptance again. This proves reachability and an explicit final-page
  action, not that a person read or understood the content.
- An actual terminal-size change resets the current review to row zero and
  recomputes the same metrics. A short body that now fits can be accepted; a
  long body must reach its final page again. Tick and unrelated async messages
  do not reset navigation. Presentation changes use current geometry and must
  never authorize an unreachable tail.
- Both Activate and RunBuffer must pass this gate before accepting a parameter
  or producing any clipboard/SQL effect. A refused acceptance keeps the
  candidate and input. Repeated attempts do not execute or bypass the gate.
  Gate before consuming a candidate, accepting an answer, allocating a job or
  changing running state. Bound the visible mask to the field width without
  changing the retained secret value or exposing it in review text.
- After the viewport gate, re-run all existing identity, connection, posture,
  typed-name and size checks immediately before the existing one-shot effect.
  No stale state becomes valid through scrolling or resizing.
- A matching asynchronous cell-update metadata response must not put a
  replacement prompt above a newer input/review owner. After validating request
  identity, discard only that older lookup when another prompt, review, prefix,
  palette, help or pane input owner exists. Preserve the current owner and its
  input; show a value-free instruction to finish/cancel it and generate the
  UPDATE again. Do not queue the response or automatically restart metadata/SQL.
  Existing busy, plan, connection and source/selection guards still apply.

## Runtime size synchronization

`Model::size` must reflect the actual rendered frame before its first paint and
after every observed frame-size change. Before reducing input actions, obtain
fresh backend dimensions and synchronize through the existing resize message;
do not let a stale queued resize payload replace those current dimensions.
Only send a resize transition when the observed size actually changes. Failure
to obtain dimensions must refuse sending before reducer mutation. Quit, Cancel
and Dismiss retain their escape/recovery behavior even if size cannot be read.
This prevents a narrow displayed review being admitted using the model's
initial 80x24 default. The runtime owns terminal I/O and supplies the fact
message `InputAtSize { action, columns, rows }`; the pure reducer owns the
review admission decision and modal precedence. No new Action is introduced.
Test the actual draw/input boundary with Ratatui's
test backend, including first-frame geometry and stale resize events, rather
than treating manually synchronized model/buffer tests as runtime evidence.

If fresh backend size changes immediately before Activate/RunBuffer while a
sending review owns input, synchronize size but suppress that acceptance once
without consuming its candidate or answer. Paint the new review first; the
next explicit acceptance follows the normal viewport gate. This prevents a
formerly clipped review becoming eligible on enlargement before any new frame
was painted. Ordinary execution without a review owner and all recovery keys
keep their existing behavior. This does not prove a person read the frame.

## Required body content

| Surface | Content that remains reachable |
| --- | --- |
| Copy | Selected source row/column, UTF-8 byte and character counts, terminal path/destination, terminal/SSH/multiplexer observation or retention, write-only/no read or clear, acceptance unconfirmed. Never the result value. |
| Generated UPDATE | Qualified target and column, selected retained row, every primary-key column, complete exact bound SQL under the existing 16 KiB bound, literal-data meaning, nothing sent yet, no automatic source refresh. |
| Production run | Explicit PROD/target, advisory impact, the existing first-four-line source preview with an explicit indication when more source exists, database-permissions limitation, required database name and typed matching where already required. |
| EXPLAIN ANALYZE | Execution, possible side effects, no automatic rollback, target, existing source preview with an indication when truncated, advisory classification/permissions and existing typed-name requirement. |
| Named parameter | Full current name, position/count, literal-text/empty-text meaning, unchanged template/no value history. Keep the answer masked and separate from the body. |

## Privacy and proof

Store navigation state only, never expanded SQL or answers in displayable model
debug state. Regenerate intentional UPDATE review text only within the current
in-memory review boundary; reuse the existing escaping, redaction and secret
types. Clipboard metadata remains value-free. No new log, history, file,
clipboard or database activity is authorized by rendering/navigation.
Preserve existing Analyze preview/target/required/typed-name and copy-target
redaction through the existing single redactor; extracting shared content must
not replace those boundaries with control escaping alone. The exact generated
UPDATE remains intentionally value-visible at its separate review boundary.

Write failing-first renderer/reducer cases for all five surfaces, including
40x8 and smaller, long/wide/control-containing names, long exact UPDATE SQL,
both densities, active keymaps, ASCII/no-colour and resize after reaching the
end. Prove the complete sanitized body is traversable, blocked acceptance has
zero effects, cancellation has zero effects, existing typed/stale/posture guards
still hold and one eligible confirmation has exactly one effect. Inspect actual
rendered buffers. Focused/full/native/hosted and participant evidence remain
separate; no real clipboard or production write is needed for these proofs.
