param(
    [string]$InstallDir = "",
    [string]$ExePath = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot

function Resolve-InstalledDirectory {
    param([string]$ExplicitDir)

    if (-not [string]::IsNullOrWhiteSpace($ExplicitDir)) {
        return $ExplicitDir.TrimEnd('\', '/')
    }

    $appIdKey = "{B7F4E2A9-8C31-4D62-9A5E-1D8F6B3C2E40}_is1"
    $registryPaths = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appIdKey",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appIdKey",
        "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$appIdKey"
    )

    foreach ($regPath in $registryPaths) {
        if (Test-Path -Path $regPath) {
            try {
                $props = Get-ItemProperty -Path $regPath -ErrorAction Stop
                if (-not [string]::IsNullOrWhiteSpace($props.InstallLocation)) {
                    $candidate = $props.InstallLocation.Trim().TrimEnd('\', '/')
                    if (Test-Path -Path $candidate -PathType Container) {
                        return $candidate
                    }
                }
            } catch {
                continue
            }
        }
    }

    return $null
}

function Resolve-BuiltExecutable {
    param([string]$ExplicitExe)

    if (-not [string]::IsNullOrWhiteSpace($ExplicitExe)) {
        if (Test-Path -Path $ExplicitExe -PathType Leaf) {
            return (Resolve-Path $ExplicitExe).Path
        }
        throw "Executable not found at explicit path: $ExplicitExe"
    }

    $candidates = @(@(
        (Join-Path $RepoRoot "target\release\manga-tui.exe"),
        (Join-Path $env:USERPROFILE ".cargo\bin\manga-tui.exe")
    ) | Where-Object { $_ -and (Test-Path -Path $_ -PathType Leaf) })

    if ($candidates.Count -gt 0) {
        $newest = $candidates |
            ForEach-Object { Get-Item -Path $_ } |
            Sort-Object -Property LastWriteTimeUtc -Descending |
            Select-Object -First 1
        return $newest.FullName
    }

    throw "No compiled manga-tui.exe found in target\release or ~\.cargo\bin"
}

$targetDir = Resolve-InstalledDirectory -ExplicitDir $InstallDir
if ([string]::IsNullOrWhiteSpace($targetDir)) {
    Write-Host "No local Inno Setup installation of Manga TUI found in registry; skipping local install sync."
    exit 0
}

$sourceExe = Resolve-BuiltExecutable -ExplicitExe $ExePath
$destExe = Join-Path $targetDir "manga-tui.exe"
$oldExe = Join-Path $targetDir "manga-tui.exe.old"

if (Test-Path -Path $oldExe) {
    Remove-Item -Path $oldExe -Force -ErrorAction SilentlyContinue
}

if (Test-Path -Path $destExe) {
    try {
        Copy-Item -Path $sourceExe -Destination $destExe -Force -ErrorAction Stop
    } catch {
        Write-Warning "manga-tui.exe is currently in use at $destExe; rotating via .old for next restart."
        Move-Item -Path $destExe -Destination $oldExe -Force
        Copy-Item -Path $sourceExe -Destination $destExe -Force
    }
} else {
    Copy-Item -Path $sourceExe -Destination $destExe -Force
}

foreach ($doc in @("README.md", "LICENSE")) {
    $srcDoc = Join-Path $RepoRoot $doc
    if (Test-Path -Path $srcDoc -PathType Leaf) {
        Copy-Item -Path $srcDoc -Destination (Join-Path $targetDir $doc) -Force
    }
}

Write-Host "Synced local Manga TUI installation:"
Write-Host "  Source: $sourceExe"
Write-Host "  Target: $destExe"
