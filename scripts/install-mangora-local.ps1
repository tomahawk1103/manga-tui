param(
    [switch]$SkipShortcut
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$MangoraDir = Join-Path $RepoRoot "apps\mangora"
$IconSource = Join-Path $MangoraDir "src-tauri\icons\icon.ico"
$ReleaseExe = Join-Path $RepoRoot "target\release\mangora.exe"
$InstallDir = Join-Path $env:LOCALAPPDATA "Mangora"
$InstalledExe = Join-Path $InstallDir "mangora.exe"
$InstalledIcon = Join-Path $InstallDir "mangora.ico"
$CargoBinExe = Join-Path $env:USERPROFILE ".cargo\bin\mangora.exe"

Write-Host "==> 1/3 Building Mangora frontend (dist)..." -ForegroundColor Cyan
Push-Location $MangoraDir
try {
    npm.cmd run build
    if ($LASTEXITCODE -ne 0) {
        throw "Mangora frontend build failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

Write-Host "==> 2/3 Building standalone Mangora release binary..." -ForegroundColor Cyan
Push-Location $RepoRoot
try {
    cargo build --release -p mangora --features custom-protocol --offline
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo release build for mangora failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path $ReleaseExe)) {
    throw "Expected Mangora release binary not found at $ReleaseExe"
}

Write-Host "==> 3/3 Installing Mangora executable & shortcuts..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Path $ReleaseExe -Destination $InstalledExe -Force
if (Test-Path $IconSource) {
    Copy-Item -Path $IconSource -Destination $InstalledIcon -Force
}

$CargoBinDir = Split-Path -Parent $CargoBinExe
if (Test-Path $CargoBinDir) {
    try {
        Copy-Item -Path $ReleaseExe -Destination $CargoBinExe -Force
    }
    catch {
        Write-Warning "Could not update $CargoBinExe ($($_.Exception.Message))."
    }
}

if (-not $SkipShortcut) {
    $WshShell = New-Object -ComObject WScript.Shell
    $DesktopDir = [Environment]::GetFolderPath("Desktop")
    $StartMenuDir = Join-Path ([Environment]::GetFolderPath("StartMenu")) "Programs"
    $ShortcutTargets = @()
    if ($DesktopDir -and (Test-Path $DesktopDir)) {
        $ShortcutTargets += (Join-Path $DesktopDir "Mangora.lnk")
    }
    if ($StartMenuDir -and (Test-Path $StartMenuDir)) {
        $ShortcutTargets += (Join-Path $StartMenuDir "Mangora.lnk")
    }

    foreach ($ShortcutPath in $ShortcutTargets) {
        $Shortcut = $WshShell.CreateShortcut($ShortcutPath)
        $Shortcut.TargetPath = $InstalledExe
        $Shortcut.WorkingDirectory = $InstallDir
        $Shortcut.Description = "Mangora - Lector de manga multiplataforma"
        if (Test-Path $InstalledIcon) {
            $Shortcut.IconLocation = "$InstalledIcon, 0"
        }
        $Shortcut.Save()
        Write-Host "    Shortcut created: $ShortcutPath" -ForegroundColor Green
    }
}

Write-Host "==> Mangora installed at $InstalledExe" -ForegroundColor Green
