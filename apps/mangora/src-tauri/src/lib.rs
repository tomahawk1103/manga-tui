pub mod commands;
pub mod state;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_state = AppState::init_shared_storage().map_err(std::io::Error::other)?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_initial_state,
            commands::search_catalog,
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
