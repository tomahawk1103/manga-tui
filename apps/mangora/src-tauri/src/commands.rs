use std::str::FromStr;
use std::time::Duration;

use manga_core::SearchTerm;
use manga_core::backend::manga_provider::{MangaProviders, Pagination};
use manga_core::backend::multi_provider::search_mangas_for_provider;
use serde::Serialize;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::state::{
    AppState, InitialAppState, LibraryOverview, OnboardingPreferencesPayload, ToggleFavoritePayload, ToggleFavoriteResult,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    id: String,
    title: String,
    description: Option<String>,
    cover_url: String,
    genres: Vec<String>,
    languages: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    items: Vec<CatalogItem>,
    has_next: bool,
}

#[tauri::command]
pub async fn search_catalog(provider: String, query: String, page: u32) -> Result<CatalogPage, String> {
    let provider = MangaProviders::from_str(&provider).map_err(|_| "Unknown catalog source")?;
    let term = SearchTerm::trimmed(&query).ok_or("Missing search term")?;
    let mut pagination = Pagination::default();
    if page == 0 || page > u32::MAX / pagination.items_per_page {
        return Err("Invalid catalog page".into());
    }
    pagination.current_page = page;
    let end_offset = page * pagination.items_per_page;
    let response = tokio::time::timeout(Duration::from_secs(20), search_mangas_for_provider(provider, Some(term), pagination))
        .await
        .map_err(|_| "Catalog request timed out")?
        .map_err(|err| err.to_string())?;
    Ok(CatalogPage {
        has_next: response.next_page
            && !response.mangas.is_empty()
            && (provider != MangaProviders::Mangadex || end_offset < response.total_mangas),
        items: response
            .mangas
            .into_iter()
            .map(|item| CatalogItem {
                id: item.id,
                title: item.title,
                description: item.description,
                cover_url: item.cover_img_url,
                genres: item.genres.into_iter().map(|genre| genre.title).collect(),
                languages: item
                    .languages
                    .into_iter()
                    .map(|lang| lang.as_iso_code())
                    .filter(|code| !code.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
            .collect(),
    })
}

#[tauri::command]
pub fn get_initial_state(state: State<'_, AppState>) -> Result<InitialAppState, String> {
    state.snapshot()
}

#[tauri::command]
pub fn get_library_overview(state: State<'_, AppState>, search: Option<String>) -> Result<LibraryOverview, String> {
    state.get_library_overview(search.as_deref())
}

#[tauri::command]
pub fn toggle_library_favorite(state: State<'_, AppState>, payload: ToggleFavoritePayload) -> Result<ToggleFavoriteResult, String> {
    state.toggle_library_favorite(payload)
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

fn validate_external_https_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || !trimmed.starts_with("https://") {
        return Err("Only HTTPS URLs can be opened externally".to_string());
    }
    Ok(trimmed)
}

fn open_folder_in_file_manager(
    app: &tauri::AppHandle,
    dir: &std::path::Path,
    label: &str,
    create_if_missing: bool,
) -> Result<(), String> {
    #[cfg(desktop)]
    {
        if create_if_missing {
            std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {label} folder {}: {e}", dir.display()))?;
        }
        app.opener()
            .open_path(dir.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| format!("Could not open {label} folder {}: {e}", dir.display()))
    }
    #[cfg(mobile)]
    {
        let _ = (app, dir, label, create_if_missing);
        Err("Opening the file manager is not supported on mobile devices".to_string())
    }
}

#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let trimmed = validate_external_https_url(&url)?;
    app.opener()
        .open_url(trimmed, None::<&str>)
        .map_err(|e| format!("Could not open URL: {e}"))
}

#[tauri::command]
pub fn open_data_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.data_dir()?;
    open_folder_in_file_manager(&app, &dir, "data", false)
}

#[tauri::command]
pub fn open_downloads_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.downloads_dir()?;
    open_folder_in_file_manager(&app, &dir, "downloads", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_external_https_url_allows_only_https() {
        assert_eq!(validate_external_https_url("  https://discord.gg/SeqsWUCpVx  ").unwrap(), "https://discord.gg/SeqsWUCpVx");
        for invalid in ["", "   ", "http://example.com", "javascript:alert(1)", "file:///etc/passwd"] {
            assert!(validate_external_https_url(invalid).is_err());
        }
    }

    #[tokio::test]
    async fn catalog_rejects_invalid_input_before_querying_the_provider() {
        for (provider, query, page) in [("missing", "Monster", 1), ("mangadex", "  ", 1), ("mangadex", "Monster", 0)] {
            assert!(search_catalog(provider.into(), query.into(), page).await.is_err());
        }
    }

    #[tokio::test]
    #[ignore = "requires a live MangaDex connection"]
    async fn mangadex_catalog_returns_titles_and_covers() {
        let page = search_catalog("mangadex".into(), "Monster".into(), 1).await.unwrap();
        assert!(
            page.items
                .iter()
                .any(|item| !item.title.is_empty() && item.cover_url.starts_with("https://"))
        );
    }
}
