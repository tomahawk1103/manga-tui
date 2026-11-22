---
name: download-tester
description: >-
  Use this skill to test, validate, and debug manga chapter downloads across supported
  formats (CBZ, EPUB, PDF, and RAW images), background download queues, and the 100%
  Offline Reader in manga-tui. Covers unified directory structures, format fallbacks,
  and archive verification.
---

# Manga Download & Offline Reader Tester Skill

This skill provides procedures for running, debugging, and validating chapter downloads, the background download queue, and the 100% offline reader in `manga-tui`.

---

## 1. Architecture & Supported Formats

### Downloader Implementations (`src/backend/manga_downloader/`)
- `cbz_downloader.rs`: ZIP archive containing sorted images with `.cbz` extension.
- `epub_downloader.rs`: Valid EPUB container (`mimetype`, `META-INF/container.xml`, OPF manifest, and XHTML page wrappers).
- `pdf_downloader.rs`: Single PDF document embedding decoded images via `lopdf`.
- `raw_images.rs`: Chapter folder containing sequential image files (`1.jpg`, `2.jpg`, etc.).
- `src/backend/download_queue.rs`: Background download manager handling concurrent chapter downloads, progress reporting, and failure retries.

### Unified Folder Structure & Naming (`src/backend/manga_downloader.rs`)
- Downloads are stored under `<data_dir>/mangaDownloads/<Manga Title>/`.
- **Single-Folder Unified Convention**: Chapters are saved directly inside the manga folder without unnecessary language subfolders when a single language is used, and dummy scanlator underscores (e.g. trailing `_`) are stripped cleanly via `ChapterToDownloadSanitized`.
- File existence verification: notifications and chapter lists verify physical files on disk (`chapter_download_id_candidates`) so deleted files don't show stale "Downloaded" badges.

### 100% Offline Reader (`src/backend/offline.rs`)
- Scans `mangaDownloads/` and cross-references `Database` (`OfflineCatalogMetadata`) to provide offline reading even without an internet connection.
- Supports reading **all 4 formats directly in the TUI reader**:
  - `.cbz` & `.epub` (extracts image entries from ZIP archive sorted with `natural_cmp`).
  - `.pdf` (extracts embedded images/streams from PDF pages using `lopdf` and `flate2`).
  - `raw_images` directories (reads image files sorted with `natural_cmp`).
- **Incomplete Download Fallback**: If a local `.cbz` or folder is missing or corrupted, the reader gracefully falls back or reports a clear status without crashing or clipping the UI.

---

## 2. Running Download & Offline Tests

### Unit Tests (Fast, Non-Ignored)
Test path sanitization, offline natural sorting, format detection, and PDF/archive extraction:
```powershell
cargo test backend::offline -- --nocapture
cargo test backend::manga_downloader -- --nocapture
cargo test backend::download_queue -- --nocapture
```

### Ignored Disk I/O Downloader Tests (`./test_results/`)
Tests that write full sample archives to `./test_results/` are marked `#[ignore]`:

```powershell
# Run all disk downloader tests:
cargo test backend::manga_downloader -- --ignored --nocapture

# Or via cargo-make:
cargo make download-all
```

---

## 3. Validating Download Outputs

Inspect `./test_results/` or the user's `mangaDownloads` directory:

1. **Verify File Existence & Non-Zero Size**:
   ```powershell
   Get-ChildItem -Path "./test_results" -Recurse | Select-Object Name, Length, FullName
   ```
2. **CBZ / EPUB Validation**:
   - Ensure archive opens cleanly with `ZipArchive` and image names sort naturally (`1.jpg`, `2.jpg`, ..., `10.jpg` via `natural_cmp`).
   - For EPUB: verify the first entry is `mimetype` with `application/epub+zip`.
3. **PDF Validation**:
   - Verify file starts with `%PDF-` and `offline.rs` can extract all page images back into `DynamicImage` frames.
4. **Raw Images Validation**:
   - Ensure files are named sequentially and pass `is_supported_image_name` (`.jpg`, `.jpeg`, `.png`, `.webp`, `.avif`).

---

## 4. Troubleshooting Common Issues

- **Duplicate or Unmatched Downloaded Chapters**: Check `normalize_chapter_number` in `src/backend/database.rs` and `chapter_download_id_candidates` in `src/backend/manga_downloader.rs`.
- **Cloud Sync of Downloads**: Downloaded `.cbz`, `.epub`, and `.pdf` files are synced to `MangaTUI/downloads/<manga_folder>/` in Google Drive (`src/backend/cloud.rs`). Ensure filename sanitization remains consistent across Windows and Unix.
