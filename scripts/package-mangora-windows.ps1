param(
    [string]$Bundles = "nsis,msi",
    [string]$OutputDir = "dist-mangora"
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$MangoraDir = Join-Path $RepoRoot "apps\mangora"
$TargetBundleDir = Join-Path $RepoRoot "target\release\bundle"
$ResolvedOutputDir = Join-Path $RepoRoot $OutputDir

Write-Host "==> Building Mangora frontend & Windows bundles ($Bundles)..." -ForegroundColor Cyan
Push-Location $MangoraDir
try {
    npm.cmd run build
    if ($LASTEXITCODE -ne 0) {
        throw "Frontend build failed with exit code $LASTEXITCODE"
    }

    npm.cmd run tauri build -- --bundles $Bundles
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri build failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path $ResolvedOutputDir | Out-Null

$Artifacts = @()
foreach ($Folder in @("nsis", "msi")) {
    $Src = Join-Path $TargetBundleDir $Folder
    if (Test-Path $Src) {
        Get-ChildItem -Path $Src -File | Where-Object { $_.Extension -in @(".exe", ".msi") } | ForEach-Object {
            $Dest = Join-Path $ResolvedOutputDir $_.Name
            Copy-Item -Path $_.FullName -Destination $Dest -Force
            $Artifacts += (Get-Item $Dest)
        }
    }
}

if ($Artifacts.Count -eq 0) {
    throw "No Mangora Windows installer artifacts were found under $TargetBundleDir"
}

$ChecksumsPath = Join-Path $ResolvedOutputDir "SHA256SUMS.txt"
$ChecksumLines = foreach ($File in $Artifacts) {
    $Hash = (Get-FileHash -Path $File.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$Hash  $($File.Name)"
}
Set-Content -Path $ChecksumsPath -Value $ChecksumLines -Encoding UTF8

Write-Host "==> Mangora Windows packages exported to $ResolvedOutputDir" -ForegroundColor Green
$ChecksumLines | ForEach-Object { Write-Host "    $_" }
