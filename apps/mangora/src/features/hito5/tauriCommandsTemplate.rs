//! Drop-in Tauri v2 command handlers for Mangora Hito 5.
//!
//! Copy this file into `apps/mangora/src-tauri/src/commands/hito5.rs` and
//! register the commands inside `tauri::generate_handler![...]` in `lib.rs` / `main.rs`.

use manga_core::backend::database::ExtendedReaderMetrics;
use manga_core::mangora_appearance::MangoraAppearanceConfig;
use manga_core::mangora_hito5_api::{
    self, AppearanceCatalogResponse, EditorialStatsCardSvgResponse, ExportStatsCardRequest,
    RecordReadingSessionRequest, SaveMangaGenresRequest,
};

#[tauri::command]
pub fn mangora_get_appearance_catalog() -> AppearanceCatalogResponse {
    mangora_hito5_api::get_appearance_catalog()
}

#[tauri::command]
pub fn mangora_get_appearance_config() -> MangoraAppearanceConfig {
    mangora_hito5_api::get_appearance_config()
}

#[tauri::command]
pub fn mangora_save_appearance_config(config: MangoraAppearanceConfig) -> Result<MangoraAppearanceConfig, String> {
    mangora_hito5_api::save_appearance_config(config)
}

#[tauri::command]
pub fn mangora_record_reading_session(request: RecordReadingSessionRequest) -> Result<i64, String> {
    mangora_hito5_api::record_reader_session(request)
}

#[tauri::command]
pub fn mangora_save_manga_genres(request: SaveMangaGenresRequest) -> Result<usize, String> {
    mangora_hito5_api::save_manga_genres(request)
}

#[tauri::command]
pub fn mangora_get_extended_reader_metrics() -> Result<ExtendedReaderMetrics, String> {
    mangora_hito5_api::get_extended_reader_metrics()
}

#[tauri::command]
pub fn mangora_export_stats_card_svg(request: ExportStatsCardRequest) -> Result<EditorialStatsCardSvgResponse, String> {
    mangora_hito5_api::build_editorial_stats_card_svg(request)
}

/*
// Register inside apps/mangora/src-tauri/src/lib.rs:
.invoke_handler(tauri::generate_handler![
    // ... Hito 1..4 commands ...
    commands::hito5::mangora_get_appearance_catalog,
    commands::hito5::mangora_get_appearance_config,
    commands::hito5::mangora_save_appearance_config,
    commands::hito5::mangora_record_reading_session,
    commands::hito5::mangora_save_manga_genres,
    commands::hito5::mangora_get_extended_reader_metrics,
    commands::hito5::mangora_export_stats_card_svg,
])
*/
