---
trigger: always_on
description: Mandatory binary reinstallation rule after code modifications in manga-tui.
---

# Always Install Updated Binary

Whenever you make changes, bug fixes, or enhancements to the `manga-tui` codebase:

1. Always verify the code compiles (`cargo check` or `cargo test`).
2. Run `cargo fmt --all`.
3. **MANDATORY**: Re-install the new binary globally into the user's environment and sync any local Windows installation by running:
   ```powershell
   cargo install --path .
   .\scripts\sync-local-install.ps1
   ```
   This ensures that whenever the user runs `manga-tui` from their terminal or from their installed shortcuts (e.g. `D:\Manga TUI`), they are immediately running the newly modified version.
4. **MANDATORY**: Once verified and installed, commit and push changes to the active branch (`git push`).

## Always Rebuild and Update Local Mangora (`apps/mangora` or shared `crates/manga-core`)

Whenever you make changes, bug fixes, or enhancements affecting **Mangora** (`apps/mangora/` or shared `crates/manga-core/`):

1. **MANDATORY**: Build the Mangora frontend (`apps/mangora/dist`):
   ```powershell
   $env:PATH = "C:\Program Files\nodejs;$env:PATH"
   # Run `npm.cmd --prefix apps/mangora ci` first if `apps/mangora/node_modules` is missing
   npm.cmd --prefix apps/mangora run build
   ```
2. **MANDATORY**: Build the standalone Mangora release binary:
   ```powershell
   cargo build --release -p mangora --features custom-protocol
   ```
3. **MANDATORY**: Update the local Windows installation (`%LOCALAPPDATA%\Mangora\mangora.exe`, `%LOCALAPPDATA%\Mangora\mangora.ico`, and `~\.cargo\bin\mangora.exe` when `~\.cargo\bin` exists), stopping any running `mangora` process first so the binary can be replaced (equivalent to `.\scripts\install-mangora-local.ps1`, executed inline if PowerShell script execution policy blocks `.ps1` files):
   ```powershell
   Get-Process mangora -ErrorAction SilentlyContinue | Stop-Process -Force
   $installDir = Join-Path $env:LOCALAPPDATA "Mangora"
   New-Item -ItemType Directory -Force -Path $installDir | Out-Null
   Copy-Item -Path "target\release\mangora.exe" -Destination (Join-Path $installDir "mangora.exe") -Force
   Copy-Item -Path "apps\mangora\src-tauri\icons\icon.ico" -Destination (Join-Path $installDir "mangora.ico") -Force
   $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
   if (Test-Path $cargoBin) {
       Copy-Item -Path "target\release\mangora.exe" -Destination (Join-Path $cargoBin "mangora.exe") -Force
   }
   ```
4. **MANDATORY**: Verify `mangora` tests (`cargo test --manifest-path apps/mangora/src-tauri/Cargo.toml`), commit, and push changes to the active branch (`git push`).

