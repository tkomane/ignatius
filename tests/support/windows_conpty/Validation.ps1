# Pure receipt validation. Dot-source from the controller and local tests.
Set-StrictMode -Version Latest
function Assert-W07([bool] $Okay, [string] $Message) {
    if (-not $Okay) { throw "W07-064: $Message" }
}
function Get-W07Number($Value) {
    Assert-W07 ($Value -is [long] -or $Value -is [int] -or $Value -is [uint32] -or $Value -is [uint64]) 'integer required'
    Assert-W07 ($Value -ge 0) 'nonnegative integer required'
    return [decimal] $Value
}
function Test-W07Flag($Value) {
    $n = Get-W07Number $Value
    Assert-W07 ($n -eq 0 -or $n -eq 1) 'native boolean range'
    return $n -eq 1
}
function Assert-W07Receipt($Receipt, [string] $ExpectedCase) {
    Assert-W07 ($ExpectedCase -cin @('no-guard', 'known-unrestored', 'hung-child', 'capability')) 'case outside first capability scope'
    Assert-W07 ($Receipt.schema -eq 1 -and $Receipt.case -ceq $ExpectedCase) 'receipt schema/case'
    Assert-W07 ($Receipt.native_windows -is [bool] -and $Receipt.native_windows -and $Receipt.success -is [bool] -and $Receipt.success) 'native failure is not a pass'
    Assert-W07 ([string]::IsNullOrEmpty($Receipt.failure)) 'failure field must be empty on success'
    foreach ($field in @('exact_job_worker','exact_job_guard','guard_waited','worker_waited','cleanup_confirmed','control_io_cleanup_confirmed')) { Assert-W07 (Test-W07Flag $Receipt.$field) "positive $field required" }
    Assert-W07 ((Get-W07Number $Receipt.job_active_processes) -eq 0) 'owned job must be empty'
    Assert-W07 ((Get-W07Number $Receipt.worker_exit) -eq 0 -and (Get-W07Number $Receipt.output_error) -eq 0) 'worker/output failure'
    Assert-W07 ((Get-W07Number $Receipt.output_seen) -le 131072 -and $Receipt.output_seen -eq $Receipt.output_kept) 'complete bounded translated capture'
    Assert-W07 ((Get-W07Number $Receipt.control_bytes) -le 65536) 'structured observation bound'
    Assert-W07 ((Get-W07Number $Receipt.child_pid) -gt 0) 'positive owned child identity'
    $frequency = Get-W07Number $Receipt.qpc_frequency
    Assert-W07 ($frequency -gt 0) 'positive monotonic frequency'
    $c = $Receipt.clocks
    foreach ($field in @('start','registered','resume_ack','guard_wait','cleanup','end')) { Assert-W07 ((Get-W07Number $c.$field) -gt 0) "clock $field required" }
    Assert-W07 ($c.start -le $c.registered -and $c.registered -le $c.resume_ack -and $c.resume_ack -le $c.guard_wait -and $c.guard_wait -le $c.cleanup -and $c.cleanup -le $c.end) 'positive lifecycle clock order'
    Assert-W07 (([decimal]$c.end - $c.start) -le (25 * $frequency)) '25 s outer bound'
    $observed = @($Receipt.observations)
    $phases = @(switch ($ExpectedCase) { 'hung-child' { @('baseline') } 'known-unrestored' { @('baseline','restored') } default { @('baseline','acquired','restored') } })
    Assert-W07 ($observed.Count -eq $phases.Count -and $Receipt.attachment_attestations -eq $phases.Count) 'all phases need exact inside-console attachment'
    for ($i = 0; $i -lt $phases.Count; ++$i) {
        $o = $observed[$i]
        Assert-W07 ($o.phase -ceq $phases[$i]) 'exact phase order'
        foreach ($field in @('input_ok','output_ok','cursor_ok')) { Assert-W07 (Test-W07Flag $o.$field) "native $field unsupported" }
        foreach ($field in @('buffer_ok','cursor_visible','sentinel_seen')) { $null = Test-W07Flag $o.$field }
        foreach ($field in @('input_mask','output_mask','native_error')) { Assert-W07 ((Get-W07Number $o.$field) -le [uint32]::MaxValue) 'native integer width' }
    }
    $base = $observed[0]
    Assert-W07 (($base.input_mask -band 7) -eq 7 -and ($base.input_mask -band 16) -eq 0 -and ($base.output_mask -band 4) -ne 0 -and (Test-W07Flag $base.cursor_visible)) 'seeded cooked/no-mouse/VT/visible baseline'
    if ($ExpectedCase -ceq 'hung-child') {
        foreach ($field in @('hang_ready_ack','hang_deadline_elapsed','terminated_owned')) { Assert-W07 (Test-W07Flag $Receipt.$field) "positive hung control $field" }
        Assert-W07 ($Receipt.child_exit -eq 124) 'owned hung termination exit'
        Assert-W07 ($c.hang_ready_ack -ge $c.resume_ack -and $c.hang_ready_ack -le $c.kill -and $c.kill -le $c.guard_wait) 'positive ready/kill/wait clocks'
        Assert-W07 (([decimal]$c.hang_ready_ack - $c.start) -le (20 * $frequency)) '20 s setup bound'
        Assert-W07 (([decimal]$c.kill - $c.hang_ready_ack) -ge ($frequency / 4)) '250 ms begins only after acknowledged ready'
    } else {
        Assert-W07 ($Receipt.child_exit -eq 0) 'controlled child success'
        Assert-W07 (([decimal]$c.guard_wait - $c.start) -le (20 * $frequency)) '20 s real case bound'
        $after = $observed[-1]
        $equal = $base.input_mask -eq $after.input_mask -and $base.output_mask -eq $after.output_mask -and $base.cursor_visible -eq $after.cursor_visible
        if ($ExpectedCase -ceq 'known-unrestored') { Assert-W07 (-not $equal -and ($after.input_mask -band 7) -eq 0) 'known unrestored raw state must be detected' }
        else {
            Assert-W07 $equal 'restore must match the stated baseline'
            $acquired = $observed[1]
            if ($ExpectedCase -ceq 'no-guard') { Assert-W07 ($base.input_mask -eq $acquired.input_mask -and $base.output_mask -eq $acquired.output_mask -and $base.cursor_visible -eq $acquired.cursor_visible) 'no-guard native state must stay constant' }
            else { Assert-W07 (($acquired.input_mask -band 7) -eq 0 -and ($acquired.input_mask -band 16) -ne 0 -and -not (Test-W07Flag $acquired.cursor_visible)) 'actual raw/mouse/cursor transition required' }
        }
    }
    Assert-W07 ($Receipt.paste_observation -ceq 'open-not-measured') 'paste remains explicitly open'
    Assert-W07 ($Receipt.alternate_buffer -cin @('open-not-proven','native-transition-observed')) 'honest alternate-buffer classification'
    if ($Receipt.alternate_buffer -ceq 'native-transition-observed') {
        Assert-W07 ($ExpectedCase -ceq 'capability' -and $observed.Count -eq 3) 'buffer claim only in actual capability'
        Assert-W07 ((Test-W07Flag $observed[0].buffer_ok) -and (Test-W07Flag $observed[1].buffer_ok) -and (Test-W07Flag $observed[2].buffer_ok) -and (Test-W07Flag $observed[0].sentinel_seen) -and -not (Test-W07Flag $observed[1].sentinel_seen) -and (Test-W07Flag $observed[2].sentinel_seen)) 'buffer label requires native main/alternate/main observations'
    }
}

# All process/drain waits share this single elapsed clock. The controller
# reserves two seconds before the 25 s hard deadline for containment/joins.
function Get-W07WaitBudget([long] $ElapsedMs, [int] $RequestedMs) {
    Assert-W07 ($ElapsedMs -ge 0 -and $RequestedMs -ge 0) 'nonnegative deadline inputs'
    return [int][Math]::Min($RequestedMs,[Math]::Max(0,25000-$ElapsedMs))
}
function Get-W07ControllerPhase([long] $ElapsedMs) {
    Assert-W07 ($ElapsedMs -ge 0) 'nonnegative elapsed clock'
    if ($ElapsedMs -ge 25000) { return 'expired' }
    if ($ElapsedMs -ge 23000) { return 'cleanup' }
    return 'run'
}
function Test-W07PinnedRust([string] $Verbose) {
    $releases = @([regex]::Matches($Verbose,'(?m)^release: ([^\r\n]+)\r?$'))
    return $releases.Count -eq 1 -and $releases[0].Groups[1].Value -ceq '1.97.1'
}
function Assert-W07FrozenInputs([object[]] $Inputs) {
    foreach ($item in $Inputs) {
        Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath $item.path).Hash.ToLowerInvariant() -ceq $item.sha256) "prehashed build input changed: $($item.path)"
    }
}

function Get-W07InventoryBytes([string] $Directory) {
    # An empty pipeline is a valid zero-byte inventory, not an indexed result.
    [long]$bytes=0
    foreach ($file in (Get-ChildItem -LiteralPath $Directory -File -Recurse -ErrorAction Stop)) {
        [long]$length=$file.Length
        Assert-W07 ($length -ge 0 -and $bytes -le [long]::MaxValue-$length) 'inventory byte count overflow'
        $bytes+=$length
    }
    return $bytes
}
