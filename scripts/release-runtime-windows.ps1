# Tests an already verified, extracted release binary against one owned
# PostgreSQL cluster. Only the metadata receipt leaves the scratch directory.
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $BinaryPath,
    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $ExpectedSourceRevision,
    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9A-Za-z._/-]+$')]
    [string] $ExpectedBuildIdentity,
    [Parameter(Mandatory)]
    [string] $EvidencePath,
    [ValidatePattern('^[0-9a-fA-F]{64}$')]
    [string] $ExpectedBinarySha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$probeEnvironmentPattern = '^(PG|IGNATIUS_|OPENSSL_)|^RUST_LOG$'

function Assert-Probe {
    param([bool] $Condition, [string] $Message)

    if (-not $Condition) {
        throw $Message
    }
}

function Invoke-ProbeProcess {
    param(
        [string] $Stage,
        [string] $ProgramPath,
        [string[]] $Arguments,
        [string] $WorkingDirectory,
        [System.Collections.IDictionary] $Variables = @{},
        [int] $TimeoutSeconds = 20
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new($ProgramPath)
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) {
        $startInfo.ArgumentList.Add($argument)
    }
    foreach ($variableName in @($startInfo.Environment.Keys)) {
        if ($variableName -match $probeEnvironmentPattern) {
            $null = $startInfo.Environment.Remove($variableName)
        }
    }
    foreach ($variableName in $Variables.Keys) {
        $startInfo.Environment[$variableName] = $Variables[$variableName]
    }

    $child = [Diagnostics.Process]::new()
    $child.StartInfo = $startInfo
    try {
        Assert-Probe ($child.Start()) "$Stage could not start"
        $child.StandardInput.Close()
        $stdout = $child.StandardOutput.ReadToEndAsync()
        $stderr = $child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(1000 * $TimeoutSeconds)) {
            if (-not $child.HasExited) {
                $child.Kill($true)
                $null = $child.WaitForExit(3000)
            }
            throw "$Stage exceeded its deadline"
        }
        $drains = [Threading.Tasks.Task[]] @($stdout, $stderr)
        Assert-Probe ([Threading.Tasks.Task]::WaitAll($drains, 2000)) "$Stage output did not close"
        Assert-Probe ($stdout.Result.Length -le 65536 -and $stderr.Result.Length -le 65536) "$Stage output exceeded the receipt limit"
        return [pscustomobject] @{ ExitCode = $child.ExitCode; Out = $stdout.Result; Error = $stderr.Result }
    }
    finally {
        $child.Dispose()
    }
}

function Assert-DebugPrivacy {
    param([string] $Log, [string[]] $Forbidden)

    Assert-Probe ($Log -cmatch '\bjob=1 statement_chars=[1-9][0-9]*\b') 'client log is missing the value-free statement descriptor'
    foreach ($marker in $Forbidden) {
        Assert-Probe (-not $Log.Contains($marker, [StringComparison]::Ordinal)) 'client debug log contains a forbidden marker'
    }
}

function New-ProbeCertificates {
    param([string] $OpenSsl, [string] $WorkRoot, [string] $Cluster)

    $stage = 'prepare synthetic certificate chain'
    $opensslConfig = Join-Path $workRoot 'openssl.cnf'
    [IO.File]::WriteAllLines($opensslConfig, [string[]] @(
        '[req]', 'distinguished_name=dn', '[dn]',
        '[probe_ca]', 'basicConstraints=critical,CA:TRUE',
        'keyUsage=critical,keyCertSign,cRLSign',
        '[probe_server]', 'basicConstraints=critical,CA:FALSE',
        'keyUsage=critical,digitalSignature,keyEncipherment',
        'extendedKeyUsage=serverAuth', 'subjectAltName=DNS:localhost'
    ), [Text.Encoding]::ASCII)
    $caCertificate = Join-Path $workRoot 'ca.crt'
    $caPrivateKey = Join-Path $workRoot 'ca.key'
    $serverRequest = Join-Path $workRoot 'server.csr'
    $serverCertificate = Join-Path $cluster 'server.crt'
    $serverPrivateKey = Join-Path $cluster 'server.key'
    $ca = Invoke-ProbeProcess $stage $openssl @(
        'req', '-new', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '2', '-sha256',
        '-config', $opensslConfig, '-extensions', 'probe_ca', '-subj', '/CN=Ignatius disposable W12 CA',
        '-keyout', $caPrivateKey, '-out', $caCertificate
    ) $workRoot
    Assert-Probe ($ca.ExitCode -eq 0) 'Could not generate disposable CA'
    $csr = Invoke-ProbeProcess $stage $openssl @(
        'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-sha256', '-subj', '/CN=localhost',
        '-config', $opensslConfig, '-keyout', $serverPrivateKey, '-out', $serverRequest
    ) $workRoot
    Assert-Probe ($csr.ExitCode -eq 0) 'Could not create disposable server certificate request'
    $leaf = Invoke-ProbeProcess $stage $openssl @(
        'x509', '-req', '-in', $serverRequest, '-days', '2', '-sha256',
        '-CA', $caCertificate, '-CAkey', $caPrivateKey, '-CAcreateserial',
        '-extfile', $opensslConfig, '-extensions', 'probe_server', '-out', $serverCertificate
    ) $workRoot
    Assert-Probe ($leaf.ExitCode -eq 0) 'Could not sign disposable DNS-only server certificate'
    return $caCertificate
}

function Get-LoopbackPort {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([Net.IPEndPoint] $listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

Assert-Probe $IsWindows 'This probe requires an owned Windows runner'
Assert-Probe ([IO.Path]::IsPathFullyQualified($BinaryPath)) 'BinaryPath must be absolute'
Assert-Probe ([IO.Path]::IsPathFullyQualified($EvidencePath)) 'EvidencePath must be absolute'
Assert-Probe (-not [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) 'RUNNER_TEMP is required'
Assert-Probe ([IO.Path]::IsPathFullyQualified($env:RUNNER_TEMP) -and [IO.Directory]::Exists($env:RUNNER_TEMP)) 'RUNNER_TEMP must identify an existing absolute directory'
Assert-Probe (-not [string]::IsNullOrWhiteSpace($env:PGBIN)) 'Expected preinstalled PostgreSQL PGBIN'
Assert-Probe ([IO.Path]::IsPathFullyQualified($env:PGBIN) -and [IO.Directory]::Exists($env:PGBIN)) 'PGBIN must identify the preinstalled PostgreSQL bin directory'

$binary = [IO.Path]::GetFullPath($BinaryPath)
$evidenceFile = [IO.Path]::GetFullPath($EvidencePath)
Assert-Probe ([IO.File]::Exists($binary) -and [IO.Path]::GetFileName($binary) -ieq 'ignatius.exe') 'Expected an extracted ignatius.exe'
Assert-Probe ([IO.Directory]::Exists([IO.Path]::GetDirectoryName($evidenceFile))) 'Evidence directory must already exist'
Assert-Probe (-not (Test-Path -LiteralPath $evidenceFile)) 'Evidence path is already used'

$initdb = Join-Path $env:PGBIN 'initdb.exe'
$pgCtl = Join-Path $env:PGBIN 'pg_ctl.exe'
$postgres = Join-Path $env:PGBIN 'postgres.exe'
$openssl = Join-Path $env:ProgramFiles 'OpenSSL/bin/openssl.exe'
foreach ($requiredFile in @($initdb, $pgCtl, $postgres, $openssl)) {
    Assert-Probe ([IO.File]::Exists($requiredFile)) 'Required preinstalled runner executable is unavailable'
}

$receipt = [ordered] @{
    schema_version = 1
    scope = 'extracted Windows archive, disposable loopback PostgreSQL'
    status = 'fail'
    target = 'x86_64-pc-windows-msvc'
    source_revision = $ExpectedSourceRevision
    build_identity = $ExpectedBuildIdentity
    binary_sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    runner = [ordered] @{
        os = [Runtime.InteropServices.RuntimeInformation]::OSDescription
        architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        image_os = $env:ImageOS
        image_version = $env:ImageVersion
    }
    postgres = $null
    openssl = $null
    checks = [ordered] @{
        archive_binary_identity = $false
        read_only_verify_full = $false
        certificate_hostname_refusal_exit = $null
        debug_log_privacy = $false
        fixture_cleanup = $false
    }
    failure_stage = $null
}

if ($ExpectedBinarySha256) {
    Assert-Probe ($receipt.binary_sha256 -ieq $ExpectedBinarySha256) 'Extracted binary digest differs from the expected digest'
}
$workRoot = Join-Path $env:RUNNER_TEMP ('ignatius-w12-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $workRoot
$cluster = Join-Path $workRoot 'cluster'
$pidFile = Join-Path $cluster 'postmaster.pid'
$clusterStartAttempted = $false
$failure = $null
$stage = 'binary identity'

try {
    $ownedConfig = Join-Path $workRoot 'client-config'
    $ownedData = Join-Path $workRoot 'client-data'
    $clientEnv = @{
        IGNATIUS_CONFIG_DIR = $ownedConfig
        IGNATIUS_DATA_DIR = $ownedData
        IGNATIUS_LOG = 'off'
    }
    $version = Invoke-ProbeProcess $stage $binary @('version', '--verbose') $workRoot $clientEnv
    $revisionPattern = '(?m)^  source revision: ' + [regex]::Escape($ExpectedSourceRevision) + ' \(clean\)\r?$'
    $identityPattern = '(?m)^  build identity:  ' + [regex]::Escape($ExpectedBuildIdentity) + '\r?$'
    $targetPattern = '(?m)^  target:          x86_64-pc-windows-msvc\r?$'
    Assert-Probe ($version.ExitCode -eq 0 -and $version.Out -match $revisionPattern -and
        $version.Out -cmatch $identityPattern -and $version.Out -cmatch $targetPattern) 'Archived binary identity differs from the requested release evidence'
    $receipt.checks.archive_binary_identity = $true

    $stage = 'runner database and TLS tools'
    $pgVersion = Invoke-ProbeProcess $stage $postgres @('--version') $workRoot
    $opensslVersion = Invoke-ProbeProcess $stage $openssl @('version') $workRoot
    Assert-Probe ($pgVersion.ExitCode -eq 0 -and $pgVersion.Out -cmatch '^postgres \(PostgreSQL\) [0-9]+\.[0-9]+') 'Preinstalled postgres version probe failed'
    Assert-Probe ($opensslVersion.ExitCode -eq 0 -and $opensslVersion.Out -cmatch '^OpenSSL [0-9]+\.') 'Preinstalled OpenSSL version probe failed'
    $receipt.postgres = $pgVersion.Out.Trim()
    $receipt.openssl = $opensslVersion.Out.Trim()

    $stage = 'initialize owned cluster'
    $bootstrapUser = 'ignatius_probe'
    $fixturePassword = 'fixture-' + [Guid]::NewGuid().ToString('N')
    $passwordFile = Join-Path $workRoot 'initdb-password'
    [IO.File]::WriteAllLines($passwordFile, [string[]] @($fixturePassword), [Text.Encoding]::ASCII)
    $initialized = Invoke-ProbeProcess $stage $initdb @(
        '-D', $cluster, '-U', $bootstrapUser, '--encoding=UTF8', '--locale=C',
        '--auth=scram-sha-256', '--pwfile', $passwordFile
    ) $workRoot -TimeoutSeconds 45
    Assert-Probe ($initialized.ExitCode -eq 0) 'Could not initialize the owned PostgreSQL cluster'

    $stage = 'prepare synthetic certificate chain'
    $caCertificate = New-ProbeCertificates $openssl $workRoot $cluster

    $stage = 'start owned loopback cluster with SSL'
    $serverPort = Get-LoopbackPort
    [IO.File]::AppendAllLines((Join-Path $cluster 'postgresql.conf'), [string[]] @(
        '', "listen_addresses = '127.0.0.1,::1'", "port = $serverPort", 'ssl = on',
        "ssl_cert_file = 'server.crt'", "ssl_key_file = 'server.key'", "unix_socket_directories = ''"
    ), [Text.Encoding]::ASCII)
    [IO.File]::WriteAllLines((Join-Path $cluster 'pg_hba.conf'), [string[]] @(
        "hostssl postgres $bootstrapUser 127.0.0.1/32 scram-sha-256",
        "hostssl postgres $bootstrapUser ::1/128 scram-sha-256"
    ), [Text.Encoding]::ASCII)
    $clusterStartAttempted = $true
    $serverLog = Join-Path $workRoot 'postgres.log'
    $emptyInput = Join-Path $workRoot 'pg-ctl.stdin'
    [IO.File]::WriteAllBytes($emptyInput, [byte[]] @())
    $fixtureEnvironment = @{}
    foreach ($variableName in [Environment]::GetEnvironmentVariables().Keys) {
        if ($variableName -match $probeEnvironmentPattern) {
            $fixtureEnvironment[$variableName] = $null
        }
    }
    # The Windows launcher can keep inherited handles until the server stops.
    $started = Start-Process -FilePath $pgCtl -WorkingDirectory $workRoot -Environment $fixtureEnvironment `
        -ArgumentList @('start', '-D', ('"' + $cluster + '"'), '-l', ('"' + $serverLog + '"'), '-w', '-t', '25') `
        -RedirectStandardInput $emptyInput -RedirectStandardOutput (Join-Path $workRoot 'pg-ctl.stdout') `
        -RedirectStandardError (Join-Path $workRoot 'pg-ctl.stderr') -NoNewWindow -PassThru
    try {
        if (-not $started.WaitForExit(32000)) {
            if (-not $started.HasExited) {
                $started.Kill($true)
                $null = $started.WaitForExit(3000)
            }
            throw 'Owned PostgreSQL startup exceeded its deadline'
        }
        $startExit = $started.ExitCode
        Assert-Probe ($startExit -eq 0) "Owned PostgreSQL cluster did not start with SSL (pg_ctl exit $startExit)"
    }
    finally {
        $started.Dispose()
    }

    $stage = 'exact archive verify-full read-only SELECT and debug logging'
    $passfile = Join-Path $workRoot 'pgpass.conf'
    [IO.File]::WriteAllLines($passfile, [string[]] @(
        ('localhost:{0}:postgres:{1}:{2}' -f $serverPort, $bootstrapUser, $fixturePassword),
        ('127.0.0.1:{0}:postgres:{1}:{2}' -f $serverPort, $bootstrapUser, $fixturePassword)
    ), [Text.Encoding]::ASCII)
    $clientEnv.PGPASSFILE = $passfile
    $clientEnv.PGSSLROOTCERT = $caCertificate
    $clientEnv.IGNATIUS_LOG = 'debug'
    $uriFormat = 'postgres://{0}@{1}:{2}/postgres?sslmode=verify-full&connect_timeout=5'
    $verifiedUri = $uriFormat -f $bootstrapUser, 'localhost', $serverPort
    $sqlMarker = 'w12-sql-' + [Guid]::NewGuid().ToString('N')
    $rowMarker = 'w12-row-' + [Guid]::NewGuid().ToString('N')
    $query = "SELECT /* $sqlMarker */ '$rowMarker' AS probe, ssl::text AS tls, current_setting('transaction_read_only') AS read_only FROM pg_catalog.pg_stat_ssl WHERE pid = pg_backend_pid()"
    $verified = Invoke-ProbeProcess $stage $binary @('query', $verifiedUri, '--read-only', '--no-history', '-c', $query, '--format', 'csv') $workRoot $clientEnv
    Assert-Probe ($verified.ExitCode -eq 0) 'Packaged client failed verify-full SELECT'
    $expectedCsv = [string]::Join([char] 10, @('probe,tls,read_only', "$rowMarker,true,on", ''))
    Assert-Probe (($verified.Out -replace '\r\n', [string] [char] 10) -ceq $expectedCsv) 'Packaged client did not prove server-side TLS and read-only state'
    $receipt.checks.read_only_verify_full = $true

    $stage = 'debug log privacy'
    $logFile = Join-Path $ownedData 'logs/ignatius.log'
    Assert-Probe ([IO.File]::Exists($logFile)) 'Packaged client did not emit its debug log'
    $forbiddenMarkers = @($sqlMarker, $rowMarker, $fixturePassword, 'tokio_postgres')
    Assert-DebugPrivacy ([IO.File]::ReadAllText($logFile)) $forbiddenMarkers
    foreach ($marker in $forbiddenMarkers) {
        Assert-Probe (-not $verified.Error.Contains($marker, [StringComparison]::Ordinal)) 'client debug stderr contains a forbidden marker'
    }
    $receipt.checks.debug_log_privacy = $true

    $stage = 'numeric certificate hostname mismatch'
    $clientEnv.IGNATIUS_LOG = 'off'
    $mismatchUri = $uriFormat -f $bootstrapUser, '127.0.0.1', $serverPort
    # The check report collapses connection failures into exit 4. Query keeps
    # the typed TLS exit code and never sends its SQL when name verification fails.
    $refused = Invoke-ProbeProcess $stage $binary @('query', $mismatchUri, '--read-only', '--no-history', '-c', 'SELECT 1') $workRoot $clientEnv
    $receipt.checks.certificate_hostname_refusal_exit = $refused.ExitCode
    Assert-Probe ($refused.ExitCode -eq 6 -and $refused.Out.Length -eq 0 -and
        $refused.Error -match 'certificate (not valid for name|does not cover the name)' -and
        $refused.Error.Contains('127.0.0.1')) 'Expected a TLS hostname error and empty query output for the numeric address'
}
catch {
    $receipt.failure_stage = $stage
    $failure = $_.Exception.Message
}
finally {
    try {
        if ($clusterStartAttempted -and [IO.File]::Exists($pidFile)) {
            $ownerRecord = [IO.File]::ReadAllLines($pidFile)
            Assert-Probe ($ownerRecord.Length -ge 2 -and
                $ownerRecord[0] -match '^[1-9][0-9]*$' -and
                [IO.Path]::IsPathFullyQualified($ownerRecord[1])) 'Owned cluster PID record is invalid'
            $recordedCluster = [IO.Path]::GetFullPath($ownerRecord[1])
            Assert-Probe ($recordedCluster.Equals([IO.Path]::GetFullPath($cluster), [StringComparison]::OrdinalIgnoreCase)) 'Owned cluster PID record did not identify the scratch data directory'
            $stopped = Invoke-ProbeProcess 'stop owned cluster' $pgCtl @(
                'stop', '-D', $cluster, '-m', 'fast', '-w', '-t', '25'
            ) $workRoot -TimeoutSeconds 32
            Assert-Probe ($stopped.ExitCode -eq 0 -and -not [IO.File]::Exists($pidFile)) 'Could not confirm owned PostgreSQL shutdown'
        }
        if ($clusterStartAttempted) {
            $status = Invoke-ProbeProcess 'verify owned cluster shutdown' $pgCtl @('status', '-D', $cluster) $workRoot
            Assert-Probe ($status.ExitCode -eq 3 -and -not [IO.File]::Exists($pidFile)) 'Owned cluster was not confirmed stopped'
        }
        Remove-Item -LiteralPath $workRoot -Recurse -Force
        $receipt.checks.fixture_cleanup = $true
    }
    catch {
        if ($null -eq $failure) {
            $receipt.failure_stage = 'fixture cleanup'
        }
        $cleanupError = 'Owned PostgreSQL scratch cleanup could not be confirmed'
        $failure = if ($null -eq $failure) { $cleanupError } else { "$failure. $cleanupError" }
    }
}

if ($null -eq $failure) {
    $receipt.status = 'pass'
}
$receiptStream = [IO.File]::Open($evidenceFile, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
try {
    $serialized = ConvertTo-Json -InputObject $receipt -Depth 5
    $receiptBytes = [Text.Encoding]::UTF8.GetBytes($serialized + [Environment]::NewLine)
    $receiptStream.Write($receiptBytes, 0, $receiptBytes.Length)
}
finally {
    $receiptStream.Dispose()
}
if ($null -ne $failure) {
    throw $failure
}
Write-Output 'Verified packaged Windows client TLS, read-only SELECT, hostname refusal and debug log privacy.'
