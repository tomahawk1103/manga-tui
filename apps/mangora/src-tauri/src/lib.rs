pub mod anilist;
pub mod commands;
pub mod state;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            app.manage(app_state);
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
