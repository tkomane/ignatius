# W07-064 first ConPTY capability/control harness

This is the separate cursor-calibration review candidate, based on exact source
`7d167a96103030914d12cda0e7f6ae2c5c903b84`. The original four capability/control
paths and their acceptance assertions are preserved. It changes no product source,
Cargo target/dependency, CLI option, fixture or CI workflow. It is not Windows
runtime evidence until the integrator explicitly authorises and runs it on an
actual Windows runner. The application contract is
`specs/001-foundation-vertical-slice/contracts/windows-conpty-observation.md`.
The separately frozen 25 October 2026 clarification is an additional input:
SHA256 `7cd9fa8da74e009b815137be2f2403618ad74b3b56304ebe2ef9e6188ab2316b`.

## Bounded first scope

`Run-Controls.ps1 -RunReviewedCapabilityControls` offers exactly four fresh
children/consoles, stopping on the first failure. It cannot run the later 24-path normal/error/panic matrix.

| Case | Meaning of success |
| --- | --- |
| `no-guard` | Native input/output masks and cursor stay at the seeded baseline through three observations. |
| `known-unrestored` | A test-only native raw/mouse change is positively detected as unrestored. This is a negative harness control, not a guard defect. |
| `hung-child` | The actual owned Rust child positively acknowledges `HANG_READY`, remains alive through a 250 ms stable-handle wait, is terminated through that handle, then is waited/reaped with bounded drainage and an empty owned Job. Startup alone cannot satisfy this case. |
| `capability` | The actual non-test library guard is acquired with mouse=true, alternate=true, paste=false. Native raw/mouse/cursor must change and then restore the stated cooked/VT/visible baseline. |

The capability child installs the actual panic hook but does not trigger a
panic. An actual wrong-Job/ownership negative control and native error-path
cleanup proof are not among these four cases and remain later closure gaps.
Normal/error/panic option combinations, paste state, Windows 11/Windows
Terminal hand use, forced restoration failures, arbitrary prior raw/output
masks and DB outcomes remain open. A buffer transition is reported separately
only when native sentinel reads positively show main/alternate/main; missing
or constant buffer observations remain `open-not-proven`. Paste remains
`open-not-measured` regardless of any translated escape sequences.

## Topology and handle ownership

1. PowerShell owns the exact supervisor `Process`, bounded stdout/stderr drains,
   the per-case evidence directory and the whole-experiment budget. No shell,
   process-name kill, PID enumeration or detached timer is used.
2. The native **supervisor stays outside its newly created Job**. It alone holds
   the unnamed, non-inheritable kill-on-close Job handle. It creates the native
   worker suspended, assigns and verifies that exact Job with the stable
   process handle, then resumes. Failed assignment also terminates and waits
   the created-but-unassigned process directly; closing the Job alone would
   leave that process behind. Original error and cleanup results are retained.
3. The **worker is inside that Job and has no console of its own**
   (`CREATE_NO_WINDOW`). It alone owns the local HPCON, synchronous anonymous
   ConPTY pipes, independent reader thread, exclusive translated capture and
   attribute storage. HPCON is never passed across processes or interpreted as
   a normal kernel handle.
4. Worker creates the **Rust child suspended with only the documented
   `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` / `EXTENDED_STARTUPINFO_PRESENT` route**.
   It does not set `STARTF_USESTDHANDLES`, create a new console or break away.
   Supervisor validates the worker-reported actual child creation identity,
   image and exact Job through a stable process handle before allowing resume.
   Worker keeps its original process/thread handles until resume/wait.
5. Before each **separate native observer**, Rust first reserves a control
   connection, then spawns the observer with inherited console/stdio. Rust sends
   its actual `Child` handle value using safe `AsRawHandle`; the supervisor
   duplicates it from the stable Rust process, validates PID/image/exact Job,
   and only then acknowledges registration. Rust retains and waits that Child.
   Failed registration kills and waits only that actual Child; the outer Job
   remains the fallback. No production or standalone Rust unsafe is introduced.
6. Every observer calls `GetConsoleProcessList` before any baseline mutation.
   The list must contain its own PID and the registered Rust PID. Its named-pipe
   client must match the duplicated actual observer process/creation identity
   and exact Job before attachment acknowledgement. Only then does the observer
   open actual `CONIN$`/`CONOUT$` and seed/read native state. Neither the worker's
   runner console nor ConPTY transport handles are used for mode observations.

The side pipe is reject-remote/first-instance, with an explicit protected DACL
for the current owner SID and SYSTEM. No world access or inherited Job handle
is allowed. Random per-case nonce, exact client-process handles/image and Job
membership are all required; nonce alone is insufficient. Byte-pipe ACKs use a
bounded client receipt before disconnect so unread ACKs are not discarded.
`FlushFileBuffers` is deliberately not used as an unbounded acknowledgement.

## Clocks, drainage and failure handling

Native setup/real cases use a 20 s monotonic deadline and a 25 s outside
supervisor bound including teardown. Native control work and the controller
stop normal work at 23 s, reserving the remaining two seconds for containment,
wait and drainage. All controller process/task waits consume the same absolute
25 s stopwatch allowance; process and each reader never get a renewed timeout. The 250 ms hung interval starts only after
positive hang ACK/client-receipt; it never includes cold startup. QPC frequency,
start, registered, actual worker-observed resume, ready ACK, kill, guard wait,
cleanup admission and end counters are retained. QPC is shared across the native
processes on supported modern Windows. Arrival order of the post-resume message
is not mistaken for the actual resume time.

The worker drains translated output independently while guard/observer control
phases run. After actual guard wait it closes input/HPCON, joins the reader and
reports cleanup. `ClosePseudoConsole` running in the worker cannot bypass the
outside supervisor/controller deadline. It is never used as proof of child
exit. Native success additionally requires the worker wait and actual owned
Job active-process count zero. Async HPCON teardown is boundedly observed via
that count, rather than inferred from elapsed time. On failure the supervisor
terminates only its exact Job, retains an explicit failed/unconfirmed cleanup
receipt and stops later cases. `TerminateThread` and breakaway flags are absent.
The controller's own timeout kills only its owned supervisor; last-close Job
containment remains in the supervisor. An unjoined controller drain is failure.
Native overlapped I/O never uses blocking post-cancel GetOverlappedResult(TRUE).
A single heap-owned operation contains its event, OVERLAPPED and all referenced
bytes. Cancellation is bounded by the remaining absolute cleanup allowance;
if completion stays unconfirmed the entire state is deliberately retained until
supervisor-process exit. It is never freed/reused by a Server destructor or
labelled confirmed cleanup. The receipt reports control_io_cleanup_confirmed.
The translated reader records ReadFile's actual BOOL and immediate native error:
only successful zero-byte EOF or zero-byte broken-pipe completion is accepted;
every unexpected failed read, including zero bytes, remains an explicit error.

At most 128 KiB translated output and 64 KiB combined structured control and
supervisor capture are accepted per case; at most 32 MiB experiment evidence is
admitted. Buffers/lines are fixed or bounded. Overflow fails and stops later
cases, preserving partial captures. Compile inputs/cache/build identity manifests
are identified separately and excluded from experiment capture accounting.
Only synthetic fixed phase data is produced; evidence has no credential route.
The Rust child receives only required Windows paths plus owned temp/config/data
and absent passfile paths. HOME is absent, never reassigned. PG/log/provider
routes are not inherited. No application connection startup or SQL occurs.

## Build and later native invocation

Use existing installed MSVC/SDK developer environment and pinned Rust 1.97.1.
The build script is offline and does not install anything. Provide a new owned
build directory and task-owned Cargo home/target with the existing public
registry cache available. It selects the non-test library and native dependency
directory from actual Cargo JSON, records all helper/compiler/binary/library/link
input hashes and actual OS/image/toolchain identity, then runs only pure tests.
It prehashes all helpers, tracked source and Cargo.lock before any compiler/test
call, keeps prebuild-inputs.json, and verifies the same hashes, revision and
tracked/staged/untracked state afterward before declaring source unchanged.
Rust release is parsed as one exact release 1.97.1 line, not a substring.

```powershell
./Build.ps1 -SourceRoot C:\owned\source -BuildRoot C:\owned\build `
  -CargoHome C:\owned\cargo-home -CargoTarget C:\owned\cargo-target
```

After immutable review and integrator native-execution approval only:

```powershell
./Run-Controls.ps1 -BuildManifest C:\owned\build\build-manifest.json `
  -EvidenceRoot C:\owned\new-evidence -RunReviewedCapabilityControls
```

The switch is an accidental-execution guard, not owner consent. A moving
`windows-latest` identity never establishes Windows 11 hand use. Failure to
compile MSVC or establish native capabilities is retained as such, not replaced
with translated-byte, mock or local macOS evidence. There is no retry or matrix
expansion after a failed mandatory capability.

## Local validation boundary

The portable C++ arithmetic/parser tests and PowerShell receipt tests can run
on macOS without native calls. A standalone Rust build must link the actual
Cargo-selected non-test library; its non-Windows route returns an explicit
unsupported error before acquiring a terminal. This calibration candidate uses
Rust formatting only locally. Its Windows-only `AsRawHandle` branch and the
native Windows host need actual Windows/MSVC compilation. The non-Windows host stub exits 77. Neither
local compile, stub nor synthetic receipt tests count as a Windows child case.

Native API references were checked on 25 October 2026:
[ConPTY session](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session),
[CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw),
[IsProcessInJob](https://learn.microsoft.com/en-us/windows/win32/api/jobapi/nf-jobapi-isprocessinjob),
[console handles](https://learn.microsoft.com/en-us/windows/console/console-handles)
and [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[CancelIoEx lifetime](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)
and [nonblocking GetOverlappedResult](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-getoverlappedresult).
Additional named-pipe API fetches were unavailable through the current browser;
Windows SDK compilation and hosted capability evidence remain required.


## Separate cursor-routing calibration

The additional authoritative immutable contract is
`/private/tmp/ignatius-w07-cursor-calibration-contract-20261025.md`, SHA256
`0cc81e4bdcd68692a155911463cd7aba279e7d03afac300c41b0811c5df5f286`.
The existing default capability predicate and its actual failed cursor sensor
run at `7d167a9` remain unchanged. This diagnostic has a separate explicit
selection, no implicit invocation, and cannot be combined with the four cases:

```powershell
./Run-Controls.ps1 -BuildManifest C:\owned\build\build-manifest.json `
  -EvidenceRoot C:\owned\cursor-runtime -RunReviewedCursorDiagnostic
```

The build manifest retains its legacy scope and adds only
`cursor_routing_diagnostic=true`. It remains `BuildRoot/build-manifest.json`.
The calibration reuses the original supervisor/worker/Rust-child topology and
owned Job, pipe authentication, exact stable Child registration, positive
console-attachment attestation, environment, output bounds and absolute
20/25-second deadlines. The Rust child launches exactly one native actor before
baseline and waits it. That actor retains its CONIN$/CONOUT$ handles throughout
all nine ordered stages; no new observer enters between a Hide and its query.
No product Guard is acquired in this selection.

| Index | Stage | Writer/change before query |
| --- | --- | --- |
| 0 | baseline-main | Phase marker, seeded cooked/VT/visible baseline |
| 1 | native-hidden | Marker then native cursor setter false |
| 2 | native-shown | Marker then native cursor setter true |
| 3 | main-vt-hidden | Fixed VT Hide plus marker in one write |
| 4 | main-vt-shown | Fixed VT Show plus marker in one write |
| 5 | alternate-entered | Fixed VT alternate entry plus marker |
| 6 | alternate-vt-hidden | Fixed VT Hide plus marker in alternate |
| 7 | alternate-vt-shown | Fixed VT Show plus marker, observed before exit |
| 8 | restored-main | Ordinary VT main return plus marker, then baseline restore |

Each marker is `W07C-<nonce>-<index>` at row 2, column 1. A full synchronous
`WriteConsoleW` to the retained output handle is followed by a complete active
buffer readback of that marker before the cursor query. Native setters keep the
baseline cursor size; no intervening console write follows those setters before
their query. Marker readback is a parser/buffer fence only. It is not cursor
state proof, an emulator response or a rendering claim. The retained-handle route
is explicit so a main/alternate sensor routing difference stays observable.

Every stage retains the sentinel API's actual BOOL/error and its requested and
returned character counts. A successful short read is not alternate absence:
completion requires requested=`4+nonce length` and returned=requested in all
nine stages. A successful full read with different content can establish that
sentinel absence, without inventing a native error for TRUE-short reads.
Restoration separately records all three verification Get* BOOL/error outcomes,
actual input/output masks and cursor size/visibility, and their own QPC time.
No query short-circuits an attempted later query. These earlier restore facts
must independently match the seeded baseline and precede the distinct stage8
requery; the later query cannot substitute for them. The diagnostic-only D/T
wire shapes are exactly 30/23 bounded tokens; legacy O/P/A/C shapes stay intact.

The separate receipt has nine stage masks, cursor sizes/visibility, API outcome
and immediate error pairs, sentinel/marker observations, native QPC observation
and supervisor acknowledgement clocks, and final restoration/owned cleanup
facts. `diagnostic_complete` means those required facts and bounds completed;
`product_acceptance` and `cursor_capability_accepted` are always false. Three
separate sensor labels require actual hidden/shown values for native setters,
main VT and alternate VT respectively. A constant visibility sensor can complete
a diagnostic with its sensor labels false; it cannot pass the original Guard
capability. API, fence, order, identity, restoration, deadline or cleanup failure
stops this diagnostic with retained evidence.

Only these bounded native evidence files are suitable for the later upload:
`summary.json`, `capture-manifest.json`, and
`cursor-routing/{controller.json,supervisor.stdout.bin,supervisor.stderr.bin,translated.bin}`.
Build identities/logs are the existing separately reviewed narrow list, not the
whole build/cache/temp tree. Paste, mode-25 query, an alt=false Guard, hidden
alternate exit pairing, wrong-ownership/error/panic controls, the full 24 paths
and Windows 11/Windows Terminal hand use remain outside this diagnostic.

For this candidate, local portable C++/PowerShell checks, actual emitted-JSON
validation, AST parsing and Rust formatting are separate evidence. No Cargo,
Windows/MSVC compile, native child or old frozen cache is run or changed locally.
The first reviewed hosted build must establish those platform-specific facts.
