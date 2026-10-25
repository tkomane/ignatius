# First capability/control execution only. Root must inspect frozen inputs first.
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $BuildManifest,
    [Parameter(Mandatory)] [string] $EvidenceRoot,
    [switch] $RunReviewedCapabilityControls
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Actual Windows is required; parser/local compilation are not native runtime evidence' }
if (-not $RunReviewedCapabilityControls) { throw 'No execution by default. First immutable source review and explicit native execution authority are required.' }
. (Join-Path $PSScriptRoot 'Validation.ps1')
$manifestPath = (Resolve-Path -LiteralPath $BuildManifest).Path
$manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
Assert-W07 ($manifest.schema -eq 1 -and $manifest.scope -ceq 'first-capability-controls-only' -and -not $manifest.native_executed -and $manifest.pre_post_inputs_verified) 'first capability build manifest'
Assert-W07 ([IO.Path]::IsPathFullyQualified($EvidenceRoot) -and -not (Test-Path -LiteralPath $EvidenceRoot)) 'new absolute owned evidence root'
$null = New-Item -ItemType Directory -Path $EvidenceRoot
$evidence = (Resolve-Path -LiteralPath $EvidenceRoot).Path
function Assert-Inputs {
    Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant() -ceq $manifestHash) 'build manifest changed'
    foreach ($item in @($manifest.helper_inputs) + @($manifest.binaries) + @($manifest.link_inputs) + @($manifest.library) + @($manifest.source_inputs)) {
        Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath $item.path).Hash.ToLowerInvariant() -ceq $item.sha256) "frozen input changed: $($item.path)"
    }
    Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $manifest.build_root 'prebuild-inputs.json')).Hash.ToLowerInvariant() -ceq $manifest.prebuild_manifest_sha256) 'prebuild input manifest changed'
    Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $manifest.source 'Cargo.lock')).Hash.ToLowerInvariant() -ceq $manifest.lock_sha256) 'source lock changed'
}
function Invoke-OwnedCase([string] $Case, [string] $Directory, [long] $Allowance) {
    $hostBinary = @($manifest.binaries | Where-Object { [IO.Path]::GetFileName($_.path) -ceq 'conpty-host.exe' })
    $guardBinary = @($manifest.binaries | Where-Object { [IO.Path]::GetFileName($_.path) -ceq 'guard-child.exe' })
    Assert-W07 ($hostBinary.Count -eq 1 -and $guardBinary.Count -eq 1) 'exact compiled executables'
    $nonce = [Guid]::NewGuid().ToString('N')
    foreach ($name in @('config','data')) { $null = New-Item -ItemType Directory -Path (Join-Path $Directory $name) }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $hostBinary[0].path; $start.WorkingDirectory = $Directory
    $start.UseShellExecute = $false; $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $start.CreateNoWindow = $true
    foreach ($arg in @('--supervise',$Case,$guardBinary[0].path,$nonce,$Directory)) { $start.ArgumentList.Add($arg) }
    $start.Environment.Clear()
    foreach ($name in @('SystemRoot','WINDIR')) { $start.Environment[$name] = [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows) }
    $start.Environment['TEMP'] = $Directory; $start.Environment['TMP'] = $Directory
    $start.Environment['IGNATIUS_CONFIG_DIR'] = Join-Path $Directory 'config'
    $start.Environment['IGNATIUS_DATA_DIR'] = Join-Path $Directory 'data'
    $start.Environment['PGPASSFILE'] = Join-Path $Directory 'absent-passfile'
    # HOME is absent, never reassigned. No inherited PG/log/provider routes.
    $process = [Diagnostics.Process]::new(); $process.StartInfo = $start
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $state = [ordered]@{cleanup_started_ms=$null;absolute_deadline_ms=25000;resource_cleanup_confirmed=$false;case=$Case;native_windows=$true;started=$false;supervisor_pid=$null;supervisor_exit=$null;elapsed_ms=$null;owned_kill_requested=$false;owned_waited=$false;drain_complete=$false;capture_bytes=0;stop=$null;receipt_valid=$false;nonce=$nonce;build_manifest_sha256=$manifestHash}
    $streams = @(); $captured = [long]0
    try {
        Assert-W07 ($process.Start()) 'start exact owned supervisor'; $state.started=$true; $state.supervisor_pid=$process.Id
        foreach ($pair in @(@('stdout',$process.StandardOutput.BaseStream), @('stderr',$process.StandardError.BaseStream))) {
            $path = Join-Path $Directory ("supervisor.$($pair[0]).bin")
            $file = [IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
            $buffer = [byte[]]::new(4096)
            $streams += [pscustomobject]@{name=$pair[0];source=$pair[1];file=$file;buffer=$buffer;task=$pair[1].ReadAsync($buffer,0,$buffer.Length);done=$false;bytes=[long]0}
        }
        # The one stopwatch starts before Process.Start and never resets.
        # Stop normal work at23 s, reserving the remainder of25 s for ownership
        # containment, process wait and BOTH drain joins together.
        while ($true) {
            $phase=Get-W07ControllerPhase $watch.ElapsedMilliseconds
            if ($phase -ceq 'expired') { $state.stop='25-second-absolute-deadline'; break }
            if ($phase -ceq 'cleanup' -and $null -eq $state.stop) { $state.stop='cleanup-reserve-deadline' }
            if ($state.stop -and -not $process.HasExited -and -not $state.owned_kill_requested) {
                $state.cleanup_started_ms=$watch.ElapsedMilliseconds
                $state.owned_kill_requested=$true; $process.Kill()
            }
            foreach ($stream in $streams) {
                if (-not $stream.done -and $stream.task.IsCompleted) {
                    $n = $stream.task.GetAwaiter().GetResult() # Already complete, never waits.
                    if ($n -eq 0) { $stream.done=$true; continue }
                    $room = [Math]::Max(0,[Math]::Min(65536-$captured,$Allowance-$captured))
                    $keep = [Math]::Min($room,$n)
                    if ($keep -gt 0) { $stream.file.Write($stream.buffer,0,[int]$keep); $stream.bytes += $keep; $captured += $keep }
                    if ($keep -ne $n -and $null -eq $state.stop) { $state.stop='structured-output-budget' }
                    $stream.task = $stream.source.ReadAsync($stream.buffer,0,$stream.buffer.Length)
                }
            }
            if ($process.HasExited -and @($streams | Where-Object { -not $_.done }).Count -eq 0) { break }
            $pause=Get-W07WaitBudget $watch.ElapsedMilliseconds 1
            if ($pause -gt 0) { Start-Sleep -Milliseconds $pause } # Event/deadline polling, not phase evidence.
        }
        $state.owned_waited = $process.WaitForExit((Get-W07WaitBudget $watch.ElapsedMilliseconds 1000))
        if ($state.owned_waited) { $state.supervisor_exit=$process.ExitCode }
        $state.drain_complete = @($streams | Where-Object { -not $_.done }).Count -eq 0
        Assert-W07 ($state.owned_waited -and $state.drain_complete -and $null -eq $state.stop) 'owned supervisor wait/drain/deadline'
        Assert-W07 ($process.ExitCode -eq 0) 'native supervisor failed; stop later cases'
        foreach ($stream in $streams) { $stream.file.Flush(); $stream.file.Dispose() }
        $raw = [IO.File]::ReadAllText((Join-Path $Directory 'supervisor.stdout.bin'))
        $receipt = $raw | ConvertFrom-Json
        Assert-W07Receipt $receipt $Case
        Assert-W07 (($captured + [long]$receipt.control_bytes) -le 65536) 'combined structured evidence bound'
        $translated = Join-Path $Directory 'translated.bin'
        Assert-W07 ((Test-Path -LiteralPath $translated -PathType Leaf) -and (Get-Item -LiteralPath $translated).Length -eq $receipt.output_kept) 'actual translated capture length'
        $state.receipt_valid=$true
    } catch {
        $state.stop = if ($state.stop) { $state.stop } else { $_.Exception.Message }
    } finally {
        if ($state.started) {
            if (-not $process.HasExited) {
                if ($null -eq $state.cleanup_started_ms) { $state.cleanup_started_ms=$watch.ElapsedMilliseconds }
                $state.owned_kill_requested=$true
                try { $process.Kill() } catch { $state.stop="owned-supervisor-kill-unconfirmed: $($_.Exception.Message)" }
            }
            # A zero allowance performs only a nonblocking observation. No
            # process or reader ever receives a renewed post-deadline timeout.
            $state.owned_waited=$process.WaitForExit((Get-W07WaitBudget $watch.ElapsedMilliseconds 1000))
            if ($state.owned_waited) { $state.supervisor_exit=$process.ExitCode }
        }
        foreach ($stream in $streams) {
            if (-not $stream.done) {
                $stream.source.Dispose() # Close only this actual owned read endpoint.
                try {
                    $allowance=Get-W07WaitBudget $watch.ElapsedMilliseconds 1000
                    if ($stream.task.IsCompleted -or ($allowance -gt 0 -and $stream.task.Wait($allowance))) { $null=$stream.task.GetAwaiter().GetResult() }
                } catch { } # Cancellation/fault is retained as incomplete drainage.
                $stream.done=$stream.task.IsCompleted
                if (-not $stream.done) {
                    # Managed async state/buffer stays rooted until controller
                    # process exit, never reused or called confirmed cleanup.
                    if (-not (Get-Variable -Name W07UnconfirmedReads -Scope Script -ErrorAction Ignore)) { $script:W07UnconfirmedReads=@() }
                    $script:W07UnconfirmedReads+=@($stream)
                    $state.stop='unjoined-owned-drain'
                }
            }
            $stream.file.Dispose()
        }
        $state.resource_cleanup_confirmed=$state.owned_waited -and @($streams | Where-Object { -not $_.done }).Count -eq 0
        if (-not $state.resource_cleanup_confirmed) { $state.stop='owned-cleanup-unconfirmed' }
        $process.Dispose()
        $state.elapsed_ms=$watch.ElapsedMilliseconds; $state.capture_bytes=$captured
        if ($state.elapsed_ms -ge 25000 -and -not $state.stop) { $state.stop='25-second-absolute-deadline' }
        if ($state.stop) { $state.receipt_valid=$false }
        $state | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Directory 'controller.json') -Encoding utf8NoBOM
    }
    return [pscustomobject]$state
}
$summary = [ordered]@{schema=1;scope='first-capability-controls-only';build_manifest_sha256=$manifestHash;platform=[Environment]::OSVersion.VersionString;os_build=[Environment]::OSVersion.Version.Build;windows11_hand_use=$false;full_24_path_matrix=$false;paste_observation='open-not-measured';native_wrong_ownership_control='open-not-run';cases=@();stop=$null;post_inputs_verified=$false;capture_budget_bytes=33554432}
try {
    Assert-Inputs
    foreach ($case in @('no-guard','known-unrestored','hung-child','capability')) {
        $used = [long](@(Get-ChildItem -LiteralPath $evidence -File -Recurse | Measure-Object Length -Sum)[0].Sum)
        Assert-W07 (($used + 262144) -le 33554432) 'experiment budget admission'
        $directory = Join-Path $evidence $case; $null=New-Item -ItemType Directory -Path $directory
        $result = Invoke-OwnedCase $case $directory (33554432-$used)
        $summary.cases += $result
        if (-not $result.receipt_valid) { throw "Case $case failed; no retries or later cases" }
    }
} catch { $summary.stop=$_.Exception.Message }
finally {
    try { Assert-Inputs; $summary.post_inputs_verified=$true } catch { $summary.stop="Frozen input verification failed: $($_.Exception.Message)" }
    $summary | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $evidence 'summary.json') -Encoding utf8NoBOM
    $files = @(Get-ChildItem -LiteralPath $evidence -File -Recurse | Sort-Object FullName | ForEach-Object { @{path=[IO.Path]::GetRelativePath($evidence,$_.FullName);bytes=$_.Length;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()} })
    $files | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidence 'capture-manifest.json') -Encoding utf8NoBOM
}
if ($summary.stop -or $summary.cases.Count -ne 4 -or -not $summary.post_inputs_verified) { throw 'Capability/control run incomplete or failed; inspect retained bounded evidence' }
Write-Output 'Four actual Windows capability/control children passed. Paste/full matrix/Windows 11 hand use remain open.'
