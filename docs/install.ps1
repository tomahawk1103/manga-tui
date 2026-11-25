param(
    [string]$Version = "",
    [string]$Channel = "",
    [switch]$Silent,
    [switch]$SkipChecksum
)

function Get-RemoteText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Uri,
        [hashtable]$Headers = @{}
    )

    $resp = Invoke-WebRequest -Uri $Uri -Headers $Headers -UseBasicParsing -ErrorAction Stop
    if ($resp.Content -is [byte[]]) {
        return [System.Text.Encoding]::UTF8.GetString($resp.Content)
    }
    return [string]$resp.Content
}

function Find-MangoraWindowsAsset {
    param([object]$Release)

    if ($null -eq $Release -or $Release.draft -or $null -eq $Release.assets) {
        return $null
    }

    foreach ($asset in $Release.assets) {
        if ($asset.name -and $asset.name -match '^Mangora.*-setup\.exe$') {
            return $asset
        }
    }

    foreach ($asset in $Release.assets) {
        if ($asset.name -and $asset.name -match '^Mangora.*\.msi$') {
            return $asset
        }
    }

    return $null
}

function Test-ReleaseHasMangoraInstallerAsset {
    param([object]$Release)

    return $null -ne (Find-MangoraWindowsAsset -Release $Release)
}

function Select-MangoraRelease {
    param(
        [object[]]$Releases,
        [string]$Version = "",
        [string]$Channel = ""
    )

    $eligible = @($Releases | Where-Object { Test-ReleaseHasMangoraInstallerAsset -Release $_ })
    if ($eligible.Count -eq 0) {
        return $null
    }

    if (-not [string]::IsNullOrWhiteSpace($Version)) {
        $wantedRaw = $Version.Trim()
        $wantedNoPrefix = $wantedRaw -replace '^(?i)mangora-', ''
        $wantedNoV = $wantedNoPrefix.TrimStart('v', 'V')
        foreach ($rel in $eligible) {
            $tagRaw = ([string]$rel.tag_name).Trim()
            $tagNoPrefix = $tagRaw -replace '^(?i)mangora-', ''
            $tagNoV = $tagNoPrefix.TrimStart('v', 'V')
            if ($tagRaw -ieq $wantedRaw -or $tagNoPrefix -ieq $wantedNoPrefix -or $tagNoV -ieq $wantedNoV) {
                return $rel
            }
        }
        return $null
    }

    if ($Channel -ieq "prerelease") {
        return $eligible[0]
    }

    foreach ($rel in $eligible) {
        if (-not $rel.prerelease -and ([string]$rel.tag_name).Trim() -match '^(?i)mangora-v') {
            return $rel
        }
    }

    foreach ($rel in $eligible) {
        if (-not $rel.prerelease) {
            return $rel
        }
    }

    return $eligible[0]
}

function Select-MangaTuiRelease {
    param(
        [object[]]$Releases,
        [string]$Version = "",
        [string]$Channel = ""
    )

    return Select-MangoraRelease -Releases $Releases -Version $Version -Channel $Channel
}

function Get-ExpectedSha256FromChecksumText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ChecksumText,
        [Parameter(Mandatory = $true)]
        [string]$FileName
    )

    $lines = @($ChecksumText -split "\r?\n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne "" })
    if ($lines.Count -eq 0) {
        return $null
    }

    foreach ($line in $lines) {
        if ($line -match '^([0-9a-fA-F]{64})\s+\*?(.+)$') {
            $hash = $Matches[1].ToLowerInvariant()
            $rawName = $Matches[2].Trim()
            $leaf = Split-Path -Path $rawName -Leaf
            if ($leaf -ieq $FileName) {
                return $hash
            }
        }
    }

    if ($lines.Count -eq 1 -and $lines[0] -match '^([0-9a-fA-F]{64})$') {
        return $Matches[1].ToLowerInvariant()
    }

    return $null
}

function Assert-FileSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string]$ExpectedHash
    )

    $expected = $ExpectedHash.Trim().ToLowerInvariant()
    $actual = (Get-FileHash -Path $FilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "Verificacion SHA-256 fallida para '$FilePath'. Esperado: $expected | Obtenido: $actual"
    }
    return $actual
}

function Install-Mangora {
    param(
        [string]$Version = "",
        [string]$Channel = "",
        [switch]$Silent,
        [switch]$SkipChecksum
    )

    $prevErrorAction = $ErrorActionPreference
    $prevProgress = $ProgressPreference
    $tempDir = $null

    try {
        $ErrorActionPreference = "Stop"
        $ProgressPreference = "SilentlyContinue"

        $targetVersion = if (-not [string]::IsNullOrWhiteSpace($Version)) {
            $Version.Trim()
        } elseif (-not [string]::IsNullOrWhiteSpace($env:MANGORA_VERSION)) {
            $env:MANGORA_VERSION.Trim()
        } else {
            [string]$env:MANGA_TUI_VERSION
        }
        $targetChannel = if (-not [string]::IsNullOrWhiteSpace($Channel)) {
            $Channel.Trim()
        } elseif (-not [string]::IsNullOrWhiteSpace($env:MANGORA_CHANNEL)) {
            $env:MANGORA_CHANNEL.Trim()
        } elseif (-not [string]::IsNullOrWhiteSpace($env:MANGA_TUI_CHANNEL)) {
            $env:MANGA_TUI_CHANNEL.Trim()
        } else {
            "stable"
        }
        $useSilent = $Silent.IsPresent -or ($env:MANGORA_SILENT -eq "1") -or ($env:MANGA_TUI_SILENT -eq "1")
        $useSkipChecksum = $SkipChecksum.IsPresent -or ($env:MANGORA_SKIP_CHECKSUM -eq "1") -or ($env:MANGA_TUI_SKIP_CHECKSUM -eq "1")

        if (-not [Environment]::Is64BitOperatingSystem) {
            throw "Mangora para Windows requiere un sistema operativo de 64 bits (x64)."
        }

        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        } catch {
        }

        $apiHeaders = @{
            "Accept"     = "application/vnd.github+json"
            "User-Agent" = "mangora-installer"
        }
        $dlHeaders = @{
            "User-Agent" = "mangora-installer"
        }

        Write-Host "[INFO] Consultando ultima version oficial de Mangora en GitHub..."
        $releasesUri = "https://api.github.com/repos/tomahawk1103/manga-tui/releases?per_page=20"
        $releases = Invoke-RestMethod -Uri $releasesUri -Headers $apiHeaders -UseBasicParsing -ErrorAction Stop
        $release = Select-MangoraRelease -Releases @($releases) -Version $targetVersion -Channel $targetChannel

        if ($null -eq $release -and -not [string]::IsNullOrWhiteSpace($targetVersion)) {
            $trimmed = $targetVersion.Trim()
            $trimmedNoPrefix = ($trimmed -replace '^(?i)mangora-', '').TrimStart('v', 'V')
            $tagCandidates = @($trimmed, "mangora-v$trimmedNoPrefix", "v$trimmedNoPrefix", $trimmedNoPrefix) | Select-Object -Unique

            foreach ($tag in $tagCandidates) {
                try {
                    $single = Invoke-RestMethod -Uri "https://api.github.com/repos/tomahawk1103/manga-tui/releases/tags/$tag" -Headers $apiHeaders -UseBasicParsing -ErrorAction Stop
                    $release = Select-MangoraRelease -Releases @($single) -Version $targetVersion -Channel $targetChannel
                    if ($null -ne $release) {
                        break
                    }
                } catch {
                }
            }
        }

        if ($null -eq $release) {
            throw "No se encontro una release valida de Mangora con instalador de Windows (.exe o .msi)."
        }

        $setupAsset = Find-MangoraWindowsAsset -Release $release
        if ($null -eq $setupAsset) {
            throw "No se encontro el archivo instalador de Mangora dentro de la release $($release.tag_name)."
        }

        $shaAsset = $null
        $expectedShaFileName = "$($setupAsset.name).sha256"
        foreach ($asset in $release.assets) {
            if ($asset.name -ieq $expectedShaFileName) {
                $shaAsset = $asset
                break
            }
        }
        if ($null -eq $shaAsset) {
            foreach ($candidateName in @("SHA256SUMS.txt", "checksum.txt")) {
                foreach ($asset in $release.assets) {
                    if ($asset.name -ieq $candidateName) {
                        $shaAsset = $asset
                        break
                    }
                }
                if ($null -ne $shaAsset) {
                    break
                }
            }
        }

        $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("mangora-install-" + [Guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
        $setupPath = Join-Path $tempDir $setupAsset.name

        Write-Host "[INFO] Descargando $($setupAsset.name) ($($release.tag_name))..."
        Invoke-WebRequest -Uri $setupAsset.browser_download_url -Headers $dlHeaders -OutFile $setupPath -UseBasicParsing -ErrorAction Stop

        if (-not $useSkipChecksum) {
            if ($null -eq $shaAsset) {
                throw "No se encontro archivo de checksum (.sha256, SHA256SUMS.txt o checksum.txt) en la release $($release.tag_name) para verificar el instalador."
            }
            $checksumText = Get-RemoteText -Uri $shaAsset.browser_download_url -Headers $dlHeaders
            $expectedHash = Get-ExpectedSha256FromChecksumText -ChecksumText $checksumText -FileName $setupAsset.name
            if ([string]::IsNullOrWhiteSpace($expectedHash)) {
                throw "No se encontro el hash SHA-256 de '$($setupAsset.name)' dentro de '$($shaAsset.name)'."
            }
            $verifiedHash = Assert-FileSha256 -FilePath $setupPath -ExpectedHash $expectedHash
            Write-Host "[OK] SHA-256 verificado: $verifiedHash"
        } else {
            Write-Warning "Verificacion SHA-256 omitida por solicitud del usuario (-SkipChecksum)."
        }

        Unblock-File -Path $setupPath -ErrorAction SilentlyContinue

        Write-Host "[INFO] Iniciando instalador de Mangora..."
        $isMsi = $setupAsset.name -match '(?i)\.msi$'
        if ($isMsi) {
            $msiArgs = if ($useSilent) { @("/i", "`"$setupPath`"", "/qn", "/norestart") } else { @("/i", "`"$setupPath`"") }
            $proc = Start-Process -FilePath "msiexec.exe" -ArgumentList $msiArgs -Wait -PassThru
        } else {
            if ($useSilent) {
                $proc = Start-Process -FilePath $setupPath -ArgumentList @("/S") -Wait -PassThru
            } else {
                $proc = Start-Process -FilePath $setupPath -Wait -PassThru
            }
        }

        if ($proc.ExitCode -eq 0) {
            Write-Host "[OK] Mangora instalado correctamente."
            Write-Host "[INFO] Puedes abrir Mangora desde el Menu Inicio o el acceso directo del Escritorio."
            return
        }

        if ($proc.ExitCode -eq 2 -or $proc.ExitCode -eq 5 -or $proc.ExitCode -eq 1602) {
            Write-Host "[INFO] Instalacion cancelada por el usuario."
            return
        }

        throw "El instalador termino con codigo de salida $($proc.ExitCode)."
    } finally {
        if ($tempDir -and (Test-Path -Path $tempDir)) {
            Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        $ErrorActionPreference = $prevErrorAction
        $ProgressPreference = $prevProgress
    }
}

function Install-MangaTui {
    param(
        [string]$Version = "",
        [string]$Channel = "",
        [switch]$Silent,
        [switch]$SkipChecksum
    )

    Install-Mangora -Version $Version -Channel $Channel -Silent:$Silent -SkipChecksum:$SkipChecksum
}

if ($MyInvocation.InvocationName -ne '.') {
    Install-Mangora -Version $Version -Channel $Channel -Silent:$Silent -SkipChecksum:$SkipChecksum
}
