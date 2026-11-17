param(
    [string]$TargetDir = 'D:\swarm-tmp\baritone-target-wA',
    [string]$TargetTriple = 'x86_64-pc-windows-msvc',
    [string]$EnvironmentScript = 'D:\sunset-work\toolchain\baritone\env.ps1',
    [switch]$SkipTests,
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$buildTarget = [System.IO.Path]::GetFullPath($TargetDir)
if (-not $repoRoot.StartsWith('D:\', [System.StringComparison]::OrdinalIgnoreCase) -or
    -not $buildTarget.StartsWith('D:\', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'Baritone sources and build outputs must be on D:.'
}
if ($TargetTriple -ne 'x86_64-pc-windows-msvc') { throw 'V1 supports x86_64-pc-windows-msvc only.' }
if ($EnvironmentScript -and (Test-Path -LiteralPath $EnvironmentScript)) { . $EnvironmentScript }
$env:TEMP = 'D:\swarm-tmp'
$env:TMP = 'D:\swarm-tmp'
$env:CARGO_TARGET_DIR = $buildTarget
if (-not $env:CARGO_HOME -or -not $env:CARGO_HOME.StartsWith('D:\', [System.StringComparison]::OrdinalIgnoreCase)) {
    $env:CARGO_HOME = 'D:\cargo-home'
}
if (-not $env:RUSTUP_HOME -or -not $env:RUSTUP_HOME.StartsWith('D:\', [System.StringComparison]::OrdinalIgnoreCase)) {
    $env:RUSTUP_HOME = 'D:\rustup-home'
}
$env:CARGO_TERM_COLOR = 'never'
New-Item -ItemType Directory -Force -Path $env:TEMP,$env:CARGO_HOME,$env:RUSTUP_HOME,$buildTarget | Out-Null
$manifest = Join-Path $repoRoot 'baritone-engine\Cargo.toml'
$canonicalAbi = (Get-FileHash -LiteralPath (Join-Path $repoRoot 'mixins\baritone\interface\abi-v1.rs') -Algorithm SHA256).Hash
foreach ($abiCopy in @('baritone-engine\src\abi.rs','mixins\baritone\loader\src\abi.rs')) {
    if ((Get-FileHash -LiteralPath (Join-Path $repoRoot $abiCopy) -Algorithm SHA256).Hash -ne $canonicalAbi) {
        throw "ABI declarations differ from the canonical interface: $abiCopy"
    }
}
$cargoArgs = @('--locked','--manifest-path',$manifest,'--target',$TargetTriple)
if ($Offline) { $cargoArgs += '--offline' }
$savedFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = (($savedFlags + ' -Dwarnings').Trim())
    if (-not $SkipTests) {
        & cargo test @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "Baritone engine tests failed ($LASTEXITCODE)." }
    }
    & cargo build @cargoArgs --release
    if ($LASTEXITCODE -ne 0) { throw "Baritone engine DLL build failed ($LASTEXITCODE)." }
    $builtDll = Join-Path $buildTarget "$TargetTriple\release\sunset_baritone_engine.dll"
    $payloadDir = Join-Path $repoRoot "mixins\baritone\data\engine\$TargetTriple"
    New-Item -ItemType Directory -Force -Path $payloadDir | Out-Null
    $payloadDll = Join-Path $payloadDir 'sunset_baritone_engine.dll'
    Copy-Item -LiteralPath $builtDll -Destination $payloadDll -Force
    Write-Output "Baritone DLL ready: $payloadDll"
    Get-FileHash -LiteralPath $payloadDll -Algorithm SHA256 | Format-List Path,Hash
} finally { $env:RUSTFLAGS = $savedFlags }
