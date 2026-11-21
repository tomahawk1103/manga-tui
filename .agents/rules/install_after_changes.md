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

