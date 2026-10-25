# Contract: Execution-wide retained-result data allowance

Specified for implementation on 25 October 2026, Africa/Johannesburg. This is
W10-013 under foundation REL-002, not an implemented or measured product limit.
W10-007 showed that row caps do not bound a single earlier driver allocation;
W10-011 distinguished small-row backpressure from a large value's allocations.
The primary adopts a separate application-owned retained-data allowance below.
W10-012 notice accounting and driver/startup allocation remain separate.

## Allowance and exact charge

A buffered Session execution has one **33,554,432-byte (32 MiB) logical
occupied-data charge** shared across every result set in that Execution.
No CLI row-cap override disables it. Existing per-set row caps, including the
CLI's zero-row count-only behavior, remain unchanged. There is no new flag,
configuration setting, dependency or persistent state in this slice.

For admitted described result data, charge the actual target's native sizes:

```text
size_of::<ResultSet>() for each available retained result set
+ column_count * (size_of::<String>() + size_of::<Option<String>>())
+ sum(retained column-name UTF-8 bytes)
+ sum(available retained type-name UTF-8 bytes)
+ retained_row_count * size_of::<Vec<Cell>>()
+ sum(retained row cell_count * size_of::<Cell>())
+ sum(retained Cell::Text UTF-8 bytes)
```

NULL/empty text still consume Cell slots; every retained row consumes a row
vector slot; every column consumes its name/type slots even when a name is empty
or a type is unavailable. Names, cells and types are charged before their new
application-owned copies. All additions/multiplications use checked arithmetic;
overflow means admission cannot be proven and is refused, never wrapped.
Charge persists across ownership transfers and successful statement completions.

This is a named logical occupied-data allowance, not measured RSS or allocator
capacity. It excludes spare capacities, allocator overhead, driver frames/type
caches, SQL/bindings, notices, output/display copies and Execution/StatementResult
control vectors. Count-only unavailable-result control records are outside the
retained-data charge. Many statements can still grow that control plane. Old
results and a replacement in flight can coexist. Public synthetic ResultSet/
Execution construction is not a universal bound; the guarantee is real buffered
Session ingestion. Streaming file exports keep complete values and do not use
this buffered allowance.

32 MiB is a conservative engineering default that prevents the previous
10,000-times-1-MiB retained-text combination. It is not a measured UX optimum.
Native slot sizes can yield different retained prefixes on different targets;
record target/toolchain when comparing them. No whole-process, incoming-frame,
startup-backlog, metadata-cache or row-counter-overflow guarantee is added.

## Admission and PostgreSQL outcome

1. Begin one budget before the first buffered statement. Borrow driver names
   and compute the complete essential-description charge before copying any
   name or allocating aligned type slots. Admit the entire ordered description
   or none. Do not shorten or invent column names to fit.
2. While a row is a candidate under the existing row cap and data retention is
   open, inspect all borrowed cells first. Preserve the existing 1-MiB retained-
   cell refusal before aggregate admission, including the mixed case where that
   same row would also overflow the aggregate budget. This existing refusal's
   diagnostic/completed-prefix behavior is unchanged.
3. Compute the complete candidate row charge before copying cells or allocating
   its owned cell vector. Admit the entire row or none. Never retain a partial
   row, shortened cell or later smaller row in place of the prefix.
4. The first essential-description or aggregate-row refusal latches data
   admission closed for the remainder of this Execution. Continue consuming and
   counting rows through real CommandComplete and remaining explicitly requested
   statements. No later small result replenishes the allowance. Row-cap-only
   dropping does not close the byte allowance for later statements.
5. Once row retention is closed by the row cap or byte policy, do not inspect
   discarded values merely to create new per-cell errors. Driver allocation can
   still precede this decision. Count delivered rows without an empty placeholder
   row accidentally entering the retained prefix.
6. Optional describe/type labels remain nonfatal. Charge the complete aligned
   type-name payload before copying it; if it cannot fit, keep aligned None
   labels and do not guess, shift, evict rows or change the query outcome. A
   declined optional type payload does not itself close later data admission.
   Skip optional description when the essential description is unavailable or
   data admission is already closed; never rerun SQL to regain metadata.
7. True server error/cancellation/connection loss retain their existing
   precedence, SQLSTATE, completed-prefix, transaction and unknown-outcome facts.
   Do not invent a successful result for the failing statement. Successful
   DML RETURNING keeps CommandComplete/rows_affected and genuine server status
   even when client retention loses rows or description. Retention loss is not
   evidence that a write failed, did not happen or rolled back.
8. Send no CANCEL, SET, COMMIT, ROLLBACK, retry or SELECT replay because of this
   limit. Buffered per-statement send and parameter-semantics guards remain
   unchanged; whole-message streaming remains unchanged. Ordinary successful
   execution keeps exit zero. Choose streaming before a future execution when
   complete output is needed; never advise automatically rerunning a completed
   statement, especially a write.

## Typed result state and unavailable description

Use typed retention reasons to distinguish ordinary row-cap truncation,
aggregate-data truncation and essential-description unavailability. Keep actual
row_cap, delivered rows_seen, affected rows and source/job identity. A missing
essential description retains only fixed reason, known source column count and
delivered row count; no names/types/rows are retained for it. This is distinct
from a zero-row result whose description is available.

A known description with a retained prefix remains a normal ResultSet. The
shared window/filtered labels must say `retained-result limit reached` for the
byte reason rather than falsely saying the configured row cap was reached.
Description-unavailable state uses fixed wording `Result data unavailable -
retained-result limit reached` plus known source-column/received-row counts.
It must never draw an empty grid/no-results instruction that implies the query
returned no answer or no rows. A real server failure/unknown-outcome panel
retains priority. No new modal or acknowledgement is added for ordinary result
retention; the result pane/summary owns these facts.

## CLI and machine-output contract

For available descriptions, all admitted data encoding, column order, NULL,
empty text, duplicate-name behavior and schemas stay unchanged. W10-009's exact
retained/received stderr line remains valid for known-description row prefixes
in CSV/TSV/NDJSON/Markdown/INSERT. Table/JSON keep existing in-band counts.
Truncation by bytes changes the retained prefix, not the representation of any
admitted value. No client status becomes a data row.

For an unavailable essential description, JSON includes one result-set object
in its original position, with this explicitly new case:

```json
{
  "columns": [],
  "rows": [],
  "rows_seen": 7,
  "truncated": true,
  "data_unavailable": "retained_result_limit",
  "source_column_count": 3
}
```

Empty arrays here mean no description/data was retained, not that the server
returned zero columns/rows. The explicit flag and source count carry that
meaning. These two additive fields appear only in this new unavailable case;
ordinary available-result JSON bytes/schema remain unchanged. `truncated` also
covers description unavailability, including zero received rows. This narrowly
amends Feature 021 FR-2110 for an explicitly unretained description.

CSV/TSV/NDJSON/Markdown/INSERT write no invented header/data for an unavailable
set; retain existing set-separator/ordinal rules and emit exactly:

`Result set 2 data unavailable: retained no description for 3 source columns and 7 received rows because the retained-result limit was reached. For a future complete streaming export, choose --output before execution.`

The ordinal excludes statements with no result set, as in W10-009. Do not emit
the ordinary retained-row truncation line a second time for that unavailable
set. Table reports unavailability/counts in-band; JSON uses the explicit object,
without an extra text status that would violate its single diagnostic object
on failure. NDJSON still refuses multiple result sets before writing anything,
counting unavailable result sets too. A successful prefix before a later real
error stays exactly the existing prefix/error contract. The unavailable object
is not a fabricated successful result for the failing statement.

Write statuses after data output succeeds. Preserve intentional stdout
BrokenPipe success and ordinary reporting-I/O failure boundaries. Keep W10-012
notice JSON/error/accounting semantics, including one JSON diagnostic object,
independent from retained-result facts. No output retry or stdout fallback.

## TUI and retained-data actions

Render typed unavailability in the Results pane in Full/Compact, both densities,
normal/zoom and after returning from Plan, ASCII/no-color and 40x8. Preserve query/transaction and
real error facts. Existing view state, source identity and explicit refresh
rules remain unchanged; an unavailable result is not an implicit refresh.
Known available prefixes continue to support filtering, sorting, inspection,
copy and export with the true retained/received boundary.

An unavailable description must refuse copy (including headers), cell update,
cell inspection and retained-row file export before any clipboard/file/SQL
effect. Give fixed value-free guidance instead of copying an empty invented
header or writing a file that looks like a real zero-row result. Revalidate
availability at runtime file/copy boundaries as well as pure intent handling.
Do not change the complete streaming export route or its W10-010 progress/flush
contract. Existing pending review invalidation and generation/job rules remain.

## Implementation order and acceptance

One integration owner serializes the shared Session/result/runtime/UI seams
with W10-012. Begin full implementation only after the notice source is
integrated and its relevant combined gate is saved. A later worker may prepare
an isolated pure budget helper/test design first; no concurrent Session edits
or canonical changes. Use the existing stack and sidecar tests. Keep public
synthetic constructors useful; document the ingestion boundary precisely.

Watch meaningful focused failures before repair: small-budget exact/one-over
headers/whole rows, checked overflow, NULL/empty/wide/multibyte slots, no partial
copies, cumulative multi-set closure, optional-type unavailability, row-cap
precedence, oversized-cell mixed precedence and no placeholder-row retention.
Prove available/unavailable zero-row distinctions and every machine output,
prefix/error, view/action refusal, stale identity and protected review behavior.

After pure checks, bounded existing synthetic fixtures may verify a 40-row
1-MiB-value result, a two-result cumulative overflow followed by `SELECT 42`,
row-cap-zero control and a complete streaming control. No individual result
payload exceeds 64 MiB, 30-second child deadline, 512-MiB completed-RSS and 256-MiB output
experiment stops, isolated state/absent passfile and environment-only dummy credentials.
Read authenticated version/orders3 before/after and preserve fixture lifecycle.
A bounded temporary-table RETURNING case may be specified by the primary only
if needed; no persistent data/schema or unrelated backend is changed here.
Do not substitute SELECT counts for evidence about a database write.

The primary reviews the concrete patch, full required verifier, affected
plain/verify-full TLS PostgreSQL evidence and actual rendered buffers before
closing W10-013. Repeat server-major acceptance after Session changes; pinned/
MSRV/Linux and native Windows/hand-use remain their separate gates. Three
resource contracts do not imply a whole-process ceiling or release readiness.
