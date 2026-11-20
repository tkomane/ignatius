# Exercise the stock Windows Terminal renderer on the job's own desktop.
[CmdletBinding()]
param(
    [string] $ArchivedClient,
    [string] $ExpectedClientSha256,
    [string] $CandidateSource,
    [string] $CandidateBuild,
    [string] $GuardBuildManifest,
    [switch] $GuardMatrix
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

function Get-GuardFile([string] $Path, [long] $Limit) {
    if (-not [IO.Path]::IsPathFullyQualified($Path)) {
        throw 'The Guard input or capture must have an absolute path.'
    }
    $file = Get-Item -LiteralPath $Path -Force
    if ($file -isnot [IO.FileInfo] -or $file.Length -le 0 -or $file.Length -gt $Limit -or
        ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'The Guard input or capture must be a bounded regular file.'
    }
    return $file
}

function Test-GuardDescendant([string] $Directory, [string] $Path) {
    if (-not [IO.Path]::IsPathFullyQualified($Directory) -or -not [IO.Path]::IsPathFullyQualified($Path)) {
        return $false
    }
    $relative = [IO.Path]::GetRelativePath($Directory, $Path)
    return -not [IO.Path]::IsPathRooted($relative) -and $relative -ne '.' -and
        $relative -notmatch '\A\.\.(?:[\\/]|\z)'
}

function Assert-GuardInput([string] $Path, [string] $Digest, [long] $Limit = [long]::MaxValue) {
    $file = Get-GuardFile $Path $Limit
    if ($Digest -cnotmatch '\A[0-9a-f]{64}\z' -or (FileDigest $file.FullName) -cne $Digest) {
        throw 'The Guard build input did not match its recorded SHA-256.'
    }
}

function Assert-GuardInputs {
    Assert-GuardInput $guardManifestFile.FullName $guardManifestHash 16MB
    Assert-GuardInput $guardManifest.library.path $guardManifest.library.sha256
    Assert-GuardInput $guardChild.path $guardChild.sha256
    Assert-GuardInput (Join-Path $guardManifest.build_root 'prebuild-inputs.json') $guardManifest.prebuild_manifest_sha256 16MB
    Assert-GuardInput (Join-Path $env:GITHUB_WORKSPACE 'Cargo.lock') $guardManifest.lock_sha256
    Assert-GuardInput (Join-Path $PSScriptRoot 'guard_child.rs') $guardSource[0].sha256
    Assert-GuardInput $compiler.FullName $guardManifest.cl_sha256
}

function Assert-GuardCapture([IO.FileInfo] $File) {
    $reader = [IO.BinaryReader]::new($File.OpenRead())
    try {
        if ($File.Length -lt 58 -or $reader.ReadUInt16() -ne 0x4D42 -or
            $reader.ReadUInt32() -ne $File.Length -or $reader.ReadUInt32() -ne 0 -or
            $reader.ReadUInt32() -ne 54 -or $reader.ReadUInt32() -ne 40) {
            throw 'The Guard renderer did not produce the bounded BMP header.'
        }
        $width = $reader.ReadInt32()
        $height = -[long]$reader.ReadInt32()
        if ($width -lt 1 -or $width -gt 1024 -or $height -lt 1 -or $height -gt 768 -or
            $reader.ReadUInt16() -ne 1 -or $reader.ReadUInt16() -ne 32 -or
            $reader.ReadUInt32() -ne 0 -or $reader.ReadUInt32() -ne (4 * $width * $height) -or
            $File.Length -ne (54 + (4 * $width * $height))) {
            throw 'The Guard renderer BMP dimensions or encoded length did not match.'
        }
    }
    finally {
        $reader.Dispose()
    }
}

$guardMode = $PSBoundParameters.ContainsKey('GuardBuildManifest')
$archiveMode = $PSBoundParameters.ContainsKey('ArchivedClient')
if (($guardMode -and $archiveMode) -or ($GuardMatrix -and -not $guardMode)) {
    throw 'Select either archived-client or Guard mode. The Guard matrix requires its build manifest.'
}
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
if ($archiveMode -and $clientArchitecture -ne 'x64') {
    throw 'Both archive probes require the retained x64 Ignatius executable.'
}

$desktop = & (Join-Path $PSScriptRoot 'Test-Desktop.ps1') | ConvertFrom-Json
Write-Output (ConvertTo-Json -InputObject $desktop -Compress)
if ($desktop.status -cne 'available') {
    throw 'The renderer probe requires the job to have an active input desktop.'
}
$revision = (& git -C $env:GITHUB_WORKSPACE rev-parse HEAD).Trim()
$revisionExit = $LASTEXITCODE
$sourceStatus = @(& git -C $env:GITHUB_WORKSPACE status --porcelain)
if ($revisionExit -ne 0 -or $LASTEXITCODE -ne 0 -or $revision -cnotmatch '\A[0-9a-f]{40}\z' -or
    $revision -cne $env:GITHUB_SHA -or $sourceStatus.Count -ne 0) {
    throw 'The renderer probe requires the exact clean dispatch revision.'
}

if ($guardMode) {
    . (Join-Path $PSScriptRoot 'Validation.ps1')
    $guardManifestFile = Get-GuardFile $GuardBuildManifest 16MB
    if ($guardManifestFile.Name -cne 'build-manifest.json' -or
        -not (Test-GuardDescendant $env:RUNNER_TEMP $guardManifestFile.FullName)) {
        throw 'The Guard manifest must come from the owned runner build directory.'
    }
    $guardManifestHash = FileDigest $guardManifestFile.FullName
    $guardManifest = Get-Content -LiteralPath $guardManifestFile.FullName -Raw | ConvertFrom-Json
    if ((Get-W07Number $guardManifest.schema) -ne 1 -or $guardManifest.scope -cne 'first-capability-controls-only' -or
        $guardManifest.revision -cne $revision -or $guardManifest.native_executed -isnot [bool] -or
        $guardManifest.native_executed -or $guardManifest.pre_post_inputs_verified -isnot [bool] -or
        -not $guardManifest.pre_post_inputs_verified -or $guardManifest.tracked_source_unchanged -isnot [bool] -or
        -not $guardManifest.tracked_source_unchanged -or @($guardManifest.untracked_status).Count -ne 0 -or
        $guardManifest.source -ine [IO.Path]::GetFullPath($env:GITHUB_WORKSPACE) -or
        $guardManifest.build_root -ine $guardManifestFile.DirectoryName -or
        [string]::IsNullOrWhiteSpace($env:ImageOS) -or [string]::IsNullOrWhiteSpace($env:ImageVersion) -or
        $guardManifest.runner_image_os -cne $env:ImageOS -or $guardManifest.runner_image_version -cne $env:ImageVersion -or
        $guardManifest.os_build -ne [Environment]::OSVersion.Version.Build -or
        $guardManifest.os_description -cne [Runtime.InteropServices.RuntimeInformation]::OSDescription) {
        throw 'The Guard build must describe this clean dispatch revision and the same hosted runner.'
    }
    if ($guardManifest.library.cargo_profile.test -isnot [bool] -or $guardManifest.library.cargo_profile.test -or
        [IO.Path]::GetFileName($guardManifest.library.path) -cnotmatch '\Alibignatius(?:-[0-9a-f]+)?\.rlib\z' -or
        -not (Test-GuardDescendant $guardManifest.cargo_target $guardManifest.library.path)) {
        throw 'The Guard build must record the selected production Ignatius library.'
    }
    $guardChildren = @($guardManifest.binaries | Where-Object { [IO.Path]::GetFileName($_.path) -ceq 'guard-child.exe' })
    $guardSource = @($guardManifest.helper_inputs | Where-Object { [IO.Path]::GetFileName($_.path) -ceq 'guard_child.rs' })
    if ($guardChildren.Count -ne 1 -or $guardSource.Count -ne 1 -or
        $guardChildren[0].path -ine (Join-Path $guardManifest.build_root 'guard-child.exe') -or
        $guardSource[0].path -ine (Join-Path $PSScriptRoot 'guard_child.rs')) {
        throw 'The Guard manifest must record exactly one owned child and its tracked source.'
    }
    $guardChild = $guardChildren[0]
    Assert-GuardInput $guardChild.path $guardChild.sha256
    if ((PeArchitecture $guardChild.path) -cne $osArchitecture) {
        throw 'The Guard child must match the native OS architecture.'
    }
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
$compilerPath = Join-Path $env:VCToolsInstallDir "bin/HostX64/$osArchitecture/cl.exe"
$compiler = Get-Item -LiteralPath $compilerPath
$targetAliases = if ($osArchitecture -eq 'x64') { @('x64', 'amd64') } else { @('arm64') }
if ($compiler -isnot [IO.FileInfo] -or $env:VSCMD_ARG_TGT_ARCH -notin $targetAliases) {
    throw 'The Visual Studio developer shell did not select the expected compiler target.'
}
if ($guardMode) {
    $rust = (& rustc --version --verbose | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or -not (Test-W07PinnedRust $rust) -or $rust -cne $guardManifest.rust_verbose -or
        $compiler.FullName -ine $guardManifest.msbuild_cl_path -or
        [string]::IsNullOrWhiteSpace($env:VCToolsVersion) -or [string]::IsNullOrWhiteSpace($env:WindowsSDKVersion) -or
        $env:VCToolsVersion -cne $guardManifest.vc_tools_version -or
        $env:WindowsSDKVersion -cne $guardManifest.windows_sdk_version) {
        throw 'The Guard build compiler and SDK must match the current pinned toolchain.'
    }
    Assert-GuardInputs
}
$probe = Join-Path $root 'renderer-probe.exe'
Push-Location -LiteralPath $root
try {
    & $compiler.FullName /nologo /std:c++17 /permissive- /utf-8 /W4 /WX /EHsc /DUNICODE /D_UNICODE `
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
    initialCols = if ($archiveMode) { 90 } else { 72 }
    initialRows = if ($archiveMode) { 27 } else { 20 }
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
$sourceNames = @('host.cpp', 'protocol.hpp', 'renderer_pixels.hpp', 'renderer_probe.hpp', 'renderer_modal.hpp',
    'renderer_archive.hpp', 'renderer_guard.hpp', 'renderer_guard_supervisor.hpp',
    'Test-Desktop.ps1', 'Run-RendererCalibration.ps1')
if ($guardMode) {
    $sourceNames += @('guard_child.rs', 'Build.ps1', 'Validation.ps1', 'Run-Controls.ps1')
}
foreach ($source in $sourceNames) {
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
    msvc_sha256 = FileDigest $compiler.FullName
    windows_version = [Environment]::OSVersion.Version.ToString()
    os_architecture = $osArchitecture
    image_os = $env:ImageOS
    image_version = $env:ImageVersion
    input_desktop = $desktop
}
if ($archiveMode) {
    $config = Join-Path $work 'config'
    $null = New-Item -ItemType Directory -Path $config
    $clientConfig = Join-Path $config 'config.toml'
    [IO.File]::WriteAllText($clientConfig, @'
schema_version = 1
[ui]
theme = "material"
color = "always"
color-depth = "truecolor"
reduced-motion = true
'@ + "`n")
    $build['archived_client_sha256'] = $clientHash
    $build['archived_client_architecture'] = $clientArchitecture
    $build['archived_client_config_sha256'] = FileDigest $clientConfig
    $build['candidate_source'] = $CandidateSource.ToLowerInvariant()
    $build['candidate_build'] = $CandidateBuild
}
if ($guardMode) {
    $build['guard_build_manifest_sha256'] = $guardManifestHash
    $build['guard_library_sha256'] = $guardManifest.library.sha256
    $build['guard_child_sha256'] = $guardChild.sha256
    $build['guard_child_architecture'] = $osArchitecture
    $build['guard_rust_release'] = '1.97.1'
    $build['guard_vc_tools_version'] = $guardManifest.vc_tools_version
    $build['guard_windows_sdk_version'] = $guardManifest.windows_sdk_version
}
[IO.File]::WriteAllText((Join-Path $work 'build.json'), ((ConvertTo-Json $build -Depth 6) + "`n"))

function Invoke-RendererSupervisor([string] $EntryPoint, [string] $Directory, [string] $LogStem, [string[]] $ExtraArgs) {
    $nonce = [Guid]::NewGuid().ToString('N')
    $stdout = Join-Path $root "$LogStem.stdout"
    $stderr = Join-Path $root "$LogStem.stderr"
    $controllerArgs = @($EntryPoint, ('"' + $terminal.FullName + '"'), ('"' + $Directory + '"'), $nonce, $rendererProfileId)
    $controllerArgs += $ExtraArgs
    $bound = if ($EntryPoint -ceq '--renderer-guard-supervise') { 25000 } else { 40000 }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $controller = Start-Process -FilePath $probe -WorkingDirectory $Directory -NoNewWindow -PassThru `
        -ArgumentList $controllerArgs -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    try {
        if (-not $controller.WaitForExit([Math]::Max(0, $bound - [int]$watch.ElapsedMilliseconds))) {
            if (-not $controller.HasExited) {
                $controller.Kill()
                $null = $controller.WaitForExit([Math]::Max(0, $bound - [int]$watch.ElapsedMilliseconds))
            }
            throw 'The native renderer helper exceeded its outer deadline.'
        }
        $exitCode = $controller.ExitCode
    }
    finally {
        $controller.Dispose()
    }
    if ((Get-Item -LiteralPath $stdout).Length -gt 16384 -or
        (Get-Item -LiteralPath $stderr).Length -gt 49152) {
        throw 'The native renderer receipt exceeded its size limit.'
    }
    $receipt = [IO.File]::ReadAllText($stdout).Trim()
    if ([string]::IsNullOrWhiteSpace($receipt) -or $receipt.Length -gt 16384) {
        throw "The native renderer helper did not supply a bounded receipt (exit $exitCode)."
    }
    return [pscustomobject]@{ ExitCode = $exitCode; Text = $receipt; Facts = ConvertFrom-Json -InputObject $receipt }
}

function Invoke-GuardRendererCase([string] $Mode, [int] $Bits, [int] $ExpectedExit) {
    $label = "guard-$Mode-$Bits"
    $caseRoot = Join-Path $root $label
    $null = New-Item -ItemType Directory -Path $caseRoot
    Assert-GuardInput $guardChild.path $guardChild.sha256
    $result = Invoke-RendererSupervisor '--renderer-guard-supervise' $caseRoot $label @(
        ('"' + $guardChild.path + '"'), $Mode, [string]$Bits)
    $facts = $result.Facts
    if ($result.ExitCode -ne 0) {
        if ($facts.schema -eq 1 -and $facts.case -ceq 'renderer-guard' -and
            $facts.guard.mode -ceq $Mode -and $facts.guard.option_bits -eq $Bits) {
            $failureRoot = Join-Path $work $label
            $null = New-Item -ItemType Directory -Path $failureRoot
            [IO.File]::WriteAllText((Join-Path $failureRoot 'renderer-report.json'), ($result.Text + "`n"))
        }
        if ($facts.failure -is [string] -and $facts.failure -cmatch '\A[\x20-\x7E]{1,768}\z') {
            Write-Host ('Guard runtime diagnostic: ' + $facts.failure)
        }
        throw "Windows Terminal Guard $Mode bits $Bits failed (supervisor exit $($result.ExitCode))."
    }
    if ((Get-W07Number $facts.schema) -ne 1 -or $facts.case -cne 'renderer-guard' -or
        $facts.guard.mode -cne $Mode -or (Get-W07Number $facts.guard.option_bits) -ne $Bits -or
        (Get-W07Number $facts.guard.expected_exit) -ne $ExpectedExit -or
        (Get-W07Number $facts.guard.exit_code) -ne $ExpectedExit -or
        -not [string]::IsNullOrEmpty($facts.failure) -or
        (Get-W07Number $facts.cleanup.job_remaining) -ne 0) {
        throw 'The Guard receipt did not identify the requested case, expected child exit and cleanup.'
    }
    foreach ($flag in @($facts.native_windows, $facts.guard_diagnostic_passed, $facts.guard.owned,
            $facts.guard.exit_waited, $facts.guard.restored_modes, $facts.guard.wrong_job_rejected,
            $facts.cleanup.job_empty, $facts.cleanup.pipe_quiescent)) {
        if ($flag -isnot [bool] -or -not $flag) {
            throw 'The Guard receipt did not establish all required native observations.'
        }
    }

    # Native scratch may also contain owned configuration and non-evidence logs.
    # Copy only validated file bytes into the separate workflow evidence tree.
    $captures = @('baseline.bmp', 'guard-acquired.bmp', 'guard-restored.bmp' | ForEach-Object {
        $file = Get-GuardFile (Join-Path $caseRoot $_) 1MB
        Assert-GuardCapture $file
        $file
    })
    $caseBytes = [Text.Encoding]::UTF8.GetByteCount($result.Text + "`n") + ($captures | Measure-Object Length -Sum).Sum
    if ((Get-W07InventoryBytes $work) + $caseBytes -gt 32MB) {
        throw 'The Guard renderer matrix exceeded its cumulative evidence limit.'
    }
    $captureRoot = Join-Path $work $label
    $null = New-Item -ItemType Directory -Path $captureRoot
    foreach ($file in $captures) {
        [IO.File]::Copy($file.FullName, (Join-Path $captureRoot $file.Name))
    }
    [IO.File]::WriteAllText((Join-Path $captureRoot 'renderer-report.json'), ($result.Text + "`n"))
    Write-Output "Guard $Mode bits $Bits passed with child exit $ExpectedExit."
}

if ($guardMode) {
    $cases = [ordered]@{ normal = 0; error = 23; panic = 101 }
    try {
        $calibrationRoot = Join-Path $root 'guard-calibration'
        $null = New-Item -ItemType Directory -Path $calibrationRoot
        $calibration = Invoke-RendererSupervisor '--renderer-supervise' $calibrationRoot 'guard-calibration' @()
        if ($calibration.ExitCode -ne 0 -or $calibration.Facts.case -cne 'renderer-calibration' -or
            $calibration.Facts.calibration_complete -ne $true -or @($calibration.Facts.phases).Count -ne 6) {
            throw 'The renderer must pass its six synthetic cursor controls before the Guard cases.'
        }
        $calibrationEvidence = Join-Path $work 'guard-calibration'
        $null = New-Item -ItemType Directory -Path $calibrationEvidence
        foreach ($index in 0..5) {
            $capture = Get-GuardFile (Join-Path $calibrationRoot "phase-$index.bmp") 1MB
            Assert-GuardCapture $capture
            [IO.File]::Copy($capture.FullName, (Join-Path $calibrationEvidence $capture.Name))
        }
        [IO.File]::WriteAllText((Join-Path $calibrationEvidence 'renderer-report.json'), ($calibration.Text + "`n"))
        Invoke-GuardRendererCase 'normal' 5 $cases.normal
        if ($GuardMatrix) {
            foreach ($mode in $cases.Keys) {
                foreach ($bits in 0..7) {
                    if ($mode -cne 'normal' -or $bits -ne 5) {
                        Invoke-GuardRendererCase $mode $bits $cases[$mode]
                    }
                }
            }
        }
    }
    finally {
        Assert-GuardInputs
    }
    return
}

$entryPoint = if ($archiveMode) { '--renderer-archive-supervise' } else { '--renderer-supervise' }
$extraArgs = @()
if ($archiveMode) {
    if ((FileDigest $client.FullName) -cne $clientHash) {
        throw 'The archived client changed before it could be launched.'
    }
    $extraArgs = @('"' + $client.FullName + '"')
}
$result = Invoke-RendererSupervisor $entryPoint $work 'renderer' $extraArgs
$receipt = $result.Text
$facts = $result.Facts
[IO.File]::WriteAllText((Join-Path $work 'renderer-report.json'), ($receipt + "`n"))
Write-Output $receipt
if ($result.ExitCode -ne 0) {
    $check = if ($archiveMode) { 'archived-client renderer verification' } else { 'renderer calibration' }
    throw "The Windows Terminal $check failed (exit $($result.ExitCode))."
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
