use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Mutex, MutexGuard};

use manga_core::SearchTerm;
use manga_core::backend::database::{
    ChapterToSaveHistory, Database, GetHistoryArgs, HomeStats, MangaHistory, MangaHistoryType, MangaReadingHistorySave,
};
use manga_core::backend::manga_provider::{Languages, MangaProviders};
use manga_core::backend::migration::update_database_with_migrations;
use manga_core::backend::offline::scan_offline_mangas_in_dir;
use manga_core::backend::{AppDirectories, StorageMode, build_data_dir, current_storage_mode};
use manga_core::config::{MangaTuiConfig, UiLanguage, get_config_directory_path};
use manga_core::logger::Logger;
use manga_core::runtime_settings::{
    ALL_PROVIDERS, active_download_type, download_type_toml_value, persist_setting_in_dir, set_active_chapter_language,
    set_active_default_provider, set_active_ui_language, ui_lang_toml_value,
};
use serde::{Deserialize, Serialize};

const MANGORA_PREFS_FILE: &str = "mangora_prefs.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    #[default]
    Paper,
    Ink,
}

impl ThemeMode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "paper" | "papel" => Ok(Self::Paper),
            "ink" | "tinta" | "dark" => Ok(Self::Ink),
            other => Err(format!("Unsupported theme mode: '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderViewMode {
    #[default]
    Cascade,
    Single,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadingDirection {
    #[default]
    Rtl,
    Ltr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageFitMode {
    #[default]
    FitWidth,
    FitHeight,
    Original,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReaderPrefs {
    pub view_mode: ReaderViewMode,
    pub direction: ReadingDirection,
    pub fit_mode: PageFitMode,
    pub zoom_percent: u16,
    pub cascade_gap_px: u16,
    pub first_page_single: bool,
    pub pin_toolbars: bool,
    pub dark_canvas: bool,
}

impl Default for ReaderPrefs {
    fn default() -> Self {
        Self {
            view_mode: ReaderViewMode::Cascade,
            direction: ReadingDirection::Rtl,
            fit_mode: PageFitMode::FitWidth,
            zoom_percent: 100,
            cascade_gap_px: 8,
            first_page_single: true,
            pin_toolbars: false,
            dark_canvas: true,
        }
    }
}

impl ReaderPrefs {
    pub fn normalized(mut self) -> Self {
        self.zoom_percent = self.zoom_percent.clamp(50, 250);
        self.cascade_gap_px = self.cascade_gap_px.clamp(0, 48);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MangoraPrefs {
    pub onboarding_completed: bool,
    pub favorite_genres: Vec<String>,
    pub theme_mode: ThemeMode,
    pub reader_prefs: ReaderPrefs,
    pub support_banner_dismissed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDetection {
    pub has_existing_data: bool,
    pub library_mangas: u32,
    pub chapters_read: u32,
    pub total_pages_read: u32,
    pub favorites: u32,
    pub downloaded_chapters: u32,
    pub offline_series_count: usize,
    pub last_read_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOption {
    pub id: String,
    pub name: String,
    pub badge: String,
    pub language_group: String,
    pub description_es: String,
    pub description_en: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialAppState {
    pub app_version: String,
    pub ui_language: String,
    pub theme_mode: ThemeMode,
    pub platform: String,
    pub is_mobile: bool,
    pub can_open_data_folder: bool,
    pub storage_mode: String,
    pub storage_mode_label: String,
    pub data_dir: String,
    pub config_dir: String,
    pub downloads_dir: String,
    pub active_provider: String,
    pub recommended_provider: String,
    pub available_providers: Vec<ProviderOption>,
    pub favorite_genres: Vec<String>,
    pub onboarding_completed: bool,
    pub download_format: String,
    pub reader_prefs: ReaderPrefs,
    pub support_banner_dismissed: bool,
    pub library: LibraryDetection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingPreferencesPayload {
    pub ui_language: String,
    pub favorite_genres: Vec<String>,
    pub provider: String,
    pub onboarding_completed: bool,
    pub theme_mode: Option<ThemeMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub provider_label: String,
    pub cover_url: Option<String>,
    pub last_read: Option<String>,
    pub last_read_chapter: Option<String>,
    pub total_read_count: u32,
    pub bookmark_chapter: Option<String>,
    pub bookmark_page: Option<u32>,
    pub is_favorite: bool,
    pub is_plan_to_read: bool,
    pub downloaded_count: u32,
    pub shelf_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryOverview {
    pub continue_reading: Vec<LibraryEntry>,
    pub favorites: Vec<LibraryEntry>,
    pub recent_history: Vec<LibraryEntry>,
    pub total_continue_reading: u32,
    pub total_favorites: u32,
    pub total_recent: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleFavoritePayload {
    pub manga_id: String,
    pub provider: String,
    pub title: String,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleFavoriteResult {
    pub is_favorite: bool,
    pub app_state: InitialAppState,
    pub library: LibraryOverview,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleChapterReadPayload {
    pub manga_id: String,
    pub provider: String,
    pub manga_title: String,
    pub cover_url: Option<String>,
    pub chapter_id: String,
    pub chapter_title: String,
    pub translated_language: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleChapterReadResult {
    pub is_read: bool,
    pub app_state: InitialAppState,
    pub library: LibraryOverview,
}

#[derive(Debug)]
struct AppStateInner {
    data_dir: PathBuf,
    config_dir: PathBuf,
    downloads_dir: PathBuf,
    db_path: PathBuf,
    storage_mode: StorageMode,
    ui_language: UiLanguage,
    default_provider: MangaProviders,
    default_chapter_language: Languages,
    prefs: MangoraPrefs,
}

#[derive(Debug)]
pub struct AppState {
    inner: Mutex<AppStateInner>,
}

impl AppState {
    pub fn init_shared_storage() -> Result<Self, String> {
        let logger = Logger;
        build_data_dir(&logger).map_err(|e| format!("Could not initialize shared manga-tui data directory: {e}"))?;

        let mut conn = Database::get_connection().map_err(|e| format!("Could not open shared SQLite database: {e}"))?;
        let db = Database::new(&conn);
        db.setup().map_err(|e| format!("Could not initialize database tables: {e}"))?;
        update_database_with_migrations(&mut conn, &logger).map_err(|e| format!("Could not run database migrations: {e}"))?;
        drop(conn);

        let config = MangaTuiConfig::get().effective();
        let data_dir = AppDirectories::get_app_directory().to_path_buf();
        let config_dir = get_config_directory_path();
        let downloads_dir = AppDirectories::MangaDownloads.get_full_path();
        let db_path = AppDirectories::History.get_full_path();
        let storage_mode = current_storage_mode();

        Ok(Self::from_paths(
            data_dir,
            config_dir,
            downloads_dir,
            db_path,
            storage_mode,
            config.ui_language,
            config.default_manga_provider,
            config.default_language,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_paths(
        data_dir: PathBuf,
        config_dir: PathBuf,
        downloads_dir: PathBuf,
        db_path: PathBuf,
        storage_mode: StorageMode,
        ui_language: UiLanguage,
        default_provider: MangaProviders,
        default_chapter_language: Languages,
    ) -> Self {
        let prefs = load_prefs_from_dir(&config_dir);
        Self {
            inner: Mutex::new(AppStateInner {
                data_dir,
                config_dir,
                downloads_dir,
                db_path,
                storage_mode,
                ui_language,
                default_provider,
                default_chapter_language,
                prefs,
            }),
        }
    }

    fn lock_inner(&self) -> Result<MutexGuard<'_, AppStateInner>, String> {
        self.inner.lock().map_err(|_| "Application state lock poisoned".to_string())
    }

    pub fn snapshot(&self) -> Result<InitialAppState, String> {
        let inner = self.lock_inner()?;
        Ok(build_initial_state(&inner))
    }

    pub fn data_dir(&self) -> Result<PathBuf, String> {
        let inner = self.lock_inner()?;
        Ok(inner.data_dir.clone())
    }

    pub fn config_dir(&self) -> Result<PathBuf, String> {
        let inner = self.lock_inner()?;
        Ok(inner.config_dir.clone())
    }

    pub fn downloads_dir(&self) -> Result<PathBuf, String> {
        let inner = self.lock_inner()?;
        Ok(inner.downloads_dir.clone())
    }

    pub fn db_path(&self) -> Result<PathBuf, String> {
        let inner = self.lock_inner()?;
        Ok(inner.db_path.clone())
    }

    pub fn default_chapter_language(&self) -> Result<Languages, String> {
        let inner = self.lock_inner()?;
        Ok(inner.default_chapter_language)
    }

    pub fn get_reader_prefs(&self) -> Result<ReaderPrefs, String> {
        let inner = self.lock_inner()?;
        Ok(inner.prefs.reader_prefs.clone())
    }

    pub fn save_reader_prefs(&self, prefs: ReaderPrefs) -> Result<InitialAppState, String> {
        let normalized = prefs.normalized();
        let mut inner = self.lock_inner()?;
        let mut next_prefs = inner.prefs.clone();
        next_prefs.reader_prefs = normalized;
        save_prefs_in_dir(&inner.config_dir, &next_prefs)?;
        inner.prefs = next_prefs;
        Ok(build_initial_state(&inner))
    }

    pub fn open_db_connection(&self) -> Result<rusqlite::Connection, String> {
        let db_path = self.db_path()?;
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Could not create database folder: {e}"))?;
        }
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| format!("Could not open shared SQLite database: {e}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;",
        )
        .map_err(|e| format!("Could not configure SQLite connection: {e}"))?;
        let db = Database::new(&conn);
        db.setup().map_err(|e| format!("Could not initialize database schema: {e}"))?;
        Ok(conn)
    }

    pub fn get_library_overview(&self, search: Option<&str>) -> Result<LibraryOverview, String> {
        let conn = self.open_db_connection()?;
        let db = Database::new(&conn);
        let covers = load_cover_urls(&conn);

        let recent_res = db
            .get_continue_reading(GetHistoryArgs {
                hist_type: MangaHistoryType::ReadingHistory,
                page: 1,
                search: search.and_then(SearchTerm::trimmed_lowercased),
                items_per_page: 48,
                provider: MangaProviders::default(),
            })
            .map_err(|e| format!("Could not load reading history: {e}"))?;

        let lib_res = db
            .get_library(GetHistoryArgs {
                hist_type: MangaHistoryType::PlanToRead,
                page: 1,
                search: search.and_then(SearchTerm::trimmed_lowercased),
                items_per_page: 64,
                provider: MangaProviders::default(),
            })
            .map_err(|e| format!("Could not load library favorites: {e}"))?;

        let recent_history: Vec<LibraryEntry> = recent_res
            .mangas
            .into_iter()
            .map(|item| map_history_to_library_entry(item, &covers))
            .collect();
        let continue_reading: Vec<LibraryEntry> = recent_history.iter().take(12).cloned().collect();
        let favorites: Vec<LibraryEntry> = lib_res
            .mangas
            .into_iter()
            .map(|item| map_history_to_library_entry(item, &covers))
            .collect();

        Ok(LibraryOverview {
            total_continue_reading: continue_reading.len() as u32,
            total_favorites: lib_res.total_items,
            total_recent: recent_res.total_items,
            continue_reading,
            favorites,
            recent_history,
        })
    }

    pub fn toggle_library_favorite(&self, payload: ToggleFavoritePayload) -> Result<ToggleFavoriteResult, String> {
        let manga_id = payload.manga_id.trim();
        let title = payload.title.trim();
        if manga_id.is_empty() || title.is_empty() {
            return Err("Missing manga identifier or title".to_string());
        }
        let provider = parse_provider(&payload.provider)?;

        let is_favorite = {
            let conn = self.open_db_connection()?;
            let db = Database::new(&conn);
            let next_fav = db
                .toggle_favorite(manga_id, provider, title)
                .map_err(|e| format!("Could not update favorite status: {e}"))?;

            if next_fav
                && let Some(cover_url) = payload
                    .cover_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            {
                let provider_str = provider.to_string();
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO mangas (id, title, img_url, manga_provider) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![manga_id, title, cover_url, provider_str],
                );
                let _ = conn.execute(
                    "UPDATE mangas SET img_url = ?1 WHERE id = ?2 AND (img_url IS NULL OR TRIM(img_url) = '')",
                    rusqlite::params![cover_url, manga_id],
                );
            }
            next_fav
        };

        Ok(ToggleFavoriteResult {
            is_favorite,
            app_state: self.snapshot()?,
            library: self.get_library_overview(None)?,
        })
    }

    pub fn toggle_chapter_read(&self, payload: ToggleChapterReadPayload) -> Result<ToggleChapterReadResult, String> {
        let manga_id = payload.manga_id.trim();
        let manga_title = payload.manga_title.trim();
        let chapter_id = payload.chapter_id.trim();
        let chapter_title = payload.chapter_title.trim();
        if manga_id.is_empty() || manga_title.is_empty() || chapter_id.is_empty() || chapter_title.is_empty() {
            return Err("Missing manga or chapter identifier".to_string());
        }
        let provider = parse_provider(&payload.provider)?;
        let cover_url = payload
            .cover_url
            .as_deref()
            .map(str::trim)
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
        let translated_language = payload.translated_language.trim();

        let is_read = {
            let conn = self.open_db_connection()?;
            let db = Database::new(&conn);
            db.toggle_chapter_read(MangaReadingHistorySave {
                id: manga_id,
                title: manga_title,
                img_url: cover_url,
                chapter: ChapterToSaveHistory {
                    id: chapter_id,
                    title: chapter_title,
                    translated_language,
                },
                provider,
            })
            .map_err(|e| format!("Could not update chapter reading status: {e}"))?
        };

        Ok(ToggleChapterReadResult {
            is_read,
            app_state: self.snapshot()?,
            library: self.get_library_overview(None)?,
        })
    }

    pub fn save_onboarding_preferences(&self, payload: OnboardingPreferencesPayload) -> Result<InitialAppState, String> {
        let ui_language = parse_ui_language(&payload.ui_language)?;
        let provider = parse_provider(&payload.provider)?;
        let chapter_language = default_chapter_language_for_ui(ui_language);
        let favorite_genres = sanitize_genres(&payload.favorite_genres);

        let mut inner = self.lock_inner()?;
        let mut next_prefs = inner.prefs.clone();
        next_prefs.onboarding_completed = payload.onboarding_completed;
        next_prefs.favorite_genres = favorite_genres;
        if let Some(theme_mode) = payload.theme_mode {
            next_prefs.theme_mode = theme_mode;
        }

        persist_shared_config(&inner.config_dir, ui_language, provider, chapter_language)?;
        save_prefs_in_dir(&inner.config_dir, &next_prefs)?;

        inner.ui_language = ui_language;
        inner.default_provider = provider;
        inner.default_chapter_language = chapter_language;
        inner.prefs = next_prefs;
        sync_runtime_atomics(ui_language, provider, chapter_language);

        Ok(build_initial_state(&inner))
    }

    pub fn set_ui_language(&self, language: &str) -> Result<InitialAppState, String> {
        let ui_language = parse_ui_language(language)?;
        let chapter_language = default_chapter_language_for_ui(ui_language);

        let mut inner = self.lock_inner()?;
        persist_setting_in_dir(&inner.config_dir, "ui_language", ui_lang_toml_value(ui_language))?;
        persist_setting_in_dir(&inner.config_dir, "default_language", chapter_language.as_iso_code())?;

        inner.ui_language = ui_language;
        inner.default_chapter_language = chapter_language;
        set_active_ui_language(ui_language);
        set_active_chapter_language(chapter_language);

        Ok(build_initial_state(&inner))
    }

    pub fn set_theme_mode(&self, theme_mode_raw: &str) -> Result<InitialAppState, String> {
        let theme_mode = ThemeMode::parse(theme_mode_raw)?;

        let mut inner = self.lock_inner()?;
        let mut next_prefs = inner.prefs.clone();
        next_prefs.theme_mode = theme_mode;
        save_prefs_in_dir(&inner.config_dir, &next_prefs)?;
        inner.prefs = next_prefs;

        Ok(build_initial_state(&inner))
    }

    pub fn set_default_provider(&self, provider_raw: &str) -> Result<InitialAppState, String> {
        let provider = parse_provider(provider_raw)?;

        let mut inner = self.lock_inner()?;
        persist_setting_in_dir(&inner.config_dir, "default_manga_provider", &provider.to_string())?;
        inner.default_provider = provider;
        set_active_default_provider(provider);

        Ok(build_initial_state(&inner))
    }

    pub fn set_favorite_genres(&self, genres: &[String]) -> Result<InitialAppState, String> {
        let sanitized = sanitize_genres(genres);

        let mut inner = self.lock_inner()?;
        let mut next_prefs = inner.prefs.clone();
        next_prefs.favorite_genres = sanitized;
        save_prefs_in_dir(&inner.config_dir, &next_prefs)?;
        inner.prefs = next_prefs;

        Ok(build_initial_state(&inner))
    }

    pub fn set_onboarding_completed(&self, completed: bool) -> Result<InitialAppState, String> {
        let mut inner = self.lock_inner()?;
        let mut next_prefs = inner.prefs.clone();
        next_prefs.onboarding_completed = completed;
        save_prefs_in_dir(&inner.config_dir, &next_prefs)?;
        inner.prefs = next_prefs;

        Ok(build_initial_state(&inner))
    }

    pub fn dismiss_support_banner(&self) -> Result<InitialAppState, String> {
        let mut inner = self.lock_inner()?;
        let mut next_prefs = inner.prefs.clone();
        next_prefs.support_banner_dismissed = true;
        save_prefs_in_dir(&inner.config_dir, &next_prefs)?;
        inner.prefs = next_prefs;

        Ok(build_initial_state(&inner))
    }
}

pub fn recommended_provider_for_language(lang: UiLanguage) -> MangaProviders {
    match lang {
        UiLanguage::Es => MangaProviders::Inmanga,
        UiLanguage::En => MangaProviders::Weebcentral,
    }
}

fn default_chapter_language_for_ui(lang: UiLanguage) -> Languages {
    match lang {
        UiLanguage::Es => Languages::Spanish,
        UiLanguage::En => Languages::English,
    }
}

fn parse_ui_language(raw: &str) -> Result<UiLanguage, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "es" | "spanish" | "español" | "espanol" => Ok(UiLanguage::Es),
        "en" | "english" | "inglés" | "ingles" => Ok(UiLanguage::En),
        other => Err(format!("Unsupported UI language: '{other}'")),
    }
}

fn parse_provider(raw: &str) -> Result<MangaProviders, String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    MangaProviders::from_str(&trimmed).map_err(|_| format!("Unsupported manga provider: '{raw}'"))
}

fn sanitize_genres(genres: &[String]) -> Vec<String> {
    let mut cleaned = Vec::new();
    for raw in genres {
        let normalized = raw.trim().to_ascii_lowercase();
        if !normalized.is_empty() && !cleaned.contains(&normalized) {
            cleaned.push(normalized);
        }
    }
    cleaned
}

fn sync_runtime_atomics(ui_language: UiLanguage, provider: MangaProviders, chapter_language: Languages) {
    set_active_ui_language(ui_language);
    set_active_default_provider(provider);
    set_active_chapter_language(chapter_language);
}

fn persist_shared_config(
    config_dir: &Path,
    ui_language: UiLanguage,
    provider: MangaProviders,
    chapter_language: Languages,
) -> Result<(), String> {
    persist_setting_in_dir(config_dir, "ui_language", ui_lang_toml_value(ui_language))?;
    persist_setting_in_dir(config_dir, "default_manga_provider", &provider.to_string())?;
    persist_setting_in_dir(config_dir, "default_language", chapter_language.as_iso_code())?;
    Ok(())
}

fn prefs_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(MANGORA_PREFS_FILE)
}

fn load_prefs_from_dir(config_dir: &Path) -> MangoraPrefs {
    let path = prefs_file_path(config_dir);
    let Ok(raw) = fs::read_to_string(path) else {
        return MangoraPrefs::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_prefs_in_dir(config_dir: &Path, prefs: &MangoraPrefs) -> Result<(), String> {
    if !config_dir.exists() {
        fs::create_dir_all(config_dir).map_err(|e| format!("Could not create config directory: {e}"))?;
    }

    let path = prefs_file_path(config_dir);
    let tmp_path = config_dir.join(format!("{MANGORA_PREFS_FILE}.tmp"));
    let json = serde_json::to_string_pretty(prefs).map_err(|e| format!("Could not serialize Mangora preferences: {e}"))?;

    {
        let mut tmp = File::create(&tmp_path).map_err(|e| format!("Could not create temporary preferences file: {e}"))?;
        tmp.write_all(json.as_bytes())
            .map_err(|e| format!("Could not write Mangora preferences: {e}"))?;
        tmp.flush().map_err(|e| format!("Could not flush Mangora preferences: {e}"))?;
    }

    fs::rename(&tmp_path, &path).map_err(|e| format!("Could not replace Mangora preferences file: {e}"))?;
    Ok(())
}

fn storage_mode_id(mode: StorageMode) -> &'static str {
    match mode {
        StorageMode::EnvOverride => "env",
        StorageMode::Portable => "portable",
        StorageMode::System => "system",
        StorageMode::AppSandbox => "sandbox",
    }
}

pub fn platform_capabilities(os: &str) -> (&'static str, bool, bool) {
    match os {
        "windows" => ("windows", false, true),
        "macos" => ("macos", false, true),
        "linux" => ("linux", false, true),
        "android" => ("android", true, false),
        "ios" => ("ios", true, false),
        _ => ("unknown", false, false),
    }
}

fn load_cover_urls(conn: &rusqlite::Connection) -> HashMap<(String, String), String> {
    let mut covers = HashMap::new();
    let Ok(mut stmt) =
        conn.prepare("SELECT id, manga_provider, img_url FROM mangas WHERE img_url IS NOT NULL AND TRIM(img_url) != ''")
    else {
        return covers;
    };
    let Ok(rows) = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
    else {
        return covers;
    };
    for row in rows.flatten() {
        covers.insert((row.0, row.1.to_ascii_lowercase()), row.2);
    }
    covers
}

fn map_history_to_library_entry(item: MangaHistory, covers: &HashMap<(String, String), String>) -> LibraryEntry {
    let provider_id = item.provider.to_string();
    let cover_url = covers.get(&(item.id.clone(), provider_id.clone())).cloned();
    LibraryEntry {
        id: item.id,
        title: item.title,
        provider: provider_id,
        provider_label: item.provider.badge_label().to_string(),
        cover_url,
        last_read: item.last_read,
        last_read_chapter: item.last_read_chapter,
        total_read_count: item.total_read_count,
        bookmark_chapter: item.bookmark_chapter,
        bookmark_page: item.bookmark_page,
        is_favorite: item.is_favorite,
        is_plan_to_read: item.is_plan_to_read,
        downloaded_count: item.downloaded_count,
        shelf_name: item.shelf_name,
    }
}

pub fn available_providers() -> Vec<ProviderOption> {
    ALL_PROVIDERS
        .iter()
        .map(|provider| {
            let (language_group, description_es, description_en) = match provider {
                MangaProviders::Inmanga => (
                    "es",
                    "Recomendado en español. Catálogo amplio y carga rápida.",
                    "Recommended for Spanish readers. Wide catalog and fast loading.",
                ),
                MangaProviders::Mangaoni => (
                    "es",
                    "Alternativa en español con buen catálogo de series populares.",
                    "Spanish alternative with a solid selection of popular series.",
                ),
                MangaProviders::Leercapitulo => (
                    "es",
                    "Fuente adicional en español para capítulos recientes.",
                    "Additional Spanish source for recent chapter releases.",
                ),
                MangaProviders::Mangadex => (
                    "multi",
                    "Catálogo internacional multilingüe con metadatos completos.",
                    "International multi-language catalog with rich metadata.",
                ),
                MangaProviders::Weebcentral => (
                    "en",
                    "Recomendado en inglés. Gran catálogo en alta calidad.",
                    "Recommended for English readers. Large high-quality catalog.",
                ),
                MangaProviders::Mangapill => (
                    "en",
                    "Fuente rápida en inglés con buena disponibilidad de capítulos.",
                    "Fast English source with reliable chapter availability.",
                ),
                MangaProviders::Comizy => (
                    "en",
                    "Fuente complementaria en inglés con lanzamientos recientes.",
                    "Complementary English source for recent releases.",
                ),
            };

            ProviderOption {
                id: provider.to_string(),
                name: provider.short_name().to_string(),
                badge: provider.badge_label().to_string(),
                language_group: language_group.to_string(),
                description_es: description_es.to_string(),
                description_en: description_en.to_string(),
            }
        })
        .collect()
}

pub fn detect_library_in_paths(db_path: &Path, downloads_dir: &Path) -> LibraryDetection {
    let (stats, offline_series_count) = if db_path.is_file() {
        match rusqlite::Connection::open(db_path) {
            Ok(conn) => {
                let db = Database::new(&conn);
                let stats = db.get_home_stats().unwrap_or_default();
                let offline_count = scan_offline_mangas_in_dir(downloads_dir, Some(&db)).len();
                (stats, offline_count)
            },
            Err(_) => (HomeStats::default(), scan_offline_mangas_in_dir(downloads_dir, None).len()),
        }
    } else {
        (HomeStats::default(), scan_offline_mangas_in_dir(downloads_dir, None).len())
    };

    let has_existing_data = stats.library_mangas > 0
        || stats.chapters_read > 0
        || stats.total_pages_read > 0
        || stats.favorites > 0
        || stats.downloaded_chapters > 0
        || offline_series_count > 0;

    LibraryDetection {
        has_existing_data,
        library_mangas: stats.library_mangas,
        chapters_read: stats.chapters_read,
        total_pages_read: stats.total_pages_read,
        favorites: stats.favorites,
        downloaded_chapters: stats.downloaded_chapters,
        offline_series_count,
        last_read_title: stats.last_read_title,
    }
}

fn build_initial_state(inner: &AppStateInner) -> InitialAppState {
    let recommended = recommended_provider_for_language(inner.ui_language);
    let library = detect_library_in_paths(&inner.db_path, &inner.downloads_dir);
    let (platform, is_mobile, can_open_data_folder) = platform_capabilities(std::env::consts::OS);

    InitialAppState {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ui_language: ui_lang_toml_value(inner.ui_language).to_string(),
        theme_mode: inner.prefs.theme_mode,
        platform: platform.to_string(),
        is_mobile,
        can_open_data_folder,
        storage_mode: storage_mode_id(inner.storage_mode).to_string(),
        storage_mode_label: inner.storage_mode.label().to_string(),
        data_dir: inner.data_dir.display().to_string(),
        config_dir: inner.config_dir.display().to_string(),
        downloads_dir: inner.downloads_dir.display().to_string(),
        active_provider: inner.default_provider.to_string(),
        recommended_provider: recommended.to_string(),
        available_providers: available_providers(),
        favorite_genres: inner.prefs.favorite_genres.clone(),
        onboarding_completed: inner.prefs.onboarding_completed,
        download_format: download_type_toml_value(active_download_type()).to_string(),
        reader_prefs: inner.prefs.reader_prefs.clone(),
        support_banner_dismissed: inner.prefs.support_banner_dismissed,
        library,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use manga_core::backend::database::{ChapterToSaveHistory, MangaReadingHistorySave};
    use manga_core::config::build_config_file_in_dir;
    use pretty_assertions::assert_eq;

    use super::*;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = std::env::temp_dir().join(format!("mangora_hito1_{name}_{nanos}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn make_test_state(base: &Path) -> AppState {
        let data_dir = base.join("data");
        let config_dir = base.join("config");
        let downloads_dir = data_dir.join("mangaDownloads");
        let db_path = data_dir.join("history").join("manga-tui-history.db");

        fs::create_dir_all(&downloads_dir).unwrap();
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        build_config_file_in_dir(&config_dir).unwrap();

        AppState::from_paths(
            data_dir,
            config_dir,
            downloads_dir,
            db_path,
            StorageMode::EnvOverride,
            UiLanguage::Es,
            MangaProviders::Inmanga,
            Languages::Spanish,
        )
    }

    #[test]
    fn recommended_provider_matches_ui_language() {
        assert_eq!(recommended_provider_for_language(UiLanguage::Es), MangaProviders::Inmanga);
        assert_eq!(recommended_provider_for_language(UiLanguage::En), MangaProviders::Weebcentral);
    }

    #[test]
    fn empty_storage_reports_no_existing_library_and_pending_onboarding() {
        let temp = TempTestDir::new("empty_storage");
        let state = make_test_state(&temp.path);

        let snapshot = state.snapshot().unwrap();
        assert!(!snapshot.onboarding_completed);
        assert_eq!(snapshot.theme_mode, ThemeMode::Paper);
        assert_eq!(snapshot.ui_language, "es");
        assert_eq!(snapshot.recommended_provider, "inmanga");
        assert!(!snapshot.library.has_existing_data);
        assert_eq!(snapshot.library.library_mangas, 0);
        assert_eq!(snapshot.library.offline_series_count, 0);
    }

    #[test]
    fn detects_existing_sqlite_library_and_counts() {
        let temp = TempTestDir::new("sqlite_library");
        let state = make_test_state(&temp.path);
        let db_path = temp.path.join("data").join("history").join("manga-tui-history.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let db = Database::new(&conn);
        db.setup().unwrap();
        db.save_history(MangaReadingHistorySave {
            id: "manga-001",
            title: "Berserk",
            img_url: Some("https://example.com/cover.jpg"),
            chapter: ChapterToSaveHistory {
                id: "ch-001",
                title: "Capítulo 1",
                translated_language: "es",
            },
            provider: MangaProviders::Inmanga,
        })
        .unwrap();
        db.toggle_favorite("manga-001", MangaProviders::Inmanga, "Berserk").unwrap();
        drop(conn);

        let snapshot = state.snapshot().unwrap();
        assert!(snapshot.library.has_existing_data);
        assert_eq!(snapshot.library.library_mangas, 1);
        assert_eq!(snapshot.library.chapters_read, 1);
        assert_eq!(snapshot.library.favorites, 1);
        assert_eq!(snapshot.library.last_read_title.as_deref(), Some("Berserk"));
    }

    #[test]
    fn detects_offline_cbz_library_even_without_db_rows() {
        let temp = TempTestDir::new("offline_cbz");
        let state = make_test_state(&temp.path);
        let series_dir = temp.path.join("data").join("mangaDownloads").join("Mi Serie");
        fs::create_dir_all(&series_dir).unwrap();
        fs::write(series_dir.join("Ch 1.cbz"), b"cbz-placeholder").unwrap();

        let snapshot = state.snapshot().unwrap();
        assert!(snapshot.library.has_existing_data);
        assert_eq!(snapshot.library.offline_series_count, 1);
    }

    #[test]
    fn save_onboarding_persists_to_prefs_json_and_shared_config_toml() {
        let temp = TempTestDir::new("onboarding_persist");
        let state = make_test_state(&temp.path);

        let saved = state
            .save_onboarding_preferences(OnboardingPreferencesPayload {
                ui_language: "en".to_string(),
                favorite_genres: vec![" Shonen ".to_string(), "SEINEN".to_string(), "shonen".to_string(), "".to_string()],
                provider: "weebcentral".to_string(),
                onboarding_completed: true,
                theme_mode: Some(ThemeMode::Ink),
            })
            .unwrap();

        assert!(saved.onboarding_completed);
        assert_eq!(saved.ui_language, "en");
        assert_eq!(saved.active_provider, "weebcentral");
        assert_eq!(saved.recommended_provider, "weebcentral");
        assert_eq!(saved.theme_mode, ThemeMode::Ink);
        assert_eq!(saved.favorite_genres, vec!["shonen".to_string(), "seinen".to_string()]);

        let config_dir = temp.path.join("config");
        let config_contents = fs::read_to_string(config_dir.join("config.toml")).unwrap();
        assert!(config_contents.contains(r#"ui_language = "en""#));
        assert!(config_contents.contains(r#"default_manga_provider = "weebcentral""#));
        assert!(config_contents.contains(r#"default_language = "en""#));
        assert!(config_contents.contains("# Language of the user interface"));
        assert!(config_contents.contains("[anilist]"));

        // Reload fresh AppState pointing at the same directories to confirm restart persistence
        let reloaded = AppState::from_paths(
            temp.path.join("data"),
            config_dir,
            temp.path.join("data").join("mangaDownloads"),
            temp.path.join("data").join("history").join("manga-tui-history.db"),
            StorageMode::EnvOverride,
            UiLanguage::En,
            MangaProviders::Weebcentral,
            Languages::English,
        );
        let after_restart = reloaded.snapshot().unwrap();
        assert!(after_restart.onboarding_completed);
        assert_eq!(after_restart.theme_mode, ThemeMode::Ink);
        assert_eq!(after_restart.favorite_genres, vec!["shonen".to_string(), "seinen".to_string()]);
    }

    #[test]
    fn can_relaunch_onboarding_and_toggle_theme() {
        let temp = TempTestDir::new("relaunch_tour");
        let state = make_test_state(&temp.path);

        state
            .save_onboarding_preferences(OnboardingPreferencesPayload {
                ui_language: "es".to_string(),
                favorite_genres: vec!["romance".to_string()],
                provider: "inmanga".to_string(),
                onboarding_completed: true,
                theme_mode: Some(ThemeMode::Ink),
            })
            .unwrap();

        let relaunched = state.set_onboarding_completed(false).unwrap();
        assert!(!relaunched.onboarding_completed);

        let toggled = state.set_theme_mode("paper").unwrap();
        assert_eq!(toggled.theme_mode, ThemeMode::Paper);

        let reloaded_prefs = load_prefs_from_dir(&temp.path.join("config"));
        assert!(!reloaded_prefs.onboarding_completed);
        assert_eq!(reloaded_prefs.theme_mode, ThemeMode::Paper);
    }

    #[test]
    fn saves_and_clamps_reader_prefs_across_restart() {
        let temp = TempTestDir::new("reader_prefs_persist");
        let state = make_test_state(&temp.path);

        let initial = state.get_reader_prefs().unwrap();
        assert_eq!(initial, ReaderPrefs::default());

        let updated = state
            .save_reader_prefs(ReaderPrefs {
                view_mode: ReaderViewMode::Double,
                direction: ReadingDirection::Ltr,
                fit_mode: PageFitMode::FitHeight,
                zoom_percent: 999,
                cascade_gap_px: 200,
                first_page_single: false,
                pin_toolbars: true,
                dark_canvas: false,
            })
            .unwrap();

        assert_eq!(updated.reader_prefs.view_mode, ReaderViewMode::Double);
        assert_eq!(updated.reader_prefs.direction, ReadingDirection::Ltr);
        assert_eq!(updated.reader_prefs.fit_mode, PageFitMode::FitHeight);
        assert_eq!(updated.reader_prefs.zoom_percent, 250);
        assert_eq!(updated.reader_prefs.cascade_gap_px, 48);
        assert!(!updated.reader_prefs.first_page_single);
        assert!(updated.reader_prefs.pin_toolbars);
        assert!(!updated.reader_prefs.dark_canvas);

        let reloaded_prefs = load_prefs_from_dir(&temp.path.join("config"));
        assert_eq!(reloaded_prefs.reader_prefs, updated.reader_prefs);
    }

    #[test]
    fn platform_capabilities_and_sandbox_storage_id_match_expected_targets() {
        assert_eq!(platform_capabilities("windows"), ("windows", false, true));
        assert_eq!(platform_capabilities("macos"), ("macos", false, true));
        assert_eq!(platform_capabilities("linux"), ("linux", false, true));
        assert_eq!(platform_capabilities("android"), ("android", true, false));
        assert_eq!(platform_capabilities("ios"), ("ios", true, false));
        assert_eq!(platform_capabilities("freebsd"), ("unknown", false, false));
        assert_eq!(storage_mode_id(StorageMode::AppSandbox), "sandbox");
    }

    #[test]
    fn library_overview_reads_continue_reading_favorites_and_filters_by_title() {
        let temp = TempTestDir::new("library_overview");
        let state = make_test_state(&temp.path);

        {
            let conn = state.open_db_connection().unwrap();
            let db = Database::new(&conn);
            db.save_history(MangaReadingHistorySave {
                id: "m-berserk",
                title: "Berserk",
                img_url: Some("https://example.com/berserk.jpg"),
                chapter: ChapterToSaveHistory {
                    id: "ch-1",
                    title: "Capítulo 1",
                    translated_language: "es",
                },
                provider: MangaProviders::Inmanga,
            })
            .unwrap();
            db.save_history(MangaReadingHistorySave {
                id: "m-monster",
                title: "Monster",
                img_url: Some("https://example.com/monster.jpg"),
                chapter: ChapterToSaveHistory {
                    id: "ch-2",
                    title: "Capítulo 4",
                    translated_language: "en",
                },
                provider: MangaProviders::Weebcentral,
            })
            .unwrap();
            db.toggle_favorite("m-berserk", MangaProviders::Inmanga, "Berserk").unwrap();
        }

        let overview = state.get_library_overview(None).unwrap();
        assert_eq!(overview.total_continue_reading, 2);
        assert_eq!(overview.total_recent, 2);
        assert_eq!(overview.total_favorites, 1);
        assert_eq!(overview.favorites[0].id, "m-berserk");
        assert_eq!(overview.favorites[0].cover_url.as_deref(), Some("https://example.com/berserk.jpg"));
        assert!(overview.favorites[0].is_favorite);

        let filtered = state.get_library_overview(Some("monst")).unwrap();
        assert_eq!(filtered.total_recent, 1);
        assert_eq!(filtered.recent_history[0].title, "Monster");
        assert_eq!(filtered.total_favorites, 0);
    }

    #[test]
    fn toggle_library_favorite_persists_bidirectionally_in_sqlite() {
        let temp = TempTestDir::new("toggle_favorite");
        let state = make_test_state(&temp.path);

        let added = state
            .toggle_library_favorite(ToggleFavoritePayload {
                manga_id: "m-frieren".to_string(),
                provider: "mangadex".to_string(),
                title: "Frieren".to_string(),
                cover_url: Some("https://example.com/frieren.jpg".to_string()),
            })
            .unwrap();

        assert!(added.is_favorite);
        assert_eq!(added.app_state.library.favorites, 1);
        assert_eq!(added.library.total_favorites, 1);
        assert_eq!(added.library.favorites[0].cover_url.as_deref(), Some("https://example.com/frieren.jpg"));

        {
            let conn = state.open_db_connection().unwrap();
            let db = Database::new(&conn);
            assert!(db.is_favorite("m-frieren", MangaProviders::Mangadex).unwrap());
        }

        let removed = state
            .toggle_library_favorite(ToggleFavoritePayload {
                manga_id: "m-frieren".to_string(),
                provider: "mangadex".to_string(),
                title: "Frieren".to_string(),
                cover_url: None,
            })
            .unwrap();

        assert!(!removed.is_favorite);
        assert_eq!(removed.app_state.library.favorites, 0);
        assert_eq!(removed.library.total_favorites, 0);
    }

    #[test]
    fn toggle_chapter_read_updates_shared_sqlite_and_library_overview() {
        let temp = TempTestDir::new("toggle_chapter_read");
        let state = make_test_state(&temp.path);

        let marked = state
            .toggle_chapter_read(ToggleChapterReadPayload {
                manga_id: "m-dandadan".to_string(),
                provider: "inmanga".to_string(),
                manga_title: "Dandadan".to_string(),
                cover_url: Some("https://example.com/dandadan.jpg".to_string()),
                chapter_id: "ch-001".to_string(),
                chapter_title: "Capítulo 1".to_string(),
                translated_language: "es".to_string(),
            })
            .unwrap();

        assert!(marked.is_read);
        assert_eq!(marked.app_state.library.chapters_read, 1);
        assert_eq!(marked.library.total_recent, 1);
        assert_eq!(marked.library.recent_history[0].total_read_count, 1);

        let unmarked = state
            .toggle_chapter_read(ToggleChapterReadPayload {
                manga_id: "m-dandadan".to_string(),
                provider: "inmanga".to_string(),
                manga_title: "Dandadan".to_string(),
                cover_url: None,
                chapter_id: "ch-001".to_string(),
                chapter_title: "Capítulo 1".to_string(),
                translated_language: "es".to_string(),
            })
            .unwrap();

        assert!(!unmarked.is_read);
        assert_eq!(unmarked.app_state.library.chapters_read, 0);
    }

    #[test]
    fn dismiss_support_banner_persists_across_restart() {
        let temp = TempTestDir::new("dismiss_support_banner");
        let state = make_test_state(&temp.path);
        assert!(!state.snapshot().unwrap().support_banner_dismissed);

        let updated = state.dismiss_support_banner().unwrap();
        assert!(updated.support_banner_dismissed);

        let reloaded = make_test_state(&temp.path);
        assert!(reloaded.snapshot().unwrap().support_banner_dismissed);
    }
}
