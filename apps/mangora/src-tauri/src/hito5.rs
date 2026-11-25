use manga_core::backend::database::{Database, ExtendedReaderMetrics};
use manga_core::mangora_appearance::{MangoraAppearanceConfig, load_appearance_from_dir, save_appearance_to_dir};
use manga_core::mangora_hito5_api::{
    self, AppearanceCatalogResponse, EditorialStatsCardSvgResponse, ExportStatsCardRequest, RecordReadingSessionRequest,
    SaveMangaGenresRequest, build_editorial_stats_card_svg_in_db, get_extended_reader_metrics_in_db, record_reader_session_in_db,
    save_manga_genres_in_db,
};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn mangora_get_appearance_catalog() -> AppearanceCatalogResponse {
    mangora_hito5_api::get_appearance_catalog()
}

#[tauri::command]
pub fn mangora_get_appearance_config(state: State<'_, AppState>) -> Result<MangoraAppearanceConfig, String> {
    let config_dir = state.config_dir()?;
    Ok(load_appearance_from_dir(&config_dir))
}

#[tauri::command]
pub fn mangora_save_appearance_config(
    state: State<'_, AppState>,
    config: MangoraAppearanceConfig,
) -> Result<MangoraAppearanceConfig, String> {
    let config_dir = state.config_dir()?;
    save_appearance_to_dir(&config_dir, &config)?;
    Ok(config)
}

#[tauri::command]
pub fn mangora_record_reading_session(state: State<'_, AppState>, request: RecordReadingSessionRequest) -> Result<i64, String> {
    let conn = state.open_db_connection()?;
    let db = Database::new(&conn);
    record_reader_session_in_db(&db, &request)
}

#[tauri::command]
pub fn mangora_save_manga_genres(state: State<'_, AppState>, request: SaveMangaGenresRequest) -> Result<usize, String> {
    let conn = state.open_db_connection()?;
    let db = Database::new(&conn);
    save_manga_genres_in_db(&db, &request)
}

#[tauri::command]
pub fn mangora_get_extended_reader_metrics(state: State<'_, AppState>) -> Result<ExtendedReaderMetrics, String> {
    let conn = state.open_db_connection()?;
    let db = Database::new(&conn);
    get_extended_reader_metrics_in_db(&db)
}

#[tauri::command]
pub fn mangora_export_stats_card_svg(
    state: State<'_, AppState>,
    request: ExportStatsCardRequest,
) -> Result<EditorialStatsCardSvgResponse, String> {
    let conn = state.open_db_connection()?;
    let db = Database::new(&conn);
    build_editorial_stats_card_svg_in_db(&db, &request)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use manga_core::backend::StorageMode;
    use manga_core::backend::manga_provider::{Languages, MangaProviders};
    use manga_core::config::UiLanguage;
    use manga_core::mangora_appearance::{CardDensity, EditorialPalette, ReaderBackground};
    use pretty_assertions::assert_eq;

    use super::*;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = std::env::temp_dir().join(format!("mangora_hito5_{label}_{nanos}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn make_state(root: &std::path::Path) -> AppState {
        AppState::from_paths(
            root.join("data"),
            root.join("config"),
            root.join("downloads"),
            root.join("data/history/manga-tui-history.db"),
            StorageMode::System,
            UiLanguage::Es,
            MangaProviders::Inmanga,
            Languages::Spanish,
        )
    }

    #[test]
    fn appearance_persists_in_state_config_dir_and_tracks_total_pages_read() {
        let temp = TempDirGuard::new("pages_and_palette");
        let state = make_state(&temp.path);

        let custom = MangoraAppearanceConfig {
            palette: EditorialPalette::EditorialSepia,
            library_density: CardDensity::Compact,
            reader_background: ReaderBackground::WarmSepia,
            ..Default::default()
        };
        save_appearance_to_dir(&state.config_dir().unwrap(), &custom).unwrap();
        let loaded = load_appearance_from_dir(&state.config_dir().unwrap());
        assert_eq!(loaded, custom);

        {
            let conn = state.open_db_connection().unwrap();
            let db = Database::new(&conn);
            record_reader_session_in_db(&db, &RecordReadingSessionRequest {
                manga_id: "m-1".to_string(),
                provider: "inmanga".to_string(),
                chapter_id: "ch-1".to_string(),
                duration_seconds: 120,
                pages_read: 24,
                completed_chapter: true,
                started_at: None,
            })
            .unwrap();
            record_reader_session_in_db(&db, &RecordReadingSessionRequest {
                manga_id: "m-1".to_string(),
                provider: "inmanga".to_string(),
                chapter_id: "ch-2".to_string(),
                duration_seconds: 90,
                pages_read: 19,
                completed_chapter: true,
                started_at: None,
            })
            .unwrap();

            let metrics = get_extended_reader_metrics_in_db(&db).unwrap();
            assert_eq!(metrics.total_pages_read, 43);

            let card = build_editorial_stats_card_svg_in_db(&db, &ExportStatsCardRequest::default()).unwrap();
            assert!(card.svg.contains("43 pág."));
        }

        let snap = state.snapshot().unwrap();
        assert_eq!(snap.library.total_pages_read, 43);
        assert!(snap.library.has_existing_data);
    }
}
