---
name: release-and-installer
description: >-
  Use this skill when bumping versions, packaging the Windows Inno Setup installer
  or portable ZIP, running installer E2E data-preservation tests, debugging the
  in-app auto-updater (Stable & Nightly channels), or updating GitHub Actions release
  workflows in manga-tui.
---

# Release, Windows Installer & Auto-Updater Skill

This skill guides you through packaging, testing, releasing, and maintaining the Windows installer, portable builds, and the in-app auto-updater in `manga-tui`.

---

## 1. Architecture Overview

- **Build Metadata & Version**:
  - `Cargo.toml`: Canonical SemVer version (`version = "X.Y.Z"`).
  - `build.rs`: Embeds Win32 icon (`assets/icon.ico`), `ProductName` (`"Manga TUI"`), `FileDescription`, `FileVersion`, and build identity env vars (`MANGA_TUI_BUILD_COMMIT`, `MANGA_TUI_BUILD_COMMIT_SHORT`, `MANGA_TUI_BUILD_TIMESTAMP`, `MANGA_TUI_BUILD_CHANNEL`).
- **Windows Packaging Scripts (`scripts/`)**:
  - `scripts/windows-installer.iss`: Inno Setup 6 definition (AppId `{B7F4E2A9-8C31-4D62-9A5E-1D8F6B3C2E40}`, UTF-8 with BOM required for Spanish messages).
  - `scripts/package-windows-installer.ps1`: Builds release binary, verifies Win32 metadata & embedded icon, compiles `dist/manga-tui-<version>-x86_64-pc-windows-msvc-setup.exe` + `.sha256`.
  - `scripts/package-windows.ps1`: Builds portable archive `dist/manga-tui-<version>-x86_64-pc-windows-msvc-portable.zip` (includes `portable.txt` and `LEEME_PORTABLE.txt`) + `.sha256`.
  - `scripts/test-windows-installer.ps1`: Automated E2E test verifying silent fresh install, Win32 icon/metadata, upgrade install, silent uninstall, and 100% preservation of user data (`config.toml`, `manga-tui-history.db`, and `mangaDownloads/`).
  - `scripts/sync-local-install.ps1`: Detects any local Inno Setup installation via the Windows Registry (`InstallLocation`, e.g. `D:\Manga TUI`) and syncs the freshly compiled `manga-tui.exe` into it.
- **In-App Auto-Updater (`src/backend/updater.rs` & `src/view/widgets/updater.rs`)**:
  - Detects installation mode (`InstallKind::WindowsInstaller`, `WindowsPortable`, `WindowsStandalone`, `UnixBinary`).
  - Supports **Stable** (`vX.Y.Z` tags) and **Nightly** (`nightly` rolling tag with `<!-- manga-tui-meta: commit=... version=... timestamp=... -->`).
  - Verifies SHA-256 checksums before applying updates.
- **CI/CD Workflows (`.github/workflows/`)**:
  - `release.yml`: Triggered on `v*` tags; builds Linux, macOS (Intel & ARM), Windows Portable ZIP, and Windows Inno Setup Installer.
  - `nightly.yml`: Triggered on pushes to `main`; publishes rolling `nightly` pre-release assets.

---

## 2. Standard Release & Version Bump Procedure

When preparing a new version `X.Y.Z`:

1. **Update Version**:
   - Bump `version = "X.Y.Z"` in `Cargo.toml`.
   - Update version references in `docs/index.html` if hardcoded badges exist.
   - Run `cargo check` so `Cargo.lock` updates cleanly.

2. **Build & Verify Portable ZIP**:
   ```powershell
   .\scripts\package-windows.ps1
   ```
   Verifies `dist/manga-tui-X.Y.Z-x86_64-pc-windows-msvc-portable.zip` contains `manga-tui.exe`, `portable.txt`, and `LEEME_PORTABLE.txt`.

3. **Build & Verify Windows Inno Setup Installer**:
   ```powershell
   .\scripts\package-windows-installer.ps1 -SkipBuild
   ```
   Verifies Win32 `ProductName` (`Manga TUI`), embedded icon resources via `Shell32::ExtractIconEx`, compiles the setup `.exe` with `ISCC.exe`, and generates the `.sha256` file.

4. **Run End-to-End Installer Test Suite**:
   ```powershell
   .\scripts\test-windows-installer.ps1
   ```
   Confirms fresh install, upgrade, and uninstall preserve user data and do not leak `portable.txt` into standard installations.

5. **Run Updater Unit Tests**:
   ```powershell
   cargo test backend::updater -- --nocapture
   ```

6. **Sync Local Environment & Tag (when requested by user)**:
   ```powershell
   cargo install --path .
   .\scripts\sync-local-install.ps1
   ```

---

## 3. Troubleshooting Common Packaging & Updater Pitfalls

- **Inno Setup `.iss` Encoding**: `scripts/windows-installer.iss` **must** remain saved as **UTF-8 with BOM** so Inno Setup renders Spanish accents (`á, é, í, ó, ú, ñ`) properly in wizard dialogs.
- **Cross-Platform Updater Tests**: In `src/backend/updater.rs`, Windows-specific path/registry helpers or Unix-specific permission (`chmod +x` / `std::os::unix::fs::PermissionsExt`) tests must use `#[cfg(target_os = "windows")]` or `#[cfg(unix)]` so Linux and macOS CI runners pass `cargo clippy --all-targets -- -D warnings`.
- **Running Exe Lock on Windows**: Both `updater.rs` and `sync-local-install.ps1` handle locked running executables by rotating `manga-tui.exe` to `manga-tui.exe.old` before writing the new binary.
