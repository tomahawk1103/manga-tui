---
name: manga-provider-scaffold
description: >-
  Use this skill when scaffolding, implementing, or registering a new manga provider
  in manga-tui. Guides through file structure, required traits, HTML/API scraping,
  MultiProvider health & notifications registration, and live test creation.
---

# Manga Provider Scaffold Skill

This skill guides you through implementing and registering a new manga provider in `manga-tui`.

## Workflow Overview

1. **Information Gathering & Target Analysis**
2. **File Structure Generation**
3. **HTTP Client & Scraper Implementation (`response.rs` & `<name>.rs`)**
4. **Trait Implementations**
5. **Codebase Registration (MultiProvider, Notifications, UI Badges)**
6. **Live & Unit Testing**
7. **Binary Installation & Local Sync**

---

## 1. Information Gathering & Target Analysis

Before writing code, analyze the target site:
- **Base URL & Language**: e.g., English, Spanish (`Languages::English`, `Languages::Spanish`).
- **Data Source**:
  - Direct JSON API (like MangaDex).
  - Server-rendered HTML (like WeebCentral, LeerCapitulo, MangaPill, InManga, Comizy) using `scraper::Html`.
  - Form/POST searches or CSRF tokens (like MangaOni).
- **Anti-Bot / Cloudflare Protections**:
  - Requires browser `User-Agent` and `sec-ch-ua` headers.
  - Requires specific `Referer` header (especially for loading chapter images).
  - CookieJar (`cookie_store(true)`) or session headers.

---

## 2. Directory & File Structure

Create the following files under `src/backend/manga_provider/`:

```text
src/backend/manga_provider/
├── <name>.rs                  # Provider struct, client, trait impls, tests
└── <name>/
    ├── response.rs            # Scraper parsing / API response models
    ├── filter_state.rs        # Filter state & FiltersHandler impl
    └── filter_widget.rs       # Filter UI widget & FiltersWidget impl
```

---

## 3. Core Implementation Checklist

### A. Data Parsing (`<name>/response.rs`)
Implement the structs that convert raw HTML/JSON into `manga-tui` models:
- Search results -> `SearchManga` / `PopularManga` / `RecentlyAddedManga`
- Manga details -> `Manga` (title, authors, status, genres, cover URL, `provider: Some(MangaProviders::<Name>)`)
- Chapter list -> `ChapterReader` or `Chapter` (ensure chapter IDs are deterministic and deduplicated so notification checks don't trigger false positives)
- Reader page links -> `Vec<ChapterPageUrl>`

### B. Provider Struct (`<name>.rs`)
Define the provider struct and constructors:
```rust
#[derive(Clone, Debug)]
pub struct <Name>Provider {
    client: reqwest::Client,
    cache_provider: Arc<dyn Cacher>,
}

impl <Name>Provider {
    const CHAPTER_PAGE_CACHE_DURATION: CacheDuration = CacheDuration::Long;
    const HOME_PAGE_CACHE_DURATION: CacheDuration = CacheDuration::Short;
    const MANGA_PAGE_CACHE_DURATION: CacheDuration = CacheDuration::LongLong;
    const SEARCH_PAGE_CACHE_DURATION: CacheDuration = CacheDuration::VeryShort;

    pub fn new(cache_provider: Arc<dyn Cacher>) -> Self {
        // Build reqwest::Client with proper headers (User-Agent, Referer, Accept, Cookie store)
    }
}
```

### C. Required Traits
Implement all required traits for full integration:
- `ProviderIdentity`: Returns `MangaProviders::<Name>`
- `GetRawImage`: Fetches image bytes (often requires `Referer` header)
- `DecodeBytesToImage`, `SearchMangaPanel`, `ReaderPageProvider`, `MangaProvider`: Marker/super traits
- `HomePageMangaProvider`: Popular & recently added mangas for homepage
- `SearchMangaById`: Manga details page
- `GetChapterPages`: URLs for all images in a chapter
- `SearchChapterById`: Look up single chapter
- `FetchChapterBookmarked`: Sync bookmark status
- `GoToReadChapter`: Resolve chapter reader context
- `MangaPageProvider`: Chapter listings & pagination
- `SearchPageProvider`: Search query & filters execution
- `FeedPageProvider`: Provider feed / latest updates

See [references/trait_cheatsheet.md](./references/trait_cheatsheet.md) for boilerplate implementations.

---

## 4. Codebase Registration

See [references/registration_checklist.md](./references/registration_checklist.md) for the exact code touchpoints across:
1. `src/backend/manga_provider.rs`: `pub mod <name>;` and add variant to `pub enum MangaProviders`.
2. `src/backend/multi_provider.rs`:
   - Add field to `MultiProvider` and initialize in `MultiProvider::new`.
   - Include in `search_all`, `get_manga_by_id`, `get_chapter_pages`, `get_raw_image`, `get_manga_chapters`, `get_popular_mangas`, `get_latest_mangas`, and cross-provider new chapter notification checks.
   - Circuit breaker / `ProviderHealthTracker` automatically tracks health per `MangaProviders` variant.
3. `src/main.rs`: Match arm in startup provider router.
4. `src/view/app.rs`: Set pagination size in `search_page_size()`.
5. `src/view/widgets/search.rs`, `src/view/pages/manga.rs`, and `src/view/widgets/home.rs`: Provider badge label & color in search results, details view, and new-chapter notification / download popups.

---

## 5. Live & Unit Testing

Add unit tests (for HTML/JSON parsing and ID deduplication) and ignored live tests to `src/backend/manga_provider/<name>.rs`:

```powershell
cargo test backend::manga_provider::<name> -- --nocapture
cargo test backend::manga_provider::<name> -- --ignored --nocapture
```

---

## 6. Binary Installation & Local Sync (Mandatory)

Always install the updated binary and sync the local Windows installation so the new provider is immediately active everywhere:
```powershell
cargo fmt --all
cargo install --path .
.\scripts\sync-local-install.ps1
```
