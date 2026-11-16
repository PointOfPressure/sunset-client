param(
    [Parameter(Mandatory=$true)][string]$StageDirectory,
    [string]$OutputDirectory = '',
    [ValidateSet('fast','release')][string]$Profile = 'fast',
    [string]$EnvironmentScript = 'D:\sunset-work\toolchain\env.ps1',
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$stageRoot = (Resolve-Path -LiteralPath $StageDirectory).Path
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repoRoot "dist\playable\$Profile" }
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
foreach ($path in @($stageRoot,$outputRoot)) {
    if (-not $path.StartsWith('D:\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Native sources and playable build outputs must stay on D:.'
    }
}
foreach ($name in @('Cargo.toml','Cargo.lock','.horizon-mixins-stage.json')) {
    if (-not (Test-Path -LiteralPath (Join-Path $stageRoot $name) -PathType Leaf)) {
        throw "Use a Horizon source stage produced by horizon-mixins apply: missing $name"
    }
}
if ($EnvironmentScript) { . $EnvironmentScript }
if (-not $env:CARGO_TARGET_DIR) { throw 'CARGO_TARGET_DIR must name a build directory on D:.' }
$targetRoot = [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
if (-not $targetRoot.StartsWith('D:\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'CARGO_TARGET_DIR must stay on D:.'
}
$cargoArgs = @('build','--locked','--profile',$Profile,'-p','horizon-client','-p','horizon-client-lib',
               '--bin','horizon-client','--bin','horizon-ctl')
if ($Offline) { $cargoArgs += '--offline' }
Push-Location $stageRoot
try {
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "Playable client build failed ($LASTEXITCODE)." }
} finally { Pop-Location }
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$artifacts = @()
foreach ($name in @('horizon-client.exe','horizon-ctl.exe')) {
    $built = Join-Path $targetRoot "$Profile\$name"
    $destination = Join-Path $outputRoot $name
    Copy-Item -LiteralPath $built -Destination $destination -Force
    $artifacts += [pscustomobject]@{
        path=$destination
        sha256=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        bytes=(Get-Item -LiteralPath $destination).Length
    }
}
$receipt = [pscustomobject]@{
    built_at=[DateTime]::UtcNow.ToString('o')
    profile=$Profile
    stage=$stageRoot
    generated_game_sha256=(Get-FileHash -LiteralPath (Join-Path $stageRoot 'crates\horizon-client\src\game.rs') -Algorithm SHA256).Hash.ToLowerInvariant()
    artifacts=$artifacts
}
$receiptPath = Join-Path $outputRoot 'build.json'
$receipt | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receiptPath
Write-Output "Playable $Profile build: $outputRoot"
Write-Output "Build receipt: $receiptPath"
