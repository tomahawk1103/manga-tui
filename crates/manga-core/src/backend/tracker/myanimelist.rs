//! Official MyAnimeList API v2 OAuth 2.0 + PKCE tracker integration.
//!
//! Each user links their own account privately using their personal MyAnimeList Client ID
//! (or `MANGA_TUI_MAL_CLIENT_ID`). Access and refresh tokens are stored securely in the
//! operating system keyring (`KeyringStorage`).

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rand::Rng;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::SearchTerm;
use crate::backend::database::{ChapterToSaveHistory, Database, MangaPlanToReadSave, MangaReadingHistorySave};
use crate::backend::manga_provider::MangaProviders;
use crate::backend::secrets::SecretStorage;
use crate::backend::secrets::keyring::KeyringStorage;
use crate::backend::tracker::{MangaToTrack, MangaTracker, MarkAsRead, PlanToReadArgs};
use crate::i18n::tr;

pub const MAL_OAUTH_AUTHORIZE_URL: &str = "https://myanimelist.net/v1/oauth2/authorize";
pub const MAL_OAUTH_TOKEN_URL: &str = "https://myanimelist.net/v1/oauth2/token";
pub const MAL_API_V2_BASE_URL: &str = "https://api.myanimelist.net/v2";
pub const MAL_APICONFIG_URL: &str = "https://myanimelist.net/apiconfig";

pub const MAL_STATE_FILE_NAME: &str = "mal_state.json";

pub const SECRET_MAL_CLIENT_ID: &str = "mal_client_id";
pub const SECRET_MAL_CLIENT_SECRET: &str = "mal_client_secret";
pub const SECRET_MAL_ACCESS_TOKEN: &str = "mal_access_token";
pub const SECRET_MAL_REFRESH_TOKEN: &str = "mal_refresh_token";

/// Generates a 64-character URL-safe PKCE verifier (`code_challenge_method=plain`, required by MyAnimeList OAuth 2.0).
#[derive(Debug, Clone)]
pub struct MalPkceChallenge {
    pub code_verifier: String,
    pub state: String,
}

impl MalPkceChallenge {
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
        Self {
            code_verifier,
            state,
        }
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

pub fn build_mal_oauth_authorize_url(auth_endpoint: &str, client_id: &str, redirect_uri: &str, pkce: &MalPkceChallenge) -> String {
    format!(
        "{auth_endpoint}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=plain&state={}",
        url_encode_component(client_id),
        url_encode_component(redirect_uri),
        url_encode_component(&pkce.code_verifier),
        url_encode_component(&pkce.state),
    )
}

#[derive(Debug, Clone, Deserialize)]
pub struct MalTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MalUserProfile {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MalSyncReport {
    pub synced_at: String,
    pub imported_from_mal: usize,
    pub pushed_to_mal: usize,
    pub total_mal_entries: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MalConnectionState {
    pub client_id: Option<String>,
    pub username: Option<String>,
    pub user_id: Option<u64>,
    pub last_sync_at: Option<String>,
    pub last_sync_report: Option<MalSyncReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_client_secret: Option<String>,
}

impl MalConnectionState {
    pub fn state_file_path() -> PathBuf {
        if let Some(dir) = crate::backend::APP_DATA_DIR.as_ref() {
            dir.join(MAL_STATE_FILE_NAME)
        } else {
            PathBuf::from(MAL_STATE_FILE_NAME)
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
            && let Ok(env_id) = std::env::var("MANGA_TUI_MAL_CLIENT_ID")
            && !env_id.trim().is_empty()
        {
            state.client_id = Some(env_id.trim().to_string());
        }
        state
    }

    pub fn save(&self) -> Result<(), String> {
        if cfg!(any(test, feature = "test-utils")) {
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
        // Never persist raw tokens in plain JSON on disk when OS keyring is available
        let mut sanitized = self.clone();
        if !cfg!(any(test, feature = "test-utils")) {
            sanitized.fallback_access_token = None;
            sanitized.fallback_refresh_token = None;
            sanitized.fallback_client_secret = None;
        }
        let json = serde_json::to_string_pretty(&sanitized).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn is_connected(&self) -> bool {
        self.get_access_token().is_some() || self.get_refresh_token().is_some()
    }

    pub fn connection_summary(&self) -> String {
        if !self.is_connected() {
            return tr("Sin vincular (Cuenta privada desactivada)", "Not linked (Private account disconnected)").to_string();
        }
        if let Some(name) = &self.username {
            format!("MyAnimeList · @{name}")
        } else {
            "MyAnimeList · OAuth 2.0 PKCE".to_string()
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

        if !cfg!(any(test, feature = "test-utils")) {
            let storage = KeyringStorage::new();
            let _ = storage.save_secret(SECRET_MAL_CLIENT_ID, client_id.trim());
            let _ = storage.save_secret(SECRET_MAL_ACCESS_TOKEN, access_token.trim());
            if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
                let _ = storage.save_secret(SECRET_MAL_CLIENT_SECRET, secret);
            }
            if let Some(refresh) = refresh_token.map(str::trim).filter(|s| !s.is_empty()) {
                let _ = storage.save_secret(SECRET_MAL_REFRESH_TOKEN, refresh);
            }
        }
    }

    pub fn get_client_id(&self) -> Option<String> {
        if let Ok(env_id) = std::env::var("MANGA_TUI_MAL_CLIENT_ID")
            && !env_id.trim().is_empty()
        {
            return Some(env_id.trim().to_string());
        }
        if let Some(saved) = self.client_id.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(saved.trim().to_string());
        }
        if !cfg!(any(test, feature = "test-utils"))
            && let Ok(Some(kr)) = KeyringStorage::new().get_secret(SECRET_MAL_CLIENT_ID)
            && !kr.trim().is_empty()
        {
            return Some(kr.trim().to_string());
        }
        option_env!("MANGA_TUI_EMBEDDED_MAL_CLIENT_ID")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    }

    pub fn get_client_secret(&self) -> Option<String> {
        if let Ok(env_sec) = std::env::var("MANGA_TUI_MAL_CLIENT_SECRET")
            && !env_sec.trim().is_empty()
        {
            return Some(env_sec.trim().to_string());
        }
        if let Some(saved) = self.fallback_client_secret.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(saved.trim().to_string());
        }
        if !cfg!(any(test, feature = "test-utils"))
            && let Ok(Some(kr)) = KeyringStorage::new().get_secret(SECRET_MAL_CLIENT_SECRET)
            && !kr.trim().is_empty()
        {
            return Some(kr.trim().to_string());
        }
        None
    }

    pub fn get_access_token(&self) -> Option<String> {
        if let Some(tok) = self.fallback_access_token.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(tok.clone());
        }
        if cfg!(any(test, feature = "test-utils")) {
            return None;
        }
        KeyringStorage::new()
            .get_secret(SECRET_MAL_ACCESS_TOKEN)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn get_refresh_token(&self) -> Option<String> {
        if let Some(tok) = self.fallback_refresh_token.as_ref().filter(|s| !s.trim().is_empty()) {
            return Some(tok.clone());
        }
        if cfg!(any(test, feature = "test-utils")) {
            return None;
        }
        KeyringStorage::new()
            .get_secret(SECRET_MAL_REFRESH_TOKEN)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn clear_credentials(&mut self) {
        if !cfg!(any(test, feature = "test-utils")) {
            let storage = KeyringStorage::new();
            let _ = storage.remove_secret(SECRET_MAL_ACCESS_TOKEN);
            let _ = storage.remove_secret(SECRET_MAL_REFRESH_TOKEN);
            let _ = storage.remove_secret(SECRET_MAL_CLIENT_SECRET);
        }
        self.username = None;
        self.user_id = None;
        self.fallback_access_token = None;
        self.fallback_refresh_token = None;
        self.fallback_client_secret = None;
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MalSearchListResponse {
    #[serde(default)]
    data: Vec<MalSearchNodeWrapper>,
}

#[derive(Debug, Clone, Deserialize)]
struct MalSearchNodeWrapper {
    node: MalMangaNode,
    #[serde(default)]
    list_status: Option<MalListStatus>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MalMangaNode {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub main_picture: Option<MalPicture>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MalPicture {
    #[serde(default)]
    pub medium: Option<String>,
    #[serde(default)]
    pub large: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MalListStatus {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub num_chapters_read: u32,
    #[serde(default)]
    pub num_volumes_read: u32,
}

#[derive(Debug, Clone)]
pub struct MalUserMangaEntry {
    pub mal_id: u64,
    pub title: String,
    pub status: String,
    pub num_chapters_read: u32,
    pub picture_url: Option<String>,
}

/// Official MyAnimeList API v2 Client & `MangaTracker` implementation.
#[derive(Debug, Clone)]
pub struct MyAnimeListClient {
    http: Client,
    auth_url: String,
    token_url: String,
    api_base_url: String,
    client_id: Option<String>,
    access_token: Option<String>,
}

impl Default for MyAnimeListClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MyAnimeListClient {
    pub fn new() -> Self {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!("manga-tui/{}", env!("CARGO_PKG_VERSION")));
        if cfg!(any(test, feature = "test-utils")) {
            builder = builder.no_proxy();
        }
        let http = builder.build().unwrap_or_else(|_| Client::new());

        Self {
            http,
            auth_url: MAL_OAUTH_AUTHORIZE_URL.to_string(),
            token_url: MAL_OAUTH_TOKEN_URL.to_string(),
            api_base_url: MAL_API_V2_BASE_URL.to_string(),
            client_id: None,
            access_token: None,
        }
    }

    pub fn from_saved_state() -> Option<Self> {
        let state = MalConnectionState::load();
        let token = state.get_access_token()?;
        Some(Self::new().with_access_token(token).with_client_id_opt(state.get_client_id()))
    }

    pub fn with_endpoints(
        mut self,
        auth_url: impl Into<String>,
        token_url: impl Into<String>,
        api_base_url: impl Into<String>,
    ) -> Self {
        self.auth_url = auth_url.into();
        self.token_url = token_url.into();
        self.api_base_url = api_base_url.into();
        self
    }

    pub fn with_access_token(mut self, token: impl Into<String>) -> Self {
        self.access_token = Some(token.into());
        self
    }

    pub fn with_client_id_opt(mut self, client_id: Option<String>) -> Self {
        self.client_id = client_id;
        self
    }

    /// Runs the loopback OAuth 2.0 + PKCE flow for MyAnimeList.
    /// Binds to `127.0.0.1:8765` first (so users can register a fixed Redirect URL `http://127.0.0.1:8765/callback` in MAL),
    /// falling back to an ephemeral port `127.0.0.1:0` if 8765 is busy.
    pub async fn run_loopback_oauth_flow<F>(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        mut on_authorize_url: F,
    ) -> Result<MalTokenResponse, String>
    where
        F: FnMut(&str),
    {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(
                tr("El Client ID de MyAnimeList no puede estar vacío.", "MyAnimeList Client ID cannot be empty.").to_string()
            );
        }

        let listener = match TcpListener::bind("127.0.0.1:8765").await {
            Ok(l) => l,
            Err(_) => TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|e| format!("Failed to bind local OAuth callback port: {e}"))?,
        };
        let local_addr = listener.local_addr().map_err(|e| e.to_string())?;
        let redirect_uri = format!("http://127.0.0.1:{}/callback", local_addr.port());
        let pkce = MalPkceChallenge::generate();
        let authorize_url = build_mal_oauth_authorize_url(&self.auth_url, client_id, &redirect_uri, &pkce);

        on_authorize_url(&authorize_url);

        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(180), listener.accept())
            .await
            .map_err(|_| {
                tr(
                    "Tiempo de espera agotado (3 min) esperando la autorización de MyAnimeList en el navegador.",
                    "Timed out (3 min) waiting for MyAnimeList authorization in the browser.",
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
                 <h2>❌ Error de autorización en MyAnimeList</h2><p>{err}</p></body></html>"
            );
            let _ = socket.write_all(html.as_bytes()).await;
            return Err(format!("MyAnimeList OAuth error: {err}"));
        }

        // Strict anti-CSRF state verification
        if let Some(returned_state) = extract_query_param(query, "state")
            && returned_state != pkce.state
        {
            return Err(tr(
                "Error de seguridad OAuth: el parámetro state no coincide (posible CSRF).",
                "OAuth security error: state parameter mismatch (possible CSRF).",
            )
            .to_string());
        }

        let Some(code) = extract_query_param(query, "code") else {
            let html = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                        <html><body>Missing authorization code</body></html>";
            let _ = socket.write_all(html.as_bytes()).await;
            return Err(tr(
                "No se recibió código de autorización de MyAnimeList.",
                "No OAuth authorization code received from MyAnimeList.",
            )
            .to_string());
        };

        let success_html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n\
            <html><head><title>Manga TUI - MyAnimeList</title></head>\
            <body style='font-family:system-ui,sans-serif;background:#1a1b26;color:#c0caf5;text-align:center;padding:60px;'>\
            <div style='max-width:480px;margin:0 auto;background:#24283b;padding:32px;border-radius:12px;border:1px solid #2e51a2;'>\
            <h2 style='color:#9ece6a;margin-top:0;'>✓ Cuenta de MyAnimeList vinculada con Manga TUI</h2>\
            <p>Tu cuenta se ha conectado de forma privada y segura mediante OAuth 2.0 + PKCE. Ya puedes cerrar esta pestaña y volver a <b>Manga TUI</b>.</p>\
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
    ) -> Result<MalTokenResponse, String> {
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

        let resp = self
            .http
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("MAL token exchange request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("MAL token exchange failed ({status}): {body}"));
        }

        resp.json::<MalTokenResponse>()
            .await
            .map_err(|e| format!("Invalid MAL OAuth token JSON: {e}"))
    }

    pub async fn refresh_access_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<MalTokenResponse, String> {
        let mut params = vec![
            ("client_id", client_id.to_string()),
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.to_string()),
        ];
        if let Some(secret) = client_secret.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(("client_secret", secret.to_string()));
        }

        let resp = self
            .http
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("MAL token refresh request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("MAL token refresh failed ({status}): {body}"));
        }

        resp.json::<MalTokenResponse>()
            .await
            .map_err(|e| format!("Invalid MAL refresh token JSON: {e}"))
    }

    /// Fetches the authenticated user's MyAnimeList profile (`GET /v2/users/@me`).
    pub async fn get_my_profile(&self, access_token: &str) -> Result<MalUserProfile, String> {
        let url = format!("{}/users/@me", self.api_base_url);
        let resp = self
            .http
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| format!("MAL profile request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("MAL profile lookup failed ({status}): {body}"));
        }

        resp.json::<MalUserProfile>()
            .await
            .map_err(|e| format!("Invalid MAL user profile JSON: {e}"))
    }

    /// Fetches the user's manga list from MyAnimeList (`GET /v2/users/@me/mangalist?fields=list_status&limit=500`).
    pub async fn get_my_manga_list(&self, access_token: &str) -> Result<Vec<MalUserMangaEntry>, String> {
        let url = format!("{}/users/@me/mangalist?fields=list_status&limit=500", self.api_base_url);
        let resp = self
            .http
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| format!("MAL mangalist request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("MAL mangalist lookup failed ({status}): {body}"));
        }

        let parsed: MalSearchListResponse = resp.json().await.map_err(|e| format!("Invalid MAL mangalist JSON: {e}"))?;

        Ok(parsed
            .data
            .into_iter()
            .map(|item| {
                let (status, num_chapters_read) = match item.list_status {
                    Some(ls) => (ls.status.unwrap_or_else(|| "reading".to_string()), ls.num_chapters_read),
                    None => ("reading".to_string(), 0),
                };
                let picture_url = item.node.main_picture.and_then(|p| p.large.or(p.medium));
                MalUserMangaEntry {
                    mal_id: item.node.id,
                    title: item.node.title,
                    status,
                    num_chapters_read,
                    picture_url,
                }
            })
            .collect())
    }

    /// Updates a manga's reading status and chapter count on the user's MyAnimeList account
    /// (`PATCH /v2/manga/{manga_id}/my_list_status`).
    pub async fn update_manga_list_status(
        &self,
        access_token: &str,
        mal_manga_id: &str,
        status: &str,
        num_chapters_read: Option<u32>,
        num_volumes_read: Option<u32>,
    ) -> Result<(), String> {
        let url = format!("{}/manga/{}/my_list_status", self.api_base_url, mal_manga_id.trim());
        let mut params = vec![("status", status.to_string())];
        if let Some(ch) = num_chapters_read {
            params.push(("num_chapters_read", ch.to_string()));
        }
        if let Some(vol) = num_volumes_read {
            params.push(("num_volumes_read", vol.to_string()));
        }

        let resp = self
            .http
            .patch(&url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, "application/json")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("MAL update list status failed: {e}"))?;

        if !resp.status().is_success() {
            let status_code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Could not update MyAnimeList status ({status_code}): {body}"));
        }
        Ok(())
    }

    /// Searches a manga on MyAnimeList v2 by title (`GET /v2/manga?q=...&limit=1`).
    pub async fn search_mal_manga(&self, access_token: &str, query: &str) -> Result<Option<MalMangaNode>, String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let encoded: String = trimmed.chars().take(64).collect();
        let url = format!("{}/manga?q={}&limit=1", self.api_base_url, url_encode_component(&encoded));
        let resp = self
            .http
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", access_token.trim()))
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| format!("MAL search manga failed: {e}"))?;

        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status_code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("MAL search error ({status_code}): {body}"));
        }

        let parsed: MalSearchListResponse = resp.json().await.map_err(|e| format!("Invalid MAL search JSON: {e}"))?;

        Ok(parsed.data.into_iter().next().map(|w| w.node))
    }

    /// Synchronizes the user's MyAnimeList manga list with the local SQLite database:
    /// 1. Pulls all entries from the user's mobile/web MyAnimeList account (`reading`, `plan_to_read`, `completed`) into local
    ///    library.
    /// 2. Pushes local reading progress (`total_read_count`) up to MyAnimeList if local progress is ahead.
    pub async fn sync_with_database(&self, state: &mut MalConnectionState, db_path: &Path) -> Result<MalSyncReport, String> {
        let token = state.get_access_token().ok_or_else(|| {
            tr("Primero inicia sesión en tu cuenta de MyAnimeList.", "Please sign in to your MyAnimeList account first.")
                .to_string()
        })?;

        let mal_entries = self.get_my_manga_list(&token).await?;
        let total_mal_entries = mal_entries.len();
        let mut imported_from_mal = 0usize;
        let mut pushed_to_mal = 0usize;

        let local_snap = {
            let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
            let db = Database::new(&conn);
            let _ = db.setup();

            // Snapshot local library before importing
            let snap = db.export_library_sync_snapshot().map_err(|e| e.to_string())?;

            // 1. Import remote MAL entries into local SQLite database so mobile mangas appear in manga-tui
            for entry in &mal_entries {
                let mal_manga_id = format!("mal:{}", entry.mal_id);
                let already_exists = snap
                    .mangas
                    .iter()
                    .any(|m| m.id == mal_manga_id || m.title.eq_ignore_ascii_case(&entry.title));

                if entry.status == "plan_to_read" {
                    if !already_exists {
                        let _ = db.save_plan_to_read(MangaPlanToReadSave {
                            id: &mal_manga_id,
                            title: &entry.title,
                            img_url: entry.picture_url.as_deref(),
                            provider: MangaProviders::Mangadex,
                        });
                        imported_from_mal += 1;
                    }
                } else if entry.num_chapters_read > 0 || entry.status == "reading" || entry.status == "completed" {
                    let ch_num = entry.num_chapters_read.max(1);
                    let ch_id = format!("mal:{}:ch:{ch_num}", entry.mal_id);
                    let ch_title = format!("Chapter {ch_num}");
                    if !already_exists {
                        let _ = db.save_history(MangaReadingHistorySave {
                            id: &mal_manga_id,
                            title: &entry.title,
                            img_url: entry.picture_url.as_deref(),
                            chapter: ChapterToSaveHistory {
                                id: &ch_id,
                                title: &ch_title,
                                translated_language: "es",
                            },
                            provider: MangaProviders::Mangadex,
                        });
                        imported_from_mal += 1;
                    }
                }
            }
            snap
        };

        // 2. Push local reading progress to MyAnimeList when local read chapter count is higher
        for local_manga in &local_snap.mangas {
            if !local_manga.in_reading_history && !local_manga.in_plan_to_read {
                continue;
            }
            let local_read_chapters = local_snap
                .chapters
                .iter()
                .filter(|c| c.manga_id == local_manga.id && c.is_read)
                .map(|c| {
                    crate::backend::database::extract_normalized_chapter_number(&c.title)
                        .and_then(|n| n.parse::<f64>().ok())
                        .map(|v| v.floor() as u32)
                        .unwrap_or(1)
                })
                .max()
                .unwrap_or(0);

            if let Some(existing_mal) = mal_entries.iter().find(|e| e.title.eq_ignore_ascii_case(&local_manga.title)) {
                if local_read_chapters > existing_mal.num_chapters_read {
                    let mal_id_str = existing_mal.mal_id.to_string();
                    if self
                        .update_manga_list_status(&token, &mal_id_str, "reading", Some(local_read_chapters), None)
                        .await
                        .is_ok()
                    {
                        pushed_to_mal += 1;
                    }
                }
            }
        }

        let report = MalSyncReport {
            synced_at: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            imported_from_mal,
            pushed_to_mal,
            total_mal_entries,
        };
        state.last_sync_at = Some(report.synced_at.clone());
        state.last_sync_report = Some(report.clone());
        let _ = state.save();

        Ok(report)
    }
}

impl MangaTracker for MyAnimeListClient {
    async fn search_manga_by_title(&self, title: SearchTerm) -> Result<Option<MangaToTrack>, Box<dyn Error>> {
        let Some(token) = self.access_token.as_deref() else {
            return Ok(None);
        };
        let res = self.search_mal_manga(token, title.get()).await?;
        Ok(res.map(|node| MangaToTrack {
            id: node.id.to_string(),
        }))
    }

    async fn mark_manga_as_read_with_chapter_count(&self, manga: MarkAsRead<'_>) -> Result<(), Box<dyn Error>> {
        let Some(token) = self.access_token.as_deref() else {
            return Ok(());
        };
        self.update_manga_list_status(token, manga.id, "reading", Some(manga.chapter_number), manga.volume_number)
            .await?;
        Ok(())
    }

    async fn mark_manga_as_plan_to_read(&self, manga_to_plan_to_read: PlanToReadArgs<'_>) -> Result<(), Box<dyn Error>> {
        let Some(token) = self.access_token.as_deref() else {
            return Ok(());
        };
        self.update_manga_list_status(token, manga_to_plan_to_read.id, "plan_to_read", None, None)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use httpmock::Method::{GET, PATCH, POST};
    use httpmock::MockServer;

    use super::*;

    #[test]
    fn mal_pkce_generates_valid_64_char_plain_verifier_and_authorize_url() {
        let pkce = MalPkceChallenge::generate();
        assert_eq!(pkce.code_verifier.len(), 64);
        assert_eq!(pkce.state.len(), 24);

        let url = build_mal_oauth_authorize_url(
            MAL_OAUTH_AUTHORIZE_URL,
            "my_personal_client_id",
            "http://127.0.0.1:8765/callback",
            &pkce,
        );
        assert!(url.contains("client_id=my_personal_client_id"));
        assert!(url.contains("code_challenge_method=plain"));
        assert!(url.contains(&format!("code_challenge={}", pkce.code_verifier)));
        assert!(url.contains(&format!("state={}", pkce.state)));
    }

    #[tokio::test]
    async fn mal_client_searches_updates_progress_and_syncs_mangalist_over_api_v2() {
        let server = MockServer::start_async().await;

        let _profile_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/users/@me").header("authorization", "Bearer mal_test_token");
                then.status(200).json_body(serde_json::json!({
                    "id": 998877,
                    "name": "FrierenReader"
                }));
            })
            .await;

        let _search_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/v2/manga").header("authorization", "Bearer mal_test_token");
                then.status(200).json_body(serde_json::json!({
                    "data": [
                        {
                            "node": {
                                "id": 126287,
                                "title": "Sousou no Frieren"
                            }
                        }
                    ]
                }));
            })
            .await;

        let _patch_mock = server
            .mock_async(|when, then| {
                when.method(PATCH)
                    .path("/v2/manga/126287/my_list_status")
                    .header("authorization", "Bearer mal_test_token");
                then.status(200).json_body(serde_json::json!({
                    "status": "reading",
                    "num_chapters_read": 42
                }));
            })
            .await;

        let _list_mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/v2/users/@me/mangalist")
                    .header("authorization", "Bearer mal_test_token");
                then.status(200).json_body(serde_json::json!({
                    "data": [
                        {
                            "node": {
                                "id": 126287,
                                "title": "Sousou no Frieren"
                            },
                            "list_status": {
                                "status": "reading",
                                "num_chapters_read": 15,
                                "num_volumes_read": 2
                            }
                        },
                        {
                            "node": {
                                "id": 2,
                                "title": "Berserk"
                            },
                            "list_status": {
                                "status": "plan_to_read",
                                "num_chapters_read": 0,
                                "num_volumes_read": 0
                            }
                        }
                    ]
                }));
            })
            .await;

        let _token_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/oauth2/token");
                then.status(200).json_body(serde_json::json!({
                    "token_type": "Bearer",
                    "expires_in": 2678400,
                    "access_token": "mal_test_token",
                    "refresh_token": "mal_refresh_token"
                }));
            })
            .await;

        let client = MyAnimeListClient::new()
            .with_endpoints(server.url("/v1/oauth2/authorize"), server.url("/v1/oauth2/token"), server.url("/v2"))
            .with_access_token("mal_test_token");

        let profile = client.get_my_profile("mal_test_token").await.unwrap();
        assert_eq!(profile.name, "FrierenReader");
        assert_eq!(profile.id, 998877);

        // Verify MangaTracker trait methods
        let found = client
            .search_manga_by_title(SearchTerm::trimmed("Sousou no Frieren").unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, "126287");

        client
            .mark_manga_as_read_with_chapter_count(MarkAsRead {
                id: "126287",
                chapter_number: 42,
                volume_number: Some(5),
            })
            .await
            .unwrap();

        // Verify bidirectional sync with SQLite database
        let base_dir = PathBuf::from("./test_results/mal_sync_test");
        let _ = fs::remove_dir_all(&base_dir);
        fs::create_dir_all(&base_dir).unwrap();
        let db_path = base_dir.join("test_mal.db");

        let mut state = MalConnectionState {
            client_id: Some("user_private_client_id".to_string()),
            username: Some("FrierenReader".to_string()),
            fallback_access_token: Some("mal_test_token".to_string()),
            ..Default::default()
        };

        let report = client.sync_with_database(&mut state, &db_path).await.unwrap();
        assert_eq!(report.total_mal_entries, 2);
        assert_eq!(report.imported_from_mal, 2);

        let _ = fs::remove_dir_all(&base_dir);
    }
}
