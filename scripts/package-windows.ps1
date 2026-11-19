param(
    [string]$Version = "",
    [string]$Target = "",
    [string]$OutputDir = "dist",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

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
        Join-Path $RepoRoot "target\$Target\release\manga-tui.exe"
    }

    if (-not (Test-Path -Path $ExePath -PathType Leaf)) {
        throw "Built executable not found at: $ExePath"
    }

    $ResolvedOutputDir = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
        $OutputDir
    } else {
        Join-Path $RepoRoot $OutputDir
    }
    New-Item -ItemType Directory -Path $ResolvedOutputDir -Force | Out-Null

    $StagingDir = Join-Path $ResolvedOutputDir "portable-staging"
    if (Test-Path $StagingDir) {
        Remove-Item -Path $StagingDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null

    Copy-Item -Path $ExePath -Destination (Join-Path $StagingDir "manga-tui.exe") -Force
    Copy-Item -Path (Join-Path $RepoRoot "README.md") -Destination (Join-Path $StagingDir "README.md") -Force
    Copy-Item -Path (Join-Path $RepoRoot "LICENSE") -Destination (Join-Path $StagingDir "LICENSE") -Force
    Copy-Item -Path (Join-Path $RepoRoot "scripts\portable.txt") -Destination (Join-Path $StagingDir "portable.txt") -Force
    Copy-Item -Path (Join-Path $RepoRoot "docs\LEEME_PORTABLE.txt") -Destination (Join-Path $StagingDir "LEEME_PORTABLE.txt") -Force

    $ZipFileName = "manga-tui-$Version-$TargetName-portable.zip"
    $ZipPath = Join-Path $ResolvedOutputDir $ZipFileName
    $ShaPath = "$ZipPath.sha256"

    if (Test-Path $ZipPath) {
        Remove-Item -Path $ZipPath -Force
    }
    if (Test-Path $ShaPath) {
        Remove-Item -Path $ShaPath -Force
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $StagingDir,
        $ZipPath,
        [System.IO.Compression.CompressionLevel]::Optimal,
        $false
    )

    # Verify required entries inside the generated ZIP
    $ZipFullPath = (Resolve-Path $ZipPath).Path
    $Archive = [System.IO.Compression.ZipFile]::OpenRead($ZipFullPath)
    try {
        $EntryNames = @($Archive.Entries | ForEach-Object { $_.FullName })
    } finally {
        $Archive.Dispose()
    }

    foreach ($Required in @("manga-tui.exe", "portable.txt", "LEEME_PORTABLE.txt")) {
        if ($EntryNames -notcontains $Required) {
            throw "Verification failed: '$Required' is missing from $ZipFileName"
        }
    }

    $Hash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -Path $ShaPath -Value "$Hash  $ZipFileName" -Encoding Ascii

    Remove-Item -Path $StagingDir -Recurse -Force

    Write-Host "Portable ZIP created and verified:"
    Write-Host "  ZIP: $ZipPath"
    Write-Host "  SHA256: $ShaPath ($Hash)"
} finally {
    Pop-Location
}
