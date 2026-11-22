---
name: ci-preflight
description: >-
  Use this skill before committing code or opening a pull request in manga-tui.
  Executes the full CI verification pipeline (cargo fmt, clippy, check, tests),
  diagnoses compiler and lint warnings (including nightly/Linux CI rules), fixes
  formatting issues automatically, and syncs the local installation.
---

# CI Preflight & Code Quality Skill

This skill enforces and automates the verification pipeline required by `manga-tui`'s CI configuration before pushing code, and ensures the local environment (`cargo install` + `sync-local-install.ps1`) is updated.

---

## 1. Quick Full CI & Install Pipeline

Run the full verification and installation sequence:

```powershell
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo check
cargo test
cargo install --path .
.\scripts\sync-local-install.ps1
```

If `cargo-make` is installed, you can also run:
```powershell
cargo make full-ci
```

---

## 2. Step-by-Step Triage & Automatic Fixes

### Phase 1: Formatting (`cargo fmt`)
Always format before checking lints or committing:
```powershell
cargo fmt --all
cargo fmt --all --check
```

### Phase 2: Clippy Linter (`cargo clippy --all-targets -- -D warnings`)
GitHub Actions CI (Linux / Windows / macOS) treats **all warnings as fatal errors** (`-D warnings`), including inside `#[cfg(test)]` modules (`--all-targets`).

```powershell
cargo clippy --all-targets -- -D warnings
```

Common Clippy warnings and fixes in `manga-tui`:
- `clippy::needless_borrow`: Remove unnecessary `&` references.
- `clippy::redundant_closure`: Replace `|x| foo(x)` with `foo`.
- `clippy::clone_on_copy`: Remove `.clone()` on `Copy` types (`ThemeVariant`, `MangaProviders`, `Languages`, primitives).
- `clippy::collapsible_if` / `clippy::collapsible_else_if`: Merge nested `if` statements into `if cond1 && cond2`.
- `clippy::manual_unwrap_or_default`: Use `.unwrap_or_default()` instead of matching `None => Default::default()`.
- `clippy::ptr_arg` / `clippy::useless_vec`: Prefer slices `&[T]` over `&Vec<T>` and array literals `[...]` over `vec![...]` when fixed-size.
- **Cross-Platform Dead Code (`#[cfg(unix)]` vs `#[cfg(windows)]`)**: In modules like `src/backend/updater.rs` or `src/backend/secrets.rs`, helper functions or imports used only on one OS must be gated with `#[cfg(target_os = "windows")]` or `#[cfg(not(target_os = "windows"))]` so Linux/macOS CI doesn't fail with `unused_imports` or `dead_code`.

If there are mechanical lints, attempt automatic fixes:
```powershell
cargo clippy --fix --allow-dirty --allow-staged
```

### Phase 3: Unit & Integration Tests
Run all non-ignored unit tests:
```powershell
cargo test
```

> **Note on Environment Variables in Tests**: When testing paths or data directories (e.g. `MANGA_TUI_DATA_DIR`), always isolate temporary directories per test so parallel test threads or existing host installations don't collide.

Run ignored tests (live providers or disk downloaders) selectively when touching those modules:
```powershell
cargo test backend::manga_downloader -- --ignored
cargo test backend::manga_provider::<provider> -- --ignored
```

---

## 3. Mandatory Binary Installation & Git Push Checklist

Per repository rules (`install_after_changes.md` and `push_after_tests.md`), after any code change passes verification:

- [ ] `cargo fmt --all --check` passes with no output.
- [ ] `cargo clippy --all-targets -- -D warnings` finishes with 0 warnings.
- [ ] `cargo test` passes 100% of unit tests.
- [ ] **Install updated binary globally**:
  ```powershell
  cargo install --path .
  ```
- [ ] **Sync local Windows Inno Setup installation** (updates `D:\Manga TUI` or registry `InstallLocation` automatically):
  ```powershell
  .\scripts\sync-local-install.ps1
  ```
- [ ] **Commit and push** following conventional commits:
  ```powershell
  git add .
  git commit -m "<type>(<scope>): <clear description>"
  git push
  ```
