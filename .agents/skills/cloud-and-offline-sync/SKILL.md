---
name: cloud-and-offline-sync
description: >-
  Use this skill when working on Google Drive Cloud Sync (OAuth 2.0 PKCE, keyring
  secrets, library_sync.json bidirectional merge, chapter download cloud backup),
  Local Cloud Drive folders, or the 100% Offline Reader & Catalog in manga-tui.
---

# Cloud Sync (Google Drive) & Offline Library Skill

This skill guides development, debugging, and testing of `manga-tui`'s cloud synchronization engine (`src/backend/cloud.rs`, `src/view/widgets/cloud.rs`) and 100% offline reading subsystem (`src/backend/offline.rs`).

---

## 1. Cloud Synchronization Architecture (`src/backend/cloud.rs`)

`manga-tui` supports two cloud backends sharing the exact same `MangaTUI/` remote structure:

1. **Google Drive REST API v3 (OAuth 2.0 + PKCE)**:
   - Scope: `https://www.googleapis.com/auth/drive.file` (only accesses files created by `manga-tui`).
   - **1-Click Sign-In & Custom Credentials**: Uses bundled OAuth client config (injected at build time via `MANGA_TUI_GDRIVE_CLIENT_ID` / `MANGA_TUI_GDRIVE_CLIENT_SECRET` or runtime configuration) and launches a local loopback HTTP listener (`127.0.0.1:<port>`) to capture the OAuth2 PKCE authorization code automatically.
   - **Credential Storage (`src/backend/secrets.rs`)**: Tokens (`gdrive_access_token`, `gdrive_refresh_token`, client ID/secret) are stored in the OS keyring (`KeyringStorage`) with fallback support.
2. **Local Cloud Drive Bridge**:
   - Auto-detects or connects to a local synced folder (Google Drive Desktop, OneDrive, iCloud Drive, or custom path).

### Cloud Folder Layout (`MangaTUI/`)
- `MangaTUI/library_sync.json`: Cross-platform JSON snapshot (`LibrarySyncSnapshot`, schema v1) containing:
  - `mangas` (`SyncMangaRecord`: reading history, plan-to-read, last_read timestamps, provider)
  - `chapters` (`SyncChapterRecord`: `is_read`, `is_downloaded`, `is_bookmarked`, `number_page_bookmarked`, language)
  - `favorites` (`SyncFavoriteRecord`)
  - `dismissed_notifications` (`SyncDismissedRecord`)
  - `search_history` (`SyncSearchRecord`)
- `MangaTUI/manga-tui-history.db`: Full SQLite database backup copy.
- `MangaTUI/downloads/<manga_folder>/<chapter_file>`: Synced `.cbz`, `.epub`, and `.pdf` downloaded chapters.

### Smart Bidirectional Merge Rules
- Library merges are **lossless and additive**:
  - `is_read` and `is_downloaded` combine via logical OR (`local || remote`).
  - `last_read` and `updated_at` keep the most recent timestamp.
  - Bookmarks (`number_page_bookmarked`) preserve the latest active bookmark.
  - Downloaded chapter archives present in the cloud but missing locally are downloaded into `<data_dir>/mangaDownloads/`, and local downloads missing in the cloud are uploaded.

---

## 2. 100% Offline Reader Architecture (`src/backend/offline.rs`)

- **Catalog Discovery**: Scans `<data_dir>/mangaDownloads/` and enriches folder/file entries with metadata stored in SQLite (`OfflineCatalogMetadata` from `src/backend/database.rs`).
- **4-Format Direct Extraction**:
  1. **CBZ (`.cbz`)**: Reads ZIP entries matching `is_supported_image_name`, sorted via `natural_cmp`.
  2. **EPUB (`.epub`)**: Extracts image assets from the EPUB ZIP container sorted via `natural_cmp`.
  3. **PDF (`.pdf`)**: Uses `lopdf` + `flate2` (`ZlibDecoder`) to decode embedded JPEG/FlateDecode image XObjects directly into `DynamicImage` pages.
  4. **Raw Images (`raw_images` folders)**: Reads loose image files sorted via `natural_cmp`.
- **Single-Folder Unified Convention**: Chapters live directly inside `<Manga Title>/` without redundant language subdirectories or trailing `_` dummy scanlator suffixes.

---

## 3. Testing & Debugging Procedures

### Run Cloud Sync & Secret Storage Tests
```powershell
cargo test backend::cloud -- --nocapture
cargo test backend::secrets -- --nocapture
```

### Run Offline Reader & Archive Extraction Tests
```powershell
cargo test backend::offline -- --nocapture
```

### Common Debugging Checks
- **OAuth Redirect / Token Refresh Issues**: Check `exchange_authorization_code` and `refresh_access_token` in `src/backend/cloud.rs`. Verify that `GOOGLE_DRIVE_SCOPE` remains `drive.file`.
- **Cloud UI Modal (`src/view/widgets/cloud.rs`)**: Ensure all status messages use `crate::i18n::tr(es, en)` and theme colors from `crate::theme::palette()`.
- **Mandatory Local Sync After Changes**:
  ```powershell
  cargo fmt --all
  cargo install --path .
  .\scripts\sync-local-install.ps1
  ```
