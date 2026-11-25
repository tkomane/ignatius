# Contract: Operational resource bounds and slow-sink measurement seam

**Status**: Accepted (`W10-008`)  
**Authorities**: `specs/001-foundation-vertical-slice/plan.md` (`W10-007`..`W10-013`), `contracts/notice-retention.md`, `contracts/retained-result-data.md`, `docs/operations/verification.md`.

---

## 1. Purpose and scope

`W10-007` proved that a row-count cap alone (`--max-rows` / `max_rows_in_memory`) does not bound three distinct resource routes:
1. Cumulative server notice retention across a statement stream.
2. Aggregate retained result-set bytes across wide rows or multiple statements in one buffered execution.
3. Pre-retention wire framing and buffering inside `tokio-postgres` before a `SimpleQueryMessage::Row` reaches application code.

This contract specifies the distinct operational bounds across all three routes, pins the slow-export-sink measurement seam established in `W10-011`, and unifies the truthful refusal, truncation, unknown-outcome and recovery wording across CLI, plain and interactive surfaces.

---

## 2. Distinct operational bounds by route

| Route | Enforced bound | Authority & implementation | What remains outside this bound |
| --- | --- | --- | --- |
| **Notice retention (`REL-005`, `W10-012`)** | At most `256` notices and `1,048,576` bytes (`1 MiB`) of retained UTF-8 fields (`severity`, `message`, `code`) per operation; individual messages shortened at `65,536` bytes (`64 KiB`); sticky parameter-privacy scrubbing (`"notice omitted for parameter privacy"`). | [`contracts/notice-retention.md`](notice-retention.md), `src/postgres/session.rs`, `src/query/result.rs` | Wire-level notice frame reading before `NoticeGuard` inspection. |
| **Aggregate retained-result bytes (`REL-002`, `W10-013`)** | Checked `33,554,432`-byte (`32 MiB`) logical occupied-data allowance shared across all buffered `ResultSet` values in one `Execution`, charging native struct/slot sizes and UTF-8 bytes before copying. Whole-description or whole-row admission latches a retained prefix while continuing to drain and count `rows_seen`. | [`contracts/retained-result-data.md`](retained-result-data.md), `src/query/result.rs`, `src/postgres/session.rs` | Pre-retention `tokio-postgres` row-frame buffers, output/display formatting copies, and metadata caches. |
| **Single-cell & input payload caps** | A buffered row that is still a candidate for retention is refused if any cell exceeds `1,048,576` bytes (`1 MiB`). Generated cell-update parameters are refused above `16,384` bytes (`16 KiB`). Bracketed paste and local SQL formatting are refused above `1 MiB`. Plan retention is bounded at `1 MiB` and `500` nodes. | `src/query/value.rs`, `src/query/result/retention_budget.rs`, `src/query/update.rs`, `src/app/editor.rs`, `src/query/format.rs`, `src/query/plan.rs` | Rows dropped after the row cap or aggregate allowance is exhausted still count as received. Streaming `--output` exports keep complete values. |
| **Incoming driver frames and read buffer** | The application caps govern copies and retention after the driver delivers a row or notice. | `vendor/tokio-postgres/src/codec.rs`, `vendor/tokio-postgres/src/connection.rs`, `src/postgres/session.rs` | Driver read and response buffers, including complete messages queued for a consumer. See the boundary below. |

### Incoming driver allocation boundary

The vendored `tokio-postgres` codec can group multiple complete messages already
in its read buffer into one response batch. It emits the complete prefix before
an incomplete frame and leaves the incomplete bytes in the buffer. Asynchronous
messages are emitted separately, and `ReadyForQuery` marks the response complete.
The driver can hold a batch waiting for the response receiver. Buffered
ingestion applies the client allowances after delivery. `Session::stream` hands
complete values to the export sink one row at a time without constructing a
retained result set. Startup and TLS negotiation have their own driver and
transport allocations.

W10-007 observed `~18.5 MiB` peak RSS for an `8 MiB` value dropped at the row-cap
boundary on its measured revision. The current source has no adopted incoming
frame, driver-buffer or whole-process memory ceiling. A future wire limit needs
a specific requirement and must define what happens when a valid row or COPY
data exceeds it. Its verification needs to check the bound before body
accumulation, preserve complete export below the limit, and cover startup and
query diagnostics, partial export, write-outcome uncertainty, cancellation and
reconnection over TCP, TLS and Unix sockets.

The [retained-result contract tests](../../../tests/retained_result_contract.rs)
cover retained prefixes across result sets, count-only dropping of an oversized
value, real server errors after a retained prefix, and complete output through
the streaming path. These cases exercise the application boundary.

---

## 3. Slow-export-sink measurement seam (`W10-011`)

A slow database server (`pg_sleep`) tests upstream wait state, not client backpressure. To measure real consumer backpressure without modifying production CLI flags or transport code:

1. **Seam location**: Wrap the synchronous `std::io::Write` destination inside `Export::writer()` (above `BufWriter<File>`) with a deterministic rate-limiting writer (`PacedWrite`).
2. **Pacing mechanics**:
   - Maintain monotonic scheduling with zero initial byte credit and at most `8 KiB` (`8,192` bytes) delegated per `write()` call.
   - Account only actual accepted bytes and handle short or `ErrorKind::Interrupted` writes without losing pacing state across rows.
   - Count a row as exported only after complete encoder write success (`W10-010`).
3. **Observed boundary (`W10-011`)**:
   - Under `1 MiB/s` accepted-byte pacing across `8,192` small rows (`8 MiB` total CSV), peak RSS remained flat (`6.812 MiB` paced vs `6.906 MiB` unpaced), proving TCP/driver backpressure propagates from the synchronous writer to the socket read loop.
   - Under `1 MiB/s` pacing of a single `8 MiB` cell value, peak RSS reflected only the single in-flight row/encoder allocation (`40.828 MiB`), never unbounded multi-row accumulation.
   - Because synchronous `Write` blocking occupies the current-thread executor between poll points, top-level CLI `Ctrl+C` responsiveness during a blocked local disk write remains a separate runtime boundary and is never claimed from pacing evidence alone.

---

## 4. Truthful refusal, truncation, unknown-outcome and recovery dictionary

Every resource boundary must state what happened, what was kept or sent, and what next action recovers cleanly:

| Boundary | Surface | Exact wording / contract |
| --- | --- | --- |
| **Row-cap buffered truncation (`W10-009`)** | CLI `CSV`, `TSV`, `NDJSON`, `Markdown`, `INSERT` stderr | `Result set {ordinal} truncated: retained {retained} of {rows_seen} received rows. For a future complete streaming export, choose --output before execution.` |
| **Execution-wide 32 MiB row prefix (`W10-013`)** | CLI / TUI / Plain result summary | Latches the retained row prefix, continues counting `rows_seen`, and states that retention stopped at the `32 MiB` client data limit (`retained_data_loss`). |
| **Execution-wide 32 MiB description refusal (`W10-013`)** | JSON `data_unavailable`, CLI stderr, TUI results pane | JSON sets `"data_unavailable": "retained_result_limit"` and `"source_column_count": N`; TUI/CLI refuse cell copy, inspector, cell update and retained export with an explicit unavailability diagnostic naming `--output` before execution. |
| **Notice allowance exhaustion (`W10-012`)** | JSON `notice_report`, Plain stderr, TUI footer & notice reader | Reports exact `omitted`, `shortened` and `abandoned` counts; TUI keeps a persistent pending notice indicator until explicitly acknowledged in the notice reader. |
| **Connection loss during in-flight write or refresh** | TUI status / CLI diagnostic | Never retries automatically; states that the connection was lost, the server outcome is unknown (`Outcome::Unknown`), and nothing was retried (`"The connection was lost while refreshing; the outcome is unknown and nothing was retried."` / `"The connection was lost; no cell update was retried."`). |
| **Oversized paste (`US1`) or format buffer (`019`)** | TUI footer notice / CLI refusal | Refuses payloads above `1 MiB` (`1,048,576` bytes) before mutating the buffer and names the `1 MiB` limit. |
| **Oversized cell update (`023`)** | TUI / CLI refusal | Refuses replacement values above `16 KiB` (`16,384` bytes) before generating or sending `UPDATE`. |
