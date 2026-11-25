# Account-free probe for the standard x64 MSI on the Windows platform runner.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64' -or
    [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
    throw 'The Azure CLI MSI probe requires native Windows x64 PowerShell.'
}

$package = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Microsoft SDKs/Azure/CLI2'
$wbin = Join-Path $package 'wbin'
$python = Join-Path $package 'python.exe'
$wrapper = Join-Path $wbin 'az.cmd'
if (-not (Test-Path -LiteralPath $python -PathType Leaf) -or
    -not (Test-Path -LiteralPath $wrapper -PathType Leaf)) {
    throw 'The standard Azure CLI MSI package interpreter or wrapper is missing.'
}
$installedWrapper = (Get-Command -Name az.cmd -CommandType Application -ErrorAction Stop).Source
if ([IO.Path]::GetFullPath($installedWrapper) -ne [IO.Path]::GetFullPath($wrapper)) {
    throw 'PATH selects an Azure CLI wrapper outside the standard x64 MSI package.'
}
$recipe = Get-Content -LiteralPath $wrapper -Raw
if (-not $recipe.Contains('"%~dp0\..\python.exe" -IBm azure.cli %*') -or
    -not $recipe.Contains('SET AZ_INSTALLER=MSI')) {
    throw 'The installed MSI launcher recipe has changed. Review it before running the probe.'
}
$uninstall = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'
$products = @(Get-ChildItem -LiteralPath $uninstall | Where-Object {
    $_.GetValue('DisplayName') -eq 'Microsoft Azure CLI (64-bit)' -and
    $_.GetValue('Publisher') -eq 'Microsoft Corporation' -and
    $_.GetValue('WindowsInstaller') -eq 1
})
if ($products.Count -ne 1) { throw 'Expected one registered Microsoft Azure CLI x64 MSI.' }
$version = $products[0].GetValue('DisplayVersion')
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Unexpected Azure CLI MSI version format.' }

# Run the already built libtest executable directly so the worker can inherit
# only the test environment below, independently of Cargo and runner settings.
$artifacts = cargo test --offline --locked --lib --all-features --no-run --message-format=json
if ($LASTEXITCODE -ne 0) { throw 'Could not locate the compiled cloud provider tests.' }
$binaries = @($artifacts | ConvertFrom-Json | Where-Object {
    $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'ignatius' -and
    $_.target.kind -contains 'lib' -and $_.executable
})
if ($binaries.Count -ne 1) { throw 'Expected exactly one Ignatius library test executable.' }
$binary = $binaries[0].executable
$testName = 'connection::cloud::tests::installed_windows_azure_msi_version'
# Rust also searches its application and Windows system directories, even
# when the worker PATH contains only the MSI wbin directory.
foreach ($directory in $wbin, (Split-Path -Parent $binary), ([Environment]::SystemDirectory), $env:WINDIR) {
    if (Test-Path -LiteralPath (Join-Path $directory 'az.exe') -PathType Leaf) {
        throw 'A native az.exe changes the observed MSI launcher layout. Review the package before probing.'
    }
}

function Invoke-MsiRoute([string] $route, [string] $parent, [string] $proxy) {
    $work = Join-Path $parent $route
    foreach ($child in 'profile', 'config', 'extensions', 'system-extensions', 'tmp') {
        $null = [IO.Directory]::CreateDirectory((Join-Path $work $child))
    }
    $config = Join-Path $work 'config'
    # CLI initialization checks for upstream releases when the version cache
    # is empty, even before the local 'version' command executes.
    @{ versions = @{ core = @{ local = $version } } } | ConvertTo-Json -Depth 4 |
        Set-Content -LiteralPath (Join-Path $config 'versionCheck.json') -Encoding utf8NoBOM
    $start = [Diagnostics.ProcessStartInfo]::new($binary)
    $start.UseShellExecute = $false
    $start.WorkingDirectory = $work
    foreach ($argument in '--ignored', '--exact', $testName) { $start.ArgumentList.Add($argument) }
    $start.Environment.Clear()
    $settings = @{
        SystemRoot = $env:SystemRoot
        WINDIR = $env:WINDIR
        PATH = $wbin
        USERPROFILE = Join-Path $work 'profile'
        APPDATA = Join-Path $work 'profile'
        LOCALAPPDATA = Join-Path $work 'profile'
        TMP = Join-Path $work 'tmp'
        TEMP = Join-Path $work 'tmp'
        AZURE_CONFIG_DIR = $config
        AZURE_EXTENSION_DIR = Join-Path $work 'extensions'
        AZURE_EXTENSION_SYS_DIR = Join-Path $work 'system-extensions'
        AZURE_CORE_COLLECT_TELEMETRY = 'no'
        'AZURE_AUTO-UPGRADE_ENABLE' = 'no'
        AZURE_EXTENSION_USE_DYNAMIC_INSTALL = 'no'
        HTTP_PROXY = $proxy
        HTTPS_PROXY = $proxy
        ALL_PROXY = $proxy
        IGNATIUS_TEST_AZ_ROUTE = $route
        IGNATIUS_TEST_AZ_VERSION = $version
    }
    if ($route -eq 'package') {
        $settings['AZ_INSTALLER'] = 'MSI'
        $settings['IGNATIUS_TEST_AZ_PYTHON'] = $python
    }
    foreach ($key in $settings.Keys) { $start.Environment[$key] = $settings[$key] }
    $childProcess = [Diagnostics.Process]::Start($start)
    try {
        if (-not $childProcess.WaitForExit(90000)) {
            $childProcess.Kill($true)
            throw "Azure CLI MSI $route worker exceeded 90 seconds."
        }
        if ($childProcess.ExitCode -ne 0) { throw "Azure CLI MSI $route version probe failed." }
    } finally {
        $childProcess.Dispose()
    }
    $receipt = [IO.File]::ReadAllText((Join-Path $work 'probe-result'))
    if ($receipt -cne 'version-matched' -and ($route -ne 'bare' -or $receipt -cne 'missing-program')) {
        throw "Azure CLI MSI $route worker returned an unrecognized receipt."
    }
    return $receipt
}

$tempRoot = [IO.Directory]::CreateTempSubdirectory('ignatius-azure-msi-')
# The reserved port has no listener, so this loopback proxy cannot forward
# traffic or accidentally connect to another local service.
$socket = [Net.Sockets.Socket]::new('InterNetwork', 'Stream', 'Tcp')
try {
    $socket.ExclusiveAddressUse = $true
    $socket.Bind([Net.IPEndPoint]::new([Net.IPAddress]::Loopback, 0))
    $proxy = 'http://127.0.0.1:' + $socket.LocalEndPoint.Port
    $bare = Invoke-MsiRoute 'bare' $tempRoot.FullName $proxy
    $packaged = Invoke-MsiRoute 'package' $tempRoot.FullName $proxy
    $image = $env:ImageVersion
    if ($image -notmatch '^\d+(\.\d+)+$') { $image = 'unreported' }
    Write-Host "Azure CLI MSI x64 version=$version image=$image os=$([Environment]::OSVersion.Version) bare-on-msi-path=$bare package=$packaged"
} finally {
    $socket.Dispose()
    $tempRoot.Delete($true)
}
