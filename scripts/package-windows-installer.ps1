param(
    [string]$Version = "",
    [string]$Target = "",
    [string]$OutputDir = "dist",
    [string]$IsccPath = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

function Resolve-IsccExecutable {
    param([string]$ExplicitPath)

    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        if (Test-Path -Path $ExplicitPath -PathType Leaf) {
            return (Resolve-Path $ExplicitPath).Path
        }
        throw "ISCC.exe not found at explicit path: $ExplicitPath"
    }

    $fromPath = Get-Command "ISCC.exe" -ErrorAction SilentlyContinue
    if ($fromPath) {
        return $fromPath.Source
    }

    $candidates = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe")
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -Path $candidate -PathType Leaf)) {
            return $candidate
        }
    }

    throw "Inno Setup 6 compiler (ISCC.exe) was not found. Install Inno Setup 6 (e.g. 'winget install JRSoftware.InnoSetup' or 'choco install innosetup') or pass -IsccPath."
}

try {
    if ([string]::IsNullOrWhiteSpace($Version)) {
        $CargoToml = Get-Content -Path (Join-Path $RepoRoot "Cargo.toml") -Raw
        if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
            $Version = $Matches[1]
        } else {
            throw "Could not determine version from Cargo.toml"
        }
    }

    $TargetName = if ([string]::IsNullOrWhiteSpace($Target)) { "x86_64-pc-windows-msvc" } else { $Target }

    $IconPath = Join-Path $RepoRoot "assets\icon.ico"
    if (-not (Test-Path -Path $IconPath -PathType Leaf)) {
        throw "Required application icon not found at: $IconPath"
    }

    if (-not $SkipBuild) {
        Write-Host "Building manga-tui v$Version ($TargetName)..."
        if ([string]::IsNullOrWhiteSpace($Target)) {
            & cargo build --release
        } else {
            & cargo build --release --target $Target
        }
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release failed with exit code $LASTEXITCODE"
        }
    }

    $ExePath = if ([string]::IsNullOrWhiteSpace($Target)) {
        Join-Path $RepoRoot "target\release\manga-tui.exe"
    } else {
        Join-Path $RepoRoot "target\$Target\manga-tui.exe"
    }

    if (-not (Test-Path -Path $ExePath -PathType Leaf)) {
        throw "Built executable not found at: $ExePath"
    }

    # Verify embedded Win32 metadata and icon before packaging installer
    $VersionInfo = (Get-Item -Path $ExePath).VersionInfo
    if ($VersionInfo.ProductName -ne "Manga TUI") {
        throw "Verification failed: expected ProductName 'Manga TUI', got '$($VersionInfo.ProductName)'"
    }
    if ([string]::IsNullOrWhiteSpace($VersionInfo.FileDescription)) {
        throw "Verification failed: FileDescription metadata is missing from $ExePath"
    }
    if ([string]::IsNullOrWhiteSpace($VersionInfo.FileVersion)) {
        throw "Verification failed: FileVersion metadata is missing from $ExePath"
    }

    Add-Type -AssemblyName System.Drawing
    $ExtractedIcon = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path $ExePath).Path)
    if ($null -eq $ExtractedIcon) {
        throw "Verification failed: could not extract embedded icon from $ExePath"
    }
    $ExtractedIcon.Dispose()

    $ResolvedOutputDir = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
        $OutputDir
    } else {
        Join-Path $RepoRoot $OutputDir
    }
    New-Item -ItemType Directory -Path $ResolvedOutputDir -Force | Out-Null

    $IsccExe = Resolve-IsccExecutable -ExplicitPath $IsccPath
    $IssScript = Join-Path $RepoRoot "scripts\windows-installer.iss"
    $BaseFilename = "manga-tui-$Version-$TargetName-setup"
    $SetupPath = Join-Path $ResolvedOutputDir "$BaseFilename.exe"
    $ShaPath = "$SetupPath.sha256"

    if (Test-Path $SetupPath) {
        Remove-Item -Path $SetupPath -Force
    }
    if (Test-Path $ShaPath) {
        Remove-Item -Path $ShaPath -Force
    }

    Write-Host "Compiling Windows installer with $IsccExe..."
    & $IsccExe `
        "/DMyAppVersion=$Version" `
        "/DMyAppTarget=$TargetName" `
        "/DMyAppExePath=$ExePath" `
        "/DMyOutputDir=$ResolvedOutputDir" `
        "/DMyOutputBaseFilename=$BaseFilename" `
        $IssScript

    if ($LASTEXITCODE -ne 0) {
        throw "ISCC.exe failed with exit code $LASTEXITCODE"
    }

    if (-not (Test-Path -Path $SetupPath -PathType Leaf)) {
        throw "Expected installer executable was not produced at: $SetupPath"
    }

    $Hash = (Get-FileHash -Path $SetupPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -Path $ShaPath -Value "$Hash  $BaseFilename.exe" -Encoding Ascii

    Write-Host "Windows installer created and verified:"
    Write-Host "  Setup:  $SetupPath"
    Write-Host "  SHA256: $ShaPath ($Hash)"
} finally {
    Pop-Location
}
