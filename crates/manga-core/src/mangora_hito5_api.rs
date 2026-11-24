//! High-level JSON-serializable facade for Mangora Hito 5 features.
//!
//! Designed so Tauri v2 `#[tauri::command]` handlers in `apps/mangora/src-tauri/`
//! can delegate visual customization, local reading session tracking, genre tagging,
//! extended reader metrics, and editorial summary card exports in one line.

use serde::{Deserialize, Serialize};

use crate::backend::database::{Database, ExtendedReaderMetrics, ReadingSessionInput};
use crate::backend::manga_provider::MangaProviders;
use crate::config::UiLanguage;
use crate::mangora_appearance::{
    EditorialPalette, MangoraAppearanceConfig, PaletteSpec, ReaderBackgroundSpec, all_palette_specs, all_reader_background_specs,
    load_appearance, save_appearance,
};
use crate::mangora_metrics_card::{
    EditorialStatsCardOptions, STATS_CARD_HEIGHT, STATS_CARD_WIDTH, render_editorial_stats_card_svg,
};

fn default_true() -> bool {
    true
}

fn parse_provider(raw: &str) -> Result<MangaProviders, String> {
    raw.trim()
        .to_ascii_lowercase()
        .parse::<MangaProviders>()
        .map_err(|e| format!("Invalid manga provider '{}': {e}", raw.trim()))
}

fn open_initialized_db() -> Result<rusqlite::Connection, String> {
    let conn = Database::get_connection().map_err(|e| e.to_string())?;
    let db = Database::new(&conn);
    db.setup().map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Complete catalog of built-in editorial palettes and subtle reader backgrounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceCatalogResponse {
    pub palettes: Vec<PaletteSpec>,
    pub reader_backgrounds: Vec<ReaderBackgroundSpec>,
}

/// Request payload for recording a 100 % local reading session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordReadingSessionRequest {
    pub manga_id: String,
    pub provider: String,
    pub chapter_id: String,
    pub duration_seconds: u32,
    pub pages_read: u32,
    #[serde(default)]
    pub completed_chapter: bool,
    #[serde(default)]
    pub started_at: Option<String>,
}

/// Request payload for storing genres associated with a manga in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMangaGenresRequest {
    pub manga_id: String,
    pub provider: String,
    pub genres: Vec<String>,
}

/// Request payload for generating an exportable editorial summary SVG card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportStatsCardRequest {
    pub palette: EditorialPalette,
    pub language: UiLanguage,
    pub reader_alias: Option<String>,
    #[serde(default = "default_true")]
    pub include_screentone: bool,
}

impl Default for ExportStatsCardRequest {
    fn default() -> Self {
        Self {
            palette: EditorialPalette::ClassicPaper,
            language: UiLanguage::Es,
            reader_alias: None,
            include_screentone: true,
        }
    }
}

/// Response payload containing the rendered SVG editorial summary card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorialStatsCardSvgResponse {
    pub svg: String,
    pub suggested_filename: String,
    pub width: u32,
    pub height: u32,
    pub generated_at: String,
}

/// Returns all editorial palettes and reader background presets supported by Mangora.
pub fn get_appearance_catalog() -> AppearanceCatalogResponse {
    AppearanceCatalogResponse {
        palettes: all_palette_specs(),
        reader_backgrounds: all_reader_background_specs(),
    }
}

/// Loads persisted appearance customization from the shared `config.toml`.
pub fn get_appearance_config() -> MangoraAppearanceConfig {
    load_appearance()
}

/// Saves visual appearance customization to `config.toml` and returns the stored config.
pub fn save_appearance_config(config: MangoraAppearanceConfig) -> Result<MangoraAppearanceConfig, String> {
    save_appearance(&config)?;
    Ok(config)
}

/// Records a reading session against an existing `Database` handle.
pub fn record_reader_session_in_db(db: &Database<'_>, request: &RecordReadingSessionRequest) -> Result<i64, String> {
    let provider = parse_provider(&request.provider)?;
    db.record_reading_session(ReadingSessionInput {
        manga_id: request.manga_id.trim(),
        provider,
        chapter_id: request.chapter_id.trim(),
        duration_seconds: request.duration_seconds,
        pages_read: request.pages_read,
        completed_chapter: request.completed_chapter,
        started_at: request.started_at.as_deref(),
    })
    .map_err(|e| e.to_string())
}

/// Records a reading session using the active local SQLite database.
pub fn record_reader_session(request: RecordReadingSessionRequest) -> Result<i64, String> {
    let conn = open_initialized_db()?;
    let db = Database::new(&conn);
    record_reader_session_in_db(&db, &request)
}

/// Saves manga genres against an existing `Database` handle.
pub fn save_manga_genres_in_db(db: &Database<'_>, request: &SaveMangaGenresRequest) -> Result<usize, String> {
    let provider = parse_provider(&request.provider)?;
    db.save_manga_genres(request.manga_id.trim(), provider, &request.genres)
        .map_err(|e| e.to_string())
}

/// Saves manga genres using the active local SQLite database.
pub fn save_manga_genres(request: SaveMangaGenresRequest) -> Result<usize, String> {
    let conn = open_initialized_db()?;
    let db = Database::new(&conn);
    save_manga_genres_in_db(&db, &request)
}

/// Computes extended reader metrics from an existing `Database` handle.
pub fn get_extended_reader_metrics_in_db(db: &Database<'_>) -> Result<ExtendedReaderMetrics, String> {
    db.get_extended_reader_metrics().map_err(|e| e.to_string())
}

/// Computes 100 % local extended reader metrics from the active SQLite database.
pub fn get_extended_reader_metrics() -> Result<ExtendedReaderMetrics, String> {
    let conn = open_initialized_db()?;
    let db = Database::new(&conn);
    get_extended_reader_metrics_in_db(&db)
}

/// Renders the editorial summary SVG card from an already-computed `ExtendedReaderMetrics`.
pub fn export_editorial_stats_card_svg(
    metrics: &ExtendedReaderMetrics,
    request: &ExportStatsCardRequest,
) -> EditorialStatsCardSvgResponse {
    let svg = render_editorial_stats_card_svg(metrics, &EditorialStatsCardOptions {
        palette: request.palette,
        language: request.language,
        reader_alias: request.reader_alias.clone(),
        include_screentone: request.include_screentone,
    });
    let date_part = metrics.generated_at.split('T').next().unwrap_or("summary");
    EditorialStatsCardSvgResponse {
        svg,
        suggested_filename: format!("mangora-stats-{date_part}.svg"),
        width: STATS_CARD_WIDTH,
        height: STATS_CARD_HEIGHT,
        generated_at: metrics.generated_at.clone(),
    }
}

/// Reads local metrics from an existing `Database` handle and renders the SVG card.
pub fn build_editorial_stats_card_svg_in_db(
    db: &Database<'_>,
    request: &ExportStatsCardRequest,
) -> Result<EditorialStatsCardSvgResponse, String> {
    let metrics = get_extended_reader_metrics_in_db(db)?;
    Ok(export_editorial_stats_card_svg(&metrics, request))
}

/// Reads local metrics from the active SQLite database and renders the editorial SVG card.
pub fn build_editorial_stats_card_svg(request: ExportStatsCardRequest) -> Result<EditorialStatsCardSvgResponse, String> {
    let conn = open_initialized_db()?;
    let db = Database::new(&conn);
    build_editorial_stats_card_svg_in_db(&db, &request)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use rusqlite::Connection;

    use super::*;
    use crate::backend::database::{ChapterToSaveHistory, MangaReadingHistorySave};

    #[test]
    fn appearance_catalog_contains_all_palettes_and_backgrounds() {
        let catalog = get_appearance_catalog();
        assert_eq!(catalog.palettes.len(), 5);
        assert_eq!(catalog.reader_backgrounds.len(), 7);

        let serialized = serde_json::to_string(&catalog).unwrap();
        let deserialized: AppearanceCatalogResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, catalog);
    }

    #[test]
    fn end_to_end_hito5_api_flow_with_in_memory_sqlite() {
        let conn = Connection::open_in_memory().unwrap();
        let db = Database::new(&conn);
        db.setup().unwrap();

        db.save_history(MangaReadingHistorySave {
            id: "frieren_1",
            title: "Sousou no Frieren",
            img_url: None,
            chapter: ChapterToSaveHistory {
                id: "fr_ch_1",
                title: "Cap 1",
                translated_language: "es",
            },
            provider: MangaProviders::Mangaoni,
        })
        .unwrap();

        let genres_req: SaveMangaGenresRequest = serde_json::from_str(
            r#"{
                "manga_id": "frieren_1",
                "provider": "MangaOni",
                "genres": ["Fantasy", "Drama", "Adventure"]
            }"#,
        )
        .unwrap();
        assert_eq!(save_manga_genres_in_db(&db, &genres_req).unwrap(), 3);

        let today_ts = Utc::now().format("%Y-%m-%d 10:00:00").to_string();
        let session_req = RecordReadingSessionRequest {
            manga_id: "frieren_1".to_string(),
            provider: "mangaoni".to_string(),
            chapter_id: "fr_ch_1".to_string(),
            duration_seconds: 420,
            pages_read: 28,
            completed_chapter: true,
            started_at: Some(today_ts),
        };
        let session_id = record_reader_session_in_db(&db, &session_req).unwrap();
        assert!(session_id > 0);

        let metrics = get_extended_reader_metrics_in_db(&db).unwrap();
        assert_eq!(metrics.total_chapters_read, 1);
        assert_eq!(metrics.estimated_total_reading_minutes, 7);
        assert_eq!(metrics.top_genres.len(), 3);
        assert!(metrics.privacy_local_only);

        let card_req: ExportStatsCardRequest = serde_json::from_str(
            r#"{
                "palette": "editorial_sepia",
                "language": "es",
                "reader_alias": "Lector",
                "include_screentone": true
            }"#,
        )
        .unwrap();
        let card = build_editorial_stats_card_svg_in_db(&db, &card_req).unwrap();
        assert_eq!(card.width, 1200);
        assert_eq!(card.height, 675);
        assert!(card.suggested_filename.starts_with("mangora-stats-"));
        assert!(card.svg.contains("Sousou no Frieren"));
        assert!(card.svg.contains("#f4ecd8"));
    }
}
