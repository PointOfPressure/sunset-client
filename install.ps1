# Installs Solstice into Horizon on Windows, builds it into the game and starts it. One command, in PowerShell:
#
#   irm https://github.com/PointOfPressure/solstice-client/releases/latest/download/install.ps1 | iex
#
# Options (from a downloaded copy): .\install.ps1 [-NoBuild] [-NoPlay] [-File solstice.hnmod] [-GameHome C:\path\to\horizon]
param(
    [switch]$NoBuild,
    [switch]$NoPlay,
    [string]$File = "",
    [string]$GameHome = "",
    [switch]$Dry
)
$ErrorActionPreference = "Stop"

# Horizon keeps the game's files in %APPDATA%\horizon (Roaming), unless HORIZON_HOME says otherwise.
if (-not $GameHome) {
    $GameHome = if ($env:HORIZON_HOME) { $env:HORIZON_HOME } else { Join-Path $env:APPDATA "horizon" }
}
$homes = @($GameHome)
$nativeInstance = Join-Path $GameHome "instances\native-mods"
if (Test-Path $nativeInstance) { $homes += $nativeInstance }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("solstice-" + [Guid]::NewGuid().ToString("N") + ".hnmod")
if ($File) {
    Copy-Item -LiteralPath $File -Destination $tmp
} else {
    Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/PointOfPressure/solstice-client/releases/latest/download/solstice.hnmod" -OutFile $tmp
}

$utf8 = New-Object System.Text.UTF8Encoding($false)
foreach ($h in $homes) {
    $mods = Join-Path $h "horizon-native-mods"
    New-Item -ItemType Directory -Force -Path $mods | Out-Null
    Copy-Item -LiteralPath $tmp -Destination (Join-Path $mods "solstice.hnmod") -Force
    # An older name of the same mod.
    foreach ($old in @("meteor.hnmod", "delta.hnmod")) {
        Remove-Item -LiteralPath (Join-Path $mods $old) -ErrorAction SilentlyContinue
    }
    $list = Join-Path $h "horizon-mods.txt"
    $lines = @()
    if (Test-Path $list) {
        $lines = @(Get-Content -LiteralPath $list | Where-Object { $_ -notmatch '^mod\.(solstice|meteor|delta):' })
    }
    $lines += "mod.solstice: true"
    [IO.File]::WriteAllText($list, (($lines -join "`n") + "`n"), $utf8)
}
Remove-Item -LiteralPath $tmp -ErrorAction SilentlyContinue
Write-Host "Solstice: installed."

# Native mods are compiled into the game together, so they all have to be made for the same Horizon version.
$others = @()
foreach ($h in $homes) {
    $mods = Join-Path $h "horizon-native-mods"
    if (Test-Path $mods) {
        $others += @(Get-ChildItem -LiteralPath $mods -Filter "*.hnmod" | Where-Object { $_.BaseName -ne "solstice" } | ForEach-Object { $_.BaseName })
    }
}
$others = @($others | Sort-Object -Unique)
if ($others.Count -gt 0) {
    Write-Host ""
    Write-Host ("You also have other native mods: " + ($others -join ", "))
    Write-Host "They are built into the game together with Solstice, so they must be made for the same Horizon version (Solstice 0.8.0 is made for Nightly ba6344a)."
    Write-Host "If the build says 'made for different Horizon revisions', turn the others off in the launcher's Mods page, or delete their .hnmod from horizon-native-mods, and run this again."
    Write-Host ""
}

# The launcher's program: running now, on the PATH, in its usual places, or behind a shortcut.
function Find-Launcher {
    $running = Get-Process -Name "horizon-launcher" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($running -and $running.Path) { return $running.Path }
    $cmd = Get-Command "horizon-launcher.exe" -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $places = @(
        "$env:LOCALAPPDATA\Programs\Horizon Launcher\horizon-launcher.exe",
        "$env:LOCALAPPDATA\Programs\Horizon\horizon-launcher.exe",
        "$env:LOCALAPPDATA\Horizon Launcher\horizon-launcher.exe",
        "$env:LOCALAPPDATA\horizon-launcher\horizon-launcher.exe",
        "$env:ProgramFiles\Horizon Launcher\horizon-launcher.exe",
        "$env:ProgramFiles\Horizon\horizon-launcher.exe"
    )
    foreach ($p in $places) { if (Test-Path $p) { return $p } }
    $globs = @(
        "$env:USERPROFILE\Downloads\horizon-launcher*\horizon-launcher.exe",
        "$env:USERPROFILE\Downloads\horizon-launcher*.exe",
        "$env:USERPROFILE\Desktop\horizon-launcher*\horizon-launcher.exe",
        "$env:USERPROFILE\Documents\horizon-launcher*\horizon-launcher.exe"
    )
    foreach ($g in $globs) {
        $hit = Get-ChildItem -Path $g -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($hit) { return $hit.FullName }
    }
    # Shortcuts (the Start menu, the desktop) that point at it.
    $shell = New-Object -ComObject WScript.Shell
    $dirs = @("$env:APPDATA\Microsoft\Windows\Start Menu\Programs", "$env:USERPROFILE\Desktop")
    foreach ($d in $dirs) {
        foreach ($lnk in (Get-ChildItem -Path $d -Filter "*Horizon*.lnk" -Recurse -ErrorAction SilentlyContinue)) {
            $target = $shell.CreateShortcut($lnk.FullName).TargetPath
            if ($target -and $target -match 'horizon-launcher' -and (Test-Path $target)) { return $target }
        }
    }
    return $null
}

if ($NoBuild) {
    Write-Host "Next: in the Horizon launcher, Settings > Game > Native mods (experimental) on, then Build. Or run this again without -NoBuild."
    return
}

$launcher = Find-Launcher
if (-not $launcher) {
    Write-Host ""
    Write-Host "I couldn't find the Horizon launcher. Install it from https://playhorizon.dev, open it once, then run this command again."
    exit 1
}
# Horizon's build (its Windows resource compiler) fails when the path to the launcher's folder has a space in it,
# which it does for a Windows user name like "Jo Smith". Keep the launcher's files somewhere without one.
$launcherHome = $env:HORIZON_LAUNCHER_HOME
if (-not $launcherHome) { $launcherHome = Join-Path $env:APPDATA "horizon-launcher" }
if ($launcherHome -match '\s') {
    $newHome = "C:\horizon-launcher"
    Write-Host ""
    Write-Host "Your Windows user name has a space in it, and Horizon's build can't handle that."
    Write-Host "Using $newHome for the launcher's files instead. The launcher will ask you to sign in again, and it fetches its files again (a few minutes)."
    $env:HORIZON_LAUNCHER_HOME = $newHome
    [Environment]::SetEnvironmentVariable("HORIZON_LAUNCHER_HOME", $newHome, "User")
    Write-Host "If the launcher window is open, close it and open it again so it uses the new folder."
    Write-Host ""
}
Write-Host "Solstice: building it into the game (10 to 30 minutes the first time, a few after that; Windows needs nothing else installed)..."
if ($Dry) {
    Write-Host "would run: $launcher native-build --trust-native-code"
    if (-not $NoPlay) { Write-Host "would run: $launcher play --instance native-mods" }
    return
}
& $launcher native-build --trust-native-code
if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "The build failed. If it said the mods were 'made for different Horizon revisions', you have another native mod built for a different Horizon version: turn it off in the launcher's Mods page (or delete its .hnmod from horizon-native-mods) and run this again."
    exit $LASTEXITCODE
}
Write-Host "Solstice: done."
if ($NoPlay) {
    Write-Host 'Play the "Native mods" instance in the Horizon launcher. In the game, press the ` key.'
} else {
    Write-Host 'Starting Horizon (the "Native mods" instance). In the game, press the ` key.'
    Start-Process -FilePath $launcher -ArgumentList @("play", "--instance", "native-mods")
}
