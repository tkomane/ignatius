using namespace System.Security.Cryptography
using namespace System.Security.Cryptography.X509Certificates

[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $ArchivedClient,
    [Parameter(Mandatory)] [string] $ExpectedClientSha256,
    [Parameter(Mandatory)] [string] $CandidateSource,
    [Parameter(Mandatory)] [string] $CandidateBuild,
    [switch] $WorkbenchPerformance,
    [switch] $WorkbenchGrid
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Environment]::OSVersion.Version.Build -lt 22000 -or
    [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'Arm64' -or
    -not [IO.Path]::IsPathFullyQualified($env:RUNNER_TEMP)) {
    throw 'The workbench fixture requires the owned Windows 11 ARM64 runner.'
}
if ($WorkbenchPerformance -and ($CandidateBuild -cne 'ci/44681770913-1' -or
    $CandidateSource -cne '61425c34097020ecec3ffead0bec7d4251680814')) {
    throw 'The workbench timings require the exact retained candidate.'
}
if ($WorkbenchGrid -and -not $WorkbenchPerformance) {
    throw 'Retained-grid timing requires the exact workbench performance mode.'
}

$scratch = Join-Path $env:RUNNER_TEMP ('ignatius-workbench-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $scratch
$cluster = Join-Path $scratch 'cluster'
$readyFile = Join-Path $cluster 'postmaster.pid'
$hash = 'a2cd8a774389347b70be7748ac4a37ef43cb6ee539c5fc3e85ae264b15e4a04c'
$download = Join-Path $scratch 'postgresql-17.12-1-windows-x64-binaries.zip'
$pgBin = Join-Path $scratch 'pgsql/bin'
$pgCtl = Join-Path $pgBin 'pg_ctl.exe'
$startAttempted = $false

function Confirm-ClusterOwner {
    $owner = [IO.File]::ReadAllLines($readyFile)
    if ($owner.Length -lt 2 -or $owner[0] -cnotmatch '\A[1-9][0-9]*\z' -or
        -not [IO.Path]::IsPathFullyQualified($owner[1]) -or
        [IO.Path]::GetFullPath($owner[1]) -ine [IO.Path]::GetFullPath($cluster)) {
        throw 'The PostgreSQL PID record must identify the owned cluster.'
    }
}

function New-WorkbenchCertificates {
    param([string] $Scratch, [string] $Cluster)

    $caKey = [RSA]::Create(2048)
    $serverKey = $null
    $ca = $null
    $server = $null
    try {
        $serverKey = [RSA]::Create(2048)
        $now = [DateTimeOffset]::UtcNow
        $rsaPadding = [RSASignaturePadding]::Pkcs1
        $sha256 = [HashAlgorithmName]::SHA256
        $signer = [X509SignatureGenerator]::CreateForRSA($caKey, $rsaPadding)
        $caRequest = [CertificateRequest]::new('CN=Ignatius disposable workbench CA', $caKey,
            $sha256, $rsaPadding)
        $caRequest.CertificateExtensions.Add([X509BasicConstraintsExtension]::new($true, $true, 0, $true))
        $caUsage = [X509KeyUsageFlags]::KeyCertSign -bor [X509KeyUsageFlags]::CrlSign
        $caRequest.CertificateExtensions.Add([X509KeyUsageExtension]::new($caUsage, $true))
        $ca = $caRequest.Create($caRequest.SubjectName, $signer, $now.AddMinutes(-5), $now.AddDays(1),
            [RandomNumberGenerator]::GetBytes(16))

        $serverRequest = [CertificateRequest]::new('CN=localhost', $serverKey,
            $sha256, $rsaPadding)
        $serverRequest.CertificateExtensions.Add([X509BasicConstraintsExtension]::new($false, $false, 0, $true))
        $serverUsage = [X509KeyUsageFlags]::DigitalSignature -bor [X509KeyUsageFlags]::KeyEncipherment
        $serverRequest.CertificateExtensions.Add([X509KeyUsageExtension]::new($serverUsage, $true))
        $serverAuth = [OidCollection]::new()
        $null = $serverAuth.Add([Oid]::new('1.3.6.1.5.5.7.3.1'))
        $serverRequest.CertificateExtensions.Add([X509EnhancedKeyUsageExtension]::new($serverAuth, $false))
        $names = [SubjectAlternativeNameBuilder]::new()
        $names.AddDnsName('localhost')
        $serverRequest.CertificateExtensions.Add($names.Build())
        $server = $serverRequest.Create($ca.SubjectName, $signer, $now.AddMinutes(-4), $now.AddHours(1),
            [RandomNumberGenerator]::GetBytes(16))

        $caPath = Join-Path $Scratch 'ca.crt'
        [IO.File]::WriteAllText($caPath, $ca.ExportCertificatePem() + "`n", [Text.Encoding]::ASCII)
        [IO.File]::WriteAllText((Join-Path $Cluster 'server.crt'), $server.ExportCertificatePem() + "`n", [Text.Encoding]::ASCII)
        [IO.File]::WriteAllText((Join-Path $Cluster 'server.key'), $serverKey.ExportPkcs8PrivateKeyPem() + "`n", [Text.Encoding]::ASCII)
        return $caPath
    }
    finally {
        if ($null -ne $server) { $server.Dispose() }
        if ($null -ne $ca) { $ca.Dispose() }
        if ($null -ne $serverKey) { $serverKey.Dispose() }
        $caKey.Dispose()
    }
}

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
    $caPath = New-WorkbenchCertificates -Scratch $scratch -Cluster $cluster
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        $port = ([Net.IPEndPoint] $listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
    [IO.File]::AppendAllLines((Join-Path $cluster 'postgresql.conf'), [string[]] @(
        '', "port = $port", "listen_addresses = '127.0.0.1,::1'", "unix_socket_directories = ''",
        'default_transaction_read_only = on', 'ssl = on',
        "ssl_cert_file = 'server.crt'", "ssl_key_file = 'server.key'"
    ), [Text.Encoding]::ASCII)
    [IO.File]::WriteAllLines((Join-Path $cluster 'pg_hba.conf'), [string[]] @(
        'hostssl postgres ig_probe 127.0.0.1/32 trust',
        'hostssl postgres ig_probe ::1/128 trust'
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
    $startAttempted = $true
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

    Confirm-ClusterOwner
    $fixtureUri = "postgres://ig_probe@localhost:$port/postgres?sslmode=verify-full&connect_timeout=5"
    $rootBefore = $env:PGSSLROOTCERT
    try {
        $env:PGSSLROOTCERT = $caPath
        $identity = & (Join-Path $pgBin 'psql.exe') -XwqAt $fixtureUri -c "SELECT current_setting('server_version_num'), current_setting('transaction_read_only'), ssl::text FROM pg_catalog.pg_stat_ssl WHERE pid = pg_backend_pid()"
        if ($LASTEXITCODE -ne 0 -or $identity -cne '170012|on|true') {
            throw 'The owned PostgreSQL fixture did not confirm PostgreSQL 17.12, verified TLS and read-only mode.'
        }
        if ($WorkbenchPerformance) {
            # Seed only this newly created loopback cluster. The client connections retain the read-only default.
            $catalogue = @'
BEGIN READ WRITE;
CREATE SCHEMA perf_aux;
DO $$
DECLARE ns text;
BEGIN
  FOREACH ns IN ARRAY ARRAY['public', 'perf_aux'] LOOP
    FOR n IN 1..15 LOOP
      EXECUTE format(
        'CREATE TABLE %I.%I (c0 int,c1 text,c2 text,c3 text,c4 text,c5 text,c6 text,c7 text,c8 text,c9 text)',
        ns, 'perf_catalog_' || lpad(n::text, 2, '0'));
    END LOOP;
  END LOOP;
END $$;
COMMIT;
'@
            & (Join-Path $pgBin 'psql.exe') -XwqAt $fixtureUri -v ON_ERROR_STOP=1 -c $catalogue
            if ($LASTEXITCODE -ne 0) { throw 'Could not seed the owned synthetic catalogue.' }
            $shape = & (Join-Path $pgBin 'psql.exe') -XwqAt $fixtureUri -c @'
SELECT count(DISTINCT table_schema), count(DISTINCT (table_schema, table_name)), count(*)
FROM information_schema.columns
WHERE table_schema IN ('public', 'perf_aux') AND table_name ~ '^perf_catalog_[0-9]{2}$'
'@
            if ($LASTEXITCODE -ne 0 -or $shape -cne '2|30|300') {
                throw 'Expected two schemas, 30 synthetic tables and 300 columns.'
            }
        }
        if ($WorkbenchGrid) {
            $grid = @'
BEGIN READ WRITE;
CREATE TABLE public.perf_grid AS
SELECT i AS source_id, 'row' || lpad(i::text, 4, '0') AS row_code,
       (613*i) % 1000 AS sort_key,
       CASE WHEN i % 10 = 1 THEN 'hit#' ELSE 'other' END AS bucket,
       repeat(chr(30028) || chr(34920), 12) AS wide_a,
       repeat(chr(26684) || chr(23376), 12) AS wide_b,
       CASE i % 3 WHEN 1 THEN NULL WHEN 2 THEN '' ELSE 'NULL' END AS nullable
FROM generate_series(1, 1050) AS i;
COMMIT;
'@
            & (Join-Path $pgBin 'psql.exe') -XwqAt $fixtureUri -v ON_ERROR_STOP=1 -c $grid
            if ($LASTEXITCODE -ne 0) { throw 'Could not seed the owned synthetic grid.' }
            $shape = & (Join-Path $pgBin 'psql.exe') -XwqAt $fixtureUri -c @'
SELECT count(*), min(char_length(wide_a)), max(octet_length(wide_a)),
       count(*) FILTER (WHERE source_id <= 1000 AND bucket = 'hit#'),
       current_setting('transaction_read_only')
FROM public.perf_grid
'@
            if ($LASTEXITCODE -ne 0 -or $shape -cne '1050|24|72|100|on') {
                throw 'The owned grid did not have the expected rows, wide values and read-only mode.'
            }
        }
    }
    finally {
        $env:PGSSLROOTCERT = $rootBefore
    }
    [ordered] @{
        postgresql_version = '17.12'
        postgresql_zip_sha256 = $hash
        transaction_read_only = $true
        sslmode = 'verify-full'
    } | ConvertTo-Json -Compress | Write-Output

    $journey = @{
        ArchivedClient = $ArchivedClient
        ExpectedClientSha256 = $ExpectedClientSha256
        CandidateSource = $CandidateSource
        CandidateBuild = $CandidateBuild
        WindowCursorCalibration = $true
        WorkbenchPort = $port
        WorkbenchCa = $caPath
        WorkbenchPerformance = $WorkbenchPerformance
        WorkbenchGrid = $WorkbenchGrid
    }
    & (Join-Path $PSScriptRoot 'Run-RendererCalibration.ps1') @journey
}
finally {
    if ($startAttempted -and (Test-Path -LiteralPath $readyFile -PathType Leaf)) {
        Confirm-ClusterOwner
        & $pgCtl stop -D $cluster -m fast -w -t 25 | Out-Null
        if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $readyFile)) {
            throw 'Could not confirm the owned PostgreSQL fixture stopped.'
        }
    }
    if ($startAttempted) {
        # pg_ctl status uses exit 3 to confirm shutdown. Read it from the
        # process handle so the successful workflow does not inherit it.
        $statusCheck = @{
            FilePath = $pgCtl
            ArgumentList = @('status', '-D', ('"' + $cluster + '"'))
            NoNewWindow = $true
            PassThru = $true
            Wait = $true
        }
        $status = Start-Process @statusCheck
        try {
            if ($status.ExitCode -ne 3 -or (Test-Path -LiteralPath $readyFile)) {
                throw 'PostgreSQL shutdown was not confirmed for the owned cluster.'
            }
        }
        finally {
            $status.Dispose()
        }
    }
    Remove-Item -LiteralPath $scratch -Recurse -Force
    Write-Output 'Owned PostgreSQL fixture removed.'
}
