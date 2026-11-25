# Account-free comparison of the stock GCP launcher and Google's Windows wrapper.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64' -or
    [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
    throw 'The Google Cloud CLI archive probe requires native Windows x64 PowerShell.'
}

# Google's independently published Windows x64 bundled-Python row, 2026-11-25:
# https://docs.cloud.google.com/sdk/docs/downloads-versioned-archives
$version = '594.0.1'
$uri = 'https://dl.google.com/dl/cloudsdk/channels/rapid/downloads/google-cloud-sdk-594.0.1-windows-x86_64-bundled-python.zip'
$digest = '9612ee5a32a13e917fda84a0dce8028264c3fec1e8ca2738d1d43c312c675332'
$image = $env:ImageVersion
if ($image -notmatch '\A[0-9]+(\.[0-9]+)+\z') { throw 'Missing or unexpected runner image version.' }

$artifacts = cargo test --offline --locked --lib --all-features --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Could not locate the compiled cloud provider tests.' }
$binaries = @($artifacts | ConvertFrom-Json | Where-Object {
    $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'ignatius' -and
    $_.target.kind -contains 'lib' -and $_.executable
})
if ($binaries.Count -ne 1) { throw 'Expected exactly one Ignatius library test executable.' }
$binary = $binaries[0].executable
$system = [Environment]::SystemDirectory
$cmd = Join-Path $system 'cmd.exe'
foreach ($directory in (Split-Path -Parent $binary), $system, $env:WINDIR) {
    if (Test-Path -LiteralPath (Join-Path $directory 'gcloud.exe') -PathType Leaf) {
        throw 'A native gcloud.exe outside the archive could be selected by the provider.'
    }
}

function Assert-GCloudVersion([string] $output, [string] $route) {
    try {
        $info = ConvertFrom-Json -InputObject $output -AsHashtable -NoEnumerate
    } catch {
        throw "The Google Cloud CLI $route returned invalid version JSON."
    }
    if ($info -isnot [Collections.IDictionary] -or $info['Google Cloud SDK'] -isnot [string] -or
        $info['Google Cloud SDK'] -cne $version) {
        throw "The Google Cloud CLI $route version differs from the pinned archive."
    }
}

function Invoke-VersionWorker([Diagnostics.ProcessStartInfo] $start, [IO.DirectoryInfo] $state, [switch] $BundledPython) {
    $profileDir = $state.CreateSubdirectory('profile').FullName
    $tmp = $state.CreateSubdirectory('tmp').FullName
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.Environment.Clear()
    $settings = @{
        SystemRoot = $env:SystemRoot
        WINDIR = $env:WINDIR
        ComSpec = $cmd
        PATH = "$bin;$system"
        PATHEXT = '.COM;.EXE;.BAT;.CMD'
        USERPROFILE = $profileDir
        APPDATA = $profileDir
        LOCALAPPDATA = $profileDir
        TMP = $tmp
        TEMP = $tmp
        CLOUDSDK_CONFIG = $state.CreateSubdirectory('config').FullName
        CLOUDSDK_PYTHON = $python
        CLOUDSDK_AUTH_DISABLE_CREDENTIALS = 'true'
        CLOUDSDK_CORE_DISABLE_PROMPTS = 'true'
        CLOUDSDK_CORE_DISABLE_USAGE_REPORTING = 'true'
        CLOUDSDK_CORE_DISABLE_FILE_LOGGING = 'true'
        CLOUDSDK_COMPONENT_MANAGER_DISABLE_UPDATE_CHECK = 'true'
        GCE_METADATA_HOST = $socket.LocalEndPoint.ToString()
        GCE_METADATA_IP = $socket.LocalEndPoint.ToString()
        HTTP_PROXY = $proxy
        HTTPS_PROXY = $proxy
        ALL_PROXY = $proxy
        IGNATIUS_TEST_GCLOUD_VERSION = $version
    }
    if ($BundledPython) {
        $settings['PATH'] = "$(Join-Path $sdk 'bin/sdk');$($settings['PATH'])"
        $settings['CLOUDSDK_ROOT_DIR'] = $sdk
    }
    foreach ($key in $settings.Keys) { $start.Environment[$key] = $settings[$key] }
    $worker = [Diagnostics.Process]::Start($start)
    try {
        $worker.StandardInput.Close()
        # Read at most 16,384 characters plus one overflow sentinel, concurrently
        # draining stderr without retaining it. A noisy child cannot grow the capture.
        $limitMs = 90000
        $elapsed = [Diagnostics.Stopwatch]::StartNew()
        $buffer = [char[]]::new(16385)
        $stdout = $worker.StandardOutput.ReadBlockAsync($buffer, 0, $buffer.Length)
        $stderr = $worker.StandardError.BaseStream.CopyToAsync([IO.Stream]::Null)
        if (-not $stdout.Wait($limitMs)) { throw 'Google Cloud CLI version worker exceeded 90 seconds.' }
        $count = $stdout.GetAwaiter().GetResult()
        if ($count -eq $buffer.Length) { throw 'Google Cloud CLI version worker exceeded the stdout limit.' }
        $remaining = [int][Math]::Max(0L, $limitMs - $elapsed.ElapsedMilliseconds)
        if (-not $worker.WaitForExit($remaining)) { throw 'Google Cloud CLI version worker exceeded 90 seconds.' }
        if ($worker.ExitCode -ne 0) { throw 'Google Cloud CLI version worker failed.' }
        if (-not $stderr.Wait(5000)) { throw 'Google Cloud CLI version worker stderr did not close.' }
        return [string]::new($buffer, 0, $count)
    } finally {
        if (-not $worker.HasExited) {
            $worker.Kill($true)
            $null = $worker.WaitForExit(5000)
        }
        $worker.Dispose()
    }
}

$work = [IO.Directory]::CreateTempSubdirectory('ig-gc-')
$socket = [Net.Sockets.Socket]::new('InterNetwork', 'Stream', 'Tcp')
try {
    $zip = Join-Path $work.FullName 'sdk.zip'
    Invoke-WebRequest -Uri $uri -OutFile $zip -MaximumRedirection 0 -TimeoutSec 120
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash -ne $digest) {
        throw 'Google Cloud CLI archive SHA-256 differs from the reviewed Google publication.'
    }
    [IO.Compression.ZipFile]::ExtractToDirectory($zip, $work.FullName)
    $sdk = Join-Path $work.FullName 'google-cloud-sdk'
    $bin = Join-Path $sdk 'bin'
    $python = Join-Path $sdk 'platform/bundledpython/python.exe'
    $nativePresent = [IO.File]::Exists((Join-Path $bin 'gcloud.exe'))
    if (-not [IO.File]::Exists((Join-Path $bin 'gcloud.cmd')) -or -not [IO.File]::Exists($python)) {
        throw 'Expected the Google Cloud CLI wrapper and bundled interpreter in the verified archive.'
    }

    $socket.ExclusiveAddressUse = $true
    $socket.Bind([Net.IPEndPoint]::new([Net.IPAddress]::Loopback, 0))
    $proxy = 'http://127.0.0.1:' + $socket.LocalEndPoint.Port

    # The documented Windows wrapper is a harness-only positive control. All
    # command text is fixed, and the verified bin directory supplies the wrapper.
    $wrapper = [Diagnostics.ProcessStartInfo]::new($cmd)
    $wrapper.WorkingDirectory = $bin
    $wrapper.Arguments = '/d /c gcloud.cmd version --format=json'
    $output = Invoke-VersionWorker $wrapper ($work.CreateSubdirectory('wrapper'))
    Assert-GCloudVersion $output 'wrapper'

    # This mirrors the verified wrapper's fixed Python launch without invoking
    # its batch file, and keeps the verified archive root and worker state.
    $bundledState = $work.CreateSubdirectory('bundled-python')
    $gcloudScript = Join-Path $sdk 'lib/gcloud.py'
    if (-not [IO.File]::Exists($gcloudScript)) { throw 'The verified archive is missing lib/gcloud.py.' }
    $bundled = [Diagnostics.ProcessStartInfo]::new($python)
    $bundled.WorkingDirectory = $bundledState.FullName
    foreach ($arg in '-S', $gcloudScript, 'version', '--format=json') {
        $bundled.ArgumentList.Add($arg)
    }
    $bundledOutput = Invoke-VersionWorker $bundled $bundledState -BundledPython
    Assert-GCloudVersion $bundledOutput 'bundled Python'

    $state = $work.CreateSubdirectory('bare')
    $test = [Diagnostics.ProcessStartInfo]::new($binary)
    $test.WorkingDirectory = $state.FullName
    foreach ($arg in '--ignored', '--exact', 'connection::cloud::tests::windows_gcloud_archive_version') {
        $test.ArgumentList.Add($arg)
    }
    $null = Invoke-VersionWorker $test $state
    $receiptPath = Join-Path $state.FullName 'probe-result'
    if (-not [IO.File]::Exists($receiptPath)) { throw 'The exact bare-name test produced no receipt.' }
    $bare = [IO.File]::ReadAllText($receiptPath)
    if ($bare -cnotin 'missing-program', 'version-matched') { throw 'Unexpected bare-name result.' }
    if (($bare -ceq 'version-matched') -ne $nativePresent) {
        throw 'The bare-name result disagrees with the presence of gcloud.exe in the verified archive bin.'
    }
    Write-Host "Google Cloud CLI x64 archive=$version sha256=$digest image=$image os=$([Environment]::OSVersion.Version) bare-gcp=$bare wrapper=version-matched bundled-python=version-matched"
} finally {
    $socket.Dispose()
    $work.Delete($true)
}
