use tauri::State;

use crate::state::{AppState, InitialAppState, OnboardingPreferencesPayload};

#[tauri::command]
pub fn get_initial_state(state: State<'_, AppState>) -> Result<InitialAppState, String> {
    state.snapshot()
}

#[tauri::command]
pub fn save_onboarding_preferences(
    state: State<'_, AppState>,
    payload: OnboardingPreferencesPayload,
) -> Result<InitialAppState, String> {
    state.save_onboarding_preferences(payload)
}

#[tauri::command]
pub fn set_ui_language(state: State<'_, AppState>, language: String) -> Result<InitialAppState, String> {
    state.set_ui_language(&language)
}

#[tauri::command]
pub fn set_theme_mode(state: State<'_, AppState>, theme_mode: String) -> Result<InitialAppState, String> {
    state.set_theme_mode(&theme_mode)
}

#[tauri::command]
pub fn set_default_provider(state: State<'_, AppState>, provider: String) -> Result<InitialAppState, String> {
    state.set_default_provider(&provider)
}

#[tauri::command]
pub fn set_favorite_genres(state: State<'_, AppState>, genres: Vec<String>) -> Result<InitialAppState, String> {
    state.set_favorite_genres(&genres)
}

#[tauri::command]
pub fn set_onboarding_completed(state: State<'_, AppState>, completed: bool) -> Result<InitialAppState, String> {
    state.set_onboarding_completed(completed)
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://") {
        return Err("Only HTTPS URLs can be opened externally".to_string());
    }
    open::that(trimmed).map_err(|e| format!("Could not open URL: {e}"))
}

#[tauri::command]
pub fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.data_dir()?;
    open::that(&dir).map_err(|e| format!("Could not open data folder {}: {e}", dir.display()))
}
