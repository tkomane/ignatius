# Contract: Bounded notice retention and visible client accounting

Adopted for implementation on 25 October 2026, Africa/Johannesburg, against
application inputs `d8a3ac34ea5a69fb957b04f94c5c90ffa27a1cef`. This pins the
W10-012 slice under foundation REL-005. It is a requirement, not a claim that
the behavior or acceptance already exists. W10-007 measurements and the
W10-008 backlog/frame investigation inform the chosen engineering limits.

The primary selected 256 admitted entries and 1 MiB of retained UTF-8 fields
per observation window, with visible accounting and unchanged database facts.
This addresses the measured notice-count growth without an application-owned
large-message copy. It does not bound earlier driver allocation, all result
data or process RSS. Those W10-008 contracts remain separate and open.

## 1. Smallest complete scope

Bound application notice admission after driver connection, including idle and
metadata traffic. Keep existing successful `StatementResult.notices`; remove
silent pre-statement clears. Carry a typed report on every ordinary execution,
stream and plan outcome, including failure. Deliver client accounting in CLI,
plain mode and a persistent TUI pending surface/explicit reader. The report is
client-authored, never a forged server `Notice`.

Each execute/stream/explain operation gets **256 admitted entries and 1,048,576
retained UTF-8 bytes**, cumulatively across all statement drains and internal
probes. Severity, retained message and retained SQLSTATE all count. Keep the
existing 65,536-byte message rule and its UTF-8/suffix behavior. Admission is an
ordered prefix: after count or byte exhaustion, copy no later notice text and
count omissions. A shortened message counts as shortened only if admitted.
Check all lengths before application-owned clones. No automatic SQL, setting,
cancel, rollback, retry or replay follows exhaustion.

Idle storage has the same pair, not one allowance per metadata query. Metadata
does not acquire the user-SQL gate or gain new causal attribution: notices
observed during an active user operation consume that operation's allowance;
otherwise they consume the idle bucket. Existing concurrent metadata behavior
is unchanged. This is an observation-window contract, not statement provenance.

On a new user operation, adopt pending idle entries into the active bucket and
charge their retained fields against its allowance. Do not first drain them
into an external report and grant a second allowance. An idle payload drain is
allowed only while no operation is active. Draining active statement payload
does not restore count/byte spending. Prior completed reports, old retained
results, two connections and downstream ownership are explicit multiplicities,
not a whole-process memory limit.

## 2. Exact report and API contract

Put value-only report types in `query/result.rs`; private accumulator/guard in
`postgres/notices.rs` (new sidecar tests). No generic database abstraction.

```rust
pub enum NoticeCount { Exact(u64), AtLeast(u64), Unknown }
pub struct NoticeCounts {
    pub omitted: NoticeCount,       // rejected at admission, not missing rows
    pub shortened: NoticeCount,     // admitted messages shortened at 64 KiB
    pub abandoned: NoticeCount,     // admitted prefix payload lost on future drop
}
pub enum NoticeScope { Query, Export, Plan, Connection }
pub struct NoticeCheckpoint {
    pub revision: u64,              // saturating, never wrap
    pub totals: NoticeCounts,       // connection-lifetime, client counters only
}
pub struct NoticeReport {
    pub scope: NoticeScope,
    pub retained: Option<u16>,      // Some(0..=256), None if accounting unavailable
    pub counts: NoticeCounts,       // this report's accounting, not cumulative
    pub unattributed: Vec<Notice>,  // bounded payload not in StatementResults
    pub checkpoint: NoticeCheckpoint,
}
pub struct Reported<T> {
    pub value: T,
    pub notice_report: NoticeReport,
}
```

`retained` includes entries moved into successful statement results; it does
not promise that `unattributed.len()` equals the count. Unavailable accounting
uses None (JSON null), never a fabricated zero. Shortened and omitted
are independent. `abandoned` is separate because a dropped future did not
necessarily hit the retention limit. Normal counters start `Exact(0)`;
overflow becomes `AtLeast(u64::MAX)`. Poisoned/unavailable accounting is
`Unknown`, never an empty/default-success report. Counter arithmetic and
checkpoint deltas are pure helpers: subtract exact baselines only; when a
saturated/unknown baseline prevents an exact delta, report `Unknown` rather
than inventing a difference. A revision that saturates cannot be treated as
proof that subsequent snapshots are unchanged.

Advance the checkpoint revision for every actual committed observation window,
including a returned zero-loss window, so older zero-loss summaries cannot
replace newer ones. Empty idle polling advances nothing. A normal older revision
or an equal-revision conflicting checkpoint is stale and must not overwrite
pending counts, the baseline or latest summary. Keep saturation/unknown handling
conservative; a saturated revision is not proof that no new loss occurred.

First-party boundaries:

```rust
Execution { /* existing fields */, notice_report: NoticeReport }
Session::stream_reported(...) -> Reported<Result<u64, StreamStop>>
Session::stream_with_parameters_reported(...) -> Reported<Result<u64, StreamStop>>
Session::explain_reported(...) -> Reported<PlanResponse>
pub struct PlanResponse {
    pub result: Result<String, Diagnostic>,
    pub transaction: TransactionState,
    pub connection_lost: bool,
}
Session::take_connection_notice_report() -> Option<NoticeReport>
Session::notice_checkpoint() -> NoticeCheckpoint
Session::notice_retirement_counts() -> NoticeCounts
connect_reported(...) -> Reported<Result<Session, Diagnostic>>
```

`execute*` retains its current return type plus the additive field. Move the
already-existing plan post-request transaction probe from runtime inside the
reported plan operation; do not add another probe. Keep `StreamStop` and its
server/sink distinction unchanged. Keep existing public stream/explain/connect APIs as thin compatibility
adapters, while all first-party consumers use the report-carrying forms. Their
API documentation must identify the payload-only reporting boundary. `take_notices` must no longer drain active storage; retire
it internally and migrate the existing privacy test to the typed idle drain.
A compatibility method, if retained, must be documented as payload-only and
must not clear accounting or active spending.

`take_connection_notice_report` returns no active payload. Checkpoints remain
available during active work and after report transfer. They retain only fixed
counters, so a missed/dropped completion does not erase known client loss and
does not create a completed-report queue. Operation reports move payload once;
checkpoint repetition is deduplicated, never added again by consumers.
Checkpoint totals finalize on operation finish, idle report transfer or guard
Drop, subject to the pending-shortening rule below. Polling during active work
returns the last committed checkpoint; it does not publish a fresh per-notice
report, reset spending or promise immediate mid-operation warning delivery.
Pending active accounting remains bounded. The distinct nonmutating retirement
snapshot includes currently observed uncommitted counts before an intentional
source retirement; it is not a normal polling checkpoint or budget reset.

On successful connect, keep startup/bootstrap notices in the idle bucket for
the first operation; return an empty connect report instead of draining and
replenishing. On a post-establish bootstrap failure, return the observed bucket
with the original failure. Before driver connection succeeds there is no
application inbox: the report covers observed application messages only, not
unseen driver startup history.

## 3. Guard, privacy and transfer lifecycle

1. Acquire the existing user-SQL gate. Begin one guard identified by a private
   token; no mutex guard survives an await. Adopt idle payload/accounting once.
2. Parameterized entry atomically activates sticky privacy **before adopting
   idle text and before the first semantics probe/bind/send**. This intentionally
   activates privacy even if local validation later fails; pin that stricter
   timing. Rewrite all accumulator-owned queued text through the existing
   private representation and safe SQLSTATE helper. No second redactor.
   Charge the fixed representation, never raw parameter-derived lengths.
   Unpublished shortening metadata from rewritten backlog is not reported as
   private message length evidence. Already delivered ordinary facts cannot be
   withdrawn. Privacy remains sticky until physical disconnect.
3. Successful buffered statements take only the active statement inbox after
   the actual row stream ends and before optional prepare/describe starts. The
   guard records how many entries/bytes have moved, preserving spending. Move
   notices observed during initial/inter-statement semantics probes and optional
   prepare/describe into the bounded unattributed payload using in-memory phase
   transfers. Idle prelude and final completion/probe/failure notices remain
   unattributed. This separates client observation phases, not server causation;
   add no SQL, wait or causal fence. Metadata sharing the same connection can
   still overlap a phase and must not be claimed as proven statement provenance.
4. Finish after the existing final transaction probe, on every return path,
   including empty SQL, binding/semantics refusal, server error, cancellation
   and connection loss. Move remaining payload into `notice_report`, record
   fixed cumulative checkpoint counters, disarm guard, then release SQL gate.
5. Stream completion/shape refusal/sink error/server error all finish the same
   guard. Do not split the existing whole Query message or wait for unrelated
   messages to invent a causal fence. Later messages enter the idle bucket.
6. If the future is dropped before finish, guard Drop performs no I/O: terminate
   that active ownership, preserve pending bounded payload in the idle bucket,
   and count already-drained prefix payload that the abandoned future will drop
   as `abandoned`. Commit omission/abandonment and any shortening of already-
   drained lost prefix. Keep shortening of untransferred pending payload with
   that bounded payload, uncommitted until ordinary transfer: a first privacy
   rewrite can suppress that unpublished metadata without decrementing a
   cumulative published total. Track trusted per-entry shortening provenance,
   never infer it from server-controlled suffix text. Ordinary later transfer
   commits the pending shortening exactly once. Transfer cannot allocate a
   second unbounded bucket; excess is counted. Starting a later *new* operation
   is an explicit new allowance; ordinary drains never reset an active allowance.
   Do not reset privacy. Abandonment is neither confirmed cancellation nor rollback.
7. Reports transferred to an external caller are caller-owned; no API can prove
   that caller displayed/read them. Checkpoints preserve known client loss even
   when a completed outcome is dropped. No delivery guarantee on forced process
   death, abort, unobserved driver messages or unavailable output.

Never classify a report failure as query failure or change completed-prefix,
SQLSTATE, cancellation-request/confirmation or transaction/unknown-outcome facts.

## 4. CLI JSON schema and exact text

Emit accounting only if omitted/shortened/abandoned is nonzero or unknown. Do
not begin printing retained server prose for successful scripted queries.
All stdout result bytes/schema remain unchanged, including JSON/Table counts.

Canonical additive `notice_report` JSON object:

```json
{
  "scope": "query",
  "observed_only": true,
  "retained": 256,
  "omitted": { "count": 4, "precision": "exact" },
  "shortened": { "count": 1, "precision": "exact" },
  "abandoned": { "count": 0, "precision": "exact" },
  "limits": { "count": 256, "text_bytes": 1048576, "message_bytes": 65536 }
}
```

Allowed scope strings: `query`, `export`, `plan`, `connection`. `precision` is
`exact`, `at_least` or `unknown`; unknown uses `count:null`. The `retained` value is a number or null when unavailable. No SQL, target,
server prose, severity, original length, hashes or parameter/name text.

- Successful `query --format json`: one stderr line
  `{"kind":"notice_retention","source":"client","notice_report":{...}}`.
  This is a status object, no error `exit_code`, and success stays 0.
- Existing JSON database error: add `notice_report` to the **same** existing
  `Diagnostic::to_json()` value in `cli/mod.rs`; preserve every existing field,
  kind, SQLSTATE and exit code. No separate status object/text before or after it.
- Non-JSON query formats/plain/export: one fixed line per nonzero category:
  `Client omitted N observed server notices after reaching the notice-retention limit.`
  `Client shortened N retained server notice messages.`
  `Client did not return N retained server notices because an operation was abandoned.`
  For lower bounds substitute `at least N`; for unknown substitute `an unknown
  number of` (use singular grammar-independent wording above).
- Nontruncated/nonshortened/ordinary zero reports add no status. A report never
  says rows were lost, a query failed, all warnings were delivered or memory is
  bounded globally.

Data writing precedes successful-query status. Preserve existing stdout
BrokenPipe success handling; a closed data sink is an explicit non-delivery
boundary, not proof that the report was seen. For other output failures retain
the original output diagnostic and report where writable, without replay. JSON
post-execution failures must be emitted once as one diagnostic object carrying
the report, rather than falling through to a second outer text diagnostic.
Report stderr failure uses the existing ordinary I/O diagnostic/exit path (70),
with no stdout fallback; it does not alter the retained database outcome. Pin
that output failure may supersede the process exit code, not the SQL facts.

Export runtime returns `Reported<Result<ExportOutcome, Diagnostic>>`, capturing
the stream report **before** `export.finish()` and attaching it to finish/flush/
sync/rename failure as well as Completed/Interrupted. Preserve W10-010 partial
wording, file bytes and normal 0/interrupted 9 outcomes. A connect error also
uses the report-carrying result, not early `?` that discards the report.

Plain mode preserves retained server messages, sanitizes them at display, adds
unattributed connection notices with a fixed connection caption and then client
accounting. It snapshots after completion/catalog success **or error**, before
the next prompt and before EOF/quit. Blocking stdin means idle messages need
not appear before the next interaction; document that boundary rather than
pretending a periodic reader exists. Reporting I/O errors stop through the
ordinary I/O path; do not silently `.ok()` the new loss-report writes.

`doctor --target` also uses reported connection outcomes. Before dropping a
successful probe session, drain its bounded idle accounting. If accounting is
nonzero/unknown, add a value-free warning check named `server notice retention`
to the existing doctor report, preserving original connection/TLS/session
checks and the single JSON report object. Use fixed client count text and the
next action `Review server logs separately if complete notice history is
required.` A failed connect keeps its original failing check plus available
client accounting. Doctor already writes a diagnostic report to stdout; do
not add a second stderr object or query-data payload. Plain connection retries
must deliver each attempt's available report before the next prompt/attempt.

## 5. Concrete bounded TUI delivery policy

Do **not** add a permanently reserved row or change W04 review geometry. Model
holds fixed per-source checkpoint baselines (user and metadata), a fixed pending
aggregate of unacknowledged omitted/shortened/abandoned counts, and the latest
fixed summary/scope per source. No extra copy of server text and no per-report
history/queue. Stop cloning execution server notices into `model.notices`;
retain that old field's local SAVED/WROTE behavior outside this server-text bound.

Pending loss is not acknowledged/cleared by paint, ticks, unrelated success,
new SQL, resize, zoom, cancellation or closing another owner. New reports
coalesce fixed counts using checkpoints once. The latest summary may be
replaced, but pending loss cannot disappear with it. A new connection can move
known unacknowledged counts into one fixed `previous connections` aggregate;
stale old events cannot become current or add the same counts again.

Add one internal `PaletteCommand::NoticeDetails`, no new mandatory binding or
configuration. Palette label `Server notice retention`; detail says `N omitted,
M shortened; pending` (precision-safe). Entry remains discoverable after
acknowledgement to inspect latest summary, including that source's fixed
omitted/shortened/abandoned counts with their precision. Add one read-only `notice_details`
owner with usize scroll and actual-key rail. Show fixed client counts, limits,
scope, `Observed messages only; SQL outcome is unchanged`, and the distinction
between limit omission/shortening/abandonment. This first slice does not invent
a server-warning viewer or user-read proof.

Opening the reader captures a fixed snapshot/checkpoint and acknowledges
nothing. Escape closes without acknowledgement. The actual configured
Activate key explicitly means `Acknowledge and close`: it acknowledges only
that displayed snapshot, never new loss that arrived while the reader was
open. This is an explicit acknowledgement, not proof of reading. Below usable
reader geometry, acknowledgement is refused and counts stay pending. No final-
page sending-review gate is added. Text/paste cannot edit the covered editor;
palette can cover it and cancel returns to it. Reuse shared wrapped-row/help
geometry patterns, not the sending-review acceptance gate. Follow connection
details modal priority: protected password/parameter/name/sending reviews,
prefix and palette remain higher. Reader never executes SQL or starts clipboard/file effects. Explicit global
Cancel remains available while a query runs: close only the reader without
acknowledgement and use the existing single cancellation transition/effect,
retaining duplicate suppression and requested-versus-confirmed semantics.
Quit remains available. These explicit recovery paths do not clear pending loss.

Actual replacement/profile retirement and Quit also account for opened sessions
already held in the runtime's fixed pending-install slots or bounded install
channel. After retiring installed
physical sources, transfer the locally available retention counts from each
current-generation, uninstalled attempt into previous-connection accounting
once per captured opaque attempt identity. Reject stale generations and avoid
counting a session already installed/shared twice. Close and drain this bounded
handoff without waiting for a held metadata reader, sending new SQL, retrying
or adding a report history. Close the channel only for shutdown; profile
replacement keeps future installs possible. Project a replaced pending attempt
before dropping it, and captured old-profile attempts before retiring their
explicit authority, even when Model has already advanced generation. Normal
late/stale events remain rejected. This covers sessions locally available at
the retirement boundary, not remote observations arriving afterward.

When no higher owner is drawn, show a complete compact footer badge
`Notices: O4 S1` (A for abandoned) after immutable query/transaction facts and
before optional search-path/key hints. Use ASCII in every glyph mode. Full
text `Notices: 4 omitted, 1 shortened` only if it fits. Lower bounds display
`O>=4`, unknown `O?`; never clip a shown count/key/meaning. If a complete badge
cannot fit or an overlay/protected owner covers it, defer it without clearing
pending state. On return to an ordinary readable frame, it reappears; palette
entry provides details. At 40x8 reviews/help retain their existing complete
input/body/rail, and the pending report is deferred. Below usable size do not
claim counts are visible; keep close/quit and retained pending state.

This explicitly trades unconditional every-frame notice visibility for
uninterrupted safety review and eventual persistent discovery. It satisfies
zoom/Plan/ordinary 40x8 visibility through the common footer, rather than a
Results-only line. Actual rendered tests must prove exact badges and required
query/transaction facts coexist in all four transaction/phase combinations;
when no complete badge fits, test the deferred path and discoverable palette.
Never force unzoom or replace the error/cancellation/transaction status.

## 6. Captured generation and transfer integration

- Use existing `Message::ForConnection` envelope, captured when a task obtains
  its `Arc<Session>`; never read the latest generation after an await to label
  old notices as new. Add fixed `NoticeSource::{User,Metadata}` and local
  installation `session_id` to status facts. No server PID/target is required.
- A successful slot install establishes that source/session identity in Model.
  Apply a status only when both connection generation and installed source id
  match. This also handles same-generation retry/replacement and shared user/
  metadata fallback. Shared-Arc fallback must poll once and must not count one
  physical Session under two source identities.
- ExecutionFinished and PlanFinished carry their report and checkpoint; existing
  job checks stay. Independently observed connection accounting is delivered as
  `Message::NoticeStatus { source, session_id, checkpoint, latest }` so a stale
  **job** cannot erase current-generation connection loss accounting. The
  envelope/session checks, not current job, own that fact.
- Event loop coalesces direct snapshots before reducing each input/event and on
  one 250ms idle select branch. No per-notice `mpsc` send and no report queue;
  unchanged checkpoints cause no model update/draw. This timer is separate from
  the paused animation ticker. That path uses the typed idle drain when no user
  operation is active, then reads the committed checkpoint; it never drains an
  active bucket. The TUI keeps client summaries only, preserving its current
  absence of a server-prose reader. Metadata methods keep their signatures;
  after each worker result the same path handles success and error.
- At an actual profile/source retirement or Quit boundary, snapshot both
  currently installed physical sessions before retiring their identities or
  incrementing generation. `notice_retirement_counts` reads committed totals
  plus that physical source's current uncommitted active/idle client counts,
  without committing, draining, resetting spending, waiting for SQL or creating
  payload copies. Do not silently substitute the ordinary last-committed-only
  checkpoint, which would omit already observed in-flight loss. Merge the
  retirement delta against the source's last seen totals once, then retire its
  identity so duplicate/late reports cannot count it again. Poison is Unknown,
  not zero. These counts cover the retirement instant; no claim includes
  later/racing server messages or future-abandonment facts not yet observed.
  Carry pending fixed counts through the deliberate transition. Do not wait
  for a busy query or new server traffic. Late old results retain normal
  rejection, not relabeling.
- On ordinary quit/event-loop error, return fixed unacknowledged counts alongside
  the loop result. `run` already explicitly restores TerminalGuard at lines
  152-164; only **after restore** write remaining fixed report text to stderr,
  preserving the original runtime/restore error. Do not prevent quit to await
  report acknowledgements. Failed stderr/restore means visible delivery is
  unconfirmed, not a successful notification. No panic/forced-death guarantee.

## 7. Ownership and failing-first acceptance

One isolated end-to-end implementation owner avoids two incompatible Session/
result APIs. Primary owns this pinned policy, shared documentation and integration; the
PostgreSQL lane independently reviews the returned isolated implementation.
Substeps are sequential checkpoints, not parallel edits of protected seams.

1. Core: `src/query/result.rs` + new `result/tests.rs` (mechanical inline-test
   move separately), new `src/postgres/notices.rs` + sidecar, `postgres/mod.rs`
   registration, `postgres/session.rs` + existing sidecar. Red tests for exact
   count/byte prefix, short/Unicode/empty fields, all field bytes, per-message
   shortening, saturation/poison, multi-statement drains, idle adoption,
   guard-drop lost prefix, sticky privacy activation/backlog and no reset on
   drain. Preserve existing successful-prefix and privacy contracts.
2. Adapters/output: `cli/interactive.rs` + sidecar; `cli/mod.rs` + `cli/tests.rs`;
   `cli/plain.rs` + new sidecar (mechanical inline-test move separately). Add
   `PlanExecution.notice_report` in `app/message.rs`. Test every stream stop and
   export finish failure, one JSON object success/error, exact text precision,
   unchanged stdout all formats and existing exit behavior, stderr failure,
   plan parse error/final probe and metadata failure. Fake flush uses existing
   private W10-010 seams; no full-disk tricks or row replay.
3. Pure TUI: `app/model.rs`, `app/update.rs`, `app/palette.rs` and sidecars;
   `app/message.rs` status/source facts; `ui/layout.rs`, `ui/widgets/footer.rs`,
   `ui/widgets/overlays.rs` and sidecars, optionally small pure `ui/notices.rs`.
   Test duplicate/stale generation and source, same-Session fallback, pending
   coalescing/ack snapshot, latest counts after acknowledgement, opened-but-
   uninstalled shutdown counts and held-reader quit liveness,
   owner/text/paste/cancel, zoom/Plan, 40x8 both
   densities, protected deferred then visible, ASCII/no-colour, long remaps and
   zero sizes. Move touched palette inline tests mechanically if needed.
4. Focused integration: existing `tests/postgres_integration.rs`,
   `tests/runtime_privacy_contract.rs`, `tests/cli_contract.rs` sidecars/new
   focused `tests/notice_retention_contract.rs` if clearer. Reuse notices test
   around postgres integration line1205 and privacy late-notice test around
   line417; do not replace them with only absence checks. Live flood256/257,
   1MiB boundary, many statements, prefix error/cancel, complete/interrupted
   export, plan notices and idle/drop transfer need positive completion/row and
   exact observed-count assertions. Bounded synthetic read-only sessions only
   under section 8's bounded authority; fixture lifecycle remains root-owned.

Current source anchors: `session.rs:106-138` privacy/admission, `229-235` drain,
`309-377` plan, `440-518` stream, `539-578` parameter execute, `627-724`
failure/execute, `973-999` polling; `cli/mod.rs:1093-1181` query output/error;
`cli/interactive.rs:1117-1252` execute/plan adapters and `1725-1786` export;
`plain.rs:536-569` notices/errors; `app/update.rs:210-214,327-329` duplication;
`layout.rs:98-159` overlay order, `371-429` footer path; footer currently
prioritizes query/transaction facts and ephemeral feedback.

Residuals: no driver startup/backlog/frame cap, incoming value cap, 32MiB
aggregate-result policy, slow-sink/cancellation proof, full-process RSS ceiling,
unbounded local SAVED/WROTE history repair, actual server-warning TUI reader,
Windows/native terminal acceptance or user-reading guarantee. This complete
notice slice cannot close W10-008's other resource requirements.


## 8. Implementation and bounded live authority

W10-012 has one end-to-end implementation owner in an independent exact-source
clone with a lane-owned Cargo cache/new SSD target. The canonical source,
feature selector, other clones/caches, root fixture lifecycle and shared docs
remain primary-owned. No new crate/dependency edge, credential route, provider,
external transmission, product flag, persistence or result-output schema is
introduced. Module separation may follow existing conventions; changes outside
the listed paths require a concrete dependency reason in the handoff.

Watch focused behavioral checks fail before fixing each boundary. Existing
inline test modules touched by this work move mechanically to sidecar tests.rs;
retain an independently checkable old/new mapping and preserve all original
cases. Do not rewrite unrelated tests or broaden a generic abstraction. Correct
source comments that incorrectly claim a row cap prevents all memory exhaustion.
Keep compatibility/threat-model/release prose as a primary-owned proposed patch
until code and evidence are reviewed together.

Bounded live work may use only the existing plain and verify-full TLS synthetic
PostgreSQL 18.6 fixtures, with environment-injected dummy credentials and
isolated configuration/data/absent passfile. Read authenticated version and
orders count before/after. Permit bounded SELECT/session settings and DO blocks
that raise synthetic notices without persistent data/schema changes. Keep
individual floods at or below 1,024 notices/2 MiB message text, never an open-
ended flood. Explicit cancellation may target only the helper's own running
Session, with joined owned controllers and bounded deadlines; it is not a
server/fixture lifecycle action. Do not terminate unrelated backends or mutate
shared data to create a failure. Existing `tests/runtime_privacy_contract.rs`
fixtures are explicitly permitted to create their own session-lifetime TEMP
tables and pg_temp helper function, perform their existing synthetic inserts/
updates and change their own search_path. Preserve those positive privacy
scenarios; they do not authorize persistent/shared objects or a fixture restart.
Model/protocol fault tests may prove other boundaries when live control is
unavailable; label those separately.

Run focused tests, fmt and Clippy with warnings denied in the isolated lane.
Return source/base, reviewed patch, helper/test/evidence digests, failures and
skips before root integration. The primary independently reviews the returned
behavior, runs required combined verification and inspects actual rendered
buffers. New Session behavior also needs affected plain/TLS major-matrix
refresh before claiming final candidate database acceptance. No native Windows,
forced process death, full hand-use or different-model-family review is implied.
No push, publication, dispatch, signing or release is authorized by this task.
