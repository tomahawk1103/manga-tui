param(
    [string]$InstallerPath = "",
    [string]$ExpectedVersion = "",
    [switch]$TestSystemAppData
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$RepoRoot = Split-Path -Parent $PSScriptRoot

if ([string]::IsNullOrWhiteSpace($ExpectedVersion)) {
    $CargoToml = Get-Content -Path (Join-Path $RepoRoot "Cargo.toml") -Raw
    if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $ExpectedVersion = $Matches[1]
    } else {
        throw "Could not determine version from Cargo.toml"
    }
}

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
    $InstallerPath = Join-Path $RepoRoot "dist\manga-tui-$ExpectedVersion-x86_64-pc-windows-msvc-setup.exe"
}

if (-not (Test-Path -Path $InstallerPath -PathType Leaf)) {
    throw "Installer executable not found at: $InstallerPath"
}

$InstallerFullPath = (Resolve-Path $InstallerPath).Path
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("manga-tui-installer-e2e-" + [Guid]::NewGuid().ToString("N"))
$InstallDir = Join-Path $TempRoot "install"
$UserDataDir = Join-Path $TempRoot "userdata"
$SystemAppDataRoot = Join-Path $env:APPDATA "manga-tui"
$CreatedSystemAppData = $false

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Seed-UserDataDirectory([string]$RootDir, [string]$ConfigDir, [string]$DataDir) {
    New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $DataDir "history") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $DataDir "mangaDownloads\One Piece") -Force | Out-Null

    $configPath = Join-Path $ConfigDir "config.toml"
    $dbPath = Join-Path $DataDir "history\manga-tui-history.db"
    $downloadPath = Join-Path $DataDir "mangaDownloads\One Piece\Chapter 1.cbz"

    Set-Content -Path $configPath -Value "download_type = `"cbz`"`n# installer-preservation-sentinel`n" -Encoding UTF8
    [System.IO.File]::WriteAllBytes($dbPath, [System.Text.Encoding]::UTF8.GetBytes("SQLite format 3`0sentinel-db-payload"))
    [System.IO.File]::WriteAllBytes($downloadPath, [System.Text.Encoding]::UTF8.GetBytes("PK`x03`x04sentinel-cbz-payload"))

    return @{
        ConfigPath   = $configPath
        ConfigHash   = (Get-Sha256 $configPath)
        DbPath       = $dbPath
        DbHash       = (Get-Sha256 $dbPath)
        DownloadPath = $downloadPath
        DownloadHash = (Get-Sha256 $downloadPath)
    }
}

function Assert-UserDataPreserved($Snapshot, [string]$Phase) {
    foreach ($pair in @(
        @("config.toml", $Snapshot.ConfigPath, $Snapshot.ConfigHash),
        @("SQLite DB", $Snapshot.DbPath, $Snapshot.DbHash),
        @("Downloaded CBZ", $Snapshot.DownloadPath, $Snapshot.DownloadHash)
    )) {
        $label = $pair[0]
        $path = $pair[1]
        $expectedHash = $pair[2]
        if (-not (Test-Path -Path $path -PathType Leaf)) {
            throw "[$Phase] User data lost! Missing $label at: $path"
        }
        $actualHash = Get-Sha256 $path
        if ($actualHash -ne $expectedHash) {
            throw "[$Phase] User data modified! $label hash mismatch at $path (expected $expectedHash, got $actualHash)"
        }
    }
}

try {
    New-Item -ItemType Directory -Path $TempRoot -Force | Out-Null

    # 1. Seed isolated user data (and real %APPDATA%\manga-tui when explicitly requested in clean CI)
    $IsolatedSnapshot = Seed-UserDataDirectory -RootDir $UserDataDir -ConfigDir $UserDataDir -DataDir $UserDataDir
    $SystemSnapshot = $null
    if ($TestSystemAppData) {
        if (Test-Path $SystemAppDataRoot) {
            Write-Warning "Skipping -TestSystemAppData check because $SystemAppDataRoot already exists."
        } else {
            $CreatedSystemAppData = $true
            $SystemSnapshot = Seed-UserDataDirectory `
                -RootDir $SystemAppDataRoot `
                -ConfigDir (Join-Path $SystemAppDataRoot "config") `
                -DataDir (Join-Path $SystemAppDataRoot "data")
        }
    }

    # 2. Fresh silent per-user install (no desktop/PATH side-effects during test)
    Write-Host "[1/5] Running fresh silent install to $InstallDir..."
    $installProc = Start-Process -FilePath $InstallerFullPath -ArgumentList @(
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CURRENTUSER",
        "/DIR=`"$InstallDir`"",
        "/TASKS=`"`""
    ) -Wait -PassThru
    if ($installProc.ExitCode -ne 0) {
        throw "Fresh install failed with exit code $($installProc.ExitCode)"
    }

    $InstalledExe = Join-Path $InstallDir "manga-tui.exe"
    $UninstallerExe = Join-Path $InstallDir "unins000.exe"

    if (-not (Test-Path -Path $InstalledExe -PathType Leaf)) {
        throw "Installed executable missing at: $InstalledExe"
    }
    if (-not (Test-Path -Path $UninstallerExe -PathType Leaf)) {
        throw "Uninstaller missing at: $UninstallerExe"
    }
    if (Test-Path -Path (Join-Path $InstallDir "portable.txt")) {
        throw "Installer must NOT create portable.txt inside the installation directory!"
    }
    if (Test-Path -Path (Join-Path $InstallDir "data")) {
        throw "Installer must NOT create a portable data/ directory inside the installation directory!"
    }

    # 3. Verify Win32 metadata, embedded icon, and CLI execution
    Write-Host "[2/5] Verifying Win32 metadata, embedded icon, and CLI execution..."
    $vi = (Get-Item -Path $InstalledExe).VersionInfo
    if ($vi.ProductName -ne "Manga TUI") {
        throw "Expected ProductName 'Manga TUI', got '$($vi.ProductName)'"
    }
    if ($vi.FileVersion -notlike "$ExpectedVersion*") {
        throw "Expected FileVersion starting with '$ExpectedVersion', got '$($vi.FileVersion)'"
    }
    if ([string]::IsNullOrWhiteSpace($vi.FileDescription)) {
        throw "FileDescription is empty on installed executable"
    }

    Add-Type -AssemblyName System.Drawing
    $icon = [System.Drawing.Icon]::ExtractAssociatedIcon($InstalledExe)
    if ($null -eq $icon) {
        throw "Failed to extract embedded icon from installed executable"
    }
    $icon.Dispose()

    $versionOutput = & $InstalledExe --version
    if ($LASTEXITCODE -ne 0) {
        throw "Installed manga-tui.exe --version failed with exit code $LASTEXITCODE"
    }
    Write-Host "  Verified executable output: $versionOutput"

    Assert-UserDataPreserved -Snapshot $IsolatedSnapshot -Phase "AfterFreshInstall"
    if ($SystemSnapshot) {
        Assert-UserDataPreserved -Snapshot $SystemSnapshot -Phase "AfterFreshInstall(AppData)"
    }

    # 4. Upgrade install over existing directory
    Write-Host "[3/5] Running upgrade install over existing installation..."
    $upgradeProc = Start-Process -FilePath $InstallerFullPath -ArgumentList @(
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CURRENTUSER",
        "/DIR=`"$InstallDir`"",
        "/TASKS=`"`""
    ) -Wait -PassThru
    if ($upgradeProc.ExitCode -ne 0) {
        throw "Upgrade install failed with exit code $($upgradeProc.ExitCode)"
    }

    if (-not (Test-Path -Path $InstalledExe -PathType Leaf)) {
        throw "Installed executable missing after upgrade at: $InstalledExe"
    }
    $null = & $InstalledExe --version
    if ($LASTEXITCODE -ne 0) {
        throw "Upgraded manga-tui.exe --version failed with exit code $LASTEXITCODE"
    }

    Write-Host "[4/5] Verifying user data survived upgrade..."
    Assert-UserDataPreserved -Snapshot $IsolatedSnapshot -Phase "AfterUpgrade"
    if ($SystemSnapshot) {
        Assert-UserDataPreserved -Snapshot $SystemSnapshot -Phase "AfterUpgrade(AppData)"
    }

    # 5. Uninstall and verify user data preservation
    Write-Host "[5/5] Running silent uninstall and verifying user data preservation..."
    $uninstallProc = Start-Process -FilePath $UninstallerExe -ArgumentList @(
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART"
    ) -Wait -PassThru
    if ($uninstallProc.ExitCode -ne 0) {
        throw "Uninstall failed with exit code $($uninstallProc.ExitCode)"
    }

    # Inno Setup uninstaller spawns a temporary helper copy to delete itself; wait briefly for cleanup
    $deadline = (Get-Date).AddSeconds(30)
    while ((Test-Path -Path $InstalledExe) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }

    if (Test-Path -Path $InstalledExe) {
        throw "Uninstall did not remove $InstalledExe within timeout"
    }

    Assert-UserDataPreserved -Snapshot $IsolatedSnapshot -Phase "AfterUninstall"
    if ($SystemSnapshot) {
        Assert-UserDataPreserved -Snapshot $SystemSnapshot -Phase "AfterUninstall(AppData)"
    }

    Write-Host "PASS: Fresh install, Win32 icon/metadata, upgrade, and uninstall all succeeded while preserving config.toml, SQLite DB, and downloads."
} finally {
    if ($CreatedSystemAppData -and (Test-Path $SystemAppDataRoot)) {
        Remove-Item -Path $SystemAppDataRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $TempRoot) {
        Remove-Item -Path $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
