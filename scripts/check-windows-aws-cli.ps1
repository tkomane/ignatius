# Account-free launch check for the machine-wide Windows x64 AWS CLI v2 MSI.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64' -or
    [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
    throw 'The AWS CLI probe requires native Windows x64 PowerShell.'
}

$programFiles = [Environment]::GetFolderPath('ProgramFiles')
$amazon = Join-Path $programFiles 'Amazon'
$package = Join-Path $amazon 'AWSCLIV2'
$aws = Join-Path $package 'aws.exe'
foreach ($entry in $programFiles, $amazon, $package, $aws) {
    if ((Get-Item -LiteralPath $entry).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'The standard AWS CLI machine installation contains a redirected path.'
    }
}
$selected = (Get-Command -Name aws.exe -CommandType Application -ErrorAction Stop).Source
if ([IO.Path]::GetFullPath($selected) -ne [IO.Path]::GetFullPath($aws)) {
    throw 'PATH selects aws.exe outside the standard AWS CLI v2 machine installation.'
}

$uninstall = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'
$products = @(Get-ChildItem -LiteralPath $uninstall | Where-Object {
    $_.GetValue('DisplayName') -ceq 'AWS Command Line Interface v2' -and
    $_.GetValue('Publisher') -ceq 'Amazon Web Services' -and $_.GetValue('WindowsInstaller') -eq 1
})
if ($products.Count -ne 1) { throw 'Expected one registered AWS CLI v2 machine MSI.' }
# The AWS 2.40.3 MSI declares ProductVersion 2.40.3.0; the CLI reports 2.40.3.
$registered = $products[0].GetValue('DisplayVersion')
$match = [regex]::Match([string]$registered, '\A(2\.[0-9]+\.[0-9]+)(?:\.0)?\z')
if (-not $match.Success) { throw 'Unrecognized AWS CLI v2 MSI version format.' }
$version = $match.Groups[1].Value

$artifacts = cargo test --offline --locked --lib --all-features --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Could not locate the compiled cloud provider tests.' }
$binaries = @($artifacts | ConvertFrom-Json | Where-Object {
    $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'ignatius' -and
    $_.target.kind -contains 'lib' -and $_.executable
})
if ($binaries.Count -ne 1) { throw 'Expected exactly one Ignatius library test executable.' }
$binary = $binaries[0].executable
# Rust also searches its application directory and the Windows system directories.
foreach ($directory in (Split-Path -Parent $binary), ([Environment]::SystemDirectory), $env:WINDIR) {
    if (Test-Path -LiteralPath (Join-Path $directory 'aws.exe') -PathType Leaf) {
        throw 'Another native aws.exe could take precedence over the AWS CLI package.'
    }
}

$work = [IO.Directory]::CreateTempSubdirectory('ignatius-aws-cli-')
$socket = [Net.Sockets.Socket]::new('InterNetwork', 'Stream', 'Tcp')
try {
    # Reserve a non-listening loopback port. Metadata is disabled separately.
    $socket.ExclusiveAddressUse = $true
    $socket.Bind([Net.IPEndPoint]::new([Net.IPAddress]::Loopback, 0))
    $proxy = 'http://127.0.0.1:' + $socket.LocalEndPoint.Port
    $profileDir = $work.CreateSubdirectory('profile').FullName
    $tmp = $work.CreateSubdirectory('tmp').FullName
    $start = [Diagnostics.ProcessStartInfo]::new($binary)
    $start.UseShellExecute = $false
    $start.WorkingDirectory = $work.FullName
    $testName = 'connection::cloud::tests::installed_windows_aws_cli_version'
    foreach ($arg in '--ignored', '--exact', $testName) { $start.ArgumentList.Add($arg) }
    $start.Environment.Clear()
    $settings = @{
        SystemRoot = $env:SystemRoot
        WINDIR = $env:WINDIR
        PATH = $package
        USERPROFILE = $profileDir
        APPDATA = $profileDir
        LOCALAPPDATA = $profileDir
        TMP = $tmp
        TEMP = $tmp
        AWS_CONFIG_FILE = Join-Path $profileDir 'config'
        AWS_SHARED_CREDENTIALS_FILE = Join-Path $profileDir 'credentials'
        AWS_EC2_METADATA_DISABLED = 'true'
        AWS_PAGER = ''
        HTTP_PROXY = $proxy
        HTTPS_PROXY = $proxy
        ALL_PROXY = $proxy
        IGNATIUS_TEST_AWS_VERSION = $version
    }
    foreach ($key in $settings.Keys) { $start.Environment[$key] = $settings[$key] }
    $worker = [Diagnostics.Process]::Start($start)
    try {
        if (-not $worker.WaitForExit(90000)) {
            $worker.Kill($true)
            $null = $worker.WaitForExit(5000)
            throw 'AWS CLI version worker exceeded 90 seconds.'
        }
        if ($worker.ExitCode -ne 0) { throw 'The native AWS CLI version probe failed.' }
    } finally {
        $worker.Dispose()
    }

    # A misspelled exact filter exits zero without running the test.
    $receiptPath = Join-Path $work.FullName 'probe-result'
    if (-not [IO.File]::Exists($receiptPath)) { throw 'The exact AWS CLI version test produced no receipt.' }
    $receipt = [IO.File]::ReadAllText($receiptPath)
    if ($receipt -cne 'version-matched') { throw 'Unexpected AWS CLI version receipt.' }
    $image = $env:ImageVersion
    if ($image -notmatch '\A[0-9]+(\.[0-9]+)+\z') { throw 'Missing or unexpected runner image version.' }
    Write-Host "AWS CLI v2 x64 version=$version registered=$registered image=$image os=$([Environment]::OSVersion.Version) launcher=aws.exe resolved-aws=$receipt"
} finally {
    $socket.Dispose()
    $work.Delete($true)
}
