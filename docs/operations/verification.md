# Verification

## ConPTY integration boundary - 26 October 2026

The diagnostic now has a separate explicit-dispatch workflow,
`.github/workflows/windows-conpty-diagnostic.yml`. It retains the unchanged
native helper/control assertions and bounded evidence, including failed cases.
There is no `continue-on-error`, accepted constant observation or replacement
sensor. Routine CI keeps mandatory Windows product tests; the experimental
capability failure remains a W07-064/T055a/T051 gap. Existing source-bound native
results below remain historical evidence, not a new runtime result for this
workflow relocation. Local workflow contracts and Actionlint pass for the new
boundary; hosted acceptance is recorded after CI completes.

## W07-064 first Windows capability harness - 2026-10-25

The integrated test-only C++/safe Rust/PowerShell harness is based on `d6d65e7`.
Root verified all 32 freeze02 artifacts and all nine integrated helper files;
one trailing space was removed during integration. Native independent review
accepted the first-four-case source scope after two reproduced harness bugs
were repaired: post-deadline cleanup waits and suppressed zero-byte failed
reads. The repaired controller shares one 25-second stopwatch, reserves cleanup
time and fails unconfirmed cleanup. Pending native I/O retains every referenced
buffer/handle until completion or supervisor process exit. Input hashes are
recorded before build and checked afterward.

Actual integration checks passed: 26 pure C++ checks, 48 synthetic PowerShell
receipt/deadline/identity checks, all 15 workflow contracts, standalone Rust
formatting and digest-pinned network-disabled Actionlint. Earlier PowerShell
AST checks covered all four scripts and the extracted CI step. Those checks
exercise local arithmetic, validation and wiring; hosted compilation and
runtime results are recorded separately below. The complete local verifier passed 1,432
outer harness cases, 1,127 library cases and 24 summaries, formatting,
warnings-denied lints/API docs and PostgreSQL 18.6 plain/TLS. The macOS socket
case remains a semantic skip. Log:
`/private/tmp/ignatius-w07-064-integrated-verify-20261025.log`. Documentation
was reconciled during/after this integration run; the installed pre-push gate
will repeat checks against the clean committed source.

Frozen source: `/private/tmp/ignatius-w07-064-d6d65e7-il6f_ql4/capability-control-freeze-02/`,
manifest `fcd7cfeaa3fa8bacb8c2773555071b94c740663fc254253e5029bf2cc517490a`.
Independent review: `/private/tmp/ignatius-w07-064-independent-review-02-20261025.md`,
SHA-256 `40aaead5667e256bc306a340ec4c086663af67b980b33f7cba94bdb024d2e4eb`.
Root integration receipt: `/private/tmp/ignatius-w07-064-root-integration-review-20261025.json`.
Both original failures, earlier freeze and compiled inputs remain preserved.

The primary admits only no-guard, known-unrestored, acknowledged hung-child and
actual guard capability to the authorised Windows CI after the publication
gate. Captures use the explicit synthetic metadata/trace upload allowlist,
excluding binaries, caches and config/data. No ConPTY child has run yet.
Wrong-ownership/native-error controls, the normal/error/uncaught-panic matrix,
paste/full T055a, Windows 11/Windows Terminal hand use and release remain open.
Four-case success alone cannot close W07-064.

Clean source `69f4de6` is published in draft PR33, stacked on unchanged D6.
The installed pre-push verifier repeated all 1,432 outer cases/24 summaries
successfully, with the same semantic socket skip. Digest-pinned Gitleaks passed
447 clean source files and 286 reachable revisions. CI run `40215610812` is
complete with 12 successes, one deliberate schedule-only skip and one Windows
controller failure before any case. Actual native helper compilation passed
Rust1.97.1 x86_64-pc-windows-msvc and MSVC toolset14.51/SDK10.0.26100,
Windows build26100 (runner image `win25-vs2026`/`20261021.278.1`).
Root verified the 94,212-byte archive digest and all 447 source/nine helper
hashes against `69f4de6`; summary cases are empty and post-input hashes match.
The initial empty capture inventory produced an array-index error. The literal
published expression reproduced that failure under local PowerShell StrictMode.
A three-script shared accumulator repair passed 52 checks, including all prior
48 and four real tiny owned-file/directory inventories, plus four script AST
checks. Independent review accepted it; root verified all 14 frozen artifacts
and exact integrated bytes. The 32 MiB admission, deadlines, first-four and
native ownership/code remain unchanged. The primary admits the same four-case
repeat after the normal pre-push gate. Repair source:
`/private/tmp/ignatius-w07-inventory-69f4de6-tblan28y/final-freeze/`; review
`/private/tmp/ignatius-w07-inventory-independent-review-20261025.md`, SHA-256
`736b89b6db7bb2023b17dc3f0dfd2172ed35657623243397a4f2a970720433cc`.
First failed Windows evidence:
`/private/tmp/ignatius-pr33-windows-first-40215610812-1/`. No ConPTY child ran.
The repaired clean head `7d167a9` is published after the unchanged full pre-push
gate passed all 1,432 cases/24 summaries with the same macOS socket skip.
Pinned source/history scans passed 447 files and 287 revisions. The hosted
repeat CI `40216991785` completed with 12 successful jobs, one deliberate
schedule-only skip and the failed Windows capability check. Four fresh native
children actually ran. No-guard, known-unrestored and acknowledged hung-child
controls passed; capability failed because acquired cursor visibility stayed1.
Actual Guard input masks were423/152/423 and native main/alternate/main sentinel
reads1/0/1. Cursor query success with constant1/1/1 does not prove cursor hiding
or identify a product defect. The observation method is under investigation.

All four exact Jobs were empty, guard/worker processes positively waited,
translated output complete/bounded and reader/HPCON/control-I/O cleanup
confirmed. Native durations were41.3788/34.0117/297.3594/48.9785ms, controller
95/81/340/92ms; the acknowledged hung interval was257.6195ms before owned
termination and actual exit124. Root verified the 101,380-byte archive SHA-256
`c55cfc2975541e6f2bf133bc85e32fd6f93eeb31b60637f029e8b25b96b8f1e4`,
all27 files,17 capture-manifest entries and447 source/nine helper hashes against
`7d167a9`. Actual checkout `da1283e4` has the identical head tree
`a2a46e4933966baedd0ad49ee5c979be17f1f7de`; revision identity differs normally.
Evidence: `/private/tmp/ignatius-pr33-windows-second-40216991785-1/`, root review
`/private/tmp/ignatius-pr33-7d167a9-second-windows-review-20261025.json`.
No assertion was weakened, product source changed or later matrix admitted.
Independent native audit:
`/private/tmp/ignatius-w07-windows-second-independent-review-9e8kfug0/REVIEW.md`,
SHA-256 `8170f35388d47964ef6b3e52cf3d0b0786dac8f977026b827e21f71892a96680`.
Root verified its two hashed review artifacts. The separate locked-Crossterm/
Microsoft routing assessment is
`/private/tmp/ignatius-w07-cursor-sensor-assessment-q4rnegi5/ASSESSMENT.md`.
It supports a sensor hypothesis but cannot identify the shipped26100 source.
The adopted one-actor calibration is integrated from repaired immutable02,
manifest `c44d1f1f3f8ea4e9c7455051cf32ccd447dd8a07ba80581f71c52a6ea7c810bc`.
Root verified all 75 artifact hashes and nine exact integrated source files.
Review01 caught successful-short sentinel reads masquerading as alternate
absence and incomplete restoration-query records. The repair independently
requires full nonce-derived sentinel counts and retains every verification
query/error/fact/clock. Both have C++/PowerShell failing-first evidence.
Native independent review accepted the repair; root's integrated repeat passed
74 portable C++ and 103 PowerShell cases. Six exact-emitter synthetic cases,
two exact wire roundtrips, four AST parses and Rust formatting are separately
qualified worker evidence. These checks are not Windows compilation/runtime.
Root's diagnostic-before-capability CI contract failed first, then passed;
formatting, pinned Actionlint, extracted PowerShell AST and independent review
also passed. Uploads add only six fixed diagnostic paths to the prior15 patterns.
The original four assertions and all ownership/deadline/environment protections
remain. The primary admits only calibration followed by the unchanged first
four on the reviewed source after the normal full pre-push gate. Published
`65cf237` passed that gate with 1,432 outer harness cases, 1,127 library cases
and 24 summaries; the macOS socket case remains skipped. The clean 447-file
source and 288 reachable revisions passed pinned Gitleaks. Actual PR merge ref
`dfe46f52` and head have identical tree `4ce8f1f7cf0b8e704294578d76fb1531b963dc21`.
CI `40221850172` completed: 12 jobs passed, one scheduled drift check skipped,
and Windows failed the unchanged Guard cursor capability after the diagnostic
completed. The one retained actor observed native-setter and main-VT hidden/
shown transitions, but alternate-VT visibility stayed 1/1. All stage writes,
marker readbacks and 36/36 sentinel reads completed, with masks 423/7 and cursor
size 25. Independent baseline verification succeeded before the final query.
Diagnostic duration was 54.2058 ms native/157 ms controller; actual alternate cursor
state remains unproved. The original no-guard/known-unrestored/hung-child controls
again passed and original capability failed. All five exact Jobs emptied and
owned process/drain/control-I/O cleanup was confirmed. No later matrix or
assertion change is admitted. Root verified 33 files, 22 capture entries and all
447 Git source/nine helper identities; archive 105,445 bytes SHA-256
`c78a505aa9c1994b8300080963e6e8268e3e1173e866a809f2527f1bb0fbac20`.
Evidence: `/private/tmp/ignatius-pr33-windows-calibration-40221850172-1/`;
root `/private/tmp/ignatius-pr33-65cf237-calibration-windows-review-20261025.json`.
This supports the sensor-routing limitation, not a diagnosed product defect or
proof that the installed 26100 binaries match current Microsoft source.
Independent native audit accepted the same limits and found no integrity or
adopted-scope inconsistency:
`/private/tmp/ignatius-w07-calibration-65cf237-native-review-j2f4lx0v/REVIEW.md`,
SHA-256 `64a56107cda058b343d6434393054bca603a91c94e99d9688b6f8bdbad5496a1`.
Executable/link hashes are recorded by the build/controller; their bytes are
excluded from the upload and cannot be independently rehashed from this archive.
Publication receipt:
`/private/tmp/ignatius-w07-65cf237-publication-20261025.json`.
Source: `/private/tmp/ignatius-w07-cursor-calibration-7d167a9-qgzdtg_o/cursor-calibration-freeze-02/`;
peer `/private/tmp/ignatius-w07-cursor-calibration-independent-review-02-20261025.md`,
SHA-256 `139acb266ab0fa04bf0aa3e9166c06adee202a83e0662134c405badbfa3beda0`.
New-head CodeQL remains pending until a main-targeted PR triggers the
unchanged filter. Overall capability and W07-064/T055a/T051 remain incomplete.

The separate read-only audit verified complete PG14/18/socket logs: each TCP
job ran 236 selected invocations, plus one actual socket query (473 combined).
Those include 24 actual retained-result and 40 notice invocations across both
TCP routes/majors. Selected counts also include pure/local controls; successful
libtest capture does not alone exclude every miscellaneous early return. Actual
major assertions are 14/18; patch labels remain reviewed image provenance.
Evidence: `/private/tmp/ignatius-pr33-db-hosted-audit-51oq6q99/RECEIPT.md`.

## Long-line editor correction - 2026-10-25

Feature025 FR-2529 / T058 is integrated from clean `d92c416` with five owned
source paths. The root presentation/plan/task amendments precede implementation.
Actual 100x30 Buffer tests first recorded four failures and one short-ASCII
control: missing long-line caret and incorrect wide/combining/control hits.
The first candidate then passed 76 widget and 114 layout cases. Independent
review found an unfocused zero-width hit override that disagreed with painting;
a separate failing actual-Buffer case precedes the one-predicate focus repair.
Final focused counts are **78 widget and 114 layout cases**, with formatting
and warnings-denied Clippy passing. All original 26,505 widget-test bytes remain
an exact prefix; the short ASCII control preserves all 3,000 cells exactly.

Root verified every artifact in both immutable freezes (49 and 38 respectively),
the 446 compiled input hashes against the published base or owned candidate
files, and the five installed files against the final freeze. Root inspected
seven synthetic raster files, including before/after long-line, Home, control,
combining, error, completion and tiny-capacity views. Raw cells remain primary;
Menlo/indexed/default/DIM approximations and missing CJK/emoji font glyphs do not
establish native appearance. The short completion regression proves display-cell
agreement; it is not an exhaustive scrolled-popup matrix, where right-clamping
can hide anchor differences. No memory/latency ceiling or grapheme-based editing
claim is added. Source positions and undo remain scalar/byte based.

The full integrated local verifier passed **1,515 outer harness cases, 1,207
library cases and 25 summaries**, with zero failures/ignored cases and all six
executed gates. Existing synthetic plain/TLS servers were freshly queried as
PostgreSQL **18.6**; the macOS Unix-socket case is still an explicit semantic
skip. Local tools are macOS 27.0.1 arm64 and Homebrew Rust/Cargo 1.99.1; hosted
Rust 1.97.1/MSRV acceptance remains separate. Log:
`/private/tmp/ignatius-t058-full-verify-20261025.log`, SHA-256
`03c40158ff7af92030466bd04d9ad3591c7b9b0bbbd0cfd502ee054ca0e8b462`.

Root's identical owned 100x30 PTY script and synthetic config show the typed
end sentinel absent before and present after, with Home revealing the prefix
in both. Both quit zero, enter/leave the alternate screen and preserve config
bytes. No profile was selected or SQL submitted. Before binary is clean d92;
after binary is d92-modified with the exact installed freeze02 source, not a
clean publication identity. Script SHA-256:
`8b12cc6426c98e299ded8f9c42fad8ad154de9e5d81a1c17b5dfa0414f06a678`.
Receipts: `/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-before-c0901prg/receipt.json`
and `/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-oarmp52a/receipt.json`.
Two prior harness redraw expectations and one non-integrated verifier attempt
are retained and excluded from acceptance. The latter was stopped by exact
task-owned process ancestry before the source-bound full repeat above.

Freeze02: `/private/tmp/ignatius-editor-eol-red-1di2ma1e/freeze-02/`, manifest
`1acce2109b6ac2cc8cdd0efa90f08284f07532e544ef589b8d51f50a4f93de71`.
Native independent review:
`/private/tmp/ignatius-t058-freeze02-independent-review-224cbtyy/REVIEW.md`.
The owner's external-review skip remains in force; no different-family provider
review is claimed. Hosted acceptance, native terminal/font/hand use, fresh
main-targeted CodeQL, parent integration and release remain separate gates.

Published clean `14d18ad97f1aafd4a4d953ea5e35f3b2f644b1a0` in draft PR36.
The normal unmodified pre-push hook repeated all 1,515/1,207 cases and six gates,
with the socket skip retained; log SHA-256
`2715c9214280f72d6d9ac567a1f7fd7caefeee926f2ba31c2d6e909e89d9149a`.
Pinned offline Gitleaks passed the exact 446-file clean Git archive, whose every
blob was independently checked against Git; source digest is
`3ac4e9d43c6aa65bc612d8e0a6868443bb97231eff72be28b4a9e380fa17bd71`.
Its all-ref history scan reports 292 non-merge commits among 321 reachable
local revisions; do not confuse those with head-only ancestry counts. Receipt:
`/private/tmp/ignatius-t058-14d18ad-scan-source-20261025.json`.

The clean local publication binary (SHA-256
`a58af2e58b011334333ea2b932f59244d8121b407e82d41b2472d493561b7976`)
passes a separate 100x30 PTY repeat. One retained early-owner harness race left
typing in the picker; the corrected harness waits for actual owner-change
repaint. It then shows the end sentinel and Home prefix, quits zero, restores
the alternate screen and preserves config without profile selection or SQL
submission. This supplements rather than relabels the identical-script
before/modified-source pair above. Clean receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-ni4f6ssl/receipt.json`.

Hosted run `40234580931` uses actual merge `42be23db`, with the same tree
`384e26053d4653a51652b965c9bcad19103b0bd0` as the head. Root inspected complete
PG14/18 and socket logs: 236 selected invocations per major plus one actual
socket contract, 473 total including pure/local cases. Readiness and the
plain/verified-TLS runtime-major/transport guard are positive. These are not
473 live queries, and runtime patch versions were not printed.

The run completed successfully with **13 successful jobs and one intentional
schedule-only fixture skip**. Root verified all eight platform artifacts and
21 policy artifacts. Every platform actually executed all 18 T058 cases with
Rust 1.97.1. Full harness/library totals are Linux 1,515/1,207, macOS
1,515/1,207 and Windows 1,459/1,188; compile, docs and startup/refusal smoke also
passed. Doctor retains three explicit skips each. MSRV Rust 1.90 compilation
passed; six allowed duplicate-version warnings remain. The release scope
reports 93 cases but three conditional Python archive-helper branches are not
independently runtime-proved; synthetic upload scope is not a real release.
Platform manifest:
`d0774396161695bfba219f4923217a8f22fc4810f08ddaa4b1d04d8cb5841c3b`
at `/private/tmp/ignatius-pr36-platform-audit-int5fsm7/`.
Policy manifest:
`a9473fa7aa8294d1ad6d6c7a8ca983311ca52bd2d48c0a860fbce56380129b3d`
at `/private/tmp/ignatius-pr36-policy-audit-hx59gy/`.
CodeQL's main-base filter is unchanged; no fresh feature-base analysis is
claimed. Native hand use, parent integration and release still require their
own acceptance.

### W10 long-line renderer diagnostic - 2026-10-25

A single identical test-only harness, exact d92/14d source archives and 12 fresh
children compare three samples per revision/size. At 16 KiB the parent/candidate
median renderer times are 6.481/19.611 ms and process maximum RSS 11.953/13.062
MiB. At 1 MiB they are 264.462/902.313 ms and 17.453/146.500 MiB. All children
preserve source, scalar cursor, revision, modified and undo state; only the new
view paints a visible caret. This is a material local test-profile regression,
not release-build p95 acceptance or a whole-process ceiling.

Root verified all 951 baseline artifacts. Compiler RSS is excluded; one
sandbox-denied resource observer sample is retained/excluded, then normal
reviewed escalation admits the valid observations. Exact host/tool/profile,
raw timing/RSS and uncontrolled-background-load limits are in
`/private/tmp/ignatius-editor-perf-2kfyw0q6/REPORT.md`, manifest
`aaa9011c90d1a7f63127411da7d573c3a762fe41369d7dcf132f740aa31b512f`.
FR-2530/T059's bounded projection is now locally accepted. Final 83 widget and
114 layout cases, formatting and warnings-denied Clippy pass. Root verified
all 80/58 artifacts in the two correction freezes, accepted independent source
reviews, and independently compared every JSON field and all 51,081 cells in
21 retained frames to the accepted T058 captures. Original widget tests are
byte-identical and original view tests remain an exact prefix. Cell comparison:
`/private/tmp/ignatius-t059-root-cell-comparison-20261025.json`.

The final small ASCII path runs only after equality with the full existing
sanitiser, then `is_ascii`; controls and Unicode retain their canonical paths.
Caret discovery keeps scalar facts, and projection filters before retaining
units. The metadata bound does not cover total source/sanitiser allocations or
make an RSS-ceiling claim. Final freeze:
`/private/tmp/ignatius-editor-projection-cjmu5kiy/freeze-02/`, manifest
`6622e7cde8d4b6f07028b68b148344bd7df5567e1a948955c6ca0a81d193c578`.
Independent microdelta review:
`/private/tmp/ignatius-t059-freeze02-independent-review-t5zkt4zn/REVIEW.md`.

With the exact same `4f3914b5785cf315a840964d52a7b3c3c04bca62bdfa47c6eed512ac0f37a322`
harness, six fresh final processes preserve state and one visible caret.
At 16 KiB, median renderer time/RSS is 6.198 ms/12.156 MiB; at 1 MiB,
138.342 ms/17.063 MiB. Three-sample ranges are retained. The intermediate
bounded-only 618.566 ms/16.031 MiB observations and non-identical formatted
harness trials remain distinct; formatted trials are excluded. One earlier
private-overlay failure log was overwritten; its tool tail and later complete
failure are retained, not recreated or accepted. No diagnostic harness ships.
These are unoptimized test-profile observations with uncontrolled host load,
not release p95/100-action, process ceiling or native-terminal acceptance.

The full integrated local verifier passes **1,520 harness cases, 1,212 library
cases and 25 summaries**, zero failed/ignored, all six executed gates. Existing
PostgreSQL 18.6 plain/TLS ran; macOS socket remains skipped. Log:
`/private/tmp/ignatius-t059-full-verify-20261025.log`, SHA-256
`9545a73faec3ab23d1560e4f7a65ed1d41c8038d7a1ee2997e0ad95d89f61107`.
The actual 100x30 owned macOS PTY end/Home/quit/config control passes at the
modified parent build with exact integrated final source. Receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-jzgapy6r/receipt.json`.
Published clean `d2edce1bde1e92ca193a94f84c954e8bb594e6d3` in draft PR37.
Normal pre-push repeats all 1,520/1,212 cases, with six passing gates and the
same socket skip; log SHA-256
`9090ca7cd89cccc02b19ce893172d321b567345176362f9a877d22a3fbd8128f`.
Pinned network-disabled Gitleaks passes the exact 446-file Git archive and
293 non-merge revisions among 322 local reachable revisions; every archive
blob is checked against Git. Source digest:
`4272b687c41e3514d79b7e1bad315f6beb46fe80d7a9371f3550c67edc779113`.
Receipt: `/private/tmp/ignatius-t059-d2edce1-scan-source-20261025.json`.

The clean publication binary (SHA-256
`ba37ca675819d3d15fe5f11d9ecdb35eb07b0f34e27629e116d88c5d0b5fb5dd`)
also passes the 100x30 owned-PTY control, with normal exit/config preservation
and no connection selection or SQL submission:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-editor-caret-pty-after-3z02aab9/receipt.json`.

Hosted run `40236560722` uses actual merge `882873ec` with the same tree as
head, `c9276f6b71c45dc439f247e37ef9b9431cd6d414`. Root accepts complete PG14/18
plain/verified-TLS and socket logs: 236 selected invocations per major plus one
socket, including pure/local controls. Positive readiness and runtime-major/
transport guards establish those live paths, not 473 queries or printed patch
versions. Seven policy jobs also passed; root verified their 20-artifact
manifest `ce9a3466760aa43eb54f72d93fc0ef427df0bdc853215486712ed72812064d90`
at `/private/tmp/ignatius-pr37-policy-audit-Ha8yQb/`. Six permitted duplicate
warnings, conditional Python archive-test runtimes and feature-base CodeQL
limits remain qualified. The run completed successfully with **13 successful
jobs and one intentional scheduled-fixture skip**. Root verified all eight
platform artifacts: Linux/macOS harness/library totals are 1,520/1,212 and
Windows 1,464/1,193. Every platform executes all five T059 and 18 T058 cases
under Rust 1.97.1; compile/docs/startup-refusal smoke passes. Doctor retains
three explicit skips each. Actual smoke binaries identify the clean merge,
while exact-head diagnostics bind the published head; equal trees establish
the product source, not identical build identities.
Platform manifest:
`9a350e422a0f5ff1df4d8ea6ade34d8c0028b10430cd90e2d59d625f22cedebd`
at `/private/tmp/ignatius-pr37-platform-audit-nwy_o3ki/`.
The final documentation checkpoint preserves all production/test source bytes
from accepted `d2edce1`; its current-head workflow remains in PR37's checks.
Whole resource, native and release-performance acceptance remain open.

## Manual theme integration - 2026-10-25

The manual FR-2513/2515/2516 slice integrates the immutable pure-theme and strict
configuration lanes with the root-owned startup cache, action, discovery and
runtime handling. Automatic background detection/T032 remains open. Root read
the parser/resolver/admission code and verified all 39 pure-theme and 127
configuration artifacts. Original Theme/Results assertions are preserved after
the diagnostic identity accessor migration; configuration preservation records
cover 41 original cases and original TOML errors/spans. Runtime review caught
two exhaustive discovery omissions, corrected before combined compilation.

Focused evidence includes the configuration lane's 152 passing cases and 150
actual isolated CLI child invocations. Root's four real-CLI tests pass again on
the integrated source. Seven reducer cases prove custom/builtin cycling,
busy-query and owner preservation, query-diagnostic mismatch handling and
profile/resize/async retention. The size-independent runtime regression failed
before correction and then passed; all six existing geometry/review controls
pass. Keymap 31 and discovery 14 focused cases pass. These overlapping scopes
are reported separately rather than added to a full-suite total.

The initial T033 freeze passed two tests across 2,520 actual 120x40 buffers.
Root verified 101 artifacts, inspected seven comparison sheets and four
individual images, then found a selected palette group/detail ink defect that
the initial assertions missed. The original captures remain pre-correction
evidence. T056 records actual failing palette/full-frame tests, followed by a
small span-style composition correction and stronger assertions. A first
candidate's extra Basic16 label change was rejected; final label ink is exactly
preserved. Root verified all 125 correction artifacts and inspected corrected
HighContrast/inverted-custom/colour-off palette images.

Root independently compared all 12,096,000 cells before/after: 2,340 foreground
cells in 60 painted palette frames change, confined to selected group/detail.
All symbols, geometry, backgrounds, modifiers and selected labels are unchanged;
all 2,100 Basic16/off and 2,160 non-palette frames are exactly preserved. These
overlapping counts must not be added. RGB/indexed/Reset/DIM/font approximations
are explicit in the synthetic rasters; raw actual cells are primary. No native
terminal appearance or participant-use claim follows from these captures.

Pure freeze: `/private/tmp/ignatius-us5-theme-ffb86a1-z53t1vo8/pure-theme-freeze-01/`
(manifest `edf823b9f4c69bb56ef2669e0501e3b230873a005a5197d12145a0fbfce11263`).
Config: `/private/tmp/ignatius-us5-config-ffb86a1-gcucqbu1/config-phase2-freeze-01/`
(`4d3b42e1232664a6c29c7f16b95462c468f0d4937664fec8e76a86376d10f873`).
T056: `/private/tmp/ignatius-t056-palette-eigzhww5/t056-correction-freeze-01/`
(`a710845a90ea43209a53e5d6d01506a8e77d5669f2ec1747f2f0698edeec8636`).
Root cell comparison: `/private/tmp/ignatius-us5-root-t056-comparison-20261025.json`.
The first combined local verifier passed **1,492 outer harness cases, 1,184
library cases and 25 summaries**, with zero failures/ignored cases and all six
executed gates green. PostgreSQL plain and verified-TLS ran; both existing
synthetic containers were freshly queried as 18.6. The unconfigured macOS Unix
socket is a semantic skip, not a pass. Actual local tools are macOS 27.0.1
arm64 and Rust/Cargo 1.99.1. Log:
`/private/tmp/ignatius-manual-themes-integrated-verify-20261025.log`, SHA-256
`045db75e41363664e9fcf637cf5a0deb52b90a999fb651bd65fdef7bd6774997`.
Root also independently confirmed the exact normalized bodies of all 41 original
configuration/diagnostic tests and the store helper. Publication,
hosted/pinned/MSRV and fresh CodeQL acceptance remain pending. Final integration
review found the existing ignored `[ui] color` startup/doctor route after this
pass. That pre-T057 pass does not establish configured-colour acceptance.
Root's independent actual-binary PTY probe confirms the omission: Custom plus
configured never paints 119 extended-colour SGRs, explicit always 119, and
NO_COLOR plus always zero. Each owned macOS PTY uses a synthetic profile picker,
chooses no connection and exits normally with alternate-screen entry/exit
captured. No native emulator/font or full-mode restoration claim is inferred.
Before receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-theme-pty-3i9adtvz/receipt.json`;
binary SHA-256 `93242cfce604c9336df67eeb42dd05ff982ae31d3f295205611328629559bbb9`.

T057 is now integrated after independent root review and verification of all
208 frozen artifacts. It retains explicit colour flag presence until the loaded
configuration is available and applies the existing resolver before depth/cache
selection. Runtime and doctor share this policy; raw facts and report schemas
remain unchanged. The lane passed 64 distinct focused cases and 27 actual doctor
JSON subprocess controls, with watched runtime/doctor failures retained.
Freeze: `/private/tmp/ignatius-t057-colour-route-vsz02y83/final-freeze-01/`, manifest
`69c866f92f1e2ebc1c4dd0bb95c40a7b93152563ac9947051e4891d44969af5d`.

The final combined repeat passed **1,497 outer cases, 1,189 library cases and
25 summaries**, all six executed gates, zero failures/ignored cases and the same
socket skip. Log: `/private/tmp/ignatius-manual-themes-t057-verify-20261025.log`,
SHA-256 `965e7cd3662408e9da11c3d59a3647c1bdda22aac32bd20b5dccd418496da914`.
The rebuilt actual binary's three owned PTY controls all exit normally with
alternate-screen entry/exit captured. Configured never now emits zero painted
SGRs; explicit always and NO_COLOR raw captures remain byte-identical to their
pre-fix controls. After receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-theme-pty-qtg0xxks/receipt.json`,
binary SHA-256 `3d0c50917ae5640bfda906be225314360c1ad015c18130319062c252eed01883`.
These are local macOS 27 arm64/Rust 1.99.1 results, with no native font or
emulator claim. Published clean `d92c416` in draft PR35 after the full unchanged
pre-push verifier repeated all 1,497 cases. Pinned Gitleaks passed 444 exact-source
files and 291 non-merge commits among 320 reachable local revisions.

CI `40231713035` completed with 13 successful jobs and one deliberate scheduled
fixture-drift skip. Actual merge `253ac7cc` and head share tree
`00158901056a97195719d3e08026eeeff7234254`. Independent platform audit confirms
the same 74 selected manual-theme/T056/T057 cases executed on each primary
platform under Rust 1.97.1; full harness/library totals are Linux 1,497/1,189,
macOS 1,497/1,189 and Windows 1,441/1,170. Startup/refusal and docs passed, with
doctor's three explicit skips per platform. Platform fixture guards are not live
database claims. Separate PG14/18 jobs each passed 236 selected invocations,
including positive runtime-major/readiness and plain/verify-full routes; one
actual Linux socket query passed. The 473 total includes pure/local cases.

MSRV 1.90 compilation and policy jobs passed. Six configured duplicate-version
warnings remain; three reported archive-helper passes have conditional Python
guards without separately printed positive invocation evidence. Neither is
converted into a zero-warning/no-semantic-skip claim. Full log receipts:
`/private/tmp/ignatius-pr35-platform-audit-2u768bko/receipt.json` (28 artifacts),
`/private/tmp/ignatius-pr35-policy-audit-ziQQqR/RECEIPT.md` (37 artifacts), and
`/private/tmp/ignatius-pr35-d92c416-db-audit-20261025.json`.

The actual clean published local binary also passed a 180x40 owned-PTY keyboard
journey through all four palettes, retaining typed SQL/config bytes and exiting
normally without connection selection or SQL execution. Earlier probe harness
setup/clipping/wrong-RGB expectations are retained as failed harness attempts.
Final receipt:
`/var/folders/vq/3120439x7q715kvbyn0jbxrc0000gn/T/ignatius-theme-cycle-pty-h7f9buo8/receipt.json`.
Fresh main-targeted CodeQL, parent integration, native desktop appearance/hand
use and release remain open. Post-publication acceptance notes remain local
owned documentation edits until the next meaningful source checkpoint.

## Expanded Results selection correction - 2026-10-25

Feature025 T055 starts from reviewed D6 on its own sibling branch. The actual
expanded Results renderer at equivalent D6/7d/65 production bytes produced a
selected HighContrast header foreground/background `#ffffff/#ffffff` and NULL
`#d0d0d0/#ffffff`; Selection specifies Surface ink `#000000` on that fill.
Positive row/content/background and unselected controls precede the meaningful
actual-Buffer failure. Root checked all 16 immutable reproduction artifacts;
manifest SHA-256 `a5c5e55a6cdfaadb040adda91d803fe545bd0b901f487d3220a178b24845f920`.
Evidence: `/private/tmp/ignatius-us5-selection-probe-3k_rf75u/freeze-01/`.

The scoped contract requires final Selection precedence on header/arrow/value
spans, preserving semantic modifiers, ordinary unselected colours and existing
basic16/colour-off reversed selection. Three built-ins, four depths, three glyph
tiers and both densities form 72 actual-renderer combinations. The two-file
correction is integrated byte-for-byte from immutable freeze01 (76 artifacts
verified), manifest SHA-256
`2d2f4b9608e87cdbc9161046f375d9f90361dca8b2a858a449a450f661c1da4d`.
All eight original Results tests are byte-identical; 11 Results and 20 theme
tests passed. Three maintained tests cover actual ink/modifiers, unselected and
colour-off controls, and NULL/empty/literal-NULL distinctions. The maintained
failing-first Header assertion is separate from the initial probe's qualified
setup correction. Independent review found no blocker:
`/private/tmp/ignatius-t055-peer-review-yojn9ktl/REVIEW.md`, SHA-256
`2ee03e1b664aeea1feb183d0af235eaad97a7c28b00e8e6a77416bb7d86f50f4`.

Root inspected the contact sheet's 12 before/after synthetic rasters and verified
their source/JSON capture identities. Only selected painted foregrounds change;
symbols, positions, backgrounds and modifiers match. Basic16/None/explicit
colour-off scenes are byte-identical. Font, indexed/reset colour and DIM
approximations remain explicit; this is not native terminal appearance evidence.

The first full gate preserved six TLS failures: this new worktree lacked the
ignored disposable CA/client certificate/key files used by the existing TLS
container. Root inspected the read-only mount and copied only those three
synthetic files, keeping the key 0600 and directory 0700. No container, test or
product-policy change occurred. The full repeat passed **1,434 outer harness
cases, 1,130 library and 24 summaries**, all six executed gates and actual
PostgreSQL 18.6 plain/TLS. All six affected cases now pass. The macOS socket case
remains a semantic skip. Actual local tools: macOS 27.0.1 arm64, Rust/Cargo 1.99.1;
hosted pinned/MSRV/platform acceptance is pending. Repeat log SHA-256
`dab6ff370743602b9dbdb18298c96d877f2b43e74d2a8cd6dfe8b13675f42fa3`.
Logs: `/private/tmp/ignatius-t055-integrated-verify-20261025.log` and
`/private/tmp/ignatius-t055-integrated-verify-repeat-20261025.log`; root receipt
`/private/tmp/ignatius-t055-root-integration-20261025.json`.
This is neither a palette correction nor broader US5/Windows acceptance.

Published clean `ffb86a1` in [draft PR34](https://github.com/tkomane/ignatius/pull/34)
after the unchanged pre-push verifier again passed all 1,434 cases/24 summaries,
with the same socket skip. Exact source 438 and reachable history 290 passed
pinned Gitleaks. PR merge ref `d38304f9` matches head tree
`77c2add104a13c5bd20bca99eb687d0de9f0578a`. CI `40223833554` completed with
13 successful jobs and one deliberate schedule-only fixture-drift skip.
Independent platform audit confirms every new T055 case ran on Linux, macOS
and Windows with Rust 1.97.1; full library passes were 1,130/1,130/1,111.
Startup/refusal smoke succeeded, with doctor's terminal/size/connection skips
preserved as limits. Hosted runners do not establish native terminal appearance.
PG14/18 plain/verify-full and one real Linux socket passed. The 473 selected
invocations include pure/local cases; positive fixtures and runtime-major guards
support the live claims, not a claim that all selected cases queried a server.
Complete job logs/source-linked receipts:
`/private/tmp/ignatius-pr34-platform-audit-kmb7tndf/RECEIPT.md` (SHA-256
`8e2cd6155079190e78da21487a0c77089055410f976a462259215c62ac944095`) and
`/private/tmp/ignatius-pr34-ffb86a1-db-audit-20261025.json`.
Fresh CodeQL follows parent integration/retargeting under the unchanged
main-target filter; it is not yet accepted. Publication receipt:
`/private/tmp/ignatius-t055-ffb86a1-publication-20261025.json`.

## W10-012 integrated local verification - 2026-10-25

The frozen isolated handoff passed 750 focused library cases and 32 actual
PostgreSQL 18.6 integration invocations across plain and verified TLS, with no
skips. The lead verified all 129 recorded artefact hashes and the original 43
moved test names. Native peer review found doctor guidance and pending-attempt
replacement/old-profile retirement gaps. The repaired freeze passed re-review:
29 artefact hashes verified, 37 interactive cases on each plain/verify-full
route and nine reducer cases inspected, with no remaining scoped blocker.

The first canonical verifier collected one new harness failure: its transport
assertion treated every non-verify-full target as explicitly TLS-disabled.
The default local Prefer route correctly reports NotNegotiated. No product TLS
policy changed. Corrected requested-versus-observed assertions passed three
actual preferred-plain/disabled-plain/verified-TLS probes and native review.
The complete repeat then passed **1,370 outer harness cases, 1,072 library cases,
23 summaries**, formatting, warnings-denied workspace Clippy/API documentation,
workspace/doc tests and PostgreSQL 18.6 plain/TLS. The macOS socket case remains
a semantic skip. Logs:
`/private/tmp/ignatius-verify-2026-10-25-notices-first-failure.log` and
`/private/tmp/ignatius-verify-2026-10-25-notices-recheck.log`.
The repeat records all 434 tracked/untracked source inputs, digest
`a801a9edbd41ba19cde4f2ae76f1c889828e4ae47014bdcc38c026147ccb0476`;
it observes the modified integration tree, not a clean/hosted candidate.
Subsequent edits reconcile documentation; the installed pre-push gate will
verify the clean committed source again before upload.

The lead captured 128 styled synthetic Ratatui buffers from a fresh Cargo-
selected non-test library and inspected representative normal, precision,
small-size and protected-review images across both densities. Four modes cover
dark/light/high-contrast truecolour Unicode and ASCII/no-colour. All 399 captured
artefact hashes verify, with no unsupported glyphs. These establish rendered
cells and pure action assertions, not native appearance, hand use or reading.
The capture precedes the final runtime-only retirement repair; it is not an
attestation of later application bytes. Evidence:
`/private/tmp/ignatius-notices-styled-uakynqds/`, manifest SHA-256
`fa64e7125ed30d7fdcb8f7d4e019abcbee7889c3c13cefa4161cdd6cf0c80b3b`;
source-input digest
`796ad74705923f3d4c188484a12af3f6d40b52d0b1a942c7068969e36f07e3f7`,
non-test library SHA-256
`37be5988221c6c91b53ba1f33611199f99f00be8ecc68eb743e657e59c745a0b`.
The Inspector-to-palette harness assumption was corrected to preserve existing
palette behavior; the covered-inspector case explicitly injects a synthetic
snapshot. The raster manifest excludes its still-open stdout log.

CI review reproduced a missing live-endpoint route for the new notice contracts.
The existing workflow contract failed before repair and passed afterward:
`/private/tmp/ignatius-notice-ci-wiring-{red,green}-20261025.log`. The PostgreSQL
jobs now include notice ingestion and runtime lifecycle cases on both plain
and verified TLS. Fixture-major assertions are being made portable for declared
PostgreSQL 14-18; that is not evidence of those versions running. Digest-pinned,
network-disabled actionlint passed locally. Fresh Gitleaks found no leaks in
280 reachable Git revisions and the 434-file modified source snapshot. Logs:
`/private/tmp/ignatius-gitleaks-notice-final-{history,source}-20261025.log`;
snapshot `/private/tmp/ignatius-notice-final-scan-a2ncvln9/`. These precede the
small transport-harness correction; a clean-source scan follows the commit.
The subsequent hosted result is recorded below.

Clean publication checkpoint `fda16f8041c4177ded2b451ec40ae252e9b9174e`
passed its installed pre-push full gate again: 1,370 outer cases/23 summaries,
plain/TLS PostgreSQL, with the same explicit macOS socket skip. Push log:
`/private/tmp/ignatius-push-fda16f8-20261025.log`. Exact clean source (434 files,
tree digest `aedd1c7786c95584f3e901b18044bea7201374d3e654a753989b5e8bf960828a`)
and 282 reachable Git revisions passed fresh pinned Gitleaks scans. Snapshot:
`/private/tmp/ignatius-source-fda16f8-pdi0klk0/`; logs:
`/private/tmp/ignatius-gitleaks-fda16f8-{source,history}-20261025.log`.
[Draft PR #32](https://github.com/tkomane/ignatius/pull/32) is published;
hosted CI `40202770798` and CodeQL `40202770715` completed for that exact head.
Sixteen workflow jobs succeeded; the upstream fixture-drift comparison was
schedule-only and skipped. The separate built-in CodeQL check resolved its
temporary neutral/missing-Rust state to success with no new alerts. No pending,
failed or cancelled check remained. Fifteen annotation sets contained runner
notices only, with no warning/error annotation. No rerun or source fix was needed.

Hosted library counts were 1,072 Linux/1,072 macOS/1,053 Windows. Their database
early-return cases are not live evidence, and doctor smoke has three expected
nonterminal/no-target skips. Separate PG14 and PG18 jobs each ran 210 selected
invocations across plain and verify-full TLS, including ten notice integration,
eight notice sidecar (six actual-session) and nine Session cases per route.
Those full logs have no semantic-skip line. The Linux Unix-socket job passed its
one actual query contract with the URI supplied, without a semantic skip.
Rust1.90, lints/docs, dependency/licence, secret, workflow/drift and all three
CodeQL analysis jobs also succeeded. Native Windows terminal/hand-use and the
schedule-only upstream comparison remain separate.

The Actions checkout is PR merge ref `d9198a74aaebfa2165b25cdb70f3aa0c4d111f50`;
the lead fetched it and verified tree
`a561e457e7e9ab6923edc39e065e9459f5d38146` equals published head `fda16f8`.
Build identity may name that merge ref; this is source-tree equivalence, not a
packaged-candidate claim. Complete check listings and PG/socket logs were read;
platform/Rust-CodeQL log APIs are truncated, so no exhaustive internal log/skip
inventory is claimed for those jobs. Evidence and limits:
`/private/tmp/ignatius-pr32-ci-fda16-20261025/`; both manifest hashes verified.
Later W10-013 working source is excluded from these results.

## Published notice-source PostgreSQL 15 smoke - 2026-10-25

Clean published `fda16f8` debug CLI (Homebrew Rust1.99.1, macOS27.0.1 arm64)
passed exact CSV/one-omission output for 257 notices against PostgreSQL15.19,
plain and verify-full TLS, with server-version/orders health before/after.
The image was immutable arm64 PostgreSQL digest
`a8e042598834300dfaa17a54f695b87b7f25d3490433bbbe2ef3bffbb9747f24`.
The independently inspected controller used unique task labels and allocated
loopback ports; both owned containers/anonymous volumes were removed. Root
verified the two output digests, no labelled containers remained and all five
pre-existing running IDs were preserved. Evidence:
`/private/tmp/ignatius-resource-fixture-proof-mmfwivkh/`; controller SHA-256
`17c5fa99a8f15a2c3d357daae1a67d6201e206399e6fdfa5d61388cd0df8cb6f`.
This is a small published notice-source smoke, not a full PG15 suite, W10-013
acceptance or performance/RSS evidence.

## W10-013 isolated live retained-data verification - 2026-10-25

The modified isolated `fda16f8` candidate passed six fresh-process cases on each
of PostgreSQL18.6 plain and verify-full TLS, with 24 separate CLI health children
confirming version18.6/orders3 before and after. Root verified 36 observations,
72 stdout/stderr byte counts/digests and all 448 prepared input hashes afterward.
No case skipped, exceeded its owned deadline/output/RSS stop or changed fixtures.
The measured debug build uses Homebrew Rust1.99.1 on macOS27.0.1 arm64;
this is separate from pinned/MSRV/hosted/native-terminal acceptance.

The 40x1-MiB buffered result retained 31 whole rows (32,507,503 logical occupied
bytes), received 40 and real CommandComplete40. Cumulative exhaustion preserved
the healthy first result and later SELECT42 count-only outcome. Row-cap0 skipped
the oversized discarded value while admitting later descriptions; real
SQLSTATE22012 preserved its completed prefix. The owned TEMP INSERT RETURNING
reported 40 affected and a subsequent same-session SELECT saw all 40 rows.
Both streaming controls verified all 40 rows/41,943,080 bytes with SHA-256
`168d2882b1bff9d4e7e2f16ec515bb39d3ebfc4c24335c4cc4e90a98dc18f8d0`.
Rust and the controller checked every byte before removing only the verified
owned CSV. Root checked those assertions/cleanup receipts and path absence;
it did not re-read the removed payloads. Completed-case RSS spans10.91-45.14MiB
across distinct workloads, not a statistical comparison or process ceiling.

Evidence: `/private/tmp/ignatius-w10-013-fda16f8-TfHH4z/evidence/`, including
`live-preparation.json`, `live-results.jsonl`, `live-cleanup.json`,
`live-summary.json` and `root-live-review.json`. The accepted pre-live freeze is
milestone02, manifest SHA-256
`f14e7b961c2e539cc6c2c6667129a53a6b08995dd29ee2d7abb5125a875c2bf8`.
A subsequent UI-only repair covers complete warning admission in reachable
one/two-inner-row Results panes; it changes no measured Session/core/output
code. Canonical combined/full/rendered and affected-major gates follow separately.

## W10-013 canonical full and rendered gate - 2026-10-25

Root verified all 268 final milestone03 artifact hashes, all 24 applied code
paths and unchanged original sidecar assertions. Final native peer review
accepted the narrow UI repair; all 446 other live-preparation inputs still
match the executed milestone02. Peer receipt:
`/private/tmp/ignatius-w10-013-peer-review-20261025/REVIEW-03.md`, SHA-256
`73e0db04678bcde622de2e2b388c2a1fd8b9f19cd2be1572dda82571e720982d`.

The complete canonical locked/offline verifier passed **1,431 outer harness
cases, 1,127 library, 24 summaries**, formatting, warnings-denied workspace
Clippy/API docs and PG18.6-backed tests. The macOS socket case is a semantic
skip. Source was the modified integrated tree based on `fda16f8`, with 437
tracked/untracked inputs, digest
`7f92451d3a5ae552e51aefed7ecb66f80e2fce10460ec357dc7fbc8edbcd083f`.
Log/receipt: `/private/tmp/ignatius-verify-2026-10-25-retained-result.log` and
`.json`; subsequent documentation reconciliation is separate from these inputs.
The CI omission regression failed with zero required routes before repair,
then passed with explicit plain/verify-full retained-result contracts. Pinned,
network-disabled Actionlint passed. Logs:
`/private/tmp/ignatius-retained-ci-wiring-{red,green}-20261025.log` and
`/private/tmp/ignatius-retained-actionlint-20261025.log`.

The fresh Cargo-selected non-test library produced 112 actual styled synthetic
Ratatui buffers: 14 scenes, both densities, dark/light/high-contrast truecolour
Unicode and ASCII/no-colour. Root verified all 354 artifact hashes and inspected
eight contact sheets (16 frame images), including later-set loss, unavailable
counts, maximum scalar stress, zero retained prefix, protected review and the
reachable one/two-inner-row split. Complete warning/count admission and ownership
have no remaining scoped material blocker. Unsupported glyphs: zero. These are
rendered-cell checks, not native screenshots or hand use. Evidence:
`/private/tmp/ignatius-retained-styled-3rtx76kd/`, final manifest SHA-256
`a93728fabc53bfc56b7ce4b229d418a7bddc41317ef334fd9d60e321d22cb851`,
source-input digest
`bf6f89ed7632919c050b1d0f2a8d3bcf2b6b1d6dc1665d209daea77bd662fc61`,
non-test library SHA-256
`8e81a2409e6c73e04326e3847d5c6549b72816042b228f3ac3b575dba0196575`.
Clean publication checkpoint `54056c210ddfaa2c6c1629b0e7ab16caa7cbb8e2`
passed the unchanged installed pre-push full gate again (1,431 outer cases/24
summaries, explicit macOS socket skip). Successful push receipt/log:
`/private/tmp/ignatius-push-54056c2-https-retry-20261025.{json,log}`.
The exact clean 437-file source snapshot and 283 reachable Git revisions passed
pinned Gitleaks. Snapshot `/private/tmp/ignatius-retained-publish-scan-avrkk8wv/`
is byte-identical to clean `54056c2`, tree digest
`df4280f1b71633407bdb106cc2706fdfd59ce24c810b8a5e9de6d2b08bc64717`.
Logs: `/private/tmp/ignatius-gitleaks-retained-{source,history}-20261025.log`.
Fresh PR CI `40209111816`, CodeQL `40209111702` and manual five-major CI
`40209244285` completed for that exact head. All 35 check runs are terminal:
33 success and two expected schedule-only fixture-drift skips. All platform,
Rust1.90, lints/docs/dependencies/secrets/workflow/drift jobs and three CodeQL
analyses passed. Built-in CodeQL resolved its temporary missing-Rust neutral
state to success with no new alerts. No failure/cancellation/pending check remains.

All PostgreSQL 14-18 jobs ran 236 selected invocations per major, 1,180 total.
These include 60 actual retained-result executions: all six new cases once on
plain and once on verify-full for every major. Session 16 per route comprises
two live parameter-gate and 14 pure cases, including seven ingestion helpers;
notice sidecar eight comprises six live lifecycle and two local cases. All ten
notice integration cases per route passed. These counts overlap as stated;
1,180 is not a live-test count. Supplied fixture environment plus positive
major/orders/transport assertions prove admission. The complete logs contain
no semantic-skip line. They do not print server patch versions: those remain
immutable fixture-image provenance. The actual Linux socket contract ran once
without skip and asserted socket transport plus SELECT1 result.

Each PG/socket job used active Rust1.97.1, compiler
`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, x86_64 Linux, debug/unoptimized.
Root verified all 11 compact receipt hashes and the independent source audit at
`/private/tmp/ignatius-pr32-54056c2-five-major-20261025-Vo7bDf/`, manifest
`b66c1e33cab5e8e040795fb927abc7cbb3128b42a3e643d6a1ed8f0a7cbf4c27`.
Complete root check listing:
`/private/tmp/ignatius-pr32-54056c2-all-checks-20261025.json`.
PR checkout `dceeaeda771098248d55c2094193f7d6c691e696` and head `54056c2`
have identical tree `6f32257cb8fbce7c36c6fe0ca50829c9639d878e`; manual CI
checks out exact head. This is source-tree equivalence, not packaged-candidate
acceptance. Platform DB early returns, schedule-only upstream comparison,
native hand-use and process-memory ceilings remain separate. W10-013 closes
for its scoped contract; no main merge or release follows from this evidence.

## The full command

```bash
cargo xtask verify
```

It runs every gate below in the order that fails fastest, continues past a
failure so one run surfaces every problem, prints a summary, and exits non-zero
if any gate failed. Underneath it is exactly this:

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --all-targets --all-features --no-fail-fast
cargo test --locked --workspace --doc --all-features
```

Run them in that order: the fast gates fail fastest. When the disposable plain
and TLS services are ready, xtask injects both test URIs into the complete
suite. Without them the database-backed cases skip and say so. The Unix-socket
case separately requires `IGNATIUS_TEST_PG_SOCKET_URI`. **A skip is not a
pass.** The verifier lists each missing live boundary after the ordinary gate
results.

The workspace gate uses `--no-fail-fast` so a failing test binary does not hide
later integration binaries. If that gate fails, fixture availability is labelled
as information; it is not reported as a database/socket test pass.

## Required gates before merging

| Gate | Command | Blocking |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | Yes |
| Lints | locked workspace clippy across all targets and features | Yes |
| API documentation | locked workspace `cargo doc`, warnings denied | Yes |
| Complete auto-discovered test suite | locked workspace `cargo test --all-targets --all-features` | Yes |
| Documentation examples | locked workspace `cargo test --doc --all-features` | Yes |
| CLI, PostgreSQL, prompt and cloud-token integration | live plain and TLS services in CI | Yes |
| Unix socket integration | focused live socket fixture on Linux in CI | Yes |
| Compilation on every primary OS | locked workspace build on Linux, macOS and Windows | Yes |
| Minimum supported Rust | locked workspace check with Rust 1.90 | Yes |
| Dependency advisories and licences | `cargo deny check` in CI | Yes |
| Secret scan | gitleaks in CI | Yes |
| Release evidence scope | allowlist validation and scoped gitleaks scan in CI | Yes |
| Code scanning | CodeQL advanced setup in CI, test sources excluded | Yes |
| Documentation and release notes updated | Review | Yes, when behaviour changes |

## Complete interactive review admission - 2026-10-25

Feature 018 T037-T040 now implements the pinned shared review contract for
copy, generated UPDATE, production run, EXPLAIN ANALYZE and named parameters.
Falling short of the current final page, 40x8 minimum or two body rows produces
no sending effect. Both acceptance aliases are gated before candidate/input/job
mutation. Local navigation, fixed input/rail, active remaps, resize reset,
bounded masks and full exact UPDATE wrapping use the same pure geometry.

Five coupled narrow-review cases failed first. Lead review additionally
reproduced extraction loss of Analyze/copy-target redaction and a real late
metadata response covering a newer copy owner. Fixed regressions preserve the
single redactor and discard only the older lookup, without automatic retry.
Actual TestBackend first-frame, fresh-size and stale-resize cases failed first;
a valid enlargement-before-paint case emitted Explain before the repair.
Runtime now supplies `InputAtSize` facts and reducer policy suppresses that
first acceptance after a newly observed resize, retaining candidate/input.

Final focused scope: app 328, UI 241, interactive runtime 27 (596 cases, zero
skips), Clippy and formatting. Three actual 40x8 typed-input/rail reds passed
after compact standard navigation labels, without truncating custom keys.
Logs and exact scoped source hashes:
`/private/tmp/ignatius-w04-minimum-evidence-20261025.txt`.

The lead inspected representative first/end/middle styled frames from 152
actual synthetic Ratatui buffers: nine scenarios, both densities, dark/light/
high-contrast truecolour and ASCII/no-colour. Inspection included all five
review families, 40x8 copy/parameter/typed-PROD, too-small refusal and complete
UPDATE tail. Buffers, symbols/styles, font/DIM/Reset assumptions and linked-
library provenance: `/private/tmp/ignatius-w04-styled-reviewed-lay5bval/`.
Source-input SHA-256:
`4c64afe28ae5549e167b3c351f1544689de775f1448dd7f7a87b5b0617cd2995`;
linked non-test library SHA-256:
`6d36e4423f2f35d8fb0de14ba30100272c7b07a749f912153763e29e986abb58`.
No effects were dispatched for these synthetic captures. They establish drawn
cells and reachable disclosure, not native appearance or proof of reading.

The combined modified `24db9c5` source, including W07-062 below, passed the
complete locked verifier on macOS 27.0.1 arm64/Homebrew Rust 1.99.1: 1,270
harness cases/996 library across 22 summaries, formatting, warnings-denied
lints/API docs and PostgreSQL 18.6 plain/TLS. Log:
`/private/tmp/ignatius-verify-2026-10-25-review-and-panic.log`.
The Unix-socket case remains a semantic skip, and the inert W07 parent helper
is not terminal evidence. Later code needs fresh acceptance. Current-source
Linux, pinned/MSRV, Windows, hosted, native hand use and required different-
family review remain separate. No real clipboard or production write was used.

After the final keymap/recovery prose edits, all 15 documentation-against-build
contracts passed again. Log:
`/private/tmp/ignatius-docs-after-review-2026-10-25.log`.

## Panic and error acquired-mode restoration - 2026-10-25

W07-062's original mouse-enabled panic failed before prior-hook output: the
hook restored default options, so mouse disable sequences arrived only later
through Drop. The two-file repair retains actual acquired options in an atomic
mask and attempts every output-mode cleanup before raw-mode cleanup and the
previous hook. Panic wording distinguishes successful restoration, failed
attempt and no held terminal, preserving the prior hook and original payload.
It no longer claims no database write or secret-free arbitrary panic details.

The isolated modified `24db9c5` clone passed 13 ordinary units, two PTY drivers
and one inert parent helper, plus warnings-denied Clippy and formatting. The
drivers exercised 16 actual owned macOS PTY children: all eight mouse/paste/
alternate-screen combinations on error return and caught panic through the
actual TerminalGuard/hook. The previous hook observed restored canonical,
echo, signal and raw states before printing details; sequence ordering and
payload preservation were checked. Children clear inherited environment and
use task-owned config/data/absent-passfile routes without overriding HOME.

Evidence and red/green logs: `/private/tmp/ignatius-w07-062-w2iPKH/`.
Final two-file patch SHA-256:
`cf3570a569025794995da01bffacfa9b1102036db75f14e5abf6350ad350b763`.
The lead reviewed/applied it; focused integration and the combined full gate
above passed. This is actual Unix guard/hook restoration evidence, not a full
uncaught application crash or native visual acceptance. Forced output/raw-mode
failures were unit-injected, not induced on an OS device. Windows, Linux and
latest-source pinned/MSRV proof remain separate at this checkpoint.

## Bounded resource baseline - 2026-10-25

W10-007 used clean `24db9c554254cce963688b759e86039a30118fd6`, an optimized
release binary built with actual Homebrew Rust/Cargo 1.99.1, macOS 27.0.1 arm64
on Apple M2 Max/32 GiB, and the existing synthetic PostgreSQL 18.6 plain fixture.
Binary SHA-256: `ad458d843f10af073565a42d62f51e361661073cdb473e94140579f50b8f6773`;
build identity: `w10-007-24db9c5-local`. Lockfile SHA-256:
`f636a58ae72d8bf665829ee7ab14e482f17dd0cd58e889c2beae871ed3ac1c3f`.

Each of nine bounded workloads ran three times in a fresh measurement helper
with one directly owned CLI child. Darwin reports peak child RSS in bytes;
the helper verified its initial child high-water mark was zero. Configuration,
data and an absent passfile were task-local; synthetic credentials were injected
only through child environment. Bounded concurrent pipe readers drained and
hashed output. No real credentials, persistent writes or lifecycle actions were
used. Twenty-eight authenticated version/`orders = 3` checks stayed unchanged.

| Workload | Median peak RSS MiB [range] | Median wall ms [range] | Exit |
| --- | --- | --- | --- |
| Small row | 8.453 [8.438-8.453] | 18.62 [16.91-19.82] | 0 |
| Retained 2 MiB value | 12.594 [12.562-12.625] | 40.17 [36.19-44.08] | 7 |
| Retained 8 MiB value | 18.578 [18.500-18.578] | 91.97 [89.58-104.03] | 7 |
| 8 MiB value after one-row cap | 18.562 [18.547-18.578] | 104.24 [100.22-106.34] | 0 |
| One 2 MiB notice | 12.688 [12.656-12.703] | 41.64 [38.04-47.28] | 0 |
| 256 notices of 256 characters | 8.672 [8.656-8.688] | 23.09 [20.54-23.50] | 0 |
| 4,096 notices of 256 characters | 10.031 [10.016-10.078] | 30.28 [28.36-30.67] | 0 |
| 16,384 notices of 256 characters | 14.438 [14.375-14.453] | 161.99 [49.31-253.57] | 0 |
| Complete streamed 8 MiB value | 42.641 [42.609-42.672] | 109.01 [101.73-112.81] | 0 |

The retained-value refusals produced no stdout and correctly named the existing
cell limit. The after-cap case still received the oversized second value; its
one retained row produced six CSV bytes and no stderr. This exposed the separate
W10-009 silent truncation defect. Each complete export contained one row and
8,388,609 bytes, SHA-256
`6ebd215f5992adfd7ea049e317a4c23bb6fff7653ca01e2f8f3e174d32f764e9`.
Only those three owned disposable payloads were removed after verification.

No 30-second deadline, 512 MiB completed-child RSS stop threshold or 256 MiB
task-output threshold was reached. These were experiment safeguards, not
enforced product ceilings. Background host/root-build load was uncontrolled;
three samples support medians/ranges, not p95. TLS overhead, a real slow sink,
native Windows/Linux and the later integrated source were not measured.

Retained evidence: `/private/tmp/ignatius-w10-007-AFKZrb/evidence/`, with helpers
`measure_one.py` and `run_experiment.py` in its parent directory. The lead read
the workload/measurement code, checked all six artifact digests and confirmed
zero remaining export/partial payloads. Manifest SHA-256:
`9ef3aea0a2888b9080022fc55a66af7b5cb818d960c21857678b4222a0645beb`.
The results motivate distinct notice, retained-result and incoming-allocation
contracts in W10-008; they do not close that requirement.

## W10-011 real-writer pacing evidence - 2026-10-25

At exact clean `d8a3ac34ea5a69fb957b04f94c5c90ffa27a1cef`, an optimized
external helper used actual Session.stream, StreamWriter, PacedWrite over
Export.writer and normal flush/sync/rename. Its persistent state paced accepted
bytes at 1 MiB/s with zero initial credit and at most 8 KiB delegated per write.
Controls used identical chunking/instrumentation. No product source changed.

Exactly 12 fresh measured export children (three per case) and 24 before/after
health checks completed once, all exit zero, no stderr/skips/retries/stops.
All health checks reported PostgreSQL 18.6/version_number 180006/orders=3.
Actual environment: macOS 27.0.1 arm64, Mac14,13/M2 Max/32 GiB, Homebrew
Rust/Cargo 1.99.1, own locked/offline release library/binary/target/registry.
This host measurement did not use the repository's pinned Rust 1.97.1.

| Case | Wall seconds median (min-max) | Peak RSS MiB median (min-max) |
| --- | --- | --- |
| Small rows, unpaced | 0.090 (0.077-0.501) | 6.812 (6.797-6.859) |
| Small rows, paced | 8.031 (8.025-8.033) | 6.797 (6.766-6.859) |
| One large value, unpaced | 0.114 (0.111-0.117) | 40.828 (40.812-40.828) |
| One large value, paced | 8.109 (8.109-8.113) | 40.812 (40.781-40.828) |

The first unpaced small sample took 0.501 s; it remains in the range rather
than being discarded. Cold-start/order/concurrent host and fixture effects
were not isolated. Fresh one-child observers used Darwin ru_maxrss bytes;
these are medians/ranges for three observations, not p95 or enforced ceilings.

Every small CSV matched exactly 8,388,608 bytes/8,192 complete rows and SHA-256
`0482a1492daed9d2f7ee7921f5742c5b38db026fb507bc078f8edcb7a245eaa6`.
Every large CSV matched exactly 8,388,609 bytes/one complete row and SHA-256
`6ebd215f5992adfd7ea049e317a4c23bb6fff7653ca01e2f8f3e174d32f764e9`.
Session, encoder, Export and finished-file counters agreed, as did accepted
bytes and closed-file sizes. The controller checked every synthetic byte,
then removed only those 12 verified owned payloads/100,663,302 bytes. Outputs
and partials were empty at handoff; source/build/safe evidence remain retained.

The lead inspected all four helper/controller/build sources and verified 17
frozen inspection inputs and 193 link inputs before authorizing execution.
Afterwards the lead read the report, verified all 38 final artifact hashes,
recomputed all four RSS/wall medians/ranges from 36 raw records, independently
regenerated both expected digests and checked all row/byte/deadline metadata
and empty outputs. Removed-file byte checks are the retained controller
assertions, not a second lead read of absent payloads.

Evidence: `/private/tmp/ignatius-w10-011-d8a3ac3-JVcSVr/`.
Preparation manifest SHA-256:
`0238a31d34a18f7ecad40356293e6adbfd96840c1ae85b20b7946bcbc52f0f67`;
report SHA-256: `e6f760dacb2c42e12fa6b67ca74eef9be2f43523f9b0665f4f4711f252930b51`;
summary SHA-256: `01c99b46f42131fa2b0935fd9cfe44234594028b2ce1bba4e22310918bc85964`.
Production optimized binary SHA-256:
`44913b4cb2924b8236b52ab7017c30f6c9a58a80b65b2d95affce86e483f3420`;
standalone helper SHA-256:
`9c43f1b56914ed266708c7c1701d0cf85bb9163be17c51b1ac14c19095a29819`.
Build identity `w10-011-d8a3ac3-jvcsvr-local`; source/rlib/lock/toolchain and
all raw timing/stall/finish fields are retained. SDK/linker temporary-directory
fallback warnings are recorded; no source workaround was introduced.

This measures application Write acceptance above BufWriter, which may flush
and catch up in bursts. All observed inner writes were full; short/Interrupted
handling was source-reviewed, not fault-injected. It establishes complete
streaming consumer backpressure and the distinct large-value allocation cost,
not instantaneous/disk bandwidth, top-level CLI responsiveness/cancellation,
TLS, hostile-frame bounds, native hand use, cross-platform or whole-process
memory acceptance. No application full-suite repeat was required for this
measurement-only work. W10-008's remaining resource contracts stay open.

## Buffered truncation disclosure - 2026-10-25

W10-009's clean-base `24db9c5` clone first reproduced nine missing-disclosure
failures while five compatibility controls passed. Its three-file patch emits
the pinned value-free stderr line only after successful buffered writing for
CSV, TSV, NDJSON, Markdown and INSERT. Counts describe received rows and
result-set ordinals; no SQL is replayed. Table/JSON stay in-band, error-prefix
status is preserved and successful file streaming remains complete at cap zero.
Streaming help now states the individual-value/row/frame memory boundary.

Private focused results: live 14, CLI 33, output 25, documentation parity 15
(87 cases, zero skips), Clippy, formatting and whitespace checks. Environment:
macOS arm64/Homebrew Rust 1.99.1, PostgreSQL 18.6 read-only/plain fixture,
isolated state/absent passfile, no HOME override/history. Evidence:
`/private/tmp/ignatius-w10-009-evidence-20261025.txt`; patch SHA-256:
`27718e2ee946e7719d86ac9535a5993ea3f3d2e12405fdf1d1e91146bcc634a8`.

After primary review/application to modified `871db73`, root passed 33 CLI
units and all 14 actual CLI contracts on each plain and `verify-full` TLS
PostgreSQL 18.6 route (61 focused passes, zero skips). Logs:
`/private/tmp/ignatius-w10-009-root-{cli-unit,plain,verified-tls}-20261025.log`.
No persistent schema/data or fixture lifecycle was changed by those focused
checks. The combined final full verifier below passed after W10-010/help
integration; the earlier 1,270-case review/panic gate does not cover this CLI
change. Latest-source Linux/MSRV/Windows and hosted acceptance remain separate.

## Final local help/export integration - 2026-10-25

The combined modified `dd69f66` source passed the complete locked verifier:
1,315 outer harness cases/1,027 library across 22 summaries, formatting,
warnings-denied lints/API documentation and PostgreSQL 18.6 plain/TLS. Actual
environment: macOS 27.0.1 arm64/Homebrew Rust 1.99.1. Log:
`/private/tmp/ignatius-verify-2026-10-25-help-and-export-final.log`.
The macOS Unix-socket case remains a semantic skip; the inert terminal parent
helper is not terminal evidence. The previous 1,310-case run passed before
five additional peer-review regressions/repairs and is retained separately;
it does not close those later findings.

W10-010 first reproduced six progress/flush failures while four controls passed,
with 24 before/mid/terminator write faults across first/later rows and four
formats. Its private output 30/export 11/runtime 29 plus three SELECT-only live
contracts passed (73 cases, zero skips), Clippy and formatting. The primary and
a native peer checked the complete-row/original-error path, typed flush state,
unchanged successful bytes and preservation of all 33 moved original tests.
Best-effort BufWriter Drop salvage remains; its unreported result never
upgrades an explicitly failed flush. Evidence and exact patch SHA-256:
`/private/tmp/ignatius-w10-010-evidence-20261025.txt`. These injected failures
do not establish native ENOSPC, durability failure or another OS's behavior.

Feature 015 T042 first reproduced four hidden-input/unreachable-content
failures and the invisible Space label. Full library integration caught two
old chord-owner regressions; their existing tests were kept and the code fixed.
Native peer review then reproduced real palette Insert choices with/without an
inspector, a swallowed selected pane command, and valid long-remap 40x8 loss of
the binding list. Twenty selected help cases passed after the fixes. The
unchosen palette/toggle control remained green; direct editor choices reveal
their target, ordinary commands retain existing guards, and narrow navigation
uses complete actual page-key labels. Reds/greens:
`/private/tmp/ignatius-help-{red-behavioral,palette-red,remap-red,peer-fixes-green}-20261025.log`.

The lead inspected representative frames from 104 actual styled Ratatui buffers
across both densities, three truecolour themes and ASCII/no-colour. Inspection
included late content, long/Unicode key rows, 40x8 default/long controls,
inspector/palette return and 20x4 close recovery. Capture checks exclude the
underlying footer when locating remapped key rows. All eight raw end/return-
from-palette pairs match. Evidence:
`/private/tmp/ignatius-help-styled-_hofm3y7/`; source-input SHA-256:
`ddc57e904571a0d47a801b7e2965d44bb5b49b6ebeb7ae2f8029c79fae9c00db`;
fresh Cargo-selected non-test library SHA-256:
`24574a885e6e5a86135b7751ebe39cb83c0d6d60c486818934b25b7a00d87d69`.
Reference fonts, Unicode fallback, DIM/Reset assumptions, raw symbols/styles
and linked-library paths are retained. No effect was dispatched. The actual
last-chord buffer assertion was strengthened after the full run and passed as
one focused check, with production inputs unchanged:
`/private/tmp/ignatius-help-actual-last-chord-20261025.log`.

These are local focused/full/live/rendered-cell results. Frozen Linux `871db73`
below excludes the later CLI/export/help implementation. Current-source
Linux/pinned/MSRV/Windows, hosted and native visual/hand-use acceptance remain
separate. Different-family review was subsequently skipped by explicit owner
instruction, not completed. Nothing was pushed or released.

## Frozen review/panic Linux acceptance - 2026-10-25

Clean `871db73673ebf4119cbc6bfcf3d39eeb2dfbadf3` (application/Cargo inputs equal
`179e0f3`) passed on Debian 13/Linux arm64 with actual pinned Rust 1.97.1:
996 full-library cases, one real PostgreSQL Unix-socket contract and six
external terminal-restoration contracts, all zero skips. The 1,003 outer cases
include two guard/hook drivers with 16 nested PTY children; those children are
not added again to the total. Three separate application PTYs prove normal
quit, while three external harness helpers cover isolation/deadline/capture.
Actual Rust 1.90.0 locked/offline workspace/all-targets/all-features compilation
also passed. The inert library parent helper is not terminal evidence.

The production debug binary confirmed clean exact source, build
`linux-871db73-xkko8x-197`, `aarch64-unknown-linux-gnu` and Rust 1.97.1. Its
SHA-256 matched after both runtime phases:
`a9093c7ed3eca365eb11cef31911f977541e060eb99f8df1ba4854f5434b625c`.
The task-only PostgreSQL 18.6 role/database queries kept `orders = 3` unchanged.
This fixture did not repeat compose's explicit SCRAM initdb flags, so these
results do not independently prove its password-challenge method.

Evidence/source/cleanup: `/private/tmp/ignatius-linux-871db73-20261025-xkKO8X/`.
All containers used no network/host ports, with read-only frozen source/vendor
mounts and separate Rust homes/targets. An initial SSD vendor-bind create
request stalled before a test/container began; a task-local vendor copy worked
and each actual suite ran once. Only five owned containers, six labelled
volumes, two addon tags and owned vendor/Cargo-home copies were removed.
Approved bases and root/COI IDs were preserved, without global prune/config
changes. Minimum observed internal free space was 16,055,525,376 bytes
(14.9529 GiB), above the six-GiB floor.

The lead read the scope/cleanup records and checked retained artifact digests.
Evidence SHA-256: `07d6f760d0b46557e94e1e2fd4d12d2c927706c4f99b6ad603fb2606f5a11da8`;
cleanup SHA-256: `5415ed87a8dd0e46d00efec1ba9b68bde2c7982834d6f6556c624d56601090e9`.
This frozen source excludes W10-009/010 and later help work. Uncaught fatal
panic/abort, latest-binary steady-state SIGTERM, native visual/hand use, Windows,
hosted and a repeated major/TLS matrix remain separate.

## W07-063 standalone uncaught panic process - 2026-10-25

Eight standalone Rust main children ran outside libtest and without
catch_unwind, against the actual production TerminalGuard/panic hook. Each
acquired one of all eight mouse/paste/screen combinations, observed raw=true,
then panicked with a fixed synthetic payload. All exited 101 with no five-second
deadline or one-MiB capture stop. The inherited observing hook saw raw=false
and actual canonical/echo/signal flags before delegating to the default hook;
reverse cleanup sequences preceded that output and preserved the payload.

Every immediate post-exit full stty comparison differed only in Darwin PENDIN,
verified against the runtime's actual termios.PENDIN constant. Normal synthetic
line input/read on the same PTY cleared it, after which every full stty snapshot
matched exactly. No post-acquisition stty reset was used. All PTYs/children
closed; no database, real profile, native app or OS permission was involved.

Evidence/helper/manifest/transcripts: `/private/tmp/ignatius-w07-063-u329k7er/`.
Manifest SHA-256: `883b1467c09cdac27e1e7f95659c96d1254a3f1c62e96c246838985e94bfba4b`;
results SHA-256: `0003dc8da237061f0aa6fd4ca41db9d5bb9fe2d83d3f81b249408cd0b7a456c8`.
Production-library rlib SHA-256:
`2c3209ed13e67879d7e951311655752a471f25a09e89eac85c910a258d9a6633`.
The lead inspected helper/driver before running and checked all transcript
hashes and PENDIN-only deltas. Build base `74eb1553c41763fb379fdcef4275a34ebddd0bc0`
had only recorded root-owned foundation plan/tasks changes; application inputs
equal clean `d8a3ac3`. Actual macOS 27.0.1 arm64/Homebrew Rust 1.99.1, debug
non-test library and helper. No production source changed; the 1,315-case
application verifier remains its separately recorded run, not repeated here.
This closes only W07-063's library-main process observation, not a full injected
application crash, panic=abort, forced restore I/O failure, native appearance/
hand use, Linux or Windows acceptance.

## Local security maintenance - 2026-10-25

The first clean `e70cddf` repeat found one confirmed scanner false positive:
`src/ui/review/tests.rs:351` assigns the keyboard label `Ctrl+Shift+F12` to
`binding.key`, triggering `generic-api-key`. It is no credential. Redacted
source/history logs are retained as
`/private/tmp/ignatius-gitleaks-e70cddf-*-policy-2026-10-25.log`.
The narrow policy repair targets only that rule, exact test-file suffix and
exact full label, with AND matching. Existing global fixture allowances are
unchanged; their old single table becomes the equivalent array table.

Actual Gitleaks 8.30.0 controls detected four synthetic findings under the old
policy and exactly three under the new policy: a changed key label and a
synthetic API key in the same file, plus the original label outside that file,
still fail. Only the intended original test literal is excluded. Logs:
`/private/tmp/ignatius-gitleaks-keyboard-policy-{old,new}-2026-10-25.log`.
Configuration behavior was checked on 25 October 2026 against the
[pinned vendor parser](https://github.com/gitleaks/gitleaks/blob/v8.30.0/config/config.go).
This makes no change to product output, scan rules or other test-file coverage;
fresh complete scans follow the saved policy checkpoint.

Fresh complete no-network/read-only Gitleaks 8.30.0 scans at clean
`179e0f32808fafbf46ddaa7151e4e1495c7a1a3d` passed with no leaks: all 261
commits/10.99 MB and the immutable 420-file tracked-source snapshot/5.06 MB.
Snapshot: `/private/tmp/ignatius-source-179e0f3-k9di6les/`; manifest tree SHA-256:
`dc0f50cef225613cf990f0a9196678b6518befa20f25918674ba293a6617dc0d`.
Logs: `/private/tmp/ignatius-gitleaks-179e0f3-{history,source}-2026-10-25.log`.
Later W10-009/010 changes are outside this frozen source boundary.

Fresh complete no-network/read-only Gitleaks 8.30.0 scans at clean
`d8a3ac34ea5a69fb957b04f94c5c90ffa27a1cef` found no leaks: all 266 saved
commits/11.11 MB and an immutable 424-file tracked-source snapshot/5.14 MB
scanned. This includes W10-009/010 and help. Snapshot:
`/private/tmp/ignatius-source-d8a3ac3-d8g0_s4q/`; manifest tree SHA-256:
`284fb26412b54b2c568b1c01626a21d568fc6971fbe050fe088e29428062da4d`.
The verified tracked input totals 5,147,682 bytes; scanner byte counts exclude
some metadata/ignored content and are recorded separately. Logs:
`/private/tmp/ignatius-gitleaks-d8a3ac3-{history,source}-2026-10-25.log`.
Both task-owned scanner containers completed and were removed. Subsequent
planning records do not expand this frozen source boundary.

Current primary advisories and maintainer releases were checked on 25 October
2026. Only these five lockfile package versions/checksums changed; existing
dependency edges, including Windows-specific resolutions, were preserved.

| Package | Old -> new | Primary advisory evidence |
| --- | --- | --- |
| rustls | 0.23.45 -> 0.23.46 | [0346](https://rustsec.org/advisories/RUSTSEC-2026-0346), [0347](https://rustsec.org/advisories/RUSTSEC-2026-0347), [0348](https://rustsec.org/advisories/RUSTSEC-2026-0348), [0349](https://rustsec.org/advisories/RUSTSEC-2026-0349) |
| rustls-webpki | 0.103.14 -> 0.103.16 | [0382](https://rustsec.org/advisories/RUSTSEC-2026-0382) |
| Tokio | 1.53.1 -> 1.53.3 | [0386](https://github.com/RustSec/advisory-db/blob/main/crates/tokio/RUSTSEC-2026-0386.md) |
| ctutils | 0.4.2 -> 0.4.4 | [0385](https://github.com/RustSec/advisory-db/blob/main/crates/ctutils/RUSTSEC-2026-0385.md) |
| webpki-root-certs | 1.0.9 -> 1.0.11 | [0383](https://rustsec.org/advisories/RUSTSEC-2026-0383), [1.0.11 distrust rollup](https://github.com/rustls/webpki-roots/releases/tag/v/1.0.11) |

Direct rustls and Tokio requirements exclude the affected releases. The root
certificate package is a conditional WebAssembly input of the platform
verifier; its presence in the audited lockfile does not establish its use on
native platforms. Declared minimum compilers fit Rust 1.90 (ctutils now requires
1.87); declarations alone do not prove an actual Rust 1.90 build. The xtask
licence now explicitly matches the repository's Apache-2.0 licence.

Cargo-deny 0.20.2 was downloaded to a task-only temporary directory and checked
against the official release asset SHA-256. The all-features, all-workspace,
locked audit passed advisories, licences and sources with zero errors or
warnings, and bans with zero errors and six existing duplicate-version
warnings. Repository policy was unchanged; the temporary policy copy only
redirected the advisory database cache. Final-graph log:
`/private/tmp/ignatius-cargo-deny-2026-10-25-final-graph.log`.

The integrated modified source based on `294b5d4`, including W03, W10-006 and
these dependency patches, passed `cargo --locked xtask verify` on macOS 27.0.1
arm64 using Homebrew Rust/Cargo 1.99.1 and disposable PostgreSQL 18.4 plain/TLS.
It reported 1,175 harness passes across 22 summaries. One socket test returned
early and remains a semantic skip. The repository's pinned compiler is 1.97.1;
this local run does not replace pinned-toolchain, Rust 1.90, Windows, hosted CI,
different-family review or packaged-candidate evidence. Log:
`/private/tmp/ignatius-verify-2026-10-25-wave2-security.log`.

## Export-interruption harness repair - 2026-10-25

The PostgreSQL 18.6 matrix exposed a harness timing assumption: exporting four
million rows completed before the test's fixed 2.5-second interrupt. The child
correctly reported a completed export with exit zero; the test expected nine.
That failed run is retained in
`/private/tmp/ignatius-matrix-2026-10-25-18-plain-and-transport.log`.

The test now emits at most 1,000 prefix rows, blocks the tail in PostgreSQL,
waits for more than 8 KiB of actual partial-file output with a ten-second
deadline and early-child-exit detection, then sends SIGINT. It checks exit nine,
the incomplete wording, absence of the complete destination and retained data
rows rather than just a header. Its temporary directory cleans itself on
failure. The focused live PostgreSQL 18.6 rerun passed in 0.70 seconds with no
skip against unchanged application source `f99fe75` plus only this harness edit.
Log: `/private/tmp/ignatius-export-interrupt-barrier-2026-10-25.log`.
The subsequent complete five-major recheck passed as recorded below.

## Frozen PostgreSQL and Linux acceptance - 2026-10-25

The reviewed multi-platform digests in `docker/postgres-images.env` were read
from the official registry on 25 October 2026. Official source annotations
identified the versions below on Alpine 3.24 with amd64 and arm64 manifests;
actual local execution then established the arm64 server facts. The local
Compose default is now 18.6; CI still resolves an immutable reviewed digest.

| Major | Actual plain/TLS server version | Combined contracts | Plain session | Verified-TLS parameter/privacy | Verified-TLS session |
| --- | --- | --- | --- | --- | --- |
| 14 | 14.24 | 127 | 7 | 14 | 7 |
| 15 | 15.19 | 127 | 7 | 14 | 7 |
| 16 | 16.15 | 127 | 7 | 14 | 7 |
| 17 | 17.11 | 127 | 7 | 14 | 7 |
| 18 | 18.6 | 127 | 7 | 14 | 7 |

All 775 cases passed with configured live fixtures. Combined contracts are
CLI, password prompt, controlled cloud-token simulator, PostgreSQL transport,
parameter semantics and runtime privacy. They are not live cloud-provider
account evidence or the complete workspace suite. Each fixture was recreated
from its reviewed digest, waited for final TCP readiness, then authenticated to
confirm its version and three synthetic orders before testing.

Source: clean `e5389f763560eae843b0b7f948e679e653043bf0`, an independent local
clone with unchanged application code from `f99fe75` and only the interruption
harness/documentation repair. Its actual binary reports that clean revision,
local build identity, aarch64-apple-darwin and Homebrew Rust 1.99.1. Debug symbols
and incremental compilation were disabled in its isolated test target. The
earlier failed 18.6 timing run is retained rather than relabelled a pass.
Logs and structured per-major records:
`/private/tmp/ignatius-matrix-recheck-2026-10-25.log` and
`/private/tmp/ignatius-matrix-recheck-2026-10-25-results.json`.

Separate Linux evidence used clean frozen
`f99fe759c2ebe942d91d75c84f42e50f9e49526a`, read-only source/vendor mounts and
task-only named Docker volumes. Actual Linux arm64 GNU results:

- Rust/Cargo 1.97.1 on Debian 13.6 passed the production PostgreSQL 18.6 Unix-
  socket contract: one test, zero skips, authenticated SCRAM socket transport
  and `SELECT 1`. The actual binary confirmed its clean SHA and
  `local-linux-socket-20261025-f99fe75` build identity.
- Rust/Cargo 1.90.0 on Debian 13.1 passed the locked, offline complete workspace,
  all-targets/all-features compilation check. This proves the declared minimum
  on aarch64-unknown-linux-gnu, not every target's minimum build.
- Rust/Cargo 1.97.1 passed the three unchanged normal-quit PTY restoration tests
  with zero skips, real util-linux `script` 2.41, GNU `stty` 9.7 and
  `TERM=xterm-256color`. Drawing, restoration sequences and their order were
  exercised; panic/signal exits, raw-mode readback and hand use were not.

Socket, MSRV, terminal logs and checksummed metadata are retained under
`/private/tmp/ignatius-day-20261025-linux-yuxbGZ/`. Task containers, named
volumes, vendor copies and the task MSRV addon image were removed; unrelated
COI container identities were preserved. Frozen Git remained clean. These
local Docker results do not establish hosted acceptance, x86-64/Windows,
Linux full-screen hand use or release readiness. Ongoing UI work is outside
both frozen revisions and needs its own integrated verification.

## Integrated keyboard panes - 2026-10-25

The modified integration tree based on `abe4aed` passed the complete locked
verifier on macOS arm64/Homebrew Rust 1.99.1 with live PostgreSQL 18.6 plain/TLS:
formatting, warnings-denied lints, API docs, complete workspace and doc tests.
It reported 1,206 harness passes across 22 summaries, including 935 library,
four isolated PTY and 14 workflow contracts. The unconfigured macOS socket case
still returned early; the earlier frozen Linux socket proof does not silently
verify this newer UI source. Log:
`/private/tmp/ignatius-verify-2026-10-25-panes.log`.

Focused app 303 and UI 215 checks passed after meaningful failing regressions
for split/paging/zoom behavior, hidden error/recovery state, default-chord modal
ownership, minimum-size diagnostic content and clipped transaction-state
feedback. The lead also checked that all original test names survived the four
inline-to-sidecar moves. Density, dragging, final native visuals and hosted
acceptance remain separate. Native cross-lane review drove the repairs;
different-family review was not performed. The owner subsequently approved
scoped OpenCode Go GLM-5.3 transmission with included allowance only, then chose
`Skip external review` when paid fallback could not be verified. That explicit
owner exception preserves native independent review and local checks.

Rendered review captured 60 actual Ratatui buffers: 15 synthetic scenarios in
dark/light/high-contrast truecolour Unicode and ASCII/no-colour modes. Raw JSON
retains symbols, foreground/background and modifiers; adjusted/restored cells
match exactly in every mode. The lead inspected the theme, error zoom and
minimum-size Unknown-state contact sheets after the overflow correction.
Source-input SHA-256:
`14d5ce27e05fa62677d8bdba42c4ba6bc14c61c88e89429432b998f0d6ed0359`.
Captures, raster assumptions and provenance are retained under
`/private/tmp/ignatius-us7-styled-buffer-_9kmu8u2/`. Rasterized buffers establish
cell layout/styles under their stated font/DIM assumptions. Native Terminal
computer use was blocked by tool policy; no native screenshot or hand-use
acceptance is claimed.

## Density integration - 2026-10-25

Feature 025 T040 and the remaining T038 test implementation now use the single
comfortable/compact spacing table. Focused config 36, app 305, UI 224, CLI one,
documentation 15, warnings-denied Clippy and a fresh library build passed.
Initial red checks cover strict parsing/template/caps, then widget spacing and
overrides. Review also reproduced wrapped-password and typed-production-input
clipping under preferred margins; the final geometry retains their previously
usable height. Focused records and raw log links:
`/private/tmp/ignatius-density-evidence-2026-10-25.md`.

The complete locked verifier passed formatting, lints, API/workspace/doc tests
and PostgreSQL 18.6 plain/TLS: 1,224 harness cases/22 summaries, including 950
library cases and six terminal harness cases. The socket test still returns
early on macOS and remains a semantic skip. Source was modified `b9a92d7` plus
the reviewed density implementation and documentation; pending W04/W07
specifications do not constitute their implementation. Log:
`/private/tmp/ignatius-verify-2026-10-25-density.log`.

A fresh non-test library with SHA-256
`96e31138be4603e71638e473ff15c996bf61c4202b74968d1599df639e69a94c`
produced 88 actual Ratatui buffers: 11 synthetic scenarios, both densities,
dark/light/high-contrast truecolour Unicode and ASCII/no-colour. Captured
symbols/styles and raster/font/DIM assumptions are retained under
`/private/tmp/ignatius-density-styled-buffer-x9t8tv6x/`; source-input digest
`2de4048cac30deb6aec2446ddd3983e2102c002eac2a41af15346d13880567cc`.
The lead inspected grid/notice, password, production, help and completion
contact sheets. Compact adds a row only when the toolbar row is empty; notices
and previously visible protected inputs remain present. Baseline static-help
clipping is tracked in Feature 015 T042; W04 review disclosure is separately
tracked in Feature 018 T037-T040. No native screenshot, hand use, live clipboard,
latest-source Linux/Windows or different-family acceptance is inferred.

## Unix startup-signal registration - 2026-10-25

REL-001 now registers SIGTERM synchronously under the runtime before terminal
acquisition and passes that existing receiver into the watcher. A failure
returns a safe ordinary diagnostic before acquisition. Windows's existing
Ctrl+C route and query cancellation are unchanged. The private event loop's
Unix-only eighth argument has a narrowly documented lint allowance to keep the
prepared receiver's ownership explicit.

The old deferred registration was reproduced deterministically: an owned
self-exec child entered an unpolled current-thread runtime, emitted readiness,
and died by SIGTERM before any watcher could register. The repair buffers that
signal and emits the existing Quit message after polling. An injected
registration failure proves the terminal-acquisition callback is never called.
The isolated clone passed 21 interactive tests, four PTY contracts and Clippy;
red/green/patch metadata remain under
`/private/tmp/ignatius-w07-startup-20261025-9eLtPj/`.

After lead review/integration, the complete locked verifier passed formatting,
warnings-denied lints, API/workspace/doc checks and live PostgreSQL 18.6 plain/
TLS: 1,208 harness cases across 22 summaries, including 937 library cases.
Its macOS socket remains a semantic skip. Source was `4a7717b` plus the reviewed
runtime patch and specification updates. Log:
`/private/tmp/ignatius-verify-2026-10-25-signal-startup.log`.

The integrated actual binary reported modified `4a7717b`, local build identity,
macOS arm64 and Homebrew Rust 1.99.1. Its SHA-256 was
`9487d4fceede4e73050736c72754003d845310b09e9333c31c7c4da96375850b`.
A separate real owned-PTY run confirmed first paint before SIGTERM, product
exit zero, restoration order, canonical input and echo, then exact stty state
after a normal recovery-line read cleared transient Darwin PENDIN. No reset or
forced cleanup was used. Evidence is retained under
`/private/tmp/ignatius-rel001-integrated-20261025-w3FUso/`; earlier clean
`e5389f7` steady-state proof and the failed default-sandbox acquisition attempt
remain separately under `/private/tmp/ignatius-rel001-sigterm-20261025-Bj2c6M/`.
Neither PTY run injects a signal during acquisition; deterministic preparation
tests establish that ordering. Latest-source Linux/MSRV, Windows, panic/error
process paths and native visuals remain separate gates.

At clean saved checkpoint `21511f19ed25a15d078cc29ec4e1ce35eb75c7ca`, the
reviewed Gitleaks 8.30.0 image ran with no network and read-only mounts. History
scan covered all 250 commits/10.77 MB and found no leaks. A separate immutable
417-file source snapshot covered 4.87 MB and also found no leaks. Its verified
tree SHA-256 is
`a1778b16636cb63b89614c11c1bd91b7057e9c037aca12f5883766a13af4ad4e`;
manifest/source are under `/private/tmp/ignatius-source-21511f1-q24p8etp/`.
Logs are `/private/tmp/ignatius-gitleaks-21511f1-history-2026-10-25.log` and
`/private/tmp/ignatius-gitleaks-21511f1-source-2026-10-25.log`. Subsequent density
work is outside those frozen source and history scopes.

Fresh Linux acceptance of clean `21511f19ed25a15d078cc29ec4e1ce35eb75c7ca`
also passed on aarch64-unknown-linux-gnu: app 303, UI 215, interactive 21
(including pre-poll SIGTERM delivery), PostgreSQL 18.6 socket one, and isolated
PTY four, all with zero skips. Actual Rust/Cargo 1.97.1 built the production
binary, which reported the exact clean source and build identity
`local-linux-21511f19-20261025`; SHA-256:
`35fa6466eb8afe9e6a8e3b8b01c43ccd096717233d8eec612ddd5e6d4a8d2954`.
The separate actual Rust 1.90.0 locked/offline workspace/all-targets/all-features
check passed. A task-only Debian procps utility supplied real external `kill`
for the owned-child signal regression; no fake signal tool or skipped test was
used. Source/vendor mounts were read-only and acceptance had no network.
Safe logs and source/tool/image metadata are retained under
`/private/tmp/ignatius-linux-21511-20261025-FWM0SW/`. This updates the earlier
frozen Linux proof through the pane and startup changes; subsequent density,
native Windows, hand use and hosted acceptance remain separate.

## Owned PTY test deadline - 2026-10-25

W07-061 removes the detached 25-second numeric-PID kill thread, which could
outlive normal child completion and signal a reused PID. The harness now polls
the still-owned `Child`, terminates/reaps it at the deadline, and joins its
bounded one-byte input writer. Private temporary regular files capture stdout
and stderr without pipe backpressure or detached reader threads. Capture is
deadline-bounded, not subject to a hard byte quota; this is not a general
process-tree supervisor.

All six checks passed on macOS arm64/Homebrew Rust 1.99.1 using the modified
integration tree: the existing three actual PTY checks, the profile-isolation
child probe, an owned hung-child deadline/reap, and normal completion with
130,000 stdout bytes plus exact stderr. Native peer review found no blocker.
Log: `/private/tmp/ignatius-w07-owned-deadline-2026-10-25.log`. This changes the
test harness only. Linux repetition, panic/error process paths, Windows and
native visual acceptance are separate evidence.

## PTY profile isolation - 2026-10-25

The Unix terminal harness previously overrode only configuration, leaving real
data/history and inherited logging/PostgreSQL routes available. W07-059 now
keeps unique temporary configuration and data directories alive through child
completion, clears inherited logging and all 18 supported PostgreSQL routes,
then supplies only a test-owned absent PGPASSFILE to block personal fallback.
It never changes HOME or process-global environment.

A real synthetic child-process environment probe failed against the old
boundary, then passed with the three existing actual macOS PTY tests (four
total, zero skips). The probe never ran the product with hostile inherited
settings or inspected a real profile. Homebrew Rust 1.99.1 used the modified
integration working tree and existing SSD cache. Red/green logs are retained
under `/private/tmp/ignatius-w07-059-20261025-emhifp/`. Source formatting and
whitespace checks passed; the full integrated repeat follows the pane slice.
This proves isolation plus existing normal-quit/drawing/order behavior, not
panic, signal, raw-mode readback, Windows or native visual use.

## Live database CI coverage repair - 2026-10-25

The PostgreSQL matrix now explicitly runs the parameter-semantics and runtime-
privacy contracts with live plain fixtures, repeats them with verified TLS, and
checks session-sidecar guards over both transports. The ordinary native-OS job
still auto-discovers all targets, but its missing database cannot substitute
for this live evidence. A new wiring assertion first failed for the omitted
parameter-semantics target before the CI change.

Socket startup now probes final-server TCP readiness. The official PostgreSQL
entrypoint's temporary initialization server accepts only sockets, so its
availability was insufficient. Two behavioral contracts execute the actual CI
shell with isolated fake Docker/delay tools: ready on the third TCP probe, and
failure after all 60 probes with diagnostics. Both reject the isolated old
socket-probe body as a negative control.

The scheduled drift job checks every supported major before failing, including
resolver/inspection errors and malformed registry output. Its five behavioral
contracts execute the actual shell and cover simultaneous early/late drift;
the initial implementation stopped at the first failure. Dependency auditing
is now a separately named step after tool installation and includes the locked,
all-features complete workspace, preserving the existing policy.

All 14 focused workflow contracts passed with zero skips against the modified
CI/map. Actionlint also passed using the workflow's pinned image and a read-only,
no-network source mount. Logs:
`/private/tmp/ignatius-workflow-final-ci-2026-10-25.log` and
`/private/tmp/ignatius-actionlint-2026-10-25-wave2.log`. These are local checks;
the updated workflow has not been pushed or executed by GitHub Actions.

## CI execution contract prepared on 2026-09-02

The hardened workflow makes failures complete and actionable: stale branch runs
are canceled, every job has an explicit timeout, checkout credentials are
removed before repository code runs, and Cargo commands that consume
dependencies use the committed lockfile. Dependency auditing, repository secret
scanning and release-evidence scanning are independent blocking jobs.

Pull requests run PostgreSQL coverage on the oldest and newest supported majors.
Pushes to `main`, manual runs and the weekly scheduled run cover every supported
major from 14 through 18. A separate PostgreSQL 18 fixture exposes a real Unix
socket to the Linux runner. The cross-platform suite uses Cargo auto-discovery,
so a newly added contract runs by default.

**Hosted readback on 2026-09-04:**
[main CI run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318)
completed successfully at `52872acd7c0e0e38c6b810723ccda5eee99a28bf`.
The executed jobs include PostgreSQL 14-18 plain/TLS contracts, PostgreSQL 18
Unix socket, native OS suites, Rust 1.90, workflow semantics, dependency/licence
checks, source/evidence scans and generated-output checks. The scheduled-only
fixture-drift job was skipped and is not included in those passes. This is
historical main-source CI evidence, not verification of the newer local
feature chain or of the separate release preflight and candidate artifacts.

## Code scanning contract added on 2026-09-11

CodeQL uses advanced setup rather than GitHub's default setup, because default
setup cannot be configured for a user-owned repository and the analysis must
keep test sources out. `.github/codeql/codeql-config.yml` ignores `tests/**` and
`**/tests.rs`: Rust has no inline alert suppression yet (github/codeql#21637),
and the tests deliberately format password-like values to prove they never
reach output, which the cleartext-logging query reports as high severity.
Production sources keep the full default query set. Unit test modules live in a
`tests.rs` file beside their module, never inline in a production file.

## Local CI-hardening evidence recorded on 2026-09-02

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Workflow syntax and policy | Actionlint and independent YAML parsing passed for all workflow files and the local fixture action. Seven workflow contracts and 80 release contracts passed. | Isolated local integration branch only. |
| Security controls | Cargo-deny 0.20.2, digest-pinned Gitleaks source/history scans and the Rust 1.90 locked check passed. | Local containers and tools; no hosted claim. |
| Full verifier | Formatting, lints, documentation, workspace tests and PostgreSQL 18.4 plain/TLS integration passed. | Disposable local services; the verifier's Unix-socket case explicitly skipped because its URI was not configured. |
| Unix-socket focused attempt | Not counted as a pass. With the URI supplied, the macOS Docker bind mount rejected PostgreSQL's socket-permission change before the server could start. | The CI job is Linux-only and must be verified on an Ubuntu runner; no local socket success is claimed. |
| Release boundary | The candidate remains non-publishing and fail-closed. No tag, signature, provenance, release or distribution was created. | Local implementation evidence only. |

## Evidence recorded on 2026-08-16

macOS 26.6.1 on Apple silicon, rustc 1.97.1 (Homebrew), against both the plain
and TLS disposable services from `postgres:18.4-alpine`, at committed HEAD
`6541610`. CI ran the same suites on Linux against PostgreSQL 14, 16 and 18, and
built and tested on Windows.

**Automated**: formatting clean, `cargo clippy --all-targets -- -D warnings`
clean, 494 library tests, 38 CLI contract tests, 38 PostgreSQL integration tests
and 5 pseudo-terminal tests, all passing. Two of the pty tests are new and prove
the password prompt by using it: the password is typed into a real terminal, the
session opens, and the transcript is read back to confirm the password is not in
it. That run covers the editor and its
undo, syntax colouring, the expanded row view and cell inspector, result
filtering, the statement history and its refusals, object definitions and
dependencies, the object tree's own read-only connection, connection profiles,
the password prompt in both surfaces, and configurable key bindings.

**Run directly rather than through `cargo xtask verify`.** The workspace's xtask
crate was being edited concurrently by another tool at the time, so the five
gates were run against the `ignatius` package individually. The gates are the
same commands the xtask runs; what is missing is only the convenience of one
invocation.

**Terminal lifecycle, captured from a real pseudo-terminal.** The client was run
under `script`, sent Ctrl+Q, and the byte stream captured. In order:

```text
entry:  ESC[?1049h   enter alternate screen
        ESC[?2004h   enable bracketed paste
        ESC[?25l     hide cursor
exit:   ESC[?25h     show cursor
        ESC[?2004l   disable bracketed paste
        ESC[?1049l   leave alternate screen
```

Every mode enabled was disabled, in reverse order, with the alternate screen left
last. No mouse capture sequence appears, because mouse capture is off by default.
The process exited 0 and the shell was usable afterwards.

**Interactive execution.** With the pty sized to 110x30, the client rendered the
header (`[PROD] [read-write]`, the target, `TLS not negotiated`), the editor with
its starter query, an empty-state message in the results pane, and the footer
hints. Sending F5 produced `Completed`, `1 row`, and the value `ignatius_demo` in
the grid. Ctrl+Q exited cleanly.

**Live CLI behaviour.** `connect --check` reported five passing stages and warned
truthfully that the connection was not encrypted. A query against a missing
relation reported SQLSTATE 42P01 with a cause and a next action, exited 7, and
wrote nothing to stdout. A value containing `ESC[2J` was rendered as `\x1B[2J`.
NULL, an empty string and the text `NULL` were each displayed distinctly. The
plain-mode subprocess checks also proved line-oriented input,
production-write confirmation, server-confirmed cancellation, failed-
transaction recovery wording and screen-reader-safe output.

## Release candidate identity and reproducibility evidence

The non-publishing release workflow in `.github/workflows/release.yml` is an
automated packaging contract, not live publication evidence. Each matrix job
builds one explicit target, writes one per-target release record, generates and
verifies `release-manifest.json` and `SHA256SUMS`, then proves that the
readiness gate refuses the incomplete candidate before retaining the bundle as
a run-scoped workflow artifact.

| Fact | Evidence source | State on 2026-09-03 |
| --- | --- | --- |
| Product version | `Cargo.toml`, read directly by the workflow; archive basename and record must match | Hosted run `33689674455` recorded `0.1.0` consistently for all three targets; no release version is published |
| Source revision and state | `cargo xtask release generate` captures `HEAD` and cleanliness, and checks exact tag identity | The latest aggregate identifies `259ac76de6477719b8a3515777f3623b24237dd6`, detached and untagged, and remains blocked |
| Build identity | The workflow injects shared `IGNATIUS_BUILD_IDENTITY=ci/<run>-<attempt>` before all three target builds and keeps target separate in each record | Hosted run `33689674455` used `ci/33689674455-1` for all three targets; this is an identity fact, not provenance evidence |
| Target identity | Workflow matrix, executable header and exact Rust target triple in the archive name, record and sidecars | Run `33689674455` completed Linux `x86_64-unknown-linux-gnu`, macOS `aarch64-apple-darwin` and Windows `x86_64-pc-windows-msvc`; each package, runtime smoke and aggregate verification step passed |
| Archive size and exact bytes | `release generate --artefact-path`, `manifest generate`, `manifest verify` and the 77-case release contract suite | The aggregate artifact from run `33689674455` contains three archives of 2,773,854, 2,806,839 and 3,053,954 bytes; downloaded SHA-256 and combined-manifest checks passed |
| Readiness | `cargo xtask release check` after sidecar verification | The workflow requires a non-zero blocked result; signing, provenance, inventory and owner authorization remain absent |
| Reproducibility | Repository-owned archive helper plus repeat archive comparison from the same source and inputs | Hosted run `33689674455` compared two fixed-identity builds per target; macOS, Windows and Linux byte comparisons passed. Windows builds use `/Brepro` to remove PE linker timestamps. |

The release-contract checks, local rehearsal and hosted runs prove the identity
and integrity rules for the three target-specific candidate bundles. Run
`33689674455` also exercised the shared build identity, compared two exact
archives for each target, ran the target runtime smoke and rollback checks, and
retained and inspected the nine-file aggregate scope. The runtime smoke is
automated hosted evidence, not full-screen terminal hand evidence or a
published-version upgrade. Signing, provenance, dependency inventory and the
remaining hand and server-version evidence remain before any release can be
called ready or published.

## Installation, upgrade and rollback evidence

The release procedure defines how to perform these checks; this table records
the evidence still required for each target. The packaging workflow and a
checksum match are not installation or recovery evidence. Each completed row
must retain the exact candidate record, archive basename and target identity,
the command outcome and the next action for any failure, without retaining
credentials, SQL text or result data.

| Platform and target | Scenario | Required evidence | Current state on 2026-09-03 |
| --- | --- | --- | --- |
| macOS Apple silicon, `aarch64-apple-darwin` | Clean install and first start | Verify the `tar.gz` bytes before extraction, run `version --verbose`, `doctor --json`, `config paths`, `config validate` and a reviewed non-secret `connect --check` | Direct hosted-archive pass on macOS 26.6.1 arm64: installed into a version-specific directory, exact identity and configuration passed, PostgreSQL 18.4 passed over localhost plain transport and TLS 1.3 with `verify-full`, and the full-screen client entered and left a real pseudo-terminal with `Ctrl+Q`. The same candidate then failed the logging privacy audit below, so this row is runtime evidence, not release acceptance. |
| macOS Apple silicon, `aarch64-apple-darwin` | Upgrade and preservation | Install beside the known-good binary, confirm configuration, saved queries, history and logs remain at the paths reported by `config paths`, and record migration output if applicable | Direct source-revision replacement pass: known-good `247d1c7` and hosted `24f1c8e` binaries were installed side by side and selected through one launcher. Configuration, saved query, history and log files were byte-identical after first candidate start. Both binaries report product version `0.1.0`, and the prior build was not a published release, so this is not semantic-version or published-upgrade evidence. |
| macOS Apple silicon, `aarch64-apple-darwin` | Failed upgrade and rollback | Force a documented startup or validation failure, return to the prior binary/configuration pair, validate it and retain the failed state for diagnosis | Direct pass with the same boundary: schema version 999 caused a controlled configuration failure with exit 3 and a repair action. The invalid file was retained, the valid backup and prior launcher were restored, the prior binary reconnected, and the complete configuration/data tree matched its pre-upgrade backup. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Clean install and first start | Verify the `zip` bytes with PowerShell, extract `ignatius.exe` to a new version directory, run the identity and diagnostic commands, and record the PATH outcome | Hosted run `33689674455` extracted and executed the exact candidate. `version --verbose`, `doctor --json`, `config paths`, `config init`, `config validate` and non-interactive refusal passed; PowerShell PATH and full-screen hand evidence remain open. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Upgrade and preservation | Keep the prior version directory, confirm `%APPDATA%\ignatius` and `%LOCALAPPDATA%\ignatius` are preserved, and record any schema migration backup | Hosted side-by-side candidate validation passed with isolated configuration/data state and unchanged preservation digest; this is not a published semantic-version upgrade. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Failed upgrade and rollback | Restore the previous PATH or launcher and matching configuration/data backup without deleting the failed version or state | Hosted controlled invalid-configuration and known-good rollback checks passed; ConPTY and operator hand evidence remain open. |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Clean install and first start | Verify the `tar.gz` bytes before extraction, confirm the GNU libc target boundary, run the identity and diagnostic commands, and record the PATH outcome | Hosted run `33689674455` extracted and executed the exact candidate. `version --verbose`, `doctor --json`, `config paths`, `config init`, `config validate` and non-interactive refusal passed; installed-path and full-screen hand evidence remain open. |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Upgrade and preservation | Keep the prior binary available, confirm XDG or explicit configuration/data paths are preserved, and record any schema migration backup | Hosted side-by-side candidate validation passed with isolated configuration/data state and unchanged preservation digest; this is not a published semantic-version upgrade. |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Failed upgrade and rollback | Restore the prior binary and matching configuration/data backup, validate it, and preserve the failed state for diagnosis | Hosted controlled invalid-configuration and known-good rollback checks passed; operator hand evidence remains open. |

The hosted smoke results close the automated runtime subset for all three
targets, but do not replace platform hand checks, terminal restoration, PATH
review or a published semantic-version upgrade. The release target matrix is
therefore still an evidence-backed rehearsal, not a ready or published release.

## Feature 008 release rehearsal evidence

This table records the local non-publishing rehearsal run on 2026-08-31 on
macOS 26.6.1 (`arm64`). It binds all local results to revision
`f84afda2eb6b332c2b9659ef5b83671fbbdc4f74` and labels the worktree modified.
Hosted CI, platform hand verification and publication remain separate evidence
classes.

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Source and build identity | Pass. The packaged binary reported product version `0.1.0`, the full source revision above, modified source state, build identity `local-rehearsal-2026-08-31`, target `aarch64-apple-darwin` and Homebrew rustc 1.98.0. | Local modified checkout only; not a release tag or CI provenance. |
| Full repository gate | Pass. With disposable PostgreSQL services running, `cargo --locked xtask verify` passed formatting, lints, 513 unit/layout tests, 38 CLI contract tests and 38 PostgreSQL integration tests. Teardown removed both containers, their network and generated data; `cargo --locked xtask db status` then reported `Not running`. | PostgreSQL 18.4 plain/TLS synthetic fixtures; no production server evidence. |
| Focused release contracts | Pass. The final focused run passed 3 archive-helper, 1 native-boundary, 71 release-contract, 7 release-notes and 1 release-schema tests. The repository catalogue also validated as one blocked record with one exact changelog entry. | Local automated contracts only; no hosted runner evidence. |
| macOS archive and sidecars | Pass. The repository helper produced a root-only `ignatius-0.1.0-aarch64-apple-darwin.tar.gz`, 3,010,634 bytes, with SHA-256 `c7ba67b568ea803d9f6f7d8e1de802e25cdf539d1377618c76825d1683aa380d`. Record validation, manifest generation, exact-byte verification and the four-file evidence-scope check passed. | Temporary local Apple silicon rehearsal; no hosted archive or retained release evidence. |
| Deterministic archive helper | Pass. Rebuilding the same `tar.gz` from the same binary and declared inputs produced byte-identical output. | Same local Python and compression runtime; the corrected hosted repeat-build evidence is recorded in the later 2026-09-03 section. |
| Scoped secret scan | Pass. The CI-pinned Gitleaks v8.30.0 container, addressed by digest and run with no container network, scanned the verified four-file upload root and reported no leaks. | Local Docker execution of the new CI command; the later hosted follow-up is recorded below. |
| Extracted first start | Partial pass. The root-only archive extracted cleanly; `version --verbose`, `config paths` and `config validate` passed. `doctor --json` reported seven checks OK, three skipped for terminal, terminal size and connection, and no failures. | Isolated local first start only; no reviewed server connection, upgrade or rollback was performed. |
| Readiness gate | Pass as a blocking test. `cargo --locked xtask release check` exited 1 and named 13 issues, including modified source, unmaterialised and incomplete evidence, absent Windows/Linux artefacts, inventory, signatures, provenance, platform recovery evidence and owner authorization. | Expected blocked result; no readiness or publication claim. |
| Hosted target matrix | Not part of this local run. | The later hosted result is recorded below. |
| Platform installation, upgrade and rollback | Incomplete. | The local macOS first-start subset above does not satisfy macOS upgrade/rollback or any Windows/Linux hand-evidence row. |
| Inventory, signing and provenance | Not materialised or verified. | The provider-neutral contracts exist; provider and retention decisions remain owner gates. |
| Owner authorization | Absent. | No version, revision, target set, destination or retention boundary has been authorized for publication. |
| Publication | Not requested and not performed. | No tag, upload, release or distribution operation is authorized by this package. |

### Hosted follow-up and repair on 2026-09-01 to 2026-09-02

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Shared CI | Pass. Run `33445418179` completed all 10 jobs at committed revision `247d1c7e829aa5b642a5047e56e53d402e386dbb`, including macOS, Windows, Ubuntu, PostgreSQL 14/16/18, release-evidence scope, network-disabled scoped Gitleaks, cargo-deny and generated-artefact drift. | Hosted CI and synthetic evidence-scope result only; not release archive, installation or publication evidence. |
| First hosted archive run | Fail. Run `33561901626` checked out the same exact revision. All three target builds passed; all three jobs then exited in the common packaging step before the archive helper ran or any candidate evidence was uploaded. | No hosted archive, checksum sidecar or retained candidate bundle exists from this run. |
| Failure diagnosis and repair | Pass. Commit `24f1c8e7801ab5f1f6805e54e4e5f4db67ca1949` makes the workflow read `Cargo.toml` directly and name missing-version, missing-binary and symlinked-binary failures. | The repair is committed on `main`, pushed and exercised by the successful hosted rerun below. |
| Focused release contracts after repair | Pass. 3 archive-helper, 1 native-boundary, 72 release-contract, 7 release-note and 1 release-schema tests passed; the six workflow-focused cases include the new version-authority contract. | Local automated contract evidence only. |
| Full repository gate after repair | Pass. `cargo --locked xtask verify` passed formatting, lints, 513 unit/layout tests, 38 CLI contracts and 38 PostgreSQL integration tests against disposable PostgreSQL 18.4 plain/TLS services. Teardown removed the containers, network and data; final status was `Not running`. | Local macOS and synthetic PostgreSQL evidence. The earlier sandboxed verifier run skipped integration and is not counted as the full result. |
| Repair CI | Pass. Run `33563478021` completed all 10 jobs at the repaired revision, including macOS, Windows, Ubuntu, PostgreSQL 14/16/18, release-evidence scope and secret, dependency, licence and generated-artefact gates. | Hosted source and synthetic integration evidence; separate from candidate packaging and hand verification. |
| Second hosted archive run | Pass. Owner-authorized non-publishing run `33563497933` checked out the repaired revision. All three jobs built, packaged, verified the deliberately blocked readiness result and uploaded one exact four-file target bundle with seven-day retention. | Target-specific run artifacts only; no aggregation, signing, provenance, canonical evidence promotion or publication. |
| Downloaded candidate bundles | Pass. Each bundle contained only its archive, record, manifest and `SHA256SUMS`, with no symlinks. All three records validated as `blocked`; all manifests, independent SHA-256 checks and four-file scope checks passed. The archives contained exactly one executable with the declared Mach-O arm64, PE32+ x86-64 or ELF x86-64 GNU/Linux format. | Independent packaging and byte-integrity inspection on arm64 macOS; executable headers are not Windows/Linux runtime evidence. |
| Hosted archive identities | Pass. macOS: 3,016,587 bytes, SHA-256 `ce5c9364615009e5516edc394b41aef2c33e66113a26491996d0f5e7927536f2`; Windows: 3,068,482 bytes, `cc0e30a71527a2ba5298f1e3066d080b60712d5e251aced11a722304ec82d056`; Linux: 3,318,764 bytes, `90512e79e4d14ac310cfddffe79fef8383f748601505f159d27f296dbf6353ef`. | Exact run `33563497933` attempt 1 bytes, retained by GitHub Actions for seven days. |
| Downloaded macOS first start | Partial pass. On arm64 macOS the extracted hosted binary reported version `0.1.0`, exact source revision, clean source, build identity `ci/33563497933-1/aarch64-apple-darwin` and the expected target. Isolated `config paths` and `config validate` passed; `doctor --json` reported seven OK, three skipped and no warnings or failures. | Direct execution of the hosted macOS archive only; no reviewed connection, installed-path, upgrade or rollback evidence. |
| Native macOS installation and recovery | Pass with a version-boundary limitation. The exact hosted binary passed installed-path identity, configuration, plain and verified-TLS PostgreSQL 18.4 checks, full-screen pseudo-terminal start and clean quit. Side-by-side launcher replacement preserved configuration, saved query, history and log bytes. A synthetic schema-999 start failed with exit 3; restoring the backup and prior launcher returned all state to the pre-upgrade bytes and reconnected successfully. | Direct arm64 macOS evidence. Both binaries are version `0.1.0`, the prior build was not published, and this does not cover Windows or Linux. |
| Logging privacy audit | **Fail, release-blocking.** With a fresh isolated profile and synthetic statement, `IGNATIUS_LOG=debug` in the exact hosted macOS binary wrote the full SQL through `tokio_postgres::simple_query`. The log did not contain the synthetic password, but SQL persistence violates the documented no-SQL logging contract. | Exact run `33563497933` macOS artifact. The candidate is rejected even though its packaging, runtime and recovery subsets passed. |
| Local logging remediation | Focused pass. The modified working tree accepts only simple log levels, excludes dependency targets and prefix collisions, redacts each complete internal event and records only `job=1 statement_chars=<n>`. Five logging unit tests and one live CLI contract against PostgreSQL 18.4 passed; newly appended bytes contained no SQL marker, result label, synthetic credential marker or dependency trace. | Local modified build identity `local-t033-log-fix` only. It is not a clean, hosted or retained candidate and does not supersede the failed artifact. |
| Clean local correction archive | Pass. Commit `b2ac0a547957409514a2400202cd0207e8acfa65` reported clean source and reproduced the same 2,764,785-byte arm64 archive twice with SHA-256 `197690aceabecc45ad43dc61ebf9b998bd6c37eddbb511465279b4f3dc55b353`. Record, manifest, exact-byte checksum and four-file scope checks passed. The extracted binary's 76-byte mode-0600 debug log contained `job=1 statement_chars=66`; the SQL sentinel, result label, synthetic credential and `tokio_postgres` target were absent. An invalid target directive created no log path. | Direct local macOS and synthetic PostgreSQL 18.4 evidence. Readiness remained blocked by six named gates. This is not hosted, retained workflow or publication evidence. |
| Full repository gate after logging remediation | Pass twice. The modified remediation tree and clean commit `b2ac0a5` each passed formatting, clippy with warnings denied, 515 unit/layout tests, 39 CLI contracts and 38 PostgreSQL integration tests against the disposable PostgreSQL 18.4 plain/TLS services, with no skips. | Local macOS and synthetic PostgreSQL evidence. Each teardown removed the containers, network and data; final status was `Not running`. |
| Focused release and documentation contracts after logging remediation | Pass. 3 archive-helper, 6 documentation/build, 1 native-boundary, 72 release-contract, 7 release-notes and 1 release-schema tests passed. The catalogue wrapper validated one blocked record against its exact changelog heading. | 90 local focused tests plus the read-only catalogue wrapper; no hosted or platform-runtime claim. |
| Multi-target aggregation implementation | Focused pass. Four aggregation contracts cover canonical three-target output, combined-manifest verification, blocked-state union, create-only output and missing, duplicate, unsupported or identity-mismatched refusals. Seven workflow contracts cover the shared run-attempt identity, exact target matrix, action pins, per-target re-verification, aggregate command and six-file retained scope. The complete focused slice passed 3 archive-helper, 6 documentation/build, 1 native-boundary, 77 release-contract, 7 release-notes and 1 release-schema tests; the catalogue wrapper also passed. | 95 local tests plus the read-only wrapper. Hosted run `33614446587` independently re-verified all three target bundles, aggregated their records and retained the exact six-file scope. |
| Hosted aggregate artifact | Pass. The retained artifact contains exactly three archives, one aggregate record, one combined manifest and one `SHA256SUMS` file, with no symlinks; all files are bound to `fc79120b5229a1724a5a589dee4337acf2daa36b`. | GitHub Actions run `33614446587`, artifact retention through 2026-09-09; downloaded archive checksums and combined manifest verified locally. |
| Full repository gate with aggregation | Pass. `cargo --locked xtask verify` passed formatting, clippy with warnings denied, 515 unit/layout tests, 39 CLI contracts and 38 PostgreSQL integration tests. | Exact committed tip `fc79120b` with disposable PostgreSQL 18.4 plain/TLS services, with no skips. Teardown removed both containers, their network and synthetic data; final status was `Not running`. |
| Readiness and authorization | Correctly blocked. The aggregate readiness check exited 1 with eight issues, including incomplete evidence, detached and untagged source, absent verified signature and provenance, and non-publishable state. The historical `33563497933` logging audit remains a separate release-blocking result; the corrected run has not had an equivalent runtime privacy audit. Owner authorization covered commit, push and this seven-day run-scoped rehearsal. | T034 is complete. T033 remains open; no tag, signing, GitHub Release, package publication or canonical evidence promotion was authorized or performed. |

### Corrected hosted runtime and reproducibility run on 2026-09-03

Owner-authorized, non-publishing workflow run `33689674455` checked out the
exact pushed source revision
`259ac76de6477719b8a3515777f3623b24237dd6`. The workflow completed all archive,
repeat-build, comparison, runtime and aggregate jobs successfully. The run is
retained by GitHub Actions through 2026-09-09 and is bound to build identity
`ci/33689674455-1`.

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Target archive jobs | Pass. macOS, Windows and Linux each built one declared target, generated and re-verified its record, manifest and checksum, and passed the blocked readiness assertion. | Hosted packaging and integrity evidence; no signature, provenance or publication claim. |
| Repeat-build reproducibility | Pass. Two fixed-identity builds for each target produced byte-identical archives; the comparison job passed for macOS, Windows and Linux. Windows uses `/Brepro` to remove PE linker timestamps. | Same source revision and declared CI inputs; not a signed or published release. |
| Runtime install and rollback smoke | Pass. Each exact archive extracted and executed its target binary. Identity, diagnostics, configuration initialisation and validation, non-interactive refusal, side-by-side candidate validation, controlled invalid-configuration exit 3 and known-good rollback validation all passed. | Hosted automated smoke evidence only. It does not replace Windows/Linux full-screen, PATH, ConPTY or hand-terminal checks, and it does not exercise a database query or the debug logging privacy boundary. |
| Aggregate evidence | Pass. The final artifact contains exactly three archives, one aggregate record, one combined manifest, `SHA256SUMS` and three runtime sidecars. The nine-file allowlist, record validation, combined manifest and independent archive checksums passed locally after download. | Run-scoped transport artifact; the record's canonical `evidence_reference.path` is still not materialised. |
| Readiness | Correctly blocked. The downloaded aggregate readiness check named eight issues: incomplete and unmaterialised evidence, detached and untagged source, non-publishable state, and absent verified signature and provenance. | This is expected fail-closed behaviour, not a release approval. |

The aggregate and runtime JSON sidecars record observed or verified evidence
without credentials, SQL text, result data or publication actions. The corrected
candidate therefore closes the hosted packaging, reproducibility and automated
runtime subset, while the remaining hand, privacy, inventory, signature,
provenance, canonical-path and authorization gates stay open.

Earlier 2026-08-16 runs remain historical evidence for their recorded
revisions. They do not supersede the later local and hosted evidence or close
any cross-platform installation, supply-chain or authorization gate.

## What has not been verified

- **Hosted runtime privacy correction.** The corrected target archives passed
  hosted packaging, reproducibility, aggregation and runtime smoke, but the
  replacement binaries have not yet had the direct logging privacy audit that
  rejected the historical candidate. The smoke jobs do not execute a database
  query or enable `IGNATIUS_LOG=debug`.
- **Canonical evidence and supply-chain records.** The retained nine-file
  aggregate is a run-scoped transport artifact. The canonical evidence path is
  not materialised, and no dependency inventory, verified signature or verified
  provenance record is attached.
- **Windows and Linux by hand.** CI builds and tests both platforms, but nobody
  has opened the full-screen client there. The same is true of Warp's renderer
  and a live terminal resize.
- **PostgreSQL 15 and 17.** CI exercises 14, 16 and 18; the local server is 18.4.
- **Windows terminal restoration.** The Unix pty test passes; the equivalent
  ConPTY test remains open work.
- **Panic and signal restoration as a process-level claim.** The RAII and panic
  paths are covered in code and unit tests; only the normal Unix pty path is
  exercised end to end here.

These are tracked in `docs/status.md`.

## Adding a test

Name it as the claim it makes: `a_stale_result_cannot_overwrite_a_newer_query`
rather than `test_update_3`. A failure message should tell someone what broke
without them reading the test body.

Assert meaning, not pixels. A layout test checks that the production marker and
the row count are present, not that a 3 KB buffer is byte-identical, because the
second kind passes while the interface silently loses its meaning.
