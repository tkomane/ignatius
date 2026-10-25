# Build only. This does not execute a Windows capability case.
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $SourceRoot,
    [Parameter(Mandatory)] [string] $BuildRoot,
    [Parameter(Mandatory)] [string] $CargoHome,
    [Parameter(Mandatory)] [string] $CargoTarget,
    [string] $ExpectedRevision = 'd6d65e717351d5e48c72bb28410a9e121917b060'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows) { throw 'Build.ps1 requires actual Windows with installed MSVC/SDK; no cross-platform native claim' }
. (Join-Path $PSScriptRoot 'Validation.ps1')
$source = (Resolve-Path -LiteralPath $SourceRoot).Path
Assert-W07 ((Resolve-Path -LiteralPath (Join-Path $source 'tests/support/windows_conpty')).Path -ceq $PSScriptRoot) 'helper source must belong to the selected isolated source'
Assert-W07 ([IO.Path]::IsPathFullyQualified($BuildRoot) -and -not (Test-Path -LiteralPath $BuildRoot)) 'new absolute owned build root required'
Assert-W07 ([IO.Path]::IsPathFullyQualified($CargoHome) -and [IO.Path]::IsPathFullyQualified($CargoTarget)) 'absolute task-owned Cargo paths required'
$null = New-Item -ItemType Directory -Path $BuildRoot
$build = (Resolve-Path -LiteralPath $BuildRoot).Path
Push-Location $source
try {
    $revision = (& git rev-parse HEAD).Trim(); Assert-W07 ($LASTEXITCODE -eq 0 -and $revision -ceq $ExpectedRevision) 'exact frozen source revision'
    & git diff --quiet; Assert-W07 ($LASTEXITCODE -eq 0) 'no tracked source changes'
    & git diff --cached --quiet; Assert-W07 ($LASTEXITCODE -eq 0) 'no staged source changes'
    $status = @(& git status --porcelain=v1 --untracked-files=all)
    foreach ($line in $status) { Assert-W07 ($line.StartsWith('?? tests/support/windows_conpty/')) 'only isolated new harness files allowed' }
    # Freeze helper, complete tracked source and lock bytes BEFORE compiler or
    # test calls. Final checks compare these exact inputs and Git authority.
    $sourceNames = @('protocol.hpp','protocol_tests.cpp','host.cpp','guard_child.rs','Validation.ps1','Build.ps1','Run-Controls.ps1','Test-Validation.ps1','README.md')
    $inputs = @($sourceNames | ForEach-Object { $p = Join-Path $PSScriptRoot $_; @{ path=$p; sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $p).Hash.ToLowerInvariant() } })
    $tracked = @(& git ls-files); Assert-W07 ($LASTEXITCODE -eq 0) 'tracked source input inventory'
    $sourceInputs = @($tracked | ForEach-Object { $p=Join-Path $source $_; @{path=$p;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $p).Hash.ToLowerInvariant()} })
    $lockHash=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $source 'Cargo.lock')).Hash.ToLowerInvariant()
    @{helper_inputs=$inputs;source_inputs=$sourceInputs;lock_sha256=$lockHash;revision=$revision;untracked_status=$status} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $build 'prebuild-inputs.json') -Encoding utf8NoBOM
    $rust = (& rustc --version --verbose | Out-String).Trim(); Assert-W07 ($LASTEXITCODE -eq 0 -and (Test-W07PinnedRust $rust)) 'installed pinned Rust 1.97.1 required'
    $cargoVersion = (& cargo --version | Out-String).Trim(); Assert-W07 ($LASTEXITCODE -eq 0) 'cargo version'
    $cl = (Get-Command cl.exe -CommandType Application -ErrorAction Stop).Source
    $clHash=(Get-FileHash -Algorithm SHA256 -LiteralPath $cl).Hash.ToLowerInvariant()
    $env:CARGO_HOME = $CargoHome; $env:CARGO_TARGET_DIR = $CargoTarget
    $env:IGNATIUS_BUILD_IDENTITY = 'w07-064-d6d65e7-capability-controls'
    & cargo --locked --offline build --lib --message-format=json 1> (Join-Path $build 'cargo-lib.jsonl') 2> (Join-Path $build 'cargo-lib.log')
    Assert-W07 ($LASTEXITCODE -eq 0) 'locked offline non-test library build'
    $messages = @(Get-Content -LiteralPath (Join-Path $build 'cargo-lib.jsonl') | ForEach-Object { $_ | ConvertFrom-Json })
    $selected = @($messages | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'ignatius' -and $_.target.kind -contains 'lib' -and -not $_.profile.test -and $_.manifest_path -eq (Join-Path $source 'Cargo.toml') })
    Assert-W07 ($selected.Count -eq 1) 'one Cargo JSON-selected production library'
    $library = @($selected[0].filenames | Where-Object { $_.EndsWith('.rlib') }); Assert-W07 ($library.Count -eq 1) 'one rlib'
    $dependencyDirectories = @($messages | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.manifest_path -ne (Join-Path $source 'Cargo.toml') -and -not $_.profile.test } | ForEach-Object { $_.filenames } | Where-Object { $_.EndsWith('.rlib') } | ForEach-Object { Split-Path -Parent $_ } | Sort-Object -Unique)
    Assert-W07 ($dependencyDirectories.Count -eq 1) 'one Cargo JSON-derived native dependency directory'
    $deps = $dependencyDirectories[0]
    $link = @('-L', "dependency=$deps")
    foreach ($message in $messages | Where-Object { $_.reason -eq 'build-script-executed' }) { foreach ($path in $message.linked_paths) { $link += @('-L', $path) } }
    & rustc --edition=2024 --deny warnings (Join-Path $PSScriptRoot 'guard_child.rs') --extern "ignatius=$($library[0])" @link -o (Join-Path $build 'guard-child.exe') 1> (Join-Path $build 'rustc.log') 2>&1
    Assert-W07 ($LASTEXITCODE -eq 0) 'safe actual-Guard child build'
    Push-Location $build
    try {
        & $cl /nologo /std:c++17 /permissive- /utf-8 /W4 /WX /EHsc /DUNICODE /D_UNICODE (Join-Path $PSScriptRoot 'host.cpp') /Fe:conpty-host.exe /Fo:conpty-host.obj /link Advapi32.lib 1> cl-host.log 2>&1
        Assert-W07 ($LASTEXITCODE -eq 0) 'installed MSVC/SDK native host build'
        & $cl /nologo /std:c++17 /permissive- /utf-8 /W4 /WX /EHsc (Join-Path $PSScriptRoot 'protocol_tests.cpp') /Fe:protocol-tests.exe /Fo:protocol-tests.obj 1> cl-protocol.log 2>&1
        Assert-W07 ($LASTEXITCODE -eq 0) 'pure protocol build'
        & (Join-Path $build 'protocol-tests.exe') 1> protocol-green.log 2>&1
        Assert-W07 ($LASTEXITCODE -eq 0) 'pure protocol controls'
    } finally { Pop-Location }
    & (Join-Path $PSScriptRoot 'Test-Validation.ps1') 1> (Join-Path $build 'validation-green.log') 2>&1
    Assert-W07 ($LASTEXITCODE -eq 0) 'pure receipt validation'
    Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath $cl).Hash.ToLowerInvariant() -ceq $clHash) 'MSVC compiler input unchanged'
    Assert-W07FrozenInputs (@($inputs) + @($sourceInputs))
    Assert-W07 ((Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $source 'Cargo.lock')).Hash.ToLowerInvariant() -ceq $lockHash) 'prehashed source lock unchanged'
    $afterRevision=(& git rev-parse HEAD).Trim(); Assert-W07 ($LASTEXITCODE -eq 0 -and $afterRevision -ceq $revision) 'source revision unchanged after build/tests'
    & git diff --quiet; Assert-W07 ($LASTEXITCODE -eq 0) 'tracked source unchanged after build/tests'
    & git diff --cached --quiet; Assert-W07 ($LASTEXITCODE -eq 0) 'staged source unchanged after build/tests'
    $afterStatus=@(& git status --porcelain=v1 --untracked-files=all); Assert-W07 ($LASTEXITCODE -eq 0 -and ($afterStatus -join "`n") -ceq ($status -join "`n")) 'source inventory unchanged after build/tests'
    $binaries = @('conpty-host.exe','guard-child.exe','protocol-tests.exe') | ForEach-Object { $p = Join-Path $build $_; @{ path=$p; sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $p).Hash.ToLowerInvariant() } }
    $linkInputs = @(Get-ChildItem -LiteralPath $deps -File | Sort-Object FullName | ForEach-Object { @{ path=$_.FullName; sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant() } })
    $os = [Environment]::OSVersion
    $manifest = [ordered]@{ schema=1; scope='first-capability-controls-only'; native_executed=$false; revision=$revision; source=$source; tracked_source_unchanged=$true; untracked_status=$status; lock_sha256=$lockHash; pre_post_inputs_verified=$true; source_inputs=$sourceInputs; prebuild_manifest_sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $build 'prebuild-inputs.json')).Hash.ToLowerInvariant(); rust_verbose=$rust; cargo_version=$cargoVersion; msbuild_cl_path=$cl; cl_sha256=$clHash; vc_tools_version=$env:VCToolsVersion; windows_sdk_version=$env:WindowsSDKVersion; os_version=$os.VersionString; os_description=[Runtime.InteropServices.RuntimeInformation]::OSDescription; os_build=$os.Version.Build; powershell_version=$PSVersionTable.PSVersion.ToString(); dotnet_version=[Environment]::Version.ToString(); cl_file_version=[Diagnostics.FileVersionInfo]::GetVersionInfo($cl).FileVersion; runner_image_os=$env:ImageOS; runner_image_version=$env:ImageVersion; library=@{path=$library[0];sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $library[0]).Hash.ToLowerInvariant();cargo_profile=$selected[0].profile}; dependency_directory=$deps; link_inputs=$linkInputs; helper_inputs=$inputs; binaries=@($binaries); build_root=$build; cargo_home=$CargoHome; cargo_target=$CargoTarget; compile_inputs_and_cache_excluded_from_experiment_budget=$true }
    $manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $build 'build-manifest.json') -Encoding utf8NoBOM
} finally { Pop-Location }
