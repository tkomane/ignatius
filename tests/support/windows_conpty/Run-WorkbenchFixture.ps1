[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $ArchivedClient,
    [Parameter(Mandatory)] [string] $ExpectedClientSha256,
    [Parameter(Mandatory)] [string] $CandidateSource,
    [Parameter(Mandatory)] [string] $CandidateBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Environment]::OSVersion.Version.Build -lt 22000 -or
    [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'Arm64' -or
    -not [IO.Path]::IsPathFullyQualified($env:RUNNER_TEMP)) {
    throw 'The workbench fixture requires the owned Windows 11 ARM64 runner.'
}

$scratch = Join-Path $env:RUNNER_TEMP ('ignatius-workbench-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $scratch
$cluster = Join-Path $scratch 'cluster'
$readyFile = Join-Path $cluster 'postmaster.pid'
$hash = 'a2cd8a774389347b70be7748ac4a37ef43cb6ee539c5fc3e85ae264b15e4a04c'
$download = Join-Path $scratch 'postgresql-17.12-1-windows-x64-binaries.zip'
$pgBin = Join-Path $scratch 'pgsql/bin'
$pgCtl = Join-Path $pgBin 'pg_ctl.exe'

try {
    # EDB publishes the reviewed hash next to the official portable ZIP.
    $fixtureDownload = @{
        Uri = "https://get.enterprisedb.com/postgresql/$([IO.Path]::GetFileName($download))"
        OutFile = $download
        MaximumRedirection = 4
        TimeoutSec = 150
    }
    Invoke-WebRequest @fixtureDownload
    if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash) {
        throw 'The PostgreSQL fixture ZIP does not match the reviewed EDB SHA-256.'
    }
    Expand-Archive -LiteralPath $download -DestinationPath $scratch
    foreach ($tool in @('postgres.exe', 'initdb.exe', 'pg_ctl.exe', 'psql.exe')) {
        $version = & (Join-Path $pgBin $tool) --version
        if ($LASTEXITCODE -ne 0 -or $version -cnotmatch '\(PostgreSQL\) 17\.12(?:\s|$)') {
            throw 'The fixture must provide the expected PostgreSQL 17.12 tools.'
        }
    }

    & (Join-Path $pgBin 'initdb.exe') -D $cluster -U ig_probe --locale=C --encoding=UTF8 --auth=trust |
        Set-Content -LiteralPath (Join-Path $scratch 'initdb.log')
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the owned PostgreSQL fixture.' }
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        $port = ([Net.IPEndPoint] $listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
    [IO.File]::AppendAllLines((Join-Path $cluster 'postgresql.conf'), [string[]] @(
        '', "port = $port", "listen_addresses = '127.0.0.1'", "unix_socket_directories = ''",
        'default_transaction_read_only = on'
    ), [Text.Encoding]::ASCII)
    [IO.File]::WriteAllLines((Join-Path $cluster 'pg_hba.conf'), [string[]] @(
        'host postgres ig_probe 127.0.0.1/32 trust'
    ), [Text.Encoding]::ASCII)

    $empty = Join-Path $scratch 'empty'
    [IO.File]::WriteAllBytes($empty, [byte[]] @())
    $serverLog = Join-Path $scratch 'postgres.log'
    $serverStart = @{
        FilePath = $pgCtl
        PassThru = $true
        NoNewWindow = $true
        WorkingDirectory = $scratch
        ArgumentList = @('start', '-D', ('"' + $cluster + '"'), '-l', ('"' + $serverLog + '"'), '-w', '-t', '25')
        RedirectStandardInput = $empty
        RedirectStandardOutput = Join-Path $scratch 'pgctl.out'
        RedirectStandardError = Join-Path $scratch 'pgctl.err'
    }
    $start = Start-Process @serverStart
    try {
        if (-not $start.WaitForExit(30000)) {
            if (-not $start.HasExited) {
                $start.Kill()
                $null = $start.WaitForExit(1000)
            }
            throw 'The owned PostgreSQL fixture did not start in time.'
        }
        if ($start.ExitCode -ne 0) { throw 'The owned PostgreSQL fixture did not start.' }
    }
    finally {
        $start.Dispose()
    }

    $identity = & (Join-Path $pgBin 'psql.exe') -XqAt -h 127.0.0.1 -p $port -U ig_probe -d postgres -c "SELECT current_setting('server_version_num'), current_setting('transaction_read_only')"
    if ($LASTEXITCODE -ne 0 -or $identity -cne '170012|on') {
        throw 'The owned PostgreSQL fixture did not report the expected read-only posture.'
    }
    [ordered] @{
        postgresql_version = '17.12'
        postgresql_zip_sha256 = $hash
        transaction_read_only = $true
    } | ConvertTo-Json -Compress | Write-Output

    $journey = @{
        ArchivedClient = $ArchivedClient
        ExpectedClientSha256 = $ExpectedClientSha256
        CandidateSource = $CandidateSource
        CandidateBuild = $CandidateBuild
        WindowCursorCalibration = $true
        WorkbenchPort = $port
    }
    & (Join-Path $PSScriptRoot 'Run-RendererCalibration.ps1') @journey
}
finally {
    if (Test-Path -LiteralPath $readyFile -PathType Leaf) {
        & $pgCtl stop -D $cluster -m fast -w -t 25 | Out-Null
        if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $readyFile)) {
            throw 'Could not confirm the owned PostgreSQL fixture stopped.'
        }
    }
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
