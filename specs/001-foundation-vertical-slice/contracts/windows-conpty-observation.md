# Contract: Owned Windows ConPTY guard observation

Adopted for W07-064 on 25 October 2026, Africa/Johannesburg, under REL-001.
This is test infrastructure for the existing guard, not a new product mode.
T055a stays open until every required mode has faithful Windows evidence;
T051 Windows 11/Windows Terminal hand use is a separate requirement.

## Scope and source

Use the existing Windows CI runner, installed MSVC/Windows SDK and pinned Rust.
A test-only native helper may perform Win32 interop. A standalone safe Rust
child must call the actual `ui::terminal::TerminalGuard::acquire` and
`install_panic_hook` from the Cargo-selected non-test library. No production
unsafe Rust, Cargo dependency, credential route, application flag, database,
personal profile, global console reset or new external service is introduced.
Do not change restoration behavior just to make the harness pass.

Build helpers before measuring children. Record source/dirty identity,
Cargo.lock, active Rust/compiler target, selected Cargo JSON library path/hash,
helper sources/compiler/binary hashes, and actual runner image/OS build.
The moving `windows-latest` label is not a Windows 11 identity. Every claim
names the source and actual platform; standalone library panic is not a full
product crash or panic-abort proof.

## Native capability boundary first

Create a fresh owned pseudoconsole with flags zero, synchronous input/output
pipes, the documented STARTUPINFOEX attribute and a directly selected absolute
executable. Do not replace console stdio with ordinary redirected pipes.
Observe through explicit `CONIN$`/`CONOUT$` handles inside the client console,
possibly using an inheriting native observer. HPCON/host pipes are not console
mode handles. Keep bounded observations on a separate owned side channel.

Before expanding the matrix, prove a no-guard control leaves observed state
unchanged and a known test-only un-restored mode is detected. Establish that
native APIs change during an actual acquisition, rather than returning constant
or unsupported answers. Record unsupported observations as gaps, never passes.

- Seed and record a cooked input baseline and a VT-enabled output baseline in
  the owned console. Record full native masks before, acquired and restored.
  Assert actual raw-input and mouse transitions and restoration of that stated
  baseline. Do not claim arbitrary prior raw/output masks round-trip: locked
  Crossterm enables output VT once and restores cooked bits in its raw path.
- Query cursor visibility on the active buffer at all phases.
- Seed a unique main-buffer sentinel. Reopen the active `CONOUT$` observation
  where necessary and read actual buffer content to distinguish acquired
  alternate screen from restored main content. Handle numbers are not identity.
  Unsupported buffer reads or constant observations leave this proof open.
- Bracketed paste has no GetConsoleMode bit. ConPTY translates output, so
  balanced ANSI 2004 calls/bytes alone do not prove restored terminal state.
  Establish a faithful query/transition observation or explicitly leave paste
  and full T055a acceptance open. Do not claim that every current ConPTY drops
  2004 merely because a historical state-forwarding issue exists.

## Cases and ordering

After capability admission, use one fresh process/console per normal return,
deliberate error return and uncaught panic, for all eight combinations of the
mouse/paste/alternate options. Default options are one of those combinations.
Do not count a parent harness return as a child case. Report acquired/restored
options and each native observation independently, including failures/gaps.

For panic, install a previous test hook before the actual product hook. The
previous hook coordinates a native observation before printing its own marker
or invoking the previous/default payload hook. Retain positive ordering and
payload evidence. Probe failures must not panic recursively inside the hook.
Use a bounded explicit phase/acknowledgement protocol, not fixed-delay guesses.
Normal/error observations occur after actual guard restoration and before
terminal reuse. A caught libtest panic is labelled separately and cannot replace
the standalone uncaught-process case.

The fixed synthetic child opens no connection and executes no SQL. Nothing in
this experiment asserts that arbitrary product panic details are secret-free,
that a database write did not happen, or that forced OS restoration errors are
covered. Existing reporting and restoration order remain authoritative.

## Ownership and bounds

Only task-owned process/thread/job/pipe/console handles may be changed. Assign
the controlled process tree to a kill-on-close Job Object before it can run
uncontrolled instructions, using suspended creation where required. Preserve
the runner's existing job membership. Keep the Job handle solely in an outside
supervisor; a contained descendant retaining that handle can defeat last-close
containment. Verify membership of the exact owned Job before resuming through
stable owned process handles; an any-job check is insufficient. Local named
control pipes require reject-remote/first-instance flags, explicit owner/SYSTEM
access and nonce/client-process checks. An owned anonymous route is acceptable.
Assignment/ownership failure is a failed probe, not permission to bypass runner
controls. No broad process-name kill,
detached numeric-PID timer, unowned signal or TerminateThread is allowed.

Each real case has a monotonic 20-second owned-process deadline. A separate
250-millisecond hung-child control starts only after a positive HANG_READY
acknowledgement, with setup separately bounded by 20 seconds, and must prove
owned termination, wait/reap and bounded drainage. An outside supervisor has
an overall 25-second bound including setup/teardown; report every clock and
never count cold process startup as a proven hang. Keep at most 128 KiB translated output and
64 KiB structured observations per case, and at most 32 MiB cumulative
experiment captures/metadata/payload. Frozen compiled inputs/build caches are
excluded and identified explicitly. Exceeding a limit stops later cases and
retains failure evidence; these are experiment stops, not product ceilings.

Drain output independently while coordinating phases. Own and join readers;
close all duplicate pipe ends, attribute storage and process/thread handles.
Handle ClosePseudoConsole's documented version-dependent blocking behavior:
keep drainage available, terminate/wait owned children first, and do not treat
HPCON close alone as proof the child exited. Bounded outer ownership must also
cover a stuck helper/teardown. Unconfirmed cleanup is an explicit failure/gap.

Use isolated config/data/temp and an absent passfile. Actual guard children
receive a minimal required Windows environment, no inherited PG/log/provider
routes or secrets. Do not reassign HOME. Source paths and controlled option bits
are arguments; no real credential or arbitrary user command is passed.

## Delivery and acceptance

The implementer works in an isolated source snapshot and returns an immutable
patch plus source/helper identities, failing-first harness controls, capability
matrix, native pre/acquired/restored observations, ordered markers, exits,
durations, deadline/drain/cleanup records and honest remaining gaps. Start with
the capability/control freeze; do not silently expand past an unsupported seam.
The integrator owns canonical source, CI, shared records and final acceptance.

The root reviews the frozen source and ownership/security seams independently,
runs affected local checks and the required full gate, then uses the authorised
branch/PR workflow for actual Windows execution. Retain bounded synthetic CI
evidence with exact source/compiler/runner identity. Linux/macOS compilation or
an inert parent is not Windows runtime evidence. No new release/upload scope is
implied; any CI artifact list is explicit, synthetic and narrow.

At least raw-input, mouse and cursor phases, plus both negative ownership/state
controls, need faithful native observations before the guard-automation scope
can close. Failure to establish them leaves W07-064 open with a capability
receipt. Alternate-buffer/paste gaps remain explicit.

W07-064 may close for its implemented, faithfully observed automation scope
while documenting an unobservable mode. That does not close full T055a, T051,
Windows Terminal rendering, SIGTERM parity, incoming allocation, DB outcomes or
release readiness. A new product defect requires a meaningful native failure,
a separately reviewed repair and affected repeats; never weaken assertions.

## Primary references

Checked 25 October 2026. The actual locked source and runner record remain the
authority for the executed version.

- [CreatePseudoConsole](https://learn.microsoft.com/en-us/windows/console/createpseudoconsole)
  and [creating a session](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session).
- [Console handles](https://learn.microsoft.com/en-us/windows/console/console-handles),
  [GetConsoleMode](https://learn.microsoft.com/en-us/windows/console/getconsolemode),
  [cursor information](https://learn.microsoft.com/en-us/windows/console/getconsolecursorinfo)
  and [buffer reading](https://learn.microsoft.com/en-us/windows/console/readconsoleoutputcharacter).
- [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
  [creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)
  and [ClosePseudoConsole](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).
- [Microsoft VT model](https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences)
  and [state-forwarding issue 6859](https://github.com/microsoft/terminal/issues/6859).
- [Current Windows runner inventory](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md).

- [Visual Studio developer shells](https://learn.microsoft.com/en-us/visualstudio/ide/reference/command-prompt-powershell?view=vs-2022)
  and [Microsoft vswhere VC discovery](https://github.com/microsoft/vswhere/wiki/Find-VC)
  support the installed-tool CI bootstrap; checked 25 October 2026. No install
  or execution-policy change is part of this workflow.
