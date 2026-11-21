//! Cloud synchronization module for `manga-tui`.
//!
//! Provides cross-platform synchronization of reading history, bookmarks, favorites,
//! Plan to Read lists, and downloaded chapters (`.cbz`, `.epub`, `.pdf`) using:
//!
//! 1. **Google Drive REST API v3** with OAuth 2.0 + PKCE (compatible with Windows, Linux, macOS, and future Android / iOS builds
//!    sharing the Rust core).
//! 2. **Local Cloud Drive Bridge** (auto-detection of iCloud Drive, Google Drive Desktop, OneDrive, or custom synced folder) using
//!    the exact same `MangaTUI/` cloud structure:
//!    - `MangaTUI/library_sync.json` (portable cross-platform JSON snapshot for smart bidirectional merge)
//!    - `MangaTUI/manga-tui-history.db` (SQLite database backup)
//!    - `MangaTUI/downloads/<manga_folder>/<chapter_file>` (`.cbz`, `.epub`, `.pdf` chapters)

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::backend::AppDirectories;
use crate::backend::database::Database;
use crate::backend::manga_provider::MangaProviders;
use crate::backend::secrets::SecretStorage;
use crate::backend::secrets::keyring::KeyringStorage;
use crate::i18n::tr;

pub const LIBRARY_SYNC_SCHEMA_VERSION: u32 = 1;
pub const CLOUD_ROOT_FOLDER_NAME: &str = "MangaTUI";
pub const CLOUD_DOWNLOADS_FOLDER_NAME: &str = "downloads";
pub const CLOUD_LIBRARY_JSON_NAME: &str = "library_sync.json";
pub const CLOUD_DB_BACKUP_NAME: &str = "manga-tui-history.db";
pub const CLOUD_STATE_FILE_NAME: &str = "cloud_state.json";

pub const GOOGLE_OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_DRIVE_API_BASE_URL: &str = "https://www.googleapis.com/drive/v3";
pub const GOOGLE_DRIVE_UPLOAD_BASE_URL: &str = "https://www.googleapis.com/upload/drive/v3";
pub const GOOGLE_DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

pub const SECRET_GDRIVE_CLIENT_ID: &str = "gdrive_client_id";
pub const SECRET_GDRIVE_CLIENT_SECRET: &str = "gdrive_client_secret";
pub const SECRET_GDRIVE_ACCESS_TOKEN: &str = "gdrive_access_token";
pub const SECRET_GDRIVE_REFRESH_TOKEN: &str = "gdrive_refresh_token";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncMangaRecord {
    pub id: String,
    pub title: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_read: Option<String>,
    pub deleted_at: Option<String>,
    pub img_url: Option<String>,
    pub manga_provider: String,
    pub in_reading_history: bool,
    pub in_plan_to_read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncChapterRecord {
    pub id: String,
    pub title: String,
    pub manga_id: String,
    pub is_read: bool,
    pub is_downloaded: bool,
    pub is_bookmarked: bool,
    pub translated_language: Option<String>,
    pub number_page_bookmarked: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncFavoriteRecord {
    pub manga_id: String,
    pub provider: String,
    pub title: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncDismissedRecord {
    pub manga_id: String,
    pub provider: String,
    pub chapter_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncSearchRecord {
    pub query: String,
    pub searched_at: i64,
}

/// Cross-platform library snapshot stored as `library_sync.json` in the cloud.
/// Designed so both desktop (`manga-tui`) and future mobile clients (Android / iOS)
/// can perform lossless bidirectional merges without SQLite schema coupling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibrarySyncSnapshot {
    pub schema_version: u32,
    pub app_version: String,
    pub synced_at: String,
    pub mangas: Vec<SyncMangaRecord>,
    pub chapters: Vec<SyncChapterRecord>,
    pub favorites: Vec<SyncFavoriteRecord>,
    pub dismissed_notifications: Vec<SyncDismissedRecord>,
    pub search_history: Vec<SyncSearchRecord>,
}

impl Default for LibrarySyncSnapshot {
    fn default() -> Self {
        Self {
            schema_version: LIBRARY_SYNC_SCHEMA_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            synced_at: Utc::now().to_rfc3339(),
            mangas: Vec::new(),
            chapters: Vec::new(),
            favorites: Vec::new(),
            dismissed_notifications: Vec::new(),
            search_history: Vec::new(),
        }
    }
}

fn pick_newer_timestamp(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(sa), Some(sb)) => {
            if sb > sa {
                Some(sb.clone())
            } else {
                Some(sa.clone())
            }
        },
        (Some(sa), None) => Some(sa.clone()),
        (None, Some(sb)) => Some(sb.clone()),
        (None, None) => None,
    }
}

fn pick_older_timestamp(a: &Option<String>, b: &Option<String>) -> Option<String> {
    match (a, b) {
        (Some(sa), Some(sb)) => {
            if sb < sa {
                Some(sb.clone())
            } else {
                Some(sa.clone())
            }
        },
        (Some(sa), None) => Some(sa.clone()),
        (None, Some(sb)) => Some(sb.clone()),
        (None, None) => None,
    }
}

/// Performs a lossless bidirectional merge between a local and a remote `LibrarySyncSnapshot`.
pub fn merge_library_snapshots(local: &LibrarySyncSnapshot, remote: &LibrarySyncSnapshot) -> LibrarySyncSnapshot {
    let mut mangas_by_id: BTreeMap<String, SyncMangaRecord> = BTreeMap::new();
    // Tracks whether remote had a strictly newer `last_read` timestamp for a given manga_id.
    let mut remote_is_newer_for_manga: HashMap<String, bool> = HashMap::new();

    for m in &local.mangas {
        mangas_by_id.insert(m.id.clone(), m.clone());
        remote_is_newer_for_manga.insert(m.id.clone(), false);
    }

    for rm in &remote.mangas {
        match mangas_by_id.get_mut(&rm.id) {
            Some(lm) => {
                let remote_newer = match (&lm.last_read, &rm.last_read) {
                    (Some(l_ts), Some(r_ts)) => r_ts > l_ts,
                    (None, Some(_)) => true,
                    _ => false,
                };
                remote_is_newer_for_manga.insert(rm.id.clone(), remote_newer);

                if remote_newer && !rm.title.trim().is_empty() {
                    lm.title = rm.title.clone();
                }
                lm.created_at = pick_older_timestamp(&lm.created_at, &rm.created_at);
                lm.updated_at = pick_newer_timestamp(&lm.updated_at, &rm.updated_at);
                lm.last_read = pick_newer_timestamp(&lm.last_read, &rm.last_read);
                if lm.img_url.is_none() || (remote_newer && rm.img_url.is_some()) {
                    lm.img_url = rm.img_url.clone().or_else(|| lm.img_url.clone());
                }
                lm.in_reading_history = lm.in_reading_history || rm.in_reading_history;
                lm.in_plan_to_read = lm.in_plan_to_read || rm.in_plan_to_read;
                if lm.deleted_at.is_some() && rm.deleted_at.is_none() {
                    lm.deleted_at = None;
                }
            },
            None => {
                remote_is_newer_for_manga.insert(rm.id.clone(), true);
                mangas_by_id.insert(rm.id.clone(), rm.clone());
            },
        }
    }

    // Determine winning bookmark per manga_id so only 1 chapter per manga is bookmarked.
    let mut local_bookmarks: HashMap<String, (String, Option<u32>)> = HashMap::new();
    let mut remote_bookmarks: HashMap<String, (String, Option<u32>)> = HashMap::new();

    for ch in &local.chapters {
        if ch.is_bookmarked {
            local_bookmarks.insert(ch.manga_id.clone(), (ch.id.clone(), ch.number_page_bookmarked));
        }
    }
    for ch in &remote.chapters {
        if ch.is_bookmarked {
            remote_bookmarks.insert(ch.manga_id.clone(), (ch.id.clone(), ch.number_page_bookmarked));
        }
    }

    let mut winning_bookmark_by_manga: HashMap<String, (String, Option<u32>)> = HashMap::new();
    let all_bookmarked_mangas: BTreeSet<String> = local_bookmarks.keys().chain(remote_bookmarks.keys()).cloned().collect();

    for manga_id in all_bookmarked_mangas {
        let remote_newer = remote_is_newer_for_manga.get(&manga_id).copied().unwrap_or(false);
        let winner = match (local_bookmarks.get(&manga_id), remote_bookmarks.get(&manga_id)) {
            (Some(l_bm), Some(r_bm)) => {
                if l_bm.0 == r_bm.0 {
                    // Same chapter bookmarked on both devices: keep the furthest page reached
                    let max_page = match (l_bm.1, r_bm.1) {
                        (Some(lp), Some(rp)) => Some(lp.max(rp)),
                        (Some(lp), None) => Some(lp),
                        (None, Some(rp)) => Some(rp),
                        (None, None) => None,
                    };
                    (l_bm.0.clone(), max_page)
                } else if remote_newer {
                    r_bm.clone()
                } else {
                    l_bm.clone()
                }
            },
            (Some(l_bm), None) => l_bm.clone(),
            (None, Some(r_bm)) => r_bm.clone(),
            (None, None) => continue,
        };
        winning_bookmark_by_manga.insert(manga_id, winner);
    }

    let mut chapters_by_id: BTreeMap<String, SyncChapterRecord> = BTreeMap::new();
    for ch in &local.chapters {
        let mut cloned = ch.clone();
        cloned.is_bookmarked = false;
        cloned.number_page_bookmarked = None;
        chapters_by_id.insert(cloned.id.clone(), cloned);
    }

    for rch in &remote.chapters {
        match chapters_by_id.get_mut(&rch.id) {
            Some(lch) => {
                lch.is_read = lch.is_read || rch.is_read;
                lch.is_downloaded = lch.is_downloaded || rch.is_downloaded;
                if lch.translated_language.as_deref().unwrap_or("").is_empty() && rch.translated_language.is_some() {
                    lch.translated_language = rch.translated_language.clone();
                }
                if lch.title.trim().is_empty() && !rch.title.trim().is_empty() {
                    lch.title = rch.title.clone();
                }
            },
            None => {
                let mut cloned = rch.clone();
                cloned.is_bookmarked = false;
                cloned.number_page_bookmarked = None;
                chapters_by_id.insert(cloned.id.clone(), cloned);
            },
        }
    }

    for (manga_id, (win_chapter_id, win_page)) in winning_bookmark_by_manga {
        if let Some(ch) = chapters_by_id.get_mut(&win_chapter_id)
            && ch.manga_id == manga_id
        {
            ch.is_bookmarked = true;
            ch.number_page_bookmarked = win_page;
        }
    }

    let mut favorites_map: BTreeMap<(String, String), SyncFavoriteRecord> = BTreeMap::new();
    for fav in local.favorites.iter().chain(remote.favorites.iter()) {
        let key = (fav.manga_id.clone(), fav.provider.clone());
        favorites_map
            .entry(key)
            .and_modify(|existing| {
                existing.created_at = pick_older_timestamp(&existing.created_at, &fav.created_at);
                if existing.title.trim().is_empty() && !fav.title.trim().is_empty() {
                    existing.title = fav.title.clone();
                }
            })
            .or_insert_with(|| fav.clone());
    }

    let mut dismissed_set: BTreeSet<(String, String, String)> = BTreeSet::new();
    for d in local.dismissed_notifications.iter().chain(remote.dismissed_notifications.iter()) {
        dismissed_set.insert((d.manga_id.clone(), d.provider.clone(), d.chapter_id.clone()));
    }
    let dismissed_notifications = dismissed_set
        .into_iter()
        .map(|(manga_id, provider, chapter_id)| SyncDismissedRecord {
            manga_id,
            provider,
            chapter_id,
        })
        .collect();

    let mut search_map: BTreeMap<String, SyncSearchRecord> = BTreeMap::new();
    for s in local.search_history.iter().chain(remote.search_history.iter()) {
        let key = s.query.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        search_map
            .entry(key)
            .and_modify(|existing| {
                if s.searched_at > existing.searched_at {
                    existing.searched_at = s.searched_at;
                    existing.query = s.query.clone();
                }
            })
            .or_insert_with(|| s.clone());
    }
    let mut search_history: Vec<SyncSearchRecord> = search_map.into_values().collect();
    search_history.sort_by(|a, b| b.searched_at.cmp(&a.searched_at));

    LibrarySyncSnapshot {
        schema_version: LIBRARY_SYNC_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        synced_at: Utc::now().to_rfc3339(),
        mangas: mangas_by_id.into_values().collect(),
        chapters: chapters_by_id.into_values().collect(),
        favorites: favorites_map.into_values().collect(),
        dismissed_notifications,
        search_history,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudBackendKind {
    GoogleDriveApi,
    LocalCloudFolder,
}

impl CloudBackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::GoogleDriveApi => "Google Drive API (OAuth 2.0)",
            Self::LocalCloudFolder => {
                tr("Unidad en la nube local (Google Drive / iCloud Drive)", "Local Cloud Drive (Google Drive / iCloud Drive)")
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloudSyncMode {
    LibraryOnly,
    FullSync,
    PushDownloadsOnly,
    PullDownloadsOnly,
}

impl CloudSyncMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::LibraryOnly => tr("Progreso y Biblioteca", "Progress & Library"),
            Self::FullSync => tr("Sincronización Completa (Biblioteca + Descargas)", "Full Sync (Library + Downloads)"),
            Self::PushDownloadsOnly => tr("Subir Capítulos Descargados", "Push Downloaded Chapters"),
            Self::PullDownloadsOnly => tr("Bajar Capítulos de la Nube", "Pull Cloud Chapters"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudSyncReport {
    pub synced_at: String,
    pub total_mangas: usize,
    pub total_read_chapters: usize,
    pub total_favorites: usize,
    pub chapters_uploaded: usize,
    pub chapters_downloaded: usize,
    pub bytes_transferred: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudSyncProgress {
    pub step_label: String,
    pub completed_items: usize,
    pub total_items: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudConnectionState {
    pub backend: Option<CloudBackendKind>,
    pub client_id: Option<String>,
    pub account_email: Option<String>,
    pub account_display_name: Option<String>,
    pub local_folder_path: Option<PathBuf>,
    pub local_folder_provider_label: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_report: Option<CloudSyncReport>,
    // Fallback encrypted/local tokens if OS keyring is unavailable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_client_secret: Option<String>,
}

impl CloudConnectionState {
    pub fn state_file_path() -> PathBuf {
        if let Some(dir) = crate::backend::APP_DATA_DIR.as_ref() {
            dir.join(CLOUD_STATE_FILE_NAME)
        } else {
            PathBuf::from(CLOUD_STATE_FILE_NAME)
        }
    }

    pub fn load() -> Self {
        let path = Self::state_file_path();
        Self::load_from_path(&path)
    }

    pub fn load_from_path(path: &Path) -> Self {
        let mut state: Self = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        if state.client_id.is_none()
            && let Ok(env_id) = std::env::var("MANGA_TUI_GDRIVE_CLIENT_ID")
            && !env_id.trim().is_empty()
        {
            state.client_id = Some(env_id.trim().to_string());
        }
        state
    }

    pub fn save(&self) -> Result<(), String> {
        if cfg!(test) {
            return Ok(());
        }
        let path = Self::state_file_path();
        self.save_to_path(&path)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn is_connected(&self) -> bool {
        match self.backend {
            Some(CloudBackendKind::GoogleDriveApi) => self.get_refresh_token().is_some() || self.get_access_token().is_some(),
            Some(CloudBackendKind::LocalCloudFolder) => self.local_folder_path.as_ref().is_some_and(|p| p.exists()),
            None => false,
        }
    }

    pub fn connection_summary(&self) -> String {
        match self.backend {
            Some(CloudBackendKind::GoogleDriveApi) => {
                if let Some(email) = &self.account_email {
                    format!("Google Drive API · {email}")
                } else if let Some(name) = &self.account_display_name {
                    format!("Google Drive API · {name}")
                } else {
                    "Google Drive API (OAuth 2.0)".to_string()
                }
            },
            Some(CloudBackendKind::LocalCloudFolder) => {
                let provider = self
                    .local_folder_provider_label
                    .as_deref()
                    .unwrap_or(tr("Carpeta en la nube", "Cloud Folder"));
                let path_str = self.local_folder_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                format!("{provider} · {path_str}")
            },
            None => tr("Sin vincular", "Not linked").to_string(),
        }
    }

    pub fn save_oauth_tokens(
        &mut self,
        client_id: &str,
        client_secret: Option<&str>,
        access_token: &str,
        refresh_token: Option<&str>,
    ) {
        self.client_id = Some(client_id.trim().to_string());
        self.fallback_access_token = Some(access_token.trim().to_string());
        if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
            self.fallback_client_secret = Some(secret.to_string());
        }
        if let Some(refresh) = refresh_token.map(str::trim).filter(|s| !s.is_empty()) {
            self.fallback_refresh_token = Some(refresh.to_string());
        }

        if !cfg!(test) {
            let storage = KeyringStorage::new();
            let _ = storage.save_secret(SECRET_GDRIVE_CLIENT_ID, client_id.trim());
            let _ = storage.save_secret(SECRET_GDRIVE_ACCESS_TOKEN, access_token.trim());
            if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
                let _ = storage.save_secret(SECRET_GDRIVE_CLIENT_SECRET, secret);
            }
            if let Some(refresh) = refresh_token.map(str::trim).filter(|s| !s.is_empty()) {
                let _ = storage.save_secret(SECRET_GDRIVE_REFRESH_TOKEN, refresh);
            }
        }
    }

    pub fn get_client_id(&self) -> Option<String> {
        if let Ok(env_id) = std::env::var("MANGA_TUI_GDRIVE_CLIENT_ID")
            && !env_id.trim().is_empty()
        {
            return Some(env_id.trim().to_string());
        }
        if let Some(id) = self.client_id.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(id.clone());
        }
        if cfg!(test) {
            return None;
        }
        KeyringStorage::new()
            .get_secret(SECRET_GDRIVE_CLIENT_ID)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn get_client_secret(&self) -> Option<String> {
        if let Ok(env_sec) = std::env::var("MANGA_TUI_GDRIVE_CLIENT_SECRET")
            && !env_sec.trim().is_empty()
        {
            return Some(env_sec.trim().to_string());
        }
        if let Some(sec) = self.fallback_client_secret.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(sec.clone());
        }
        if cfg!(test) {
            return None;
        }
        KeyringStorage::new()
            .get_secret(SECRET_GDRIVE_CLIENT_SECRET)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn get_access_token(&self) -> Option<String> {
        if let Some(tok) = self.fallback_access_token.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(tok.clone());
        }
        if cfg!(test) {
            return None;
        }
        KeyringStorage::new()
            .get_secret(SECRET_GDRIVE_ACCESS_TOKEN)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn get_refresh_token(&self) -> Option<String> {
        if let Some(tok) = self.fallback_refresh_token.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(tok.clone());
        }
        if cfg!(test) {
            return None;
        }
        KeyringStorage::new()
            .get_secret(SECRET_GDRIVE_REFRESH_TOKEN)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn clear_credentials(&mut self) {
        if !cfg!(test) {
            let storage = KeyringStorage::new();
            let _ = storage.remove_secret(SECRET_GDRIVE_ACCESS_TOKEN);
            let _ = storage.remove_secret(SECRET_GDRIVE_REFRESH_TOKEN);
        }
        self.backend = None;
        self.account_email = None;
        self.account_display_name = None;
        self.local_folder_path = None;
        self.local_folder_provider_label = None;
        self.fallback_access_token = None;
        self.fallback_refresh_token = None;
    }
}

/// PKCE (Proof Key for Code Exchange, RFC 7636) verifier and challenge for OAuth 2.0.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

impl PkceChallenge {
    pub fn generate() -> Self {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let mut rng = rand::rng();
        let code_verifier: String = (0..64)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        let state: String = (0..24)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        let code_challenge = Self::compute_s256_challenge(&code_verifier);
        Self {
            code_verifier,
            code_challenge,
            state,
        }
    }

    pub fn compute_s256_challenge(verifier: &str) -> String {
        let digest = Sha256::digest(verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }
}

fn url_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_decode_component(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'+' {
            bytes.push(b' ');
            i += 1;
        } else if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex_str) = std::str::from_utf8(&raw[i + 1..i + 3])
                && let Ok(val) = u8::from_str_radix(hex_str, 16)
            {
                bytes.push(val);
                i += 3;
                continue;
            }
            bytes.push(raw[i]);
            i += 1;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn build_google_oauth_authorize_url(auth_endpoint: &str, client_id: &str, redirect_uri: &str, pkce: &PkceChallenge) -> String {
    format!(
        "{auth_endpoint}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent&state={}",
        url_encode_component(client_id),
        url_encode_component(redirect_uri),
        url_encode_component(GOOGLE_DRIVE_SCOPE),
        url_encode_component(&pkce.code_challenge),
        url_encode_component(&pkce.state),
    )
}

fn extract_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == key
        {
            return Some(url_decode_component(v));
        }
    }
    None
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriveAboutUser {
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DriveAboutResponse {
    pub user: Option<DriveAboutUser>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DriveFileEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType", default)]
    pub mime_type: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(rename = "modifiedTime", default)]
    pub modified_time: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveFileListResponse {
    #[serde(default)]
    files: Vec<DriveFileEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveCreateIdResponse {
    id: String,
}

/// Google Drive REST API v3 client supporting OAuth 2.0 PKCE, automatic token refresh,
/// folder hierarchy management, and file upload/download.
#[derive(Debug, Clone)]
pub struct GoogleDriveClient {
    http: Client,
    auth_url: String,
    token_url: String,
    api_base_url: String,
    upload_base_url: String,
}

impl Default for GoogleDriveClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleDriveClient {
    pub fn new() -> Self {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(format!("manga-tui/{}", env!("CARGO_PKG_VERSION")));
        if cfg!(test) {
            builder = builder.no_proxy();
        }
        let http = builder.build().unwrap_or_else(|_| Client::new());

        Self {
            http,
            auth_url: GOOGLE_OAUTH_AUTH_URL.to_string(),
            token_url: GOOGLE_OAUTH_TOKEN_URL.to_string(),
            api_base_url: GOOGLE_DRIVE_API_BASE_URL.to_string(),
            upload_base_url: GOOGLE_DRIVE_UPLOAD_BASE_URL.to_string(),
        }
    }

    pub fn with_endpoints(
        mut self,
        auth_url: impl Into<String>,
        token_url: impl Into<String>,
        api_base_url: impl Into<String>,
        upload_base_url: impl Into<String>,
    ) -> Self {
        self.auth_url = auth_url.into();
        self.token_url = token_url.into();
        self.api_base_url = api_base_url.into();
        self.upload_base_url = upload_base_url.into();
        self
    }

    /// Runs the loopback OAuth 2.0 + PKCE authorization flow on `127.0.0.1:0`, invoking
    /// `on_authorize_url` with the browser URL to open, waiting for the redirect callback,
    /// and exchanging the authorization code for access and refresh tokens.
    pub async fn run_loopback_oauth_flow<F>(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        mut on_authorize_url: F,
    ) -> Result<OAuthTokenResponse, String>
    where
        F: FnMut(&str),
    {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(
                tr("El Client ID de Google Drive no puede estar vacío.", "Google Drive Client ID cannot be empty.").to_string()
            );
        }

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind local OAuth callback port: {e}"))?;
        let local_addr = listener.local_addr().map_err(|e| e.to_string())?;
        let redirect_uri = format!("http://127.0.0.1:{}", local_addr.port());
        let pkce = PkceChallenge::generate();
        let authorize_url = build_google_oauth_authorize_url(&self.auth_url, client_id, &redirect_uri, &pkce);

        on_authorize_url(&authorize_url);

        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(180), listener.accept())
            .await
            .map_err(|_| {
                tr(
                    "Tiempo de espera agotado (3 min) esperando la autorización de Google en el navegador.",
                    "Timed out (3 min) waiting for Google authorization in the browser.",
                )
                .to_string()
            })?
            .map_err(|e| format!("OAuth socket error: {e}"))?;

        let mut buf = vec![0u8; 4096];
        let n = socket.read(&mut buf).await.map_err(|e| e.to_string())?;
        let request_str = String::from_utf8_lossy(&buf[..n]);
        let request_line = request_str.lines().next().unwrap_or("");
        let path_and_query = request_line.split_whitespace().nth(1).unwrap_or("");
        let query = path_and_query.split_once('?').map(|(_, q)| q).unwrap_or("");

        if let Some(err) = extract_query_param(query, "error") {
            let html = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                 <html><body style='font-family:sans-serif;background:#1a1b26;color:#f7768e;text-align:center;padding:40px;'>\
                 <h2>❌ Error de autorización en Google Drive</h2><p>{err}</p></body></html>"
            );
            let _ = socket.write_all(html.as_bytes()).await;
            return Err(format!("Google OAuth error: {err}"));
        }

        let Some(code) = extract_query_param(query, "code") else {
            let html = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                        <html><body>Missing authorization code</body></html>";
            let _ = socket.write_all(html.as_bytes()).await;
            return Err(tr("No se recibió código de autorización OAuth.", "No OAuth authorization code received.").to_string());
        };

        let success_html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
            <html><head><title>Manga TUI - Google Drive</title></head>\
            <body style='font-family:system-ui,sans-serif;background:#1a1b26;color:#c0caf5;text-align:center;padding:60px;'>\
            <div style='max-width:480px;margin:0 auto;background:#24283b;padding:32px;border-radius:12px;border:1px solid #7aa2f7;'>\
            <h2 style='color:#9ece6a;margin-top:0;'>✓ Google Drive vinculado con Manga TUI</h2>\
            <p>La autorización se ha completado correctamente. Ya puedes cerrar esta pestaña y volver a <b>Manga TUI</b>.</p>\
            </div></body></html>";
        let _ = socket.write_all(success_html.as_bytes()).await;
        let _ = socket.shutdown().await;

        self.exchange_code_for_token(client_id, client_secret, &code, &pkce.code_verifier, &redirect_uri)
            .await
    }

    pub async fn exchange_code_for_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthTokenResponse, String> {
        let mut params = vec![
            ("client_id", client_id.to_string()),
            ("code", code.to_string()),
            ("code_verifier", code_verifier.to_string()),
            ("grant_type", "authorization_code".to_string()),
            ("redirect_uri", redirect_uri.to_string()),
        ];
        if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(("client_secret", secret.to_string()));
        }

        let response = self
            .http
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("OAuth token request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("OAuth token exchange failed ({status}): {body}"));
        }

        response
            .json::<OAuthTokenResponse>()
            .await
            .map_err(|e| format!("Invalid OAuth token response: {e}"))
    }

    pub async fn refresh_access_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<OAuthTokenResponse, String> {
        let mut params = vec![
            ("client_id", client_id.to_string()),
            ("refresh_token", refresh_token.to_string()),
            ("grant_type", "refresh_token".to_string()),
        ];
        if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(("client_secret", secret.to_string()));
        }

        let response = self
            .http
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token refresh request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Token refresh failed ({status}): {body}"));
        }

        response
            .json::<OAuthTokenResponse>()
            .await
            .map_err(|e| format!("Invalid refresh token response: {e}"))
    }

    pub async fn get_about_user(&self, access_token: &str) -> Result<DriveAboutResponse, String> {
        let url = format!("{}/about?fields=user(displayName,emailAddress)", self.api_base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Drive about request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Drive about error ({status}): {body}"));
        }

        resp.json::<DriveAboutResponse>()
            .await
            .map_err(|e| format!("Failed parsing Drive about response: {e}"))
    }

    pub async fn find_or_create_folder(
        &self,
        access_token: &str,
        folder_name: &str,
        parent_id: Option<&str>,
    ) -> Result<String, String> {
        let escaped_name = folder_name.replace('\\', "\\\\").replace('\'', "\\'");
        let query = match parent_id {
            Some(pid) => format!(
                "name='{escaped_name}' and mimeType='application/vnd.google-apps.folder' and '{pid}' in parents and trashed=false"
            ),
            None => format!("name='{escaped_name}' and mimeType='application/vnd.google-apps.folder' and trashed=false"),
        };
        let list_url =
            format!("{}/files?q={}&fields=files(id,name,mimeType)&pageSize=10", self.api_base_url, url_encode_component(&query));

        let resp = self
            .http
            .get(&list_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Drive folder lookup failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Drive folder lookup error ({status}): {body}"));
        }

        let list: DriveFileListResponse = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(existing) = list.files.into_iter().next() {
            return Ok(existing.id);
        }

        let create_url = format!("{}/files?fields=id", self.api_base_url);
        let body = match parent_id {
            Some(pid) => serde_json::json!({
                "name": folder_name,
                "mimeType": "application/vnd.google-apps.folder",
                "parents": [pid],
            }),
            None => serde_json::json!({
                "name": folder_name,
                "mimeType": "application/vnd.google-apps.folder",
            }),
        };

        let create_resp = self
            .http
            .post(&create_url)
            .bearer_auth(access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Drive folder creation failed: {e}"))?;

        if !create_resp.status().is_success() {
            let status = create_resp.status();
            let err_body = create_resp.text().await.unwrap_or_default();
            return Err(format!("Drive folder creation error ({status}): {err_body}"));
        }

        let created: DriveCreateIdResponse = create_resp.json().await.map_err(|e| e.to_string())?;
        Ok(created.id)
    }

    pub async fn list_files_in_folder(&self, access_token: &str, folder_id: &str) -> Result<Vec<DriveFileEntry>, String> {
        let query = format!("'{folder_id}' in parents and trashed=false");
        let url = format!(
            "{}/files?q={}&fields=files(id,name,mimeType,size,modifiedTime)&pageSize=1000",
            self.api_base_url,
            url_encode_component(&query)
        );

        let resp = self
            .http
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Drive file list failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Drive file list error ({status}): {body}"));
        }

        let list: DriveFileListResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(list.files)
    }

    pub async fn download_file_bytes(&self, access_token: &str, file_id: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/files/{file_id}?alt=media", self.api_base_url);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| format!("Drive file download failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Drive file download error ({status}): {body}"));
        }

        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        Ok(bytes.to_vec())
    }

    pub async fn upload_or_update_file(
        &self,
        access_token: &str,
        parent_id: &str,
        file_name: &str,
        existing_file_id: Option<&str>,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<String, String> {
        let file_id = match existing_file_id {
            Some(id) => id.to_string(),
            None => {
                let create_url = format!("{}/files?fields=id", self.api_base_url);
                let meta = serde_json::json!({
                    "name": file_name,
                    "parents": [parent_id],
                });
                let create_resp = self
                    .http
                    .post(&create_url)
                    .bearer_auth(access_token)
                    .json(&meta)
                    .send()
                    .await
                    .map_err(|e| format!("Drive metadata creation failed: {e}"))?;

                if !create_resp.status().is_success() {
                    let status = create_resp.status();
                    let body = create_resp.text().await.unwrap_or_default();
                    return Err(format!("Drive metadata creation error ({status}): {body}"));
                }
                let created: DriveCreateIdResponse = create_resp.json().await.map_err(|e| e.to_string())?;
                created.id
            },
        };

        let upload_url = format!("{}/files/{file_id}?uploadType=media", self.upload_base_url);
        let upload_resp = self
            .http
            .patch(&upload_url)
            .bearer_auth(access_token)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("Drive media upload failed: {e}"))?;

        if !upload_resp.status().is_success() {
            let status = upload_resp.status();
            let body = upload_resp.text().await.unwrap_or_default();
            return Err(format!("Drive media upload error ({status}): {body}"));
        }

        Ok(file_id)
    }

    /// Resolves a valid Google Drive `access_token`, refreshing via `refresh_token` if needed.
    pub async fn resolve_valid_access_token(&self, state: &mut CloudConnectionState) -> Result<String, String> {
        if let Some(token) = state.get_access_token() {
            if self.get_about_user(&token).await.is_ok() {
                return Ok(token);
            }
        }

        let client_id = state.get_client_id().ok_or_else(|| {
            tr(
                "Falta el Client ID de Google Drive. Vincula tu cuenta primero.",
                "Missing Google Drive Client ID. Please link your account first.",
            )
            .to_string()
        })?;
        let refresh_token = state.get_refresh_token().ok_or_else(|| {
            tr(
                "La sesión de Google Drive ha expirado. Vuelve a iniciar sesión.",
                "Google Drive session expired. Please sign in again.",
            )
            .to_string()
        })?;
        let client_secret = state.get_client_secret();

        let refreshed = self.refresh_access_token(&client_id, client_secret.as_deref(), &refresh_token).await?;

        state.save_oauth_tokens(
            &client_id,
            client_secret.as_deref(),
            &refreshed.access_token,
            refreshed.refresh_token.as_deref().or(Some(&refresh_token)),
        );
        let _ = state.save();
        Ok(refreshed.access_token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedCloudFolder {
    pub provider_label: String,
    pub root_path: PathBuf,
    pub manga_tui_folder: PathBuf,
}

/// Auto-detects installed desktop cloud drives (Google Drive Desktop, iCloud Drive, OneDrive).
pub fn detect_installed_cloud_folders() -> Vec<DetectedCloudFolder> {
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();

    if let Some(user_dirs) = directories::UserDirs::new() {
        let home = user_dirs.home_dir();
        // iCloud Drive on Windows & macOS
        candidates.push(("iCloud Drive".to_string(), home.join("iCloudDrive")));
        candidates.push(("iCloud Drive".to_string(), home.join("Library").join("Mobile Documents").join("com~apple~CloudDocs")));

        // Google Drive in home or CloudStorage
        candidates.push(("Google Drive Desktop".to_string(), home.join("Google Drive")));
        candidates.push(("Google Drive Desktop".to_string(), home.join("My Drive")));
        candidates.push(("Google Drive Desktop".to_string(), home.join("Mi unidad")));

        let cloud_storage = home.join("Library").join("CloudStorage");
        if let Ok(entries) = fs::read_dir(&cloud_storage) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("GoogleDrive") {
                    let my_drive = entry.path().join("My Drive");
                    let mi_unidad = entry.path().join("Mi unidad");
                    if my_drive.is_dir() {
                        candidates.push(("Google Drive Desktop".to_string(), my_drive));
                    } else if mi_unidad.is_dir() {
                        candidates.push(("Google Drive Desktop".to_string(), mi_unidad));
                    } else {
                        candidates.push(("Google Drive Desktop".to_string(), entry.path()));
                    }
                }
            }
        }
    }

    // Windows virtual drive letters for Google Drive for Desktop (typically G:\Mi unidad or G:\My Drive)
    #[cfg(target_os = "windows")]
    {
        for letter in ['G', 'H', 'I', 'D', 'E', 'F'] {
            for sub in ["Mi unidad", "My Drive"] {
                let p = PathBuf::from(format!("{letter}:\\{sub}"));
                if p.is_dir() {
                    candidates.push(("Google Drive Desktop".to_string(), p));
                }
            }
        }
    }

    // OneDrive via environment variable or home folder
    if let Ok(onedrive) = std::env::var("OneDrive").or_else(|_| std::env::var("ONEDRIVE")) {
        let p = PathBuf::from(onedrive);
        if p.is_dir() {
            candidates.push(("OneDrive".to_string(), p));
        }
    }
    if let Some(user_dirs) = directories::UserDirs::new() {
        candidates.push(("OneDrive".to_string(), user_dirs.home_dir().join("OneDrive")));
    }

    let mut seen = BTreeSet::new();
    let mut detected = Vec::new();
    for (label, root) in candidates {
        if root.is_dir() && seen.insert(root.clone()) {
            let manga_tui_folder = root.join(CLOUD_ROOT_FOLDER_NAME);
            detected.push(DetectedCloudFolder {
                provider_label: label,
                root_path: root,
                manga_tui_folder,
            });
        }
    }
    detected
}

#[derive(Debug, Clone)]
struct LocalDownloadChapterFile {
    manga_folder: String,
    file_name: String,
    full_path: PathBuf,
    size_bytes: u64,
}

fn is_syncable_download_archive(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".cbz") || lower.ends_with(".epub") || lower.ends_with(".pdf")
}

fn scan_local_download_archives(downloads_dir: &Path) -> Vec<LocalDownloadChapterFile> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(downloads_dir) else {
        return files;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let manga_folder = entry.file_name().to_string_lossy().to_string();
            if let Ok(sub_entries) = fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    let sub_path = sub.path();
                    if sub_path.is_file() {
                        let file_name = sub.file_name().to_string_lossy().to_string();
                        if is_syncable_download_archive(&file_name) {
                            let size_bytes = sub.metadata().map(|m| m.len()).unwrap_or(0);
                            files.push(LocalDownloadChapterFile {
                                manga_folder: manga_folder.clone(),
                                file_name,
                                full_path: sub_path,
                                size_bytes,
                            });
                        }
                    }
                }
            }
        }
    }
    files.sort_by(|a, b| (&a.manga_folder, &a.file_name).cmp(&(&b.manga_folder, &b.file_name)));
    files
}

fn register_pulled_archive_in_database(db: &Database<'_>, manga_folder: &str, file_name: &str) {
    let stem = file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(file_name).trim();
    if stem.is_empty() {
        return;
    }
    let _ = db.set_chapter_downloaded(crate::backend::database::SetChapterDownloaded {
        id: stem,
        title: stem,
        manga_id: manga_folder,
        manga_title: manga_folder,
        img_url: None,
        provider: MangaProviders::default(),
    });
}

/// Synchronizes reading progress, library metadata, and downloaded chapters using the active cloud backend.
pub async fn run_cloud_sync<F>(
    state: &mut CloudConnectionState,
    mode: CloudSyncMode,
    drive_client: &GoogleDriveClient,
    mut on_progress: F,
) -> Result<CloudSyncReport, String>
where
    F: FnMut(CloudSyncProgress),
{
    let downloads_dir = AppDirectories::MangaDownloads.get_full_path();
    let db_path = AppDirectories::History.get_full_path();
    run_cloud_sync_with_paths(state, mode, drive_client, &db_path, &downloads_dir, &mut on_progress).await
}

pub async fn run_cloud_sync_with_paths<F>(
    state: &mut CloudConnectionState,
    mode: CloudSyncMode,
    drive_client: &GoogleDriveClient,
    db_path: &Path,
    downloads_dir: &Path,
    on_progress: &mut F,
) -> Result<CloudSyncReport, String>
where
    F: FnMut(CloudSyncProgress),
{
    let backend = state.backend.ok_or_else(|| {
        tr(
            "No hay ninguna nube vinculada. Vincula Google Drive o una carpeta en la nube primero.",
            "No cloud account linked. Please link Google Drive or a cloud folder first.",
        )
        .to_string()
    })?;

    match backend {
        CloudBackendKind::GoogleDriveApi => {
            sync_with_google_drive_api(state, mode, drive_client, db_path, downloads_dir, on_progress).await
        },
        CloudBackendKind::LocalCloudFolder => {
            let cloud_folder = state.local_folder_path.clone().ok_or_else(|| {
                tr("No se ha configurado la ruta de la carpeta en la nube.", "Cloud folder path is not configured.").to_string()
            })?;
            let report = sync_with_local_cloud_folder(&cloud_folder, mode, db_path, downloads_dir, on_progress)?;
            state.last_sync_at = Some(report.synced_at.clone());
            state.last_sync_report = Some(report.clone());
            let _ = state.save();
            Ok(report)
        },
    }
}

async fn sync_with_google_drive_api<F>(
    state: &mut CloudConnectionState,
    mode: CloudSyncMode,
    drive_client: &GoogleDriveClient,
    db_path: &Path,
    downloads_dir: &Path,
    on_progress: &mut F,
) -> Result<CloudSyncReport, String>
where
    F: FnMut(CloudSyncProgress),
{
    on_progress(CloudSyncProgress {
        step_label: tr("Verificando sesión de Google Drive...", "Verifying Google Drive session...").to_string(),
        completed_items: 0,
        total_items: 4,
    });

    let access_token = drive_client.resolve_valid_access_token(state).await?;
    let root_folder_id = drive_client.find_or_create_folder(&access_token, CLOUD_ROOT_FOLDER_NAME, None).await?;
    let root_files = drive_client.list_files_in_folder(&access_token, &root_folder_id).await?;

    let mut total_mangas = 0usize;
    let mut total_read_chapters = 0usize;
    let mut total_favorites = 0usize;
    let mut chapters_uploaded = 0usize;
    let mut chapters_downloaded = 0usize;
    let mut bytes_transferred = 0u64;

    if matches!(mode, CloudSyncMode::LibraryOnly | CloudSyncMode::FullSync) {
        on_progress(CloudSyncProgress {
            step_label: tr(
                "Sincronizando y fusionando progreso de lectura (library_sync.json)...",
                "Syncing and merging reading progress (library_sync.json)...",
            )
            .to_string(),
            completed_items: 1,
            total_items: 4,
        });

        let remote_snapshot_entry = root_files.iter().find(|f| f.name == CLOUD_LIBRARY_JSON_NAME);
        let remote_snapshot: Option<LibrarySyncSnapshot> = if let Some(entry) = remote_snapshot_entry {
            let bytes = drive_client.download_file_bytes(&access_token, &entry.id).await?;
            bytes_transferred += bytes.len() as u64;
            serde_json::from_slice(&bytes).ok()
        } else {
            None
        };

        let merged = {
            let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
            let db = Database::new(&conn);
            db.setup().map_err(|e| e.to_string())?;

            match remote_snapshot {
                Some(ref remote) => db.import_and_merge_library_sync_snapshot(remote).map_err(|e| e.to_string())?,
                None => db.export_library_sync_snapshot().map_err(|e| e.to_string())?,
            }
        };

        total_mangas = merged.mangas.len();
        total_read_chapters = merged.chapters.iter().filter(|c| c.is_read).count();
        total_favorites = merged.favorites.len();

        let merged_json = serde_json::to_vec_pretty(&merged).map_err(|e| e.to_string())?;
        bytes_transferred += merged_json.len() as u64;

        drive_client
            .upload_or_update_file(
                &access_token,
                &root_folder_id,
                CLOUD_LIBRARY_JSON_NAME,
                remote_snapshot_entry.map(|e| e.id.as_str()),
                "application/json",
                merged_json,
            )
            .await?;

        // Also upload a clean SQLite backup file if db_path exists on disk
        if db_path.is_file()
            && let Ok(db_bytes) = fs::read(db_path)
        {
            let existing_db_entry = root_files.iter().find(|f| f.name == CLOUD_DB_BACKUP_NAME);
            bytes_transferred += db_bytes.len() as u64;
            let _ = drive_client
                .upload_or_update_file(
                    &access_token,
                    &root_folder_id,
                    CLOUD_DB_BACKUP_NAME,
                    existing_db_entry.map(|e| e.id.as_str()),
                    "application/x-sqlite3",
                    db_bytes,
                )
                .await;
        }
    }

    if matches!(mode, CloudSyncMode::FullSync | CloudSyncMode::PushDownloadsOnly | CloudSyncMode::PullDownloadsOnly) {
        let downloads_folder_id = drive_client
            .find_or_create_folder(&access_token, CLOUD_DOWNLOADS_FOLDER_NAME, Some(&root_folder_id))
            .await?;
        let remote_manga_folders = drive_client.list_files_in_folder(&access_token, &downloads_folder_id).await?;

        let mut remote_folder_map: HashMap<String, String> = remote_manga_folders
            .into_iter()
            .filter(|f| f.mime_type == "application/vnd.google-apps.folder")
            .map(|f| (f.name, f.id))
            .collect();

        // 1. Push local downloads not yet on Google Drive
        if matches!(mode, CloudSyncMode::FullSync | CloudSyncMode::PushDownloadsOnly) {
            let local_archives = scan_local_download_archives(downloads_dir);
            let total_local = local_archives.len();
            let mut remote_files_cache: HashMap<String, Vec<DriveFileEntry>> = HashMap::new();

            for (idx, item) in local_archives.into_iter().enumerate() {
                on_progress(CloudSyncProgress {
                    step_label: format!(
                        "{} [{}/{}]: {} / {}",
                        tr("Verificando subida", "Checking upload"),
                        idx + 1,
                        total_local.max(1),
                        item.manga_folder,
                        item.file_name
                    ),
                    completed_items: idx + 1,
                    total_items: total_local.max(1),
                });

                let manga_folder_id = match remote_folder_map.get(&item.manga_folder) {
                    Some(id) => id.clone(),
                    None => {
                        let created_id = drive_client
                            .find_or_create_folder(&access_token, &item.manga_folder, Some(&downloads_folder_id))
                            .await?;
                        remote_folder_map.insert(item.manga_folder.clone(), created_id.clone());
                        created_id
                    },
                };

                let existing_files = match remote_files_cache.get(&manga_folder_id) {
                    Some(files) => files,
                    None => {
                        let listed = drive_client.list_files_in_folder(&access_token, &manga_folder_id).await?;
                        remote_files_cache.insert(manga_folder_id.clone(), listed);
                        remote_files_cache.get(&manga_folder_id).unwrap()
                    },
                };

                let already_on_drive = existing_files.iter().any(|f| f.name == item.file_name);
                if !already_on_drive {
                    let bytes = fs::read(&item.full_path).map_err(|e| e.to_string())?;
                    bytes_transferred += bytes.len() as u64;
                    let uploaded_id = drive_client
                        .upload_or_update_file(
                            &access_token,
                            &manga_folder_id,
                            &item.file_name,
                            None,
                            "application/octet-stream",
                            bytes,
                        )
                        .await?;
                    chapters_uploaded += 1;
                    if let Some(cached) = remote_files_cache.get_mut(&manga_folder_id) {
                        cached.push(DriveFileEntry {
                            id: uploaded_id,
                            name: item.file_name,
                            mime_type: "application/octet-stream".to_string(),
                            size: Some(item.size_bytes.to_string()),
                            modified_time: None,
                        });
                    }
                }
            }
        }

        // 2. Pull remote downloads from Google Drive not yet on local disk
        if matches!(mode, CloudSyncMode::FullSync | CloudSyncMode::PullDownloadsOnly) {
            let folder_pairs: Vec<(String, String)> = remote_folder_map.into_iter().collect();
            let total_folders = folder_pairs.len();
            let mut pulled_items: Vec<(String, String)> = Vec::new();

            for (f_idx, (manga_folder_name, manga_folder_id)) in folder_pairs.into_iter().enumerate() {
                let remote_files = drive_client.list_files_in_folder(&access_token, &manga_folder_id).await?;
                for rfile in remote_files {
                    if !is_syncable_download_archive(&rfile.name) {
                        continue;
                    }
                    let local_target_dir = downloads_dir.join(&manga_folder_name);
                    let local_target_path = local_target_dir.join(&rfile.name);
                    if !local_target_path.exists() {
                        on_progress(CloudSyncProgress {
                            step_label: format!(
                                "{} [{}/{}]: {} / {}",
                                tr("Descargando de Drive", "Downloading from Drive"),
                                f_idx + 1,
                                total_folders.max(1),
                                manga_folder_name,
                                rfile.name
                            ),
                            completed_items: f_idx + 1,
                            total_items: total_folders.max(1),
                        });

                        fs::create_dir_all(&local_target_dir).map_err(|e| e.to_string())?;
                        let bytes = drive_client.download_file_bytes(&access_token, &rfile.id).await?;
                        bytes_transferred += bytes.len() as u64;
                        fs::write(&local_target_path, bytes).map_err(|e| e.to_string())?;
                        chapters_downloaded += 1;
                        pulled_items.push((manga_folder_name.clone(), rfile.name));
                    }
                }
            }

            if !pulled_items.is_empty()
                && let Ok(conn) = rusqlite::Connection::open(db_path)
            {
                let db = Database::new(&conn);
                let _ = db.setup();
                for (manga_folder_name, file_name) in pulled_items {
                    register_pulled_archive_in_database(&db, &manga_folder_name, &file_name);
                }
            }
        }
    }

    if total_mangas == 0
        && let Ok(conn) = rusqlite::Connection::open(db_path)
    {
        let db = Database::new(&conn);
        let _ = db.setup();
        if let Ok(snap) = db.export_library_sync_snapshot() {
            total_mangas = snap.mangas.len();
            total_read_chapters = snap.chapters.iter().filter(|c| c.is_read).count();
            total_favorites = snap.favorites.len();
        }
    }

    let report = CloudSyncReport {
        synced_at: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        total_mangas,
        total_read_chapters,
        total_favorites,
        chapters_uploaded,
        chapters_downloaded,
        bytes_transferred,
    };
    state.last_sync_at = Some(report.synced_at.clone());
    state.last_sync_report = Some(report.clone());
    let _ = state.save();
    Ok(report)
}

pub fn sync_with_local_cloud_folder<F>(
    cloud_manga_tui_dir: &Path,
    mode: CloudSyncMode,
    db_path: &Path,
    downloads_dir: &Path,
    on_progress: &mut F,
) -> Result<CloudSyncReport, String>
where
    F: FnMut(CloudSyncProgress),
{
    fs::create_dir_all(cloud_manga_tui_dir).map_err(|e| format!("Could not create cloud sync folder: {e}"))?;

    let mut total_mangas = 0usize;
    let mut total_read_chapters = 0usize;
    let mut total_favorites = 0usize;
    let mut chapters_uploaded = 0usize;
    let mut chapters_downloaded = 0usize;
    let mut bytes_transferred = 0u64;

    if matches!(mode, CloudSyncMode::LibraryOnly | CloudSyncMode::FullSync) {
        on_progress(CloudSyncProgress {
            step_label: tr(
                "Fusionando biblioteca y progreso con carpeta en la nube...",
                "Merging library and progress with cloud folder...",
            )
            .to_string(),
            completed_items: 1,
            total_items: 3,
        });

        let cloud_json_path = cloud_manga_tui_dir.join(CLOUD_LIBRARY_JSON_NAME);
        let remote_snapshot: Option<LibrarySyncSnapshot> = if cloud_json_path.is_file() {
            let raw = fs::read(&cloud_json_path).map_err(|e| e.to_string())?;
            bytes_transferred += raw.len() as u64;
            serde_json::from_slice(&raw).ok()
        } else {
            None
        };

        let merged = {
            let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
            let db = Database::new(&conn);
            db.setup().map_err(|e| e.to_string())?;

            match remote_snapshot {
                Some(ref remote) => db.import_and_merge_library_sync_snapshot(remote).map_err(|e| e.to_string())?,
                None => db.export_library_sync_snapshot().map_err(|e| e.to_string())?,
            }
        };

        total_mangas = merged.mangas.len();
        total_read_chapters = merged.chapters.iter().filter(|c| c.is_read).count();
        total_favorites = merged.favorites.len();

        let merged_bytes = serde_json::to_vec_pretty(&merged).map_err(|e| e.to_string())?;
        bytes_transferred += merged_bytes.len() as u64;
        fs::write(&cloud_json_path, merged_bytes).map_err(|e| e.to_string())?;

        if db_path.is_file() {
            let cloud_db_path = cloud_manga_tui_dir.join(CLOUD_DB_BACKUP_NAME);
            if let Ok(copied) = fs::copy(db_path, &cloud_db_path) {
                bytes_transferred += copied;
            }
        }
    }

    let cloud_downloads_dir = cloud_manga_tui_dir.join(CLOUD_DOWNLOADS_FOLDER_NAME);
    if matches!(mode, CloudSyncMode::FullSync | CloudSyncMode::PushDownloadsOnly) {
        fs::create_dir_all(&cloud_downloads_dir).map_err(|e| e.to_string())?;
        let local_archives = scan_local_download_archives(downloads_dir);
        let total = local_archives.len();
        for (idx, item) in local_archives.into_iter().enumerate() {
            let target_manga_dir = cloud_downloads_dir.join(&item.manga_folder);
            let target_file = target_manga_dir.join(&item.file_name);
            if !target_file.exists() {
                on_progress(CloudSyncProgress {
                    step_label: format!(
                        "{} [{}/{}]: {} / {}",
                        tr("Copiando a la nube", "Copying to cloud"),
                        idx + 1,
                        total.max(1),
                        item.manga_folder,
                        item.file_name
                    ),
                    completed_items: idx + 1,
                    total_items: total.max(1),
                });
                fs::create_dir_all(&target_manga_dir).map_err(|e| e.to_string())?;
                let copied = fs::copy(&item.full_path, &target_file).map_err(|e| e.to_string())?;
                bytes_transferred += copied;
                chapters_uploaded += 1;
            }
        }
    }

    if matches!(mode, CloudSyncMode::FullSync | CloudSyncMode::PullDownloadsOnly) && cloud_downloads_dir.is_dir() {
        let conn = rusqlite::Connection::open(db_path).ok();
        let db = conn.as_ref().map(Database::new);
        if let Some(ref d) = db {
            let _ = d.setup();
        }

        let cloud_archives = scan_local_download_archives(&cloud_downloads_dir);
        let total = cloud_archives.len();
        for (idx, item) in cloud_archives.into_iter().enumerate() {
            let local_manga_dir = downloads_dir.join(&item.manga_folder);
            let local_file = local_manga_dir.join(&item.file_name);
            if !local_file.exists() {
                on_progress(CloudSyncProgress {
                    step_label: format!(
                        "{} [{}/{}]: {} / {}",
                        tr("Importando de la nube", "Importing from cloud"),
                        idx + 1,
                        total.max(1),
                        item.manga_folder,
                        item.file_name
                    ),
                    completed_items: idx + 1,
                    total_items: total.max(1),
                });
                fs::create_dir_all(&local_manga_dir).map_err(|e| e.to_string())?;
                let copied = fs::copy(&item.full_path, &local_file).map_err(|e| e.to_string())?;
                bytes_transferred += copied;
                chapters_downloaded += 1;
                if let Some(ref d) = db {
                    register_pulled_archive_in_database(d, &item.manga_folder, &item.file_name);
                }
            }
        }
    }

    if total_mangas == 0
        && let Ok(conn) = rusqlite::Connection::open(db_path)
    {
        let db = Database::new(&conn);
        let _ = db.setup();
        if let Ok(snap) = db.export_library_sync_snapshot() {
            total_mangas = snap.mangas.len();
            total_read_chapters = snap.chapters.iter().filter(|c| c.is_read).count();
            total_favorites = snap.favorites.len();
        }
    }

    Ok(CloudSyncReport {
        synced_at: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        total_mangas,
        total_read_chapters,
        total_favorites,
        chapters_uploaded,
        chapters_downloaded,
        bytes_transferred,
    })
}

#[cfg(test)]
mod tests {
    use httpmock::Method::{GET, PATCH, POST};
    use httpmock::MockServer;

    use super::*;

    #[test]
    fn pkce_s256_challenge_matches_rfc7636_test_vector() {
        // RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected_challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(PkceChallenge::compute_s256_challenge(verifier), expected_challenge);
    }

    #[test]
    fn merge_library_snapshots_combines_progress_bookmarks_and_favorites_losslessly() {
        let local = LibrarySyncSnapshot {
            schema_version: 1,
            app_version: "0.2.0".to_string(),
            synced_at: "2026-11-21T10:00:00Z".to_string(),
            mangas: vec![SyncMangaRecord {
                id: "m1".to_string(),
                title: "One Piece".to_string(),
                created_at: Some("2026-11-01 10:00:00".to_string()),
                updated_at: Some("2026-11-20 10:00:00".to_string()),
                last_read: Some("2026-11-20 10:00:00".to_string()),
                deleted_at: None,
                img_url: None,
                manga_provider: "mangadex".to_string(),
                in_reading_history: true,
                in_plan_to_read: false,
            }],
            chapters: vec![
                SyncChapterRecord {
                    id: "c1".to_string(),
                    title: "Cap 1".to_string(),
                    manga_id: "m1".to_string(),
                    is_read: true,
                    is_downloaded: true,
                    is_bookmarked: true,
                    translated_language: Some("es".to_string()),
                    number_page_bookmarked: Some(5),
                },
                SyncChapterRecord {
                    id: "c2".to_string(),
                    title: "Cap 2".to_string(),
                    manga_id: "m1".to_string(),
                    is_read: false,
                    is_downloaded: false,
                    is_bookmarked: false,
                    translated_language: Some("es".to_string()),
                    number_page_bookmarked: None,
                },
            ],
            favorites: vec![SyncFavoriteRecord {
                manga_id: "m1".to_string(),
                provider: "mangadex".to_string(),
                title: "One Piece".to_string(),
                created_at: Some("2026-11-01 10:00:00".to_string()),
            }],
            dismissed_notifications: vec![],
            search_history: vec![SyncSearchRecord {
                query: "one piece".to_string(),
                searched_at: 100,
            }],
        };

        // Remote has newer reading timestamp on m1, read c2, bookmarked c2 on page 12, and added m2
        let remote = LibrarySyncSnapshot {
            schema_version: 1,
            app_version: "0.2.0".to_string(),
            synced_at: "2026-11-21T12:00:00Z".to_string(),
            mangas: vec![
                SyncMangaRecord {
                    id: "m1".to_string(),
                    title: "One Piece".to_string(),
                    created_at: Some("2026-11-05 10:00:00".to_string()),
                    updated_at: Some("2026-11-21 12:00:00".to_string()),
                    last_read: Some("2026-11-21 12:00:00".to_string()),
                    deleted_at: None,
                    img_url: Some("cover.jpg".to_string()),
                    manga_provider: "mangadex".to_string(),
                    in_reading_history: true,
                    in_plan_to_read: true,
                },
                SyncMangaRecord {
                    id: "m2".to_string(),
                    title: "Berserk".to_string(),
                    created_at: Some("2026-11-21 11:00:00".to_string()),
                    updated_at: Some("2026-11-21 11:00:00".to_string()),
                    last_read: Some("2026-11-21 11:00:00".to_string()),
                    deleted_at: None,
                    img_url: None,
                    manga_provider: "weebcentral".to_string(),
                    in_reading_history: false,
                    in_plan_to_read: true,
                },
            ],
            chapters: vec![SyncChapterRecord {
                id: "c2".to_string(),
                title: "Cap 2".to_string(),
                manga_id: "m1".to_string(),
                is_read: true,
                is_downloaded: false,
                is_bookmarked: true,
                translated_language: Some("es".to_string()),
                number_page_bookmarked: Some(12),
            }],
            favorites: vec![SyncFavoriteRecord {
                manga_id: "m2".to_string(),
                provider: "weebcentral".to_string(),
                title: "Berserk".to_string(),
                created_at: Some("2026-11-21 11:00:00".to_string()),
            }],
            dismissed_notifications: vec![],
            search_history: vec![SyncSearchRecord {
                query: "One Piece".to_string(),
                searched_at: 250,
            }],
        };

        let merged = merge_library_snapshots(&local, &remote);
        assert_eq!(merged.mangas.len(), 2);

        let m1 = merged.mangas.iter().find(|m| m.id == "m1").unwrap();
        assert_eq!(m1.last_read.as_deref(), Some("2026-11-21 12:00:00"));
        assert_eq!(m1.created_at.as_deref(), Some("2026-11-01 10:00:00"));
        assert!(m1.in_reading_history);
        assert!(m1.in_plan_to_read);
        assert_eq!(m1.img_url.as_deref(), Some("cover.jpg"));

        // Both c1 and c2 are read, c1 retains is_downloaded, and only c2 is bookmarked (since remote was newer)
        let c1 = merged.chapters.iter().find(|c| c.id == "c1").unwrap();
        assert!(c1.is_read);
        assert!(c1.is_downloaded);
        assert!(!c1.is_bookmarked);

        let c2 = merged.chapters.iter().find(|c| c.id == "c2").unwrap();
        assert!(c2.is_read);
        assert!(c2.is_bookmarked);
        assert_eq!(c2.number_page_bookmarked, Some(12));

        assert_eq!(merged.favorites.len(), 2);
        assert_eq!(merged.search_history.len(), 1);
        assert_eq!(merged.search_history[0].searched_at, 250);
    }

    #[tokio::test]
    async fn google_drive_client_syncs_library_and_downloads_over_rest_v3() {
        let server = MockServer::start_async().await;

        let _about_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/drive/v3/about");
                then.status(200).json_body(serde_json::json!({
                    "user": {
                        "displayName": "Test Reader",
                        "emailAddress": "reader@example.com"
                    }
                }));
            })
            .await;

        let _folder_lookup_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/drive/v3/files");
                then.status(200).json_body(serde_json::json!({
                    "files": [
                        {
                            "id": "folder_root_123",
                            "name": "MangaTUI",
                            "mimeType": "application/vnd.google-apps.folder"
                        }
                    ]
                }));
            })
            .await;

        let _create_file_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/drive/v3/files");
                then.status(200).json_body(serde_json::json!({
                    "id": "created_file_456"
                }));
            })
            .await;

        let _upload_media_mock = server
            .mock_async(|when, then| {
                when.method(PATCH).path("/upload/drive/v3/files/created_file_456");
                then.status(200).json_body(serde_json::json!({
                    "id": "created_file_456"
                }));
            })
            .await;

        let client = GoogleDriveClient::new().with_endpoints(
            server.url("/o/oauth2/v2/auth"),
            server.url("/token"),
            server.url("/drive/v3"),
            server.url("/upload/drive/v3"),
        );

        let base_dir = PathBuf::from("./test_results/cloud_gdrive_api_test");
        let _ = fs::remove_dir_all(&base_dir);
        fs::create_dir_all(&base_dir).unwrap();
        let db_path = base_dir.join("test_history.db");
        let dl_dir = base_dir.join("downloads");
        fs::create_dir_all(&dl_dir).unwrap();

        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let db = Database::new(&conn);
            db.setup().unwrap();
            db.save_history(crate::backend::database::MangaReadingHistorySave {
                id: "manga_1",
                title: "Chainsaw Man",
                img_url: None,
                chapter: crate::backend::database::ChapterToSaveHistory {
                    id: "ch_1",
                    title: "Cap 1",
                    translated_language: "es",
                },
                provider: MangaProviders::Mangadex,
            })
            .unwrap();
        }

        let mut state = CloudConnectionState {
            backend: Some(CloudBackendKind::GoogleDriveApi),
            client_id: Some("test_client_id".to_string()),
            account_email: Some("reader@example.com".to_string()),
            fallback_access_token: Some("valid_test_token".to_string()),
            ..Default::default()
        };

        let mut steps = Vec::new();
        let report = run_cloud_sync_with_paths(&mut state, CloudSyncMode::LibraryOnly, &client, &db_path, &dl_dir, &mut |p| {
            steps.push(p.step_label)
        })
        .await
        .unwrap();

        assert_eq!(report.total_mangas, 1);
        assert_eq!(report.total_read_chapters, 1);
        assert!(!steps.is_empty());

        let _ = fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn local_cloud_folder_sync_merges_db_and_syncs_chapter_archives_bidirectionally() {
        let base_dir = PathBuf::from("./test_results/cloud_folder_bidirectional_test");
        let _ = fs::remove_dir_all(&base_dir);
        let cloud_dir = base_dir.join("cloud").join("MangaTUI");
        let local_db = base_dir.join("local_history.db");
        let local_dl = base_dir.join("local_downloads");
        fs::create_dir_all(&cloud_dir).unwrap();
        fs::create_dir_all(&local_dl).unwrap();

        // 1. Seed local DB and a local CBZ download
        {
            let conn = rusqlite::Connection::open(&local_db).unwrap();
            let db = Database::new(&conn);
            db.setup().unwrap();
            db.save_history(crate::backend::database::MangaReadingHistorySave {
                id: "m_local",
                title: "Dandadan",
                img_url: None,
                chapter: crate::backend::database::ChapterToSaveHistory {
                    id: "ch_local_1",
                    title: "Cap 1",
                    translated_language: "es",
                },
                provider: MangaProviders::Mangadex,
            })
            .unwrap();
        }
        let local_manga_folder = local_dl.join("Dandadan");
        fs::create_dir_all(&local_manga_folder).unwrap();
        fs::write(local_manga_folder.join("ch_local_1.cbz"), b"dummy_cbz_bytes").unwrap();

        // 2. Seed remote cloud folder with a remote library_sync.json and a remote EPUB chapter
        let remote_snap = LibrarySyncSnapshot {
            schema_version: 1,
            app_version: "0.2.0".to_string(),
            synced_at: "2026-11-21T14:00:00Z".to_string(),
            mangas: vec![SyncMangaRecord {
                id: "m_remote".to_string(),
                title: "Sakamoto Days".to_string(),
                created_at: None,
                updated_at: None,
                last_read: None,
                deleted_at: None,
                img_url: None,
                manga_provider: "mangadex".to_string(),
                in_reading_history: true,
                in_plan_to_read: false,
            }],
            chapters: vec![SyncChapterRecord {
                id: "ch_remote_1".to_string(),
                title: "Cap 1".to_string(),
                manga_id: "m_remote".to_string(),
                is_read: true,
                is_downloaded: true,
                is_bookmarked: false,
                translated_language: Some("es".to_string()),
                number_page_bookmarked: None,
            }],
            favorites: vec![],
            dismissed_notifications: vec![],
            search_history: vec![],
        };
        fs::write(cloud_dir.join(CLOUD_LIBRARY_JSON_NAME), serde_json::to_vec_pretty(&remote_snap).unwrap()).unwrap();
        let cloud_remote_manga_dir = cloud_dir.join(CLOUD_DOWNLOADS_FOLDER_NAME).join("Sakamoto Days");
        fs::create_dir_all(&cloud_remote_manga_dir).unwrap();
        fs::write(cloud_remote_manga_dir.join("ch_remote_1.epub"), b"dummy_epub_bytes").unwrap();

        let report = sync_with_local_cloud_folder(&cloud_dir, CloudSyncMode::FullSync, &local_db, &local_dl, &mut |_| {}).unwrap();

        assert_eq!(report.total_mangas, 2);
        assert_eq!(report.total_read_chapters, 2);
        assert_eq!(report.chapters_uploaded, 1);
        assert_eq!(report.chapters_downloaded, 1);
        assert!(
            cloud_dir
                .join(CLOUD_DOWNLOADS_FOLDER_NAME)
                .join("Dandadan")
                .join("ch_local_1.cbz")
                .is_file()
        );
        assert!(local_dl.join("Sakamoto Days").join("ch_remote_1.epub").is_file());

        let _ = fs::remove_dir_all(&base_dir);
    }
}
