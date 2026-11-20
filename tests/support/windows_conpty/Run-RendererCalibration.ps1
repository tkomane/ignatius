# Exercise the stock Windows Terminal renderer on the job's own desktop.
[CmdletBinding()]
param(
    [string] $ArchivedClient,
    [string] $ExpectedClientSha256,
    [string] $CandidateSource,
    [string] $CandidateBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function FileDigest([string] $Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function PeArchitecture([string] $Path) {
    $stream = [IO.File]::OpenRead($Path)
    $binary = [IO.BinaryReader]::new($stream)
    try {
        if ($stream.Length -lt 70 -or $binary.ReadUInt16() -ne 0x5A4D) {
            throw 'Expected the DOS header of a Windows executable.'
        }
        $stream.Position = 0x3C
        $header = $binary.ReadUInt32()
        if ($header -lt 0x40 -or $header -gt ($stream.Length - 6)) {
            throw 'Invalid Windows executable header offset.'
        }
        $stream.Position = $header
        if ($binary.ReadUInt32() -ne 0x00004550) {
            throw 'Expected the PE signature of a Windows executable.'
        }
        switch ($binary.ReadUInt16()) {
            0x8664 { return 'x64' }
            0xAA64 { return 'arm64' }
            default { throw 'The Windows executable architecture is outside this probe.' }
        }
    }
    finally {
        $binary.Dispose()
    }
}

$archiveMode = $PSBoundParameters.ContainsKey('ArchivedClient')
if ($archiveMode) {
    if ([string]::IsNullOrWhiteSpace($ArchivedClient) -or -not [IO.Path]::IsPathFullyQualified($ArchivedClient) -or
        $ExpectedClientSha256 -notmatch '\A[0-9a-f]{64}\z' -or $CandidateSource -notmatch '\A[0-9a-f]{40}\z' -or
        $CandidateBuild -cnotmatch '\Aci/[1-9][0-9]{0,18}-[1-9][0-9]{0,3}\z') {
        throw 'Archived-client mode requires an absolute executable path, binary SHA-256, source SHA and CI build identity.'
    }
    $client = Get-Item -LiteralPath $ArchivedClient -Force
    if ($client -isnot [IO.FileInfo] -or $client.Extension -ine '.exe' -or
        ($client.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'The archived client must be a regular Windows executable.'
    }
    $clientHash = FileDigest $client.FullName
    if ($clientHash -cne $ExpectedClientSha256.ToLowerInvariant()) {
        throw 'The archived client did not match its expected SHA-256.'
    }
    $clientArchitecture = PeArchitecture $client.FullName
}
elseif ($PSBoundParameters.ContainsKey('ExpectedClientSha256') -or $PSBoundParameters.ContainsKey('CandidateSource') -or
    $PSBoundParameters.ContainsKey('CandidateBuild')) {
    throw 'Archived-client identity arguments require -ArchivedClient.'
}

$osArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
$vcComponents = @('Microsoft.VisualStudio.Component.VC.Tools.x86.x64')
switch -Exact ($osArchitecture) {
    'x64' {
        $expectedSha256 = '273d6f1502eca1b5bd95c43f5d4d7ec72cc503df340102f8659717e5bead3437'
        $vcTarget = 'amd64'
    }
    'arm64' {
        $expectedSha256 = 'b7e1d2a414e6368affd098d91a4cba441dcacd3ab634a6f39c8e6f5490deef89'
        $vcTarget = 'arm64'
        $vcComponents += 'Microsoft.VisualStudio.Component.VC.Tools.ARM64'
    }
    default { throw 'The native Windows OS must be x64 or ARM64.' }
}
if ($archiveMode -and $osArchitecture -eq 'x64' -and $clientArchitecture -ne 'x64') {
    throw 'An x64 Windows host requires an x64 archived client.'
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
$asset = "Microsoft.WindowsTerminal_${version}_${osArchitecture}.zip"
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
$terminalArchitecture = PeArchitecture $terminal.FullName
if ($terminalArchitecture -cne $osArchitecture) {
    throw 'The portable Windows Terminal binary must match the native OS architecture.'
}
$portable = $terminal.DirectoryName
$null = New-Item -ItemType File -Path (Join-Path $portable '.portable')
$settings = Join-Path $portable 'settings'
$null = New-Item -ItemType Directory -Path $settings

$locator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$instances = @(& $locator -latest -products '*' -requires @vcComponents -property installationPath)
if ($LASTEXITCODE -ne 0 -or $instances.Count -ne 1) {
    throw "The hosted runner must provide the $osArchitecture MSVC target toolset and x64 compiler host."
}
& (Join-Path ($instances[0].Trim()) 'Common7/Tools/Launch-VsDevShell.ps1') -Arch $vcTarget -HostArch amd64 -SkipAutomaticLocation
$compiler = Get-Command cl.exe -CommandType Application -ErrorAction Stop
$probe = Join-Path $root 'renderer-probe.exe'
Push-Location -LiteralPath $root
try {
    & $compiler.Source /nologo /std:c++17 /permissive- /utf-8 /W4 /WX /EHsc /DUNICODE /D_UNICODE `
        /DIGNATIUS_RENDERER_CALIBRATION (Join-Path $PSScriptRoot 'host.cpp') `
        /Fe:renderer-probe.exe /Fo:renderer-probe.obj /link Advapi32.lib User32.lib Gdi32.lib Dwmapi.lib Ws2_32.lib
    if ($LASTEXITCODE -ne 0 -or -not [IO.File]::Exists($probe)) {
        throw 'Could not compile the native renderer calibration helper.'
    }
}
finally {
    Pop-Location
}
$helperArchitecture = PeArchitecture $probe
if ($helperArchitecture -cne $osArchitecture) {
    throw 'The renderer helper must match the native OS architecture.'
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
foreach ($source in @('host.cpp', 'protocol.hpp', 'renderer_pixels.hpp', 'renderer_probe.hpp', 'renderer_modal.hpp', 'renderer_archive.hpp',
        'Test-Desktop.ps1', 'Run-RendererCalibration.ps1')) {
    $sourceHashes[$source] = FileDigest (Join-Path $PSScriptRoot $source)
}
$build = [ordered] @{
    schema_version = 1
    source_revision = $revision
    source_hashes = $sourceHashes
    helper_sha256 = FileDigest $probe
    helper_architecture = $helperArchitecture
    terminal_version = $version
    terminal_sha256 = FileDigest $terminal.FullName
    terminal_architecture = $terminalArchitecture
    package_sha256 = $expectedSha256
    settings_sha256 = FileDigest $settingsFile
    profile_id = $rendererProfileId
    msvc_sha256 = FileDigest $compiler.Source
    windows_version = [Environment]::OSVersion.Version.ToString()
    os_architecture = $osArchitecture
    image_os = $env:ImageOS
    image_version = $env:ImageVersion
    input_desktop = $desktop
}
if ($archiveMode) {
    $build['archived_client_sha256'] = $clientHash
    $build['archived_client_architecture'] = $clientArchitecture
    $build['candidate_source'] = $CandidateSource.ToLowerInvariant()
    $build['candidate_build'] = $CandidateBuild
}
[IO.File]::WriteAllText((Join-Path $work 'build.json'), ((ConvertTo-Json $build -Depth 6) + "`n"))

$nonce = [Guid]::NewGuid().ToString('N')
$stdout = Join-Path $root 'renderer.stdout'
$stderr = Join-Path $root 'renderer.stderr'
$entryPoint = if ($archiveMode) { '--renderer-archive-supervise' } else { '--renderer-supervise' }
$controllerArgs = @($entryPoint, ('"' + $terminal.FullName + '"'), ('"' + $work + '"'), $nonce, $rendererProfileId)
if ($archiveMode) {
    if ((FileDigest $client.FullName) -cne $clientHash) {
        throw 'The archived client changed before it could be launched.'
    }
    $controllerArgs += ('"' + $client.FullName + '"')
}
$controller = Start-Process -FilePath $probe -WorkingDirectory $work -NoNewWindow -PassThru `
    -ArgumentList $controllerArgs `
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
$facts = ConvertFrom-Json -InputObject $receipt
[IO.File]::WriteAllText((Join-Path $work 'renderer-report.json'), ($receipt + "`n"))
Write-Output $receipt
if ($exitCode -ne 0) {
    $check = if ($archiveMode) { 'archived-client renderer verification' } else { 'renderer calibration' }
    throw "The Windows Terminal $check failed (exit $exitCode)."
}
if ($archiveMode) {
    if ($facts.case -cne 'renderer-archive' -or -not $facts.archive_diagnostic_passed -or
        -not $facts.archived_client.owned -or -not $facts.archived_client.exit_waited -or
        $facts.archived_client.exit_code -ne 0 -or -not $facts.archived_client.restored_modes -or
        -not $facts.cleanup.job_empty -or -not $facts.cleanup.pipe_quiescent) {
        throw 'The archived-client renderer did not establish its declared native observations.'
    }
    foreach ($name in @('baseline.bmp', 'ignatius-active.bmp', 'ignatius-restored.bmp')) {
        $capture = Get-Item -LiteralPath (Join-Path $work $name)
        if ($capture -isnot [IO.FileInfo] -or $capture.Length -le 0 -or
            ($capture.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "The archived-client renderer did not retain $name."
        }
    }
}
