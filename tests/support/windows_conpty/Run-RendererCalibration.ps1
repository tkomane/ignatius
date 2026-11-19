# Calibrate the stock Windows Terminal renderer on the job's own desktop.
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function FileDigest([string] $Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$desktop = & (Join-Path $PSScriptRoot 'Test-Desktop.ps1') | ConvertFrom-Json
Write-Output (ConvertTo-Json -InputObject $desktop -Compress)
if ($desktop.status -cne 'available') {
    throw 'The renderer probe requires the job to have an active input desktop.'
}
$revision = (& git -C $env:GITHUB_WORKSPACE rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $revision -cne $env:GITHUB_SHA -or
    (& git -C $env:GITHUB_WORKSPACE status --porcelain)) {
    throw 'The renderer probe requires the exact clean dispatch revision.'
}

$root = Join-Path $env:RUNNER_TEMP "ignatius-renderer-$env:GITHUB_RUN_ID-$env:GITHUB_RUN_ATTEMPT"
$work = Join-Path $root 'work'
$null = New-Item -ItemType Directory -Path $root
$null = New-Item -ItemType Directory -Path $work

$version = '1.25.3131.0'
$expectedSha256 = '273d6f1502eca1b5bd95c43f5d4d7ec72cc503df340102f8659717e5bead3437'
$asset = "Microsoft.WindowsTerminal_${version}_x64.zip"
$archive = Join-Path $root $asset
Invoke-WebRequest -Uri "https://github.com/microsoft/terminal/releases/download/v$version/$asset" `
    -OutFile $archive -MaximumRedirection 5 -TimeoutSec 60
if ((FileDigest $archive) -cne $expectedSha256) {
    throw 'The official Windows Terminal ZIP did not match the pinned checksum.'
}
$package = Join-Path $root 'terminal'
Expand-Archive -LiteralPath $archive -DestinationPath $package
$executables = @(Get-ChildItem -LiteralPath $package -Filter 'WindowsTerminal.exe' -File -Recurse)
if ($executables.Count -ne 1) {
    throw 'Expected exactly one WindowsTerminal.exe in the pinned ZIP.'
}
$terminal = $executables[0]
$portable = $terminal.DirectoryName
$null = New-Item -ItemType File -Path (Join-Path $portable '.portable')
$settings = Join-Path $portable 'settings'
$null = New-Item -ItemType Directory -Path $settings

$locator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$instances = @(& $locator -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if ($LASTEXITCODE -ne 0 -or $instances.Count -ne 1) {
    throw 'The hosted runner must provide an installed x64 MSVC toolset.'
}
& (Join-Path ($instances[0].Trim()) 'Common7/Tools/Launch-VsDevShell.ps1') -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
$compiler = Get-Command cl.exe -CommandType Application -ErrorAction Stop
$probe = Join-Path $root 'renderer-probe.exe'
Push-Location -LiteralPath $root
try {
    & $compiler.Source /nologo /std:c++17 /permissive- /utf-8 /W4 /WX /EHsc /DUNICODE /D_UNICODE `
        /DIGNATIUS_RENDERER_CALIBRATION (Join-Path $PSScriptRoot 'host.cpp') `
        /Fe:renderer-probe.exe /Fo:renderer-probe.obj /link Advapi32.lib User32.lib Gdi32.lib Dwmapi.lib
    if ($LASTEXITCODE -ne 0 -or -not [IO.File]::Exists($probe)) {
        throw 'Could not compile the native renderer calibration helper.'
    }
}
finally {
    Pop-Location
}

$rendererProfileId = '{cac612cd-581f-4749-b07b-15930ed48cf2}'
$configuration = [ordered] @{
    defaultProfile = $rendererProfileId
    theme = 'dark'
    disableAnimations = $true
    alwaysOnTop = $true
    centerOnLaunch = $true
    initialCols = 72
    initialRows = 20
    showTerminalTitleInTitlebar = $true
    windowingBehavior = 'useNew'
    profiles = [ordered] @{
        defaults = [ordered] @{
            closeOnExit = 'always'
            foreground = '#F2F2F2'
            background = '#101010'
            cursorShape = 'filledBox'
            cursorColor = '#FF00FF'
            font = @{ face = 'Consolas'; size = 12 }
            opacity = 100
            padding = '0'
            useAcrylic = $false
        }
        list = @([ordered] @{
            guid = $rendererProfileId
            name = 'Ignatius renderer calibration'
            commandline = ('"' + $probe + '"')
        })
    }
}
$settingsFile = Join-Path $settings 'settings.json'
[IO.File]::WriteAllText($settingsFile, (ConvertTo-Json $configuration -Depth 8))

$sourceHashes = [ordered] @{}
foreach ($source in @('host.cpp', 'protocol.hpp', 'renderer_pixels.hpp', 'renderer_probe.hpp',
        'Test-Desktop.ps1', 'Run-RendererCalibration.ps1')) {
    $sourceHashes[$source] = FileDigest (Join-Path $PSScriptRoot $source)
}
$build = [ordered] @{
    schema_version = 1
    source_revision = $revision
    source_hashes = $sourceHashes
    helper_sha256 = FileDigest $probe
    terminal_version = $version
    terminal_sha256 = FileDigest $terminal.FullName
    package_sha256 = $expectedSha256
    settings_sha256 = FileDigest $settingsFile
    profile_id = $rendererProfileId
    msvc_sha256 = FileDigest $compiler.Source
    windows_version = [Environment]::OSVersion.Version.ToString()
    image_os = $env:ImageOS
    image_version = $env:ImageVersion
    input_desktop = $desktop
}
[IO.File]::WriteAllText((Join-Path $work 'build.json'), ((ConvertTo-Json $build -Depth 6) + "`n"))

$nonce = [Guid]::NewGuid().ToString('N')
$stdout = Join-Path $root 'renderer.stdout'
$stderr = Join-Path $root 'renderer.stderr'
$controller = Start-Process -FilePath $probe -WorkingDirectory $work -NoNewWindow -PassThru `
    -ArgumentList @('--renderer-supervise', ('"' + $terminal.FullName + '"'), ('"' + $work + '"'), $nonce, $rendererProfileId) `
    -RedirectStandardOutput $stdout -RedirectStandardError $stderr
try {
    if (-not $controller.WaitForExit(40000)) {
        if (-not $controller.HasExited) {
            $controller.Kill()
            $null = $controller.WaitForExit(3000)
        }
        throw 'The native renderer helper exceeded its outer deadline.'
    }
    $exitCode = $controller.ExitCode
}
finally {
    $controller.Dispose()
}
if ((Get-Item -LiteralPath $stdout).Length -gt 16384) {
    throw 'The native renderer receipt exceeded its size limit.'
}
$receipt = [IO.File]::ReadAllText($stdout).Trim()
if ([string]::IsNullOrWhiteSpace($receipt) -or $receipt.Length -gt 16384) {
    throw "The native renderer helper did not supply a bounded receipt (exit $exitCode)."
}
$null = ConvertFrom-Json -InputObject $receipt
[IO.File]::WriteAllText((Join-Path $work 'renderer-report.json'), ($receipt + "`n"))
Write-Output $receipt
if ($exitCode -ne 0) {
    throw "The Windows Terminal renderer calibration failed (exit $exitCode)."
}
