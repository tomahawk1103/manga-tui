pub mod anilist;
pub mod commands;
pub mod downloads;
pub mod reader;
pub mod state;

use std::sync::Arc;

use manga_core::backend::download_queue::ManagedDownloadQueue;
use tauri::{Emitter, Manager};

use crate::reader::ReaderSessionStore;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("mangora", reader::handle_mangora_protocol_request)
        .setup(|app| {
            #[cfg(mobile)]
            {
                let app_data_dir = app.path().app_data_dir().map_err(std::io::Error::other)?;
                let app_config_dir = app.path().app_config_dir().unwrap_or_else(|_| app_data_dir.clone());
                let app_cache_dir = app.path().app_cache_dir().unwrap_or_else(|_| app_data_dir.join("cache"));
                let _ = manga_core::backend::set_custom_data_dir(app_data_dir);
                let _ = manga_core::config::set_custom_config_dir(app_config_dir);
                let _ = manga_core::backend::set_custom_cache_dir(app_cache_dir);
            }
            let app_state = AppState::init_shared_storage().map_err(std::io::Error::other)?;
            let downloads_dir = app_state.downloads_dir().map_err(std::io::Error::other)?;
            let config_dir = app_state.config_dir().map_err(std::io::Error::other)?;
            let db_path = app_state.db_path().map_err(std::io::Error::other)?;

            app.manage(ReaderSessionStore::new());
            app.manage(app_state);

            let app_for_snap = app.handle().clone();
            let app_for_done = app.handle().clone();
            let download_queue = tauri::async_runtime::block_on(async move {
                ManagedDownloadQueue::spawn(
                    downloads_dir,
                    config_dir,
                    db_path,
                    Arc::new(move |snap| {
                        let _ = app_for_snap.emit(downloads::DOWNLOAD_QUEUE_EVENT, downloads::map_queue_snapshot(snap));
                    }),
                    Arc::new(move |ev| {
                        let _ = app_for_done.emit(downloads::DOWNLOAD_FINISHED_EVENT, downloads::map_finished_event(ev));
                    }),
                )
            });
            app.manage(download_queue);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_initial_state,
            commands::get_library_overview,
            commands::toggle_library_favorite,
            commands::toggle_chapter_read,
            commands::search_catalog,
            commands::get_series_detail,
            commands::get_anilist_community,
            commands::get_for_you_recommendations,
            commands::open_reader_chapter,
            commands::save_reader_progress,
            commands::save_reader_prefs,
            commands::get_download_queue_snapshot,
            commands::enqueue_chapters_download,
            commands::control_download_queue,
            commands::set_download_format,
            commands::list_offline_library,
            commands::save_onboarding_preferences,
            commands::set_ui_language,
            commands::set_theme_mode,
            commands::set_default_provider,
            commands::set_favorite_genres,
            commands::set_onboarding_completed,
            commands::open_external_url,
            commands::open_data_folder,
            commands::open_downloads_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running mangora tauri application");
}
