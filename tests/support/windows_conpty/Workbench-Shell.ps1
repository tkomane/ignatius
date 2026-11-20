[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Archive,
    [Parameter(Mandatory)] [int] $Port
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Wait-ActorSignal([string] $Name) {
    $path = Join-Path $PSScriptRoot $Name
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while (-not [IO.File]::Exists($path) -and $watch.Elapsed.TotalSeconds -lt 22) {
        Start-Sleep -Milliseconds 20
    }
    if (-not [IO.File]::Exists($path)) { throw "Missing owned actor signal $Name." }
}

function Publish-ActorValue([string] $Name, [string] $Text) {
    $pending = Join-Path $PSScriptRoot "$Name.pending"
    [IO.File]::WriteAllText($pending, $Text, [Text.Encoding]::ASCII)
    [IO.File]::Move($pending, (Join-Path $PSScriptRoot $Name))
}

try {
    if ($PSVersionTable.PSVersion.Major -ne 7 -or
        [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'Arm64' -or
        $Port -le 0 -or $Port -gt 65535) {
        throw 'Expected native PowerShell 7 and an owned loopback fixture port.'
    }
    Set-PSReadLineOption -HistorySaveStyle SaveNothing
    function global:prompt { 'W07-PS> ' }
    Publish-ActorValue 'shell-ready' "$($PSVersionTable.PSVersion)"
    Wait-ActorSignal 'shell-launch'
    $uri = "postgres://ig_probe@127.0.0.1:$Port/postgres?sslmode=disable&connect_timeout=3"
    $start = @{
        FilePath = $Archive
        WorkingDirectory = $PSScriptRoot
        NoNewWindow = $true
        PassThru = $true
        ArgumentList = @('connect', '--read-only', '--no-history', $uri)
    }
    $client = Start-Process @start
    try {
        # The actor duplicates the client handle from the known shell process.
        $value = $client.SafeHandle.DangerousGetHandle().ToInt64()
        Publish-ActorValue 'shell-client' "$($client.Id) $value"
        if (-not $client.WaitForExit(22000) -or $client.ExitCode -ne 0) {
            throw 'The owned workbench did not quit successfully.'
        }
    }
    finally {
        $client.Dispose()
    }
    Wait-ActorSignal 'shell-return'
    # -NoExit now reads the next command from the same console.
}
catch {
    Write-Error -ErrorAction Continue $_
    exit 125
}
