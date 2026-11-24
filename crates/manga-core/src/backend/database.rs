//! Database module for managing manga reading history, bookmarks, and user data persistence.
//!
//! This module provides the `Database` struct and related types for managing manga-related data in a SQLite database.
//! It handles various aspects of manga tracking including:
//!
//! - **Reading History**: Track which manga chapters have been read and when
//! - **Plan to Read**: Manage a list of manga that users plan to read later
//! - **Downloads**: Track which chapters have been downloaded for offline reading
//! - **Bookmarks**: Save reading progress within specific chapters
//! - **Multi-provider Support**: Handle manga from different sources (MangaDx, Weebcentral, etc.)
//!
//! ## Database Schema
//!
//! The database consists of several interconnected tables:
//! - `mangas`: Core manga information (id, title, cover image, provider)
//! - `chapters`: Chapter details with read/download/bookmark status
//! - `history_types`: Categories for organizing manga (ReadingHistory, PlanToRead)
//! - `manga_history_union`: Links manga to history categories
//!
//! ## Key Features
//!
//! ### History Management
//! - Separate tracking for "currently reading" vs "plan to read" manga
//! - Search and pagination support for large manga collections
//! - Provider-specific filtering (e.g., only show MangaDx manga)
//!
//! ### Chapter Tracking  
//! - Mark chapters as read/unread
//! - Track download status for offline reading
//! - Bookmark specific pages within chapters
//! - Only one bookmark per manga (automatically replaces previous bookmarks)
//!
//! ### Data Integrity
//! - Automatic creation of manga records when adding chapters
//! - Foreign key relationships ensure data consistency
//! - Graceful handling of duplicate entries
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! # use rusqlite::Connection;
//! # use crate::backend::database::{Database, MangaReadingHistorySave, ChapterToSaveHistory};
//! # use crate::backend::manga_provider::MangaProviders;
//! let conn = Connection::open("manga.db")?;
//! let db = Database::new(&conn);
//! db.setup()?;
//!
//! // Save reading progress
//! db.save_history(MangaReadingHistorySave {
//!     id: "manga_123",
//!     title: "One Piece",
//!     img_url: Some("cover.jpg"),
//!     chapter: ChapterToSaveHistory {
//!         id: "chapter_456",
//!         title: "Chapter 1000",
//!         translated_language: "en",
//!     },
//!     provider: MangaProviders::Mangadx,
//! })?;
//! ```
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{Duration, NaiveDate, Utc};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Connection, OptionalExtension, ToSql, params};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter};

use super::AppDirectories;
use super::manga_provider::{Languages, MangaProviders};
use crate::SearchTerm;

/// Represents the type of manga history list.
///
/// Manga can be categorized into different lists for user organization:
/// - `PlanToRead`: Manga the user intends to read in the future
/// - `ReadingHistory`: Manga the user has actually started reading
#[derive(Display, Debug, Clone, Copy)]
pub enum MangaHistoryType {
    PlanToRead,
    ReadingHistory,
}

impl ToSql for MangaHistoryType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let as_string = self.to_string();

        Ok(ToSqlOutput::from(as_string))
    }
}

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum Table {
    #[strum(to_string = "mangas")]
    Mangas,
    #[strum(to_string = "history_types")]
    HistoryTypes,
    #[strum(to_string = "chapters")]
    Chapters,
    #[strum(to_string = "manga_history_union")]
    MangaHistoryUnion,
}

#[derive(Clone)]
pub struct MangaInsert<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub img_url: Option<&'a str>,
    pub provider: MangaProviders,
}

#[derive(Debug, Default)]
pub struct ChapterToSaveHistory<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub translated_language: &'a str,
}

pub struct MangaReadingHistorySave<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub img_url: Option<&'a str>,
    pub chapter: ChapterToSaveHistory<'a>,
    pub provider: MangaProviders,
}

pub struct MangaReadingHistoryRetrieve {
    pub id: String,
    pub is_downloaded: bool,
    pub is_read: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MangaHistory {
    pub id: String,
    pub title: String,
    pub provider: MangaProviders,
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MangaHistoryResponse {
    pub mangas: Vec<MangaHistory>,
    pub page: u32,
    pub total_items: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedManga {
    pub id: String,
    pub title: String,
    pub downloaded_chapter_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedMangaWithProvider {
    pub id: String,
    pub title: String,
    pub provider: MangaProviders,
    pub downloaded_chapter_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteMangaRecord {
    pub manga_id: String,
    pub provider: MangaProviders,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineDbManga {
    pub id: String,
    pub title: String,
    pub provider: MangaProviders,
    pub is_favorite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineDbChapter {
    pub id: String,
    pub title: String,
    pub manga_id: String,
    pub is_read: bool,
    pub is_downloaded: bool,
    pub is_bookmarked: bool,
    pub translated_language: String,
    pub number_page_bookmarked: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OfflineCatalogMetadata {
    pub mangas: Vec<OfflineDbManga>,
    pub chapters: Vec<OfflineDbChapter>,
}

/// User statistics displayed in the Home page hero header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HomeStats {
    pub library_mangas: u32,
    pub chapters_read: u32,
    pub downloaded_chapters: u32,
    pub favorites: u32,
    pub last_read_title: Option<String>,
}

/// Comprehensive local reading statistics for the Reading Stats Dashboard modal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadingDashboardStats {
    pub total_chapters_read: u32,
    pub estimated_reading_minutes: u32,
    pub reading_mangas: u32,
    pub plan_to_read_mangas: u32,
    pub favorites_count: u32,
    pub shelves_count: u32,
    pub bookmarks_count: u32,
    pub downloaded_db_chapters: u32,
    pub active_days_count: u32,
    pub current_streak_days: u32,
    pub read_last_7_days: u32,
    pub top_read_mangas: Vec<(String, MangaProviders, u32)>,
    pub provider_breakdown: Vec<(MangaProviders, u32, u32)>,
}

/// Input payload for recording a 100 % local reading session in SQLite.
#[derive(Debug, Clone)]
pub struct ReadingSessionInput<'a> {
    pub manga_id: &'a str,
    pub provider: MangaProviders,
    pub chapter_id: &'a str,
    pub duration_seconds: u32,
    pub pages_read: u32,
    pub completed_chapter: bool,
    pub started_at: Option<&'a str>,
}

/// Reading metrics aggregated per genre for a user's local library.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenreReadingStat {
    pub genre: String,
    pub mangas_count: u32,
    pub chapters_read: u32,
    pub reading_minutes: u32,
    pub share_percent: f32,
}

/// Top read manga entry with local reading duration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopMangaStat {
    pub manga_id: String,
    pub title: String,
    pub provider: MangaProviders,
    pub chapters_read: u32,
    pub reading_minutes: u32,
}

/// Aggregated series and read-chapter count per manga provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReadingStat {
    pub provider: MangaProviders,
    pub mangas_count: u32,
    pub chapters_read: u32,
}

/// Daily reading activity bucket used for 14-day charts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyReadingActivity {
    pub date: String,
    pub chapters_read: u32,
    pub reading_minutes: u32,
    pub pages_read: u32,
}

/// Comprehensive 100 % device-local reader analytics (no external telemetry).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtendedReaderMetrics {
    pub total_chapters_read: u32,
    pub chapters_read_today: u32,
    pub chapters_read_last_7_days: u32,
    pub chapters_read_last_30_days: u32,
    pub total_pages_read: u32,
    pub recorded_reading_seconds: u64,
    pub estimated_total_reading_minutes: u32,
    pub average_minutes_per_chapter: f32,
    pub active_days_count: u32,
    pub current_streak_days: u32,
    pub longest_streak_days: u32,
    pub reading_mangas: u32,
    pub plan_to_read_mangas: u32,
    pub favorites_count: u32,
    pub shelves_count: u32,
    pub downloaded_chapters: u32,
    pub top_genres: Vec<GenreReadingStat>,
    pub top_read_mangas: Vec<TopMangaStat>,
    pub provider_breakdown: Vec<ProviderReadingStat>,
    pub daily_activity_last_14_days: Vec<DailyReadingActivity>,
    pub generated_at: String,
    pub privacy_local_only: bool,
}

/// Computes `(current_streak_days, longest_streak_days)` from active calendar dates.
pub fn compute_reading_streaks(sorted_unique_dates: &[NaiveDate], today: NaiveDate) -> (u32, u32) {
    if sorted_unique_dates.is_empty() {
        return (0, 0);
    }

    let mut dates: Vec<NaiveDate> = sorted_unique_dates.to_vec();
    dates.sort_unstable();
    dates.dedup();

    let mut longest: u32 = 0;
    let mut run: u32 = 0;
    let mut prev: Option<NaiveDate> = None;

    for &date in &dates {
        if let Some(p) = prev
            && date == p + Duration::days(1)
        {
            run += 1;
        } else {
            run = 1;
        }
        longest = longest.max(run);
        prev = Some(date);
    }

    let eligible_dates: Vec<NaiveDate> = dates.into_iter().filter(|&d| d <= today).collect();
    let current = match eligible_dates.last().copied() {
        Some(last_date) if last_date == today || last_date == today - Duration::days(1) => {
            let mut streak: u32 = 1;
            let mut cursor = last_date;
            for &d in eligible_dates.iter().rev().skip(1) {
                if d == cursor - Duration::days(1) {
                    streak += 1;
                    cursor = d;
                } else {
                    break;
                }
            }
            streak
        },
        _ => 0,
    };

    (current, longest)
}

pub const PRESET_SHELVES: [&str; 4] = ["🔥 Obras Maestras", "⏸ En Pausa", "✔ Completados", "💎 Pendiente Top"];

/// Arguments for retrieving manga history with filtering and pagination.
///
/// Used with `Database::get_history()` to specify exactly what manga to retrieve.
/// Supports complex queries with provider filtering, text search, and pagination.
#[derive(Debug)]
pub struct GetHistoryArgs {
    /// Which type of history to retrieve (ReadingHistory or PlanToRead)
    pub hist_type: MangaHistoryType,
    /// Page number (1-based) for pagination
    pub page: u32,
    /// Optional search term to filter by manga title
    pub search: Option<SearchTerm>,
    /// Number of items to return per page
    pub items_per_page: u32,
    /// Filter results to only this manga provider
    pub provider: MangaProviders,
}

/// This is used in the `feed` page to retrieve the mangas the user is currently reading
pub struct MangaPlanToReadSave<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub img_url: Option<&'a str>,
    pub provider: MangaProviders,
}

/// Information needed to mark a chapter as downloaded.
///
/// Used with `Database::set_chapter_downloaded()` when a user downloads
/// a chapter for offline reading. Contains both manga and chapter information
/// since the manga record may need to be created.
pub struct SetChapterDownloaded<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub manga_id: &'a str,
    pub manga_title: &'a str,
    pub img_url: Option<&'a str>,
    pub provider: MangaProviders,
}

/// Normalizes a chapter number string (e.g. `"01"`, `"1.0"` -> `"1"`, `"10.50"` -> `"10.5"`).
pub fn normalize_chapter_number(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(val) = trimmed.parse::<f64>()
        && val.is_finite()
    {
        if (val.fract()).abs() < 1e-6 {
            return format!("{}", val as i64);
        }
        return format!("{val}");
    }
    trimmed.to_string()
}

/// Extracts a normalized chapter number from a chapter title or identifier
/// (e.g. `"Capítulo 12.5"`, `"Chapter 05 - Start"`, `"Ch. 1"`, or `"12"`).
pub fn extract_normalized_chapter_number(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.parse::<f64>().is_ok() {
        return Some(normalize_chapter_number(trimmed));
    }

    let lower = trimmed.to_lowercase();
    for marker in ["capítulo", "capitulo", "chapter", "cap.", "cap ", "ch.", "ch ", "episodio", "ep."] {
        if let Some(pos) = lower.find(marker) {
            let after = &lower[pos + marker.len()..];
            let num_str: String = after
                .chars()
                .skip_while(|c| c.is_whitespace() || *c == ':' || *c == '-' || *c == '#')
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let num_str = num_str.trim_end_matches('.');
            if !num_str.is_empty() && num_str.parse::<f64>().is_ok() {
                return Some(normalize_chapter_number(num_str));
            }
        }
    }

    None
}

/// Database wrapper providing high-level operations for manga data management.
///
/// The `Database` struct encapsulates a SQLite connection and provides methods for:
/// - Managing reading history and "plan to read" lists
/// - Tracking chapter read/download status
/// - Handling bookmarks with page-level precision
/// - Multi-provider manga support
///
/// ## Lifecycle
///
/// 1. Create with `Database::new(&connection)`
/// 2. Initialize schema with `setup()`
/// 3. Use various methods to manage manga data
///
/// ## Thread Safety
///
/// This struct holds a reference to a SQLite connection. SQLite connections are not
/// thread-safe by default, so ensure proper synchronization if using across threads.
#[derive(Debug)]
pub struct Database<'a> {
    pub(super) connection: &'a Connection,
}

impl<'a> Database<'a> {
    /// Creates a new Database instance wrapping the provided SQLite connection.
    ///
    /// # Arguments
    ///
    /// * `conn` - Reference to an open SQLite connection
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use rusqlite::Connection;
    /// # use crate::backend::database::Database;
    /// let conn = Connection::open_in_memory()?;
    /// let db = Database::new(&conn);
    /// ```
    #[inline]
    pub fn new(conn: &'a Connection) -> Self {
        Self { connection: conn }
    }

    /// Initializes the database schema and default data.
    ///
    /// Creates all required tables if they don't exist:
    /// - `app_version`: Tracks application version
    /// - `mangas`: Core manga information
    /// - `chapters`: Chapter details and status
    /// - `history_types`: Categories (ReadingHistory, PlanToRead)
    /// - `manga_history_union`: Links manga to categories
    ///
    /// This method is idempotent - safe to call multiple times.
    ///
    /// # Errors
    ///
    /// Returns `rusqlite::Result<()>` on database errors.
    pub fn setup(&self) -> rusqlite::Result<()> {
        self.connection.execute(
            "CREATE TABLE if not exists app_version (
                version TEXT PRIMARY KEY
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists chapter_notification_dismissals (
                manga_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                chapter_id TEXT NOT NULL,
                PRIMARY KEY (manga_id, provider, chapter_id)
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists favorite_mangas (
                manga_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at DATETIME DEFAULT (datetime('now')),
                PRIMARY KEY (manga_id, provider)
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists manga_collections (
                manga_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                title TEXT NOT NULL,
                shelf_name TEXT NOT NULL,
                created_at DATETIME DEFAULT (datetime('now')),
                PRIMARY KEY (manga_id, provider)
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists search_history (
                query TEXT PRIMARY KEY,
                searched_at INTEGER NOT NULL
             )",
            (),
        )?;

        let already_has_data: i32 = self.connection.query_row("SELECT COUNT(*) from app_version", [], |row| row.get(0))?;

        if already_has_data == 0 {
            self.connection
                .execute("INSERT INTO app_version(version) VALUES (?1) ", [env!("CARGO_PKG_VERSION")])?;
        }

        self.connection.execute(
            "CREATE TABLE if not exists history_types (
                id    INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists mangas (
                id    TEXT  PRIMARY KEY,
                title TEXT  NOT NULL,
                created_at  DATETIME DEFAULT (datetime('now')),
                updated_at  DATETIME DEFAULT (datetime('now')),
                last_read  DATETIME DEFAULT (datetime('now')),
                deleted_at  DATETIME NULL,
                img_url TEXT NULL,
                manga_provider TEXT NOT NULL DEFAULT mangadex
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists chapters (
                id    TEXT  PRIMARY KEY,
                title TEXT  NOT NULL,
                manga_id TEXT  NOT NULL,
                is_read BOOLEAN NOT NULL DEFAULT 0,
                is_downloaded BOOLEAN NOT NULL DEFAULT 0,
                is_bookmarked BOOLEAN NOT NULL DEFAULT false,
                translated_language TEXT NULL,
                number_page_bookmarked INT NULL,
                FOREIGN KEY (manga_id) REFERENCES mangas (id)
            )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE if not exists manga_history_union (
                manga_id TEXT, 
                type_id INTEGER, 
                PRIMARY KEY (manga_id, type_id),
                FOREIGN KEY (manga_id) REFERENCES mangas (id),
                FOREIGN KEY (type_id) REFERENCES history_types (id)
             )",
            (),
        )?;

        let already_has_data: i32 = self.connection.query_row("SELECT COUNT(*) from history_types", [], |row| row.get(0))?;

        if already_has_data < 2 {
            self.connection
                .execute("INSERT INTO history_types(name) VALUES (?1) ", [MangaHistoryType::ReadingHistory.to_string()])?;

            self.connection
                .execute("INSERT INTO history_types(name) VALUES (?1) ", [MangaHistoryType::PlanToRead.to_string()])?;
        }

        self.connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_chapters_manga_lookup ON chapters(manga_id, is_read, is_downloaded, is_bookmarked)",
            (),
        )?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_mangas_provider_last_read ON mangas(manga_provider, last_read)", ())?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_favorite_mangas_provider ON favorite_mangas(provider, manga_id)", ())?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_manga_collections_shelf ON manga_collections(shelf_name, provider)", ())?;

        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS reading_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                manga_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                chapter_id TEXT NOT NULL,
                started_at DATETIME DEFAULT (datetime('now')),
                duration_seconds INTEGER NOT NULL DEFAULT 0,
                pages_read INTEGER NOT NULL DEFAULT 0,
                completed_chapter BOOLEAN NOT NULL DEFAULT 0
             )",
            (),
        )?;

        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS manga_genres (
                manga_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                genre TEXT NOT NULL,
                created_at DATETIME DEFAULT (datetime('now')),
                PRIMARY KEY (manga_id, provider, genre)
             )",
            (),
        )?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_reading_sessions_started_at ON reading_sessions(started_at)", ())?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_reading_sessions_manga ON reading_sessions(manga_id, provider)", ())?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_reading_sessions_chapter ON reading_sessions(chapter_id)", ())?;

        self.connection
            .execute("CREATE INDEX IF NOT EXISTS idx_manga_genres_genre ON manga_genres(genre, provider)", ())?;

        for statement in super::anime_history::SCHEMA {
            self.connection.execute(statement, [])?;
        }

        Ok(())
    }

    /// Gets a database connection, using in-memory for tests or file-based for production.
    ///
    /// # Returns
    ///
    /// - In test mode: Returns an in-memory SQLite connection
    /// - In production: Returns a file-based connection to the app's history database
    #[inline]
    pub fn get_connection() -> rusqlite::Result<Connection> {
        if cfg!(any(test, feature = "test-utils")) {
            let conn = Connection::open_in_memory()?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            Ok(conn)
        } else {
            let conn = Connection::open(AppDirectories::History.get_full_path())?;
            conn.execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA busy_timeout = 5000;
                PRAGMA foreign_keys = ON;
                PRAGMA synchronous = NORMAL;
            ",
            )?;
            Ok(conn)
        }
    }

    /// Checks if a chapter has already been marked as read.
    ///
    /// # Arguments
    ///
    /// * `id` - Chapter ID to check
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if chapter exists and is marked as read
    /// - `Ok(false)` if chapter doesn't exist or is not read
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # let db = setup_test_db();
    /// if !db.check_chapter_is_already_reading("chapter_123")? {
    ///     // Chapter not read yet, safe to mark as read
    ///     db.save_history(reading_data)?;
    /// }
    /// ```
    pub fn check_chapter_is_already_reading(&self, id: &str) -> rusqlite::Result<bool> {
        let exists = self.check_exists(id, Table::Chapters)?;

        if !exists {
            return Ok(false);
        }

        let is_read: bool = self
            .connection
            .query_row("SELECT is_read FROM chapters WHERE id = ?1", params![id], |row| row.get(0))?;

        Ok(is_read)
    }

    fn create_manga_if_not_exists(&self, manga: MangaInsert<'_>) -> rusqlite::Result<()> {
        if self.check_exists(manga.id, Table::Mangas)? {
            return Ok(());
        }

        self.connection
            .execute("INSERT INTO mangas(id, title, img_url, manga_provider) VALUES(?1, ?2, ?3, ?4)", params![
                manga.id,
                manga.title,
                manga.img_url,
                manga.provider.to_string()
            ])?;

        Ok(())
    }

    fn create_chapter_if_not_exists(&self, chap: ChapterToInsert<'_>) -> rusqlite::Result<()> {
        if self.check_exists(chap.id, Table::Chapters)? {
            return Ok(());
        }

        self.connection
            .execute("INSERT INTO chapters(id, title, manga_id, is_read, translated_language, number_page_bookmarked, is_downloaded, is_bookmarked) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![chap.id, chap.title, chap.manga_id, chap.is_read, chap.translated_language, chap.number_page_bookmarked, chap.is_downloaded, chap.is_bookmarked])?;

        Ok(())
    }

    fn bookmark_chapter(&mut self, chapter_to_bookmark: ChapterToBookmark<'_>) -> rusqlite::Result<()> {
        self.create_manga_if_not_exists(MangaInsert {
            id: chapter_to_bookmark.manga_id,
            title: chapter_to_bookmark.manga_title,
            img_url: chapter_to_bookmark.manga_cover_url,
            provider: chapter_to_bookmark.provider,
        })?;

        self.create_chapter_if_not_exists(ChapterToInsert {
            id: chapter_to_bookmark.chapter_id,
            title: chapter_to_bookmark.chapter_title,
            manga_id: chapter_to_bookmark.manga_id,
            is_read: false,
            is_downloaded: false,
            is_bookmarked: true,
            number_page_bookmarked: chapter_to_bookmark.page_number,
            translated_language: chapter_to_bookmark.translated_language.as_iso_code(),
        })?;

        self.connection
            .execute("UPDATE chapters SET is_bookmarked = false WHERE manga_id = ?1", [chapter_to_bookmark.manga_id])?;

        self.connection
            .execute("UPDATE chapters SET is_bookmarked = true, number_page_bookmarked = ?1 WHERE id = ?2", params![
                chapter_to_bookmark.page_number,
                chapter_to_bookmark.chapter_id
            ])?;

        Ok(())
    }

    fn get_chapter_bookmarked(&self, manga_id: &str) -> rusqlite::Result<Option<ChapterBookmarked>> {
        let query = r"
        SELECT chapters.id, chapters.translated_language, chapters.number_page_bookmarked, mangas.title, mangas.id 

        FROM chapters INNER JOIN mangas ON chapters.manga_id = mangas.id

        WHERE manga_id = ?1 AND is_bookmarked = true
        ";

        self.connection
            .query_row(query, params![manga_id], |row| {
                let chapter: ChapterBookmarked = ChapterBookmarked {
                    id: row.get(0)?,
                    translated_language: row.get(1)?,
                    number_page_bookmarked: row.get(2)?,
                    manga_title: row.get(3)?,
                    manga_id: row.get(4)?,
                };

                Ok(chapter)
            })
            .optional()
    }

    fn get_history_type(&self, hist_type: MangaHistoryType) -> rusqlite::Result<i32> {
        let history_type_id: i32 =
            self.connection
                .query_row("SELECT id FROM history_types where name = ?1", params![hist_type.to_string()], |row| row.get(0))?;
        Ok(history_type_id)
    }

    fn manga_is_plan_to_read(&self, manga_id: &str) -> rusqlite::Result<bool> {
        let history_type = self.get_history_type(MangaHistoryType::PlanToRead)?;
        let is_already_plan_to_read: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT * FROM manga_history_union WHERE manga_id = ?1 AND type_id = ?2)",
            params![manga_id, history_type],
            |row| row.get(0),
        )?;

        Ok(is_already_plan_to_read)
    }

    /// check if a value exists in a table
    fn check_exists(&self, id: &str, table: Table) -> rusqlite::Result<bool> {
        let table = table.to_string();
        let exists: bool = self.connection.query_row(
            format!("SELECT EXISTS(SELECT id FROM {table} WHERE id = ?1) as row_exists").as_str(),
            rusqlite::params![id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn manga_is_reading(&self, id: &str) -> rusqlite::Result<bool> {
        let history_type: i32 = self.connection.query_row(
            "SELECT id FROM history_types where name = ?1",
            params![MangaHistoryType::ReadingHistory.to_string()],
            |row| row.get(0),
        )?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT * FROM manga_history_union WHERE manga_id = ?1 AND type_id = ?2)",
            rusqlite::params![id, history_type],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    fn insert_manga_in_reading_history(&self, manga_id: &str) -> rusqlite::Result<()> {
        let reading_history: i32 = self.connection.query_row(
            "SELECT id FROM history_types where name = ?1",
            params![MangaHistoryType::ReadingHistory.to_string()],
            |row| row.get(0),
        )?;

        self.connection
            .execute("INSERT INTO manga_history_union VALUES (?1, ?2)", (manga_id, reading_history))?;
        Ok(())
    }

    /// Insert a manga in the reading history type or update the `last_read` field
    fn update_or_insert_manga_in_most_recent_reading_history(&self, manga_id: &str) -> rusqlite::Result<()> {
        if !self.manga_is_reading(manga_id)? {
            self.insert_manga_in_reading_history(manga_id)?;
        }
        let now = Utc::now().naive_utc();
        self.connection
            .execute("UPDATE mangas SET last_read = ?1 WHERE id = ?2", params![now.to_string(), manga_id])?;
        Ok(())
    }

    /// Marks a chapter as downloaded and read.
    ///
    /// This method handles the complete flow for downloaded chapters:
    /// 1. Creates manga record if it doesn't exist
    /// 2. Adds chapter to reading history
    /// 3. Marks chapter as both downloaded and read
    ///
    /// Used when a user downloads a chapter for offline reading.
    ///
    /// # Arguments
    ///
    /// * `chapter` - Chapter download information
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use crate::backend::database::SetChapterDownloaded;
    /// db.set_chapter_downloaded(SetChapterDownloaded {
    ///     id: "chapter_123",
    ///     title: "Chapter 1",
    ///     manga_id: "manga_456",
    ///     manga_title: "One Piece",
    ///     img_url: Some("cover.jpg"),
    ///     provider: MangaProviders::Mangadx,
    /// })?;
    /// ```
    pub fn set_chapter_downloaded(&self, chapter: SetChapterDownloaded<'_>) -> rusqlite::Result<()> {
        if self.check_exists(chapter.manga_id, Table::Mangas)? {
            self.update_or_insert_manga_in_most_recent_reading_history(chapter.manga_id)?;

            if self.check_exists(chapter.id, Table::Chapters)? {
                self.connection
                    .execute("UPDATE chapters SET is_downloaded = ?1, is_read = ?2 WHERE id = ?3", params![
                        true, true, chapter.id
                    ])?;
            } else {
                self.create_chapter_if_not_exists(ChapterToInsert {
                    id: chapter.id,
                    title: chapter.title,
                    manga_id: chapter.manga_id,
                    is_read: true,
                    is_downloaded: true,
                    is_bookmarked: false,
                    number_page_bookmarked: None,
                    translated_language: "",
                })?;
            }

            Ok(())
        } else {
            self.create_manga_if_not_exists(MangaInsert {
                id: chapter.manga_id,
                title: chapter.manga_title,
                img_url: chapter.img_url,
                provider: chapter.provider,
            })?;

            self.create_chapter_if_not_exists(ChapterToInsert {
                id: chapter.id,
                title: chapter.title,
                manga_id: chapter.manga_id,
                is_read: true,
                is_downloaded: true,
                is_bookmarked: false,
                number_page_bookmarked: None,
                translated_language: "",
            })?;

            self.update_or_insert_manga_in_most_recent_reading_history(chapter.manga_id)?;

            Ok(())
        }
    }

    /// Records that a user has read a chapter, adding it to reading history.
    ///
    /// This is the main method for tracking reading progress. It:
    /// 1. Creates manga and chapter records if they don't exist
    /// 2. Adds manga to reading history (if not already there)
    /// 3. Marks the specific chapter as read
    /// 4. Updates the manga's last_read timestamp
    ///
    /// If the chapter is already marked as read, this method does nothing.
    ///
    /// # Arguments
    ///
    /// * `data` - Reading history data including manga and chapter info
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use crate::backend::database::{MangaReadingHistorySave, ChapterToSaveHistory};
    /// db.save_history(MangaReadingHistorySave {
    ///     id: "manga_123",
    ///     title: "Attack on Titan",
    ///     img_url: Some("cover.jpg"),
    ///     chapter: ChapterToSaveHistory {
    ///         id: "chapter_456",
    ///         title: "The Final Chapter",
    ///         translated_language: "en",
    ///     },
    ///     provider: MangaProviders::Mangadx,
    /// })?;
    /// ```
    pub fn save_history(&self, data: MangaReadingHistorySave<'_>) -> rusqlite::Result<()> {
        if self.check_chapter_is_already_reading(data.chapter.id)? {
            if self.check_exists(data.id, Table::Mangas)? {
                self.update_or_insert_manga_in_most_recent_reading_history(data.id)?;
            }
            return Ok(());
        }

        self.create_manga_if_not_exists(MangaInsert {
            id: data.id,
            title: data.title,
            img_url: data.img_url,
            provider: data.provider,
        })?;

        self.create_chapter_if_not_exists(ChapterToInsert {
            id: data.chapter.id,
            title: data.chapter.title,
            manga_id: data.id,
            is_read: false,
            is_downloaded: false,
            is_bookmarked: false,
            translated_language: data.chapter.translated_language,
            number_page_bookmarked: None,
        })?;

        self.update_or_insert_manga_in_most_recent_reading_history(data.id)?;

        self.connection
            .execute("UPDATE chapters SET is_read = true WHERE id = ?1", params![data.chapter.id])?;
        Ok(())
    }

    /// Toggles the `is_read` status of a chapter in the database.
    /// Returns `Ok(true)` if the chapter is now marked as read, or `Ok(false)` if it is now unread.
    pub fn toggle_chapter_read(&self, data: MangaReadingHistorySave<'_>) -> rusqlite::Result<bool> {
        if self.check_chapter_is_already_reading(data.chapter.id)? {
            self.connection
                .execute("UPDATE chapters SET is_read = false WHERE id = ?1", params![data.chapter.id])?;
            Ok(false)
        } else {
            self.save_history(data)?;
            Ok(true)
        }
    }

    /// Adds a manga to the "Plan to Read" list.
    ///
    /// This method allows users to bookmark manga they want to read later.
    /// If the manga is already in the plan to read list, this method does nothing.
    ///
    /// # Arguments
    ///
    /// * `manga` - Manga information to add to plan to read
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use crate::backend::database::MangaPlanToReadSave;
    /// db.save_plan_to_read(MangaPlanToReadSave {
    ///     id: "manga_789",
    ///     title: "Demon Slayer",
    ///     img_url: Some("cover.jpg"),
    ///     provider: MangaProviders::Mangadx,
    /// })?;
    /// ```
    pub fn save_plan_to_read(&self, manga: MangaPlanToReadSave<'_>) -> rusqlite::Result<()> {
        let history_type = self.get_history_type(MangaHistoryType::PlanToRead)?;

        if !self.manga_is_plan_to_read(manga.id)? {
            if self.check_exists(manga.id, Table::Mangas)? {
                self.connection
                    .execute("INSERT INTO manga_history_union VALUES (?1, ?2)", (manga.id, history_type))?;
                return Ok(());
            }

            self.create_manga_if_not_exists(MangaInsert {
                id: manga.id,
                title: manga.title,
                img_url: manga.img_url,
                provider: manga.provider,
            })?;

            self.connection
                .execute("INSERT INTO manga_history_union VALUES (?1, ?2)", (manga.id, history_type))?;
        }
        Ok(())
    }

    /// Retrieves the read/download status of all chapters for a specific manga.
    ///
    /// Returns a list of all chapters associated with the manga, along with their
    /// current read and download status. Useful for displaying chapter lists in the UI.
    ///
    /// # Arguments
    ///
    /// * `manga_id` - ID of the manga to get chapter status for
    ///
    /// # Returns
    ///
    /// Vector of `MangaReadingHistoryRetrieve` containing chapter IDs and their status.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let chapters = db.get_chapters_history_status("manga_123")?;
    /// for chapter in chapters {
    ///     println!(
    ///         "Chapter {}: read={}, downloaded={}",
    ///         chapter.id, chapter.is_read, chapter.is_downloaded
    ///     );
    /// }
    /// ```
    pub fn get_chapters_history_status(&self, manga_id: &str) -> rusqlite::Result<Vec<MangaReadingHistoryRetrieve>> {
        let mut chapter_ids: Vec<MangaReadingHistoryRetrieve> = vec![];

        let mut result = self.connection
        .prepare("SELECT chapters.id, chapters.is_downloaded, chapters.is_read from chapters INNER JOIN mangas ON mangas.id = chapters.manga_id WHERE mangas.id = ?1")?;

        let result_iter = result.query_map(params![manga_id], |row| {
            Ok(MangaReadingHistoryRetrieve {
                id: row.get(0)?,
                is_downloaded: row.get(1)?,
                is_read: row.get(2)?,
            })
        })?;

        for chapter_id in result_iter.flatten() {
            chapter_ids.push(chapter_id);
        }

        Ok(chapter_ids)
    }

    /// Retrieves normalized chapter numbers marked as read across any provider for mangas sharing the same title.
    /// Useful when migrating or switching providers for the same manga so reading progress is preserved.
    pub fn get_read_chapter_numbers_by_manga_title(
        &self,
        manga_title: &str,
    ) -> rusqlite::Result<std::collections::HashSet<String>> {
        let trimmed = manga_title.trim();
        if trimmed.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let mut stmt = self.connection.prepare(
            "SELECT chapters.title, chapters.id
             FROM chapters
             INNER JOIN mangas ON mangas.id = chapters.manga_id
             WHERE LOWER(TRIM(mangas.title)) = LOWER(TRIM(?1)) AND chapters.is_read = 1",
        )?;

        let rows = stmt.query_map(params![trimmed], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;

        let mut numbers = std::collections::HashSet::new();
        for (title, id) in rows.flatten() {
            if let Some(num) = extract_normalized_chapter_number(&title).or_else(|| extract_normalized_chapter_number(&id)) {
                numbers.insert(num);
            }
        }

        Ok(numbers)
    }

    pub fn get_downloaded_mangas(&self, provider: MangaProviders) -> rusqlite::Result<Vec<DownloadedManga>> {
        let mut statement = self.connection.prepare(
            "SELECT mangas.id, mangas.title, chapters.id
             FROM mangas
             INNER JOIN chapters ON chapters.manga_id = mangas.id
             WHERE mangas.manga_provider = ?1 AND chapters.is_downloaded = 1
             ORDER BY mangas.id",
        )?;

        let rows = statement.query_map(params![provider.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?;
        let mut mangas = BTreeMap::<String, DownloadedManga>::new();

        for row in rows {
            let (id, title, chapter_id) = row?;
            mangas
                .entry(id.clone())
                .or_insert_with(|| DownloadedManga {
                    id,
                    title,
                    downloaded_chapter_ids: Vec::new(),
                })
                .downloaded_chapter_ids
                .push(chapter_id);
        }

        Ok(mangas.into_values().collect())
    }

    pub fn get_all_downloaded_mangas(&self) -> rusqlite::Result<Vec<DownloadedMangaWithProvider>> {
        let mut statement = self.connection.prepare(
            "SELECT mangas.id, mangas.title, mangas.manga_provider, chapters.id
             FROM mangas
             INNER JOIN chapters ON chapters.manga_id = mangas.id
             WHERE chapters.is_downloaded = 1
             ORDER BY mangas.manga_provider, mangas.id",
        )?;

        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?;
        let mut mangas = BTreeMap::<(MangaProviders, String), DownloadedMangaWithProvider>::new();

        for row in rows {
            let (id, title, provider_str, chapter_id) = row?;
            let Ok(provider) = provider_str.parse::<MangaProviders>() else {
                continue;
            };
            mangas
                .entry((provider, id.clone()))
                .or_insert_with(|| DownloadedMangaWithProvider {
                    id,
                    title,
                    provider,
                    downloaded_chapter_ids: Vec::new(),
                })
                .downloaded_chapter_ids
                .push(chapter_id);
        }

        Ok(mangas.into_values().collect())
    }

    pub fn get_offline_catalog_metadata(&self) -> rusqlite::Result<OfflineCatalogMetadata> {
        let mut fav_set = std::collections::HashSet::new();
        {
            let mut fav_stmt = self.connection.prepare("SELECT manga_id, provider FROM favorite_mangas")?;
            let fav_rows = fav_stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
            for row in fav_rows {
                let (manga_id, provider_str) = row?;
                if let Ok(provider) = provider_str.parse::<MangaProviders>() {
                    fav_set.insert((provider, manga_id));
                }
            }
        }

        let mut mangas = Vec::new();
        {
            let mut manga_stmt = self
                .connection
                .prepare("SELECT id, title, manga_provider FROM mangas WHERE deleted_at IS NULL")?;
            let manga_rows = manga_stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?;
            for row in manga_rows {
                let (id, title, provider_str) = row?;
                let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
                let is_favorite = fav_set.contains(&(provider, id.clone()));
                mangas.push(OfflineDbManga {
                    id,
                    title,
                    provider,
                    is_favorite,
                });
            }
        }

        let mut chapters = Vec::new();
        {
            let mut ch_stmt = self.connection.prepare(
                "SELECT id, title, manga_id, is_read, is_downloaded, is_bookmarked,
                        COALESCE(translated_language, ''), number_page_bookmarked
                 FROM chapters",
            )?;
            let ch_rows = ch_stmt.query_map([], |row| {
                Ok(OfflineDbChapter {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    manga_id: row.get(2)?,
                    is_read: row.get(3)?,
                    is_downloaded: row.get(4)?,
                    is_bookmarked: row.get(5)?,
                    translated_language: row.get(6)?,
                    number_page_bookmarked: row.get(7)?,
                })
            })?;
            for row in ch_rows {
                chapters.push(row?);
            }
        }

        Ok(OfflineCatalogMetadata { mangas, chapters })
    }

    /// Exports the complete library, reading progress, bookmarks, favorites,
    /// dismissed notifications, and search history as a portable `LibrarySyncSnapshot`.
    pub fn export_library_sync_snapshot(&self) -> rusqlite::Result<crate::backend::cloud::LibrarySyncSnapshot> {
        use std::collections::HashSet;

        use crate::backend::cloud::{
            LIBRARY_SYNC_SCHEMA_VERSION, LibrarySyncSnapshot, SyncChapterRecord, SyncCollectionRecord, SyncDismissedRecord,
            SyncFavoriteRecord, SyncMangaGenreRecord, SyncMangaRecord, SyncSearchRecord,
        };

        let reading_type_id = self.get_history_type(MangaHistoryType::ReadingHistory)?;
        let plan_type_id = self.get_history_type(MangaHistoryType::PlanToRead)?;

        let mut reading_set: HashSet<String> = HashSet::new();
        let mut plan_set: HashSet<String> = HashSet::new();
        {
            let mut stmt = self.connection.prepare("SELECT manga_id, type_id FROM manga_history_union")?;
            let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)))?;
            for row in rows {
                let (manga_id, type_id) = row?;
                if type_id == reading_type_id {
                    reading_set.insert(manga_id);
                } else if type_id == plan_type_id {
                    plan_set.insert(manga_id);
                }
            }
        }

        let mut mangas = Vec::new();
        {
            let mut stmt = self.connection.prepare(
                "SELECT id, title, created_at, updated_at, last_read, deleted_at, img_url, manga_provider
                 FROM mangas ORDER BY id",
            )?;
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(SyncMangaRecord {
                    in_reading_history: reading_set.contains(&id),
                    in_plan_to_read: plan_set.contains(&id),
                    id,
                    title: row.get(1)?,
                    created_at: row.get(2).ok(),
                    updated_at: row.get(3).ok(),
                    last_read: row.get(4).ok(),
                    deleted_at: row.get(5).ok(),
                    img_url: row.get(6).ok(),
                    manga_provider: row.get(7)?,
                })
            })?;
            for r in rows {
                mangas.push(r?);
            }
        }

        let mut chapters = Vec::new();
        {
            let mut stmt = self.connection.prepare(
                "SELECT id, title, manga_id, is_read, is_downloaded, is_bookmarked, translated_language, number_page_bookmarked
                 FROM chapters ORDER BY id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SyncChapterRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    manga_id: row.get(2)?,
                    is_read: row.get(3)?,
                    is_downloaded: row.get(4)?,
                    is_bookmarked: row.get(5)?,
                    translated_language: row.get(6).ok(),
                    number_page_bookmarked: row.get(7).ok(),
                })
            })?;
            for r in rows {
                chapters.push(r?);
            }
        }

        let mut favorites = Vec::new();
        {
            let mut stmt = self
                .connection
                .prepare("SELECT manga_id, provider, title, created_at FROM favorite_mangas ORDER BY manga_id, provider")?;
            let rows = stmt.query_map([], |row| {
                Ok(SyncFavoriteRecord {
                    manga_id: row.get(0)?,
                    provider: row.get(1)?,
                    title: row.get(2)?,
                    created_at: row.get(3).ok(),
                })
            })?;
            for r in rows {
                favorites.push(r?);
            }
        }

        let mut dismissed_notifications = Vec::new();
        {
            let mut stmt = self.connection.prepare(
                "SELECT manga_id, provider, chapter_id FROM chapter_notification_dismissals ORDER BY manga_id, provider, chapter_id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SyncDismissedRecord {
                    manga_id: row.get(0)?,
                    provider: row.get(1)?,
                    chapter_id: row.get(2)?,
                })
            })?;
            for r in rows {
                dismissed_notifications.push(r?);
            }
        }

        let mut search_history = Vec::new();
        {
            let mut stmt = self
                .connection
                .prepare("SELECT query, searched_at FROM search_history ORDER BY searched_at DESC")?;
            let rows = stmt.query_map([], |row| {
                Ok(SyncSearchRecord {
                    query: row.get(0)?,
                    searched_at: row.get(1)?,
                })
            })?;
            for r in rows {
                search_history.push(r?);
            }
        }

        let mut collections = Vec::new();
        if let Ok(mut stmt) = self
            .connection
            .prepare("SELECT manga_id, provider, title, shelf_name, created_at FROM manga_collections ORDER BY manga_id, provider")
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok(SyncCollectionRecord {
                    manga_id: row.get(0)?,
                    provider: row.get(1)?,
                    title: row.get(2)?,
                    shelf_name: row.get(3)?,
                    created_at: row.get(4).ok(),
                })
            })
        {
            for r in rows.flatten() {
                collections.push(r);
            }
        }

        let mut manga_genres = Vec::new();
        if let Ok(mut stmt) = self
            .connection
            .prepare("SELECT manga_id, provider, genre, created_at FROM manga_genres ORDER BY manga_id, provider, genre")
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok(SyncMangaGenreRecord {
                    manga_id: row.get(0)?,
                    provider: row.get(1)?,
                    genre: row.get(2)?,
                    created_at: row.get(3).ok(),
                })
            })
        {
            for r in rows.flatten() {
                manga_genres.push(r);
            }
        }

        Ok(LibrarySyncSnapshot {
            schema_version: LIBRARY_SYNC_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            synced_at: Utc::now().to_rfc3339(),
            mangas,
            chapters,
            favorites,
            dismissed_notifications,
            search_history,
            collections,
            manga_genres,
        })
    }

    /// Merges a remote `LibrarySyncSnapshot` with the local database and applies the merged state atomically.
    pub fn import_and_merge_library_sync_snapshot(
        &self,
        remote: &crate::backend::cloud::LibrarySyncSnapshot,
    ) -> rusqlite::Result<crate::backend::cloud::LibrarySyncSnapshot> {
        let local = self.export_library_sync_snapshot()?;
        let merged = crate::backend::cloud::merge_library_snapshots(&local, remote);

        let reading_type_id = self.get_history_type(MangaHistoryType::ReadingHistory)?;
        let plan_type_id = self.get_history_type(MangaHistoryType::PlanToRead)?;

        let tx = self.connection.unchecked_transaction()?;

        for m in &merged.mangas {
            tx.execute(
                "INSERT INTO mangas (id, title, created_at, updated_at, last_read, deleted_at, img_url, manga_provider)
                 VALUES (?1, ?2, COALESCE(?3, datetime('now')), COALESCE(?4, datetime('now')), COALESCE(?5, datetime('now')), ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    created_at = COALESCE(excluded.created_at, mangas.created_at),
                    updated_at = COALESCE(excluded.updated_at, mangas.updated_at),
                    last_read = COALESCE(excluded.last_read, mangas.last_read),
                    deleted_at = excluded.deleted_at,
                    img_url = COALESCE(excluded.img_url, mangas.img_url),
                    manga_provider = excluded.manga_provider",
                params![
                    m.id,
                    m.title,
                    m.created_at,
                    m.updated_at,
                    m.last_read,
                    m.deleted_at,
                    m.img_url,
                    m.manga_provider
                ],
            )?;

            if m.in_reading_history {
                tx.execute("INSERT OR IGNORE INTO manga_history_union (manga_id, type_id) VALUES (?1, ?2)", params![
                    m.id,
                    reading_type_id
                ])?;
            }
            if m.in_plan_to_read {
                tx.execute("INSERT OR IGNORE INTO manga_history_union (manga_id, type_id) VALUES (?1, ?2)", params![
                    m.id,
                    plan_type_id
                ])?;
            }
        }

        tx.execute("UPDATE chapters SET is_bookmarked = 0", [])?;

        for ch in &merged.chapters {
            tx.execute(
                "INSERT INTO chapters (id, title, manga_id, is_read, is_downloaded, is_bookmarked, translated_language, number_page_bookmarked)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    is_read = excluded.is_read,
                    is_downloaded = excluded.is_downloaded,
                    is_bookmarked = excluded.is_bookmarked,
                    translated_language = COALESCE(excluded.translated_language, chapters.translated_language),
                    number_page_bookmarked = excluded.number_page_bookmarked",
                params![
                    ch.id,
                    ch.title,
                    ch.manga_id,
                    ch.is_read,
                    ch.is_downloaded,
                    ch.is_bookmarked,
                    ch.translated_language,
                    ch.number_page_bookmarked
                ],
            )?;
        }

        for fav in &merged.favorites {
            tx.execute(
                "INSERT OR REPLACE INTO favorite_mangas (manga_id, provider, title, created_at)
                 VALUES (?1, ?2, ?3, COALESCE(?4, datetime('now')))",
                params![fav.manga_id, fav.provider, fav.title, fav.created_at],
            )?;
        }

        for d in &merged.dismissed_notifications {
            tx.execute(
                "INSERT OR IGNORE INTO chapter_notification_dismissals (manga_id, provider, chapter_id)
                 VALUES (?1, ?2, ?3)",
                params![d.manga_id, d.provider, d.chapter_id],
            )?;
        }

        for s in &merged.search_history {
            tx.execute("INSERT OR REPLACE INTO search_history (query, searched_at) VALUES (?1, ?2)", params![
                s.query,
                s.searched_at
            ])?;
        }

        for col in &merged.collections {
            if !col.shelf_name.trim().is_empty() {
                let _ = tx.execute(
                    "INSERT OR REPLACE INTO manga_collections (manga_id, provider, title, shelf_name, created_at)
                     VALUES (?1, ?2, ?3, ?4, COALESCE(?5, datetime('now')))",
                    params![col.manga_id, col.provider, col.title, col.shelf_name, col.created_at],
                );
            }
        }

        for genre in &merged.manga_genres {
            let trimmed = genre.genre.trim();
            if !trimmed.is_empty() {
                let _ = tx.execute(
                    "INSERT INTO manga_genres (manga_id, provider, genre, created_at)
                     SELECT ?1, ?2, ?3, COALESCE(?4, datetime('now'))
                     WHERE NOT EXISTS (
                         SELECT 1 FROM manga_genres
                         WHERE manga_id = ?1 AND provider = ?2 AND lower(genre) = lower(?3)
                     )",
                    params![genre.manga_id, genre.provider, trimmed, genre.created_at],
                );
            }
        }

        tx.commit()?;
        Ok(merged)
    }

    pub fn get_dismissed_chapter_ids(&self, manga_id: &str, provider: MangaProviders) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT chapter_id FROM chapter_notification_dismissals
             WHERE manga_id = ?1 AND provider = ?2",
        )?;
        let rows = statement.query_map(params![manga_id, provider.to_string()], |row| row.get(0))?;
        rows.collect()
    }

    pub fn save_dismissed_chapter_ids(
        &self,
        manga_id: &str,
        provider: MangaProviders,
        chapter_ids: &[String],
    ) -> rusqlite::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for chapter_id in chapter_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO chapter_notification_dismissals(manga_id, provider, chapter_id)
                 VALUES (?1, ?2, ?3)",
                params![manga_id, provider.to_string(), chapter_id],
            )?;
        }

        transaction.commit()
    }

    pub fn is_favorite(&self, manga_id: &str, provider: MangaProviders) -> rusqlite::Result<bool> {
        let count: i32 = self.connection.query_row(
            "SELECT COUNT(*) FROM favorite_mangas WHERE manga_id = ?1 AND provider = ?2",
            params![manga_id, provider.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn toggle_favorite(&self, manga_id: &str, provider: MangaProviders, title: &str) -> rusqlite::Result<bool> {
        let is_fav = self.is_favorite(manga_id, provider)?;
        if is_fav {
            self.connection
                .execute("DELETE FROM favorite_mangas WHERE manga_id = ?1 AND provider = ?2", params![
                    manga_id,
                    provider.to_string()
                ])?;
            Ok(false)
        } else {
            self.connection.execute(
                "INSERT OR REPLACE INTO favorite_mangas (manga_id, provider, title) VALUES (?1, ?2, ?3)",
                params![manga_id, provider.to_string(), title],
            )?;
            Ok(true)
        }
    }

    pub fn get_favorite_manga_ids(&self, provider: MangaProviders) -> rusqlite::Result<std::collections::HashSet<String>> {
        let mut statement = self.connection.prepare("SELECT manga_id FROM favorite_mangas WHERE provider = ?1")?;
        let rows = statement.query_map(params![provider.to_string()], |row| row.get(0))?;
        let mut set = std::collections::HashSet::new();
        for id in rows {
            set.insert(id?);
        }
        Ok(set)
    }

    pub fn get_all_favorite_mangas(&self) -> rusqlite::Result<Vec<FavoriteMangaRecord>> {
        let mut statement = self
            .connection
            .prepare("SELECT manga_id, provider, title FROM favorite_mangas ORDER BY created_at DESC")?;
        let rows = statement.query_map([], |row| {
            let manga_id: String = row.get(0)?;
            let provider_str: String = row.get(1)?;
            let title: String = row.get(2)?;
            let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
            Ok(FavoriteMangaRecord {
                manga_id,
                provider,
                title,
            })
        })?;
        rows.collect()
    }

    pub fn save_search_history(&self, query: &str) -> rusqlite::Result<()> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let now = Utc::now().timestamp_millis();
        let trimmed_lower = trimmed.to_lowercase();
        let existing_queries: Vec<String> = {
            let mut stmt = self.connection.prepare("SELECT query FROM search_history")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<String>>>()?
        };
        for existing in existing_queries {
            if existing.to_lowercase() == trimmed_lower {
                self.connection
                    .execute("DELETE FROM search_history WHERE query = ?1", rusqlite::params![existing])?;
            }
        }
        self.connection
            .execute("INSERT INTO search_history (query, searched_at) VALUES (?1, ?2)", rusqlite::params![trimmed, now])?;
        Ok(())
    }

    pub fn get_recent_searches(&self, limit: usize) -> rusqlite::Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT query FROM search_history ORDER BY searched_at DESC, rowid DESC LIMIT ?1")?;
        let rows = statement.query_map([limit as i64], |row| row.get(0))?;
        let mut searches = Vec::new();
        for query in rows {
            searches.push(query?);
        }
        Ok(searches)
    }

    pub fn clear_search_history(&self) -> rusqlite::Result<()> {
        self.connection.execute("DELETE FROM search_history", ())?;
        Ok(())
    }

    pub fn get_recent_reading_mangas(&self, limit: u32) -> rusqlite::Result<Vec<FavoriteMangaRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT mangas.id, mangas.manga_provider, mangas.title 
             FROM mangas
             INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id
             INNER JOIN history_types ON manga_history_union.type_id = history_types.id
             WHERE history_types.name = 'ReadingHistory'
             ORDER BY mangas.last_read DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            let manga_id: String = row.get(0)?;
            let provider_str: String = row.get(1)?;
            let title: String = row.get(2)?;
            let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
            Ok(FavoriteMangaRecord {
                manga_id,
                provider,
                title,
            })
        })?;
        rows.collect()
    }

    pub fn get_recently_read_user_mangas(&self, limit: u32) -> rusqlite::Result<Vec<(String, MangaProviders, u32)>> {
        let mut statement = self.connection.prepare(
            "SELECT mangas.title, mangas.manga_provider, COUNT(chapters.id) AS read_count
             FROM mangas
             INNER JOIN chapters ON mangas.id = chapters.manga_id
             WHERE chapters.is_read = 1 AND mangas.deleted_at IS NULL
             GROUP BY mangas.id
             ORDER BY mangas.last_read DESC, read_count DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            let title: String = row.get(0)?;
            let provider_str: String = row.get(1)?;
            let read_count: u32 = row.get(2)?;
            let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
            Ok((title, provider, read_count))
        })?;
        rows.collect()
    }

    /// Aggregated user statistics shown in the Home page hero header.
    pub fn get_home_stats(&self) -> rusqlite::Result<HomeStats> {
        let library_mangas: u32 = self.connection.query_row(
            "SELECT COUNT(DISTINCT manga_history_union.manga_id)
             FROM manga_history_union
             INNER JOIN mangas ON mangas.id = manga_history_union.manga_id
             WHERE mangas.deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        let chapters_read: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM chapters WHERE is_read = 1", [], |row| row.get(0))?;
        let downloaded_chapters: u32 =
            self.connection
                .query_row("SELECT COUNT(*) FROM chapters WHERE is_downloaded = 1", [], |row| row.get(0))?;
        let favorites: u32 = self.connection.query_row("SELECT COUNT(*) FROM favorite_mangas", [], |row| row.get(0))?;
        let last_read_title: Option<String> = self
            .connection
            .query_row(
                "SELECT mangas.title
                 FROM mangas
                 INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id
                 INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                 WHERE history_types.name = 'ReadingHistory' AND mangas.deleted_at IS NULL
                 ORDER BY mangas.last_read DESC
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        Ok(HomeStats {
            library_mangas,
            chapters_read,
            downloaded_chapters,
            favorites,
            last_read_title,
        })
    }

    pub fn get_downloaded_chapter_ids(&self, manga_id: &str, provider: MangaProviders) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT chapters.id
             FROM chapters
             INNER JOIN mangas ON mangas.id = chapters.manga_id
             WHERE mangas.id = ?1 AND mangas.manga_provider = ?2 AND chapters.is_downloaded = 1",
        )?;
        let rows = statement.query_map(params![manga_id, provider.to_string()], |row| row.get(0))?;
        rows.collect()
    }

    /// Returns the custom shelf/category name assigned to a manga, if any.
    pub fn get_manga_shelf(&self, manga_id: &str, provider: MangaProviders) -> rusqlite::Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT shelf_name FROM manga_collections WHERE manga_id = ?1 AND provider = ?2 LIMIT 1",
                params![manga_id, provider.to_string()],
                |row| row.get(0),
            )
            .optional()
    }

    /// Sets or removes (`shelf_name = None`) a custom shelf/category for a manga.
    pub fn set_manga_shelf(
        &self,
        manga_id: &str,
        provider: MangaProviders,
        title: &str,
        shelf_name: Option<&str>,
    ) -> rusqlite::Result<Option<String>> {
        match shelf_name.map(str::trim).filter(|s| !s.is_empty()) {
            Some(shelf) => {
                self.connection.execute(
                    "INSERT OR REPLACE INTO manga_collections (manga_id, provider, title, shelf_name, created_at)
                     VALUES (?1, ?2, ?3, ?4, datetime('now'))",
                    params![manga_id, provider.to_string(), title, shelf],
                )?;
                Ok(Some(shelf.to_string()))
            },
            None => {
                self.connection
                    .execute("DELETE FROM manga_collections WHERE manga_id = ?1 AND provider = ?2", params![
                        manga_id,
                        provider.to_string()
                    ])?;
                Ok(None)
            },
        }
    }

    /// Lists all shelves (preset + any custom shelves stored in DB) along with their manga counts.
    pub fn list_all_shelves(&self) -> rusqlite::Result<Vec<(String, u32)>> {
        let mut counts_by_shelf: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
        if let Ok(mut stmt) = self
            .connection
            .prepare("SELECT shelf_name, COUNT(*) FROM manga_collections GROUP BY shelf_name")
            && let Ok(rows) = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)))
        {
            for r in rows.flatten() {
                if !r.0.trim().is_empty() {
                    counts_by_shelf.insert(r.0, r.1);
                }
            }
        }

        let mut ordered = Vec::new();
        for preset in PRESET_SHELVES {
            let count = counts_by_shelf.remove(preset).unwrap_or(0);
            ordered.push((preset.to_string(), count));
        }
        for (custom_shelf, count) in counts_by_shelf {
            ordered.push((custom_shelf, count));
        }
        Ok(ordered)
    }

    /// Cycles the assigned shelf for a manga through `PRESET_SHELVES` (plus any custom shelves) and back to `None`.
    pub fn cycle_manga_shelf(&self, manga_id: &str, provider: MangaProviders, title: &str) -> rusqlite::Result<Option<String>> {
        let current = self.get_manga_shelf(manga_id, provider)?;
        let all_shelves: Vec<String> = self.list_all_shelves()?.into_iter().map(|(s, _)| s).collect();
        if all_shelves.is_empty() {
            return Ok(None);
        }

        let next_shelf: Option<&str> = match current.as_deref() {
            None => Some(all_shelves[0].as_str()),
            Some(curr) => {
                if let Some(pos) = all_shelves.iter().position(|s| s == curr) {
                    if pos + 1 < all_shelves.len() { Some(all_shelves[pos + 1].as_str()) } else { None }
                } else {
                    Some(all_shelves[0].as_str())
                }
            },
        };

        self.set_manga_shelf(manga_id, provider, title, next_shelf)
    }

    /// Computes comprehensive local reading statistics for the Reading Stats Dashboard modal.
    pub fn get_reading_dashboard_stats(&self) -> rusqlite::Result<ReadingDashboardStats> {
        let total_chapters_read: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM chapters WHERE is_read = 1", [], |row| row.get(0))
            .unwrap_or(0);
        let estimated_reading_minutes = total_chapters_read.saturating_mul(6);

        let reading_mangas: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT manga_history_union.manga_id)
                 FROM manga_history_union
                 INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                 INNER JOIN mangas ON mangas.id = manga_history_union.manga_id
                 WHERE history_types.name = 'ReadingHistory' AND mangas.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let plan_to_read_mangas: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT manga_history_union.manga_id)
                 FROM manga_history_union
                 INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                 INNER JOIN mangas ON mangas.id = manga_history_union.manga_id
                 WHERE history_types.name = 'PlanToRead' AND mangas.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let favorites_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM favorite_mangas", [], |row| row.get(0))
            .unwrap_or(0);

        let shelves_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM manga_collections", [], |row| row.get(0))
            .unwrap_or(0);

        let bookmarks_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM chapters WHERE is_bookmarked = 1", [], |row| row.get(0))
            .unwrap_or(0);

        let downloaded_db_chapters: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM chapters WHERE is_downloaded = 1", [], |row| row.get(0))
            .unwrap_or(0);

        let active_days_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT date(mangas.last_read))
                 FROM mangas
                 INNER JOIN chapters ON chapters.manga_id = mangas.id
                 WHERE chapters.is_read = 1 AND mangas.last_read IS NOT NULL AND mangas.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let read_last_7_days: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(chapters.id)
                 FROM chapters
                 INNER JOIN mangas ON mangas.id = chapters.manga_id
                 WHERE chapters.is_read = 1
                   AND mangas.deleted_at IS NULL
                   AND mangas.last_read >= datetime('now', '-7 days')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Compute current consecutive-day streak based on distinct `date(last_read)` in descending order
        let mut current_streak_days: u32 = 0;
        if let Ok(mut stmt) = self.connection.prepare(
            "SELECT DISTINCT CAST(julianday('now') - julianday(date(mangas.last_read)) AS INTEGER) AS days_ago
             FROM mangas
             INNER JOIN chapters ON chapters.manga_id = mangas.id
             WHERE chapters.is_read = 1 AND mangas.last_read IS NOT NULL AND mangas.deleted_at IS NULL
               AND days_ago >= 0
             ORDER BY days_ago ASC",
        ) && let Ok(rows) = stmt.query_map([], |row| row.get::<_, i64>(0))
        {
            let days_ago_list: Vec<i64> = rows.flatten().collect();
            if let Some(&first) = days_ago_list.first()
                && first <= 1
            {
                let mut expected = first;
                for d in days_ago_list {
                    if d == expected {
                        current_streak_days += 1;
                        expected += 1;
                    } else if d > expected {
                        break;
                    }
                }
            }
        }

        let top_read_mangas = {
            let mut list = Vec::new();
            if let Ok(mut stmt) = self.connection.prepare(
                "SELECT mangas.title, mangas.manga_provider, COUNT(chapters.id) AS read_count
                 FROM mangas
                 INNER JOIN chapters ON mangas.id = chapters.manga_id
                 WHERE chapters.is_read = 1 AND mangas.deleted_at IS NULL
                 GROUP BY mangas.id
                 ORDER BY read_count DESC, mangas.last_read DESC
                 LIMIT 5",
            ) && let Ok(rows) = stmt.query_map([], |row| {
                let title: String = row.get(0)?;
                let prov_str: String = row.get(1)?;
                let cnt: u32 = row.get(2)?;
                let prov = prov_str.parse::<MangaProviders>().unwrap_or_default();
                Ok((title, prov, cnt))
            }) {
                for r in rows.flatten() {
                    list.push(r);
                }
            }
            list
        };

        let provider_breakdown = {
            let mut list = Vec::new();
            if let Ok(mut stmt) = self.connection.prepare(
                "SELECT mangas.manga_provider,
                        COUNT(DISTINCT mangas.id) AS manga_count,
                        COALESCE(SUM(CASE WHEN chapters.is_read = 1 THEN 1 ELSE 0 END), 0) AS read_chapters
                 FROM mangas
                 LEFT JOIN chapters ON mangas.id = chapters.manga_id
                 WHERE mangas.deleted_at IS NULL
                 GROUP BY mangas.manga_provider
                 ORDER BY read_chapters DESC, manga_count DESC",
            ) && let Ok(rows) = stmt.query_map([], |row| {
                let prov_str: String = row.get(0)?;
                let m_cnt: u32 = row.get(1)?;
                let ch_cnt: u32 = row.get(2)?;
                let prov = prov_str.parse::<MangaProviders>().unwrap_or_default();
                Ok((prov, m_cnt, ch_cnt))
            }) {
                for r in rows.flatten() {
                    list.push(r);
                }
            }
            list
        };

        Ok(ReadingDashboardStats {
            total_chapters_read,
            estimated_reading_minutes,
            reading_mangas,
            plan_to_read_mangas,
            favorites_count,
            shelves_count,
            bookmarks_count,
            downloaded_db_chapters,
            active_days_count,
            current_streak_days,
            read_last_7_days,
            top_read_mangas,
            provider_breakdown,
        })
    }

    /// Records a 100 % device-local reading session for extended reader metrics.
    pub fn record_reading_session(&self, input: ReadingSessionInput<'_>) -> rusqlite::Result<i64> {
        // Guard against forgotten open tabs skewing totals (cap individual session at 12h).
        let clamped_seconds = input.duration_seconds.min(43_200);
        self.connection.execute(
            "INSERT INTO reading_sessions (manga_id, provider, chapter_id, started_at, duration_seconds, pages_read, completed_chapter)
             VALUES (?1, ?2, ?3, COALESCE(?4, datetime('now')), ?5, ?6, ?7)",
            params![
                input.manga_id,
                input.provider.to_string(),
                input.chapter_id,
                input.started_at,
                clamped_seconds,
                input.pages_read,
                input.completed_chapter,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Persists genres/tags associated with a manga, normalizing whitespace and avoiding case-insensitive duplicates.
    pub fn save_manga_genres(&self, manga_id: &str, provider: MangaProviders, genres: &[String]) -> rusqlite::Result<usize> {
        let provider_str = provider.to_string();
        let mut seen: HashSet<String> = self.get_manga_genres(manga_id, provider)?.into_iter().map(|g| g.to_lowercase()).collect();

        let tx = self.connection.unchecked_transaction()?;
        let mut inserted = 0usize;
        for raw in genres {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_lowercase();
            if !seen.insert(lower) {
                continue;
            }
            let affected =
                tx.execute("INSERT OR IGNORE INTO manga_genres (manga_id, provider, genre) VALUES (?1, ?2, ?3)", params![
                    manga_id,
                    provider_str,
                    trimmed
                ])?;
            inserted += affected;
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Returns stored genres for a given `(manga_id, provider)` sorted alphabetically.
    pub fn get_manga_genres(&self, manga_id: &str, provider: MangaProviders) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.connection.prepare(
            "SELECT genre FROM manga_genres
             WHERE manga_id = ?1 AND provider = ?2
             ORDER BY genre COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map(params![manga_id, provider.to_string()], |row| row.get(0))?;
        rows.collect()
    }

    /// Returns all stored genres indexed by `(manga_id, provider)`.
    pub fn get_all_manga_genres_map(&self) -> rusqlite::Result<HashMap<(String, String), Vec<String>>> {
        let mut map: HashMap<(String, String), Vec<String>> = HashMap::new();
        let mut stmt = self.connection.prepare(
            "SELECT manga_id, provider, genre FROM manga_genres
             ORDER BY manga_id, provider, genre COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?;
        for row in rows.flatten() {
            map.entry((row.0, row.1)).or_default().push(row.2);
        }
        Ok(map)
    }

    /// Computes comprehensive 100 % device-local reader metrics (zero external telemetry).
    pub fn get_extended_reader_metrics(&self) -> rusqlite::Result<ExtendedReaderMetrics> {
        let now = Utc::now();
        let today = now.date_naive();
        let generated_at = now.to_rfc3339();

        let reading_mangas: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT manga_history_union.manga_id)
                 FROM manga_history_union
                 INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                 INNER JOIN mangas ON mangas.id = manga_history_union.manga_id
                 WHERE history_types.name = 'ReadingHistory' AND mangas.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let plan_to_read_mangas: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT manga_history_union.manga_id)
                 FROM manga_history_union
                 INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                 INNER JOIN mangas ON mangas.id = manga_history_union.manga_id
                 WHERE history_types.name = 'PlanToRead' AND mangas.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let favorites_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM favorite_mangas", [], |row| row.get(0))
            .unwrap_or(0);

        let shelves_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM manga_collections", [], |row| row.get(0))
            .unwrap_or(0);

        let downloaded_chapters: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM chapters WHERE is_downloaded = 1", [], |row| row.get(0))
            .unwrap_or(0);

        let chapters_in_table: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chapters
                 INNER JOIN mangas ON mangas.id = chapters.manga_id
                 WHERE chapters.is_read = 1 AND mangas.deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let session_only_completed: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(DISTINCT chapter_id) FROM reading_sessions
                 WHERE completed_chapter = 1
                   AND NOT EXISTS (
                       SELECT 1 FROM chapters c
                       WHERE c.id = reading_sessions.chapter_id AND c.is_read = 1
                   )",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_chapters_read = chapters_in_table.saturating_add(session_only_completed);

        let recorded_reading_seconds: u64 = self
            .connection
            .query_row("SELECT COALESCE(SUM(duration_seconds), 0) FROM reading_sessions", [], |row| row.get(0))
            .unwrap_or(0);

        let total_pages_read: u32 = self
            .connection
            .query_row("SELECT COALESCE(SUM(pages_read), 0) FROM reading_sessions", [], |row| row.get(0))
            .unwrap_or(0);

        let legacy_read_without_session: u64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chapters
                 INNER JOIN mangas ON mangas.id = chapters.manga_id
                 WHERE chapters.is_read = 1 AND mangas.deleted_at IS NULL
                   AND NOT EXISTS (SELECT 1 FROM reading_sessions rs WHERE rs.chapter_id = chapters.id)",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_seconds = recorded_reading_seconds.saturating_add(legacy_read_without_session.saturating_mul(360));
        let estimated_total_reading_minutes =
            if total_seconds == 0 { 0 } else { ((total_seconds as f64) / 60.0).round().max(1.0) as u32 };

        let average_minutes_per_chapter = if total_chapters_read == 0 {
            0.0
        } else {
            ((estimated_total_reading_minutes as f32 / total_chapters_read as f32) * 10.0).round() / 10.0
        };

        let mut active_dates: BTreeSet<NaiveDate> = BTreeSet::new();
        let mut chapters_by_date: HashMap<NaiveDate, u32> = HashMap::new();
        let mut seconds_by_date: HashMap<NaiveDate, u64> = HashMap::new();
        let mut pages_by_date: HashMap<NaiveDate, u32> = HashMap::new();

        if let Ok(mut stmt) = self.connection.prepare(
            "SELECT date(started_at) AS d,
                    COALESCE(SUM(duration_seconds), 0) AS secs,
                    COALESCE(SUM(pages_read), 0) AS pages,
                    COUNT(DISTINCT CASE WHEN completed_chapter = 1 THEN chapter_id END) AS completed_chs
             FROM reading_sessions
             WHERE started_at IS NOT NULL
             GROUP BY d",
        ) && let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, u64>(1)?, row.get::<_, u32>(2)?, row.get::<_, u32>(3)?))
        }) {
            for (date_str_opt, secs, pages, completed_chs) in rows.flatten() {
                if let Some(date_str) = date_str_opt
                    && let Ok(date) = NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d")
                {
                    active_dates.insert(date);
                    *seconds_by_date.entry(date).or_insert(0) += secs;
                    *pages_by_date.entry(date).or_insert(0) += pages;
                    *chapters_by_date.entry(date).or_insert(0) += completed_chs;
                }
            }
        }

        if let Ok(mut stmt) = self.connection.prepare(
            "SELECT date(mangas.last_read) AS d,
                    COUNT(chapters.id) AS ch_cnt,
                    COALESCE(SUM(CASE
                        WHEN NOT EXISTS (SELECT 1 FROM reading_sessions rs WHERE rs.chapter_id = chapters.id) THEN 1
                        ELSE 0
                    END), 0) AS no_session_cnt
             FROM chapters
             INNER JOIN mangas ON mangas.id = chapters.manga_id
             WHERE chapters.is_read = 1
               AND mangas.deleted_at IS NULL
               AND mangas.last_read IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM reading_sessions rs
                   WHERE rs.chapter_id = chapters.id AND rs.completed_chapter = 1
               )
             GROUP BY d",
        ) && let Ok(rows) =
            stmt.query_map([], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, u32>(1)?, row.get::<_, u64>(2)?)))
        {
            for (date_str_opt, ch_cnt, no_session_cnt) in rows.flatten() {
                if let Some(date_str) = date_str_opt
                    && let Ok(date) = NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d")
                {
                    active_dates.insert(date);
                    *chapters_by_date.entry(date).or_insert(0) += ch_cnt;
                    *seconds_by_date.entry(date).or_insert(0) += no_session_cnt.saturating_mul(360);
                }
            }
        }

        let chapters_read_today = chapters_by_date.get(&today).copied().unwrap_or(0);
        let window_7_start = today - Duration::days(6);
        let window_30_start = today - Duration::days(29);

        let chapters_read_last_7_days: u32 = chapters_by_date
            .iter()
            .filter(|&(&d, _)| d >= window_7_start && d <= today)
            .map(|(_, &cnt)| cnt)
            .sum();
        let chapters_read_last_30_days: u32 = chapters_by_date
            .iter()
            .filter(|&(&d, _)| d >= window_30_start && d <= today)
            .map(|(_, &cnt)| cnt)
            .sum();

        let active_days_count = active_dates.len() as u32;
        let sorted_dates: Vec<NaiveDate> = active_dates.into_iter().collect();
        let (current_streak_days, longest_streak_days) = compute_reading_streaks(&sorted_dates, today);

        let mut daily_activity_last_14_days = Vec::with_capacity(14);
        for days_back in (0..14).rev() {
            let date = today - Duration::days(days_back);
            let chapters_read = chapters_by_date.get(&date).copied().unwrap_or(0);
            let secs = seconds_by_date.get(&date).copied().unwrap_or(0);
            let reading_minutes = if secs == 0 { 0 } else { ((secs as f64) / 60.0).round().max(1.0) as u32 };
            let pages_read = pages_by_date.get(&date).copied().unwrap_or(0);
            daily_activity_last_14_days.push(DailyReadingActivity {
                date: date.format("%Y-%m-%d").to_string(),
                chapters_read,
                reading_minutes,
                pages_read,
            });
        }

        // Per-manga aggregation without double-counting chapters or reading time
        let mut manga_meta: HashMap<(String, String), String> = HashMap::new();
        let mut manga_chapters_read: HashMap<(String, String), u32> = HashMap::new();
        let mut manga_seconds: HashMap<(String, String), u64> = HashMap::new();

        if let Ok(mut stmt) = self
            .connection
            .prepare("SELECT id, title, manga_provider FROM mangas WHERE deleted_at IS NULL")
            && let Ok(rows) =
                stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
        {
            for (id, title, provider) in rows.flatten() {
                manga_meta.insert((id, provider), title);
            }
        }

        if let Ok(mut stmt) = self.connection.prepare(
            "SELECT mangas.id,
                    mangas.manga_provider,
                    COUNT(chapters.id) AS read_cnt,
                    COALESCE(SUM(CASE
                        WHEN NOT EXISTS (SELECT 1 FROM reading_sessions rs WHERE rs.chapter_id = chapters.id) THEN 1
                        ELSE 0
                    END), 0) AS legacy_cnt
             FROM mangas
             INNER JOIN chapters ON chapters.manga_id = mangas.id
             WHERE chapters.is_read = 1 AND mangas.deleted_at IS NULL
             GROUP BY mangas.id, mangas.manga_provider",
        ) && let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u32>(2)?, row.get::<_, u64>(3)?))
        }) {
            for (id, provider, read_cnt, legacy_cnt) in rows.flatten() {
                *manga_chapters_read.entry((id.clone(), provider.clone())).or_insert(0) += read_cnt;
                *manga_seconds.entry((id, provider)).or_insert(0) += legacy_cnt.saturating_mul(360);
            }
        }

        if let Ok(mut stmt) = self.connection.prepare(
            "SELECT manga_id,
                    provider,
                    COALESCE(SUM(duration_seconds), 0) AS secs,
                    COUNT(DISTINCT CASE
                        WHEN completed_chapter = 1
                             AND NOT EXISTS (SELECT 1 FROM chapters c WHERE c.id = reading_sessions.chapter_id AND c.is_read = 1)
                        THEN chapter_id
                    END) AS session_only_chs
             FROM reading_sessions
             GROUP BY manga_id, provider",
        ) && let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u64>(2)?, row.get::<_, u32>(3)?))
        }) {
            for (id, provider, secs, extra_chs) in rows.flatten() {
                *manga_seconds.entry((id.clone(), provider.clone())).or_insert(0) += secs;
                *manga_chapters_read.entry((id, provider)).or_insert(0) += extra_chs;
            }
        }

        let mut top_read_mangas: Vec<TopMangaStat> = manga_chapters_read
            .iter()
            .filter_map(|((id, provider_str), &ch_count)| {
                if ch_count == 0 {
                    return None;
                }
                let title = manga_meta.get(&(id.clone(), provider_str.clone()))?.clone();
                let secs = manga_seconds.get(&(id.clone(), provider_str.clone())).copied().unwrap_or(0);
                let reading_minutes = if secs == 0 { 0 } else { ((secs as f64) / 60.0).round().max(1.0) as u32 };
                Some(TopMangaStat {
                    manga_id: id.clone(),
                    title,
                    provider: provider_str.parse::<MangaProviders>().unwrap_or_default(),
                    chapters_read: ch_count,
                    reading_minutes,
                })
            })
            .collect();
        top_read_mangas.sort_by(|a, b| {
            b.chapters_read
                .cmp(&a.chapters_read)
                .then_with(|| b.reading_minutes.cmp(&a.reading_minutes))
                .then_with(|| a.title.cmp(&b.title))
        });
        top_read_mangas.truncate(5);

        struct GenreAcc {
            display_name: String,
            mangas: HashSet<(String, String)>,
            chapters_read: u32,
            seconds: u64,
        }
        let mut genre_map: HashMap<String, GenreAcc> = HashMap::new();
        if let Ok(mut stmt) = self
            .connection
            .prepare("SELECT manga_id, provider, genre FROM manga_genres ORDER BY genre COLLATE NOCASE ASC")
            && let Ok(rows) =
                stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
        {
            for (manga_id, provider, genre) in rows.flatten() {
                let trimmed = genre.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let key = trimmed.to_lowercase();
                let manga_key = (manga_id, provider);
                let acc = genre_map.entry(key).or_insert_with(|| GenreAcc {
                    display_name: trimmed.to_string(),
                    mangas: HashSet::new(),
                    chapters_read: 0,
                    seconds: 0,
                });
                if acc.mangas.insert(manga_key.clone()) {
                    acc.chapters_read += manga_chapters_read.get(&manga_key).copied().unwrap_or(0);
                    acc.seconds += manga_seconds.get(&manga_key).copied().unwrap_or(0);
                }
            }
        }

        let total_genre_chapters: u32 = genre_map.values().map(|g| g.chapters_read).sum();
        let total_genre_mangas: u32 = genre_map.values().map(|g| g.mangas.len() as u32).sum();
        let mut top_genres: Vec<GenreReadingStat> = genre_map
            .into_values()
            .map(|acc| {
                let mangas_count = acc.mangas.len() as u32;
                let reading_minutes = if acc.seconds == 0 { 0 } else { ((acc.seconds as f64) / 60.0).round().max(1.0) as u32 };
                let share_percent = if total_genre_chapters > 0 {
                    ((acc.chapters_read as f32 / total_genre_chapters as f32) * 1000.0).round() / 10.0
                } else if total_genre_mangas > 0 {
                    ((mangas_count as f32 / total_genre_mangas as f32) * 1000.0).round() / 10.0
                } else {
                    0.0
                };
                GenreReadingStat {
                    genre: acc.display_name,
                    mangas_count,
                    chapters_read: acc.chapters_read,
                    reading_minutes,
                    share_percent,
                }
            })
            .collect();
        top_genres.sort_by(|a, b| {
            b.chapters_read
                .cmp(&a.chapters_read)
                .then_with(|| b.mangas_count.cmp(&a.mangas_count))
                .then_with(|| a.genre.cmp(&b.genre))
        });
        top_genres.truncate(8);

        let provider_breakdown = {
            let mut list = Vec::new();
            if let Ok(mut stmt) = self.connection.prepare(
                "SELECT mangas.manga_provider,
                        COUNT(DISTINCT mangas.id) AS manga_count,
                        COALESCE(SUM(CASE WHEN chapters.is_read = 1 THEN 1 ELSE 0 END), 0) AS read_chapters
                 FROM mangas
                 LEFT JOIN chapters ON mangas.id = chapters.manga_id
                 WHERE mangas.deleted_at IS NULL
                 GROUP BY mangas.manga_provider
                 ORDER BY read_chapters DESC, manga_count DESC",
            ) && let Ok(rows) = stmt.query_map([], |row| {
                let prov_str: String = row.get(0)?;
                let m_cnt: u32 = row.get(1)?;
                let ch_cnt: u32 = row.get(2)?;
                Ok(ProviderReadingStat {
                    provider: prov_str.parse::<MangaProviders>().unwrap_or_default(),
                    mangas_count: m_cnt,
                    chapters_read: ch_cnt,
                })
            }) {
                for r in rows.flatten() {
                    list.push(r);
                }
            }
            list
        };

        Ok(ExtendedReaderMetrics {
            total_chapters_read,
            chapters_read_today,
            chapters_read_last_7_days,
            chapters_read_last_30_days,
            total_pages_read,
            recorded_reading_seconds,
            estimated_total_reading_minutes,
            average_minutes_per_chapter,
            active_days_count,
            current_streak_days,
            longest_streak_days,
            reading_mangas,
            plan_to_read_mangas,
            favorites_count,
            shelves_count,
            downloaded_chapters,
            top_genres,
            top_read_mangas,
            provider_breakdown,
            daily_activity_last_14_days,
            generated_at,
            privacy_local_only: true,
        })
    }

    fn get_total_mangas_in_history(&self, history_type_id: i32, provider: MangaProviders) -> rusqlite::Result<u32> {
        let total: u32 = self.connection.query_row(
            "
                SELECT COUNT(*) from mangas
                INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
                WHERE manga_history_union.type_id = ?1 AND mangas.manga_provider = ?2",
            params![history_type_id, provider.to_string()],
            |row| row.get(0),
        )?;

        Ok(total)
    }

    fn filter_manga_history_by_search_term(
        &self,
        search_term: SearchTerm,
        history_type_id: i32,
        current_page: u32,
        items_per_page: u32,
        offset: u32,
        provider: MangaProviders,
    ) -> rusqlite::Result<MangaHistoryResponse> {
        let search_term = search_term.get();
        let mut manga_history: Vec<MangaHistory> = vec![];
        let total_mangas_with_search: u32 = self.connection.query_row(
            "
                SELECT COUNT(*) from mangas
                INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
                WHERE manga_history_union.type_id = ?1 AND LOWER(mangas.title) LIKE '%' || ?2 || '%' AND mangas.manga_provider = ?3",
            params![history_type_id, search_term, provider.to_string()],
            |row| row.get(0),
        )?;

        let mut get_statement_with_search_term = self.connection.prepare(
            "SELECT  mangas.id, mangas.title, mangas.manga_provider, mangas.last_read,
                     (SELECT title FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_read = 1 ORDER BY chapters.rowid DESC LIMIT 1) AS last_chapter,
                     (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_read = 1) AS read_count,
                     (SELECT title FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_bookmarked = 1 LIMIT 1) AS bookmark_title,
                     (SELECT number_page_bookmarked FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_bookmarked = 1 LIMIT 1) AS bookmark_page,
                     (SELECT COUNT(*) FROM favorite_mangas WHERE favorite_mangas.manga_id = mangas.id AND favorite_mangas.provider = mangas.manga_provider) AS is_fav,
                     (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_downloaded = 1) AS dl_count,
                     (SELECT shelf_name FROM manga_collections WHERE manga_collections.manga_id = mangas.id AND manga_collections.provider = mangas.manga_provider LIMIT 1) AS shelf_name
             FROM mangas 
             INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
             WHERE manga_history_union.type_id = ?1 AND LOWER(mangas.title) LIKE '%' || ?2 || '%' AND mangas.manga_provider = ?3
             ORDER BY mangas.last_read DESC
             LIMIT ?4 OFFSET ?5",
        )?;
        let iter_mangas = get_statement_with_search_term.query_map(
            params![history_type_id, search_term, provider.to_string(), items_per_page, offset],
            |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let provider_str: String = row.get(2).unwrap_or_default();
                let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
                let last_read: Option<String> = row.get(3).ok();
                let last_read_chapter: Option<String> = row.get(4).ok();
                let total_read_count: u32 = row.get(5).unwrap_or(0);
                let bookmark_chapter: Option<String> = row.get(6).ok();
                let bookmark_page: Option<u32> = row.get(7).ok();
                let is_fav_count: i32 = row.get(8).unwrap_or(0);
                let dl_count: u32 = row.get(9).unwrap_or(0);
                let shelf_name: Option<String> = row.get(10).ok().flatten();

                Ok(MangaHistory {
                    id,
                    title,
                    provider,
                    last_read,
                    last_read_chapter,
                    total_read_count,
                    bookmark_chapter,
                    bookmark_page,
                    is_favorite: is_fav_count > 0,
                    is_plan_to_read: false,
                    downloaded_count: dl_count,
                    shelf_name,
                })
            },
        )?;

        for manga in iter_mangas {
            manga_history.push(manga?);
        }

        Ok(MangaHistoryResponse {
            mangas: manga_history,
            total_items: total_mangas_with_search,
            page: current_page,
        })
    }

    /// Retrieves manga from reading history or plan to read list with pagination and search.
    ///
    /// This is the main method for fetching manga collections for display in the UI.
    /// Supports:
    /// - Pagination for large collections
    /// - Text search by manga title
    /// - Provider filtering (only show manga from specific sources)
    /// - Separate history types (ReadingHistory vs PlanToRead)
    ///
    /// Results are ordered by last_read timestamp (most recent first).
    ///
    /// # Arguments
    ///
    /// * `args` - Query parameters including page, search term, provider filter
    ///
    /// # Returns
    ///
    /// `MangaHistoryResponse` containing:
    /// - `mangas`: List of manga for the requested page
    /// - `page`: Current page number
    /// - `total_items`: Total count of matching manga (for pagination)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use crate::backend::database::{GetHistoryArgs, MangaHistoryType};
    /// # use crate::SearchTerm;
    /// let history = db.get_history(GetHistoryArgs {
    ///     hist_type: MangaHistoryType::ReadingHistory,
    ///     page: 1,
    ///     search: Some(SearchTerm::new("attack on titan")),
    ///     items_per_page: 20,
    ///     provider: MangaProviders::Mangadx,
    /// })?;
    ///
    /// println!("Found {} manga", history.total_items);
    /// for manga in history.mangas {
    ///     println!("- {}", manga.title);
    /// }
    /// ```
    pub fn get_history(&self, args: GetHistoryArgs) -> rusqlite::Result<MangaHistoryResponse> {
        let items_per_page = args.items_per_page;
        let offset = (args.page - 1) * items_per_page;

        let history_type_id: i32 = self.connection.query_row(
            "SELECT id from history_types WHERE name = ?1",
            params![args.hist_type.to_string()],
            |row| row.get(0),
        )?;

        if let Some(search_term) = args.search {
            return self.filter_manga_history_by_search_term(
                search_term,
                history_type_id,
                args.page,
                items_per_page,
                offset,
                args.provider,
            );
        }

        let total_mangas: u32 = self.get_total_mangas_in_history(history_type_id, args.provider)?;

        let mut get_statement = self.connection.prepare(
            "SELECT  mangas.id, mangas.title, mangas.manga_provider, mangas.last_read,
                     (SELECT title FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_read = 1 ORDER BY chapters.rowid DESC LIMIT 1) AS last_chapter,
                     (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_read = 1) AS read_count,
                     (SELECT title FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_bookmarked = 1 LIMIT 1) AS bookmark_title,
                     (SELECT number_page_bookmarked FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_bookmarked = 1 LIMIT 1) AS bookmark_page,
                     (SELECT COUNT(*) FROM favorite_mangas WHERE favorite_mangas.manga_id = mangas.id AND favorite_mangas.provider = mangas.manga_provider) AS is_fav,
                     (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_downloaded = 1) AS dl_count,
                     (SELECT shelf_name FROM manga_collections WHERE manga_collections.manga_id = mangas.id AND manga_collections.provider = mangas.manga_provider LIMIT 1) AS shelf_name
             FROM mangas 
             INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
             WHERE manga_history_union.type_id = ?1 AND mangas.manga_provider = ?2
             ORDER BY mangas.last_read DESC
             LIMIT ?3 OFFSET ?4",
        )?;

        let mut manga_history: Vec<MangaHistory> = vec![];

        let iter_mangas =
            get_statement.query_map(params![history_type_id, args.provider.to_string(), items_per_page, offset], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let provider_str: String = row.get(2).unwrap_or_default();
                let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
                let last_read: Option<String> = row.get(3).ok();
                let last_read_chapter: Option<String> = row.get(4).ok();
                let total_read_count: u32 = row.get(5).unwrap_or(0);
                let bookmark_chapter: Option<String> = row.get(6).ok();
                let bookmark_page: Option<u32> = row.get(7).ok();
                let is_fav_count: i32 = row.get(8).unwrap_or(0);
                let dl_count: u32 = row.get(9).unwrap_or(0);
                let shelf_name: Option<String> = row.get(10).ok().flatten();

                Ok(MangaHistory {
                    id,
                    title,
                    provider,
                    last_read,
                    last_read_chapter,
                    total_read_count,
                    bookmark_chapter,
                    bookmark_page,
                    is_favorite: is_fav_count > 0,
                    is_plan_to_read: false,
                    downloaded_count: dl_count,
                    shelf_name,
                })
            })?;

        for manga in iter_mangas {
            manga_history.push(manga?);
        }

        Ok(MangaHistoryResponse {
            mangas: manga_history,
            total_items: total_mangas,
            page: args.page,
        })
    }

    /// Retrieves reading history across ALL providers, ordered by last_read timestamp.
    pub fn get_continue_reading(&self, args: GetHistoryArgs) -> rusqlite::Result<MangaHistoryResponse> {
        let items_per_page = args.items_per_page;
        let offset = (args.page - 1) * items_per_page;
        let search_filter = args.search.map(|s| s.get().to_lowercase());

        let count_query = "
            SELECT COUNT(*) 
            FROM mangas 
            INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
            INNER JOIN history_types ON manga_history_union.type_id = history_types.id 
            WHERE history_types.name = 'ReadingHistory' 
              AND (?1 IS NULL OR LOWER(mangas.title) LIKE '%' || ?1 || '%')
        ";
        let total_items: u32 = self.connection.query_row(count_query, params![search_filter], |row| row.get(0))?;

        let select_query = "
            SELECT mangas.id, mangas.title, mangas.manga_provider, mangas.last_read,
                   (SELECT title FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_read = 1 ORDER BY chapters.rowid DESC LIMIT 1) AS last_chapter,
                   (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_read = 1) AS read_count,
                   (SELECT title FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_bookmarked = 1 LIMIT 1) AS bookmark_title,
                   (SELECT number_page_bookmarked FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_bookmarked = 1 LIMIT 1) AS bookmark_page,
                   (SELECT COUNT(*) FROM favorite_mangas WHERE favorite_mangas.manga_id = mangas.id AND favorite_mangas.provider = mangas.manga_provider) AS is_fav,
                   (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = mangas.id AND chapters.is_downloaded = 1) AS dl_count,
                   (SELECT shelf_name FROM manga_collections WHERE manga_collections.manga_id = mangas.id AND manga_collections.provider = mangas.manga_provider LIMIT 1) AS shelf_name
            FROM mangas 
            INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
            INNER JOIN history_types ON manga_history_union.type_id = history_types.id 
            WHERE history_types.name = 'ReadingHistory' 
              AND (?1 IS NULL OR LOWER(mangas.title) LIKE '%' || ?1 || '%')
            ORDER BY mangas.last_read DESC
            LIMIT ?2 OFFSET ?3
        ";

        let mut stmt = self.connection.prepare(select_query)?;
        let rows = stmt.query_map(params![search_filter, items_per_page, offset], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let provider_str: String = row.get(2).unwrap_or_default();
            let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();
            let last_read: Option<String> = row.get(3).ok();
            let last_read_chapter: Option<String> = row.get(4).ok();
            let total_read_count: u32 = row.get(5).unwrap_or(0);
            let bookmark_chapter: Option<String> = row.get(6).ok();
            let bookmark_page: Option<u32> = row.get(7).ok();
            let is_fav_count: i32 = row.get(8).unwrap_or(0);
            let dl_count: u32 = row.get(9).unwrap_or(0);
            let shelf_name: Option<String> = row.get(10).ok().flatten();

            Ok(MangaHistory {
                id,
                title,
                provider,
                last_read,
                last_read_chapter,
                total_read_count,
                bookmark_chapter,
                bookmark_page,
                is_favorite: is_fav_count > 0,
                is_plan_to_read: false,
                downloaded_count: dl_count,
                shelf_name,
            })
        })?;

        let mut manga_history = Vec::new();
        for manga in rows {
            manga_history.push(manga?);
        }

        Ok(MangaHistoryResponse {
            mangas: manga_history,
            total_items,
            page: args.page,
        })
    }

    pub fn get_library(&self, args: GetHistoryArgs) -> rusqlite::Result<MangaHistoryResponse> {
        self.get_library_with_shelf(args, None)
    }

    pub fn get_library_with_shelf(
        &self,
        args: GetHistoryArgs,
        shelf_filter: Option<&str>,
    ) -> rusqlite::Result<MangaHistoryResponse> {
        let items_per_page = args.items_per_page;
        let offset = (args.page - 1) * items_per_page;
        let search_filter = args.search.map(|s| s.get().to_lowercase());
        let shelf_filter = shelf_filter.map(str::trim).filter(|s| !s.is_empty());

        let count_query = "
            WITH library_entries AS (
                SELECT manga_id, provider, title FROM favorite_mangas
                UNION
                SELECT mangas.id, mangas.manga_provider, mangas.title
                FROM mangas
                INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id
                INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                WHERE history_types.name = 'PlanToRead'
                UNION
                SELECT manga_id, provider, title FROM manga_collections
            )
            SELECT COUNT(*) FROM library_entries
            WHERE (?1 IS NULL OR LOWER(title) LIKE '%' || ?1 || '%')
              AND (?2 IS NULL OR EXISTS (
                    SELECT 1 FROM manga_collections
                    WHERE manga_collections.manga_id = library_entries.manga_id
                      AND manga_collections.provider = library_entries.provider
                      AND manga_collections.shelf_name = ?2
              ))
        ";
        let total_items: u32 = self
            .connection
            .query_row(count_query, params![search_filter, shelf_filter], |row| row.get(0))?;

        let select_query = "
            WITH library_entries AS (
                SELECT 
                    manga_id, 
                    provider, 
                    title, 
                    1 AS is_fav, 
                    0 AS is_plan, 
                    created_at AS added_at 
                FROM favorite_mangas
                UNION ALL
                SELECT 
                    mangas.id AS manga_id, 
                    mangas.manga_provider AS provider, 
                    mangas.title, 
                    0 AS is_fav, 
                    1 AS is_plan, 
                    mangas.created_at AS added_at
                FROM mangas
                INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id
                INNER JOIN history_types ON manga_history_union.type_id = history_types.id
                WHERE history_types.name = 'PlanToRead'
                UNION ALL
                SELECT
                    manga_id,
                    provider,
                    title,
                    0 AS is_fav,
                    0 AS is_plan,
                    created_at AS added_at
                FROM manga_collections
            )
            SELECT 
                manga_id, 
                provider, 
                title, 
                MAX(is_fav) AS is_favorite, 
                MAX(is_plan) AS is_plan_to_read,
                (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = library_entries.manga_id AND chapters.is_downloaded = 1) AS dl_count,
                (SELECT COUNT(*) FROM chapters WHERE chapters.manga_id = library_entries.manga_id AND chapters.is_read = 1) AS read_count,
                (SELECT title FROM chapters WHERE chapters.manga_id = library_entries.manga_id AND chapters.is_read = 1 ORDER BY chapters.rowid DESC LIMIT 1) AS last_chapter,
                (SELECT shelf_name FROM manga_collections WHERE manga_collections.manga_id = library_entries.manga_id AND manga_collections.provider = library_entries.provider LIMIT 1) AS shelf_name
            FROM library_entries
            WHERE (?1 IS NULL OR LOWER(title) LIKE '%' || ?1 || '%')
              AND (?2 IS NULL OR EXISTS (
                    SELECT 1 FROM manga_collections
                    WHERE manga_collections.manga_id = library_entries.manga_id
                      AND manga_collections.provider = library_entries.provider
                      AND manga_collections.shelf_name = ?2
              ))
            GROUP BY manga_id, provider
            ORDER BY is_favorite DESC, added_at DESC
            LIMIT ?3 OFFSET ?4
        ";

        let mut stmt = self.connection.prepare(select_query)?;
        let rows = stmt.query_map(params![search_filter, shelf_filter, items_per_page, offset], |row| {
            let id: String = row.get(0)?;
            let provider_str: String = row.get(1)?;
            let title: String = row.get(2)?;
            let is_fav_val: i32 = row.get(3).unwrap_or(0);
            let is_plan_val: i32 = row.get(4).unwrap_or(0);
            let dl_count: u32 = row.get(5).unwrap_or(0);
            let read_count: u32 = row.get(6).unwrap_or(0);
            let last_chapter: Option<String> = row.get(7).ok();
            let shelf_name: Option<String> = row.get(8).ok().flatten();
            let provider = provider_str.parse::<MangaProviders>().unwrap_or_default();

            Ok(MangaHistory {
                id,
                title,
                provider,
                last_read: None,
                last_read_chapter: last_chapter,
                total_read_count: read_count,
                bookmark_chapter: None,
                bookmark_page: None,
                is_favorite: is_fav_val > 0,
                is_plan_to_read: is_plan_val > 0,
                downloaded_count: dl_count,
                shelf_name,
            })
        })?;

        let mut mangas = Vec::new();
        for manga in rows {
            mangas.push(manga?);
        }

        Ok(MangaHistoryResponse {
            mangas,
            total_items,
            page: args.page,
        })
    }

    /// Removes a specific manga from all history lists.
    ///
    /// This removes the manga from both reading history and plan to read lists,
    /// but does not delete the manga or chapter records themselves.
    ///
    /// # Arguments
    ///
    /// * `manga_id` - ID of manga to remove from history
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Remove manga from user's history lists
    /// db.remove_from_history("manga_123")?;
    /// ```
    pub fn remove_from_history(&self, manga_id: &str, hist_type: MangaHistoryType) -> rusqlite::Result<()> {
        let hist_type_id = self.get_history_type(hist_type)?;

        self.connection.execute(
            "DELETE FROM manga_history_union WHERE manga_history_union.manga_id = ?1 AND manga_history_union.type_id = ?2",
            params![manga_id, hist_type_id],
        )?;

        Ok(())
    }

    /// Removes all manga of a specific provider from a specific history type.
    ///
    /// This is a bulk operation to clear entire sections of a user's library.
    /// For example, removing all MangaDx manga from reading history while
    /// leaving plan to read and other providers untouched.
    ///
    /// # Arguments
    ///
    /// * `hist_type` - Which history list to clear (ReadingHistory or PlanToRead)
    /// * `provider` - Which manga provider to remove (Mangadx, Weebcentral, etc.)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Clear all MangaDx manga from reading history
    /// db.remove_all_from_history(MangaHistoryType::ReadingHistory, MangaProviders::Mangadx)?;
    /// ```
    pub fn remove_all_from_history(&self, hist_type: MangaHistoryType, provider: MangaProviders) -> rusqlite::Result<()> {
        let history_type_id = self.get_history_type(hist_type)?;

        let get_ids_manga_to_delete_statement = r#"SELECT mangas.id from mangas 
                     INNER JOIN manga_history_union ON mangas.id = manga_history_union.manga_id 
                     WHERE manga_history_union.type_id = ?1 AND mangas.manga_provider = ?2
            "#;

        let delete_statement = format!(
            "DELETE FROM manga_history_union WHERE manga_history_union.manga_id IN ({get_ids_manga_to_delete_statement}) AND manga_history_union.type_id = ?3"
        );

        self.connection
            .execute(&delete_statement, params![history_type_id, provider.to_string(), history_type_id])?;

        Ok(())
    }
}

#[derive(Default, Debug, Clone)]
pub struct ChapterToInsert<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub manga_id: &'a str,
    pub is_read: bool,
    pub is_downloaded: bool,
    pub is_bookmarked: bool,
    pub translated_language: &'a str,
    pub number_page_bookmarked: Option<u32>,
}

/// Information needed to save a chapter bookmark.
///
/// Bookmarks allow users to save their exact reading position within a chapter,
/// including the specific page number. Only one bookmark per manga is allowed -
/// setting a new bookmark automatically removes any previous bookmark for that manga.
#[derive(Default, Debug)]
pub struct ChapterToBookmark<'a> {
    pub chapter_id: &'a str,
    pub manga_id: &'a str,
    pub chapter_title: &'a str,
    pub manga_title: &'a str,
    pub manga_cover_url: Option<&'a str>,
    pub translated_language: Languages,
    pub page_number: Option<u32>,
    pub provider: MangaProviders,
}

/// Trait for types that can create chapter bookmarks.
///
/// Implementing this trait allows a type to save user bookmarks, which track
/// the exact page where a user stopped reading within a chapter.
pub trait Bookmark {
    /// Creates or updates a bookmark for the specified chapter.
    ///
    /// If a bookmark already exists for this manga, it will be replaced.
    /// Only one bookmark per manga is allowed at a time.
    ///
    /// # Arguments
    ///
    /// * `chapter_to_bookmark` - Complete bookmark information
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    fn bookmark(&mut self, chapter_to_bookmark: ChapterToBookmark<'_>) -> Result<(), Box<dyn std::error::Error>>;
}

/// A retrieved bookmark containing the user's saved reading position.
///
/// This represents a bookmark that was previously saved and is now being
/// retrieved from the database. Contains all the information needed to
/// resume reading at the exact position where the user left off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChapterBookmarked {
    pub id: String,
    pub translated_language: Option<String>,
    pub number_page_bookmarked: Option<u32>,
    pub manga_title: String,
    pub manga_id: String,
}

/// Trait for types that can retrieve saved bookmarks.
///
/// Implementing this trait allows a type to fetch previously saved bookmarks,
/// enabling users to resume reading exactly where they left off.
pub trait RetrieveBookmark {
    /// Gets the current bookmark for a specific manga.
    ///
    /// Since only one bookmark per manga is allowed, this returns the single
    /// active bookmark if one exists.
    ///
    /// # Arguments
    ///
    /// * `manga_id` - ID of the manga to get the bookmark for
    ///
    /// # Returns
    ///
    /// - `Ok(Some(bookmark))` if a bookmark exists
    /// - `Ok(None)` if no bookmark is set for this manga
    /// - `Err(...)` if the database operation fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # let db = setup_test_db();
    /// if let Some(bookmark) = db.get_bookmarked("manga_123")? {
    ///     println!("Resume reading at page {}", bookmark.number_page_bookmarked.unwrap_or(1));
    /// }
    /// ```
    fn get_bookmarked(&self, manga_id: &str) -> Result<Option<ChapterBookmarked>, Box<dyn std::error::Error>>;
}

impl<'a> Bookmark for Database<'a> {
    /// Creates or updates a bookmark for the specified chapter.
    ///
    /// This implementation handles the complete bookmark workflow:
    /// 1. Creates manga record if it doesn't exist
    /// 2. Creates chapter record if it doesn't exist
    /// 3. Removes any existing bookmark for this manga (only one bookmark per manga allowed)
    /// 4. Sets the new bookmark with the specified page number
    ///
    /// Bookmarks are used to save the exact page where a user stopped reading,
    /// allowing them to resume exactly where they left off.
    ///
    /// # Arguments
    ///
    /// * `chapter_to_bookmark` - Complete bookmark information including manga, chapter, and page number
    ///
    /// # Errors
    ///
    /// Returns an error if any database operation fails (insert, update, etc.)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use crate::backend::database::{Database, ChapterToBookmark};
    /// # use crate::backend::manga_provider::{MangaProviders, Languages};
    /// # let mut db = setup_test_db();
    /// let bookmark_data = ChapterToBookmark {
    ///     chapter_id: "chapter_123",
    ///     manga_id: "manga_456",
    ///     chapter_title: "Chapter 1",
    ///     manga_title: "One Piece",
    ///     manga_cover_url: Some("cover.jpg"),
    ///     translated_language: Languages::English,
    ///     page_number: Some(15), // User stopped at page 15
    ///     provider: MangaProviders::Mangadx,
    /// };
    ///
    /// db.bookmark(bookmark_data)?;
    /// ```
    fn bookmark(&mut self, chapter_to_bookmark: ChapterToBookmark<'_>) -> Result<(), Box<dyn std::error::Error>> {
        Ok(self.bookmark_chapter(chapter_to_bookmark)?)
    }
}

impl<'a> RetrieveBookmark for Database<'a> {
    /// Retrieves the current bookmark for a specific manga.
    ///
    /// This implementation queries the database to find the active bookmark for the given manga.
    /// Since only one bookmark per manga is allowed, this returns the single bookmark if it exists.
    ///
    /// The bookmark contains all information needed to resume reading:
    /// - Chapter ID and title
    /// - Manga ID and title
    /// - Translated language
    /// - Exact page number where reading stopped
    ///
    /// # Arguments
    ///
    /// * `manga_id` - ID of the manga to get the bookmark for
    ///
    /// # Returns
    ///
    /// - `Ok(Some(bookmark))` if a bookmark exists for this manga
    /// - `Ok(None)` if no bookmark is set for this manga
    /// - `Err(...)` if the database query fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use crate::backend::database::Database;
    /// # let db = setup_test_db();
    /// match db.get_bookmarked("manga_456")? {
    ///     Some(bookmark) => {
    ///         println!(
    ///             "Resume reading '{}' at chapter '{}', page {}",
    ///             bookmark.manga_title,
    ///             bookmark.id,
    ///             bookmark.number_page_bookmarked.unwrap_or(1)
    ///         );
    ///     },
    ///     None => {
    ///         println!("No bookmark found for this manga");
    ///     },
    /// }
    /// ```
    fn get_bookmarked(&self, manga_id: &str) -> Result<Option<ChapterBookmarked>, Box<dyn std::error::Error>> {
        Ok(self.get_chapter_bookmarked(manga_id)?)
    }
}

#[cfg(test)]
mod test {

    use std::collections::BTreeMap;
    use std::error::Error;

    use pretty_assertions::assert_eq;
    use rusqlite::Result;
    use strum::IntoEnumIterator;
    use uuid::Uuid;

    use super::*;

    fn check_tables_exist(connection: &Connection) -> Result<()> {
        for table in Table::iter() {
            connection.query_row(
                format!("SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'").as_str(),
                [],
                |row| row.get::<_, String>(0),
            )?;
        }

        let amount_types: i32 =
            connection.query_row(format!("SELECT COUNT(*) from {}", Table::HistoryTypes).as_str(), [], |row| row.get(0))?;

        assert!(amount_types > 0, "there should be history types");

        Ok(())
    }

    struct GetChapters {
        id: String,
        title: String,
        manga_id: String,
        is_read: bool,
        is_downloaded: bool,
    }

    fn get_all_chapters(conn: &Connection) -> Result<Vec<GetChapters>> {
        let mut statement = conn.prepare(format!("SELECT * FROM {}", Table::Chapters).as_str())?;

        let mut chapters: Vec<GetChapters> = vec![];

        let chapter_rows = statement.query_map(params![], |row| {
            Ok(GetChapters {
                id: row.get(0)?,
                title: row.get(1)?,
                manga_id: row.get(2)?,
                is_read: row.get(3)?,
                is_downloaded: row.get(4)?,
            })
        })?;

        for chapter in chapter_rows {
            chapters.push(chapter?);
        }

        Ok(chapters)
    }

    #[test]
    fn downloaded_manga_and_dismissed_notifications_are_provider_scoped() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id = Uuid::new_v4().to_string();
        let downloaded_chapter_id = Uuid::new_v4().to_string();
        database.set_chapter_downloaded(SetChapterDownloaded {
            id: &downloaded_chapter_id,
            title: "Chapter 1",
            manga_id: &manga_id,
            manga_title: "Downloaded manga",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        let mangas = database.get_downloaded_mangas(MangaProviders::Mangadex)?;
        assert_eq!(mangas.len(), 1);
        assert_eq!(mangas[0].id, manga_id);
        assert_eq!(mangas[0].title, "Downloaded manga");
        assert_eq!(mangas[0].downloaded_chapter_ids, vec![downloaded_chapter_id]);
        assert!(database.get_downloaded_mangas(MangaProviders::Weebcentral)?.is_empty());

        let dismissed_chapters = vec!["chapter-1".to_string(), "chapter-2".to_string()];
        database.save_dismissed_chapter_ids(&manga_id, MangaProviders::Mangadex, &dismissed_chapters)?;
        assert_eq!(database.get_dismissed_chapter_ids(&manga_id, MangaProviders::Mangadex)?, dismissed_chapters);
        database.save_dismissed_chapter_ids(&manga_id, MangaProviders::Mangadex, &["chapter-3".into()])?;
        let all_dismissed_chapters = database.get_dismissed_chapter_ids(&manga_id, MangaProviders::Mangadex)?;
        assert_eq!(all_dismissed_chapters.len(), 3);
        assert!(all_dismissed_chapters.contains(&"chapter-3".to_string()));
        assert!(database.get_dismissed_chapter_ids(&manga_id, MangaProviders::Weebcentral)?.is_empty());

        Ok(())
    }

    #[test]
    fn get_all_downloaded_mangas_returns_mangas_across_providers() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);
        database.setup()?;

        database.set_chapter_downloaded(SetChapterDownloaded {
            id: "ch-md-1",
            title: "Chapter 1",
            manga_id: "manga-md",
            manga_title: "MangaDex Series",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;
        database.set_chapter_downloaded(SetChapterDownloaded {
            id: "ch-lc-1",
            title: "Capitulo 1",
            manga_id: "manga-lc",
            manga_title: "LeerCapitulo Series",
            img_url: None,
            provider: MangaProviders::Leercapitulo,
        })?;

        let all_downloaded = database.get_all_downloaded_mangas()?;
        assert_eq!(all_downloaded.len(), 2);
        assert!(all_downloaded.iter().any(|m| {
            m.id == "manga-md" && m.provider == MangaProviders::Mangadex && m.downloaded_chapter_ids == vec!["ch-md-1"]
        }));
        assert!(all_downloaded.iter().any(|m| {
            m.id == "manga-lc" && m.provider == MangaProviders::Leercapitulo && m.downloaded_chapter_ids == vec!["ch-lc-1"]
        }));

        Ok(())
    }

    #[test]
    fn home_stats_aggregate_library_reading_downloads_and_favorites() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);
        database.setup()?;

        assert_eq!(database.get_home_stats()?, HomeStats::default());

        database.save_history(MangaReadingHistorySave {
            id: "berserk",
            title: "Berserk",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "berserk-1",
                title: "Chapter 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        })?;
        database.save_history(MangaReadingHistorySave {
            id: "berserk",
            title: "Berserk",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "berserk-2",
                title: "Chapter 2",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        })?;
        database.set_chapter_downloaded(SetChapterDownloaded {
            id: "vagabond-1",
            title: "Chapter 1",
            manga_id: "vagabond",
            manga_title: "Vagabond",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;
        database.toggle_favorite("berserk", MangaProviders::Mangadex, "Berserk")?;
        // Ensure deterministic ordering on last_read inside an in-memory fast test.
        connection.execute("UPDATE mangas SET last_read = '2026-11-15 10:00:00' WHERE id = 'vagabond'", [])?;
        connection.execute("UPDATE mangas SET last_read = '2026-11-15 12:00:00' WHERE id = 'berserk'", [])?;

        let stats = database.get_home_stats()?;
        assert_eq!(stats.chapters_read, 3);
        assert_eq!(stats.downloaded_chapters, 1);
        assert_eq!(stats.favorites, 1);
        assert_eq!(stats.library_mangas, 2);
        assert_eq!(stats.last_read_title.as_deref(), Some("Berserk"));

        Ok(())
    }

    #[test]
    fn favorites_are_provider_scoped_and_toggleable() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id = "one-piece";
        assert!(!database.is_favorite(manga_id, MangaProviders::Mangadex)?);

        // Toggle on
        let is_fav = database.toggle_favorite(manga_id, MangaProviders::Mangadex, "One Piece")?;
        assert!(is_fav);
        assert!(database.is_favorite(manga_id, MangaProviders::Mangadex)?);
        // Provider scoped:
        assert!(!database.is_favorite(manga_id, MangaProviders::Inmanga)?);

        let fav_ids = database.get_favorite_manga_ids(MangaProviders::Mangadex)?;
        assert_eq!(fav_ids.len(), 1);
        assert!(fav_ids.contains(manga_id));

        // Toggle off
        let is_fav = database.toggle_favorite(manga_id, MangaProviders::Mangadex, "One Piece")?;
        assert!(!is_fav);
        assert!(!database.is_favorite(manga_id, MangaProviders::Mangadex)?);
        assert!(database.get_favorite_manga_ids(MangaProviders::Mangadex)?.is_empty());

        Ok(())
    }

    #[test]
    fn test_get_all_favorite_mangas_and_downloaded_chapter_ids() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);
        database.setup()?;

        database.toggle_favorite("op-dex", MangaProviders::Mangadex, "One Piece Dex")?;
        database.toggle_favorite("naruto-inm", MangaProviders::Inmanga, "Naruto Inmanga")?;

        let all_favs = database.get_all_favorite_mangas()?;
        assert_eq!(all_favs.len(), 2);
        assert!(all_favs.iter().any(|f| f.manga_id == "op-dex" && f.provider == MangaProviders::Mangadex));
        assert!(
            all_favs
                .iter()
                .any(|f| f.manga_id == "naruto-inm" && f.provider == MangaProviders::Inmanga)
        );

        database.set_chapter_downloaded(SetChapterDownloaded {
            id: "ch-1",
            title: "Cap 1",
            manga_id: "naruto-inm",
            manga_title: "Naruto Inmanga",
            img_url: None,
            provider: MangaProviders::Inmanga,
        })?;

        let downloaded_ids = database.get_downloaded_chapter_ids("naruto-inm", MangaProviders::Inmanga)?;
        assert_eq!(downloaded_ids, vec!["ch-1"]);
        assert!(database.get_downloaded_chapter_ids("naruto-inm", MangaProviders::Mangadex)?.is_empty());

        Ok(())
    }

    #[test]
    fn database_is_initialized() -> Result<()> {
        let connection = Connection::open_in_memory()?;

        let database = Database::new(&connection);

        database.setup().expect("could not setup the database");

        check_tables_exist(&connection)?;

        Ok(())
    }

    #[test]
    fn insert_manga_and_chapter() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id = Uuid::new_v4().to_string();

        let chapter_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.create_chapter_if_not_exists(ChapterToInsert {
            id: &chapter_id,
            title: "some_title",
            manga_id: &manga_id,
            is_read: true,
            is_downloaded: false,
            is_bookmarked: false,
            number_page_bookmarked: None,
            translated_language: "en",
        })?;

        Ok(())
    }

    #[test]
    fn check_chapter_is_already_reading() -> Result<()> {
        let conn = Connection::open_in_memory()?;

        let database = Database::new(&conn);

        database.setup()?;

        let manga_id = Uuid::new_v4().to_string();

        let chapter_id_not_read = Uuid::new_v4().to_string();
        let chapter_id_is_read = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        conn.execute("INSERT INTO chapters(id, title, manga_id) VALUES(?1, ?2, ?3)", params![
            chapter_id_not_read.clone(),
            "some_title",
            manga_id.clone(),
        ])?;

        conn.execute("INSERT INTO chapters(id, is_read, title, manga_id) VALUES(?1, ?2, ?3, ?4)", params![
            chapter_id_is_read.clone(),
            true,
            "some_title",
            manga_id,
        ])?;

        let non_existent = database.check_chapter_is_already_reading("non_existent")?;
        let is_already_reading = database.check_chapter_is_already_reading(&chapter_id_is_read)?;
        let not_reading = database.check_chapter_is_already_reading(&chapter_id_not_read)?;

        assert!(is_already_reading);
        assert!(!non_existent);
        assert!(!not_reading);

        Ok(())
    }

    #[test]
    fn save_manga_plan_to_read_which_does_not_exist() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;
        let manga_id = Uuid::new_v4().to_string();

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        let manga_was_saved = database.check_exists(&manga_id, Table::Mangas)?;

        assert!(manga_was_saved, "manga should have been saved");

        let manga_is_plan_to_read = database.manga_is_plan_to_read(&manga_id)?;

        assert!(manga_is_plan_to_read, "the manga was not plan to read");

        Ok(())
    }

    #[test]
    fn save_already_existing_manga_plan_to_read() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;
        let manga_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        let manga_should_not_be_plan_to_read = database.manga_is_plan_to_read(&manga_id)?;

        assert!(!manga_should_not_be_plan_to_read);

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        let manga_should_be_plan_to_read = database.manga_is_plan_to_read(&manga_id)?;

        assert!(manga_should_be_plan_to_read);

        Ok(())
    }

    // Both manga and chapter are not in the database
    #[test]
    fn save_manga_reading_status_which_does_not_exist() -> Result<()> {
        let connection = Connection::open_in_memory()?;

        let database = Database::new(&connection);

        database.setup()?;

        let manga_id = Uuid::new_v4().to_string();
        let chapter_id = Uuid::new_v4().to_string();

        database.save_history(MangaReadingHistorySave {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: &chapter_id,
                ..Default::default()
            },
            provider: MangaProviders::Weebcentral,
        })?;

        let manga_was_created = database.check_exists(&manga_id, Table::Mangas)?;
        let chapter_was_created = database.check_exists(&chapter_id, Table::Chapters)?;

        assert!(manga_was_created);

        assert!(chapter_was_created);

        Ok(())
    }

    // manga is already in database, chapter isnt
    #[test]
    fn save_manga_reading_status_which_already_exists() -> Result<()> {
        let connection = Connection::open_in_memory()?;

        let database = Database::new(&connection);

        database.setup()?;

        let manga_id = Uuid::new_v4().to_string();
        let chapter_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_title",
            img_url: None,

            provider: MangaProviders::Weebcentral,
        })?;

        database.save_history(MangaReadingHistorySave {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: &chapter_id,
                ..Default::default()
            },

            provider: MangaProviders::Weebcentral,
        })?;

        let chapters = get_all_chapters(&connection)?;

        let saved_chapter = chapters
            .iter()
            .find(|chap| chap.id == chapter_id)
            .expect("no chapter was saved as being read");

        assert!(saved_chapter.is_read);

        Ok(())
    }

    #[test]
    fn save_manga_reading_both_manga_and_chapter_exist_and_chapter_is_already_reading() -> Result<()> {
        let connection = Database::get_connection()?;

        let database = Database::new(&connection);
        database.setup()?;
        let manga_id = Uuid::new_v4().to_string();
        let chapter_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        let chapter_which_is_already_reading = ChapterToInsert {
            id: &chapter_id,
            title: "some_title",
            manga_id: &manga_id,
            is_read: true,
            is_downloaded: true,
            is_bookmarked: false,
            number_page_bookmarked: None,
            translated_language: "en",
        };

        database.create_chapter_if_not_exists(chapter_which_is_already_reading.clone())?;

        database
            .save_history(MangaReadingHistorySave {
                id: &manga_id,
                title: chapter_which_is_already_reading.title,
                img_url: None,
                chapter: ChapterToSaveHistory {
                    id: &chapter_id,
                    ..Default::default()
                },
                provider: MangaProviders::Weebcentral,
            })
            .expect("could not save chapter history");

        let chapters = get_all_chapters(&connection)?;

        let saved_chapter = chapters
            .iter()
            .find(|chap| chap.id == chapter_id)
            .expect("no chapter was saved as being read");

        // saving reading status should not have overwritten its donwload status
        assert!(saved_chapter.is_downloaded);
        assert!(saved_chapter.is_read);

        Ok(())
    }

    #[test]
    fn get_chapters_which_have_reading_status() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id = Uuid::new_v4().to_string();
        let chapter_id_not_read = Uuid::new_v4().to_string();
        let chapter_id_read = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_manga",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.create_chapter_if_not_exists(ChapterToInsert {
            id: &chapter_id_not_read,
            title: "some_chapter",
            manga_id: &manga_id,
            is_read: false,
            is_downloaded: false,
            ..Default::default()
        })?;

        database.create_chapter_if_not_exists(ChapterToInsert {
            id: &chapter_id_read,
            title: "some_chapter",
            manga_id: &manga_id,
            is_read: true,
            is_downloaded: false,
            ..Default::default()
        })?;

        let chapters = database.get_chapters_history_status(&manga_id)?;

        assert!(!chapters.is_empty());

        let mut chapters = chapters.into_iter();

        let first_chapter = chapters.next().unwrap();

        assert_eq!(chapter_id_not_read, first_chapter.id);
        assert!(!first_chapter.is_read);

        let second_chapter = chapters.next().unwrap();

        assert_eq!(chapter_id_read, second_chapter.id);
        assert!(second_chapter.is_read);

        Ok(())
    }

    #[test]
    fn get_manga_history_reading_with_no_search_term() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_is_in_reading_history_id = Uuid::new_v4().to_string();
        let manga_not_in_reading_history_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_is_in_reading_history_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.insert_manga_in_reading_history(&manga_is_in_reading_history_id)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_not_in_reading_history_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        let history = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 100,
            provider: MangaProviders::Weebcentral,
        })?;

        assert!(history.total_items > 0);

        assert!(history.mangas.iter().any(|manga| manga.id == manga_is_in_reading_history_id));

        assert!(!history.mangas.iter().any(|manga| manga.id == manga_not_in_reading_history_id));

        Ok(())
    }

    #[test]
    fn get_manga_history_reading_of_provider() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id_mangadex = Uuid::new_v4().to_string();
        let manga_id_weebcentral = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_weebcentral,
            title: "of Weebcentral",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.insert_manga_in_reading_history(&manga_id_weebcentral)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_mangadex,
            title: "of mangadex",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex)?;

        // No search term
        let history = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 100,
            provider: MangaProviders::Weebcentral,
        })?;

        // There are 2 mangas but of Weebcentral there is only one
        assert_eq!(1, history.total_items);

        assert!(!history.mangas.iter().any(|manga| manga.id == manga_id_mangadex));

        assert!(history.mangas.iter().any(|manga| manga.id == manga_id_weebcentral));

        let history = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: SearchTerm::trimmed_lowercased("Weebcentral"),
            items_per_page: 100,
            provider: MangaProviders::Mangadex,
        })?;

        // Should be 0 because it is requesting mangas from mangadex
        assert_eq!(0, history.total_items);

        assert!(!history.mangas.iter().any(|manga| manga.id == manga_id_mangadex));
        assert!(!history.mangas.iter().any(|manga| manga.id == manga_id_weebcentral));

        Ok(())
    }

    #[test]
    fn get_manga_history_reading_with_search_term() -> Result<()> {
        let connection = Database::get_connection()?;

        let database = Database::new(&connection);
        database.setup()?;
        let manga_id_filtered_out = Uuid::new_v4().to_string();
        let manga_id_included_in_search = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_filtered_out,
            title: "filtered_out",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.insert_manga_in_reading_history(&manga_id_filtered_out)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_included_in_search,
            title: "included",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.insert_manga_in_reading_history(&manga_id_included_in_search)?;

        let history = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: SearchTerm::trimmed_lowercased("Included"),
            items_per_page: 100,
            provider: MangaProviders::Weebcentral,
        })?;

        assert!(history.total_items > 0);

        assert!(!history.mangas.iter().any(|manga| manga.id == manga_id_filtered_out));
        assert!(history.mangas.iter().any(|manga| manga.id == manga_id_included_in_search));

        Ok(())
    }

    #[test]
    fn get_manga_planned_to_read_with_search_term() -> Result<()> {
        let connection = Database::get_connection()?;

        let database = Database::new(&connection);
        database.setup()?;
        let manga_id_filtered_out = Uuid::new_v4().to_string();
        let manga_id_included_in_search = Uuid::new_v4().to_string();

        let manga_filtered_out = MangaPlanToReadSave {
            id: &manga_id_filtered_out,
            title: "filtered_out",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        };

        let manga_included = MangaPlanToReadSave {
            id: &manga_id_included_in_search,
            title: "included",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        };

        database.save_plan_to_read(manga_filtered_out)?;

        database.save_plan_to_read(manga_included)?;

        let history = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::PlanToRead,
            page: 1,
            search: SearchTerm::trimmed_lowercased("Included"),
            items_per_page: 100,
            provider: MangaProviders::Weebcentral,
        })?;

        assert!(history.total_items > 0);

        assert!(!history.mangas.iter().any(|manga| manga.id == manga_id_filtered_out));
        assert!(history.mangas.iter().any(|manga| manga.id == manga_id_included_in_search));

        Ok(())
    }

    #[test]
    fn get_manga_planned_to_read() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id_1 = Uuid::new_v4().to_string();
        let manga_id_2 = Uuid::new_v4().to_string();

        let manga_1 = MangaPlanToReadSave {
            id: &manga_id_1,
            title: "manga_1",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        };

        let manga_2 = MangaPlanToReadSave {
            id: &manga_id_2,
            title: "manga_2",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        };

        database.save_plan_to_read(manga_1)?;

        database.save_plan_to_read(manga_2)?;

        let history = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::PlanToRead,
            page: 1,
            search: None,
            items_per_page: 100,
            provider: MangaProviders::Weebcentral,
        })?;

        assert!(history.total_items > 0);

        assert!(history.mangas.iter().any(|manga| manga.id == manga_id_1));
        assert!(history.mangas.iter().any(|manga| manga.id == manga_id_2));

        Ok(())
    }

    // Test the case when a manga is not in the database and a chapters is not in the database
    // either
    #[test]
    fn save_chapter_download_status_manga_doesnt_exist_and_chapter_doesnt_exist() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let chapter_id = Uuid::new_v4().to_string();
        let manga_id = Uuid::new_v4().to_string();

        database.set_chapter_downloaded(SetChapterDownloaded {
            id: &chapter_id,
            title: "some_title",
            manga_id: &manga_id,
            manga_title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        assert!(database.check_exists(&manga_id, Table::Mangas)?);

        let chapters = get_all_chapters(&connection)?;

        let chapter_downloaded = chapters
            .iter()
            .find(|chap| chap.id == chapter_id)
            .expect("chapter downloaded was nost found");

        assert!(chapter_downloaded.is_read);
        assert!(chapter_downloaded.is_downloaded);

        Ok(())
    }

    // Test the case when both manga and chapter already exist in database
    #[test]
    fn save_chapter_download_status_manga_and_chapter_exists() -> Result<()> {
        let connection = Database::get_connection()?;

        let database = Database::new(&connection);
        database.setup()?;
        let chapter_id_exist_in_database = Uuid::new_v4().to_string();
        let manga_id_exist_in_database = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_exist_in_database,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.create_chapter_if_not_exists(ChapterToInsert {
            id: &chapter_id_exist_in_database,
            title: "some_title",
            manga_id: &manga_id_exist_in_database,
            is_read: false,
            is_downloaded: false,
            ..Default::default()
        })?;

        database.set_chapter_downloaded(SetChapterDownloaded {
            id: &chapter_id_exist_in_database,
            title: "some_title",
            manga_id: &manga_id_exist_in_database,
            manga_title: "some_title",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        let chapters = get_all_chapters(&connection)?;

        let chapter_downloaded = chapters
            .iter()
            .find(|chap| chap.id == chapter_id_exist_in_database)
            .expect("chapter downloaded was nost found");

        assert!(chapter_downloaded.is_read);
        assert!(chapter_downloaded.is_downloaded);

        Ok(())
    }

    #[test]
    fn save_chapter_download_status_manga_exist_and_chapter_doesnt_exist() -> Result<()> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let chapter_id = Uuid::new_v4().to_string();
        let manga_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.set_chapter_downloaded(SetChapterDownloaded {
            id: &chapter_id,
            title: "some_title",
            manga_id: &manga_id,
            manga_title: "some_title",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        let chapters = get_all_chapters(&connection)?;

        let chapter_downloaded = chapters
            .iter()
            .find(|chap| chap.id == chapter_id)
            .expect("chapter downloaded was nost found");

        assert!(chapter_downloaded.is_read);
        assert!(chapter_downloaded.is_downloaded);

        Ok(())
    }

    #[test]
    fn database_bookmarks_chapter() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let mut database = Database::new(&connection);

        database.setup()?;

        let chapter_id = Uuid::new_v4().to_string();
        let manga_id = Uuid::new_v4().to_string();

        connection.execute("INSERT INTO mangas(id, title) VALUES(?1,?2)", params![manga_id.clone(), "some_title"])?;

        connection.execute("INSERT INTO chapters(id, title, manga_id) VALUES(?1,?2,?3)", params![
            chapter_id.clone(),
            "some_title",
            manga_id
        ])?;

        let chapter_to_bookmark1: ChapterToBookmark = ChapterToBookmark {
            chapter_id: &chapter_id,
            manga_id: &manga_id,
            page_number: Some(3),
            ..Default::default()
        };

        database.bookmark(chapter_to_bookmark1).expect("failed to bookmark chapter");

        let was_bookmarked: bool =
            connection.query_row("SELECT is_bookmarked FROM chapters WHERE id = ?1", params![chapter_id], |row| row.get(0))?;

        let page_set: Option<u32> =
            connection
                .query_row("SELECT number_page_bookmarked FROM chapters WHERE id = ?1", params![chapter_id], |row| row.get(0))?;

        assert!(was_bookmarked);
        assert_eq!(page_set.expect("should not be null"), 3);

        let chapter_to_bookmark1: ChapterToBookmark = ChapterToBookmark {
            chapter_id: &chapter_id,
            manga_id: &manga_id,
            page_number: None,
            ..Default::default()
        };

        database.bookmark(chapter_to_bookmark1).expect("failed to bookmark chapter");

        let page_set_to_none: Option<u32> =
            connection
                .query_row("SELECT number_page_bookmarked FROM chapters WHERE id = ?1", params![chapter_id], |row| row.get(0))?;

        assert!(page_set_to_none.is_none());
        Ok(())
    }

    #[test]
    fn database_only_bookmarks_one_chapter_at_a_time_per_manga() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let mut database = Database::new(&connection);

        database.setup()?;

        let chapter_id = Uuid::new_v4().to_string();
        let chapter_id_2 = Uuid::new_v4().to_string();
        let chapter_id_should_stay_bookmarked = Uuid::new_v4().to_string();
        let manga_id = Uuid::new_v4().to_string();
        let manga_id_2 = Uuid::new_v4().to_string();

        connection.execute("INSERT INTO mangas(id, title) VALUES(?1,?2)", params![manga_id.clone(), "some_title"])?;
        connection.execute("INSERT INTO mangas(id, title) VALUES(?1,?2)", params![manga_id_2.clone(), "some_title2"])?;

        connection.execute("INSERT INTO chapters(id, title, manga_id) VALUES(?1,?2,?3)", params![
            chapter_id.clone(),
            "some_title",
            manga_id
        ])?;

        connection.execute("INSERT INTO chapters(id, title, manga_id) VALUES(?1,?2,?3)", params![
            chapter_id_2.clone(),
            "some_title",
            manga_id
        ])?;

        connection.execute("INSERT INTO chapters(id, title, manga_id) VALUES(?1,?2,?3)", params![
            chapter_id_should_stay_bookmarked.clone(),
            "some_title",
            manga_id_2
        ])?;

        let chapter_to_bookmark1: ChapterToBookmark = ChapterToBookmark {
            chapter_id: &chapter_id,
            manga_id: &manga_id,
            page_number: None,
            ..Default::default()
        };

        database.bookmark_chapter(chapter_to_bookmark1).expect("failed to bookmark chapter1");

        let chapter_to_bookmark_should_stay_bookmarked: ChapterToBookmark = ChapterToBookmark {
            chapter_id: &chapter_id_should_stay_bookmarked,
            manga_id: &manga_id_2,
            page_number: None,
            ..Default::default()
        };

        database
            .bookmark_chapter(chapter_to_bookmark_should_stay_bookmarked)
            .expect("failed to bookmark chapter_id_should_stay_bookmarked");

        let chapter_to_bookmark2 = ChapterToBookmark {
            chapter_id: &chapter_id_2,
            manga_id: &manga_id,
            page_number: None,
            ..Default::default()
        };

        database.bookmark_chapter(chapter_to_bookmark2).expect("failed to bookmark chapter2");

        let was_bookmarked_1: bool =
            connection.query_row("SELECT is_bookmarked FROM chapters WHERE id = ?1", params![chapter_id], |row| row.get(0))?;

        let was_bookmarked_2: bool =
            connection.query_row("SELECT is_bookmarked FROM chapters WHERE id = ?1", params![chapter_id_2], |row| row.get(0))?;

        let should_stay_bookmarked: bool = connection.query_row(
            "SELECT is_bookmarked FROM chapters WHERE id = ?1",
            params![chapter_id_should_stay_bookmarked],
            |row| row.get(0),
        )?;

        assert!(!was_bookmarked_1);
        assert!(was_bookmarked_2);
        assert!(should_stay_bookmarked);

        Ok(())
    }

    #[test]
    fn it_inserts_manga_and_chapter_if_it_does_not_exists() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);

        database.setup()?;

        let id_manga = Uuid::new_v4().to_string();
        let chapter_id = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &id_manga,
            title: "some_title",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        let id_was_created: String = connection
            .query_row("SELECT id from mangas WHERE id = ?1", params![id_manga], |row| row.get(0))
            .expect("manga was not created");

        assert_eq!(id_was_created, id_manga);

        database
            .create_manga_if_not_exists(MangaInsert {
                id: &id_manga,
                title: "some_title",
                img_url: None,
                provider: MangaProviders::Weebcentral,
            })
            .expect("should not try to create already existing manga");

        database
            .create_chapter_if_not_exists(ChapterToInsert {
                id: &chapter_id,
                title: "some_title",
                manga_id: &id_manga,
                ..Default::default()
            })
            .expect("should create chapter");

        let id_chapter_was_created: String = connection
            .query_row("SELECT id from chapters WHERE id = ?1", params![chapter_id], |row| row.get(0))
            .expect("chapter was not created");

        assert_eq!(chapter_id, id_chapter_was_created);

        database
            .create_chapter_if_not_exists(ChapterToInsert {
                id: &chapter_id,
                title: "some_title",
                manga_id: &id_manga,
                ..Default::default()
            })
            .expect("should try to create chapter already existing");

        Ok(())
    }

    #[test]
    fn it_bookmarks_chapter_if_it_does_not_exits_in_database() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let mut database = Database::new(&connection);

        database.setup()?;

        let chapter_id = Uuid::new_v4().to_string();
        let manga_id = Uuid::new_v4().to_string();

        let chapter_to_bookmark = ChapterToBookmark {
            chapter_id: &chapter_id,
            manga_id: &manga_id,
            page_number: None,
            ..Default::default()
        };

        database.bookmark_chapter(chapter_to_bookmark).expect("failed to bookmark chapter");

        let was_bookmarked: bool = connection
            .query_row("SELECT is_bookmarked FROM chapters WHERE id = ?1", params![chapter_id], |row| row.get(0))
            .expect("chapter was not created");

        assert!(was_bookmarked);

        Ok(())
    }

    #[test]
    fn database_gets_chapter_bookmarked() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let database = Database::new(&connection);

        database.setup()?;

        let chapter_id = Uuid::new_v4().to_string();
        let manga_id = Uuid::new_v4().to_string();

        connection.execute("INSERT INTO mangas(id, title) VALUES(?1, ?2)", params![manga_id.clone(), "some_title"])?;

        connection.execute("INSERT INTO chapters(id, title, manga_id) VALUES(?1,?2,?3)", params![
            chapter_id.clone(),
            "some_title",
            manga_id
        ])?;

        let expected: ChapterBookmarked = ChapterBookmarked {
            id: "bookmarked".to_string(),
            translated_language: Some("en".to_string()),
            number_page_bookmarked: Some(2),
            manga_id: manga_id.clone(),
            manga_title: "some_title".to_string(),
        };

        connection.execute(
            "INSERT INTO chapters(id, title, manga_id, translated_language, number_page_bookmarked, is_bookmarked) VALUES(?1,?2,?3,?4,?5,?6)",
            params![expected.id, "some_title", manga_id, expected.translated_language, expected.number_page_bookmarked, true],
        )?;

        let result = database.get_bookmarked(&manga_id).expect("should be ok").expect("should not be none");

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn it_deletes_a_manga_from_reading_history() -> Result<(), Box<dyn Error>> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id_mangadex = Uuid::new_v4().to_string();
        let manga_id_mangadex2 = Uuid::new_v4().to_string();

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_mangadex,
            title: "of mangadex 1",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_mangadex2,
            title: "of mangadex 2",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex2)?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* at this point 2 mangas must be stored in reading history */
        assert_eq!(expected.mangas.len(), 2);

        database.remove_from_history(&manga_id_mangadex, MangaHistoryType::ReadingHistory)?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* now only one should exist */
        assert_eq!(expected.mangas.len(), 1);

        Ok(())
    }

    #[test]
    fn it_deletes_a_manga_from_plan_to_read() -> Result<(), Box<dyn Error>> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id_mangadex = Uuid::new_v4().to_string();
        let manga_id_mangadex2 = Uuid::new_v4().to_string();

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id_mangadex,
            title: "of mangadex 1",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex)?;

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id_mangadex2,
            title: "of mangadex 2",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex2)?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::PlanToRead,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* at this point 2 mangas must be stored in reading history */
        assert_eq!(expected.mangas.len(), 2);

        database.remove_from_history(&manga_id_mangadex, MangaHistoryType::PlanToRead)?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::PlanToRead,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* now only one should exist */
        assert_eq!(expected.mangas.len(), 1);

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* the reading history should be ketp the same */
        assert_eq!(expected.mangas.len(), 2);

        Ok(())
    }

    #[test]
    fn it_delets_all_mangas_from_history() -> Result<(), Box<dyn Error>> {
        let connection = Database::get_connection()?;
        let database = Database::new(&connection);
        database.setup()?;

        let manga_id_mangadex = Uuid::new_v4().to_string();
        let manga_id_mangadex2 = Uuid::new_v4().to_string();
        let manga_id_mangadex3 = Uuid::new_v4().to_string();

        let manga_id_plan_to_read = Uuid::new_v4().to_string();
        let manga_id_plan_to_read2 = Uuid::new_v4().to_string();

        let manga_id_weeb_central = Uuid::new_v4().to_string();
        let manga_id_weeb_centra2 = Uuid::new_v4().to_string();

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id_mangadex,
            title: "of mangadex 1",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex)?;

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id_mangadex2,
            title: "of mangadex 2",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex2)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_mangadex3,
            title: "of mangadex 3",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.insert_manga_in_reading_history(&manga_id_mangadex3)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_weeb_central,
            title: "of weebcentral",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.insert_manga_in_reading_history(&manga_id_weeb_central)?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_weeb_centra2,
            title: "of weebcentral #2",
            img_url: None,
            provider: MangaProviders::Weebcentral,
        })?;

        database.insert_manga_in_reading_history(&manga_id_weeb_centra2)?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* at this point 3 mangas must be stored in reading history */
        assert_eq!(expected.mangas.len(), 3);

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_plan_to_read,
            title: "of mangadex 1",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id_plan_to_read,
            title: "of mangadex 1 plan to read",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.create_manga_if_not_exists(MangaInsert {
            id: &manga_id_plan_to_read2,
            title: "of mangadex 1",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        database.save_plan_to_read(MangaPlanToReadSave {
            id: &manga_id_plan_to_read2,
            title: "of mangadex 2 plan to read",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::PlanToRead,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* at this point 4 mangas in plant to read should exist */
        assert_eq!(expected.mangas.len(), 4);

        database.remove_all_from_history(MangaHistoryType::ReadingHistory, MangaProviders::Mangadex)?;

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* now none from mangadex should exist */
        assert_eq!(expected.mangas.len(), 0);

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::ReadingHistory,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Weebcentral,
        })?;

        /* weebcentral reading history should remain untouched */
        assert_eq!(expected.mangas.len(), 2);

        let expected = database.get_history(GetHistoryArgs {
            hist_type: MangaHistoryType::PlanToRead,
            page: 1,
            search: None,
            items_per_page: 10,
            provider: MangaProviders::Mangadex,
        })?;

        /* the plan to read history should remain untouched */
        assert_eq!(expected.mangas.len(), 4);

        Ok(())
    }

    #[test]
    fn test_search_history() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        database.save_search_history("One Piece")?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        database.save_search_history("Naruto")?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        database.save_search_history("Bleach")?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        // Re-searching "One Piece" bumps it to the top
        database.save_search_history("One Piece")?;

        let recent = database.get_recent_searches(10)?;
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0], "One Piece");
        assert_eq!(recent[1], "Bleach");
        assert_eq!(recent[2], "Naruto");

        database.clear_search_history()?;
        let recent_empty = database.get_recent_searches(10)?;
        assert_eq!(recent_empty.len(), 0);

        Ok(())
    }

    #[test]
    fn test_normalize_and_extract_chapter_numbers() {
        assert_eq!(normalize_chapter_number("01"), "1");
        assert_eq!(normalize_chapter_number("1.0"), "1");
        assert_eq!(normalize_chapter_number("12.50"), "12.5");

        assert_eq!(extract_normalized_chapter_number("Capítulo 05: El inicio").as_deref(), Some("5"));
        assert_eq!(extract_normalized_chapter_number("Chapter 12.5 - Bonus").as_deref(), Some("12.5"));
        assert_eq!(extract_normalized_chapter_number("Ch. 100").as_deref(), Some("100"));
        assert_eq!(extract_normalized_chapter_number("42").as_deref(), Some("42"));
    }

    #[test]
    fn test_get_read_chapter_numbers_by_manga_title() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        database.save_history(MangaReadingHistorySave {
            id: "inmanga_op",
            title: "One Piece",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "inmanga_ch_1",
                title: "Capítulo 01: Romance Dawn",
                translated_language: "es",
            },
            provider: MangaProviders::Inmanga,
        })?;

        database.save_history(MangaReadingHistorySave {
            id: "inmanga_op",
            title: "One Piece",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "inmanga_ch_2",
                title: "Chapter 2.5",
                translated_language: "es",
            },
            provider: MangaProviders::Inmanga,
        })?;

        let read_nums = database.get_read_chapter_numbers_by_manga_title("one piece")?;
        assert!(read_nums.contains("1"));
        assert!(read_nums.contains("2.5"));
        assert!(!read_nums.contains("3"));

        Ok(())
    }

    #[test]
    fn toggle_chapter_read_marks_and_unmarks_chapter() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        let save_data = MangaReadingHistorySave {
            id: "manga_toggle_1",
            title: "Dandadan",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "ch_toggle_1",
                title: "Capítulo 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        };

        assert!(!database.check_chapter_is_already_reading("ch_toggle_1")?);

        let now_read = database.toggle_chapter_read(MangaReadingHistorySave {
            id: save_data.id,
            title: save_data.title,
            img_url: save_data.img_url,
            chapter: ChapterToSaveHistory {
                id: save_data.chapter.id,
                title: save_data.chapter.title,
                translated_language: save_data.chapter.translated_language,
            },
            provider: save_data.provider,
        })?;
        assert!(now_read);
        assert!(database.check_chapter_is_already_reading("ch_toggle_1")?);

        let now_read_again = database.toggle_chapter_read(save_data)?;
        assert!(!now_read_again);
        assert!(!database.check_chapter_is_already_reading("ch_toggle_1")?);

        Ok(())
    }

    #[test]
    fn test_get_recently_read_user_mangas_orders_by_last_read() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        for idx in 1..=3 {
            let chap_id = format!("older_ch_{idx}");
            let chap_title = format!("Chapter {idx}");
            database.save_history(MangaReadingHistorySave {
                id: "older_many",
                title: "Older Many Caps",
                img_url: None,
                chapter: ChapterToSaveHistory {
                    id: &chap_id,
                    title: &chap_title,
                    translated_language: "es",
                },
                provider: MangaProviders::Mangaoni,
            })?;
        }

        std::thread::sleep(std::time::Duration::from_millis(5));

        database.save_history(MangaReadingHistorySave {
            id: "newer_few",
            title: "Newer Few Caps",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "newer_ch_1",
                title: "Chapter 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangaoni,
        })?;

        let list = database.get_recently_read_user_mangas(12)?;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, "Newer Few Caps");
        assert_eq!(list[0].2, 1);
        assert_eq!(list[1].0, "Older Many Caps");
        assert_eq!(list[1].2, 3);

        // Re-reading an already-read chapter bumps `last_read` without inflating count
        std::thread::sleep(std::time::Duration::from_millis(5));
        database.save_history(MangaReadingHistorySave {
            id: "older_many",
            title: "Older Many Caps",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "older_ch_1",
                title: "Chapter 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangaoni,
        })?;

        let list_after_reread = database.get_recently_read_user_mangas(12)?;
        assert_eq!(list_after_reread[0].0, "Older Many Caps");
        assert_eq!(list_after_reread[0].2, 3);
        assert_eq!(list_after_reread[1].0, "Newer Few Caps");

        // Transitioning from PlanToRead to ReadingHistory updates `last_read`
        database.save_plan_to_read(MangaPlanToReadSave {
            id: "planned_first",
            title: "Planned First",
            img_url: None,
            provider: MangaProviders::Mangadex,
        })?;
        conn.execute("UPDATE mangas SET last_read = '2020-01-01 00:00:00' WHERE id = 'planned_first'", [])?;

        std::thread::sleep(std::time::Duration::from_millis(5));
        database.save_history(MangaReadingHistorySave {
            id: "planned_first",
            title: "Planned First",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "planned_ch_1",
                title: "Chapter 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        })?;

        let list_after_planned = database.get_recently_read_user_mangas(12)?;
        assert_eq!(list_after_planned.len(), 3);
        assert_eq!(list_after_planned[0].0, "Planned First");
        assert_eq!(list_after_planned[0].2, 1);

        Ok(())
    }

    #[test]
    fn test_search_history_case_insensitive_dedup() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        database.save_search_history("Boku no kokoro")?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        database.save_search_history("Vagabond")?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        database.save_search_history("boku no kokoro")?;

        let recent = database.get_recent_searches(10)?;
        assert_eq!(recent, vec!["boku no kokoro".to_string(), "Vagabond".to_string()]);

        Ok(())
    }

    #[test]
    fn test_custom_shelves_and_reading_dashboard_stats() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        assert_eq!(database.get_manga_shelf("berserk_1", MangaProviders::Mangaoni)?, None);

        let shelf1 = database.cycle_manga_shelf("berserk_1", MangaProviders::Mangaoni, "Berserk")?;
        assert_eq!(shelf1.as_deref(), Some(PRESET_SHELVES[0]));
        assert_eq!(database.get_manga_shelf("berserk_1", MangaProviders::Mangaoni)?.as_deref(), Some(PRESET_SHELVES[0]));

        // Library filtered by shelf returns the manga even if not favorited
        let filtered = database.get_library_with_shelf(
            GetHistoryArgs {
                hist_type: MangaHistoryType::PlanToRead,
                page: 1,
                search: None,
                items_per_page: 10,
                provider: MangaProviders::Mangaoni,
            },
            Some(PRESET_SHELVES[0]),
        )?;
        assert_eq!(filtered.total_items, 1);
        assert_eq!(filtered.mangas[0].title, "Berserk");
        assert_eq!(filtered.mangas[0].shelf_name.as_deref(), Some(PRESET_SHELVES[0]));

        // Save reading history so dashboard stats reflect read chapters, streak, and top mangas
        database.save_history(MangaReadingHistorySave {
            id: "berserk_1",
            title: "Berserk",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "b_ch_1",
                title: "Cap 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangaoni,
        })?;
        database.save_history(MangaReadingHistorySave {
            id: "berserk_1",
            title: "Berserk",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "b_ch_2",
                title: "Cap 2",
                translated_language: "es",
            },
            provider: MangaProviders::Mangaoni,
        })?;

        let stats = database.get_reading_dashboard_stats()?;
        assert_eq!(stats.total_chapters_read, 2);
        assert_eq!(stats.estimated_reading_minutes, 12);
        assert_eq!(stats.reading_mangas, 1);
        assert_eq!(stats.shelves_count, 1);
        assert_eq!(stats.current_streak_days, 1);
        assert_eq!(stats.read_last_7_days, 2);
        assert_eq!(stats.top_read_mangas.len(), 1);
        assert_eq!(stats.top_read_mangas[0], ("Berserk".to_string(), MangaProviders::Mangaoni, 2));

        // Cloud sync snapshot exports and merges collections
        let snap = database.export_library_sync_snapshot()?;
        assert_eq!(snap.collections.len(), 1);
        assert_eq!(snap.collections[0].shelf_name, PRESET_SHELVES[0]);

        Ok(())
    }

    #[test]
    fn compute_reading_streaks_handles_gaps_yesterday_and_empty() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        assert_eq!(compute_reading_streaks(&[], today), (0, 0));

        // Streak active through today
        let active_today = vec![
            NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 24).unwrap(),
        ];
        assert_eq!(compute_reading_streaks(&active_today, today), (3, 3));

        // Streak still alive if last read was yesterday, even with an older longer streak
        let yesterday_only = vec![
            NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 11).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 12).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
        ];
        assert_eq!(compute_reading_streaks(&yesterday_only, today), (2, 4));

        // Broken streak (> 1 day gap before today) resets current streak to 0 while keeping longest
        let stale = vec![
            NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 19).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
        ];
        assert_eq!(compute_reading_streaks(&stale, today), (0, 3));
    }

    #[test]
    fn extended_reader_metrics_tracks_sessions_genres_and_avoids_double_counting() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let database = Database::new(&conn);
        database.setup()?;

        // Manga 1 with two read chapters: one with a real session (180s), one legacy without session (360s fallback)
        database.save_history(MangaReadingHistorySave {
            id: "dandadan_1",
            title: "Dandadan",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "dd_ch_1",
                title: "Cap 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        })?;
        database.save_history(MangaReadingHistorySave {
            id: "dandadan_1",
            title: "Dandadan",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "dd_ch_2",
                title: "Cap 2",
                translated_language: "es",
            },
            provider: MangaProviders::Mangadex,
        })?;

        let today_str = Utc::now().format("%Y-%m-%d 12:00:00").to_string();
        let row_id = database.record_reading_session(ReadingSessionInput {
            manga_id: "dandadan_1",
            provider: MangaProviders::Mangadex,
            chapter_id: "dd_ch_1",
            duration_seconds: 180,
            pages_read: 22,
            completed_chapter: true,
            started_at: Some(&today_str),
        })?;
        assert!(row_id > 0);

        // Save genres across repeated calls with whitespace/case differences
        let first_inserted = database.save_manga_genres("dandadan_1", MangaProviders::Mangadex, &[
            "Action".to_string(),
            "Sci-Fi".to_string(),
            "  ".to_string(),
        ])?;
        assert_eq!(first_inserted, 2);

        let second_inserted =
            database.save_manga_genres("dandadan_1", MangaProviders::Mangadex, &["action".to_string(), " Comedy ".to_string()])?;
        assert_eq!(second_inserted, 1);

        let stored_genres = database.get_manga_genres("dandadan_1", MangaProviders::Mangadex)?;
        assert_eq!(stored_genres, vec!["Action", "Comedy", "Sci-Fi"]);

        let metrics = database.get_extended_reader_metrics()?;
        assert!(metrics.privacy_local_only);
        assert_eq!(metrics.total_chapters_read, 2);
        assert_eq!(metrics.chapters_read_today, 2);
        assert_eq!(metrics.chapters_read_last_7_days, 2);
        assert_eq!(metrics.total_pages_read, 22);
        assert_eq!(metrics.recorded_reading_seconds, 180);
        // 180s real session + 360s fallback for dd_ch_2 = 540s = 9 min (no double counting of dd_ch_1)
        assert_eq!(metrics.estimated_total_reading_minutes, 9);
        assert_eq!(metrics.current_streak_days, 1);
        assert_eq!(metrics.longest_streak_days, 1);
        assert_eq!(metrics.daily_activity_last_14_days.len(), 14);
        assert_eq!(metrics.top_read_mangas.len(), 1);
        assert_eq!(metrics.top_read_mangas[0].title, "Dandadan");
        assert_eq!(metrics.top_read_mangas[0].chapters_read, 2);
        assert_eq!(metrics.top_read_mangas[0].reading_minutes, 9);
        assert_eq!(metrics.top_genres.len(), 3);
        assert_eq!(metrics.top_genres[0].chapters_read, 2);

        // Cloud sync exports manga_genres metadata while keeping reading_sessions strictly local
        let snapshot = database.export_library_sync_snapshot()?;
        assert_eq!(snapshot.manga_genres.len(), 3);

        // Merging snapshot with another genre from remote imports into a second database
        let conn2 = Connection::open_in_memory()?;
        let db2 = Database::new(&conn2);
        db2.setup()?;
        let merged = db2.import_and_merge_library_sync_snapshot(&snapshot)?;
        assert_eq!(merged.manga_genres.len(), 3);
        assert_eq!(db2.get_manga_genres("dandadan_1", MangaProviders::Mangadex)?, vec!["Action", "Comedy", "Sci-Fi"]);

        // Ensure excessive duration (> 12h) is clamped to 43_200s
        database.record_reading_session(ReadingSessionInput {
            manga_id: "dandadan_1",
            provider: MangaProviders::Mangadex,
            chapter_id: "dd_ch_2",
            duration_seconds: 999_999,
            pages_read: 18,
            completed_chapter: true,
            started_at: Some(&today_str),
        })?;
        let after_clamp = database.get_extended_reader_metrics()?;
        assert_eq!(after_clamp.recorded_reading_seconds, 180 + 43_200);

        Ok(())
    }
}
