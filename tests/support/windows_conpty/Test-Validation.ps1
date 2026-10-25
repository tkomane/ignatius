# Portable validator/owned-file tests. No controlled children, terminal or DB.
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
# Actual tiny owned filesystem inventories, under the same StrictMode as CI.
$inventory=[IO.Path]::Combine([IO.Path]::GetTempPath(),'ignatius-w07-inventory-'+[Guid]::NewGuid().ToString('N'))
$null=[IO.Directory]::CreateDirectory($inventory)
$nested=[IO.Path]::Combine($inventory,'nested'); $deeper=[IO.Path]::Combine($nested,'deeper')
$emptyA=[IO.Path]::Combine($inventory,'empty-a'); $emptyB=[IO.Path]::Combine($inventory,'empty-b')
$one=[IO.Path]::Combine($inventory,'one.bin'); $two=[IO.Path]::Combine($nested,'two.bin'); $three=[IO.Path]::Combine($deeper,'three.bin')
try {
    Assert-W07 ((Get-W07InventoryBytes $inventory) -eq 0) 'empty owned directory inventory is zero'; ++$script:Checks
    [IO.File]::WriteAllBytes($one,[byte[]](1,2,3))
    Assert-W07 ((Get-W07InventoryBytes $inventory) -eq 3) 'one actual file inventory counts bytes'; ++$script:Checks
    $null=[IO.Directory]::CreateDirectory($deeper)
    [IO.File]::WriteAllBytes($two,[byte[]](1,2,3,4)); [IO.File]::WriteAllBytes($three,[byte[]](1,2,3,4,5))
    Assert-W07 ((Get-W07InventoryBytes $inventory) -eq 12) 'nested actual files inventory sums all bytes'; ++$script:Checks
    $null=[IO.Directory]::CreateDirectory($emptyA); $null=[IO.Directory]::CreateDirectory($emptyB)
    Assert-W07 ((Get-W07InventoryBytes $inventory) -eq 12) 'directory entries do not contribute inventory bytes'; ++$script:Checks
} finally {
    foreach ($file in @($one,$two,$three)) { [IO.File]::Delete($file) }
    foreach ($directory in @($deeper,$nested,$emptyA,$emptyB,$inventory)) { if ([IO.Directory]::Exists($directory)) { [IO.Directory]::Delete($directory) } }
}

function New-CursorDiagnostic {
    $good=@{ok=1;error=0};$none=@{ok=0;error=0};$rows=@()
    $phases=@('baseline-main','native-hidden','native-shown','main-vt-hidden','main-vt-shown','alternate-entered','alternate-vt-hidden','alternate-vt-shown','restored-main')
    $prefix=@('','','',"$([char]27)[?25l","$([char]27)[?25h","$([char]27)[?1049h","$([char]27)[?25l","$([char]27)[?25h","$([char]27)[?1049l")
    for($i=0;$i -lt 9;++$i) {
        $setter=$i -eq 1 -or $i -eq 2 -or $i -eq 8;$marker="W07C-nonce-$i";$expected=($prefix[$i]+"$([char]27)[2;1H"+$marker).Length
        $rows+=@{sequence=$i;phase=$phases[$i];actor_pid=42;query=@($good,$good,$good,$good);setter_attempted=[int]$setter;setter=$(if($setter){$good}else{$none});write=$good;fence=$good;written=$expected;expected=$expected;input_mask=423;output_mask=7;cursor_size=25;cursor_visible=[int]($i -ne 1 -and $i -ne 3 -and $i -ne 6);sentinel_expected=9;sentinel_read=9;sentinel_seen=[int]($i -lt 5 -or $i -eq 8);marker_seen=1;marker=$marker;observed_qpc=(1220+$i*2);acknowledged_qpc=(1221+$i*2)}
    }
    $r=@{schema=1;case='cursor-routing';native_windows=$true;diagnostic_complete=$true;product_acceptance=$false;cursor_capability_accepted=$false;failure='';actor_pid=42;child_pid=41;actor_exit=0;observer_registrations=1;attachment_attestations=1;actor_waited=1;exact_job_actor=1;child_exit=0;worker_exit=0;exact_job_worker=1;exact_job_guard=1;guard_waited=1;worker_waited=1;cleanup_confirmed=1;control_io_cleanup_confirmed=1;job_active_processes=0;output_seen=100;output_kept=100;output_error=0;control_bytes=1500;qpc_frequency=1000;clocks=@{start=1000;registered=1100;resume_ack=1200;guard_wait=1700;cleanup=1800;end=1900};writer_route='WriteConsoleW retained CONOUT$';handle_route='one retained CONIN$/CONOUT$ actor';fence_meaning='parser-buffer-only';sensor_capabilities=@{native_setter=$true;main_vt=$true;alternate_vt=$true};initialization=@{api=@($good,$good,$good,$good,$good,$good,$good);written=9;expected=9;input_mask=423;output_mask=7;cursor_size=25;observed_qpc=1210};stages=$rows;restoration=@{api=@($good,$good,$good);verification=@($good,$good,$good);input_mask=423;output_mask=7;cursor_size=25;cursor_visible=1;verified_qpc=1235;baseline_restored=1;observed_qpc=1260};paste_observation='open-not-measured'}
    return($r|ConvertTo-Json -Depth 12|ConvertFrom-Json)
}
function Bad-Cursor([string]$Name,[scriptblock]$Change) {
    $r=New-CursorDiagnostic;&$Change $r;$rejected=$false
    try{Assert-W07CursorDiagnostic $r 'nonce'}catch{$rejected=$true}
    Assert-W07 $rejected "cursor false completion: $Name";++$script:Checks
}
Assert-W07CursorDiagnostic (New-CursorDiagnostic) 'nonce';++$script:Checks
Bad-Cursor 'wrong baseline nonce sentinel count' {param($r)$r.initialization.written=8;$r.initialization.expected=8}
Bad-Cursor 'setup deadline overrun' {param($r)$r.clocks.guard_wait=22000;$r.clocks.cleanup=22001;$r.clocks.end=22002}
Bad-Cursor 'outer deadline overrun' {param($r)$r.clocks.end=26001}
Bad-Cursor 'same actor as Rust parent' {param($r)$r.child_pid=$r.actor_pid}
Bad-Cursor 'coherent cursor size differs from seeded baseline' {param($r)$r.stages[0].cursor_size=75;$r.stages[8].cursor_size=75}
Bad-Cursor 'wrong actor' {param($r)$r.stages[3].actor_pid=43}
Bad-Cursor 'missing final baseline' {param($r)$r.stages[8].cursor_visible=0}
Bad-Cursor 'cleanup unconfirmed' {param($r)$r.cleanup_confirmed=0}

Bad-Cursor 'diagnostic completion false' {param($r)$r.diagnostic_complete=$false}
Bad-Cursor 'wrong phase' {param($r)$r.stages[5].sequence=4}
Bad-Cursor 'missing stage' {param($r)$r.stages=$r.stages[0..7]}
Bad-Cursor 'second actor registration' {param($r)$r.observer_registrations=2}
Bad-Cursor 'unconfirmed actor identity' {param($r)$r.exact_job_actor=0}
Bad-Cursor 'API error despite true flag' {param($r)$r.stages[3].query[2].error=5}
Bad-Cursor 'partial write' {param($r)$r.stages[6].written--}
Bad-Cursor 'missing marker' {param($r)$r.stages[6].marker_seen=0}
Bad-Cursor 'wrong phase marker' {param($r)$r.stages[6].marker='W07C-other-6'}
Bad-Cursor 'restoration API failure' {param($r)$r.restoration.api[2].ok=0}
Bad-Cursor 'missing final restoration' {param($r)$r.restoration.baseline_restored=0}
Bad-Cursor 'truncated output' {param($r)$r.output_kept=99}
Bad-Cursor 'capture overflow' {param($r)$r.control_bytes=65537}
Bad-Cursor 'product success claimed' {param($r)$r.product_acceptance=$true}
Bad-Cursor 'sensor success forged' {param($r)$r.sensor_capabilities.alternate_vt=$false}
Bad-Cursor 'reordered acknowledgement' {param($r)$r.stages[6].acknowledged_qpc=1}
$constant=New-CursorDiagnostic;foreach($row in $constant.stages){$row.cursor_visible=1};$constant.sensor_capabilities.native_setter=$false;$constant.sensor_capabilities.main_vt=$false;$constant.sensor_capabilities.alternate_vt=$false
Assert-W07CursorDiagnostic $constant 'nonce';++$script:Checks
Assert-W07Selection $true $false;++$script:Checks
Assert-W07Selection $false $true;++$script:Checks
foreach($selection in @(@($false,$false),@($true,$true))){$rejected=$false;try{Assert-W07Selection $selection[0] $selection[1]}catch{$rejected=$true};Assert-W07 $rejected 'no implicit or combined selection';++$script:Checks}
Bad-Cursor 'TRUE sentinel short read' {param($r)$r.stages[6].sentinel_read=8}
Bad-Cursor 'restoration verification API failed despite good stage8' {param($r)$r.restoration.verification[0].ok=0;$r.restoration.verification[0].error=5}
$different=New-CursorDiagnostic;$different.stages[6].sentinel_seen=0
Assert-W07CursorDiagnostic $different 'nonce';++$script:Checks
for($i=0;$i -lt 9;++$i){$index=$i;Bad-Cursor 'every stage refuses TRUE-short' {param($r)$r.stages[$index].sentinel_read--}}
Bad-Cursor 'sentinel requested count differs from full nonce' {param($r)$r.stages[6].sentinel_expected=8;$r.stages[6].sentinel_read=8}
for($i=0;$i -lt 3;++$i){$index=$i;Bad-Cursor 'every restoration verification API outcome' {param($r)$r.restoration.verification[$index].ok=0;$r.restoration.verification[$index].error=5}}
Bad-Cursor 'independent restoration input differs despite good stage8' {param($r)$r.restoration.input_mask=152}
Bad-Cursor 'independent restoration output differs despite good stage8' {param($r)$r.restoration.output_mask=3}
Bad-Cursor 'independent restoration cursor size differs despite good stage8' {param($r)$r.restoration.cursor_size=75}
Bad-Cursor 'independent restoration visibility differs despite good stage8' {param($r)$r.restoration.cursor_visible=0}
Bad-Cursor 'verification clock substituted by later stage8' {param($r)$r.restoration.verified_qpc=1237}
Write-Output "$script:Checks receipt/deadline/identity/inventory/cursor checks passed; no Windows/native execution"
