---
name: provider-doctor
description: >-
  Use this skill to diagnose, debug, and repair broken manga providers in manga-tui.
  Covers investigating failing searches, chapter list errors, duplicate chapter IDs
  in notifications, image loading issues, fixing CSS selectors in response.rs,
  circuit breaker health in MultiProvider, and bypassing HTTP 403/Cloudflare blocks.
---

# Provider Doctor Skill

This skill provides a standardized diagnostic and repair workflow when a manga provider in `manga-tui` stops working, trips the circuit breaker, or produces broken chapter/notification data.

---

## 1. Triage & Reproduction

### Step 1.1: Run Provider Unit & Live Tests
Locate the provider's test suite in `src/backend/manga_provider/<provider>.rs` and execute:
```powershell
cargo test backend::manga_provider::<provider> -- --nocapture
cargo test backend::manga_provider::<provider> -- --ignored --nocapture
```

If no live test exists for the failing behavior, write a `#[tokio::test] #[ignore]` test in `<provider>.rs` targeting the failing method, plus a non-ignored unit test with a captured HTML/JSON fixture.

### Step 1.2: Classify the Failure
Identify which phase is breaking:
| Failure Type | Typical Symptoms | Suspected Component |
| :--- | :--- | :--- |
| **Search Broken** | Returns 0 results or `NoneError` | `search_mangas` / `response.rs` search selector |
| **Manga Details Broken** | Cannot open manga page / 404 | `get_manga_by_id` / slug URL format |
| **Chapters Missing** | Manga opens but chapter list is empty | `get_all_chapters` / chapter table selector |
| **False New-Chapter Notifications** | Same chapter notified repeatedly | Non-deterministic or duplicate chapter IDs in `response.rs` (e.g. MangaOni ID deduplication) |
| **Pages Blank / Reader Stuck** | Reader shows 0 pages | `get_chapter_pages` / image selector or script JSON extraction |
| **HTTP 403 / Circuit Open (🔴)** | `ProviderError::CloudflareBlocked` or `RateLimited` | Missing `Referer`, `User-Agent`, or tripped `ProviderHealthTracker` in `multi_provider.rs` |

---

## 2. Inspecting Live Response

When a CSS selector or JSON parser fails, inspect the actual live response returned by the site.

### Step 2.1: Fetch HTML to Scratchpad
Fetch the HTML payload using `curl` with standard browser headers:
```powershell
curl.exe -s -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36" -H "Referer: <BASE_URL>" "<TARGET_URL>" -o "scratch_page.html"
```
*(Remember to remove temporary scratch files before committing).*

### Step 2.2: Analyze the Payload
1. **Cloudflare Challenge?** Search for `Just a moment...` or `challenge-platform`. If present, headers need adjusting (see [references/common_blockers.md](./references/common_blockers.md)).
2. **Changed Class Names?** Websites often update CSS class names (e.g. Tailwind hashed classes). Prefer semantic tags, `id`, or `data-*` attributes (e.g. `[data-chapter-id]`, `a[href*='/chapter/']`).
3. **Moved to Client-Side Hydration?** Search for `<script id="__NEXT_DATA__">` or `window.__INITIAL_STATE__`. Parsing embedded JSON is far more resilient than CSS scraping.

---

## 3. Repairing Parsers & Chapter IDs (`response.rs`)

### CSS Selector & Embedded JSON Repairs
- Use resilient selectors in `src/backend/manga_provider/<provider>/response.rs`.
- Ensure chapter numbers are parsed cleanly into both `number: f64` and `number_str: String`.
- **Deduplicate Chapter IDs**: When scraping HTML lists that may contain duplicate links (e.g., mobile + desktop DOM nodes or "latest chapter" banners above the full chapter list), deduplicate by chapter ID or normalized chapter number so the notification checker (`check_new_chapters`) and download modal don't show duplicates.

---

## 4. Fixing HTTP 403 / Image Loading Errors

If `get_raw_image` fails with 403 Forbidden:
1. Ensure the `REFERER` header is sent with the image request:
   ```rust
   let res = self.client.get(url)
       .header(http::header::REFERER, "<PROVIDER_BASE_URL>")
       .send()
       .await?;
   ```
2. Verify if a custom CDN hostname requires `Accept: image/avif,image/webp,image/*,*/*;q=0.8`.

---

## 5. Verification, Installation & Local Sync

1. Run unit and live tests:
   ```powershell
   cargo test backend::manga_provider::<provider> -- --nocapture
   cargo test backend::manga_provider::<provider> -- --ignored --nocapture
   ```
2. Check Clippy and formatting:
   ```powershell
   cargo clippy --all-targets -- -D warnings
   cargo fmt --all
   ```
3. **Mandatory: Install updated binary and sync local Windows install**:
   ```powershell
   cargo install --path .
   .\scripts\sync-local-install.ps1
   ```
