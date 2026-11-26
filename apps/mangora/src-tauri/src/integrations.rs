use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use manga_core::backend::cloud::{
    CLOUD_ROOT_FOLDER_NAME, CLOUD_STATE_FILE_NAME, CloudBackendKind, CloudConnectionState, CloudSyncMode, CloudSyncReport,
    GoogleDriveClient, accept_google_loopback_code, detect_installed_cloud_folders, has_embedded_google_oauth_config,
    run_cloud_sync_with_paths,
};
use manga_core::backend::secrets::SecretStorage;
use manga_core::backend::secrets::keyring::KeyringStorage;
use manga_core::backend::tracker::anilist::{Anilist, BASE_ANILIST_API_URL};
use manga_core::backend::tracker::myanimelist::{
    MAL_STATE_FILE_NAME, MalConnectionState, MalSyncReport, MyAnimeListClient, accept_mal_loopback_code,
    has_embedded_mal_oauth_config,
};
use manga_core::backend::updater::sanitize_release_notes;
use manga_core::config::build_config_file_in_dir;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::feedback::{DEFAULT_GITHUB_ISSUES_URL, FeedbackConfigDto, build_feedback_config};
use crate::state::{AppState, InitialAppState};

pub const DEFAULT_DISCORD_URL: &str = "https://discord.gg/SeqsWUCpVx";
pub const DEFAULT_GITHUB_URL: &str = "https://github.com/tomahawk1103/manga-tui";
pub const DEFAULT_WEBSITE_URL: &str = "https://tomahawk1103.github.io/manga-tui/";
pub const DEFAULT_SPONSORS_URL: &str = "https://github.com/sponsors/tomahawk1103";
pub const DEFAULT_RELEASES_API_URL: &str = "https://api.github.com/repos/tomahawk1103/manga-tui";
pub const OAUTH_COMPLETED_EVENT: &str = "mangora://oauth-completed";
pub const DEFAULT_ANILIST_CLIENT_ID: &str = "18641";

const SECRET_ANILIST_CLIENT_ID: &str = "anilist_client_id";
const SECRET_ANILIST_ACCESS_TOKEN: &str = "anilist_access_token";

#[derive(Debug, Clone)]
pub struct PendingOAuthSession {
    pub provider: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub code_verifier: String,
    pub state: String,
}

#[derive(Debug)]
struct PendingOAuthEntry {
    session: PendingOAuthSession,
    abort_handle: Option<tokio::task::AbortHandle>,
}

#[derive(Debug, Default)]
pub struct PendingOAuthStore {
    sessions: Mutex<HashMap<String, PendingOAuthEntry>>,
}

impl PendingOAuthStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, provider: &str, session: PendingOAuthSession) {
        self.insert_with_abort(provider, session, None);
    }

    pub fn insert_with_abort(&self, provider: &str, session: PendingOAuthSession, abort_handle: Option<tokio::task::AbortHandle>) {
        if let Ok(mut guard) = self.sessions.lock()
            && let Some(prev) = guard.insert(provider.to_string(), PendingOAuthEntry {
                session,
                abort_handle,
            })
            && let Some(handle) = prev.abort_handle
        {
            handle.abort();
        }
    }

    pub fn get(&self, provider: &str) -> Option<PendingOAuthSession> {
        self.sessions.lock().ok().and_then(|guard| guard.get(provider).map(|e| e.session.clone()))
    }

    pub fn take(&self, provider: &str) -> Option<PendingOAuthSession> {
        self.sessions.lock().ok().and_then(|mut guard| {
            guard.remove(provider).map(|entry| {
                if let Some(handle) = entry.abort_handle {
                    handle.abort();
                }
                entry.session
            })
        })
    }

    pub fn take_matching_state(&self, provider: &str, expected_state: &str) -> Option<PendingOAuthSession> {
        self.sessions.lock().ok().and_then(|mut guard| {
            let matches = guard.get(provider).is_some_and(|e| e.session.state == expected_state);
            if matches { guard.remove(provider).map(|e| e.session) } else { None }
        })
    }

    pub fn remove_matching_state(&self, provider: &str, expected_state: &str) {
        if let Ok(mut guard) = self.sessions.lock() {
            let matches = guard.get(provider).is_some_and(|e| e.session.state == expected_state);
            if matches {
                guard.remove(provider);
            }
        }
    }

    pub fn cancel_and_remove(&self, provider: &str) {
        if let Ok(mut guard) = self.sessions.lock()
            && let Some(prev) = guard.remove(provider)
            && let Some(handle) = prev.abort_handle
        {
            handle.abort();
        }
    }

    pub fn remove(&self, provider: &str) {
        self.cancel_and_remove(provider);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCompletedEventDto {
    pub provider: String,
    pub ok: bool,
    pub error: Option<String>,
    pub overview: Option<IntegrationsOverviewDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCloudFolderDto {
    pub provider_label: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncReportDto {
    pub synced_at: String,
    pub total_mangas: usize,
    pub total_read_chapters: usize,
    pub total_favorites: usize,
    pub chapters_uploaded: usize,
    pub chapters_downloaded: usize,
    pub bytes_transferred: u64,
}

impl From<CloudSyncReport> for CloudSyncReportDto {
    fn from(value: CloudSyncReport) -> Self {
        Self {
            synced_at: value.synced_at,
            total_mangas: value.total_mangas,
            total_read_chapters: value.total_read_chapters,
            total_favorites: value.total_favorites,
            chapters_uploaded: value.chapters_uploaded,
            chapters_downloaded: value.chapters_downloaded,
            bytes_transferred: value.bytes_transferred,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatusDto {
    pub connected: bool,
    pub backend: Option<String>,
    pub summary: String,
    pub account_email: Option<String>,
    pub account_display_name: Option<String>,
    pub local_folder_path: Option<String>,
    pub local_folder_label: Option<String>,
    pub has_embedded_oauth: bool,
    pub saved_client_id: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_report: Option<CloudSyncReportDto>,
    pub detected_folders: Vec<DetectedCloudFolderDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnilistStatusDto {
    pub connected: bool,
    pub client_id_hint: Option<String>,
    pub source: Option<String>,
    pub track_reading_history: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MalSyncReportDto {
    pub synced_at: String,
    pub imported_from_mal: usize,
    pub pushed_to_mal: usize,
    pub total_mal_entries: usize,
}

impl From<MalSyncReport> for MalSyncReportDto {
    fn from(value: MalSyncReport) -> Self {
        Self {
            synced_at: value.synced_at,
            imported_from_mal: value.imported_from_mal,
            pushed_to_mal: value.pushed_to_mal,
            total_mal_entries: value.total_mal_entries,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MalStatusDto {
    pub connected: bool,
    pub username: Option<String>,
    pub summary: String,
    pub has_embedded_oauth: bool,
    pub saved_client_id: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_report: Option<MalSyncReportDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackersStatusDto {
    pub anilist: AnilistStatusDto,
    pub my_anime_list: MalStatusDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportConfigDto {
    pub kofi_url: Option<String>,
    pub sponsors_url: String,
    pub discord_url: String,
    pub github_url: String,
    pub github_issues_url: String,
    pub website_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationsOverviewDto {
    pub cloud: CloudStatusDto,
    pub trackers: TrackersStatusDto,
    pub support_config: SupportConfigDto,
    pub feedback_config: FeedbackConfigDto,
    pub manga_tui_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStartResultDto {
    pub provider: String,
    pub mode: String,
    pub authorize_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub completed: bool,
    pub overview: IntegrationsOverviewDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncActionResultDto {
    pub report: CloudSyncReportDto,
    pub overview: IntegrationsOverviewDto,
    pub app_state: InitialAppState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MalSyncActionResultDto {
    pub report: MalSyncReportDto,
    pub overview: IntegrationsOverviewDto,
    pub app_state: InitialAppState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResultDto {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub android_apk_url: Option<String>,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableBackupImportResultDto {
    pub total_mangas: usize,
    pub total_read_chapters: usize,
    pub total_favorites: usize,
    pub app_state: InitialAppState,
    pub library: crate::state::LibraryOverview,
}

fn resolve_https_url_from(runtime_env: Option<&str>, compile_env: Option<&str>) -> Option<String> {
    [runtime_env, compile_env]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| value.starts_with("https://"))
        .map(ToOwned::to_owned)
}

pub fn resolve_support_config() -> SupportConfigDto {
    let runtime_kofi = std::env::var("MANGORA_KOFI_URL").ok();
    let kofi_url = resolve_https_url_from(runtime_kofi.as_deref(), option_env!("MANGORA_KOFI_URL"));

    let runtime_sponsors = std::env::var("MANGORA_SPONSORS_URL").ok();
    let sponsors_url = resolve_https_url_from(runtime_sponsors.as_deref(), option_env!("MANGORA_SPONSORS_URL"))
        .unwrap_or_else(|| DEFAULT_SPONSORS_URL.to_string());

    SupportConfigDto {
        kofi_url,
        sponsors_url,
        discord_url: DEFAULT_DISCORD_URL.to_string(),
        github_url: DEFAULT_GITHUB_URL.to_string(),
        github_issues_url: DEFAULT_GITHUB_ISSUES_URL.to_string(),
        website_url: DEFAULT_WEBSITE_URL.to_string(),
    }
}

fn cloud_state_path(state: &AppState) -> Result<PathBuf, String> {
    Ok(state.data_dir()?.join(CLOUD_STATE_FILE_NAME))
}

fn mal_state_path(state: &AppState) -> Result<PathBuf, String> {
    Ok(state.data_dir()?.join(MAL_STATE_FILE_NAME))
}

fn list_detected_folders() -> Vec<DetectedCloudFolderDto> {
    #[cfg(not(target_os = "android"))]
    {
        detect_installed_cloud_folders()
            .into_iter()
            .map(|entry| DetectedCloudFolderDto {
                provider_label: entry.provider_label,
                path: entry.manga_tui_folder.display().to_string(),
            })
            .collect()
    }
    #[cfg(target_os = "android")]
    {
        Vec::new()
    }
}

fn mask_hint(raw: &str) -> String {
    let trimmed = raw.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len());
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••{tail}")
}

fn strip_toml_quotes(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(comment_stripped) = trimmed.split(" #").next() {
        let val = comment_stripped.trim();
        if ((val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\''))) && val.len() >= 2 {
            return val[1..val.len() - 1].to_string();
        }
        return val.to_string();
    }
    trimmed.to_string()
}

fn read_anilist_status(config_dir: &Path) -> AnilistStatusDto {
    let config_path = config_dir.join("config.toml");
    let raw = fs::read_to_string(&config_path).unwrap_or_default();

    let mut track_reading_history = false;
    let mut config_client_id: Option<String> = None;
    let mut config_token: Option<String> = None;
    let mut in_table = false;
    let mut in_anilist = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_table = true;
            in_anilist = trimmed == "[anilist]";
            continue;
        }
        let Some((lhs, rhs)) = trimmed.split_once('=') else {
            continue;
        };
        let key = lhs.trim();
        let val = strip_toml_quotes(rhs);
        if !in_table && key == "track_reading_history" {
            track_reading_history = val.eq_ignore_ascii_case("true");
        } else if in_anilist && key == "client_id" && !val.is_empty() {
            config_client_id = Some(val);
        } else if in_anilist && key == "access_token" && !val.is_empty() {
            config_token = Some(val);
        }
    }

    let keyring_creds = if cfg!(test) {
        (None, None)
    } else {
        let storage = KeyringStorage::new();
        (
            storage
                .get_secret(SECRET_ANILIST_CLIENT_ID)
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty()),
            storage
                .get_secret(SECRET_ANILIST_ACCESS_TOKEN)
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty()),
        )
    };

    if let (Some(id), Some(_tok)) = (keyring_creds.0, keyring_creds.1) {
        return AnilistStatusDto {
            connected: true,
            client_id_hint: Some(mask_hint(&id)),
            source: Some("keyring".to_string()),
            track_reading_history,
        };
    }

    if let (Some(id), Some(_tok)) = (config_client_id, config_token) {
        return AnilistStatusDto {
            connected: true,
            client_id_hint: Some(mask_hint(&id)),
            source: Some("config".to_string()),
            track_reading_history,
        };
    }

    AnilistStatusDto {
        connected: false,
        client_id_hint: None,
        source: None,
        track_reading_history,
    }
}

/// Updates root `track_reading_history` and the `[anilist]` section inside `config.toml` while preserving the rest of the file.
fn update_anilist_in_config_dir(
    config_dir: &Path,
    track_reading_history: bool,
    client_id: &str,
    access_token: &str,
) -> Result<(), String> {
    if !config_dir.join("config.toml").exists() {
        let _ = build_config_file_in_dir(config_dir);
    }
    let config_path = config_dir.join("config.toml");
    let contents = fs::read_to_string(&config_path).unwrap_or_default();

    let mut lines: Vec<String> = contents.lines().map(ToOwned::to_owned).collect();
    let mut updated_track = false;
    let mut first_table_idx: Option<usize> = None;

    for (idx, line) in lines.iter_mut().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if first_table_idx.is_none() {
                first_table_idx = Some(idx);
            }
            break;
        }
        if !trimmed.starts_with('#')
            && let Some((lhs, _)) = trimmed.split_once('=')
            && lhs.trim() == "track_reading_history"
        {
            *line = format!("track_reading_history = {track_reading_history}");
            updated_track = true;
            break;
        }
    }

    if !updated_track {
        let assignment = format!("track_reading_history = {track_reading_history}");
        match first_table_idx {
            Some(idx) => {
                lines.insert(idx, String::new());
                lines.insert(idx, assignment);
            },
            None => lines.push(assignment),
        }
    }

    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len() + 6);
    let mut in_anilist = false;
    let mut saw_anilist = false;
    let mut wrote_client_id = false;
    let mut wrote_access_token = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_anilist {
                if !wrote_client_id {
                    out_lines.push(format!("client_id = \"{}\"", client_id.replace('"', "\\\"")));
                }
                if !wrote_access_token {
                    out_lines.push(format!("access_token = \"{}\"", access_token.replace('"', "\\\"")));
                }
            }
            in_anilist = trimmed == "[anilist]";
            if in_anilist {
                saw_anilist = true;
            }
            out_lines.push(line);
            continue;
        }

        if in_anilist
            && !trimmed.starts_with('#')
            && let Some((lhs, _)) = trimmed.split_once('=')
        {
            match lhs.trim() {
                "client_id" => {
                    out_lines.push(format!("client_id = \"{}\"", client_id.replace('"', "\\\"")));
                    wrote_client_id = true;
                    continue;
                },
                "access_token" => {
                    out_lines.push(format!("access_token = \"{}\"", access_token.replace('"', "\\\"")));
                    wrote_access_token = true;
                    continue;
                },
                _ => {},
            }
        }
        out_lines.push(line);
    }

    if in_anilist {
        if !wrote_client_id {
            out_lines.push(format!("client_id = \"{}\"", client_id.replace('"', "\\\"")));
        }
        if !wrote_access_token {
            out_lines.push(format!("access_token = \"{}\"", access_token.replace('"', "\\\"")));
        }
    } else if !saw_anilist {
        if !out_lines.is_empty() && !out_lines.last().unwrap().is_empty() {
            out_lines.push(String::new());
        }
        out_lines.push("[anilist]".to_string());
        out_lines.push(format!("client_id = \"{}\"", client_id.replace('"', "\\\"")));
        out_lines.push(format!("access_token = \"{}\"", access_token.replace('"', "\\\"")));
    }

    let mut rendered = out_lines.join("\n");
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    fs::write(&config_path, rendered).map_err(|e| format!("Could not update config.toml: {e}"))
}

pub fn build_integrations_overview(state: &AppState) -> Result<IntegrationsOverviewDto, String> {
    let cloud_path = cloud_state_path(state)?;
    let cloud_state = CloudConnectionState::load_from_path(&cloud_path);
    let mal_path = mal_state_path(state)?;
    let mal_state = MalConnectionState::load_from_path(&mal_path);
    let config_dir = state.config_dir()?;

    let cloud_backend = cloud_state.backend.map(|kind| match kind {
        CloudBackendKind::GoogleDriveApi => "googleDrive".to_string(),
        CloudBackendKind::LocalCloudFolder => "localFolder".to_string(),
    });

    let cloud = CloudStatusDto {
        connected: cloud_state.is_connected(),
        backend: cloud_backend,
        summary: cloud_state.connection_summary(),
        account_email: cloud_state.account_email.clone(),
        account_display_name: cloud_state.account_display_name.clone(),
        local_folder_path: cloud_state.local_folder_path.as_ref().map(|p| p.display().to_string()),
        local_folder_label: cloud_state.local_folder_provider_label.clone(),
        has_embedded_oauth: has_embedded_google_oauth_config(),
        saved_client_id: cloud_state.client_id.clone(),
        last_sync_at: cloud_state.last_sync_at.clone(),
        last_sync_report: cloud_state.last_sync_report.clone().map(CloudSyncReportDto::from),
        detected_folders: list_detected_folders(),
    };

    let my_anime_list = MalStatusDto {
        connected: mal_state.is_connected(),
        username: mal_state.username.clone(),
        summary: mal_state.connection_summary(),
        has_embedded_oauth: has_embedded_mal_oauth_config(),
        saved_client_id: mal_state.client_id.clone(),
        last_sync_at: mal_state.last_sync_at.clone(),
        last_sync_report: mal_state.last_sync_report.clone().map(MalSyncReportDto::from),
    };

    Ok(IntegrationsOverviewDto {
        cloud,
        trackers: TrackersStatusDto {
            anilist: read_anilist_status(&config_dir),
            my_anime_list,
        },
        support_config: resolve_support_config(),
        feedback_config: build_feedback_config(),
        manga_tui_detected: detect_manga_tui_executable().is_some(),
    })
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'+' {
            bytes.push(b' ');
            i += 1;
        } else if raw[i] == b'%'
            && i + 2 < raw.len()
            && let Ok(hex) = std::str::from_utf8(&raw[i + 1..i + 3])
            && let Ok(val) = u8::from_str_radix(hex, 16)
        {
            bytes.push(val);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Parses either an OAuth callback URL (`mangora://oauth/google?code=...&state=...`) or a raw authorization code.
pub fn parse_oauth_callback_input(raw: &str) -> Result<(String, Option<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Authorization callback or code cannot be empty".to_string());
    }

    if let Some((_, after_q)) = trimmed.split_once('?') {
        let query = after_q.split('#').next().unwrap_or(after_q);
        let mut code: Option<String> = None;
        let mut state: Option<String> = None;
        let mut oauth_error: Option<String> = None;

        for pair in query.split('&') {
            let Some((k, v)) = pair.split_once('=') else {
                continue;
            };
            match k {
                "code" => code = Some(percent_decode(v)),
                "state" => state = Some(percent_decode(v)),
                "error" => oauth_error = Some(percent_decode(v)),
                _ => {},
            }
        }

        if let Some(err) = oauth_error {
            return Err(format!("OAuth provider returned an error: {err}"));
        }
        let code = code
            .filter(|c| !c.trim().is_empty())
            .ok_or_else(|| "No 'code' parameter found in callback URL".to_string())?;
        return Ok((code, state));
    }

    Ok((trimmed.to_string(), None))
}

pub async fn connect_google_drive(
    app: &AppHandle,
    state: &AppState,
    pending_oauth: &PendingOAuthStore,
    client_id_override: Option<String>,
    client_secret_override: Option<String>,
    use_deeplink: bool,
) -> Result<OAuthStartResultDto, String> {
    let cloud_path = cloud_state_path(state)?;
    let mut cloud_state = CloudConnectionState::load_from_path(&cloud_path);

    let effective_client_id = client_id_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| cloud_state.get_client_id())
        .ok_or_else(|| {
            "Google Drive Client ID is not configured in this build. Enter your OAuth Client ID or link a synced folder instead."
                .to_string()
        })?;

    let effective_secret = client_secret_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| cloud_state.get_client_secret());

    let mobile_flow = cfg!(target_os = "android") || use_deeplink;
    if mobile_flow {
        pending_oauth.cancel_and_remove("googleDrive");
        let client = GoogleDriveClient::new();
        let prepared = client.prepare_loopback_oauth(&effective_client_id).await?;
        let authorize_url = prepared.authorize_url.clone();
        let redirect_uri = prepared.redirect_uri.clone();
        let state_token = prepared.pkce.state.clone();
        let session = PendingOAuthSession {
            provider: "googleDrive".to_string(),
            client_id: effective_client_id,
            client_secret: effective_secret,
            redirect_uri: redirect_uri.clone(),
            code_verifier: prepared.pkce.code_verifier,
            state: state_token.clone(),
        };

        let app_handle = app.clone();
        let listener = prepared.listener;
        let expected_state = state_token.clone();
        let task_handle = tokio::spawn(async move {
            let (code, responder) = match accept_google_loopback_code(listener, &expected_state).await {
                Ok(pair) => pair,
                Err(err) => {
                    app_handle
                        .state::<PendingOAuthStore>()
                        .remove_matching_state("googleDrive", &expected_state);
                    let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                        provider: "googleDrive".to_string(),
                        ok: false,
                        error: Some(err),
                        overview: None,
                    });
                    return;
                },
            };

            let Some(pending_session) = app_handle
                .state::<PendingOAuthStore>()
                .take_matching_state("googleDrive", &expected_state)
            else {
                responder.send_success(Some("mangora://oauth/done")).await;
                return;
            };

            let client = GoogleDriveClient::new();
            match client
                .exchange_code_for_token(
                    &pending_session.client_id,
                    pending_session.client_secret.as_deref(),
                    &code,
                    &pending_session.code_verifier,
                    &pending_session.redirect_uri,
                )
                .await
            {
                Ok(tokens) => {
                    let about = client.get_about_user(&tokens.access_token).await.ok();
                    let app_state = app_handle.state::<AppState>();
                    let save_res = (|| -> Result<IntegrationsOverviewDto, String> {
                        let cloud_path = cloud_state_path(&app_state)?;
                        let mut cloud_state = CloudConnectionState::load_from_path(&cloud_path);
                        cloud_state.backend = Some(CloudBackendKind::GoogleDriveApi);
                        cloud_state.save_oauth_tokens(
                            &pending_session.client_id,
                            pending_session.client_secret.as_deref(),
                            &tokens.access_token,
                            tokens.refresh_token.as_deref(),
                        );
                        if let Some(user) = about.and_then(|a| a.user) {
                            cloud_state.account_display_name = user.display_name;
                            cloud_state.account_email = user.email_address;
                        }
                        cloud_state.save_to_path(&cloud_path)?;
                        build_integrations_overview(&app_state)
                    })();

                    match save_res {
                        Ok(overview) => {
                            responder.send_success(Some("mangora://oauth/done")).await;
                            let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                                provider: "googleDrive".to_string(),
                                ok: true,
                                error: None,
                                overview: Some(overview),
                            });
                        },
                        Err(err) => {
                            responder.send_error(&err, Some("mangora://oauth/done")).await;
                            let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                                provider: "googleDrive".to_string(),
                                ok: false,
                                error: Some(err),
                                overview: None,
                            });
                        },
                    }
                },
                Err(err) => {
                    responder.send_error(&err, Some("mangora://oauth/done")).await;
                    let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                        provider: "googleDrive".to_string(),
                        ok: false,
                        error: Some(err),
                        overview: None,
                    });
                },
            }
        });

        pending_oauth.insert_with_abort("googleDrive", session, Some(task_handle.abort_handle()));
        let _ = app.opener().open_url(&authorize_url, None::<&str>);
        let overview = build_integrations_overview(state)?;
        return Ok(OAuthStartResultDto {
            provider: "googleDrive".to_string(),
            mode: "loopback_async".to_string(),
            authorize_url: Some(authorize_url),
            redirect_uri: Some(redirect_uri),
            completed: false,
            overview,
        });
    }

    let client = GoogleDriveClient::new();
    let app_handle = app.clone();
    let tokens = client
        .run_loopback_oauth_flow(&effective_client_id, effective_secret.as_deref(), move |url| {
            let _ = app_handle.opener().open_url(url, None::<&str>);
        })
        .await?;

    let about = client.get_about_user(&tokens.access_token).await.ok();
    cloud_state.backend = Some(CloudBackendKind::GoogleDriveApi);
    cloud_state.save_oauth_tokens(
        &effective_client_id,
        effective_secret.as_deref(),
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
    );
    if let Some(user) = about.and_then(|a| a.user) {
        cloud_state.account_display_name = user.display_name;
        cloud_state.account_email = user.email_address;
    }
    cloud_state.save_to_path(&cloud_path)?;

    let overview = build_integrations_overview(state)?;
    Ok(OAuthStartResultDto {
        provider: "googleDrive".to_string(),
        mode: "loopback".to_string(),
        authorize_url: None,
        redirect_uri: None,
        completed: true,
        overview,
    })
}

pub fn connect_local_cloud_folder(
    state: &AppState,
    folder_path: String,
    provider_label: Option<String>,
) -> Result<IntegrationsOverviewDto, String> {
    let trimmed = folder_path.trim();
    if trimmed.is_empty() {
        return Err("Folder path cannot be empty".to_string());
    }
    let raw_path = PathBuf::from(trimmed);
    if raw_path.exists() && !raw_path.is_dir() {
        return Err(format!("Selected path '{}' is not a folder", raw_path.display()));
    }
    let target_dir =
        if raw_path.file_name().is_some_and(|n| n == CLOUD_ROOT_FOLDER_NAME) || raw_path.join("library_sync.json").is_file() {
            raw_path
        } else {
            raw_path.join(CLOUD_ROOT_FOLDER_NAME)
        };
    fs::create_dir_all(&target_dir).map_err(|e| format!("Could not create synced folder '{}': {e}", target_dir.display()))?;

    let cloud_path = cloud_state_path(state)?;
    let mut cloud_state = CloudConnectionState::load_from_path(&cloud_path);
    cloud_state.backend = Some(CloudBackendKind::LocalCloudFolder);
    cloud_state.local_folder_path = Some(target_dir);
    cloud_state.local_folder_provider_label = provider_label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some("Synced Folder".to_string()));
    cloud_state.save_to_path(&cloud_path)?;

    build_integrations_overview(state)
}

pub async fn run_cloud_sync_now(state: &AppState, mode_raw: &str) -> Result<CloudSyncActionResultDto, String> {
    let mode = match mode_raw.trim() {
        "fullSync" => CloudSyncMode::FullSync,
        "pushDownloads" => CloudSyncMode::PushDownloadsOnly,
        "pullDownloads" => CloudSyncMode::PullDownloadsOnly,
        _ => CloudSyncMode::LibraryOnly,
    };

    let cloud_path = cloud_state_path(state)?;
    let mut cloud_state = CloudConnectionState::load_from_path(&cloud_path);
    let db_path = state.db_path()?;
    let downloads_dir = state.downloads_dir()?;
    let _ = state.open_db_connection()?;
    let drive_client = GoogleDriveClient::new();

    let report = run_cloud_sync_with_paths(&mut cloud_state, mode, &drive_client, &db_path, &downloads_dir, &mut |_| {}).await?;
    cloud_state.last_sync_at = Some(report.synced_at.clone());
    cloud_state.last_sync_report = Some(report.clone());
    cloud_state.save_to_path(&cloud_path)?;

    Ok(CloudSyncActionResultDto {
        report: CloudSyncReportDto::from(report),
        overview: build_integrations_overview(state)?,
        app_state: state.snapshot()?,
    })
}

pub fn disconnect_cloud(state: &AppState) -> Result<IntegrationsOverviewDto, String> {
    let cloud_path = cloud_state_path(state)?;
    let mut cloud_state = CloudConnectionState::load_from_path(&cloud_path);
    cloud_state.clear_credentials();
    cloud_state.save_to_path(&cloud_path)?;
    build_integrations_overview(state)
}

/// Extracts a raw AniList Bearer token from a raw token, `"Bearer <tok>"`, or an OAuth redirect URL (`#access_token=...`).
pub fn extract_anilist_access_token(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(after_bearer) = trimmed.strip_prefix("Bearer ").or_else(|| trimmed.strip_prefix("bearer ")) {
        return after_bearer.trim().to_string();
    }
    if let Some((_, query_or_frag)) = trimmed.split_once('#').or_else(|| trimmed.split_once('?')) {
        for pair in query_or_frag.split('&') {
            if let Some((k, v)) = pair.split_once('=')
                && k == "access_token"
            {
                let decoded = percent_decode(v);
                if !decoded.trim().is_empty() {
                    return decoded.trim().to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

pub async fn save_anilist_credentials(
    state: &AppState,
    client_id: String,
    access_token: String,
    verify: bool,
) -> Result<IntegrationsOverviewDto, String> {
    let clean_id = {
        let trimmed = client_id.trim();
        if trimmed.is_empty() { DEFAULT_ANILIST_CLIENT_ID.to_string() } else { trimmed.to_string() }
    };
    let clean_token = extract_anilist_access_token(&access_token);
    if clean_token.is_empty() {
        return Err("AniList Access Token is required".to_string());
    }

    if verify {
        let base_url = BASE_ANILIST_API_URL.parse().map_err(|e| format!("Invalid AniList URL: {e}"))?;
        let checker = Anilist::new(base_url).with_client_id(clean_id.clone()).with_token(clean_token.clone());
        let viewer = checker
            .verify_viewer_token()
            .await
            .map_err(|e| format!("Could not verify AniList credentials: {e}"))?;
        if viewer.is_none() {
            return Err("AniList rejected the provided Access Token".to_string());
        }
    }

    let mut keyring_saved = false;
    if !cfg!(test) {
        let storage = KeyringStorage::new();
        if storage.save_secret(SECRET_ANILIST_CLIENT_ID, &clean_id).is_ok()
            && storage.save_secret(SECRET_ANILIST_ACCESS_TOKEN, &clean_token).is_ok()
        {
            keyring_saved = true;
        }
    }

    let config_dir = state.config_dir()?;
    if keyring_saved {
        update_anilist_in_config_dir(&config_dir, true, "", "")?;
    } else {
        // Android or sandbox fallback when OS keyring is unavailable
        update_anilist_in_config_dir(&config_dir, true, &clean_id, &clean_token)?;
    }

    build_integrations_overview(state)
}

pub fn disconnect_anilist(state: &AppState) -> Result<IntegrationsOverviewDto, String> {
    if !cfg!(test) {
        let storage = KeyringStorage::new();
        let _ = storage.remove_secret(SECRET_ANILIST_CLIENT_ID);
        let _ = storage.remove_secret(SECRET_ANILIST_ACCESS_TOKEN);
    }
    let config_dir = state.config_dir()?;
    update_anilist_in_config_dir(&config_dir, false, "", "")?;
    build_integrations_overview(state)
}

pub async fn connect_myanimelist(
    app: &AppHandle,
    state: &AppState,
    pending_oauth: &PendingOAuthStore,
    client_id_override: Option<String>,
    client_secret_override: Option<String>,
    use_deeplink: bool,
) -> Result<OAuthStartResultDto, String> {
    let mal_path = mal_state_path(state)?;
    let mut mal_state = MalConnectionState::load_from_path(&mal_path);

    let effective_client_id = client_id_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| mal_state.get_client_id())
        .ok_or_else(|| {
            "MyAnimeList Client ID is not configured in this build. Enter your Client ID in Advanced options.".to_string()
        })?;

    let effective_secret = client_secret_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| mal_state.get_client_secret());

    let mobile_flow = cfg!(target_os = "android") || use_deeplink;
    if mobile_flow {
        pending_oauth.cancel_and_remove("myAnimeList");
        tokio::task::yield_now().await;
        let client = MyAnimeListClient::new();
        let prepared = client.prepare_loopback_oauth(&effective_client_id).await?;
        let authorize_url = prepared.authorize_url.clone();
        let redirect_uri = prepared.redirect_uri.clone();
        let state_token = prepared.pkce.state.clone();
        let session = PendingOAuthSession {
            provider: "myAnimeList".to_string(),
            client_id: effective_client_id,
            client_secret: effective_secret,
            redirect_uri: redirect_uri.clone(),
            code_verifier: prepared.pkce.code_verifier,
            state: state_token.clone(),
        };

        let app_handle = app.clone();
        let listener = prepared.listener;
        let expected_state = state_token.clone();
        let task_handle = tokio::spawn(async move {
            let (code, responder) = match accept_mal_loopback_code(listener, &expected_state).await {
                Ok(pair) => pair,
                Err(err) => {
                    app_handle
                        .state::<PendingOAuthStore>()
                        .remove_matching_state("myAnimeList", &expected_state);
                    let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                        provider: "myAnimeList".to_string(),
                        ok: false,
                        error: Some(err),
                        overview: None,
                    });
                    return;
                },
            };

            let Some(pending_session) = app_handle
                .state::<PendingOAuthStore>()
                .take_matching_state("myAnimeList", &expected_state)
            else {
                responder.send_success(Some("mangora://oauth/done")).await;
                return;
            };

            let client = MyAnimeListClient::new();
            match client
                .exchange_code_for_token(
                    &pending_session.client_id,
                    pending_session.client_secret.as_deref(),
                    &code,
                    &pending_session.code_verifier,
                    &pending_session.redirect_uri,
                )
                .await
            {
                Ok(tokens) => {
                    let profile = client.get_my_profile(&tokens.access_token).await.ok();
                    let app_state = app_handle.state::<AppState>();
                    let save_res = (|| -> Result<IntegrationsOverviewDto, String> {
                        let mal_path = mal_state_path(&app_state)?;
                        let mut mal_state = MalConnectionState::load_from_path(&mal_path);
                        mal_state.save_oauth_tokens(
                            &pending_session.client_id,
                            pending_session.client_secret.as_deref(),
                            &tokens.access_token,
                            tokens.refresh_token.as_deref(),
                        );
                        if let Some(p) = profile {
                            mal_state.user_id = Some(p.id);
                            mal_state.username = Some(p.name);
                        }
                        mal_state.save_to_path(&mal_path)?;
                        build_integrations_overview(&app_state)
                    })();

                    match save_res {
                        Ok(overview) => {
                            responder.send_success(Some("mangora://oauth/done")).await;
                            let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                                provider: "myAnimeList".to_string(),
                                ok: true,
                                error: None,
                                overview: Some(overview),
                            });
                        },
                        Err(err) => {
                            responder.send_error(&err, Some("mangora://oauth/done")).await;
                            let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                                provider: "myAnimeList".to_string(),
                                ok: false,
                                error: Some(err),
                                overview: None,
                            });
                        },
                    }
                },
                Err(err) => {
                    responder.send_error(&err, Some("mangora://oauth/done")).await;
                    let _ = app_handle.emit(OAUTH_COMPLETED_EVENT, OAuthCompletedEventDto {
                        provider: "myAnimeList".to_string(),
                        ok: false,
                        error: Some(err),
                        overview: None,
                    });
                },
            }
        });

        pending_oauth.insert_with_abort("myAnimeList", session, Some(task_handle.abort_handle()));
        let _ = app.opener().open_url(&authorize_url, None::<&str>);
        let overview = build_integrations_overview(state)?;
        return Ok(OAuthStartResultDto {
            provider: "myAnimeList".to_string(),
            mode: "loopback_async".to_string(),
            authorize_url: Some(authorize_url),
            redirect_uri: Some(redirect_uri),
            completed: false,
            overview,
        });
    }

    let client = MyAnimeListClient::new();
    let app_handle = app.clone();
    let tokens = client
        .run_loopback_oauth_flow(&effective_client_id, effective_secret.as_deref(), move |url| {
            let _ = app_handle.opener().open_url(url, None::<&str>);
        })
        .await?;

    let profile = client.get_my_profile(&tokens.access_token).await.ok();
    mal_state.save_oauth_tokens(
        &effective_client_id,
        effective_secret.as_deref(),
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
    );
    if let Some(p) = profile {
        mal_state.user_id = Some(p.id);
        mal_state.username = Some(p.name);
    }
    mal_state.save_to_path(&mal_path)?;

    Ok(OAuthStartResultDto {
        provider: "myAnimeList".to_string(),
        mode: "loopback".to_string(),
        authorize_url: None,
        redirect_uri: None,
        completed: true,
        overview: build_integrations_overview(state)?,
    })
}

pub async fn complete_oauth_callback(
    state: &AppState,
    pending_oauth: &PendingOAuthStore,
    provider: &str,
    callback_input: &str,
) -> Result<IntegrationsOverviewDto, String> {
    let preview = pending_oauth
        .get(provider)
        .ok_or_else(|| format!("No active OAuth session found for '{provider}'"))?;

    let (code, returned_state) = parse_oauth_callback_input(callback_input)?;
    if let Some(st) = returned_state
        && st != preview.state
    {
        return Err("OAuth security error: state parameter mismatch".to_string());
    }

    let pending = pending_oauth
        .take(provider)
        .ok_or_else(|| format!("No active OAuth session found for '{provider}'"))?;

    match provider {
        "googleDrive" => {
            let client = GoogleDriveClient::new();
            let tokens = client
                .exchange_code_for_token(
                    &pending.client_id,
                    pending.client_secret.as_deref(),
                    &code,
                    &pending.code_verifier,
                    &pending.redirect_uri,
                )
                .await?;
            let about = client.get_about_user(&tokens.access_token).await.ok();
            let cloud_path = cloud_state_path(state)?;
            let mut cloud_state = CloudConnectionState::load_from_path(&cloud_path);
            cloud_state.backend = Some(CloudBackendKind::GoogleDriveApi);
            cloud_state.save_oauth_tokens(
                &pending.client_id,
                pending.client_secret.as_deref(),
                &tokens.access_token,
                tokens.refresh_token.as_deref(),
            );
            if let Some(user) = about.and_then(|a| a.user) {
                cloud_state.account_display_name = user.display_name;
                cloud_state.account_email = user.email_address;
            }
            cloud_state.save_to_path(&cloud_path)?;
        },
        "myAnimeList" => {
            let client = MyAnimeListClient::new();
            let tokens = client
                .exchange_code_for_token(
                    &pending.client_id,
                    pending.client_secret.as_deref(),
                    &code,
                    &pending.code_verifier,
                    &pending.redirect_uri,
                )
                .await?;
            let profile = client.get_my_profile(&tokens.access_token).await.ok();
            let mal_path = mal_state_path(state)?;
            let mut mal_state = MalConnectionState::load_from_path(&mal_path);
            mal_state.save_oauth_tokens(
                &pending.client_id,
                pending.client_secret.as_deref(),
                &tokens.access_token,
                tokens.refresh_token.as_deref(),
            );
            if let Some(p) = profile {
                mal_state.user_id = Some(p.id);
                mal_state.username = Some(p.name);
            }
            mal_state.save_to_path(&mal_path)?;
        },
        other => return Err(format!("Unsupported OAuth provider: '{other}'")),
    }

    pending_oauth.remove(provider);
    build_integrations_overview(state)
}

pub async fn sync_myanimelist_now(state: &AppState) -> Result<MalSyncActionResultDto, String> {
    let mal_path = mal_state_path(state)?;
    let mut mal_state = MalConnectionState::load_from_path(&mal_path);
    let db_path = state.db_path()?;
    let _ = state.open_db_connection()?;
    let client = MyAnimeListClient::new();
    let report = client.sync_with_database(&mut mal_state, &db_path).await?;
    mal_state.last_sync_at = Some(report.synced_at.clone());
    mal_state.last_sync_report = Some(report.clone());
    mal_state.save_to_path(&mal_path)?;

    Ok(MalSyncActionResultDto {
        report: MalSyncReportDto::from(report),
        overview: build_integrations_overview(state)?,
        app_state: state.snapshot()?,
    })
}

pub fn disconnect_myanimelist(state: &AppState) -> Result<IntegrationsOverviewDto, String> {
    let mal_path = mal_state_path(state)?;
    let mut mal_state = MalConnectionState::load_from_path(&mal_path);
    mal_state.clear_credentials();
    mal_state.save_to_path(&mal_path)?;
    build_integrations_overview(state)
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAssetDto {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseApiDto {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    published_at: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GithubReleaseAssetDto>,
}

fn find_android_apk_asset_url(assets: &[GithubReleaseAssetDto]) -> Option<String> {
    assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with("-android-arm64-v8a.apk"))
        .or_else(|| assets.iter().find(|a| a.name.to_ascii_lowercase().ends_with(".apk")))
        .map(|a| a.browser_download_url.clone())
}

fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let stripped = raw.trim().trim_start_matches("mangora-").trim_start_matches(['v', 'V']);
    let mut parts = stripped
        .split('.')
        .map(|part| part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

pub async fn check_mangora_updates_at(api_base_url: &str, current_version: &str) -> Result<UpdateCheckResultDto, String> {
    let mut builder = Client::builder().timeout(Duration::from_secs(10)).user_agent("mangora-updater");
    if cfg!(test) {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| format!("Could not initialize update HTTP client: {e}"))?;

    let url = format!("{}/releases?per_page=10", api_base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Update check request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub releases returned HTTP {}", resp.status()));
    }

    let releases: Vec<GithubReleaseApiDto> = resp.json().await.map_err(|e| format!("Invalid GitHub releases payload: {e}"))?;

    let non_draft: Vec<&GithubReleaseApiDto> = releases.iter().filter(|r| !r.draft && !r.prerelease).collect();
    let chosen = non_draft
        .iter()
        .copied()
        .find(|r| r.tag_name.to_ascii_lowercase().starts_with("mangora-v"))
        .or_else(|| non_draft.first().copied())
        .ok_or_else(|| "No published stable releases found yet".to_string())?;

    let update_available = match (parse_semver(&chosen.tag_name), parse_semver(current_version)) {
        (Some(latest), Some(cur)) => latest > cur,
        _ => chosen.tag_name.trim_start_matches("mangora-").trim_start_matches('v') != current_version.trim(),
    };

    let android_apk_url = find_android_apk_asset_url(&chosen.assets);

    Ok(UpdateCheckResultDto {
        current_version: current_version.to_string(),
        latest_version: chosen.tag_name.clone(),
        update_available,
        release_url: chosen
            .html_url
            .clone()
            .unwrap_or_else(|| format!("{DEFAULT_GITHUB_URL}/releases/tag/{}", chosen.tag_name)),
        android_apk_url,
        release_notes: sanitize_release_notes(chosen.body.as_deref().unwrap_or("")),
        published_at: chosen.published_at.clone(),
        checked_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().to_string(),
    })
}

pub fn export_library_backup_json(state: &AppState) -> Result<String, String> {
    let conn = state.open_db_connection()?;
    let db = manga_core::backend::database::Database::new(&conn);
    let snapshot = db.export_library_sync_snapshot().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&snapshot).map_err(|e| format!("Could not serialize library backup: {e}"))
}

pub fn export_library_backup_to_path(state: &AppState, target_dir: &Path) -> Result<String, String> {
    let json = export_library_backup_json(state)?;
    fs::create_dir_all(target_dir).map_err(|e| format!("Could not create export directory: {e}"))?;
    let out_path = target_dir.join(manga_core::backend::cloud::CLOUD_LIBRARY_JSON_NAME);
    fs::write(&out_path, json).map_err(|e| format!("Could not write backup file '{}': {e}", out_path.display()))?;
    Ok(out_path.display().to_string())
}

pub fn import_library_backup_json(state: &AppState, json_payload: &str) -> Result<PortableBackupImportResultDto, String> {
    let trimmed = json_payload.trim();
    if trimmed.is_empty() {
        return Err("Backup JSON file is empty".to_string());
    }
    let snapshot: manga_core::backend::cloud::LibrarySyncSnapshot =
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid library_sync.json backup file: {e}"))?;

    let merged = {
        let conn = state.open_db_connection()?;
        let db = manga_core::backend::database::Database::new(&conn);
        db.import_and_merge_library_sync_snapshot(&snapshot).map_err(|e| e.to_string())?
    };

    Ok(PortableBackupImportResultDto {
        total_mangas: merged.mangas.len(),
        total_read_chapters: merged.chapters.iter().filter(|c| c.is_read).count(),
        total_favorites: merged.favorites.len(),
        app_state: state.snapshot()?,
        library: state.get_library_overview(None)?,
    })
}

pub async fn check_mangora_updates(current_version: &str) -> Result<UpdateCheckResultDto, String> {
    check_mangora_updates_at(DEFAULT_RELEASES_API_URL, current_version).await
}

pub fn detect_manga_tui_executable() -> Option<PathBuf> {
    #[cfg(not(target_os = "android"))]
    {
        let bin_name = if cfg!(target_os = "windows") { "manga-tui.exe" } else { "manga-tui" };

        if let Ok(current) = std::env::current_exe()
            && let Some(dir) = current.parent()
        {
            let sibling = dir.join(bin_name);
            if sibling.is_file() {
                return Some(sibling);
            }
        }

        if let Some(path_var) = std::env::var_os("PATH") {
            for entry in std::env::split_paths(&path_var) {
                let candidate = entry.join(bin_name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
    #[cfg(target_os = "android")]
    {
        None
    }
}

pub fn launch_manga_tui_desktop() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let exe = detect_manga_tui_executable()
            .ok_or_else(|| "manga-tui executable was not found in PATH or beside Mangora".to_string())?;
        Command::new("cmd")
            .args(["/C", "start", "\"Manga TUI\""])
            .arg(&exe)
            .spawn()
            .map_err(|e| format!("Could not launch manga-tui: {e}"))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "android")))]
    {
        use std::process::Command;
        let exe = detect_manga_tui_executable()
            .ok_or_else(|| "manga-tui executable was not found in PATH or beside Mangora".to_string())?;
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .args(["-a", "Terminal"])
                .arg(&exe)
                .spawn()
                .map_err(|e| format!("Could not launch manga-tui in Terminal: {e}"))?;
            return Ok(());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let terms = [
                ("x-terminal-emulator", vec!["-e"]),
                ("kitty", vec!["-e"]),
                ("alacritty", vec!["-e"]),
                ("gnome-terminal", vec!["--"]),
                ("konsole", vec!["-e"]),
            ];
            for (term, prefix) in terms {
                if Command::new(term).args(&prefix).arg(&exe).spawn().is_ok() {
                    return Ok(());
                }
            }
            Err("No supported terminal emulator found to launch manga-tui".to_string())
        }
    }
    #[cfg(target_os = "android")]
    {
        Err("Launching the terminal app is only available on desktop".to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use httpmock::Method::GET;
    use httpmock::MockServer;
    use manga_core::backend::StorageMode;
    use manga_core::backend::manga_provider::{Languages, MangaProviders};
    use manga_core::config::UiLanguage;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = std::env::temp_dir().join(format!("mangora_hito4_{label}_{nanos}"));
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

    fn make_state(root: &Path) -> AppState {
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
    fn parse_oauth_callback_input_supports_deeplink_and_loopback_urls_and_raw_code() {
        let (code, state) = parse_oauth_callback_input("mangora://oauth/google?code=abc%2F123&state=st_42").unwrap();
        assert_eq!(code, "abc/123");
        assert_eq!(state.as_deref(), Some("st_42"));

        let (mal_code, mal_state) =
            parse_oauth_callback_input("http://127.0.0.1:8765/callback?code=mal_code_1&state=mal_st").unwrap();
        assert_eq!(mal_code, "mal_code_1");
        assert_eq!(mal_state.as_deref(), Some("mal_st"));

        let (gd_code, gd_state) = parse_oauth_callback_input("http://127.0.0.1:54321/?code=gd_code_1&state=gd_st").unwrap();
        assert_eq!(gd_code, "gd_code_1");
        assert_eq!(gd_state.as_deref(), Some("gd_st"));

        let (raw_code, raw_state) = parse_oauth_callback_input("  plain-auth-code  ").unwrap();
        assert_eq!(raw_code, "plain-auth-code");
        assert_eq!(raw_state, None);

        assert!(parse_oauth_callback_input("mangora://oauth/mal?error=access_denied").is_err());
        assert!(parse_oauth_callback_input("   ").is_err());
    }

    #[test]
    fn extract_anilist_access_token_handles_raw_bearer_and_redirect_url() {
        assert_eq!(extract_anilist_access_token("  tok_raw_123  "), "tok_raw_123");
        assert_eq!(extract_anilist_access_token("Bearer tok_bearer_456"), "tok_bearer_456");
        assert_eq!(
            extract_anilist_access_token("https://anilist.co/api/v2/oauth/pin#access_token=jwt.part.sig&token_type=Bearer"),
            "jwt.part.sig"
        );
    }

    #[test]
    fn pending_oauth_store_matches_state_and_cancels_cleanly() {
        let store = PendingOAuthStore::new();
        store.insert("googleDrive", PendingOAuthSession {
            provider: "googleDrive".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            redirect_uri: "http://127.0.0.1:12345".to_string(),
            code_verifier: "ver".to_string(),
            state: "st_1".to_string(),
        });
        assert!(store.take_matching_state("googleDrive", "wrong").is_none());
        assert!(store.get("googleDrive").is_some());
        assert_eq!(store.take_matching_state("googleDrive", "st_1").unwrap().client_id, "cid");
        assert!(store.get("googleDrive").is_none());

        store.insert("myAnimeList", PendingOAuthSession {
            provider: "myAnimeList".to_string(),
            client_id: "mal_cid".to_string(),
            client_secret: None,
            redirect_uri: "http://127.0.0.1:8765/callback".to_string(),
            code_verifier: "ver2".to_string(),
            state: "st_2".to_string(),
        });
        store.cancel_and_remove("myAnimeList");
        assert!(store.get("myAnimeList").is_none());
    }

    #[tokio::test]
    async fn complete_oauth_callback_rejects_mismatched_state_before_network() {
        let temp = TempDirGuard::new("csrf");
        let state = make_state(&temp.path);
        let store = PendingOAuthStore::new();
        store.insert("googleDrive", PendingOAuthSession {
            provider: "googleDrive".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            redirect_uri: "mangora://oauth/google".to_string(),
            code_verifier: "ver".to_string(),
            state: "expected-state".to_string(),
        });

        let err = complete_oauth_callback(&state, &store, "googleDrive", "mangora://oauth/google?code=123&state=tampered")
            .await
            .unwrap_err();
        assert!(err.contains("state parameter mismatch"));
    }

    #[tokio::test]
    async fn check_mangora_updates_prioritizes_mangora_tag_and_ignores_drafts() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/releases");
                then.status(200).json_body(json!([
                    {
                        "tag_name": "mangora-v9.9.9",
                        "draft": true,
                        "prerelease": false,
                        "body": "Borrador interno"
                    },
                    {
                        "tag_name": "v0.3.1",
                        "draft": false,
                        "prerelease": false,
                        "body": "Release TUI"
                    },
                    {
                        "tag_name": "mangora-v0.2.0",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-11-24T10:00:00Z",
                        "body": "<!-- manga-tui-meta: commit=abc -->\nNotas limpias de Mangora 0.2.0",
                        "html_url": "https://github.com/tomahawk1103/manga-tui/releases/tag/mangora-v0.2.0",
                        "assets": [
                            {
                                "name": "Mangora-0.2.0-android-arm64-v8a.apk",
                                "browser_download_url": "https://github.com/tomahawk1103/manga-tui/releases/download/mangora-v0.2.0/Mangora-0.2.0-android-arm64-v8a.apk"
                            }
                        ]
                    }
                ]));
            })
            .await;

        let update = check_mangora_updates_at(&server.base_url(), "0.1.0").await.unwrap();
        mock.assert_async().await;
        assert!(update.update_available);
        assert_eq!(update.latest_version, "mangora-v0.2.0");
        assert_eq!(
            update.android_apk_url.as_deref(),
            Some("https://github.com/tomahawk1103/manga-tui/releases/download/mangora-v0.2.0/Mangora-0.2.0-android-arm64-v8a.apk")
        );
        assert_eq!(update.release_notes, "Notas limpias de Mangora 0.2.0");

        let up_to_date = check_mangora_updates_at(&server.base_url(), "0.2.0").await.unwrap();
        assert!(!up_to_date.update_available);
    }

    #[tokio::test]
    async fn local_cloud_folder_sync_end_to_end_creates_snapshot() {
        let temp = TempDirGuard::new("local_cloud_sync");
        let state = make_state(&temp.path);
        fs::create_dir_all(state.data_dir().unwrap()).unwrap();
        fs::create_dir_all(state.downloads_dir().unwrap()).unwrap();
        let cloud_dir = temp.path.join("SyncedCloud");

        let overview = connect_local_cloud_folder(&state, cloud_dir.display().to_string(), Some("OneDrive".to_string())).unwrap();
        assert!(overview.cloud.connected);
        assert_eq!(overview.cloud.backend.as_deref(), Some("localFolder"));

        let res = run_cloud_sync_now(&state, "libraryOnly").await.unwrap();
        assert!(res.overview.cloud.last_sync_at.is_some());
        assert!(cloud_dir.join("MangaTUI/library_sync.json").is_file());

        let after_disconnect = disconnect_cloud(&state).unwrap();
        assert!(!after_disconnect.cloud.connected);
    }

    #[tokio::test]
    async fn anilist_credentials_fallback_saves_and_disconnects_in_config_toml() {
        let temp = TempDirGuard::new("anilist_cfg");
        let state = make_state(&temp.path);

        let connected = save_anilist_credentials(&state, "".to_string(), "tok_9999".to_string(), false)
            .await
            .unwrap();
        assert!(connected.trackers.anilist.connected);
        assert!(connected.trackers.anilist.track_reading_history);
        assert_eq!(connected.trackers.anilist.client_id_hint.as_deref(), Some("••••8641"));

        let disconnected = disconnect_anilist(&state).unwrap();
        assert!(!disconnected.trackers.anilist.connected);
        assert!(!disconnected.trackers.anilist.track_reading_history);
    }

    #[test]
    fn portable_backup_export_and_import_merges_library_losslessly() {
        let src_temp = TempDirGuard::new("backup_src");
        let dst_temp = TempDirGuard::new("backup_dst");
        let src_state = make_state(&src_temp.path);
        let dst_state = make_state(&dst_temp.path);

        src_state
            .toggle_chapter_read(crate::state::ToggleChapterReadPayload {
                manga_id: "m-backup-1".to_string(),
                provider: "inmanga".to_string(),
                manga_title: "Dandadan".to_string(),
                cover_url: Some("https://example.com/dd.jpg".to_string()),
                chapter_id: "ch-backup-1".to_string(),
                chapter_title: "Cap 1".to_string(),
                translated_language: "es".to_string(),
            })
            .unwrap();
        src_state
            .toggle_library_favorite(crate::state::ToggleFavoritePayload {
                manga_id: "m-backup-1".to_string(),
                provider: "inmanga".to_string(),
                title: "Dandadan".to_string(),
                cover_url: Some("https://example.com/dd.jpg".to_string()),
            })
            .unwrap();

        let exported_json = export_library_backup_json(&src_state).unwrap();
        let exported_path = export_library_backup_to_path(&src_state, &dst_temp.path.join("exports")).unwrap();
        let disk_json = fs::read_to_string(&exported_path).unwrap();
        let exported_snap: manga_core::backend::cloud::LibrarySyncSnapshot = serde_json::from_str(&exported_json).unwrap();
        let mut disk_snap: manga_core::backend::cloud::LibrarySyncSnapshot = serde_json::from_str(&disk_json).unwrap();
        disk_snap.synced_at = exported_snap.synced_at.clone();
        assert_eq!(disk_snap, exported_snap);

        let imported = import_library_backup_json(&dst_state, &disk_json).unwrap();
        assert_eq!(imported.total_mangas, 1);
        assert_eq!(imported.total_read_chapters, 1);
        assert_eq!(imported.total_favorites, 1);
        assert_eq!(imported.app_state.library.library_mangas, 1);
        assert_eq!(imported.app_state.library.favorites, 1);
        assert_eq!(imported.library.favorites.len(), 1);
    }

    #[test]
    fn resolve_support_config_includes_https_website_url() {
        let cfg = resolve_support_config();
        assert_eq!(cfg.website_url, DEFAULT_WEBSITE_URL);
        assert!(cfg.website_url.starts_with("https://"));
    }
}
