# Status

**Updated: 2026-10-25.** This file is the resumption point. Read it, then check
`git log`, `specs/001-foundation-vertical-slice/tasks.md`, and the working tree
before trusting anything else.

## Autonomous roadmap continuation - 2026-10-25 (in progress)

### Latest reviewable checkpoint

Latest product-source commit is **`d2edce1`**, published in
[draft PR37](https://github.com/tkomane/ignatius/pull/37). Its CI run
`40236560722` passed 13 executable jobs with one intentional scheduled-fixture
skip. All five T059 and 18 T058 cases executed on Linux/macOS/Windows. The
normal local/pre-push gate passed 1,520 harness cases, including 1,212 library
cases; real PG14/18 plain/TLS and Linux socket hosted evidence also passed.
The documentation checkpoint retains that exact product source; current-head
workflow state is visible in PR37's checks.

Review order is PR32 -> PR34 -> PR35 -> PR36 -> PR37. PR33 is the separate
Windows ConPTY diagnostic with its preserved sensor limitation, not an accepted
restoration fix. PR32 remains open at `d6d65e7`, against unchanged main `445ed82`.
Its already-requested specific main-merge permission is still pending; phone
workflow-scope approval did not authorize that protected action. After actual
owner permission, refresh exact-head checks and integrate sequentially, with
fresh main-targeted CodeQL on retargeted descendants. Native Warp/Windows11
hand use, cloud-provider evidence, release p95/trust and release remain open.

The team also prepared source-bound US6 mouse, US8 quiet-chrome and T032 input/
lifecycle readiness notes, indexed in the roadmap. No mouse default or automatic
background-detection support was enabled. One authorized Codex reset was used;
external OpenCode review was explicitly skipped. The canonical W07 checkout
and its unpublished `9278db4` record are preserved. The active managed worktree
is retained for review; only owned acceptance records enter this checkpoint.

### Current local progress and active assignments

Historical W07 publication checkpoint:

Current branch: `codex/windows-conpty-observation-2026-10-25`, published
`65cf237` in draft PR33, stacked on unchanged D6 in PR32. The separate cursor
calibration and reviewed CI wiring are published after the normal full pre-push
gate passed. CI `40221850172` completed with 12 successful jobs, one scheduled
skip and the unchanged Windows capability failure. The separate diagnostic
completed: native setters and main VT are observable, alternate VT is not
tracked by this sensor. Full Windows acceptance stays open. Parent PR32's
exact protected-main merge approval is pending.

Historical UI stack checkpoints:

Accepted manual-theme branch: `codex/manual-themes-2026-10-25`, reusing the managed worktree
from accepted `ffb86a1`. Published clean **`d92c416`** in
[draft PR35](https://github.com/tkomane/ignatius/pull/35), stacked on PR34.
The unchanged full pre-push hook repeated all 1,497 cases successfully; pinned
Gitleaks passed the exact 444-file clean source and 291 non-merge commits among
320 reachable local revisions. CI `40231713035` completed with **13 successful
jobs and one intentional scheduled-fixture skip**. Actual merge `253ac7cc`
matches head tree `00158901056a97195719d3e08026eeeff7234254`.
Continuation now uses `codex/editor-caret-2026-10-25` from that same accepted
head. Four owned post-publication records were carried forward intact. Root
added FR-2529, T058 and a dated presentation/plan contract for the observed
long-line editor caret clipping. An isolated worker is retaining the actual
Buffer red before implementation; a separate native worker owns independent
review. The actual 100x30 Buffer baseline has four meaningful failures and one
passing short-line control: long ASCII draws zero caret cells; wide, combining
and escaped-control cell hits name the wrong source scalar. Red log:
`/private/tmp/ignatius-editor-eol-red-1di2ma1e/red.log`. Root's actual clean-d92
100x30 PTY also clips the typed end sentinel, while Home restores the prefix;
normal quit, alternate-screen exit and unchanged config passed, with no profile
selected or SQL submitted. Receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-before-c0901prg/receipt.json`.
Two preceding harness expectations incorrectly required unchanged words in a
differential repaint; both are retained with normal child cleanup, not labelled
product failures. T058 is now locally accepted: the derived horizontal view,
shared completion/hit mapping and independently caught unfocused-zero-width
repair passed 78 widget and 114 layout tests, Clippy/formatting and the complete
1,515-harness / 1,207-library local verifier. PostgreSQL 18.6 plain/TLS ran;
macOS socket is still skipped. Root verified both freezes, original tests and
installed source and inspected seven synthetic images. The identical 100x30
PTY repeat now shows the end sentinel, still restores the prefix with Home,
quits zero and preserves config, without selecting a profile or submitting SQL.
After receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-oarmp52a/receipt.json`.
Full log: `/private/tmp/ignatius-t058-full-verify-20261025.log`
(`03c40158ff7af92030466bd04d9ad3591c7b9b0bbbd0cfd502ee054ca0e8b462`).
Published clean **`14d18ad`** in [draft PR36](https://github.com/tkomane/ignatius/pull/36),
stacked on PR35. The unchanged normal pre-push hook repeated all 1,515 cases;
pinned network-disabled Gitleaks passed all 446 clean-source files and 292
non-merge revisions among 321 reachable local revisions. CI `40234580931` is
completed on actual merge `42be23db`, whose tree matches head
`384e26053d4653a51652b965c9bcad19103b0bd0`. All 13 executable jobs succeeded,
with one intentional scheduled-fixture skip. Root verified eight platform and
21 policy artifacts plus complete PG14/18/socket logs. All 18 T058 cases ran on
Linux/macOS/Windows with Rust 1.97.1; harness/library totals are 1,515/1,207,
1,515/1,207 and 1,459/1,188. Doctor retains three explicit skips per platform.
Six permitted duplicate warnings and three conditional Python archive-test
guards remain qualified; this is not native or release acceptance.
The clean published local binary also passes the same 100x30 end/Home journey
with normal quit, alternate-screen exit and unchanged config. A retained early
repeat typed before the picker had observably closed; the final harness now
waits for the underlying-owner repaint before typing. Receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-ni4f6ssl/receipt.json`.
Only post-publication acceptance notes are now local edits. Native appearance,
hand use, fresh main-targeted CodeQL, parent integration and release stay open.
A bounded W10 diagnostic found a material T058 transient-allocation increase.
With three fresh test-profile samples per revision/size, the 1 MiB line median
render rose from 264.462 to 902.313 ms and process maximum RSS from 17.453 to
146.500 MiB; at 16 KiB medians were 6.481/19.611 ms and 11.953/13.062 MiB.
Root verified all 951 frozen baseline artifacts. One initial sandboxed observer
failure is retained/excluded; 12 valid children all preserved source/state.
This is not a process ceiling or release-build 100-action p95 result. Receipt:
`/private/tmp/ignatius-editor-perf-2kfyw0q6/REPORT.md`.
Root adopted FR-2530/T059 and the bounded-projection contract before delegating
an isolated correction. Active branch is now
`codex/bounded-editor-view-2026-10-25` from accepted PR36; only owned records and
contracts are local edits. Worker source remains isolated until review. PR36's
draft body discloses this follow-up. T059 is now locally accepted: final
sanitizer-gated ASCII projection passed 83 widget/114 layout tests, lints and
the full 1,520-harness / 1,212-library verifier. Root verified final 58 artifacts,
original test preservation and every JSON field/cell in 21 frames/51,081 cells;
independent review accepted the bounded projection and narrow ASCII delta.
Final identical-harness samples give 16 KiB median 6.198 ms/12.156 MiB RSS and
1 MiB 138.342 ms/17.063 MiB, preserving source/state and one visible caret.
The 100x30 owned PTY end/Home/normal-quit/config control also passes. Receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-jzgapy6r/receipt.json`.
These are local test-profile observations, not release p95, process ceilings
or native hand use. Published clean **`d2edce1`** in
[draft PR37](https://github.com/tkomane/ignatius/pull/37), stacked on PR36.
The normal pre-push verifier repeated all 1,520/1,212 cases; pinned offline
Gitleaks passed 446 exact-source files and 293 non-merge commits among 322
reachable local revisions. Local/head/upstream match. The clean published
binary also passes the 100x30 PTY control, receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-3z02aab9/receipt.json`.
CI `40236560722` uses merge `882873ec`, whose tree equals head
`c9276f6b71c45dc439f247e37ef9b9431cd6d414`. PG14/18 plain/TLS, real Linux socket,
all 13 executable jobs passed, with one intentional scheduled-fixture skip.
Root verified eight platform/20 policy artifacts and complete database logs.
Linux/macOS totals are 1,520/1,212 library and Windows 1,464/1,193; all five
T059 plus 18 T058 cases execute on each. Doctor retains three explicit skips.
These acceptance records form a documentation-only checkpoint. Canonical
W07 checkout remains clean at its original `9278db4`; no owner work was replaced.
Read-only US6/US8/T032 readiness
reviews are reflected in the roadmap; their unresolved policy dependencies
remain open and mouse capture stays disabled.
Root adopted the dated manual theme contract/plan and
T028a test subset. Root integrated the five-file pure theme freeze after checking
all 39 artifact hashes and reviewing the grammar, safe refusals and resolver.
That lane passed 42 theme and 11 Results tests; four measured Null/comment
contrast failures precede the four admitted RGB repairs. Root's seven reducer
tests pass, including custom cycling, owners, busy queries, mismatch/query
diagnostic preservation and profile/resize/async retention. Four original
runtime/palette cases were observed red first. Independent runtime review found
two missing exhaustive discovery arms; both are corrected. The 31 keymap tests
pass after extending reachability coverage to open the actual palette.

The strict-schema prerequisite preserved original TOML errors/spans; its final
phase2 source is now integrated after root reviewed the actual admission code
and verified all 127 artifacts. Root independently confirmed the normalized
bodies of all 41 original configuration/diagnostic tests and the store helper.
Four real-CLI cases were observed red first and now pass on the combined source:
150 isolated child runs cover exact refusals across startup/query/show/validate/
migrate, valid roundtrip, doctor JSON and bounded redacted reflection.

The initial five-theme T033 matrix passed 2,520 actual frames. Root checked 101
artifacts and inspected all seven sheets, then individual HighContrast palette
inspection exposed selected group/detail ink retaining Muted on white fill.
T056's failing-first correction and stronger assertions are integrated from a
new 125-artifact freeze. Root inspected corrected HighContrast, inverted Custom
and colour-off palette images and the comparison sheet. Its own comparison of
12,096,000 cells confirms only 2,340 foregrounds change in 60 painted palette
frames; all labels, backgrounds, modifiers, geometry, Basic16/off and non-palette
frames are preserved. The rejected first candidate's extra Basic16 label change
and all earlier evidence remain retained. Native appearance is still unverified.

The final combined local verifier passed **1,497 outer harness cases, 1,189
library cases and 25 summaries**, all six executed gates and actual PostgreSQL
18.6 plain/TLS. Both existing synthetic servers were freshly queried as 18.6.
The macOS socket case remains skipped. T028a/T029/T030/T031/T033/T056/T057 record
manual local implementation, including the configured-colour route.
T028/T028b/T032 and whole-US5 completion stay open.
Published source is locally/hosted verified; current post-publication acceptance
notes are owned local documentation edits for the next meaningful checkpoint.
Fresh main-targeted CodeQL, parent integration, native appearance/hand use and
release remain open. PR32's already-requested protected-main approval is still
pending; phone workflow-scope approval did not authorize that merge.
Full log: `/private/tmp/ignatius-manual-themes-t057-verify-20261025.log`
(SHA-256 `965e7cd3662408e9da11c3d59a3647c1bdda22aac32bd20b5dccd418496da914`).

Final independent integration review found an existing route omission after the
earlier 1,492-case pass: full-screen startup and doctor ignored `[ui] color`.
That pre-T057 pass did not prove the route. Root pinned exact precedence before
delegating a narrow private CLI correction: explicit flag, configured choice,
then auto facts, with plain/NO_COLOR/dumb absolute. Root reviewed and integrated
the five-file correction after verifying all 208 frozen artifacts; 64 distinct
focused cases and 27 actual doctor subprocess controls passed. The final full
repeat above includes this correction. Source/history scans, publication and
hosted platform/MSRV acceptance have now passed as recorded above.
Root also confirmed T057 in the actual pre-fix local binary using three owned
macOS PTYs and a synthetic profile picker: configured Custom/never emitted 119
painted SGR sequences, explicit always emitted 119, and NO_COLOR plus always
emitted zero. All three quit normally with alternate-screen enter/exit captured;
no profile was chosen or SQL sent. This proves the route defect and controls,
not native emulator/font appearance. Before evidence:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-theme-pty-3i9adtvz/`.
The corrected binary changes configured never to zero painted SGRs. Explicit
always and NO_COLOR control captures are byte-identical to before; all three
sessions quit normally. After receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-theme-pty-qtg0xxks/receipt.json`.

Independent hosted platform audit confirms the same 74 selected manual-theme/
T056/T057 cases executed on Linux/macOS/Windows with Rust 1.97.1. Full harness/
library totals were 1,497/1,189, 1,497/1,189 and 1,441/1,170. Startup/refusal smoke
passed; doctor retains three explicit skips per platform. Rust 1.90 compilation,
advisories/licences, workflow and release-scope gates passed. Six permitted
duplicate-version warnings and three conditional Python archive-test guards
remain qualified in the policy audit. PG14/18 plain/verified-TLS and one actual
Linux socket passed; 473 selected invocations include pure/local tests and are
not 473 live queries. Runtime-major guards and readiness support the live claims.

The clean published local binary also completed a real palette keyboard cycle
through dark/light/high-contrast/custom in an owned 180x40 macOS PTY, retaining
typed SQL and exact config bytes, with normal quit and no connection selection
or SQL execution. Earlier probe harness failures are retained: one stalled
setup was terminated by exact owned PID; another assumed visible text at a
clipped width; a later one used a wrong expected Light RGB. None is claimed as
a product regression or passing journey. Final receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-theme-cycle-pty-h7f9buo8/receipt.json`.

Hosted platform receipt: `/private/tmp/ignatius-pr35-platform-audit-2u768bko/receipt.json`
(28 artifacts, manifest `14b6a8b72446181242dd24d1fa256a35bc9b96d9e991ef06a41f0c7a53cd08d9`).
Policy: `/private/tmp/ignatius-pr35-policy-audit-ziQQqR/RECEIPT.md`
(37 artifacts, manifest `d0b7857d77b8c9fd7a437143cc86effdf62396a035121a4534f8151dbbce6131`).
DB audit: `/private/tmp/ignatius-pr35-d92c416-db-audit-20261025.json`.
Publication: `/private/tmp/ignatius-manual-d92c416-publication-20261025.json`.

Next roadmap readiness: `/private/tmp/ignatius-t032-readiness-w4djagqv/ASSESSMENT.md`
identifies late OSC reply/input ownership, staged guard, deadline/parser and
doctor-provenance decisions before T032. It is a read-only, source-backed
proposal, not an adopted input design or permission to enable auto. Continue
the day-long goal without weakening the current explicit auto refusal.

Theme freeze: `/private/tmp/ignatius-us5-theme-ffb86a1-z53t1vo8/pure-theme-freeze-01/`
(manifest SHA-256 `edf823b9f4c69bb56ef2669e0501e3b230873a005a5197d12145a0fbfce11263`).
Root runtime log: `/private/tmp/ignatius-us5-runtime-root-custom-green-20261025.log`.
The runtime size-query regression was also observed red then green; theme keys
now bypass the geometry prerequisite, preserving covered offsets. All six
existing runtime geometry/review controls pass. Independent review found no
remaining scoped blocker for that delta.
CLI red: `/private/tmp/ignatius-us5-cli-root-red-20261025.log`.

Previous branch `codex/selected-field-contrast-2026-10-25` is published in PR34,
isolated from reviewed D6 in PR32. Feature025 T055 corrects an expanded Results
selection defect before manual custom themes: child Header/NullValue colours
override Selection ink, producing white-on-white HighContrast text. Root
verified all 16 immutable reproduction artifacts. The worker owns only
`src/ui/widgets/results.rs` and its sidecar; contracts/spec/plan/tasks pin the
correction and 72 presentation combinations before implementation. No palette,
configuration, runtime action or geometry change is included. The exact two-file
correction is integrated and independently reviewed. Root verified all 76 frozen
artifacts; all eight original Results tests remain byte-identical. Focused
Results 11/theme 20 passed, with the maintained actual-Buffer red preserved.
Root inspected all 12 before/after synthetic rasters; only selected painted
foregrounds change. These captures do not prove native terminal appearance.
The complete local verifier passed **1,434 outer harness cases, 1,130 library
cases and 24 summaries**, formatting, warnings-denied lints/API docs and actual
PostgreSQL 18.6 plain/TLS. The first run's six TLS failures came from missing
ignored synthetic fixture files in this new worktree; all affected cases passed
after fixture wiring and a full repeat. The macOS socket case remains skipped.
Published clean `ffb86a1` in [draft PR34](https://github.com/tkomane/ignatius/pull/34)
after the installed full pre-push verifier again passed all 1,434 cases, with the
same socket skip. Pinned Gitleaks passed the exact 438-file source and 290
reachable revisions. PR merge ref `d38304f9` has the identical source tree
`77c2add104a13c5bd20bca99eb687d0de9f0578a`. CI `40223833554` completed with
13 successful jobs and one deliberate schedule-only drift skip. The three new
Buffer cases ran and passed on Linux/macOS/Windows; library totals were
1,130/1,130/1,111 under actual Rust 1.97.1. Startup/refusal smoke, Rust 1.90,
PG14/18 plain/verified-TLS and the real Linux socket passed. Database jobs ran
236 selected invocations per major plus one socket; these include pure/local
cases and are not 473 live queries. Fresh main-targeted CodeQL remains pending.
Local receipt:
`/private/tmp/ignatius-t055-root-integration-20261025.json`.
Publication: `/private/tmp/ignatius-t055-ffb86a1-publication-20261025.json`.
Hosted platform audit:
`/private/tmp/ignatius-pr34-platform-audit-kmb7tndf/RECEIPT.md` (SHA-256
`8e2cd6155079190e78da21487a0c77089055410f976a462259215c62ac944095`);
DB/socket audit: `/private/tmp/ignatius-pr34-ffb86a1-db-audit-20261025.json`.
Repro: `/private/tmp/ignatius-us5-selection-probe-3k_rf75u/freeze-01/`.

Next: integrate configuration refusal/startup gates, complete styled parity and
CLI checks, then run the full gate and publish the reviewed manual slice while
keeping auto/T032 open.
Read-only strict-parser/identity design brief:
`/private/tmp/ignatius-us5-strict-identity-design-ffb86a1-20261025.md`, SHA-256
`363f853c8c6245f2982b59eb89d482ea04b89a7dfc123be70c2917cbda9e33cc`.
The design has been adopted in the dated contract with only theme leaves
deferred and Custom identity inseparable from its validated cache. Exact
reflection escaping, caller-specific precedence and the actual capsule contrast
set are pinned. Preserve migrate/save's existing lack of ordinary range
validation; add only theme checks there.
Post-publication acceptance records are local owned documentation edits for the
next meaningful source checkpoint; published application source remains `ffb86a1`.

Windows work continues separately in [draft PR33](https://github.com/tkomane/ignatius/pull/33):
at `65cf237`, calibration follows native setters/main VT but cannot track
alternate-VT visibility; the unchanged original cursor capability still fails.
That harness is not part of this sibling branch. Parent PR32's protected-main
merge still needs the already-requested specific owner approval. Nothing has
merged or released. Earlier source-bound checkpoints follow.

Parent branch: `codex/autonomous-roadmap-2026-10-25`. Notice source is saved in
`d65024b`. W10-012's integrated notice
implementation passed the complete local verifier: **1,370 outer harness cases,
1,072 library, 23 summaries**, formatting, warnings-denied lints/API docs and
PostgreSQL 18.6 plain/TLS. The macOS socket case remains a semantic skip.
The initial full run exposed a new fixture assertion's incorrect Disable/Prefer
assumption; three actual transport probes and independent review accepted the
test-only correction before the full repeat. Log:
`/private/tmp/ignatius-verify-2026-10-25-notices-recheck.log`.

Root verified the immutable 129-file handoff and 29-file repair evidence, all
43 moved original test names, and inspected 128 styled synthetic buffers. Peer
review accepted the repaired doctor guidance, pending replacement/profile/Quit
accounting and weak-reference deduplication. CLI/server outcomes remain intact.
Live CI wiring now includes notice contracts and runtime lifecycle cases on
both plain and verified TLS. Source publication, hosted/other-major/native
acceptance are still separate. Prior application checkpoints are `918c0b1`
(export progress) and `d8a3ac3` (help ownership); W10-011 pacing is measured and
reviewed. Read Git state before taking an assignment; the day-long goal remains
active.

Owner steering on 25 October: skip external OpenCode review,
continue with native independent review, and follow the normal DevOps workflow
rather than keeping finished work local. The lead will push the task feature
branch, open a draft PR and inspect hosted CI after integrated local checks.
No release/production action follows from that workflow.

Published clean checkpoint `fda16f8` to the task feature branch and opened
[draft PR #32](https://github.com/tkomane/ignatius/pull/32). The installed
pre-push hook repeated all 1,370 outer cases/23 summaries successfully before
upload; the macOS socket skip remains explicit. Clean-source/history Gitleaks
also passed (434 files/282 reachable revisions). Hosted CI run `40202770798`
and CodeQL run `40202770715` completed for exact head `fda16f8`: 16 workflow jobs
succeeded, one schedule-only drift job skipped, and the separate built-in
CodeQL check succeeded with no new alerts. All three hosted platform jobs,
Rust1.90, PG14/18 plain/verify-full and the real socket contract passed.
The weekly fixture-drift job is deliberately schedule-only and skipped on this
PR; that is not a passing upstream-manifest comparison. Hosted platform DB
early returns are not live evidence; the separate PG jobs each ran 210 selected
invocations across both routes. The socket job ran one real test without skip.
PR merge ref `d9198a7` and head `fda16f8` have identical source tree
`a561e457e7e9ab6923edc39e065e9459f5d38146`. Native Windows terminal/hand-use,
all-major refresh beyond the representative PR matrix and release remain open.
Reviewed evidence: `/private/tmp/ignatius-pr32-ci-fda16-20261025/`.

A separate bounded smoke of published `fda16f8` passed PostgreSQL 15.19 plain
and verify-full TLS, including exact 257-notice omission/output checks and
before/after server-version/orders health. The isolated lifecycle controller
removed only its two labelled containers/anonymous volumes; all five pre-existing
running containers were preserved. Root verified both output digests and absence
of task-labelled containers. Evidence:
`/private/tmp/ignatius-resource-fixture-proof-mmfwivkh/`. This is notice-source
PG15 smoke, not W10-013 or a full five-major repeat.

W10-013 is integrated and locally verified. Root checked all 268 final
milestone03 artifact hashes and exact equality for its 24 canonical code paths.
Native independent review accepted the final patch, including the reproduced
one/two-inner-row warning-clipping repair. All original sidecar assertions were
preserved. The complete canonical verifier passed **1,431 outer harness cases,
1,127 library, 24 summaries**, formatting, warnings-denied Clippy/API docs and
PostgreSQL18.6-backed tests. Its macOS socket case remains a semantic skip.
Log: `/private/tmp/ignatius-verify-2026-10-25-retained-result.log`.

The unchanged core/output of milestone02 passed 12 fresh case children and 24
separate CLI health children on PG18.6 plain/verify-full, without skips. Buffered
40-MiB input retained 31/received 40/affected 40, including an owned TEMP write and
subsequent positive same-session count; real SQLSTATE22012 remained true. Both
complete streams verified 41,943,080 bytes before owned payload removal. Root
verified 72 capture hashes and all 448 inputs after execution, then confirmed
only the two authorised UI files changed in final03. Removed stream bytes were
checked by Rust/controller; root inspected receipts and path absence.

Root captured 112 styled synthetic Ratatui buffers in both densities/four
presentations, verified all 354 artifact hashes and inspected eight contact
sheets (16 frame images), including the reachable one/two-inner-row panes.
No remaining scoped warning/count/ownership issue was found; native appearance
and hand use remain separate. Evidence:
`/private/tmp/ignatius-retained-styled-3rtx76kd/`, final manifest
`a93728fabc53bfc56b7ce4b229d418a7bddc41317ef334fd9d60e321d22cb851`.
CI omission red observed zero of two required routes; plain/verify-full wiring,
the contract and pinned network-disabled Actionlint now pass. Source freeze:
`/private/tmp/ignatius-w10-013-fda16f8-TfHH4z/milestone-03/`.

Published clean checkpoint `54056c210ddfaa2c6c1629b0e7ab16caa7cbb8e2`
to the task branch and updated draft PR32. The unchanged installed pre-push
verifier again passed all 1,431 outer cases/24 summaries with the explicit
macOS socket skip. Clean source (437 files) and 283 reachable Git revisions
passed pinned Gitleaks. PR CI `40209111816`, CodeQL `40209111702` and the
manually dispatched full five-major CI `40209244285` completed for exact
head `54056c2`. All 35 check runs are now terminal: 33 succeeded and two
schedule-only fixture-drift checks skipped deliberately. CodeQL's temporary
missing-Rust state resolved to success with no new alerts. All platform/MSRV
jobs, actual PG14-18 plain/verify-full and one real Linux socket passed.
Each major ran 236 selected invocations (1,180 total); these include 60 actual
new retained-result invocations. Session's 16 selected cases per route include
two live and 14 pure cases; notice sidecar eight includes six live/two local.
Runtime server majors are positively asserted; patch versions are immutable
fixture-image provenance, not printed runtime strings. PR merge ref `dceeaeda`
has the identical `54056c2` tree `6f32257cb8fbce7c36c6fe0ca50829c9639d878e`.
Root verified the 11 hashed hosted receipts and full check listing. Evidence:
`/private/tmp/ignatius-pr32-54056c2-five-major-20261025-Vo7bDf/`.
W10-013 is checked for this scoped implementation/acceptance. Native Windows
hand-use and other resource routes remain open. Source publication has not
merged main or released. The next record commit changes documentation only.

Documentation-only checkpoints `999aba8` and `d6d65e7` are published after the
unchanged full pre-push gate passed again. Rust/Cargo/build-script/test/workflow
file bytes are identical to accepted `54056c2`; build revision identity changes
normally. Fresh PR CI `40211200487` and CodeQL `40211200491` completed at
`d6d65e7`: 17 successful checks, one deliberate schedule-only skip and no new
CodeQL alerts. Reviews/comments/unresolved threads are empty; PR32 is ready.
An attempted protected-main merge was rejected by automatic approval review
because the publication grant did not explicitly authorise that merge. The
owner's exact-head merge question is pending; do not retry without that approval.
No main mutation occurred. Root inspected all 30 annotated checks at `54056c2`:
30 runner queue/image notices and zero warning/error annotations.

W07-064 is now specified as the next isolated assignment under foundation
REL-001 and `contracts/windows-conpty-observation.md`. Use a test-only native
SDK observer plus the selected safe Rust guard/hook process; establish native
capability and negative ownership/state controls before the full option/path
matrix. Root owns CI and acceptance. Unsupported paste/buffer observations
cannot close T055a; Windows 11/Windows Terminal hand use stays T051. No Windows
ConPTY runtime evidence has run yet.

The PostgreSQL lane now implements the first W07-064 capability/control freeze
in `/private/tmp/ignatius-w07-064-d6d65e7-il6f_ql4/source`, exact detached D6,
with a fresh SSD cache/target and all previous frozen inputs preserved. Owned
paths are new test-support C++/safe Rust/PowerShell only; root owns CI, product
source, canonical docs and Windows execution after immutable review. The root
clarification pins 250 ms after positive HANG_READY, 20 s setup and 25 s outer
supervision, exact Job membership and protected local pipes. Current canonical
contract includes it; frozen input copy/hash is recorded in the private handoff.
No 24-case expansion or Windows runtime is authorised before capability review.

First W07 freeze01 is retained under the lane's
`capability-control-freeze-01/`: 36 snapshot hashes independently verified. Local
19 pure C++/30 synthetic PowerShell/4 parser checks and helper compilation ran;
zero Windows cases. Root and peer blocked native execution on two harness
findings: cleanup waits after the 25-second deadline and suppressed zero-byte
ReadFile errors. Freeze02 repairs both with a shared absolute cleanup budget,
heap-owned pending I/O lifetime, strict error classification and pre/post build
input proof. Root verified all 32 new artifact hashes and nine integrated files;
the only integration deviation removes one trailing space. Independent review
accepted the limited source scope and rehashed all 36 prior artifacts and 1,207
compiled inputs unchanged. Review:
`/private/tmp/ignatius-w07-064-independent-review-02-20261025.md`, SHA-256
`40aaead5667e256bc306a340ec4c086663af67b980b33f7cba94bdb024d2e4eb`.

Root admits only the first four controls/capability cases to the authorised
hosted CI workflow after the local publication gate. Actual wrong-ownership,
native error-path cleanup and the full 24-path matrix remain open. All 15
workflow contracts, 26 pure C++ and 48 synthetic PowerShell checks passed in the
integration tree; standalone Rust formatting and pinned Actionlint passed.
The Windows-only CI/evidence wiring previously failed first; its script AST
also passed. The complete local verifier passed 1,432 outer harness cases,
1,127 library cases and 24 summaries, formatting, warnings-denied lints/API
docs and PostgreSQL 18.6 plain/TLS. The macOS socket case remains a semantic
skip. Log: `/private/tmp/ignatius-w07-064-integrated-verify-20261025.log`.
Published clean `69f4de6` and opened [draft PR33](https://github.com/tkomane/ignatius/pull/33)
against unchanged parent D6 for a scoped 14-file review. The unchanged pre-push
gate passed all 1,432 cases again, with the same macOS socket skip. Pinned
Gitleaks passed the exact 447-file clean snapshot and 286 reachable revisions.
CI `40215610812` finished: 12 jobs succeeded, one schedule-only job skipped and
Windows failed in the new controller before any native child. Actual pinned
Rust 1.97.1 and MSVC toolset 14.51/SDK 10.0.26100 helper builds succeeded. Root verified
the 94,212-byte artifact digest and all 447 source/nine helper hashes against
the committed source. Its summary records empty cases, unchanged inputs and
`Index was outside the bounds of the array.` The exact empty-directory failure
reproduced under local PowerShell StrictMode. The three-script inventory repair
passed 52 checks (all prior 48 plus four actual file/directory cases), four AST
checks and independent review. Root verified the 14 frozen artifacts and
integrated source bytes. Budget/ownership/native code and first-four scope are
unchanged; the primary admits the repeat after the normal pre-push gate.
The failed run remains preserved and no native result is
claimed. First evidence: `/private/tmp/ignatius-pr33-windows-first-40215610812-1/`.
Repair `7d167a9` is now published after the installed full pre-push gate again
passed all 1,432 cases with the same macOS socket skip. Clean 447-file source
and 287 reachable revisions passed pinned Gitleaks. CI `40216991785` finished:
12 jobs passed, scheduled drift skipped and Windows failed the capability
cursor assertion after four actual children. No-guard, known-unrestored and
acknowledged hung-child controls passed. Guard input masks were 423/152/423,
with genuine raw/mouse transition/restoration; buffer sentinel was 1/0/1.
Cursor queries succeeded but stayed visible 1/1/1, so capability remains failed.
All four exact Jobs were empty, processes waited and output/control-I/O cleanup
confirmed; the acknowledged hang interval was 257.6195 ms. Root verified all
27 archive files, 17 capture entries and 447 source/nine helper hashes.
Evidence: `/private/tmp/ignatius-pr33-windows-second-40216991785-1/`.
The team is diagnosing sensor/active-buffer semantics without product changes,
assertion weakening or matrix expansion. PR33 stays draft; parent PR32's
specific protected-main merge approval remains unanswered.
The root adopted a single separate cursor calibration in the existing contract:
one persistent authenticated actor compares native setters and VT main/alternate
cursor operations, with controlled marker readback and complete native facts.
The implementation lane now owns only test-support changes in a fresh private
`7d167a9` snapshot. The independent lane reviews the calibration design. Root
owns canonical source/CI and native admission after frozen review;
the exact published calibration is now admitted under the bounded scope below. Frozen contract input:
`/private/tmp/ignatius-w07-cursor-calibration-contract-20261025.md`, SHA-256
`0cc81e4bdcd68692a155911463cd7aba279e7d03afac300c41b0811c5df5f286`.
Candidate freeze01 contains 60 root-verified artifacts and reported 53 portable
C++/82 PowerShell/three actual-emitter synthetic checks, preserving prior26/52.
Review01 blocked native admission on a short-sentinel-read gap: requested/
actual read lengths must prove complete content before alternate absence.
Repair02 captures individual restoration verification API results and all
requested/actual sentinel counts. Root verified all 75 repaired artifact hashes,
read the repaired fields and integrated all nine files byte-for-byte. Independent
review accepted both repairs; 74 C++ and 103 PowerShell checks passed again in
the canonical tree. Worker evidence separately records six synthetic emitter
cases, two exact wire-emitter roundtrips, four AST parses and Rust formatting.
The original immutable candidate remains preserved. The primary admits only
the separate calibration and unchanged first four on this reviewed source.
Published `65cf237` passed the normal full pre-push gate: 1,432 outer harness
cases, 1,127 library cases, 24 summaries and all six executed gates. The macOS
socket case remains a semantic skip. Pinned Gitleaks passed the exact 447-file
source and 288 reachable revisions. PR merge ref `dfe46f52` has identical tree
`4ce8f1f7cf0b8e704294578d76fb1531b963dc21`. CI `40221850172` finished with 12
successful jobs, one deliberate schedule-only skip and a Windows failure.
The separate nine-stage diagnostic completed with one authenticated retained
actor: native setters and main VT produced hidden/shown transitions, while
alternate VT remained visible 1/1 despite complete writes, unique phase marker
readback and 36/36 sentinel reads. Baseline 423/7, cursor size 25 and visibility 1
were independently restored before the distinct final query. Diagnostic duration
was 54.2058 ms native/157 ms controller. The unchanged original four again produced
three control passes and failed cursor capability; all five exact Jobs emptied
and actual process/drain/control-I/O cleanup completed. Root verified the
105,445-byte ZIP SHA-256 `c78a505aa9c1994b8300080963e6e8268e3e1173e866a809f2527f1bb0fbac20`,
all 33 files, 22 capture entries, 447 Git source blobs and nine helpers. This
measures a sensor/routing limitation, not actual alternate cursor state or a
product defect. No matrix expansion or weaker assertion is admitted. Evidence:
`/private/tmp/ignatius-pr33-windows-calibration-40221850172-1/`; root receipt
`/private/tmp/ignatius-pr33-65cf237-calibration-windows-review-20261025.json`.
Independent native audit found no integrity or adopted-scope inconsistency:
`/private/tmp/ignatius-w07-calibration-65cf237-native-review-j2f4lx0v/REVIEW.md`,
SHA-256 `64a56107cda058b343d6434393054bca603a91c94e99d9688b6f8bdbad5496a1`.
Compiler/link binary hashes remain build/controller-recorded identities because
their bytes are intentionally excluded from the narrow archive.
Root's separate diagnostic-before-capability CI wiring passed its meaningful
red/green workflow contract, formatting, pinned Actionlint, PS AST and independent
review. The 21-pattern evidence allowlist is published with the source.
Freeze01: `/private/tmp/ignatius-w07-cursor-calibration-7d167a9-qgzdtg_o/cursor-calibration-freeze-01/`.
Accepted repair:
`/private/tmp/ignatius-w07-cursor-calibration-7d167a9-qgzdtg_o/cursor-calibration-freeze-02/`,
manifest `c44d1f1f3f8ea4e9c7455051cf32ccd447dd8a07ba80581f71c52a6ea7c810bc`;
peer `/private/tmp/ignatius-w07-cursor-calibration-independent-review-02-20261025.md`,
SHA-256 `139acb266ab0fa04bf0aa3e9166c06adee202a83e0662134c405badbfa3beda0`.
Fresh CodeQL is pending retargeting to main because its existing base-main
filter is preserved.
The independent roadmap preparation now assesses Feature025 US5's manual
custom-theme/startup/runtime-switch journey; auto/T032 and whole-US5 acceptance
remain open. Its unadopted contract proposal is
`/private/tmp/ignatius-us5-manual-contract-proposal-afimm9za/PROPOSED-PINS.md`
(SHA-256 `0fbc914b56ad2e38beac198dc728868cdd0a143f04b45698c8dd068a9406d9e8`).
Root verified both planning artifact hashes. The actual-library probe confirmed
a P2 expanded-field selection defect: HighContrast header white on white and
NULL grey on white, instead of Selection's Surface ink. Root verified all 16
frozen repro artifacts; the meaningful real-Buffer red remains preserved in
`/private/tmp/ignatius-us5-selection-probe-3k_rf75u/freeze-01/`. T055 specifies
the narrow composition correction before custom themes, with no palette change.
Active isolated worktree:
`/Users/tshiamo/.codex/worktrees/selected-field-contrast/ignatius-psql-client`,
branch `codex/selected-field-contrast-2026-10-25` from D6, independent of PR33.
The worker owns only Results renderer/sidecar; root owns contracts, integration,
rendered review and publication. The fix is not integrated or accepted yet.
The independent integration review accepted the installed-tool bootstrap and
15-pattern evidence allowlist; receipt:
`/private/tmp/ignatius-w07-064-independent-integration-review-20261025.md`.
The next wrong-owned-Job and one deliberate Guard error-return controls are
proposed privately, not implemented or admitted before first-four results.

The team separately assessed incoming allocation and next-slice feasibility.
No frame/startup/RSS bound is adopted: the locked driver has no single public
all-transport hook; raw ownership also needs cancellation/dial parity. Auto
background detection needs a truthful late-reply/restoration contract. W07's
Windows ConPTY automation is being assessed against the existing hosted runner;
none of those discovery notes changes product behavior or acceptance yet.

- W03's fresh string-semantics guard and W10-006's private parsed-snapshot
  migration backups are saved in `4fc3357` and `ecdb9bb`. Their failing-first
  probes, plain/TLS checks and native peer review are recorded in their tasks.
- `f99fe75` patches exactly five security packages without other dependency-edge
  changes. All-workspace/all-features locked cargo-deny passed with zero errors
  and six existing duplicate warnings; policy was unchanged.
- W05's final clean `e5389f7` source passed 155 scoped contracts on each of
  PostgreSQL 14.24, 15.19, 16.15, 17.11 and 18.6 (775 total), including verified
  TLS. Application code matches `f99fe75`; `e5389f7` repairs a reproduced fixed-
  delay export-test race. Reviewed digests/default 18.6 are saved in `5e6d232`.
- Clean `f99fe75` passed actual Linux arm64 PostgreSQL 18.6 socket, Rust 1.90.0
  locked/offline workspace/all-targets/all-features compilation and three
  normal-quit PTY cases. Those frozen results exclude later UI work.
- Feature 025 T039/T041 keyboard panes are implemented: reversible splits/zoom,
  shared geometry and accurate paging, modal ownership through chords, visible
  error/recovery state, minimum-size diagnostic content and readable feedback.
  Full local verifier `/private/tmp/ignatius-verify-2026-10-25-panes.log` passed
  1,206 harness cases (935 library, four isolated PTY, 14 workflow), formatting,
  lints, API/doc checks and PostgreSQL 18.6 plain/TLS. Its macOS socket case
  remains a semantic skip. Source was the modified integration tree based on
  `abe4aed`. Density T040, the rest of T038 and dragging T035 remain open.
  The verified keyboard slice is saved in `4a7717b`.
- W07-059 is saved in `abe4aed`: unique PTY config/data, inherited logging/PG
  routes removed, explicit absent passfile. A real child probe failed first;
  all four focused PTY contracts and the integrated repeat passed.
- A separate owned-PTY probe at clean `e5389f7` passed macOS SIGTERM after first
  paint, restoration order and actual canonical/echo/signal-mode recovery.
  Initial Darwin PENDIN state cleared after a same-PTY line read; full stty
  then matched without reset. Startup registration still had a window.
- W07-060's owned self-exec red died by SIGTERM before runtime polling. The
  pre-terminal registration repair passed 21 focused runtime tests, was reviewed
  and integrated, then passed the latest full locked verifier: 1,208 harness
  cases/937 library, PostgreSQL 18.6 plain/TLS, semantic macOS socket skip.
  Log: `/private/tmp/ignatius-verify-2026-10-25-signal-startup.log`. The actual
  integrated `4a7717b` modified binary also passed owned-PTY SIGTERM, restoration
  order, canonical/echo modes and exact stty after normal PENDIN recovery input.
  Panic/error process paths and Windows remain open.
  The repair is saved in `21511f1`. A fresh clean Linux arm64 repeat at that
  commit passed app 303, UI 215, interactive 21, socket one and PTY four
  (544 focused cases, zero skips), plus actual Rust 1.90.0 locked/offline
  workspace/all-targets/all-features compilation. Its production binary
  confirmed clean source and exactly pinned Rust 1.97.1. Evidence is under
  `/private/tmp/ignatius-linux-21511-20261025-FWM0SW/`. Density remains outside
  that frozen source.
- W07-061 removes the detached delayed numeric-PID kill from the PTY harness.
  All six focused checks passed: the three existing actual macOS PTY checks,
  profile isolation, an owned hung-child deadline/reap and normal completion
  with 130,000 output bytes. Native peer review found no blocker. Private file
  captures avoid pipe backpressure; no delayed watchdog survives completion.
  Log: `/private/tmp/ignatius-w07-owned-deadline-2026-10-25.log`. The integrated
  full repeat follows density; Linux/abnormal-exit/Windows proof is separate.

- Density T040 and T038 test implementation passed their complete integrated
  local gate: 1,224 harness cases/950 library across 22 summaries, PostgreSQL
  18.6 plain/TLS, semantic macOS socket skip. Focused config 36/app 305/UI 224/
  CLI one/docs 15 and final Clippy passed. The lead inspected 88 styled
  synthetic buffers, including two reproduced/fixed protected-input regressions.
  Log: `/private/tmp/ignatius-verify-2026-10-25-density.log`; buffer provenance
  and scope are recorded in verification. Dragging and full US7 acceptance
  remain open. Density's source/Cargo ownership has been handed back.

W04 now shares scrollable content, wrapping and admission across copy,
generated UPDATE, production run, EXPLAIN ANALYZE and named parameters. Both
acceptance aliases stay blocked before the current final page or below usable
dimensions; inputs and complete command rails stay fixed. The lead reproduced
and repaired extraction regressions in redaction, late update-metadata modal
ownership and enlargement before repaint. Runtime supplies actual frame/backend
dimensions; the pure reducer owns the new `InputAtSize` admission policy.
The final focused repeat passed app 328/UI 241/runtime 27 (596 cases, zero
skips), Clippy and formatting. All source ownership is handed back to the lead.

The lead inspected representative first/end/middle styled frames from 152
actual synthetic Ratatui buffers across both densities, three truecolour themes
and ASCII/no-colour. Default 40x8 parameter and typed-PROD prompts retain input,
two body rows and command rails; the exact UPDATE tail is reachable. Evidence:
`/private/tmp/ignatius-w04-styled-reviewed-lay5bval/`. These are rendered-cell
checks, not native screenshots or proof of reading. Feature 018 T039/T040 are
checked after the combined locked full verifier passed 1,270 harness cases/996
library across 22 summaries, formatting, lints/API docs and PostgreSQL 18.6
plain/TLS. Its macOS socket remains a semantic skip. Log:
`/private/tmp/ignatius-verify-2026-10-25-review-and-panic.log`.

W07-062's isolated error/panic repair was reviewed and applied to root's two
terminal files. It retains actual acquired options atomically, restores them
before previous-hook output, and removes unsupported panic claims. The HOME-
free private clone passed 13 ordinary units, two PTY drivers and one inert
parent helper; the drivers exercised 16 actual owned macOS PTY children across
all mouse/paste/screen combinations, with canonical/echo/signal and raw-state
observations. It also passed Clippy/formatting. Patch SHA-256:
`cf3570a569025794995da01bffacfa9b1102036db75f14e5abf6350ad350b763`;
source/logs/manifest are under `/private/tmp/ignatius-w07-062-w2iPKH/`.
The combined full verifier passed and W07-062 is checked for that exact scope;
latest-source Linux/Windows, pinned/MSRV and native visuals remain separate.

W07-063 now observes uncaught unwinding outside libtest: all eight standalone
production-library guard/hook processes restored actual raw/canonical/echo/
signal state before previous/default panic output and exited 101. Immediate
Darwin PENDIN cleared after normal same-PTY line input, then full flags matched
exactly without reset. Evidence: `/private/tmp/ignatius-w07-063-u329k7er/`.
Only planning docs changed at the recorded `74eb155` build; application inputs
equal `d8a3ac3`. This is not a full injected application crash, abort, native
visual or Windows claim. Product source and the prior full-gate count unchanged.

W10-007's reviewed clean-`24db9c5` optimized-binary experiment completed 27
measured CLI children and 28 authenticated health/count checks without changing
the fixture. Three samples per case show pre-retention value allocation,
increasing notice retention and complete 8 MiB streaming values. Maximum
observed RSS was 42.67 MiB; this is not a process ceiling. Owned export payloads
were checked and removed. Source, helpers, manifest and results are retained
under `/private/tmp/ignatius-w10-007-AFKZrb/`; verification records exact scope.
W10-011's exact-clean-`d8a3ac3` real-writer experiment is complete and reviewed:
12 fresh exports plus 24 unchanged PostgreSQL 18.6/orders=3 checks, no stops,
retries or skips. Small/large paced medians were 8.031/8.109 s with
6.797/40.812 MiB peak RSS; all complete bytes/digests/four counters matched.
Only the 12 verified owned payloads were removed. Sources/report/raw evidence:
`/private/tmp/ignatius-w10-011-d8a3ac3-JVcSVr/`. This proves library-path
backpressure above BufWriter, not CLI cancellation, TLS or process ceilings.

W10-008's incoming-frame design remains unadopted. Root specified W10-013's
32-MiB logical retained-data allowance and typed-unavailability/output contract,
including execution-level disclosure when later nonvisible sets lose data;
its implementation was serialized after W10-012 and is now accepted above.
The earlier separate pure budget candidate passed 24 standalone arithmetic/admission
tests, a non-test warnings-denied build and formatting. The lead verified its
13 recorded artefacts. It is preparation only, with no Session/UI integration
or product memory-limit claim. The final macOS 64-bit ResultSet layout was
subsequently measured as 104 bytes; the accepted code uses each target's layout. Evidence: `/private/tmp/ignatius-w10-013-budget-OJB7A3/`.
Root adopted W10-012's complete notice policy under foundation REL-005, with fixed cumulative
allowances, precision-safe pending counts and explicit acknowledgement. Privacy
implemented in isolated exact `e5b8c72`; PostgreSQL peer reviews immutable core
snapshots. Admission/lifecycle reds and focused greens are retained. Independent
review found probe/describe phase mixing and a dropped pending-shortening edge;
contract `2918069` pins their repair plus nonmutating active-count snapshots at
actual source retirement/quit. Core03 passed its focused repeat and independent
peer re-review with no remaining core blocker; adapter/UI work now includes
explicit captured source retirement and bounded main-loop session installation.
The frozen adapter/UI/live handoff is now applied in the canonical working
tree. Root inspected 128 styled buffers, incorporated the bounded final
runtime/doctor review repair and live CI wiring, and passed the full 1,370-case
local verifier after its test-only transport correction. Root owns shared
records/policy; the notice implementation checkpoint is `d65024b`.
Representative hosted platform/major acceptance has also passed at `fda16f8`;
W10-012 is checked. W10-013 subsequently completed its isolated assignment and
is accepted at `54056c2` as recorded above. Native/release work stays open.

W10-009 is integrated after primary review: 87 private focused passes, followed
by root's 33 CLI units and all 14 new contracts on each plain/verified-TLS
PostgreSQL 18.6 route (61 integrated focused passes, zero skips). The canonical
three-file source/docs change passed the final combined full gate below and
W10-009 is checked. No persistent schema/data changed by its focused checks.

Completed worker lanes: privacy implemented W10-010 in an independent exact-`871db73`
clone/new SSD target: encoder/runtime progress must not count failed rows;
typed abandonment distinguishes confirmed flush from unknown file-row count.
Feature 021 FR-2112/T023-T025 pinned scope before code. PostgreSQL worker completed
frozen Linux arm64 pinned/MSRV/socket/PTY verification at clean `871db73`,
with only its new labelled resources and explicit root/COI preservation. That
frozen source excludes W10-009. Both workers leave canonical source/cache and
docs alone; root owns integration and the combined full gate.

W10-010's six-file patch is now primary-reviewed and applied. It preserves
BufWriter Drop salvage, counts only accepted complete rows and adds typed
flushed/unconfirmed reporting. Private proof: six behavioral reds/four controls,
73 focused passes including three read-only live export contracts, Clippy/fmt;
no native disk-full claim. Exact patch/evidence are retained in
`/private/tmp/ignatius-w10-010-evidence-20261025.txt`. A separate native peer
found no blocker and verified all 33 original moved tests. Feature 021
T023-T025 are checked after the final combined gate. The change is saved in
`918c0b1`.

Root also owns Feature015 T042's specified help follow-up. Four behavioral
regressions reproduced hidden typing/paste/Enter edits and the unreachable final
chord. One additional red proved the blank Space key label. A pure shared help
viewport, local navigation, matching overlay ownership and visible Space names
passed 20 selected cases after additional peer findings, including actual-cell
traversal with valid long/Unicode remaps. Native peer re-review found no remaining
blocker in those fixes. T042 is checked after representative inspection from
104 styled synthetic buffers and the final full gate. Both workers completed
their scoped reviews before the new resource assignments. Help is saved in `d8a3ac3`; root owns the next resource
contracts and coordinated measurement.

The final full verifier at modified `dd69f66`, including W10-009/010/help, passed
1,315 harness cases/1,027 library, formatting, lints/API docs and PostgreSQL 18.6
plain/TLS. The macOS socket remains a semantic skip. Log:
`/private/tmp/ignatius-verify-2026-10-25-help-and-export-final.log`.
The earlier 1,310-case run excludes five later peer regressions/repairs: actual
object Insert choices with/without an inspector, a chosen pane command and long
40x8 controls. All were reproduced, repaired and re-reviewed. The strengthened
actual-last-chord assertion also passed separately after the full run, without
production changes. Styled provenance: `/private/tmp/ignatius-help-styled-_hofm3y7/`.
These are local implementation/focused/full/live/rendered-cell results, not
current hosted/native-platform/hand-use or different-family acceptance.

Frozen `871db73` Linux acceptance/cleanup is complete: full library 996, socket
one, terminal six (1,003 outer harness cases, zero skips), actual pinned Rust
1.97.1 and Rust 1.90 compilation. Sixteen nested guard/hook PTY children are
separate observations, not additional outer passes. Exact scope, source/binary
identity, initdb-auth caveat and reviewed cleanup are in verification.

The lead alone owns CLI/runtime-sidecar writes, documentation, compatibility,
changelog and integration. No shared reducer/runtime writer is permitted.
Linux's clean `871db73` acceptance and task-only cleanup are complete and
exclude later CLI/export/help work. No shared/root fixture may be torn down by
a worker; only its explicitly assigned labelled resources may be cleaned.
Rendered-cell checks
do not establish native terminal appearance or hand-use acceptance.

No recursive agents or shared reducer/runtime writers. Root's PostgreSQL 18.6
fixture pair remains available; unrelated COI services and original branches
are preserved. The owner approved scoped OpenCode Go GLM-5.3 source review on
25 October, using included allowance only, then explicitly chose `Skip external
review` when the paid-fallback setting could not be verified. No code was sent.
This is the owner's exception to different-family review for today's work;
native independent review and local verification remain required.

Detailed sources, environments, exact logs and boundaries are in
[verification](operations/verification.md). Synthetic buffer images establish
rendered cells, not native terminal appearance. Native Terminal computer use
was blocked by tool policy. Scoped external source transmission now has owner
consent, then was cancelled by the owner's `Skip external review` instruction.
Different-family review was not performed. Hosted, Windows, participant,
full hand-use and release acceptance are separate open gates.

### Current security and governance evidence - 2026-10-25

- Fresh no-network/read-only Gitleaks 8.30.0 scans at clean `d8a3ac3`
  found no leaks: all 266 saved commits/11.11 MB and a verified immutable
  424-file tracked-source snapshot/5.14 MB scanned. Manifest tree digest:
  `284fb26412b54b2c568b1c01626a21d568fc6971fbe050fe088e29428062da4d`.
  Logs: `/private/tmp/ignatius-gitleaks-d8a3ac3-{history,source}-2026-10-25.log`.
  These include W10-009/010 and help; later changes need their own boundary.
- A no-network, read-only Gitleaks 8.30.0 history scan actually scanned 239
  commits/10.46 MB with no leaks. A separate 412-file modified-source snapshot
  (manifest tree digest `93b573dbfaffa6cf35d9b3f1346e362e4f03231fae37581a05942dd6ff6032f6`)
  scanned 4.77 MB with no leaks. These cover their recorded source snapshots,
  not future commits. Local logs are `/private/tmp/ignatius-gitleaks-history-2026-10-25.log`
  and `/private/tmp/ignatius-gitleaks-wave2-source-2026-10-25.log`.
- Fresh clean `21511f1` scans passed with no leaks: all 250 saved commits/
  10.77 MB and a verified immutable 417-file source snapshot/4.87 MB. Snapshot
  tree digest is `a1778b16636cb63b89614c11c1bd91b7057e9c037aca12f5883766a13af4ad4e`;
  its manifest is under `/private/tmp/ignatius-source-21511f1-q24p8etp/`.
  Logs use `/private/tmp/ignatius-gitleaks-21511f1-*-2026-10-25.log`. Later
  density changes are outside these frozen scans.
- Scheduled remote CI run `39226168252`, inspected read-only, failed actual
  rustls advisories and mutable PostgreSQL image drift on 19 October. Native
  OS jobs passed at remote `445ed82`; this is not today's local-candidate CI.
  Its drift loop stopped at 14, so that run proves nothing about 15-18 drift.
- Fresh maintainer/advisory checks led to precise patches: rustls 0.23.46,
  rustls-webpki 0.103.16, ctutils 0.4.4, Tokio 1.53.3 and webpki-root-certs
  1.0.11. All other lockfile versions and dependency edges were preserved.
  The all-workspace/all-features locked cargo-deny 0.20.2 audit passed with zero
  errors; six existing duplicate-version warnings remain. The same broader
  audit found missing xtask licence metadata, now the existing Apache-2.0
  licence. No audit exception or allowance was added. See
  [verification](operations/verification.md#local-security-maintenance---2026-10-25)
  for primary advisory sources and exact local scope.
- Live GitHub readback confirms public visibility; active main ruleset
  `22835887` requires PRs, resolved review threads and 13 CI contexts, with no
  bypass actors. Required approvals are zero, code-owner approval and strict
  up-to-date checks are off, and the three observed CodeQL contexts are absent.
  The `v*` tag ruleset `22835888` prevents updates/deletion. No setting changed.
- Private vulnerability reporting, secret scanning, push protection and
  Dependabot security updates are enabled. The private reporting route was not
  submitted to or tested for notification/response. Classic protection reads
  returned 403 through the integration and 404 through the owner CLI; neither
  negates the independently observed active rulesets. The release collection
  was empty. Naming/legal/domain and exact release actions remain owner gates.

Owner requested day-long autonomous development and a bounded team. Start:
`39d7923cf3b7d132c12a98bc081a454e45e1ec58`, clean branch
`w01/reconcile-1d841b4-onto-bd05b9d`, 16 commits ahead of the local tracked
`origin/main` before the read-only remote refresh described below. The prior local
commits and unrelated COI Docker containers are preserved. The active-feature
pointer was stale at 024 and now matches 025, whose US1-US4 are recorded in
the historical sections below; US5-US10 remain unchecked.

First-batch lanes (single writers, no recursive delegation) were W02 async-envelope
repair in the interactive runtime/reducer sidecar tests; W04 generated-UPDATE
privacy probes in a new integration-test file; lead-owned W10 saved-query and
atomic local-state recovery. Native workers inherit the lead's model settings.
The existing OpenCode Go connection and configured GLM-5.3 reviewer are
available for the required review on another model family; no overage or new
provider is authorised. The roadmap now reflects the existing 025 work rather
than describing 024 as the latest slice.

Automatic approval review rejected the attempted read-only OpenCode review
because transmitting unpublished repository changes to that external service
needs explicit owner consent. No review payload was sent then. The owner
subsequently approved scoped OpenCode Go GLM-5.3 review, limited to included
subscription allowance. Current vendor documentation says Extra Usage can
fall back to paid Console credit; the CLI does not expose that setting and
the browser Console requires sign-in. The owner answered the billing-fact
question with `Skip external review`. No inference or source transmission ran;
that instruction is the owner's exception to today's different-family gate.
Cross-lane native peer review found one W10 regression
(temporary basename too long); it was reproduced, repaired and re-reviewed.
Native independent review and local acceptance still apply; no different-family
review result is claimed.

Environment: macOS arm64, Homebrew Rust/Cargo 1.99.1 (the repository pins
1.97.1 for rustup, so this is explicitly a different local toolchain),
`DEVELOPER_DIR=/Library/Developer/CommandLineTools`, existing build cache on
the external SSD. Lead started only the repository's disposable PostgreSQL
18.4 plain/TLS fixtures. A bounded keep-awake process covers today's local run.
Phone pairing and mobile push delivery have not been verified; owner-only
decisions must remain actual chat/approval requests, never simulated input.

No new push, workflow dispatch, tag, signing, release, paid resource,
governance change or production operation is authorised by this progress
record. New local branch: `codex/autonomous-roadmap-2026-10-25`. The remote-main
query returned `445ed8228d344a160928456659c7ba932d4cbdef`. SSH fetch could not
use the existing signing agent; the authenticated GitHub connector confirmed
the repository is public, and a read-only HTTPS fetch succeeded. Its only new
changes since the shared baseline were lockfile dependency patches and CodeQL
action pins. Local merge `4cdf96e` preserves both histories and changed only
`Cargo.lock` and `.github/workflows/codeql.yml`. This did not merge anything on
GitHub or publish today's changes.

### First hardening batch - local verification

| Field | Record |
| --- | --- |
| Changes | W02 queues profile failures/catalogue reloads with connection generation and rechecks the reconnect-target slot under its lock. W04 omits parameter-derived server prose from parameterized errors and enables sticky value-free notices. W10 uses private exclusive atomic replacements and refuses linked/non-regular saved-query leaves. |
| Source | Starting commit `39d7923` plus the current explicitly modified/untracked files on the new local branch; no hosted or clean-candidate claim. |
| Environment | macOS 27.0.1 arm64, Homebrew Rust/Cargo 1.99.1, CommandLineTools developer directory, existing external-SSD build cache; disposable server reports PostgreSQL 18.4. |
| Focused evidence | W02 sender and target-lock regressions failed before fixes; interactive 19 and reducer 171 passed. W10 original temporary/link probes failed before fixes; configuration 13, history 14, query 10 and atomic helper 6 passed. W04 generated constraint failure leaked before the correction; 8 error units, 4 session units and 5 live plain PostgreSQL subprocess privacy contracts passed after it. |
| Full gate | First run failed a roadmap phrase-parity assertion after 898 library tests passed. Corrected the meaningful Feature 024 reference. The repeat `cargo --locked xtask verify` passed formatting, warnings-denied lints, API docs, the complete workspace and doc tests, including plain/TLS PostgreSQL. The harness reports 1,152 passing cases across 21 summaries; its socket case returns early and remains a semantic skip, not live evidence. |
| Verifier repair | Added workspace `--no-fail-fast` so later binaries run after failure. A failed workspace gate now labels configured fixtures as information rather than falsely reporting database/socket passes. |
| Review | Cross-lane native peer review found and re-checked the repaired W10 long-basename regression and found no remaining blocking W02/W04/W10 issue in scope. The owner subsequently chose `Skip external review`; different-family review was not performed, under that explicit exception. |
| Retention | Safe local verifier logs: `/private/tmp/ignatius-verify-2026-10-25-wave1.log` and `/private/tmp/ignatius-verify-2026-10-25-wave1-recheck.log`; regressions retained in sidecar/integration tests. |
| Open boundaries | Unix socket, native Windows/Linux, real terminal/screen-reader use, packaged-candidate privacy and release gates. External review was later skipped by explicit owner instruction. The separate verified-TLS repeat of all five privacy contracts passed with zero skips using the generated fixture CA and `sslmode=verify-full`. |

Local commits: `f8cbce8` W02 identity, `2017f03` W10 replacement boundaries,
`b845106` W04 privacy and its specified next W03 guard. The platform's existing
inline tests were also moved unchanged to a sidecar to follow the CodeQL test
path contract. The first full pass above used the pre-merge lockfile. The repeat
`cargo --locked xtask verify` against integrated `4cdf96e` plus the verifier
workflow/documentation edits also passed every enabled gate, with the same
1,152 harness cases and semantic socket skip. Its retained local log is
`/private/tmp/ignatius-verify-2026-10-25-wave1-upstream.log`. `actionlint` also
passed against the integrated workflow definitions. Hosted CI, cargo-deny,
gitleaks and CodeQL were not run in this batch.

Next independent work: the specified FR-2215 fresh string-semantics guard
(Feature 022 W03-034/035), W10 migration-backup exclusive reservation, and the
remaining Feature 025 stories. US5 preparation found contract details to pin
before coding: text-token contrast scope, exact theme-refusal rendering and
missing custom-definition wording, and late OSC 11 reply filtering. No theme
contract was silently changed to resolve those questions.

## W10 streaming export footprint 200k vs 2M rows - 2026-09-30

Measured the acceptance streaming-growth scenario on branch
`w01/reconcile-1d841b4-onto-bd05b9d` with the release binary against
disposable PostgreSQL 18.4 on localhost: `SELECT g, md5(g::text) FROM
generate_series(...)`, two fixed columns, CSV to a file, peak RSS from
`/usr/bin/time -l`. Machine: Mac Studio arm64, rustc 1.98.1, ignatius 0.1.0
release profile.

| Rows | Output bytes | Wall time | Peak RSS |
| --- | --- | --- | --- |
| 200,000 | 7,888,903 | 0.22 s | 2,933,216 |
| 2,000,000 | 80,888,904 | 1.77 s | 2,916,832 |

Ten times the rows adds sixteen kilobytes of peak memory: retained growth is
flat, not proportional. Time scales with bytes at roughly 45 MB/s on
localhost. Still open in W10: whole-process ceiling for one oversized value
or notice, p95 input/grid latency baselines, and the recovery matrix.

## W05 local server matrix 14-18 on the integration branch - 2026-09-30

Ran the full 53-test plain/TLS integration suite on branch
`w01/reconcile-1d841b4-onto-bd05b9d` against each supported major from the
reviewed digest map in `docker/postgres-images.env`, one disposable server at
a time on macOS arm64 with Docker Desktop. Every major passed 53/53:

| Major | Server version | Result |
| --- | --- | --- |
| 14 | 14.24 on aarch64-unknown-linux-musl (Alpine) | 53 passed |
| 15 | 15.19 on aarch64-unknown-linux-musl (Alpine) | 53 passed |
| 16 | 16.15 on aarch64-unknown-linux-musl (Alpine) | 53 passed |
| 17 | 17.11 on aarch64-unknown-linux-musl (Alpine) | 53 passed |
| 18 | 18.4 (existing `cargo xtask verify` gates) | pass |

One procedural note: the tests read the password from `IGNATIUS_TEST_PG_URI`
itself, so the URIs were exported with the synthetic fixture password via a
variable reference, never on the command line. The Unix-socket gate stays
skipped and the hosted matrix at candidate source stays open.

## W01/W02 integration branch: connection identity and bounds on the 025 base - 2026-09-30

Branch `w01/reconcile-1d841b4-onto-bd05b9d` (based on `origin/main` bd05b9d)
ports the local-only hardening onto the 025 base without dropping remote
behaviour: the reducer `ForConnection` envelope with generation increment on
profile switch, runtime generation checks plus envelope wrapping in connect,
execute, parameterized, explain, and cancel workers, provider `kill_on_drop`
with ms-aware timeout wording, 1 MiB retained-value refusal, 64 KiB notice
cap, and partial-acquisition terminal restore. The xtask second redactor was
deliberately not ported; the central redactor remains the single
implementation. Grid MIN/STEP stay at the remote 4/2. A direct merge into
local `main` 1d841b4 was attempted with `--allow-unrelated-histories` and
aborted: the histories share no merge base, every file conflicts, and an
automatic merge would risk dropping one side. Old `main` is retained
untouched; the forward line is this branch via pull request.

| Field | Record |
| --- | --- |
| Claim | W01 behaviour preservation plus W02 envelope/worker scoping and regression tests |
| Source | `w01/reconcile-1d841b4-onto-bd05b9d` e7b8059, pushed to origin; local `main` 1d841b4 retained |
| Environment | macOS arm64, Docker Desktop `desktop-linux`, disposable `postgres:18.4-alpine` plain/TLS fixtures |
| Method | `cargo xtask db up`; `cargo xtask verify`; `cargo xtask db down`; focused `app::update` envelope tests (2 new) |
| Result | Pass. Formatting, lints, API docs, workspace tests, doc tests, and database-backed plain/TLS gates all passed. Unix-socket slot skipped because `IGNATIUS_TEST_PG_SOCKET_URI` is not set. Pre-push hook passed with DB gates skipped (no development database in hook context) |
| Scope | Proves reducer envelope rejection and worker generation checks on the current branch. Does not prove hosted Linux matrix, socket job, Windows ConPTY, hand-terminal, provider-account, signing, provenance, or publication readiness |
| Retention | This section, the pushed branch, and PR https://github.com/tkomane/ignatius/pull/new/w01/reconcile-1d841b4-onto-bd05b9d |

## Feature 025 Phase 6: the first frame is useful (US4) - 2026-09-18

US4 is implemented on the `025-us4-first-frame` branch. The reducer and the
runtime gained the pinned connecting vocabulary (`resolving profile`,
`acquiring credential`, `TLS handshake`, `server handshake`, `loading
catalogue`): the runtime reports only the stage it can honestly observe
(plain or `sslmode=prefer` targets name the server handshake; a target that
requires encryption names the TLS handshake), the header shows the stage
with the target and the elapsed time in words at every tier, a failure keeps
the stage it failed in beside the unchanged diagnostic, and a real
connecting-to-connected transition focuses the editor and opens the tree's
first schema level once. An unconfigured first frame names one next action
instead of empty panes.

| Field | Record |
| --- | --- |
| Claim | T024-T027: FR-2510..FR-2512, SC-2505 - named connecting steps, a painted first frame, a productive first connected frame |
| Source | `025-us4-first-frame` branch, verified before the Phase 6 commit; based on `origin/main` c0045f6 |
| Environment | macOS 26.6.1 arm64, Homebrew Rust/Cargo 1.98.1, `DEVELOPER_DIR=/Library/Developer/CommandLineTools` because the Xcode licence is not accepted; disposable `postgres:18.4-alpine` plain/TLS fixtures |
| Method | `cargo fmt --all -- --check`; workspace clippy with warnings denied; focused sets (`ui::widgets::header` 9, `ui::widgets::objects` 7, `app::update` 169, `ui::widgets` 48, `--lib` 869); `cargo --locked xtask db up`; `cargo --locked xtask verify` (20 test binaries, 1,111 tests passed); `cargo xtask db down`; independent review of the whole slice |
| Result | Pass. All gates passed including the database-backed plain/TLS gates; the Unix-socket gate skipped because `IGNATIUS_TEST_PG_SOCKET_URI` is not set. Review found no blocking or substantive issues and nine minor ones; the two it recommended in-slice (a generation check before the step report, and a pure sslmode-to-stage test) plus six others were fixed, with the remaining honesty limit recorded below; re-review accepted |
| Scope | Proves reducer transitions, runtime step selection (by test of the pure mapping), and buffer-level rendering. Does not prove live terminal rendering, runtime timing under a real connect, Windows/Linux, or the catalogue-loading step as a visible transient: `loading catalogue` is the tree's own state, and the runtime's single opaque `session::connect` call cannot report a post-TLS failure separately from the TLS stage, so a `Require` target's authentication failure displays as `failed during TLS handshake` beside the unchanged diagnostic |
| Retention | This section, the `025-us4-first-frame` pull request, and the commit that carries it |

The task list names `src/connection/service.rs` as the step-reporting home,
but that file holds `pg_service.conf` parsing; the connect flow lives in the
runtime, so the vocabulary is in `src/app/model.rs` and the emissions are in
`src/cli/interactive.rs`. Nothing in FR-2510..FR-2512 requires a specific
file, and the task's focused `connection::service` command proves nothing
about this slice.

## Feature 025 Phase 5: a workable results grid (US3) - 2026-09-18

US3 is implemented on the `025-us3-grid` branch. The Results state line now
opens with the viewport range in the contract's shapes (`rows 120-160 of
1,248 retained`, the truncated and filtered forms, and no range rather than
`rows 0-0` in a pane with no room for a data row), beside the pinned
`sorted by {column} ascending|descending` wording and the existing server
order note. Column widths are the contract's minimum 4, maximum 80 and
2-cell keyboard step, and `Ctrl+K [` / `Ctrl+K ]` (configurable
`narrow-column` / `widen-column`) resize the selected column locally with no
execution effect and the selected record preserved. The grid contract's
NULL, empty and literal-`NULL` distinction is asserted on painted stripes at
all four colour depths; the keymap document carries both chord rows and both
bindable names, and the parity suite passes.

| Field | Record |
| --- | --- |
| Claim | T020-T023: FR-2507..FR-2509, SC-2507 - exact range wording, three-state sort wording, bounded resizing, keyboard reachability, zero executions |
| Source | `025-us3-grid` branch, verified before the Phase 5 commit; based on `origin/main` a165ebd |
| Environment | macOS 26.6.1 arm64, Homebrew Rust/Cargo 1.98.1, `DEVELOPER_DIR=/Library/Developer/CommandLineTools` because the Xcode licence is not accepted; disposable `postgres:18.4-alpine` plain/TLS fixtures for the verifier |
| Method | `cargo fmt --all -- --check`; workspace clippy with warnings denied; focused sets (`app::grid` 18, `ui::widgets` 32, `ui::keymap` 29, `app::discovery` 12, `app::update` 163); `--test documentation_matches_the_build` 15 and `--test keymap_contract` 2; `cargo --locked xtask db up`; `cargo --locked xtask verify` (20 test binaries, 1,090 tests passed); `cargo xtask db down` then `cargo xtask db status` reported `Not running.`; independent review |
| Result | Pass. All gates passed including the database-backed plain/TLS gates; the Unix-socket gate skipped because `IGNATIUS_TEST_PG_SOCKET_URI` is not set. Review found one substantive gap (no reducer-level test for the two new resize actions) and two minors (a degenerate-pane range overclaim and the footer hint cap hiding the new hints); the first two were fixed with tests watched failing first and re-review accepted, the third is recorded for the T046 documentation pass |
| Scope | Proves the pinned wording and bounds, the local-only resize and sort transitions, keymap reachability and the NULL distinction at buffer level. Does not prove live terminal rendering, mouse resizing (US6) or cross-platform behaviour |
| Retention | This section, the `025-us3-grid` pull request, and the commit that carries it |

## Feature 025 Phase 4: painted surfaces and colour depth (US2) - 2026-09-18

US2 is implemented on the `025-us2-surfaces` branch. The frame now owns its
background: at full-colour and 256-colour depth every cell is painted from
three elevation levels (`Surface`, `SurfacePane`, `SurfaceOverlay`), panes
and overlays carry their own surfaces with Clear-then-block overlays, and at
16 colours and below the same words render with no painted surface and the
existing modifier-only rules. A deterministic 6x6x6-plus-grey quantizer and
a fixed 16-colour family table sit behind one depth-aware conversion point
in `src/ui/theme.rs`. Detection follows the pinned precedence (forced-off,
then `--color-depth`, then `ui.color-depth`, then `COLORTERM`, then `TERM`,
then the 16-colour default) and is a pure function of captured
`TerminalFacts`. `doctor` reports the effective depth and its source label;
the `[ui]` template and the global `--color-depth` flag land with the
schema.

| Field | Record |
| --- | --- |
| Claim | T013-T019: FR-2503..FR-2506, SC-2502, SC-2503 - painted surfaces at truecolor/256, honest 16-colour and off, deterministic quantization, depth reporting |
| Source | `025-us2-surfaces` branch, verified before the Phase 4 commit; based on `origin/main` 889efa3 |
| Environment | macOS 26.6.1 arm64, Homebrew Rust/Cargo 1.98.1. The Xcode licence is still not accepted, so every cargo command used `DEVELOPER_DIR=/Library/Developer/CommandLineTools`; no system setting changed. Disposable `postgres:18.4-alpine` plain/TLS fixtures for the verifier |
| Method | `cargo fmt --all -- --check`; workspace clippy with warnings denied; the focused module filters (`ui::theme` 20, `ui::terminal` 10, `ui::widgets` 25, `ui::layout` 88, `config` 42, `diagnostics` 39, `cli` 97); `cargo --locked xtask db up`; `cargo --locked xtask verify` (20 test binaries, 1,075 tests passed); `cargo xtask db down`, then `cargo xtask db status` reported `Not running.`; independent review of the full slice |
| Result | Pass. Formatting, lints, API documentation, workspace tests, documentation tests and the database-backed plain/TLS gates all passed. The Unix-socket gate skipped because `IGNATIUS_TEST_PG_SOCKET_URI` is not set; that is a skip, not a pass. Every contract starting hex passed the extended contrast tests unchanged, so no adjustment was made. Review found no blocking or substantive issues and three minor ones (a wrong `no_color` doc comment, a full-frame test narrower than SC-2502's wording, and missing grey/cube-vs-grey quantizer tie assertions); all three were fixed and re-checked |
| Scope | Proves buffer-level rendering, quantization, detection, configuration refusal and doctor wording. Does not prove live terminal rendering at any depth, Windows or Linux behaviour, or the OSC 11 `auto` path (a later phase). The feature's own plan names Windows terminal evidence as an outstanding gap |
| Retention | This section, the `025-us2-surfaces` pull request, and the commit that carries it |

## Feature 025 Phase 3: pasted SQL arrives (US1) - 2026-09-18

US1 is implemented on the `025-us1-paste` branch. A delivered paste now
routes through the reducer to the surface that owns input, in the same peel
order a keystroke takes: the editor (at the caret, CRLF and CR normalised to
LF, one non-coalesced undo step, text over 1 MiB refused with the limit
named), masked password and parameter prompts, the name and update prompts,
the palette query, and the tree and result filters. A surface that owns
input but does not accept text sets `PasteNotice::Ignored`; no route emits
an effect, so a paste can never execute SQL. The editor route goes through
`edit_editor`, so a landed paste invalidates a stale server error marker and
the format notice exactly as typing does, and the footer rail shows the
newest paste or format notice.

| Field | Record |
| --- | --- |
| Claim | T010-T012: FR-2501 and FR-2502, no paste silently dropped, one undo step, bounded and normalised |
| Source | `025-us1-paste` branch, verified before the Phase 3 commit; based on `origin/main` 77c4124 |
| Environment | macOS 26.6.1 arm64, Homebrew Rust/Cargo 1.98.1. Mid-session the linker began refusing because the Xcode licence was not accepted; focused runs then used `DEVELOPER_DIR=/Library/Developer/CommandLineTools` per command. No system setting was changed; accepting the Xcode licence remains an owner action |
| Method | `cargo fmt --all -- --check`; workspace clippy with warnings denied; `cargo test --locked --lib app::editor` (27), `--lib app::update::tests::paste` (14), `--lib ui::widgets::footer` (6), `--lib` (799), `--test documentation_matches_the_build` (15), `--test release_notes_contract` (7) |
| Result | Focused and full-library sets pass, no failures and no skips. The reducer and editor tests were watched failing against the inert arm before the routing landed. Independent review found two substantive defects (a paste bypassed `edit_editor`, leaving a stale error marker and format notice; a refusal or ignored notice was invisible while a format notice was up) and three minor ones; all five were fixed with regression tests, and the review accepted the slice |
| Scope | Proves library-level paste routing, the editor edit semantics, the notice rail and the privacy boundary for masked prompts. Does not prove live terminal paste delivery in Warp or Windows Terminal, cross-platform line-ending delivery, or non-macOS builds |
| Retention | This section, the `025-us1-paste` pull request, and the commit that carries it |

The full `cargo --locked xtask verify` remains the Phase 12 gate (T050). A
live terminal paste check is a manual evidence gap.

## Feature 025 Phase 2 foundational extraction - 2026-09-18

Phase 2 (T002-T009) of `specs/025-gui-grade-experience/` is implemented on the
`025-gui-grade-experience` branch and merged as its own pull request before
any later phase starts. The 6,553-line `src/ui/layout.rs` became seven per-pane
widget modules under `src/ui/widgets/`; `layout.rs` keeps the orchestrator role
plus the paging and viewport helpers. A pure
`region_at(model, area, column, row)` returns `Region` values derived from the
same layout arithmetic the renderers use. `Presentation` is model state built
in `src/cli/interactive.rs` at startup, and `Message::Mouse` and
`Message::Pasted` are translated from terminal events into exhaustive inert
reducer arms. No rendered output changed.

| Field | Record |
| --- | --- |
| Claim | T002-T009 (structure only): the extraction is behaviour-preserving and the hit regions agree with what is drawn |
| Source | `025-gui-grade-experience` branch, verified before the Phase 2 commit; based on `origin/main` 66e28cd |
| Environment | macOS 26.6.1 arm64, Homebrew Rust/Cargo 1.98.1 |
| Method | `cargo fmt --all -- --check`; workspace clippy with warnings denied; `cargo test --locked --lib ui::layout`, `--lib app::update`, `--lib ui::widgets`, `--test documentation_matches_the_build`; the independent reviewer diffed every moved function against `git show 66e28cd:src/ui/layout.rs` and re-ran the focused sets |
| Result | Focused: 87 layout, 148 app::update, 15 widgets and 15 documentation-parity tests pass; clippy clean; every moved function byte-identical apart from visibility and path qualification. Review found one substantive hit-region defect (a filtered tree's rows were off by one in `src/ui/widgets/mod.rs`); it was fixed with the regression test watched failing first, and the review then accepted the extraction |
| Scope | Proves the extraction changed no rendered output in the render-to-string suites and that `region_at` matches the drawing arithmetic in both layout modes. Does not prove live terminal rendering, mouse behaviour (nothing consumes `region_at` until US6), or any later story |
| Retention | This section, the `025-gui-grade-experience` pull request, and the commit that carries it |

The full `cargo --locked xtask verify` was not run in this phase; the Phase 12
verifier run is the full-suite and live evidence gate. Unix-socket, Windows
input and live-terminal claims remain unverified.

## Feature 025 GUI-grade experience: specified; Phase 2 merged - 2026-09-17

The owner reported abandoning Ignatius for the VS Code Azure PostgreSQL
extension during real work against an Azure PostgreSQL database on
2026-09-17 because the extension was cleaner and simpler. That observation,
recorded honestly as sample size one, is the W09 observed-friction evidence
behind `specs/025-gui-grade-experience/`: a complete specification, plan,
research record, data model, four pinned-value contracts, quickstart and a
worker-decomposed task list, authored on the `025-gui-grade-experience`
branch. The feature covers painted surfaces with a three-level elevation
ladder and a colour-depth tier, user-owned and runtime-switchable themes
with a truthful OSC 11 `auto`, a workable results grid, named connecting
steps under a fully painted first frame, first-class mouse support with
full keyboard parity, movable and zoomable panes with a density setting,
quiet threshold-gated chrome, palette previews, and the paste fix. It links
to outcomes O2 and O3 and journeys J1, J2 and J3. `.specify/feature.json`
now names the 025 directory.

One defect was confirmed by code reading during specification: bracketed
paste is enabled (`src/ui/terminal.rs`) but no `Event::Paste` consumer
exists (`src/cli/interactive.rs` drops it), so pasting into the full-screen
client does nothing. Live terminal behaviour was not exercised; the finding
is from reading, and the fix is specified as User Story 1, not applied.

The task list is decomposed for the OpenCode multiagent setup in
`docs/operations/agent-orchestration.md`: a serialized foundational
extraction of `src/ui/layout.rs` into widget modules gates everything, the
five single-writer files route through one integrator lane, each slice
carries a pre-filled assignment block with owned paths, focused commands
and stopping conditions, and design-document amendments land with the
behaviour (keymap rows in the same change as their actions; narrative
documents in the Polish phase). Implementation started on 2026-09-18 with
the Phase 2 extraction recorded above.

The owner added a tenth user story on 2026-09-18, "The name materializes":
an original ASCII identity mark in the README and a bounded boot reveal in
the interactive client, riding the connecting wait. The specification, a
fifth pinned-value contract (`contracts/banner.md`: art, sweep, easing,
decode charsets, colour depth and README placement), the US10 tasks
T052-T054 and the T046 design-document amendments were added in the
specification change described here. Nothing in this amendment has been
implemented; the reveal's colour path depends on the US2 depth machinery
and its placement on the US4 connecting shell.

Design research was performed on 2026-09-17 by web reading, one pass per
source with citations and stated gaps, recorded in the feature's
`research.md`. Verification of this documentation-only change: the working
tree diff touches only the 025 specification directory,
`.specify/feature.json` and this file; the documentation-parity suite was
run against the unchanged design documents. The application test suite was
not rerun, no database fixture was started, and `cargo xtask verify` was
not run. The specification was committed on the `025-gui-grade-experience`
branch and submitted through the ordinary pull-request gate on 2026-09-17
at the owner's request; no tag, signing, release or publication was
performed.

## CodeQL test exclusion and advanced setup - 2026-09-11

CodeQL's default setup reported 32 high-severity `rust/cleartext-logging`
findings, all on test assertions that deliberately format password-like values
to prove they never reach output. Rust has no inline alert suppression yet
(github/codeql#21637), and default setup cannot be configured for a user-owned
repository, so the repository moved to advanced setup
(`.github/workflows/codeql.yml`) with a configuration file
(`.github/codeql/codeql-config.yml`) that ignores `tests/**` and `**/tests.rs`.

The unit test modules moved out of `src/app/update.rs` and
`src/connection/target.rs` into `tests.rs` files beside them so the exclusion
covers them; production sources keep the full default query set. The existing
32 findings are dismissed as test-only, and the first hosted run of the advanced
workflow on the pull request is the proof that the noise is gone.

## Public repository and guardrails - 2026-09-11

**Decision.** The owner made `tkomane/ignatius` public, for an operational
reason: every CI job was rejected before it started because the private Actions
quota was exhausted and the account's spending limit is zero. Public
standard-runner minutes are free, and the previously blocked runs executed
immediately after the change. `docs/operations/release.md` had deferred public
source until CI was green on all three platforms; that prerequisite was not met
at the time (the active W03 branch was red on Windows), so this is an owner
decision that overrides the deferred route, not completion of it.

**Pre-publication audit.** The full 97-commit history was scanned with the same
digest-pinned Gitleaks image CI uses: no leaks. No employer or client content,
private keys, credential files or internal endpoints were found, but the scan
missed one unexpanded internal acronym in three documents. It was generalised
across those documents on 2026-09-11 so the tree carries no client identifier.
The only credentials in the tree are the documented synthetic container
fixtures. Commit metadata includes the owner's public commit address.

**Enabled the same day.** Secret scanning and push protection, private
vulnerability reporting, CodeQL default setup (auto-detected rust, python and
actions; the setup API's explicit language list does not yet accept rust), an
Actions allowlist with SHA-pinning required, approval for all external fork
contributors, deletion of merged branches, a ruleset requiring a pull request
and all 13 CI checks on `main` with resolved review threads, and a ruleset
protecting `v*` tags from updates and deletion. The free plan does not include
non-provider secret patterns or validity checks; those remain disabled.

**Diagnosed and fixed: the Windows-only failure.** GitHub's Windows runner
image sets `PGPASSWORD` machine-wide for its preinstalled PostgreSQL.
`prepare_profile_target` read the process environment directly, so the picker
test's `target.password.is_none()` assertion failed on Windows for a reason that
had nothing to do with the profile under test. The picker body now takes an
`EnvSnapshot`: production still reads the real environment, and the test supplies
a fixed empty snapshot, matching the existing "resolution is a pure function"
rule in `src/connection/target.rs`. Focused evidence on 2026-09-11: `cargo fmt
--all --check`, workspace clippy with warnings denied, the three
`cli::interactive::tests::a_picker` tests, the 15 documentation-match contracts
and the 7 workflow contracts all pass locally. The first hosted Windows run for
the branched revision passed on 2026-09-11 in 2m40s, and the merge's
main-branch run passed as well; the check suite on pull request 4 is the
record. `Cargo.toml` named `github.com/tshiamo/ignatius` in its
`repository` field, which does not resolve to the public source; it now points
at `tkomane/ignatius`, and a later move of accounts can update the one field
again.

## W04 privacy probes and provider redaction fix - 2026-09-11

The W04 runtime-privacy probes found a real leak, and the fix ships with its
reproduction. A provider program that writes `Authorization: Bearer <token>` to
standard error produced a diagnostic in which the token survived, because the
single redaction implementation recognised URI userinfo and libpq keyword/value
secrets but not HTTP authorization headers. The redactor now covers inline
`Authorization` and `Proxy-Authorization` values (unquoted, quoted and
repr-style), keeps a known scheme visible, and removes bare bearer tokens.
Overlapping matches are merged, so a shorter match cannot leave a tail behind.
`docs/security/data-handling.md` gained the explicit redaction boundary, and
the threat-model row names the covered shapes and the residual folded or
unusual-shape limit.

Three probes accompany it: a live `IGNATIUS_LOG=debug` run with a named
parameter proves the expanded value reaches neither the log nor stderr; a
provider that never answers proves the timeout path yields a Connection
diagnostic; and the provider stderr regression test proves the redacted reason
survives rather than being dropped.

| Gate at the final source | Result |
| --- | --- |
| `cargo test --locked --lib diagnostics::redaction` | 15 passed |
| `cargo test --locked --lib connection::cloud` | 16 passed |
| `cargo test --locked --test cli_contract debug_logging_never_captures` | 2 passed, live against PostgreSQL 18.4 |
| `cargo --locked xtask verify` | Pass; full suite, plain and TLS database gates, one Unix-socket skip |

Not proven: the packaged-candidate repeat (W12), the panic path, the generated
update and refresh or clipboard log audits, and any non-macOS platform. No
product behaviour changed outside the redactor.

## W01 reconciliation and W03 reviewed-write evidence - 2026-09-10

**Baseline.** The working tree was clean at `247d1c7` and 21 commits behind
`origin/main`; a fast-forward to `6df522e` ("Record main integration state")
brought in the integrated chain for Features 012 through 024. Every task box in
`specs/012-schema-completion/tasks.md` through
`specs/024-retained-result-refresh/tasks.md` is checked, so the handoff that
named schema completion as the next priority predates the merge. The active
priorities are the delivery plan's W01 to W05 hardening packages, not a new
feature. W01 reconciliation is complete for the source tree: `HEAD` is
`6df522e`, the tree was clean before this session's changes, and the inherited
evidence remains valid only for the revisions it names.

**Fixed: `FROM ONLY` misresolved its relation.** `parse_relation` in
`src/query/update.rs` accepted the keyword `ONLY` as the relation name for
`SELECT ... FROM ONLY relation`, then took the real relation as its alias. With
a relation named `only` on the search path, a generated cell update could be
sent to that relation using row identity read from another table's result. The
source is now refused with a truthful reason; a quoted relation named `"only"`
still works. The new test was watched failing before the fix, and the
independent review confirmed no other accepted shape regressed. The
specification Edge Case, the compatibility wording, a checked T020 task and an
Unreleased CHANGELOG fix record the behaviour.

**W03 evidence: reviewed-write identity and truthfulness.** Four live tests
against disposable PostgreSQL 18.4 prove a composite quoted primary key (two
ordered predicates, exactly one intended row changed, neighbours untouched),
that review does not lock a row (a concurrent delete reports zero rows; a
concurrent non-key change is overwritten and reports one), and that
`restricted_reader` metadata is readable but not writable while the server
itself refuses the generated update with SQLSTATE 42501. A reducer assertion
proves the emitted parameterized effect binds to exactly the reviewed
`bound_sql`. The accepted concurrency semantics are documented in
`compatibility.md`: no lock, no conflict detection, no retry, no rollback.

| Gate at the final source | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked --lib query::update` | 10 passed |
| `cargo test --locked --lib app::update` | 148 passed |
| `cargo test --locked --test documentation_matches_the_build` | 15 passed |
| `cargo --locked xtask verify` | Pass; 995 workspace tests, 53 PostgreSQL integration tests live |
| Unix-socket integration | Skip; `IGNATIUS_TEST_PG_SOCKET_URI` is not set |

That verify run counted 995 workspace tests: 754 library, 3 archive helper, 51
CLI contract, 4 cloud identity, 15 documentation matches, 3 editor contract, 2
keymap contract, 1 native boundary, 3 password prompt, 53 PostgreSQL
integration, 1 Unix-socket harness returning early (the separate semantic skip
above), 80 release contract, 7 release notes, 2 release runtime, 1 release
schema, 3 terminal restoration, 7 workflow contract and 5 xtask tests, plus 0
documentation tests. No test failed.

Environment: macOS 26.6.1 on Apple silicon, rustc and cargo 1.98.1 (Homebrew),
image `postgres:18.4-alpine` digest
`sha256:9a8afca54e7861fd90fab5fdf4c42477a6b1cb7d293595148e674e0a3181de15`.
The container fixture ran on the deliberate CLI-only colima runtime; Docker
Desktop was uninstalled on 2026-09-10 and must not be reinstalled. A missing
`docker-credential-osxkeychain` made the standard compose pull fail until
`brew install docker-credential-helper` restored it. The standard path was then
re-validated with `cargo xtask db up` and the focused live test, and the
disposable services were removed with `cargo xtask db down`, after which
`cargo xtask db status` reported `Not running.` The runtime rules and the
colima lifecycle are recorded in `docs/operations/agent-orchestration.md`.

**Orchestration.** Project-local roles (explorer, implementer, reviewer),
routing, fallback and dated OpenCode Go offer observations are recorded in
`docs/operations/agent-orchestration.md`, with a schema-only `opencode.json`
and `.opencode/agent/` role files. Only OpenCode Go is authorised; no overage,
auto-reload or new provider was enabled. Independent review of both slices
found no blocking or substantive issues; minor observations are in the review
returns. This session ran its workers on the default `deepseek-v4.1-flash`
because agent-level model routing takes effect only after an opencode restart.

**Not proven.** Manual terminal use of the new refusal, Windows and Linux hand
checks, the Unix-socket route, PostgreSQL 14-17 for the new evidence (18.4
only), live cloud-provider authentication, and anything release-related. The
changes are committed on the `w03-reviewed-write-evidence` branch for
integration; no tag, signing, release or publication was performed.

**Integration state.** The repository became public on 2026-09-11 (owner
decision; see the section above) and hosted CI started executing. The Windows
job failed for an environment reason: the runner image sets `PGPASSWORD`
machine-wide and the picker test read the process environment. Pull request 5
isolated that test from ambient credentials, added the public repository
guardrails and merged as `ba284f9`; this branch is rebased on it. Hosted checks
for the rebased revision are recorded on pull request 4. The local
`cargo --locked xtask verify` result above, with the disposable database
running, remains the strongest local evidence.

## Agent planning handoff - 2026-09-04

The [roadmap](product/roadmap.md) now defines the path from the local feature
chain through integrated correctness, daily-use acceptance, verified release
and maintenance. Its [delivery plan](product/agent-delivery-plan.md) contains
16 bounded work packages with role ownership, dependencies, write boundaries
and exit evidence. Start with **W01 integration reconciliation**, retaining
Feature 024's completed local implementation and verification below.

Read the [dated baseline](product/planning-baseline.md) for all 26 feature
directories, [product acceptance](product/acceptance.md) for six outcomes and
eight reference journeys, and the [agent playbook](operations/agent-playbook.md)
for assignment and handoff. The architecture documents now include provider,
clipboard, persistence and session-identity boundaries. Rejected credential-
store/libpq work is not in the active backlog. No application behaviour or
feature checkbox was changed by this planning work.

GitHub readback on 2026-09-04 confirmed private `tkomane/ignatius`, remote main
`52872acd7c0e0e38c6b810723ccda5eee99a28bf` and unprotected main. Its
[CI run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318)
passed the executed OS, PostgreSQL 14-18 plain/TLS, PostgreSQL 18 socket,
MSRV, workflow, security and generated-output gates. The scheduled fixture-drift
job skipped. This source-bound hosted evidence does not cover local HEAD
`3168e7c` or later uncommitted features. Planning verification is separate from
the application runs below.
The private-vulnerability-reporting API returned HTTP 404, so that route's
availability or access remains unverified in W13; it is not labelled disabled.

**Planning verification:** all 15 existing
`cargo test --locked --test documentation_matches_the_build` contracts passed.
The roadmap's named Feature 024 entry was restored after that contract caught
its omission. A concurrent terminal-key documentation update was allowed to
settle and the final contract run passed against the updated files. Local link,
feature-inventory, package/outcome/journey coverage, prose and whitespace checks
passed across 25 documents and 87 local links. The 16-package dependency graph
is acyclic; all 26 feature directories, six outcomes and eight journeys are
covered. All five Mermaid blocks were reviewed as source; graphical rendering
was not performed. No fresh application full-suite, database, cloud-provider
or hand-terminal result is claimed by this documentation-only planning task.
The existing application results below retain their original scope.

| Planning deliverable | Verified repository evidence |
| --- | --- |
| Analyse product objectives and users | `product/planning-baseline.md` and six measurable outcomes in `product/acceptance.md`, grounded in the vision, personas and experience brief |
| Architect subsequent agent work | Updated context, component and runtime documents, with trust/data flows, identity contracts and extension rules |
| Build a thorough execution roadmap | Six milestones, all 26 feature directories, 16 scoped packages, dependencies, role ownership, risks, decisions, release and maintenance gates |
| Make agent assignments repeatable | `operations/agent-playbook.md`, synchronized AGENTS/CLAUDE entry points and an assignment/return template |
| Preserve evidence and work ownership | Existing feature implementation retained, current hosted baseline re-read, rejected work excluded, no feature checkbox or application source changed by the planning task |

The planning handoff is complete. Product implementation acceptance and release
readiness remain the work described in W01-W16. No commit, push, tag, signing,
release or publication operation was performed by this planning task.

## Where the work is

**Current continuation**: Feature 024 explicit retained-result refresh and the
first W02 identity-transition hardening slice are integrated into `main` at
merge commit `a8a2b19664699b46a2edc3fff9478eb38f60ae40`, pushed to
`origin/main` on 2026-09-04. The integrated source is now the resumption point;
the older local feature branch was deleted after the merge. With Results
focused, portable `F6`, the
command palette and the contextual footer request one deliberate refresh from
the retained source; terminals with enhanced modifier reporting also accept
`Ctrl+Shift+R`. Only one read-classified statement is eligible; the editor stays
unchanged, named parameters are prompted again, outcomes are visible, and
focus, filtering, sorting, reconnect or failure never replays the query.
Focused, live and locked-verifier evidence is recorded below; manual terminal
and screen-reader checks and the semantic Unix-socket gate remain separate
evidence boundaries. Feature 023 reviewable cell-to-UPDATE and Feature 022
prompted named parameters are integrated and have their evidence below.
Feature 020 connection picking and Feature 019 formatting are integrated as
well. Feature 012 schema-aware completion is in ancestor commit `3168e7c`;
Feature 017
connection trust remains implemented and was previously pushed to `main` as
`9f80bef3040ba80d017dae3d5d0ae120a4cfc264` on 2026-09-03. No tag, signing,
release or publication operation was performed for this integration. Feature 008 release evidence
remains the separate release-readiness boundary. Its documentation was first pushed at
`b1e032f0344b88c0002c8fb32c08be13d72d73d9`; the hosted candidate
evidence is bound to source revision
`259ac76de6477719b8a3515777f3623b24237dd6`. That revision hardens the hosted
workflow's deterministic archive builds (`/Brepro` for Windows) and corrects
the exact runtime bundle layout. The pre-push `cargo --locked xtask verify` gate
passed formatting, lints, 515 unit/layout tests, 39 CLI contract tests and 38
PostgreSQL integration tests against disposable PostgreSQL 18.4 services, with
no skips.

Owner-authorized non-publishing workflow run `33689674455` passed all target
archive, repeat-build comparison, runtime smoke and aggregate jobs. The
macOS, Windows and Linux repeat archives were byte-identical. Each runtime job
passed identity, diagnostics, configuration initialisation and validation,
non-interactive refusal, side-by-side candidate validation, controlled invalid
configuration exit 3 and known-good rollback validation. The retained
aggregate contains exactly nine ordinary files: three archives, one aggregate
record, one combined manifest, `SHA256SUMS` and three runtime sidecars. The
downloaded allowlist, record, combined manifest and archive checksums passed
independent local verification. Artifacts are retained through 2026-09-09.

The candidate is still correctly blocked. The readiness check names eight
issues: the canonical evidence path is not materialised, the source is detached
and untagged, the record is incomplete and non-publishable, and verified
signature and provenance evidence are absent. The runtime sidecars are
automated smoke evidence only and do not exercise a database query or the
direct `IGNATIUS_LOG=debug` privacy audit. The historical hosted candidate's
privacy failure and the local remediation remain documented in
`docs/operations/verification.md`.

T034 is complete. T033 remains open for Windows/Linux hand-terminal checks,
direct runtime privacy review of the corrected binaries, canonical evidence
materialisation and the remaining supply-chain evidence. T035 remains open
because the accepted release criteria are not all evidenced; the roadmap stays
in planning. No tag, signing, release or publication operation was performed.

The merged CI-hardening work preserves that hosted aggregate as historical
evidence while adding exhaustive local verification, Rust 1.90 and PostgreSQL
14-18 coverage, a real Linux Unix-socket job, immutable action and
workflow-security contracts, bounded jobs, semantic workflow linting, and
independent dependency and secret controls. Its release preflight freezes the
source and runs the same security and verifier gates before retaining target
evidence; the aggregate job rechecks transport, scope, target, product, source
and build identity before retaining its six-file package.

Those changes first passed locally in an isolated integration worktree, including
Actionlint and YAML parsing, 80 release contracts, 7 workflow contracts,
Cargo-deny 0.20.2, digest-pinned Gitleaks history scanning and Rust 1.90. The
full verifier passed PostgreSQL 18.4 plain and TLS; the Unix-socket check was
explicitly skipped because its URI was not configured. The later main CI
readback above now proves PostgreSQL 15/17, the socket job, Rust 1.90 and
workflow linting at that source. It does not prove the separate hardened
release preflight or a new release candidate's runtime evidence.
An explicit macOS Docker socket attempt is not counted as a pass: the bind
mount rejected PostgreSQL's socket-permission change before startup. The
socket job deliberately targets Ubuntu, where the runner filesystem supports
the required Unix-domain socket boundary.

**Earlier implemented product feature**: cloud identity authentication and its
read-only connection trust surface, implemented and pushed in
`9f80bef3040ba80d017dae3d5d0ae120a4cfc264`. The
owner asked for Entra ID first and for a first-class experience on other clouds,
and checking the other two turned that instruction from a generalisation into the
obvious shape: Azure, AWS and Google Cloud authenticate a PostgreSQL connection
identically. A command-line tool returns a short-lived bearer token, the token
is presented as the password, and the transport must be encrypted. Only the
command differs, so the command is data: three built-ins ship and
`[auth.providers]` defines a fourth without waiting for a release.

The interactive client now makes that route inspectable from the command
palette with `Connection and auth details`. The surface names the target,
environment, server posture, observed TLS state, provider command summary,
credential lifetime caveat, and provider remedy. It is display-only: opening it
does not execute a provider, refresh a token, or perform network I/O; arguments
and token values are never rendered. A password-authenticated session says that
no cloud token was requested instead of guessing which password route won.

Two refusals are the security of it. Nothing is fetched for a target whose
`sslmode` would permit an unencrypted connection - the check runs before the
program does, so the token was never asked for rather than asked for and
discarded. And nothing falls back to the password prompt: a token was accepted
or refused on its own terms, so asking a person would describe the wrong problem
and take a real password on the way. `postgres::connect` refuses outright if a
target names a provider and carries no credential, which turns a forgotten call
site into a loud defect rather than a quiet prompt.

Proven end to end against the TLS container with no cloud account: a provider
whose program prints the container's synthetic password, through the subprocess,
the extraction, the `SecretString` and a real authenticated session. What is
**not** proven is that `az`, `aws` and `gcloud` behave as documented - that needs
an account, and `docs/support/compatibility.md` carries a separate column per
provider rather than letting one word cover both.

Also: `query --format json` no longer writes `[]` for a run that failed, which
to a stdout-only reader was indistinguishable from a query that matched nothing.

**Feature 012 implementation**: the TUI and plain mode now consume one shared,
lexer-backed completion engine and one PostgreSQL catalogue snapshot with bounded
visible candidate lists.
The editor offers keywords, schema objects, relations, functions, columns,
aliases and CTEs at the cursor; acceptance quotes identifiers and is one undo
step, while dismissal leaves the buffer and cursor unchanged. Loading, stale,
unavailable, unreadable and bounded states are written in the UI, automatic
popups can be disabled without removing explicit `Ctrl+Space`, and plain mode
uses `\complete` plus `\use` without executing a partial buffer. The focused
completion run, locked verifier, and live catalogue test pass against disposable
PostgreSQL 18.4. The Unix-socket gate is explicitly skipped because its URI is
not configured; cross-platform terminal evidence remains pending.

**Feature 013 implementation**: query failures now retain the server's
statement ordinal and structured schema, table, column, and constraint fields.
For a valid position in the unchanged buffer, the TUI moves the caret to the
server's answer, marks the line with `!`, emphasizes the smallest known token,
and keeps the error explanation visible. A changed buffer, missing position, or
unmappable position is labelled instead of guessed. Plain mode renders the same
statement, line, column, source excerpt, and caret without terminal control
sequences; failed JSON queries keep stdout empty and place the structured
diagnostic on stderr. The live PostgreSQL suite covers syntax, unique,
foreign-key, not-null, and check failures. Focused and full evidence is recorded
below; cross-platform terminal, screen-reader, and Unix-socket evidence remain
separate gaps.

**Feature 014 implementation**: the interactive grid is now a local, reversible
view over retained result rows. Sort cycles through ascending, descending, and
original server order without an execution effect and preserves the selected
source row. A searchable `Ctrl+K g` palette controls source-column visibility,
bounded widths, optional server type labels, and a frozen first visible column;
duplicate labels remain distinct by source position, the last visible column is
protected, and reset or a new execution clears result-specific shape. The
existing filter, inspector, expanded row, editor, history, plain, JSON, and
streaming-export contracts remain separate.

**Feature 015 implementation**: guided discovery is now derived from the current
model and the active configured keymap. The first frame names the connection
state and a valid next action; empty and blocked editor, result and object states
explain their boundary; and the footer presents no more than five contextual
action/key pairs. The general command palette identifies itself, searches by
intent, groups useful actions, and keeps prerequisites beside unavailable chord
entries. Opening, searching, rendering and dismissing these surfaces are
presentation-only and do not execute SQL, reload metadata, write files, record
history, transfer clipboard data, or persist onboarding state.

**Feature 018 implementation**: copying a retained text cell is now an explicit,
opt-in terminal action. `Ctrl+K c`, the inspector and the command palette stage
only the result job, source row, source column and UTF-8 counts; confirmation
revalidates that identity before creating the redacted payload. The interactive
runtime writes one base64-framed OSC 52 sequence and flushes it. Disabled,
NULL, empty, oversized, stale, unsupported and failed paths remain value-free,
and success reports bytes and characters while leaving terminal acceptance
unconfirmed. The terminal, SSH path, or multiplexer may observe or retain the
value; the client never reads or clears the destination. Plain, JSON, NDJSON,
history, export and SQL editing remain outside the transport boundary.

**Feature 019 implementation**: the editor and plain mode now share one bounded,
deterministic SQL formatter. `Ctrl+Shift+F`, `Ctrl+K q`, the command palette,
and plain `\format` format the complete local buffer without contacting
PostgreSQL or creating any effect. Major clauses, boolean predicates and
eligible lists receive readable layout; strings, quoted identifiers,
dollar-quoted bodies and comments remain byte-for-byte exact. A changed buffer
is one undoable edit with logical cursor recovery, while empty, comment-only,
malformed and over-limit input remains untouched with value-free next-action
wording. Plain mode keeps the formatted statement pending, writes its sanitized
preview to stderr, and leaves result stdout unchanged.

**Previous**: the four open decisions are closed, and the product has a
direction rather than a finished roadmap. The owner named the real requirement
on 2026-08-16 - Microsoft Entra ID against Azure Database for PostgreSQL, for a
production application - and it turns out not to need libpq at all: Entra
authenticates with a token presented as the password over TLS, which this
adapter already does. ADR-0012 records that and supersedes ADR-0009. The
credential store is rejected (ADR-0011), copy goes through OSC 52 opt-in
(ADR-0013), and `docs/product/experience-roadmap.md` sets out what "the best
experience anyone has used" means in concrete features, first of which is
completion that knows the schema. Specifications were written for cloud identity
(since renamed to `specs/011-cloud-identity/`) and completion
(`specs/012-schema-completion/`); neither was implemented at that point.

Also fixed: the two Feature 005 editor defects the audit of `555203f` found.
Enter now reaches `Editor::insert_newline()` so indentation survives a line
break through the real key path, and every cursor movement ends the undo
coalescing run so typing either side of a move is two undo steps. Both are
proven by tests that were watched failing with the fix removed - and the first
version of the movement test passed without the fix, so it was rewritten until
it could fail.

That handoff also unblocked `tests/editor_contract.rs`, the acceptance contract
for the same feature, which had been held behind `#[ignore]` waiting
for it. Two steps in it needed correcting and both are the same mistake in
different clothes: it removed inherited indentation with forward-delete, which
was a no-op that happened to leave the right text behind while Enter still
inserted a bare newline. Backspace is what a person would press and what the
implemented behaviour requires. The pty half also asserted only that
`FROM orders AS o` appeared somewhere, which a line still carrying four
inherited spaces satisfies; it now says the clause is top level. The three
contracts are active and pass in the normal focused test run.

**Previous**: consolidation, and the bug it found. Five chords - the
definition panel, dependencies, save, open and write - were documented, palette-
listed and bound to nothing: the features worked and no key reached them. A new
suite compares the keymap document, the bindable action names, the profile
fields and the exit codes against the build in both directions, and it fails when
either side drifts. That is checked by making each side drift on purpose.

**Previous**: consolidation. The password prompt's happy path is now
proven under a real pseudo-terminal against the container - typed, connected,
and the password absent from the transcript afterwards - which was the largest
gap in recent work. Four features had been built without specifications, against
the constitution's own rule; `specs/010-asking-and-keeping/spec.md` records them
and says plainly that it was written after the fact.

**Previous**: writing the rows on screen to a file, and saved queries.
The export from the client says how many rows it will write and what it will not
contain before it writes anything, because what is on screen is not what the
query returned whenever the result was truncated.

**Previous**: saved queries. `Ctrl+K w` and `Ctrl+K o` write and open
ordinary `.sql` files in the directory `config paths` has advertised since the
first release and nothing had ever used. A name is checked before it reaches the
filesystem, and that check is tested as the security boundary it is.

**Previous**: `config init`, a password prompt in plain mode, and named
connections. ADR-0011 proposes where a stored credential would live and is
waiting on the owner: `.pgpass` already exists and is shared with `psql`, so
whether to carry a keyring dependency at all is a decision, not a default.
Plain mode now asks without echoing when the server demands a password, on the
same rule the client follows: a terminal at both ends or no question at all.

**Previous**: named connections. A `[profiles]` table says where a
database is and how it is classified, reached as `@name` or `--profile name`. The
classification is the reason it exists: written down once, the production write
guard applies every time. A profile that tries to hold a password is refused by
name. Specified in `specs/009-connection-profiles/spec.md`.

**Previous**: a password prompt. When the server refuses for want of a
password, the client asks for one and retries; the characters are masked, held
only until the attempt is made, and printed by nothing. That closes half of the
"no password prompting" gap - the full-screen client. Plain mode still relies on
the other password routes.

**Previous**: configurable key bindings. A `[keys]` table in
`config.toml` replaces the built-in binding for an action; an unknown action
name, an unreadable key and two actions on one key are all errors reported before
the terminal is taken. That completes roadmap Feature 007 apart from
hand-verification, which is evidence rather than code.

**Previous**: dependency navigation. `Ctrl+K y` lists what an object is
used by and what it depends on, with the reason for each edge, following the two
edges PostgreSQL records - view rewrite rules and foreign keys - and saying that
a function body's reads are not among them. That completes roadmap Feature 005.

**Previous**: the object tree has a connection of its own, so a long
query cannot delay it. Same resolved target, moved rather than re-derived, with
two deliberate differences the server can see: an `application_name` saying it is
the tree, and a read-only session. If it cannot be opened the tree shares the
session's connection and the header says so.

**Previous**: indexes and extensions in the object tree, and object
definitions. `Ctrl+K d` on an object in the tree
shows what it is, coloured by the same lexer that colours the editor. Views,
indexes and functions come from PostgreSQL's own renderers; a table is assembled
from its columns, constraints and indexes and is labelled as a description rather
than a script. Specified in `specs/007-object-definitions/spec.md`.

**Previous**: a filter over the result rows, on the same key that filters
the object tree. It searches retained rows, which is all there is to search, and
the count line says so: `matching 3 of 10000 retained rows, of 200000 returned`.
Row numbers stay the rows' own, and the inspector and expanded view follow the
selection to the real row. That leaves copying a value out as the only unbuilt
part of Feature 004, and it is waiting on the clipboard decision.

**Previous**: the statement history. Interactive sessions record what ran,
`Ctrl+K s` searches it and puts a statement back in the editor, and every control
over it is explicit: a credential-bearing statement is never written, recording
pauses for a run (`--no-history`) or a session (`Ctrl+K v`), configuration can
switch it off entirely, and `ignatius history list` and `clear` show and remove
what is kept. Scripted `query` runs are not recorded. Specified in
`specs/006-statement-history/spec.md`.

**Previous**: syntax colouring, and a real editor for the SQL buffer.
Keywords, literals, numbers, comments, quoted identifiers and placeholders are
coloured from the statement lexer's own rules rather than a second set, and a bar
in the gutter marks the statement `Ctrl+T` would run. Vertical movement with a
remembered column, line and buffer ends, word movement and deletion, movement by
a screenful, and undo and redo a word at a time including over loaded text. The
window follows the cursor rather than being stored, so there is one source of
truth for where the cursor is. Specified in `specs/005-sql-editing/spec.md`.
Syntax highlighting and query history are the rest of roadmap Feature 003.

**Previous**: reading a result. `Ctrl+K x` lays one row down the screen,
one column per line; Enter on a cell opens it in full, wrapped, scrollable, with
what the value is stated in words. Specified in
`specs/004-result-inspection/spec.md`. That is two of the four items Feature 004
had left. Result filtering and copying a value out remain; copying waits on a
decision about the clipboard route, not on time, because it moves data out of
this process.

**Previous**: a plain, line-oriented client behind `--plain`. No
alternate screen, no raw mode, no cursor addressing: it works in `TERM=dumb`,
stays in the scrollback, and can be driven by a pipe. It is the accessibility
item that had been unchecked in the Feature 001 quality checklist since it was
written.

**Previous**: transaction state, read from the server rather than inferred.

**Previous**: 006, production-aware safety. A production-classified
connection holds back writes until they are confirmed, and `--read-only` asks
the server to enforce it.

**Previous**: transport hardening. `verify-ca` and client certificates
are implemented natively and tested against a TLS server, which closes the
largest honesty gap the project had: the headline differentiator had never been
exercised against a server that speaks TLS.

**Previous**: 004, result export. Streaming export is implemented and
verified with a 200,000-row result in 13 MB of resident memory.

**Previous**: 003, credential routes. Password files and service files
are implemented and verified against a real server.

**Previous**: 002, object navigation. Implemented and verified on macOS
and in CI. Feature 001 is complete apart from the hand-verification gaps below.

The owner asked for navigation ahead of the libpq migration in ADR-0009. The
reorder is safe because this feature touches no credential route: it reads the
catalogue over an existing session.

**Product name**: Ignatius, confirmed by the owner on 2026-08-15. The
availability check is recorded in `docs/product/landscape.md`. A trademark search
and a domain remain outstanding before publishing; neither blocks development.

## Feature 024 verification - 2026-09-04

Feature 024 is integrated into `main` and remains separate from release
readiness. The interactive client exposes an explicit
portable `F6` action, an enhanced-terminal `Ctrl+Shift+R` alias, a command-
palette entry and a contextual Results hint. Refresh reads only the retained
`last_sql` source, requires exactly one
read-classified statement and a usable non-failed session, leaves the editor
unchanged, and refuses multi-statement, write, plan, missing-result and
unavailable states without an execution effect. Named parameters are prompted
again without retaining entered values. The old snapshot stays visible while
the refresh runs, and success, cancellation, failure and unknown connection
outcomes are distinct. Passive result actions never trigger a refresh.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked --lib refresh` | 14 passed |
| `cargo test --locked --lib app::discovery::tests::retained_result_refresh` | 1 passed |
| `cargo test --locked --lib a_retained_result_refresh` | 1 passed |
| `cargo test --locked --lib a_refused_retained_result_refresh` | 1 passed |
| focused keymap reachability, configured-key and printable-input tests | 3 passed |
| `cargo test --locked --test cli_contract retained_result_refresh_stays_out_of_noninteractive_machine_output` | 1 passed |
| `cargo test --locked --test documentation_matches_the_build` | 15 passed |
| `git diff --check` | Pass |

The focused evidence proves retained-source execution rather than editor-buffer
execution, editor text and revision preservation, named-parameter re-prompting
including empty input, source classification and multi-statement refusal,
zero-row eligibility, failed-transaction and connection refusal, visible
in-flight and outcome wording, cancellation without retry, connection loss with
the old snapshot preserved, ordinary history behaviour, no passive replay,
configured-key discoverability, printable `r` preservation, ASCII output and a
narrow refusal layout. The non-interactive contract confirms that refresh state
and a synthetic parameter value remain outside machine output.

The final authoritative `cargo --locked xtask verify` exited successfully in a
host context that could inspect the disposable services. Formatting, lints, API
documentation, workspace tests and documentation tests all passed. The
workspace test gate ran 990 cases: 753 library tests, 3 archive-helper tests,
51 CLI-contract tests, 4 cloud-identity tests, 15 documentation matches, 3
editor-contract tests, 2 keymap-contract tests, 1 native-boundary test, 3
password-prompt tests, 49 PostgreSQL integration tests, 1 Unix-socket harness
test, 80 release-contract tests, 7 release-note tests, 2 release-runtime tests,
1 release-schema test, 3 terminal-restoration tests, 7 workflow-contract tests
and 5 xtask tests. Documentation tests ran 0 tests and passed. Plain and TLS
database-backed gates passed against disposable PostgreSQL 18.4 using
synthetic credentials from `docker/dev.env`.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` was not set; the harness returning without that
URI is not live socket evidence. Manual confirmation in Warp, Windows Terminal,
Linux terminals, SSH and multiplexers, screen-reader use, reduced-motion and
live-resize checks remain unverified. The run used Homebrew Rust/Cargo 1.98.0
and PostgreSQL 18.4; broader PostgreSQL matrix, hosted toolchain and
cross-platform evidence remain separate. Release readiness, signing,
provenance, publication and owner authorization remain separate and are not
implied by this green local verifier. The disposable services were torn down
with `cargo xtask db down`, and `cargo xtask db status` reported `Not running.`
No tag, signing, release, push or publication operation was performed.

## W02 identity-transition hardening slice - 2026-09-04

The reducer now treats a connection-loss message as the boundary for every
server-bound prompt and running identity. It discards production confirmations,
analyzed-plan confirmations, named-parameter prompts, update metadata/value/review
state and late execution markers before the runtime's stale completion arrives.
It sets transaction state to `Unknown`, preserves the prior retained result, and
keeps a pending copy available because copying that already-retained snapshot is
local and does not contact the server. Refresh remains bound to its retained
source when the editor changes during the request.

The focused W02 checks passed: refresh-source/editor interleaving, connection
loss across parameter, refresh-parameter, production, plan and cell-update
prompts, local copy after connection loss, and late execution identity cleanup.
The refreshed `cargo --locked xtask verify` exited successfully after this
reducer change: 753 library tests and 990 workspace tests passed, along with
formatting, lints, API documentation, documentation tests and the plain/TLS
PostgreSQL gates. The Unix-socket semantic gate remained skipped because
`IGNATIUS_TEST_PG_SOCKET_URI` was not set. Runtime connection-generation
interleavings beyond this reducer slice, the broader PostgreSQL matrix, native
terminal evidence and hand acceptance remain open under W02 and later
packages.

## Feature 023 verification - 2026-09-04

Feature 023 is integrated into `main` and remains separate from release
readiness. The interactive client now turns one selected
cell from a retained direct single-table `SELECT` into a conservative,
reviewable `UPDATE`. It resolves bare relations through the active session
search path, reads live relation kind, read and update privileges and ordered
primary-key metadata, then keeps the replacement and key values in secret
parameter bindings. Production and server read-only sessions refuse before
replacement input. The exact bound statement is shown in a separate review;
only its Enter emits one parameterized execution, and the prior result remains
a snapshot rather than being rerun.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --locked --lib query::update` | 9 passed |
| `cargo test --locked --lib app::update::tests::cell_update` | 3 passed |
| stale metadata identity reducer test | 1 passed |
| generated-update snapshot and no-rerun reducer test | 1 passed |
| `cargo test --locked --lib app::discovery::tests::cell_update_is_discoverable_only_for_a_retained_read_write_result_source` | 1 passed |
| `cargo test --locked --lib ui::layout::tests::cell_update` | 2 passed |
| `cargo test --locked --test documentation_matches_the_build` | 14 passed |
| focused live table-update PostgreSQL check | 1 passed against disposable PostgreSQL 18.4 |
| focused live view-kind refusal PostgreSQL check | 1 passed against disposable PostgreSQL 18.4 |
| `git diff --check` | Pass |

The focused evidence proves direct projection and alias mapping, trailing
multiple-statement refusal, joins, expressions, CTEs, set operations,
duplicate output, missing and NULL keys, primary-key target refusal, quoted
identifiers, literal escaping, empty text, NUL and 16 KiB bounds, redacted
debug state, stale result identity, production and read-only refusal, live
search-path resolution, primary-key discovery, update privilege and view-kind
metadata. The renderer checks the value prompt, literal semantics, exact bound
review statement, no-send boundary, narrow output and ASCII output. The live
table check proves no row changes before the execution effect, one intended row
changes after confirmation, and a deliberate read sees the replacement. The
live view check proves that a non-table relation is reported before the
interactive refusal.

The authoritative `cargo --locked xtask verify` exited successfully with the
disposable `postgres:18.4-alpine` plain and TLS fixtures running and synthetic
credentials from `docker/dev.env`. It passed formatting, lints, API
documentation, 737 unit and layout tests, 50 CLI-contract tests, 3
archive-helper tests, 4 cloud-identity tests, 14 documentation matches, 3
editor-contract tests, 2 keymap-contract tests, 1 native-boundary test, 3
password-prompt tests, 49 PostgreSQL integration tests, 80 release-contract
tests, 7 release-note tests, 2 release-runtime tests, 1 release-schema test, 3
terminal-restoration tests, 7 workflow-contract tests, 5 xtask tests, and the
documentation tests. Plain and TLS PostgreSQL database gates passed.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The socket harness returned without a
configured URI; that is not live socket evidence. Manual confirmation in Warp,
Windows Terminal, Linux terminals, SSH and multiplexers, screen-reader use,
reduced-motion and live-resize checks remain unverified. The available local run
used Homebrew Rust/Cargo 1.98.0 and PostgreSQL 18.4; broader PostgreSQL matrix,
hosted toolchain and cross-platform evidence remain separate. Release
readiness, signing, provenance, publication and owner authorization remain
separate and are not implied by this green local verifier. The disposable
services were explicitly torn down with `cargo xtask db down`, and `cargo xtask
db status` reported `Not running.` No tag, signing, release, push or publication
operation was performed.

## Feature 022 verification - 2026-09-04

Feature 022 is integrated into `main` and remains separate from release
readiness. The full-screen and plain interactive clients
recognize bounded `:name` placeholders only in executable SQL, prompt once per
distinct name in first-use order, accept empty text, and keep the original
template in the editor and history. Values are held in `SecretString`, shown
only as a mask, and bound as escaped PostgreSQL text literals at the existing
simple-query boundary. Non-interactive `query` accepts only complete
`--param-env NAME=VARIABLE` mappings and validates them before target work.
NUL is refused, and server positions after expansion are reported without a
guessed template caret.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo test --locked --lib parameter` | 25 passed |
| `cargo test --locked --test cli_contract named_parameter` | 1 passed |
| `cargo test --locked --test documentation_matches_the_build prompted_parameters` | 1 passed |
| `cargo test --locked --test postgres_integration parameterized` | 3 passed against disposable PostgreSQL 18.4 |
| `cargo test --locked --test password_prompt plain_parameter` | 1 passed through a real macOS pseudo-terminal against disposable PostgreSQL 18.4 |
| CLI `--param-env` probe | Pass; stdout contained only the requested CSV data |
| Missing mapping against an unreachable target | Pass; usage error returned before connection work |
| `git diff --check` | Pass |

The focused evidence proves protected-region scanning, cast exclusion, first-use
ordering, repeated-name reuse, adversarial literal escaping, empty values, NUL
refusal, the 64-name limit, masked prompt rendering, cancellation without an
effect, template-preserving history and diagnostics, redacted effect and log
descriptors, exact environment mapping validation, data-only stdout, and honest
post-expansion error locations. The live PostgreSQL checks also prove execute,
stream, escaped and repeated values, empty text, NUL refusal before send, and
server-confirmed cancellation. The pseudo-terminal check proves that plain
mode accepts a hidden answer and that the returned value appears once as data,
not as prompt echo.

The final authoritative `cargo --locked xtask verify` exited successfully with
the disposable `postgres:18.4-alpine` plain and TLS fixtures running and
synthetic credentials from `docker/dev.env`. It passed formatting, lints, API
documentation, the complete 720-test library suite, 50 CLI-contract tests, 3
archive-helper tests, 4 cloud-identity tests, 13 documentation matches, 3
editor-contract tests, 2 keymap-contract tests, 1 native-boundary test, 3
password-prompt tests, 47 PostgreSQL integration tests, 80 release-contract
tests, 7 release-note tests, 2 release-runtime tests, 1 release-schema test, 3
terminal-restoration tests, 7 workflow-contract tests, 5 xtask tests, and the
documentation tests. Plain and TLS PostgreSQL database gates passed.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The socket harness returned without a
configured URI; that is not live socket evidence. Manual confirmation in Warp,
Windows Terminal, Linux terminals, SSH and multiplexers, screen-reader use,
reduced-motion and live-resize checks, and live cloud-provider account routes
remain unverified. The available local run used Homebrew Rust/Cargo 1.98.0 and
PostgreSQL 18.4; the broader PostgreSQL 14-18 matrix and hosted toolchain
evidence remain separate. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this green local
verifier. The disposable services were explicitly torn down with `cargo xtask
db down`, and `cargo xtask db status` reported `Not running.` No tag, signing,
release, push, or publication operation was performed.

## Feature 021 verification - 2026-09-04

Feature 021 is integrated into `main` and remains separate from release
readiness. Interactive export now chooses CSV, TSV, JSON,
NDJSON or Markdown in a searchable palette before the destination path. The
choice is carried through the existing retained-row, filter, no-overwrite and
atomic export boundary; the filename extension is never consulted. CLI INSERT
output requires an explicit destination table, quotes table and column
identifiers, preserves NULL, emits escaped PostgreSQL text literals without type
inference, and refuses duplicate labels, NUL text and header suppression.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo test --locked cli::output --lib` | 25 passed |
| `cargo test --locked app::update --lib` | 126 passed |
| `cargo test --locked ui::layout --lib` | 82 passed |
| `cargo test --locked cli::interactive --lib` | 14 passed |
| `cargo test --locked --test documentation_matches_the_build` | 12 passed |
| `cargo test --locked --test cli_contract` without database environment | 48 passed; database cases skipped |
| `git diff --check` | Pass |

The focused evidence proves deterministic format ordering, searchable choice,
no effect before path acceptance, retained and filtered row scope, compact and
ASCII wording, output safety for hostile identifiers and literals, NULL and
empty-text distinction, NUL and duplicate-label refusal, header validation,
buffered/streamed byte equivalence, existing-format stability, and no
decoration on machine output. The interactive JSON route also preserves the
server's original row count and truncation fact while writing only retained
filtered rows. The CLI contract covers validation before connection work,
streamed file output, and the explicit application of generated INSERT data to
a disposable probe table.

The authoritative `cargo --locked xtask verify` exited successfully with the
following environment: `rustc 1.98.0` and `cargo 1.98.0` from Homebrew, against
the local `postgres:18.4-alpine` image
`sha256:db676a0ed906c00f55020fb8999e4fb30c598bf5c3b5c188630aef2812d3f11d`.
It passed formatting, lints, API documentation, the complete 703-test library
suite, 49 CLI-contract tests, 3 archive-helper tests, 4 cloud-identity tests,
12 documentation matches, 3 editor-contract tests, 2 keymap-contract tests, 1
native-boundary test, 2 password-prompt tests, 44 PostgreSQL integration tests,
80 release-contract tests, 7 release-note tests, 2 release-runtime tests, 1
release-schema test, 3 terminal-restoration tests, 7 workflow-contract tests,
5 xtask tests, and the documentation tests. Plain and TLS PostgreSQL database
gates passed. The live INSERT contract generated, streamed, and explicitly
applied the statements to the disposable probe table, confirming three rows.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The workspace socket harness safely
returned without a configured URI; that is not live socket evidence. Manual
confirmation in Warp, Windows Terminal, Linux terminals, SSH and multiplexers,
screen-reader use, reduced-motion and live-resize checks, and live
cloud-provider account routes remain unverified. The pinned rustup 1.97.1
toolchain and the broader PostgreSQL 14-18 matrix remain separate hosted or
environment evidence gates; this run used the available Homebrew 1.98.0 and
PostgreSQL 18.4 fixtures. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this green local
verifier. The disposable services were explicitly torn down with
`cargo xtask db down`, and `cargo xtask db status` reported `Not running.` No
tag, signing, release, push, or publication operation was performed.

## Feature 020 verification - 2026-09-04

Feature 020 is integrated into `main` and remains separate from release
readiness. Named profiles now produce a searchable
connection picker before implicit startup resolution, with an explicit default
route and safe, bounded profile summaries. `Ctrl+K n` reaches the same picker
after a quiet session. Selection carries only an optional profile name through
the reducer, keeps resolved targets and credentials in runtime-only slots,
retains the editor and local result preferences, clears server-derived state,
and rejects late work from the previous connection generation.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked app::connection_picker --lib` | 5 passed |
| `cargo test --locked app::palette --lib` | 14 passed |
| `cargo test --locked app::update --lib` | 125 passed |
| `cargo test --locked app::discovery --lib` | 9 passed |
| `cargo test --locked ui::keymap --lib` | 28 passed |
| `cargo test --locked ui::layout --lib` | 81 passed |
| `cargo test --locked cli::interactive --lib` | 13 passed |
| `cargo test --locked --test native_boundary_contract` | 1 passed |
| `cargo test --locked --test documentation_matches_the_build` | 11 passed |
| `cargo test --locked --test cli_contract` | 46 passed |
| `cargo test --locked --lib` | 696 passed |
| `git diff --check` | Pass |

The focused evidence proves deterministic safe summaries, missing-field and
provider wording, bounded derived details, exclusion of password-shaped
unknown values and provider arguments, searchable picker construction, default
and profile choice transitions, quiet-session reset semantics, retained editor
and reading preferences, explicit-route detection, resolver and cloud-auth
boundaries, no-fallback refusal, active-keymap discovery, printable `n`
preservation, full and compact accessible rendering, documentation parity, and
unchanged plain and machine-output routes. The native-boundary check also
confirmed that the new pure picker layer contains no driver or FFI markers.

The authoritative `cargo --locked xtask verify` exited successfully. It passed
formatting, lints, API documentation, the complete 696-test library suite, 46
CLI-contract tests, 3 archive-helper tests, 4 cloud-identity tests, 11
documentation matches, 3 editor-contract tests, 2 keymap-contract tests, 1
native-boundary test, 2 password-prompt tests, 44 PostgreSQL integration tests,
80 release-contract tests, 7 release-note tests, 2 release-runtime tests, 1
release-schema test, 3 terminal-restoration tests, 7 workflow-contract tests,
and 5 xtask tests. The disposable PostgreSQL 18.4 plain and TLS fixtures
supplied live database evidence using synthetic credentials only.

The semantic Unix-socket integration gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. The workspace socket harness was
included and safely returned without a configured URI; that is not live socket
evidence. Manual confirmation in Warp, Windows Terminal, Linux terminals, SSH
and multiplexers, screen-reader use, reduced-motion and live-resize checks,
and live cloud-provider account routes remain unverified. Release readiness,
signing, provenance, publication, and owner authorization remain separate and
are not implied by this green local verifier. The disposable services were
torn down and `cargo xtask db status` reported `Not running.` No tag, signing,
release, push, or publication operation was performed.

## Feature 019 verification - 2026-09-04

Feature 019 is integrated into `main` and remains separate from release
readiness. The focused checks cover the bounded lexical
formatter, protected regions, cursor mapping, editor history, reducer,
discovery, keymap, renderer, plain mode, documentation, and machine-output
boundaries.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked query::format --lib` | 14 passed |
| `cargo test --locked app::editor --lib` | 23 passed |
| `cargo test --locked app::update --lib` | 121 passed |
| `cargo test --locked app::discovery --lib` | 9 passed |
| `cargo test --locked ui::keymap --lib` | 27 passed |
| `cargo test --locked ui::layout --lib` | 80 passed |
| `cargo test --locked cli::plain --lib` | 16 passed |
| `cargo test --locked --test documentation_matches_the_build` | 10 passed |
| `cargo test --locked --test cli_contract` | 45 passed |
| `cargo test --locked --lib` | 678 passed |
| `git diff --check` | Pass |

The focused evidence proves clause, predicate, list, join, nested-query,
operator, cast, array, parameter, custom-operator, numeric-exponent,
multiple-statement and idempotence behavior; exact protected strings,
identifiers, dollar bodies and nested comments; control-looking protected text;
empty and comment-only no-ops; every supported unterminated region refusal;
the inclusive 1 MiB bound; UTF-8-safe cursor mapping; one editor edit with
undo/redo and modified-state preservation; stale error-location invalidation;
active-keymap discovery; accessible outcome wording; plain pending-buffer
replacement; sanitized message-stream preview; and unchanged result stdout.

The authoritative `cargo --locked xtask verify` passed every gate that ran:
formatting, lints, API documentation, 678 library tests, 45 CLI-contract tests,
44 PostgreSQL integration tests against disposable PostgreSQL 18.4 plain and
TLS services, 3 terminal-restoration tests, 10 documentation matches, 4
cloud-identity tests, 2 keymap-contract tests, 3 editor-contract tests, 1
native-boundary test, 2 password-prompt tests, 80 release-contract tests, 7
release-note tests, 2 release-runtime tests, 1 release-schema test, 7
workflow-contract tests, and 5 xtask tests. The run exited successfully.

The semantic Unix-socket gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual confirmation in Warp, Windows
Terminal, Linux terminals, SSH and multiplexers, screen-reader use, and
reduced-motion or live-resize hand checks remain unverified. The formatter's
local transformation does not prove terminal rendering on those platforms.
Release readiness, signing, provenance, publication, and owner authorization
remain separate and are not implied by this local verifier. The disposable
services were torn down and `cargo xtask db status` reported `Not running.` No
tag, signing, release, push, or publication operation was performed.

## Feature 018 verification - 2026-09-04

Feature 018 is integrated into `main` and remains separate from release
readiness. The focused checks below cover the bounded
transport, configuration, reducer, discovery, renderer, runtime, documentation,
and non-interactive boundaries.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked clipboard --lib` | 10 passed |
| `cargo test --locked config --lib` | 36 passed |
| `cargo test --locked app::update --lib` | 118 passed |
| `cargo test --locked app::discovery --lib` | 8 passed |
| `cargo test --locked ui::keymap --lib` | 26 passed |
| `cargo test --locked ui::layout --lib` | 79 passed |
| `cargo test --locked --test documentation_matches_the_build` | 9 passed |
| `cargo test --locked --test cli_contract` | 44 passed |
| `cargo test --locked --lib` | 652 passed |
| `git diff --check` | Pass |

The focused evidence proves exact OSC 52 framing and UTF-8 bytes, control-byte
containment, the inclusive 1 MiB bound, redacted payload and retained-cell
identity, opt-in and refusal paths, confirmation and cancellation, stale
selection rejection, filtered source-row identity, inspector and palette
discovery, printable `c` preservation, safe write failures, value-free
rendering, and no transport in plain, JSON, NDJSON, history, or export routes.

The authoritative `cargo --locked xtask verify` passed every gate that ran:
formatting, lints, API documentation, workspace tests, documentation tests,
652 library tests, 44 CLI-contract tests, 44 PostgreSQL integration tests, 3
terminal-restoration tests, 9 documentation matches, 4 cloud-identity tests,
2 keymap-contract tests, 3 editor-contract tests, 1 native-boundary test, 2
password-prompt tests, 80 release-contract tests, 7 release-note tests, 2
release-runtime tests, 1 release-schema test, 7 workflow-contract tests, and 5
xtask tests. Plain and TLS disposable PostgreSQL 18.4 services supplied the
database evidence. The run exited successfully with no failures.

The semantic Unix-socket gate remains a skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual confirmation in Warp, Windows
Terminal, Linux terminals, SSH and multiplexers, screen-reader use, and
terminal clipboard acceptance remain unverified. The client deliberately cannot
infer acceptance from a successful write and flush. Release readiness, signing,
provenance, publication, and owner authorization remain separate and are not
implied by this local verifier. The disposable services were torn down and
`cargo xtask db status` reported `Not running.` No tag, signing, release, push,
or publication operation was performed.

## Feature 016 verification - 2026-09-04

Feature 016 is integrated into `main` and remains separate from release
readiness. It adds a pure bounded PostgreSQL JSON plan
model, a Results-pane tree, plain and analyzed plan actions, explicit analysis
confirmation, local selection and collapse state, stale-completion protection,
connection-loss recovery, and preserved ordinary result state.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --locked query::plan --lib` | 7 passed |
| `cargo test --locked app::plan --lib` | 4 passed |
| `cargo test --locked app::discovery --lib` | 8 passed |
| `cargo test --locked app::update --lib` | 110 passed |
| `cargo test --locked ui::keymap --lib` | 25 passed |
| `cargo test --locked ui::layout --lib` | 77 passed |
| `cargo test --locked --test documentation_matches_the_build` | 8 passed |
| `cargo test --locked --test cli_contract` | 43 passed |
| `cargo test --locked --test postgres_integration --no-run` | Pass |
| `git diff --check` | Pass |

The focused evidence proves bounded parsing, truthful missing metrics, plan
preconditions, confirmation and cancellation, stale completion rejection,
ordinary-result preservation, local navigation, compact/narrow/ASCII/no-colour
rendering, key discovery, documentation contracts, and unchanged plain/JSON/
NDJSON/history boundaries.

The authoritative `cargo --locked xtask verify` passed every gate that ran:
formatting, lints, API documentation, workspace tests, documentation tests,
632 library tests, 43 CLI-contract tests, 44 PostgreSQL integration tests, 3
terminal-restoration tests, 8 documentation matches, 4 cloud-identity tests, 2
keymap-contract tests, 3 editor-contract tests, 1 native-boundary test, 2
password-prompt tests, 80 release-contract tests, 7 release-note tests, 2
release-runtime tests, 1 release-schema test, 7 workflow-contract tests, and 5
xtask tests. The run exited successfully.

The live plan evidence ran against disposable `postgres:18.4-alpine` plain and
TLS services. Plain EXPLAIN did not mutate a guarded UPDATE, analyzed SELECT
returned actual metrics, and the TLS plan ran on the encrypted session. The
fixtures were then removed with `cargo xtask db down`; they are disposable and
recreatable with `cargo xtask db up`. A subsequent `cargo xtask db status`
reported `Not running.`

The Unix-socket integration gate remains a semantic skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual Windows/Linux terminal checks,
live resize, Warp rendering, screen-reader use, and a configured Unix-socket
server remain unverified. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this local
verifier. No tag, signing, release, push, or publication operation was
performed.

## Feature 015 verification - 2026-09-04

Feature 015 is integrated into `main` and remains separate from release
readiness. Focused checks cover the pure discovery model,
active keymap snapshot, reducer, palette, renderer, documentation, and the
machine-output boundary.

| Focused gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo check --locked` | Pass |
| `cargo test app::discovery --lib` | 7 passed |
| `cargo test app::palette --lib` | 14 passed |
| `cargo test app::update --lib` | 103 passed |
| `cargo test ui::keymap --lib` | 24 passed |
| `cargo test ui::layout --lib` | 72 passed |
| `cargo test --test documentation_matches_the_build` | 7 passed |
| Guided-discovery CLI contract test | Pass |
| `git diff --check` | Pass |

The focused evidence proves that state-specific first-frame and empty-state
guidance, the five-item contextual footer, configured-key labels, intent search,
prerequisite wording, ASCII input cursors, filtered-empty recovery, and command
palette dismissal remain presentation-only. No discovery surface executes SQL,
reloads metadata, records history, writes files, transfers clipboard data, or
persists onboarding state. Plain, JSON, NDJSON, streaming export, history, and
terminal-restoration routes remain outside the discovery path.

The authoritative `cargo --locked xtask verify` passed every gate that ran. The
auto-discovered suite recorded 607 library tests, 43 CLI-contract tests, 41
PostgreSQL integration tests, 3 terminal-restoration tests, 7 documentation
matches, 4 cloud-identity tests, 2 keymap-contract tests, 3 editor-contract
tests, 1 native-boundary test, 2 password-prompt tests, 80 release-contract
tests, 7 release-note tests, 2 release-runtime tests, 1 release-schema test, 7
workflow-contract tests, and 5 xtask tests, with no failures. Disposable
`postgres:18.4-alpine` plain and TLS services supplied the passed database
evidence and were torn down afterward; `cargo xtask db status` reported
`Not running.`

The Unix-socket integration gate remains a semantic skip because
`IGNATIUS_TEST_PG_SOCKET_URI` is not set. Manual Windows/Linux terminal checks,
live resize, Warp rendering, screen-reader use, and a configured Unix-socket
server remain unverified. Release readiness, signing, provenance, publication,
and owner authorization remain separate and are not implied by this local
verifier.

## Feature 013 verification - 2026-09-04

The integrated source on `main` passed
`cargo --locked xtask verify` with the disposable PostgreSQL 18.4 plain and TLS
services running. The verification run was non-publishing and did not create a
release or tag; the later main integration is recorded above.

| Gate | Result |
| --- | --- |
| Formatting | Pass |
| Clippy with warnings denied | Pass |
| API documentation with warnings denied | Pass |
| Workspace tests | 772 passed, 0 failed |
| Library tests | 563 passed |
| CLI contract tests | 42 passed, including failed JSON and constraint diagnostics |
| PostgreSQL integration tests | 40 passed, including syntax and four constraint cases |
| Documentation tests | Pass |
| Database-backed tests | Pass for disposable plain and TLS PostgreSQL |
| Unix-socket integration | Skip: `IGNATIUS_TEST_PG_SOCKET_URI` is not set |

Focused mapper, editor, reducer, renderer, plain-mode, object-context, and JSON
checks are included in those results. The Unix-socket, Windows and Linux hand
terminal checks, screen-reader use, and Warp renderer behavior remain
unverified.

## Feature 014 verification - 2026-09-04

Feature 014 is integrated into `main` and remains separate from release
readiness. The focused checks below exercise the new grid
state and its boundaries. The authoritative verifier and disposable database
evidence are now complete for the available host environment.

| Focused gate | Result |
| --- | --- |
| `cargo check --locked` | Pass |
| `cargo test app::grid --lib` | 14 passed |
| `cargo test query::result --lib` | 10 passed |
| `cargo test postgres::session --lib` | 3 passed |
| `cargo test app::update --lib` | 100 passed |
| `cargo test app::palette --lib` | 13 passed |
| `cargo test ui::keymap --lib` | 23 passed |
| `cargo test ui::layout --lib` | 68 passed |
| `cargo test cli::output --lib` | 20 passed |
| `cargo test --test postgres_integration --no-run` | Pass |
| `git diff --check` | Pass after the final review |

The focused evidence proves local sorting is stable and reversible, selection
keeps source-row identity, filtering remains retained-row scoped, duplicate
column labels remain source-position safe, the last visible column is protected,
widths are bounded, available and unavailable type labels are distinct, frozen
wide views keep the selected column reachable, ASCII and no-colour output retain
meaning, hostile labels and cells cannot emit terminal controls, and machine
output is unchanged by result-view metadata. No focused check emitted an
`Effect::Execute` for a grid action.

The live row-bearing type-description test in `tests/postgres_integration.rs`
passed against disposable PostgreSQL 18.4 in both plain and TLS modes. It
returned `n`, `label`, and `active` as `int4`, `text`, and `bool`, while the
retained values stayed `2`, `ok`, and `NULL`; no second execution was required.

The final `cargo --locked xtask verify` passed formatting, lints with warnings
denied, API documentation, the complete auto-discovered suite, and documentation
tests. The suite reported 591 library tests and 210 integration or contract
cases without harness failures; the Unix-socket case remains a semantic skip,
not live evidence, because `IGNATIUS_TEST_PG_SOCKET_URI` is unset. The verifier
also marked the disposable plain and TLS PostgreSQL gates as passed.

Manual screen-reader use, live resize, Warp rendering, Windows/Linux hand
terminal checks, and a configured Unix-socket server remain unverified. Release
readiness, signing, provenance, publication, and owner authorization remain
separate and are not implied by this green local verifier.

## Previous green verification

Run on 2026-08-16 at committed HEAD `20bb3c3`, macOS 26.6.1 on Apple silicon,
rustc 1.97.1 (Homebrew), against `postgres:18.4-alpine` both plain and with TLS,
with `cargo xtask db up` running. 692 tests, and **one skip**: the Unix-socket
integration check, which needs `IGNATIUS_TEST_PG_SOCKET_URI` and a server
reachable over a socket, which the container does not publish. Everything else
ran. The count of skips is stated rather than the absence of failures, because
an earlier run in this same session reported these numbers with the database
down and the PostgreSQL suites had skipped rather than passed.

| Gate | Result |
| --- | --- |
| Formatting and clippy, `-D warnings` | Pass |
| Library tests | 513 passed |
| CLI contract tests | 38 passed |
| PostgreSQL integration tests | 38 passed, plain and TLS |
| Editor acceptance, reducer and pty | 3 passed |
| Password prompt, in a pty | 2 passed |
| Terminal restoration, in a pty | 3 passed |
| Documentation against the build | 6 passed |
| Cloud identity, against the TLS container | 4 passed |
| Release archive, native, core, notes and schema contracts | 3, 1, 71, 7 and 1 passed |
| CI, all jobs | Green on macOS, Windows and Linux |

Live evidence recorded in `docs/operations/verification.md`.

Separately, and not the same class of evidence: a statically linked x86-64
Linux binary built from `4bec4e3` was run on a bare Alpine container with
nothing installed, connected to PostgreSQL 18.4, and returned rows. From that
same artefact, three refusals and one success that matter:

- it refused a plain connection under `sslmode=verify-full` rather than
  downgrading;
- it opened a TLS session with a credential supplied by a provider program;
- it refused `sslmode=disable` for that provider without asking for the
  credential at all.

Those are the claims this feature rests on, made by something distributable
rather than by a test binary. A Windows binary exists from CI run
`31962001053` at the same revision and has done nothing but start and report
its own identity. No human has used either interactively yet.

Both are in `~/Downloads/ignatius-handoff-2026-08-16/` on the development
machine with their digests and a README, for hand-verification on 2026-08-17.

## What is proven, and by what

| Claim | Evidence |
| --- | --- |
| A query runs and renders | Integration tests, plus a pty capture of the client |
| A long statement is cancelled | Server confirms SQLSTATE 57014, in under a second |
| Results stay bounded | 5000 rows capped at 10 with the true count kept |
| An export stays bounded | 200,000 rows exported in 13 MB resident memory |
| An interrupted export keeps its rows | Subprocess test: exit 9, `.partial` file, count reported |
| verify-full checks the name | Refuses a certificate that does not cover the address used |
| verify-ca does not | Accepts that same certificate, against the same server |
| A client certificate authenticates | Connects as `cert_user` with no password at all |
| TLS is never silently dropped | A server refusing TLS exits 6, not 4 |
| Hostile values cannot drive the terminal | Escapes rendered as text, in values and object names |
| Hostile object names cannot alter SQL | A table named to break interpolation is listed and queried |
| Passwords never leak | Subprocess tests over failing connections |
| A failed transaction is reported | Read from the server, with ROLLBACK named as the way out |
| Plain mode emits nothing screen-reader-hostile | Subprocess test under `TERM=dumb`: no escape sequences at all |
| Plain mode still guards production | Subprocess test: a write to a production target is confirmed in words |
| Every action has a way in | A test over the list of actions rather than the list of keys |
| The documentation matches the build | A suite comparing keys, actions, profile fields and exit codes both ways |
| A typed password opens a session and is not echoed | pty test against the container: typed, connected, absent from the transcript |
| An export from the client says what it leaves out | Reducer test over a truncated and filtered result |
| A saved query's name can never escape its directory | Unit tests over separators, `..`, drive letters and control characters |
| A pipe is never asked for a password | Subprocess test with stdin closed: it exits 5 rather than waiting |
| A profile classified as production guards it | Subprocess test: the write is refused with no flag typed |
| A profile cannot hold a password | Subprocess test: refused by name, and the value is not echoed |
| A key binding that would do nothing is refused | Subprocess test: three broken files, each exiting 3 with what to fix |
| Dependencies are read in both directions | Integration test over a view and the table it reads |
| The tree's connection is separate and read-only | Integration test: two backend pids, and a write refused with SQLSTATE 25006 |
| An object's definition is what the server renders | Integration tests over a view, a function and a table |
| A hostile object name is safe to describe | Integration test: the definition is read, `orders` still exists |
| A credential never reaches the history file | Subprocess test: the statement runs, the file does not hold it |
| A paused session records nothing and says so | Subprocess test over a real session |
| The history file is the owner's alone | Subprocess test asserting 0600 after writing and after trimming |
| Colouring never alters the buffer | Property test rebuilding the text from its tokens |
| A buffer of any length can be navigated | Layout test: the window follows the cursor and the line numbers stay right |
| A deleted word comes back | Reducer test through the real key actions |
| A value is never abbreviated without recourse | The inspector renders it whole, wrapped, with its position stated |
| NULL, empty and the text NULL are distinguishable | Layout test asserting the words for each |
| Every documented exit code | Produced by a real invocation |

## Known gaps

These are real and none of them is hidden anywhere else:

1. **Windows and Linux are CI-verified, not hand-verified.** CI builds them and
   runs the unit, layout and CLI contract tests plus a startup smoke test on
   both. Nobody has yet opened the full-screen client on either platform and
   used it. Warp's own renderer and a live terminal resize are likewise
   unverified anywhere.
2. **Hosted database integration runs on Linux.** PostgreSQL 14-18 and the
   PostgreSQL 18 socket passed at remote main `52872ac`, rechecked on
   2026-09-04. The macOS and Windows CI jobs do not connect to a server.
   Newer local features have recorded macOS PostgreSQL 18.4 evidence and
   require a fresh candidate-source hosted matrix and native platform checks.
3. Every documented exit code now has a real producer with subprocess-level
   evidence, including 9 from an interrupted export.
4. **GSSAPI, Kerberos and Windows SSPI are unsupported**, and stay that way.
   ADR-0012 closed the libpq migration: the enterprise requirement that actually
   exists is Entra ID, which is a token over TLS and needs no driver capability
   this adapter lacks. If a real Kerberos requirement appears the decision
   reopens on the same terms.
5. **No OS credential store, by decision.** ADR-0011 is rejected. The routes are
   a password file, the environment, the connection string and the prompt, and
   `.pgpass` is shared with `psql`.
5a. **Cloud identity is implemented, and no cloud has been connected to.** The
   mechanism is proven against a real PostgreSQL server; `entra`, `aws` and
   `gcp` are transcriptions of vendor documentation checked on 2026-08-16 and
   nobody has run any of them against that cloud's own database. Treat the
   built-in commands as unverified until a dated row says otherwise.
5b. **The cloud provider tests are Unix-only.** They name real programs by
   absolute path. The feature is not Unix-only; the coverage is.
5c. **`--auth entra` is suspected not to work on Windows-native**, and this is
   the first thing to check there. The Azure CLI installs as `az.cmd`;
   `CreateProcessW` does no `PATHEXT` appending and Rust's `Command` appends
   only `.exe` when searching the path, so `Command::new("az")` will probably
   report the program as missing on a machine that has it - a legible message
   that is false, which is the worst kind. `gcloud` has the same shape; `aws`
   ships a real `.exe` and should be fine. Confirmed from the Rust
   documentation on 2026-08-16, not yet confirmed against a Windows machine.

   Not fixed tonight on purpose: the fix is Windows-only, untested, and
   collides with FR-1108's "no shell is involved at any point", which is a
   security property rather than a style rule. A workaround exists today and
   needs no release, because providers are data:

   ```toml
   [auth.providers.entra]
   command = ["cmd", "/C", "az", "account", "get-access-token",
              "--resource-type", "oss-rdbms", "--output", "json"]
   json-field = "accessToken"
   ```

   If Windows confirms it, the fix is a `cfg(windows)` built-in plus an explicit
   carve-out in the spec - safe for `entra` and `gcp` in particular because
   neither command carries a substituted argument.
6. **No screen reader has been used with this.** `--plain` is built and proven
   to emit no escape sequences under `TERM=dumb`, which is the mechanical part.
   Whether it is pleasant with VoiceOver or NVDA is unknown, because neither has
   been driven by hand.
7. **Terminal restoration is proven automatically on Unix**, by tests that run
   owned real PTYs. Published `7d167a9` now has three actual Windows ConPTY control
   passes and partial Guard raw/mouse/buffer evidence. Its hidden-cursor
   capability check fails; calibration and full T055a acceptance remain open.
   Warp's own renderer and a live terminal resize have not been exercised by
   hand; the PTY runs use a forced size.
8. **The secret scan was previously scanning nothing.** It walks the commit
   range of a push, which the default shallow checkout could not resolve, so it
   reported no leaks after scanning zero bytes. Fixed on 2026-08-15 by fetching
   full history for that job. Every earlier green run of that gate should be read
   as "did not run".
9. **Main is protected by an active ruleset as of 2026-10-25.** The older
   2026-09-04 `protected: false` snapshot is superseded. Current required PRs,
   13 CI contexts and resolved threads are enforced without bypass actors;
   approvals are zero and strict up-to-date/code-owner approval are off. The
   three CodeQL contexts are absent despite the documented blocking gate.
   W13 has prepared the exact owner decision; no remote setting was changed.
10. **`rust-toolchain.toml` is inert on the development machine**, which uses a
   Homebrew rustc rather than rustup. This is an environment limitation, not a
   defect.
11. **Feature 005 editor is closed.** Enter routes to
    `Editor::insert_newline()` and every movement ends the undo coalescing run,
    both proven by tests confirmed to fail without the fix. The acceptance
    scenario in `tests/editor_contract.rs` is active: an eight-line statement is
    typed, corrected, navigated and run through the reducer, and the same
    journey runs through a real pty at 100 by 30. All three tests pass, along
    with the two focused keymap contracts.

## Decisions taken

- **Name**: Ignatius, confirmed 2026-08-15.
- **Driver**: `tokio-postgres` stays. ADR-0012 supersedes ADR-0009 on
  2026-08-16 and closes Feature 001a unimplemented: the enterprise requirement
  is Entra ID, which is a token presented as a password over TLS. The `unsafe`
  exception ADR-0009 granted is withdrawn, ADR-0010 is moot, and the binary
  stays self-contained.
- **Credential store**: rejected, ADR-0011, 2026-08-16.
- **Copying a value out**: OSC 52, off unless configuration turns it on, no new
  dependency. ADR-0013, 2026-08-16. Decided on the environment it will be used
  in: a Linux clipboard crate needs a display that WSL does not have, and
  Windows Terminal implements the write half of OSC 52 and deliberately not the
  read half.
- **Repository**: public at `tkomane/ignatius` since 2026-09-11, an owner
  decision taken to restore CI after the private Actions quota was exhausted.
  Route options remain in `docs/operations/release.md`; distribution beyond
  source is still deferred.

## Decisions still pending

1. **Trademark search and a domain**, before publishing only. Neither blocks
   development.
2. **Release trust choices in ADR-0008:** inventory/SBOM, signing identity and
   custody, provenance verification and retention. W11 prepares a concrete
   decision package before seeking the owner's choice.
3. **Public distribution/support and repository-governance actions:** W13
   prepares the routes and changes; W14 obtains authority for the exact
   candidate and external operation. Historical non-publishing authority is
   not a public release decision.

The credential-store, driver and initial clipboard decisions are closed.
Controlled provider accounts and native/assistive-technology access are
evidence dependencies, not reasons to reopen those decisions.

## Next actions, in order

1. **W07:** calibrate the Windows cursor sensor under the owned native contract,
   preserving `7d167a9`'s three control passes and failed capability. Admit the
   Guard option/path matrix only after faithful observations; preserve T055a's
   remaining gaps and T051's hand-use limits. W10-013 is accepted at `54056c2`.
2. **W10-008:** retain the separate notice and retained-data contracts; finish
   the incoming-allocation design without claiming driver/startup/RSS ceilings.
   Preserve cancellation, sticky privacy and unknown write outcomes.
3. **W05/W07:** retain exact `54056c2` pinned five-major/plain/TLS/socket
   acceptance, distinct from older frozen measurements. Repeat affected server
   paths only after Session/protocol changes; Windows ConPTY and hand use remain
   separately scoped.
4. **Feature 025/015:** continue theme/pointer/daily-experience contracts after
   resource work; mouse capture stays off until pointer behavior/parity exists.
5. **W06/W08/W09:** obtain controlled provider/native/screen-reader evidence when
   access exists. Synthetic buffers and PTYs do not replace hand use.
6. **W11/W13/W12/W14:** keep prepared trust/governance/naming decisions open and
   today's owner-skipped external review explicit. Branch push, draft PR and CI
   are authorised; merge, signing and release remain separate owner decisions.
