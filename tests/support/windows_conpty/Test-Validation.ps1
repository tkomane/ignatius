# Portable pure validator tests. No processes, terminals, native calls or DB.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Validation.ps1')
$script:Checks = 0
function New-Receipt([string] $Case) {
    $base = [ordered]@{phase='baseline';input_ok=1;output_ok=1;cursor_ok=1;buffer_ok=1;input_mask=135;output_mask=7;cursor_visible=1;sentinel_seen=1;native_error=0}
    $after = [ordered]@{phase='restored';input_ok=1;output_ok=1;cursor_ok=1;buffer_ok=1;input_mask=135;output_mask=7;cursor_visible=1;sentinel_seen=1;native_error=0}
    $acquired = [ordered]@{phase='acquired';input_ok=1;output_ok=1;cursor_ok=1;buffer_ok=1;input_mask=152;output_mask=7;cursor_visible=0;sentinel_seen=0;native_error=0}
    $r = [ordered]@{schema=1;case=$Case;native_windows=$true;success=$true;failure='';qpc_frequency=1000;clocks=[ordered]@{start=1000;registered=1100;resume_ack=1200;hang_ready_ack=0;kill=0;guard_wait=1700;cleanup=1800;end=1900};child_pid=42;child_exit=0;worker_exit=0;exact_job_worker=1;exact_job_guard=1;hang_ready_ack=0;hang_deadline_elapsed=0;terminated_owned=0;guard_waited=1;worker_waited=1;cleanup_confirmed=1;control_io_cleanup_confirmed=1;job_active_processes=0;output_seen=100;output_kept=100;output_error=0;attachment_attestations=3;control_bytes=512;paste_observation='open-not-measured';alternate_buffer='open-not-proven';observations=@($base,$acquired,$after)}
    switch ($Case) {
        'no-guard' { $acquired.input_mask=135; $acquired.cursor_visible=1; $acquired.sentinel_seen=1 }
        'known-unrestored' { $after.input_mask=152; $r.observations=@($base,$after); $r.attachment_attestations=2 }
        'hung-child' { $r.observations=@($base); $r.attachment_attestations=1; $r.child_exit=124; $r.hang_ready_ack=1; $r.hang_deadline_elapsed=1; $r.terminated_owned=1; $r.clocks.hang_ready_ack=1300; $r.clocks.kill=1550 }
    }
    return ($r | ConvertTo-Json -Depth 8 | ConvertFrom-Json)
}
function Good([string] $Case) { Assert-W07Receipt (New-Receipt $Case) $Case; ++$script:Checks }
function Bad([string] $Name, [scriptblock] $Change, [string] $Case='capability') {
    $r=New-Receipt $Case; & $Change $r; $rejected=$false
    try { Assert-W07Receipt $r $Case } catch { $rejected=$true }
    Assert-W07 $rejected "negative receipt accepted: $Name"; ++$script:Checks
}
foreach ($case in @('no-guard','known-unrestored','hung-child','capability')) { Good $case }
Bad 'native false' { param($r) $r.native_windows=$false }
Bad 'failure cannot be pass' { param($r) $r.success=$false }
Bad 'exact job required' { param($r) $r.exact_job_guard=0 }
Bad 'unwaited process' { param($r) $r.guard_waited=0 }
Bad 'unconfirmed control I/O' { param($r) $r.control_io_cleanup_confirmed=0 }
Bad 'unconfirmed teardown' { param($r) $r.cleanup_confirmed=0 }
Bad 'remaining process' { param($r) $r.job_active_processes=1 }
Bad 'output overflow' { param($r) $r.output_seen=131073; $r.output_kept=131073 }
Bad 'partial capture' { param($r) $r.output_kept=99 }
Bad 'observation overflow' { param($r) $r.control_bytes=65537 }
Bad 'negative integer' { param($r) $r.output_seen=-1; $r.output_kept=-1 }
Bad 'numeric string' { param($r) $r.qpc_frequency='1000' }
Bad 'native boolean range' { param($r) $r.observations[0].input_ok=2 }
Bad 'missing phase' { param($r) $r.observations=@($r.observations[0],$r.observations[2]) }
Bad 'attachment unproven' { param($r) $r.attachment_attestations=2 }
Bad 'constant native acquisition' { param($r) $r.observations[1].input_mask=135 }
Bad 'mouse transition missing' { param($r) $r.observations[1].input_mask=128 }
Bad 'cursor transition missing' { param($r) $r.observations[1].cursor_visible=1 }
Bad 'restore mask lost' { param($r) $r.observations[2].input_mask=152 }
Bad 'no-guard changed' { param($r) $r.observations[1].input_mask=152 } 'no-guard'
Bad 'unrestored undetected' { param($r) $r.observations[1].input_mask=135 } 'known-unrestored'
Bad 'cold startup called hang' { param($r) $r.hang_ready_ack=0 } 'hung-child'
Bad 'short hang' { param($r) $r.clocks.kill=1549 } 'hung-child'
Bad 'kill without wait' { param($r) $r.guard_waited=0 } 'hung-child'
Bad 'outer bound' { param($r) $r.clocks.end=26001 }
Bad 'setup bound' { param($r) $r.clocks.hang_ready_ack=22000; $r.clocks.kill=22250; $r.clocks.guard_wait=22300; $r.clocks.cleanup=22400; $r.clocks.end=22500 } 'hung-child'
Bad 'constant buffer claimed as transition' { param($r) $r.alternate_buffer='native-transition-observed'; $r.observations[1].sentinel_seen=1 }
Bad 'unsupported buffer claimed' { param($r) $r.alternate_buffer='native-transition-observed'; $r.observations[1].buffer_ok=0 }
$bufferReceipt=New-Receipt 'capability'; $bufferReceipt.alternate_buffer='native-transition-observed'; Assert-W07Receipt $bufferReceipt 'capability'; ++$script:Checks
Bad 'paste claimed' { param($r) $r.paste_observation='passed' }
Assert-W07 ((Get-W07WaitBudget 25000 1000) -eq 0) 'no extra process/drain wait after overall deadline'; ++$script:Checks
Assert-W07 ((Get-W07WaitBudget 24990 1000) -eq 10) 'cleanup wait capped by remaining overall budget'; ++$script:Checks
Assert-W07 ((Get-W07WaitBudget 24000 1000) -eq 1000) 'one remaining second'; ++$script:Checks
Assert-W07 ((Get-W07WaitBudget 26000 1000) -eq 0) 'expired budget never renews'; ++$script:Checks
Assert-W07 (Test-W07PinnedRust "release: 1.97.1`nhost: x86_64-pc-windows-msvc") 'exact pinned release'; ++$script:Checks
Assert-W07 (-not (Test-W07PinnedRust 'rustc 1.97.1 without verbose release')) 'missing release line must be rejected'; ++$script:Checks
Assert-W07 (-not (Test-W07PinnedRust 'release: 1.97.10')) 'substring patch release must be rejected'; ++$script:Checks
Assert-W07 (-not (Test-W07PinnedRust 'release: 1.97.1-nightly')) 'suffix release must be rejected'; ++$script:Checks
Assert-W07 (-not (Test-W07PinnedRust "release: 1.97.1`nrelease: 1.97.10")) 'duplicate release lines must be rejected'; ++$script:Checks
Assert-W07 ((Get-W07ControllerPhase 22999) -ceq 'run' -and (Get-W07ControllerPhase 23000) -ceq 'cleanup' -and (Get-W07ControllerPhase 25000) -ceq 'expired') 'cleanup reserve precedes absolute expiry'; ++$script:Checks
# A fake-clock blocked-process/two-drain sequence spends one shared allowance.
$elapsed=23000; $spent=0
foreach ($request in @(1000,1000,1000)) { $grant=Get-W07WaitBudget $elapsed $request; $elapsed+=$grant; $spent+=$grant }
Assert-W07 ($elapsed -eq 25000 -and $spent -eq 2000 -and (Get-W07WaitBudget $elapsed 1000) -eq 0) 'blocked process and drains cannot renew timeout'; ++$script:Checks
Assert-W07 (-not (Test-W07PinnedRust "release: 1.97.1 `nhost: x86_64-pc-windows-msvc")) 'trailing release text must be rejected'; ++$script:Checks
$owned=[IO.Path]::Combine([IO.Path]::GetTempPath(),'ignatius-w07-pure-input-'+[Guid]::NewGuid().ToString('N'))
$null=[IO.Directory]::CreateDirectory($owned); $inputFile=[IO.Path]::Combine($owned,'input.txt')
try {
    [IO.File]::WriteAllText($inputFile,'before')
    $frozen=@(@{path=$inputFile;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $inputFile).Hash.ToLowerInvariant()})
    Assert-W07FrozenInputs $frozen; ++$script:Checks
    [IO.File]::WriteAllText($inputFile,'changed'); $rejected=$false
    try { Assert-W07FrozenInputs $frozen } catch { $rejected=$true }
    Assert-W07 $rejected 'changed source between prehash/postcheck must be rejected'; ++$script:Checks
} finally { [IO.File]::Delete($inputFile); [IO.Directory]::Delete($owned) }
Write-Output "$script:Checks receipt/deadline/identity checks passed; only one owned temporary hash fixture; no Windows/native execution"
