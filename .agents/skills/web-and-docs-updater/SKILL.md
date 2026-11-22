---
name: web-and-docs-updater
description: >-
  Use this skill when updating the GitHub Pages website (docs/index.html), the
  interactive browser sample viewer, the Google Drive Privacy Policy (docs/privacy.html),
  the PowerShell SHA-256 web installer (docs/install.ps1), or user documentation
  (README.md, docs/LEEME_PORTABLE.txt, docs/GOOGLE_DRIVE_SETUP.md) in manga-tui.
---

# Website, Web Installer & Documentation Skill

This skill guides updates to `manga-tui`'s public GitHub Pages site (`docs/`), one-liner PowerShell installer (`docs/install.ps1`), and user documentation.

---

## 1. Structure of `docs/` & Public Assets

GitHub Pages serves directly from the `docs/` folder on `main` (with `docs/.nojekyll` enabled):

- **`docs/index.html`**: Main landing page for Manga TUI.
  - Features direct download buttons for the Windows Installer (`-setup.exe`) and Portable ZIP (`-portable.zip`) dynamically linked to GitHub Releases.
  - Includes the **1-command PowerShell installer snippet** (`irm https://tomahawk1103.github.io/manga-tui/install.ps1 | iex`) and **Microsoft Edge / Windows SmartScreen guidance** so users know how to run unsigned open-source binaries (`Más información` -> `Ejecutar de todas formas`).
  - Contains the **Interactive Sample Viewer** allowing visitors to preview manga pages (`docs/images/pagina-1.jpg`, `pagina-2.jpg`, `pagina-3.jpg`) and TUI aesthetics right in the browser.
- **`docs/install.ps1`**: Official PowerShell web installer script.
  - Queries GitHub Releases API (`https://api.github.com/repos/tomahawk1103/manga-tui/releases`) supporting `-Version`, `-Channel` (`stable` or `nightly`), `-Silent`, and `-SkipChecksum`.
  - Downloads the Windows Inno Setup `.exe` installer and its `.sha256` (or `checksum.txt`), verifies the **SHA-256 cryptographic hash** via `Assert-FileSha256`, unblocks the file (`Unblock-File`), and runs the installer.
- **`docs/privacy.html`**: Public Privacy Policy & Terms page required for Google OAuth verification (explains that `drive.file` scope only accesses the user's own `MangaTUI/` folder in Google Drive and never collects telemetry).
- **`docs/GOOGLE_DRIVE_SETUP.md`**: Step-by-step guide for Google Drive synchronization.
- **`docs/LEEME_PORTABLE.txt`**: Spanish user guide bundled inside every Windows Portable ZIP by `scripts/package-windows.ps1`.

---

## 2. Guidelines When Updating Website or Installer

### A. Updating `docs/install.ps1`
- Keep helper functions (`Select-MangaTuiRelease`, `Get-ExpectedSha256FromChecksumText`, `Assert-FileSha256`, `Install-MangaTui`) dot-sourceable (`if ($MyInvocation.InvocationName -ne '.')`) so unit/integration tests can import and test them without triggering a live installation.
- Always preserve TLS 1.2 compatibility and `-UseBasicParsing` so the script works on stock Windows PowerShell 5.1 as well as PowerShell 7 (`pwsh`).
- Ensure asset suffix matching (`-x86_64-pc-windows-msvc-setup.exe`) stays synchronized with `scripts/package-windows-installer.ps1` and `.github/workflows/release.yml`.

### B. Updating `docs/index.html` & `docs/privacy.html`
- Maintain responsive layout, dark Tokyo Night / Catppuccin aesthetics, and accessible contrast.
- When adding new features to `manga-tui` (e.g. new providers, reader shortcuts, cloud sync capabilities), highlight them in the feature grid on `docs/index.html` and in `README.md`.
- Never break the `docs/images/pagina-*.jpg` relative paths used by the interactive sample viewer.

### C. Updating `docs/LEEME_PORTABLE.txt`
- Remember that `scripts/package-windows.ps1` verifies the presence of `LEEME_PORTABLE.txt` inside the portable ZIP. Keep keybindings (`z` for Zen mode, provider selector, offline tab, cloud sync) up to date in plain, friendly Spanish.

---

## 3. Verification Checklist

1. If `docs/install.ps1` was modified, test syntax and function parsing in PowerShell:
   ```powershell
   powershell -NoProfile -Command ". .\docs\install.ps1; Write-Host 'install.ps1 parsed OK'"
   ```
2. If `docs/LEEME_PORTABLE.txt` was modified, verify portable packaging still passes:
   ```powershell
   .\scripts\package-windows.ps1 -SkipBuild
   ```
3. Commit and push changes to `main` so GitHub Pages deploys the updated site.
