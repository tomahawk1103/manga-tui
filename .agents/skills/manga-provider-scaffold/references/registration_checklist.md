# Provider Registration Checklist

When adding `<name>` (struct `<Name>Provider`, enum variant `MangaProviders::<Name>`), you must touch the following files:

---

### 1. `src/backend/manga_provider.rs`
* Declare module:
  ```rust
  pub mod <name>;
  ```
* Add to enum `MangaProviders`:
  ```rust
  pub enum MangaProviders {
      // ...
      #[strum(to_string = "<name>")]
      <Name>,
  }
  ```

---

### 2. `src/backend/multi_provider.rs`
* Import `<Name>Provider` and `<Name>FilterState`.
* Add field to `MultiProvider`:
  ```rust
  pub struct MultiProvider {
      // ...
      pub <name>: <Name>Provider,
  }
  ```
* Initialize in `MultiProvider::new`:
  ```rust
  <name>: <Name>Provider::new(Arc::clone(&cache)),
  ```
* Set provider enum in `search_all`:
  ```rust
  manga.provider = Some(MangaProviders::<Name>);
  ```
* Add match arm in all dispatch methods:
  - `get_manga_by_id`: `MangaProviders::<Name> => pool.<name>.get_manga_by_id(manga_id)...`
  - `get_chapter_pages`: `MangaProviders::<Name> => pool.<name>.get_chapter_pages_url_with_extension(...)`
  - `get_raw_image`: `MangaProviders::<Name> => pool.<name>.get_raw_image(...)`
  - `get_manga_chapters` & `get_all_chapters` (used by reader & new-chapter notifications across all providers): `MangaProviders::<Name> => pool.<name>.get_all_chapters(...)`
  - `get_popular_mangas`: `MangaProviders::<Name> => pool.<name>.get_popular_mangas(...)`
  - `get_latest_mangas`: `MangaProviders::<Name> => pool.<name>.get_latest_mangas(...)`

---

### 3. `src/main.rs`
* Import `backend::manga_provider::<name>::<Name>Provider`.
* Add match arm in provider startup router:
  ```rust
  MangaProviders::<Name> => {
      run_app(
          terminal,
          events,
          <Name>Provider::new(cache_provider),
          // ...
      ).await
  }
  ```

---

### 4. `src/view/app.rs`
* Specify default page size in `search_page_size()`:
  ```rust
  MangaProviders::<Name> => 20, // Or whatever page size the site returns
  ```

---

### 5. `src/view/widgets/search.rs`, `src/view/pages/manga.rs` & `src/view/widgets/home.rs`
* Search item provider tag style and prefix (`src/view/widgets/search.rs`):
  ```rust
  MangaProviders::<Name> => ("<Name>", Color::<ChosenColor>),
  ```
* Manga details header provider badge (`src/view/pages/manga.rs`):
  ```rust
  crate::backend::manga_provider::MangaProviders::<Name> => ("<Name>", ratatui::style::Color::<ChosenColor>),
  ```
* Download / New Chapter Notification popup badge (`src/view/widgets/home.rs`):
  Ensure provider badge rendering covers `MangaProviders::<Name>`.

---

### 6. Verification & Local Sync
Run check and tests to ensure all exhaustive pattern matches compile, then install and sync:
```powershell
cargo check
cargo test
cargo install --path .
.\scripts\sync-local-install.ps1
```
