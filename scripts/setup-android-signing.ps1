param(
    [string]$KeystoreDir = ".keystore",
    [string]$Alias = "mangora"
)

$ErrorActionPreference = "Stop"

function Find-Keytool {
    $cmd = Get-Command keytool -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    if ($env:JAVA_HOME) {
        $candidate = Join-Path $env:JAVA_HOME "bin\keytool.exe"
        if (Test-Path $candidate) {
            return $candidate
        }
    }
    $javaMatches = Get-ChildItem "C:\Program Files\Java\*\bin\keytool.exe" -ErrorAction SilentlyContinue | Sort-Object FullName -Descending
    if ($javaMatches.Count -gt 0) {
        return $javaMatches[0].FullName
    }
    throw "No se encontro keytool.exe. Instala JDK 17+ o asegurate de que JAVA_HOME este configurado."
}

function New-RandomPassword([int]$Length = 24) {
    $chars = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789".ToCharArray()
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $bytes = New-Object byte[] $Length
    $rng.GetBytes($bytes)
    $result = New-Object System.Text.StringBuilder
    foreach ($b in $bytes) {
        [void]$result.Append($chars[$b % $chars.Length])
    }
    return $result.ToString()
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ResolvedKeystoreDir = if ([System.IO.Path]::IsPathRooted($KeystoreDir)) {
    $KeystoreDir
} else {
    Join-Path $RepoRoot $KeystoreDir
}
New-Item -ItemType Directory -Force -Path $ResolvedKeystoreDir | Out-Null

$KeystorePath = Join-Path $ResolvedKeystoreDir "mangora-release.jks"
$InstructionsPath = Join-Path $ResolvedKeystoreDir "GITHUB_SECRETS_INSTRUCTIONS.txt"
$Keytool = Find-Keytool

$CreatedNew = $false
$Password = $null

if (-not (Test-Path $KeystorePath)) {
    $Password = New-RandomPassword 24
    & $Keytool -genkeypair -v `
        -keystore $KeystorePath `
        -storetype PKCS12 `
        -alias $Alias `
        -keyalg RSA `
        -keysize 2048 `
        -validity 10000 `
        -storepass $Password `
        -keypass $Password `
        -dname "CN=Mangora, OU=MangaTUI, O=tomahawk1103, C=ES" | Out-Null

    if ($LASTEXITCODE -ne 0) {
        throw "Error generando el keystore Android con keytool (codigo $LASTEXITCODE)."
    }
    $CreatedNew = $true
}

$Base64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($KeystorePath))

$Sha1 = ""
$Sha256 = ""
if ($Password) {
    $CertInfo = & $Keytool -list -v -keystore $KeystorePath -alias $Alias -storepass $Password 2>&1
    foreach ($line in $CertInfo) {
        if ($line -match "SHA1:\s*(.+)") { $Sha1 = $Matches[1].Trim() }
        if ($line -match "SHA256:\s*(.+)") { $Sha256 = $Matches[1].Trim() }
    }
}

$PasswordLine = if ($CreatedNew) { $Password } else { "<tu-password-existente>" }

$Instructions = @"
========================================================================
MANGORA ANDROID RELEASE SIGNING — GITHUB ACTIONS SECRETS
========================================================================
Archivo keystore local (NO subir a Git): $KeystorePath
Alias: $Alias

Configura estos 4 Secrets en GitHub:
Repository -> Settings -> Secrets and variables -> Actions -> New repository secret

1) ANDROID_KEYSTORE_BASE64
$Base64

2) ANDROID_KEYSTORE_PASSWORD
$PasswordLine

3) ANDROID_KEY_ALIAS
$Alias

4) ANDROID_KEY_PASSWORD
$PasswordLine

------------------------------------------------------------------------
Huellas del certificado (utiles para Google Cloud OAuth Android):
SHA-1:   $Sha1
SHA-256: $Sha256
========================================================================
"@

Set-Content -Path $InstructionsPath -Value $Instructions -Encoding UTF8

Write-Host "==> Keystore listo en: $KeystorePath" -ForegroundColor Green
Write-Host "==> Instrucciones y secretos guardados en: $InstructionsPath" -ForegroundColor Cyan
if ($Sha256) {
    Write-Host "    SHA-256: $Sha256" -ForegroundColor Gray
}
Write-Host "    IMPORTANTE: La carpeta .keystore/ esta ignorada en .gitignore; haz copia de seguridad personal." -ForegroundColor Yellow
