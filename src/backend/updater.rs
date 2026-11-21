use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::backend::is_portable_next_to_exe;
use crate::backend::release_notifier::{GITHUB_URL, ReleaseNotifier};
use crate::global::APP_USER_AGENT;
use crate::i18n::tr;

const NIGHTLY_META_PREFIX: &str = "<!-- manga-tui-meta:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildIdentity {
    pub version: String,
    pub commit: String,
    pub commit_short: String,
    pub timestamp: i64,
    pub channel: String,
}

impl BuildIdentity {
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            commit: env!("MANGA_TUI_BUILD_COMMIT").to_string(),
            commit_short: env!("MANGA_TUI_BUILD_COMMIT_SHORT").to_string(),
            timestamp: env!("MANGA_TUI_BUILD_TIMESTAMP").parse::<i64>().unwrap_or(0),
            channel: env!("MANGA_TUI_BUILD_CHANNEL").to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    WindowsInstaller,
    WindowsPortable,
    WindowsStandalone,
    UnixBinary,
}

impl InstallKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::WindowsInstaller => tr("Instalador Windows", "Windows Installer"),
            Self::WindowsPortable => tr("Portable Windows", "Windows Portable"),
            Self::WindowsStandalone => tr("Ejecutable Windows", "Windows Standalone"),
            Self::UnixBinary => tr("Binario Unix", "Unix Binary"),
        }
    }
}

pub fn detect_windows_install_kind(exe_path: &Path) -> InstallKind {
    let Some(exe_dir) = exe_path.parent() else {
        return InstallKind::WindowsStandalone;
    };

    if is_portable_next_to_exe(exe_dir) {
        return InstallKind::WindowsPortable;
    }

    if let Ok(entries) = fs::read_dir(exe_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let lower = file_name.to_string_lossy().to_ascii_lowercase();
            if lower.starts_with("unins") && lower.ends_with(".exe") && entry.path().is_file() {
                return InstallKind::WindowsInstaller;
            }
        }
    }

    InstallKind::WindowsStandalone
}

pub fn detect_install_kind(exe_path: &Path) -> InstallKind {
    #[cfg(target_os = "windows")]
    {
        detect_windows_install_kind(exe_path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = exe_path;
        InstallKind::UnixBinary
    }
}

pub fn detect_current_install_kind() -> InstallKind {
    match std::env::current_exe() {
        Ok(exe_path) => detect_install_kind(&exe_path),
        Err(_) => {
            if cfg!(target_os = "windows") {
                InstallKind::WindowsStandalone
            } else {
                InstallKind::UnixBinary
            }
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Nightly,
}

impl UpdateChannel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => tr("Estable", "Stable"),
            Self::Nightly => "Nightly",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub tag_name: String,
    pub display_version: String,
    pub channel: UpdateChannel,
    pub commit_short: Option<String>,
    pub published_at: Option<String>,
    pub release_notes: String,
    pub html_url: String,
    pub install_kind: InstallKind,
    pub download_asset: Option<ReleaseAsset>,
    pub sha256_asset: Option<ReleaseAsset>,
    pub checksum_txt_asset: Option<ReleaseAsset>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateProgressStage {
    Downloading,
    Verifying,
    Preparing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateProgress {
    pub stage: UpdateProgressStage,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedUpdate {
    pub available: AvailableUpdate,
    pub downloaded_file: PathBuf,
    pub verified_sha256: String,
    pub target_exe: PathBuf,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct NightlyMeta {
    commit: Option<String>,
    version: Option<String>,
    timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseDto {
    tag_name: String,
    name: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
    published_at: Option<String>,
    created_at: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubAssetDto>,
}

#[derive(Debug, Deserialize)]
struct GitHubAssetDto {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

fn parse_nightly_meta(body: &str) -> NightlyMeta {
    let mut meta = NightlyMeta::default();

    for line in body.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(NIGHTLY_META_PREFIX) else {
            continue;
        };
        let content = rest.strip_suffix("-->").unwrap_or(rest).trim();
        for token in content.split_whitespace() {
            let Some((key, value)) = token.split_once('=') else {
                continue;
            };
            let clean_val = value.trim();
            if clean_val.is_empty() {
                continue;
            }
            match key.trim() {
                "commit" => meta.commit = Some(clean_val.to_string()),
                "version" => meta.version = Some(clean_val.to_string()),
                "timestamp" => meta.timestamp = clean_val.parse::<i64>().ok(),
                _ => {},
            }
        }
        break;
    }

    meta
}

pub fn sanitize_release_notes(body: &str) -> String {
    let mut cleaned_lines: Vec<&str> = Vec::new();
    let mut previous_blank = true;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(NIGHTLY_META_PREFIX) {
            continue;
        }
        if trimmed.is_empty() {
            if !previous_blank {
                cleaned_lines.push("");
                previous_blank = true;
            }
            continue;
        }
        cleaned_lines.push(line.trim_end());
        previous_blank = false;
    }

    while cleaned_lines.last().is_some_and(|l| l.is_empty()) {
        cleaned_lines.pop();
    }

    cleaned_lines.join("\n")
}

fn is_valid_sha256_hex(candidate: &str) -> bool {
    candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn parse_sha256_for_asset(content: &str, asset_name: &str) -> Option<String> {
    let non_empty_lines: Vec<&str> = content.lines().map(str::trim).filter(|l| !l.is_empty()).collect();

    for line in &non_empty_lines {
        let mut parts = line.splitn(2, char::is_whitespace);
        let Some(hash_candidate) = parts.next() else {
            continue;
        };
        if !is_valid_sha256_hex(hash_candidate) {
            continue;
        }
        if let Some(rest) = parts.next() {
            let file_part = rest.trim().trim_start_matches('*');
            let base_name = Path::new(file_part).file_name().and_then(|n| n.to_str()).unwrap_or(file_part);
            if file_part.eq_ignore_ascii_case(asset_name) || base_name.eq_ignore_ascii_case(asset_name) {
                return Some(hash_candidate.to_ascii_lowercase());
            }
        }
    }

    if non_empty_lines.len() == 1 {
        let first_token = non_empty_lines[0].split_whitespace().next()?;
        if is_valid_sha256_hex(first_token) {
            return Some(first_token.to_ascii_lowercase());
        }
    }

    None
}

fn parse_rfc3339_timestamp(raw: Option<&str>) -> Option<i64> {
    raw.and_then(|s| chrono::DateTime::parse_from_rfc3339(s.trim()).ok())
        .map(|dt| dt.timestamp())
}

fn commits_match(a: &str, b: &str) -> bool {
    let a_trim = a.trim();
    let b_trim = b.trim();
    if a_trim.is_empty() || b_trim.is_empty() || a_trim.eq_ignore_ascii_case("unknown") || b_trim.eq_ignore_ascii_case("unknown") {
        return false;
    }

    let a_lower = a_trim.to_ascii_lowercase();
    let b_lower = b_trim.to_ascii_lowercase();
    if a_lower == b_lower {
        return true;
    }

    let min_len = a_lower.len().min(b_lower.len());
    min_len >= 7 && (a_lower.starts_with(&b_lower) || b_lower.starts_with(&a_lower))
}

fn to_release_asset(dto: &GitHubAssetDto) -> ReleaseAsset {
    ReleaseAsset {
        name: dto.name.clone(),
        download_url: dto.browser_download_url.clone(),
        size: dto.size,
    }
}

fn unix_asset_suffixes() -> &'static [&'static str] {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => &["-x86_64-unknown-linux-gnu.tar.gz"],
        ("macos", "x86_64") => &["-x86_64-apple-darwin.tar.gz"],
        ("macos", "aarch64") => &["-aarch64-apple-darwin.tar.gz"],
        _ => &[".tar.gz"],
    }
}

fn select_assets_for_install_kind(
    assets: &[GitHubAssetDto],
    install_kind: InstallKind,
) -> (Option<ReleaseAsset>, Option<ReleaseAsset>, Option<ReleaseAsset>) {
    let find_matching = |suffixes: &[&str]| -> Option<ReleaseAsset> {
        for suffix in suffixes {
            if let Some(asset) = assets.iter().find(|a| {
                let lower = a.name.to_ascii_lowercase();
                !lower.ends_with(".sha256") && lower.ends_with(suffix)
            }) {
                return Some(to_release_asset(asset));
            }
        }
        None
    };

    let download_asset = match install_kind {
        InstallKind::WindowsInstaller => {
            find_matching(&["-x86_64-pc-windows-msvc-setup.exe", "-x86_64-pc-windows-msvc-portable.zip"])
        },
        InstallKind::WindowsPortable | InstallKind::WindowsStandalone => find_matching(&["-x86_64-pc-windows-msvc-portable.zip"]),
        InstallKind::UnixBinary => find_matching(unix_asset_suffixes()),
    };

    let sha256_asset = download_asset.as_ref().and_then(|dl| {
        let expected_name = format!("{}.sha256", dl.name);
        assets.iter().find(|a| a.name.eq_ignore_ascii_case(&expected_name)).map(to_release_asset)
    });

    let checksum_txt_asset = assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("checksum.txt") || a.name.eq_ignore_ascii_case("checksums.txt"))
        .map(to_release_asset);

    (download_asset, sha256_asset, checksum_txt_asset)
}

#[derive(Debug, Clone)]
pub struct UpdateClient {
    github_api_base: Url,
    api_client: Client,
    download_client: Client,
}

impl UpdateClient {
    pub fn default_github() -> Self {
        let url = GITHUB_URL.parse().expect("GITHUB_URL must be valid");
        Self::new(url)
    }

    pub fn new(github_api_base: Url) -> Self {
        Self::with_proxy_setting(github_api_base, false)
    }

    #[cfg(test)]
    fn new_no_proxy(github_api_base: Url) -> Self {
        Self::with_proxy_setting(github_api_base, true)
    }

    fn with_proxy_setting(github_api_base: Url, disable_proxy: bool) -> Self {
        let mut default_headers = http::HeaderMap::new();
        default_headers.insert("X-GitHub-Api-Version", http::HeaderValue::from_static("2022-11-28"));
        default_headers.insert(http::header::ACCEPT, http::HeaderValue::from_static("application/vnd.github+json"));

        let mut api_builder = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .default_headers(default_headers)
            .user_agent(&*APP_USER_AGENT);

        let mut download_builder = Client::builder().connect_timeout(Duration::from_secs(10)).user_agent(&*APP_USER_AGENT);

        if disable_proxy {
            api_builder = api_builder.no_proxy();
            download_builder = download_builder.no_proxy();
        }

        let api_client = api_builder.build().expect("Failed to construct UpdateClient API client");
        let download_client = download_builder.build().expect("Failed to construct UpdateClient download client");

        Self {
            github_api_base,
            api_client,
            download_client,
        }
    }

    pub async fn check_for_update(
        &self,
        current: &BuildIdentity,
        install_kind: InstallKind,
    ) -> Result<Option<AvailableUpdate>, String> {
        let base = self.github_api_base.as_str().trim_end_matches('/');
        let endpoint = format!("{base}/releases?per_page=15");

        let response = self
            .api_client
            .get(&endpoint)
            .send()
            .await
            .map_err(|e| format!("{}: {e}", tr("No se pudo conectar con GitHub", "Could not connect to GitHub")))?;

        if response.status() != StatusCode::OK {
            return Err(format!(
                "{} (HTTP {})",
                tr("Error consultando releases en GitHub", "Failed to query GitHub releases"),
                response.status()
            ));
        }

        let releases: Vec<GitHubReleaseDto> = response
            .json()
            .await
            .map_err(|e| format!("{}: {e}", tr("Respuesta inválida de GitHub", "Invalid response from GitHub")))?;

        let current_ver = ReleaseNotifier::parse_version(&current.version).unwrap_or((0, 0, 0));

        // 1. Evaluate latest stable release (non-draft, non-prerelease with semver tag)
        let best_stable = releases
            .iter()
            .filter(|r| !r.draft.unwrap_or(false) && !r.prerelease.unwrap_or(false))
            .filter_map(|r| ReleaseNotifier::parse_version(&r.tag_name).map(|ver| (ver, r)))
            .max_by_key(|(ver, _)| *ver)
            .filter(|(ver, _)| *ver > current_ver);

        // 2. Evaluate nightly release (non-draft, tag_name == "nightly")
        let candidate_nightly = releases
            .iter()
            .filter(|r| !r.draft.unwrap_or(false) && r.tag_name.eq_ignore_ascii_case("nightly"))
            .filter_map(|r| {
                let body = r.body.as_deref().unwrap_or("");
                let meta = parse_nightly_meta(body);
                if meta.commit.as_deref().is_some_and(|mc| commits_match(mc, &current.commit)) {
                    return None;
                }

                let nightly_ver = meta.version.as_deref().and_then(ReleaseNotifier::parse_version);
                let nightly_ts = meta
                    .timestamp
                    .or_else(|| parse_rfc3339_timestamp(r.published_at.as_deref()))
                    .or_else(|| parse_rfc3339_timestamp(r.created_at.as_deref()));

                let newer_by_ver = nightly_ver.is_some_and(|nv| nv > current_ver);
                let newer_by_ts = nightly_ts.is_some_and(|nt| nt > 0 && current.timestamp > 0 && nt > current.timestamp);

                if newer_by_ver || newer_by_ts { Some((nightly_ver, nightly_ts, meta, r)) } else { None }
            })
            .next();

        let chosen = match (best_stable, candidate_nightly) {
            (Some((stable_ver, stable_rel)), Some((nightly_ver, nightly_ts, nightly_meta, nightly_rel))) => {
                let stable_ts = parse_rfc3339_timestamp(stable_rel.published_at.as_deref())
                    .or_else(|| parse_rfc3339_timestamp(stable_rel.created_at.as_deref()));
                let stable_higher_semver = nightly_ver.is_some_and(|nv| stable_ver > nv);
                let stable_newer_or_equal_time = match (stable_ts, nightly_ts) {
                    (Some(st), Some(nt)) => st >= nt,
                    _ => false,
                };
                if stable_higher_semver || stable_newer_or_equal_time {
                    Some((UpdateChannel::Stable, None, stable_rel))
                } else {
                    Some((UpdateChannel::Nightly, Some(nightly_meta), nightly_rel))
                }
            },
            (Some((_, stable_rel)), None) => Some((UpdateChannel::Stable, None, stable_rel)),
            (None, Some((_, _, nightly_meta, nightly_rel))) => Some((UpdateChannel::Nightly, Some(nightly_meta), nightly_rel)),
            (None, None) => None,
        };

        let Some((channel, maybe_meta, release)) = chosen else {
            return Ok(None);
        };

        let (download_asset, sha256_asset, checksum_txt_asset) = select_assets_for_install_kind(&release.assets, install_kind);

        let commit_short = maybe_meta
            .as_ref()
            .and_then(|m| m.commit.as_deref())
            .filter(|c| !c.is_empty())
            .map(|c| c.chars().take(7).collect::<String>());

        let display_version = match channel {
            UpdateChannel::Stable => {
                let tag = release.tag_name.trim();
                if tag.starts_with('v') || tag.starts_with('V') { tag.to_string() } else { format!("v{tag}") }
            },
            UpdateChannel::Nightly => {
                let ver = maybe_meta
                    .as_ref()
                    .and_then(|m| m.version.as_deref())
                    .map(|v| v.trim_start_matches(['v', 'V']));
                match (ver, commit_short.as_deref()) {
                    (Some(v), Some(short)) => format!("nightly (v{v} · {short})"),
                    (None, Some(short)) => format!("nightly ({short})"),
                    _ => "nightly".to_string(),
                }
            },
        };

        let html_url = release
            .html_url
            .clone()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| format!("https://github.com/tomahawk1103/manga-tui/releases/tag/{}", release.tag_name));

        let release_notes = sanitize_release_notes(release.body.as_deref().unwrap_or(""));

        Ok(Some(AvailableUpdate {
            tag_name: release.tag_name.clone(),
            display_version,
            channel,
            commit_short,
            published_at: release.published_at.clone(),
            release_notes,
            html_url,
            install_kind,
            download_asset,
            sha256_asset,
            checksum_txt_asset,
        }))
    }

    async fn fetch_expected_sha256(&self, update: &AvailableUpdate, asset_name: &str) -> Result<String, String> {
        let mut last_err: Option<String> = None;

        for candidate in [update.sha256_asset.as_ref(), update.checksum_txt_asset.as_ref()].into_iter().flatten() {
            match self.api_client.get(&candidate.download_url).send().await {
                Ok(resp) if resp.status() == StatusCode::OK => match resp.text().await {
                    Ok(body) => {
                        if let Some(hash) = parse_sha256_for_asset(&body, asset_name) {
                            return Ok(hash);
                        }
                        last_err = Some(format!(
                            "{} ({})",
                            tr(
                                "No se encontró el hash SHA-256 esperado en el archivo de verificación",
                                "Expected SHA-256 hash not found in checksum file"
                            ),
                            candidate.name
                        ));
                    },
                    Err(e) => {
                        last_err = Some(format!("{}: {e}", tr("Error leyendo archivo SHA-256", "Failed reading SHA-256 file")));
                    },
                },
                Ok(resp) => {
                    last_err = Some(format!(
                        "{} {} (HTTP {})",
                        tr("Error descargando SHA-256", "Failed downloading SHA-256"),
                        candidate.name,
                        resp.status()
                    ));
                },
                Err(e) => {
                    last_err = Some(format!("{}: {e}", tr("Error descargando SHA-256", "Failed downloading SHA-256")));
                },
            }
        }

        Err(last_err.unwrap_or_else(|| {
            tr(
                "Esta release no incluye archivo SHA-256 para verificar la descarga.",
                "This release does not include a SHA-256 checksum file to verify the download.",
            )
            .to_string()
        }))
    }

    pub async fn download_and_verify(
        &self,
        update: &AvailableUpdate,
        target_exe: PathBuf,
        on_progress: impl FnMut(UpdateProgress),
    ) -> Result<PreparedUpdate, String> {
        let update_dir = std::env::temp_dir().join("manga-tui-update");
        self.download_and_verify_in_dir(update, target_exe, &update_dir, on_progress).await
    }

    pub async fn download_and_verify_in_dir(
        &self,
        update: &AvailableUpdate,
        target_exe: PathBuf,
        update_dir: &Path,
        mut on_progress: impl FnMut(UpdateProgress),
    ) -> Result<PreparedUpdate, String> {
        let asset = update.download_asset.as_ref().ok_or_else(|| {
            tr(
                "No se encontró un paquete compatible para tu sistema en esta release.",
                "No compatible package for your system was found in this release.",
            )
            .to_string()
        })?;

        if update.sha256_asset.is_none() && update.checksum_txt_asset.is_none() {
            return Err(tr(
                "La release no incluye checksum SHA-256; descarga cancelada por seguridad.",
                "Release does not include a SHA-256 checksum; download aborted for safety.",
            )
            .to_string());
        }

        let safe_name = Path::new(&asset.name)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| "Invalid asset filename".to_string())?;

        let expected_sha256 = self.fetch_expected_sha256(update, safe_name).await?;

        fs::create_dir_all(update_dir).map_err(|e| format!("Failed to create update dir {}: {e}", update_dir.display()))?;

        let part_path = update_dir.join(format!("{safe_name}.part"));
        let final_path = update_dir.join(safe_name);
        let _ = fs::remove_file(&part_path);

        let mut response = self
            .download_client
            .get(&asset.download_url)
            .send()
            .await
            .map_err(|e| format!("{}: {e}", tr("Error descargando actualización", "Failed downloading update")))?;

        if response.status() != StatusCode::OK {
            return Err(format!(
                "{} (HTTP {})",
                tr("Error descargando paquete de actualización", "Failed downloading update package"),
                response.status()
            ));
        }

        let total_bytes = response.content_length().or((asset.size > 0).then_some(asset.size));
        let mut downloaded_bytes: u64 = 0;

        on_progress(UpdateProgress {
            stage: UpdateProgressStage::Downloading,
            downloaded_bytes: 0,
            total_bytes,
        });

        let mut hasher = Sha256::new();
        {
            let mut file = File::create(&part_path).map_err(|e| format!("Failed creating {}: {e}", part_path.display()))?;
            let mut last_reported_bytes: u64 = 0;
            let mut last_reported_at = Instant::now();

            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if let Err(e) = file.write_all(&chunk) {
                            drop(file);
                            let _ = fs::remove_file(&part_path);
                            return Err(format!("Failed writing update file: {e}"));
                        }
                        hasher.update(&chunk);
                        downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);

                        if downloaded_bytes.saturating_sub(last_reported_bytes) >= 64 * 1024
                            || last_reported_at.elapsed() >= Duration::from_millis(100)
                        {
                            last_reported_bytes = downloaded_bytes;
                            last_reported_at = Instant::now();
                            on_progress(UpdateProgress {
                                stage: UpdateProgressStage::Downloading,
                                downloaded_bytes,
                                total_bytes,
                            });
                        }
                    },
                    Ok(None) => break,
                    Err(e) => {
                        drop(file);
                        let _ = fs::remove_file(&part_path);
                        return Err(format!("{}: {e}", tr("Descarga interrumpida", "Download interrupted")));
                    },
                }
            }

            if let Err(e) = file.flush() {
                drop(file);
                let _ = fs::remove_file(&part_path);
                return Err(format!("Failed flushing update file: {e}"));
            }
        }

        on_progress(UpdateProgress {
            stage: UpdateProgressStage::Verifying,
            downloaded_bytes,
            total_bytes,
        });

        let actual_sha256: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !actual_sha256.eq_ignore_ascii_case(&expected_sha256) {
            let _ = fs::remove_file(&part_path);
            return Err(format!(
                "{} ({}: {expected_sha256}, {}: {actual_sha256})",
                tr("Fallo de verificación SHA-256", "SHA-256 verification failed"),
                tr("esperado", "expected"),
                tr("obtenido", "actual")
            ));
        }

        if final_path.exists() {
            let _ = fs::remove_file(&final_path);
        }
        fs::rename(&part_path, &final_path).map_err(|e| {
            let _ = fs::remove_file(&part_path);
            format!("Failed finalizing downloaded update {}: {e}", final_path.display())
        })?;

        on_progress(UpdateProgress {
            stage: UpdateProgressStage::Preparing,
            downloaded_bytes,
            total_bytes,
        });

        Ok(PreparedUpdate {
            available: update.clone(),
            downloaded_file: final_path,
            verified_sha256: actual_sha256,
            target_exe,
        })
    }
}

fn escape_ps_single_quoted(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

pub fn build_windows_installer_ps1(pid: u32, setup_path: &Path, install_dir: &Path, target_exe: &Path) -> String {
    let setup_escaped = escape_ps_single_quoted(setup_path);
    let dir_escaped = escape_ps_single_quoted(install_dir);
    let exe_escaped = escape_ps_single_quoted(target_exe);

    format!(
        "$ErrorActionPreference = 'SilentlyContinue'\r\n\
$oldPid = {pid}\r\n\
$setupPath = '{setup_escaped}'\r\n\
$installDir = '{dir_escaped}'\r\n\
$targetExe = '{exe_escaped}'\r\n\
\r\n\
try {{ Wait-Process -Id $oldPid -Timeout 20 -ErrorAction SilentlyContinue }} catch {{}}\r\n\
Start-Sleep -Milliseconds 300\r\n\
\r\n\
$dirArg = '/DIR=\"' + $installDir + '\"'\r\n\
$innoArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CLOSEAPPLICATIONS', $dirArg)\r\n\
$proc = Start-Process -FilePath $setupPath -ArgumentList $innoArgs -Wait -PassThru\r\n\
if ($proc -and $proc.ExitCode -eq 0) {{\r\n\
    Remove-Item -LiteralPath $setupPath -Force -ErrorAction SilentlyContinue\r\n\
    Remove-Item Env:MANGA_TUI_NO_WT_RELAUNCH -ErrorAction SilentlyContinue\r\n\
    Remove-Item Env:WT_SESSION -ErrorAction SilentlyContinue\r\n\
    if (Test-Path -LiteralPath $targetExe) {{\r\n\
        Start-Process -FilePath $targetExe -WorkingDirectory $installDir\r\n\
    }}\r\n\
}}\r\n\
Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue\r\n"
    )
}

pub fn build_windows_relaunch_ps1(pid: u32, old_exe: &Path, install_dir: &Path, target_exe: &Path) -> String {
    let old_escaped = escape_ps_single_quoted(old_exe);
    let dir_escaped = escape_ps_single_quoted(install_dir);
    let exe_escaped = escape_ps_single_quoted(target_exe);

    format!(
        "$ErrorActionPreference = 'SilentlyContinue'\r\n\
$oldPid = {pid}\r\n\
$oldExe = '{old_escaped}'\r\n\
$installDir = '{dir_escaped}'\r\n\
$targetExe = '{exe_escaped}'\r\n\
\r\n\
try {{ Wait-Process -Id $oldPid -Timeout 20 -ErrorAction SilentlyContinue }} catch {{}}\r\n\
Start-Sleep -Milliseconds 300\r\n\
if (Test-Path -LiteralPath $oldExe) {{\r\n\
    Remove-Item -LiteralPath $oldExe -Force -ErrorAction SilentlyContinue\r\n\
}}\r\n\
Remove-Item Env:MANGA_TUI_NO_WT_RELAUNCH -ErrorAction SilentlyContinue\r\n\
Remove-Item Env:WT_SESSION -ErrorAction SilentlyContinue\r\n\
if (Test-Path -LiteralPath $targetExe) {{\r\n\
    Start-Process -FilePath $targetExe -WorkingDirectory $installDir\r\n\
}}\r\n\
Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue\r\n"
    )
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os = OsString::from(path.as_os_str());
    os.push(suffix);
    PathBuf::from(os)
}

pub fn replace_windows_binary_from_zip(zip_path: &Path, target_exe: &Path) -> Result<PathBuf, String> {
    let exe_dir = target_exe
        .parent()
        .ok_or_else(|| format!("Target executable has no parent directory: {}", target_exe.display()))?;

    let zip_file = File::open(zip_path).map_err(|e| format!("Failed opening update archive {}: {e}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| format!("Invalid ZIP archive {}: {e}", zip_path.display()))?;

    let mut exe_index: Option<usize> = None;
    let mut readme_index: Option<usize> = None;
    let mut license_index: Option<usize> = None;

    for idx in 0..archive.len() {
        let entry = archive.by_index(idx).map_err(|e| format!("Failed reading ZIP entry: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let normalized = entry.name().replace('\\', "/").to_ascii_lowercase();
        if normalized == "manga-tui.exe" || normalized.ends_with("/manga-tui.exe") {
            exe_index = Some(idx);
        } else if normalized == "readme.md" || normalized.ends_with("/readme.md") {
            readme_index = Some(idx);
        } else if normalized == "license" || normalized.ends_with("/license") {
            license_index = Some(idx);
        }
    }

    let exe_idx = exe_index.ok_or_else(|| "manga-tui.exe was not found inside the portable ZIP archive".to_string())?;

    let new_exe_path = append_suffix(target_exe, ".new");
    let old_exe_path = append_suffix(target_exe, ".old");
    let _ = fs::remove_file(&new_exe_path);

    {
        let mut entry = archive.by_index(exe_idx).map_err(|e| format!("Failed extracting manga-tui.exe: {e}"))?;
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("Failed reading manga-tui.exe from ZIP: {e}"))?;
        if buf.is_empty() {
            return Err("Extracted manga-tui.exe is empty".to_string());
        }
        let mut out = File::create(&new_exe_path).map_err(|e| format!("Failed writing {}: {e}", new_exe_path.display()))?;
        out.write_all(&buf)
            .map_err(|e| format!("Failed writing {}: {e}", new_exe_path.display()))?;
        out.flush().map_err(|e| format!("Failed flushing {}: {e}", new_exe_path.display()))?;
    }

    // Best-effort update of documentation files; never touch portable.txt or data/.
    for (maybe_idx, dest_name) in [(readme_index, "README.md"), (license_index, "LICENSE")] {
        if let Some(idx) = maybe_idx
            && let Ok(mut entry) = archive.by_index(idx)
        {
            let mut bytes = Vec::new();
            if entry.read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                let _ = fs::write(exe_dir.join(dest_name), &bytes);
            }
        }
    }

    if old_exe_path.exists() {
        let _ = fs::remove_file(&old_exe_path);
    }

    if target_exe.exists()
        && let Err(e) = fs::rename(target_exe, &old_exe_path)
    {
        let _ = fs::remove_file(&new_exe_path);
        return Err(format!("Failed moving current executable to {}: {e}", old_exe_path.display()));
    }

    if let Err(e) = fs::rename(&new_exe_path, target_exe) {
        if old_exe_path.exists() {
            let _ = fs::rename(&old_exe_path, target_exe);
        }
        let _ = fs::remove_file(&new_exe_path);
        return Err(format!("Failed placing new executable at {}: {e}", target_exe.display()));
    }

    Ok(old_exe_path)
}

#[cfg(target_os = "windows")]
fn write_ps1_with_utf8_bom(path: &Path, script: &str) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(3 + script.len());
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(script.as_bytes());
    fs::write(path, bytes).map_err(|e| format!("Failed writing helper script {}: {e}", path.display()))
}

#[cfg(target_os = "windows")]
fn spawn_hidden_powershell_script(ps1_path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(ps1_path)
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|e| format!("Failed launching PowerShell updater helper: {e}"))?;

    Ok(())
}

pub fn apply_prepared_update_and_relaunch(prepared: &PreparedUpdate) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let pid = std::process::id();
        let install_dir = prepared.target_exe.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let update_dir = prepared
            .downloaded_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::env::temp_dir().join("manga-tui-update"));
        let _ = fs::create_dir_all(&update_dir);
        let ps1_path = update_dir.join(format!("manga-tui-apply-{pid}.ps1"));

        let file_name_lower = prepared
            .downloaded_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if file_name_lower.ends_with("-setup.exe") {
            let script = build_windows_installer_ps1(pid, &prepared.downloaded_file, &install_dir, &prepared.target_exe);
            write_ps1_with_utf8_bom(&ps1_path, &script)?;
            spawn_hidden_powershell_script(&ps1_path)?;
            Ok(())
        } else if file_name_lower.ends_with(".zip") {
            let old_exe = replace_windows_binary_from_zip(&prepared.downloaded_file, &prepared.target_exe)?;
            let _ = fs::remove_file(&prepared.downloaded_file);
            let script = build_windows_relaunch_ps1(pid, &old_exe, &install_dir, &prepared.target_exe);
            write_ps1_with_utf8_bom(&ps1_path, &script)?;
            spawn_hidden_powershell_script(&ps1_path)?;
            Ok(())
        } else {
            Err(format!("Unsupported Windows update artifact: {}", prepared.downloaded_file.display()))
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let extract_dir = prepared
            .downloaded_file
            .parent()
            .map(|p| p.join(format!("extract-{}", std::process::id())))
            .unwrap_or_else(|| std::env::temp_dir().join("manga-tui-update-extract"));
        let _ = fs::remove_dir_all(&extract_dir);
        fs::create_dir_all(&extract_dir).map_err(|e| format!("Failed creating extraction dir: {e}"))?;

        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&prepared.downloaded_file)
            .arg("-C")
            .arg(&extract_dir)
            .status()
            .map_err(|e| format!("Failed running tar: {e}"))?;

        if !status.success() {
            let _ = fs::remove_dir_all(&extract_dir);
            return Err(format!("tar extraction failed with exit code {status}"));
        }

        let extracted_bin = extract_dir.join("manga-tui");
        if !extracted_bin.is_file() {
            let _ = fs::remove_dir_all(&extract_dir);
            return Err("manga-tui binary not found inside extracted archive".to_string());
        }

        let new_path = append_suffix(&prepared.target_exe, ".new");
        let old_path = append_suffix(&prepared.target_exe, ".old");
        fs::copy(&extracted_bin, &new_path).map_err(|e| format!("Failed copying new binary: {e}"))?;
        let _ = fs::set_permissions(&new_path, fs::Permissions::from_mode(0o755));

        if old_path.exists() {
            let _ = fs::remove_file(&old_path);
        }
        if prepared.target_exe.exists() {
            fs::rename(&prepared.target_exe, &old_path).map_err(|e| format!("Failed backing up old binary: {e}"))?;
        }
        if let Err(e) = fs::rename(&new_path, &prepared.target_exe) {
            if old_path.exists() {
                let _ = fs::rename(&old_path, &prepared.target_exe);
            }
            return Err(format!("Failed replacing binary: {e}"));
        }

        let _ = fs::remove_dir_all(&extract_dir);
        let _ = fs::remove_file(&prepared.downloaded_file);

        Command::new(&prepared.target_exe)
            .spawn()
            .map_err(|e| format!("Failed relaunching manga-tui: {e}"))?;

        Ok(())
    }
}

pub fn cleanup_stale_old_binary() {
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = append_suffix(&current_exe, ".old");
        if old_exe.exists() {
            let _ = fs::remove_file(old_exe);
        }
    }
}

#[cfg(test)]
mod tests {
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    fn test_identity(version: &str, commit: &str, timestamp: i64) -> BuildIdentity {
        BuildIdentity {
            version: version.to_string(),
            commit: commit.to_string(),
            commit_short: commit.chars().take(7).collect(),
            timestamp,
            channel: "dev".to_string(),
        }
    }

    #[tokio::test]
    async fn check_for_update_detects_newer_stable_and_ignores_older_and_drafts() {
        let server = MockServer::start_async().await;
        let base_url: Url = server.base_url().parse().unwrap();
        let client = UpdateClient::new_no_proxy(base_url);

        server
            .mock_async(|when, then| {
                when.method(GET).path_contains("/releases");
                then.status(200).json_body(json!([
                    {
                        "tag_name": "v9.9.9",
                        "draft": true,
                        "prerelease": false,
                        "published_at": "2026-11-22T10:00:00Z",
                        "body": "draft should be ignored",
                        "html_url": "https://github.com/tomahawk1103/manga-tui/releases/tag/v9.9.9",
                        "assets": []
                    },
                    {
                        "tag_name": "v0.2.1",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-11-21T12:00:00Z",
                        "body": "Bug fixes and improvements",
                        "html_url": "https://github.com/tomahawk1103/manga-tui/releases/tag/v0.2.1",
                        "assets": [
                            {
                                "name": "manga-tui-0.2.1-x86_64-pc-windows-msvc-setup.exe",
                                "browser_download_url": "https://example.invalid/setup.exe",
                                "size": 6600000
                            },
                            {
                                "name": "manga-tui-0.2.1-x86_64-pc-windows-msvc-setup.exe.sha256",
                                "browser_download_url": "https://example.invalid/setup.exe.sha256",
                                "size": 120
                            },
                            {
                                "name": "manga-tui-0.2.1-x86_64-pc-windows-msvc-portable.zip",
                                "browser_download_url": "https://example.invalid/portable.zip",
                                "size": 6100000
                            }
                        ]
                    },
                    {
                        "tag_name": "v0.1.9",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-11-10T10:00:00Z",
                        "body": "Older release",
                        "assets": []
                    }
                ]));
            })
            .await;

        // Current 0.2.0 should pick v0.2.1 setup.exe for WindowsInstaller
        let update = client
            .check_for_update(&test_identity("0.2.0", "abcdef1234567", 1700000000), InstallKind::WindowsInstaller)
            .await
            .unwrap()
            .expect("should detect v0.2.1 update");

        assert_eq!(update.channel, UpdateChannel::Stable);
        assert_eq!(update.display_version, "v0.2.1");
        assert_eq!(update.release_notes, "Bug fixes and improvements");
        assert_eq!(
            update.download_asset.as_ref().map(|a| a.name.as_str()),
            Some("manga-tui-0.2.1-x86_64-pc-windows-msvc-setup.exe")
        );
        assert_eq!(
            update.sha256_asset.as_ref().map(|a| a.name.as_str()),
            Some("manga-tui-0.2.1-x86_64-pc-windows-msvc-setup.exe.sha256")
        );

        // Current 0.2.1 or 0.3.0 should report None (up to date)
        let same = client
            .check_for_update(&test_identity("0.2.1", "abcdef1234567", 1700000000), InstallKind::WindowsInstaller)
            .await
            .unwrap();
        assert_eq!(same, None);

        let newer_local = client
            .check_for_update(&test_identity("0.3.0", "abcdef1234567", 1700000000), InstallKind::WindowsInstaller)
            .await
            .unwrap();
        assert_eq!(newer_local, None);
    }

    #[tokio::test]
    async fn check_for_update_evaluates_nightly_commits_timestamps_and_stable_precedence() {
        let server = MockServer::start_async().await;
        let base_url: Url = server.base_url().parse().unwrap();
        let client = UpdateClient::new_no_proxy(base_url);

        server
            .mock_async(|when, then| {
                when.method(GET).path_contains("/releases");
                then.status(200).json_body(json!([
                    {
                        "tag_name": "nightly",
                        "draft": false,
                        "prerelease": true,
                        "published_at": "2026-11-21T14:00:00Z",
                        "body": "<!-- manga-tui-meta: commit=9999999abcdef version=0.2.0 timestamp=1795266000 -->\nLatest nightly build\n\n- Improved updater",
                        "html_url": "https://github.com/tomahawk1103/manga-tui/releases/tag/nightly",
                        "assets": [
                            {
                                "name": "manga-tui-nightly-x86_64-pc-windows-msvc-portable.zip",
                                "browser_download_url": "https://example.invalid/nightly.zip",
                                "size": 6200000
                            },
                            {
                                "name": "checksum.txt",
                                "browser_download_url": "https://example.invalid/checksum.txt",
                                "size": 256
                            }
                        ]
                    },
                    {
                        "tag_name": "v0.2.0",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-11-20T12:00:00Z",
                        "body": "Stable v0.2.0",
                        "assets": []
                    }
                ]));
            })
            .await;

        // Same version 0.2.0 with older timestamp & different commit -> update to nightly
        let nightly = client
            .check_for_update(&test_identity("0.2.0", "1111111abcdef", 1795000000), InstallKind::WindowsPortable)
            .await
            .unwrap()
            .expect("nightly should be detected");
        assert_eq!(nightly.channel, UpdateChannel::Nightly);
        assert_eq!(nightly.display_version, "nightly (v0.2.0 · 9999999)");
        assert_eq!(nightly.commit_short.as_deref(), Some("9999999"));
        assert_eq!(nightly.release_notes, "Latest nightly build\n\n- Improved updater");
        assert_eq!(nightly.checksum_txt_asset.as_ref().map(|a| a.name.as_str()), Some("checksum.txt"));

        // Matching commit SHA -> no update even if timestamp differs
        let same_commit = client
            .check_for_update(&test_identity("0.2.0", "9999999abcdef", 1795000000), InstallKind::WindowsPortable)
            .await
            .unwrap();
        assert_eq!(same_commit, None);

        // Newer local commit timestamp -> no downgrade
        let newer_local_ts = client
            .check_for_update(&test_identity("0.2.0", "aaaaaaa123456", 1795999999), InstallKind::WindowsPortable)
            .await
            .unwrap();
        assert_eq!(newer_local_ts, None);

        // Local version 0.2.1 > nightly 0.2.0 and newer timestamp -> no downgrade
        let newer_local_ver = client
            .check_for_update(&test_identity("0.2.1", "aaaaaaa123456", 1795999999), InstallKind::WindowsPortable)
            .await
            .unwrap();
        assert_eq!(newer_local_ver, None);
    }

    #[tokio::test]
    async fn check_for_update_prefers_stable_when_stable_is_newer_or_higher_semver() {
        let server = MockServer::start_async().await;
        let base_url: Url = server.base_url().parse().unwrap();
        let client = UpdateClient::new_no_proxy(base_url);

        server
            .mock_async(|when, then| {
                when.method(GET).path_contains("/releases");
                then.status(200).json_body(json!([
                    {
                        "tag_name": "nightly",
                        "draft": false,
                        "prerelease": true,
                        "published_at": "2026-11-22T12:00:00Z",
                        "body": "<!-- manga-tui-meta: commit=bbbbbbb123456 version=0.2.0 timestamp=1795348800 -->\nNightly",
                        "assets": []
                    },
                    {
                        "tag_name": "v0.3.0",
                        "draft": false,
                        "prerelease": false,
                        "published_at": "2026-11-21T12:00:00Z",
                        "body": "Stable v0.3.0",
                        "assets": []
                    }
                ]));
            })
            .await;

        let chosen = client
            .check_for_update(&test_identity("0.2.0", "1111111abcdef", 1795000000), InstallKind::WindowsInstaller)
            .await
            .unwrap()
            .expect("should pick update");
        assert_eq!(chosen.channel, UpdateChannel::Stable);
        assert_eq!(chosen.display_version, "v0.3.0");
    }

    #[test]
    fn parses_nightly_meta_sanitizes_notes_and_parses_sha256() {
        let body =
            "<!-- manga-tui-meta: timestamp=1700000123 version=0.2.0 commit=deadbeef123456 -->\n\nFirst line\n\n\nSecond line\n";
        let meta = parse_nightly_meta(body);
        assert_eq!(meta.commit.as_deref(), Some("deadbeef123456"));
        assert_eq!(meta.version.as_deref(), Some("0.2.0"));
        assert_eq!(meta.timestamp, Some(1700000123));

        assert_eq!(sanitize_release_notes(body), "First line\n\nSecond line");

        let sha_a = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let sha_b = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let checksum_txt = format!("{sha_a}  manga-tui-0.2.0-portable.zip\n{sha_b} *releases/manga-tui-0.2.0-setup.exe\n");
        assert_eq!(
            parse_sha256_for_asset(&checksum_txt, "manga-tui-0.2.0-portable.zip").as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(parse_sha256_for_asset(&checksum_txt, "manga-tui-0.2.0-setup.exe").as_deref(), Some(sha_b));
        assert_eq!(parse_sha256_for_asset(sha_b, "any-name.exe").as_deref(), Some(sha_b));
        assert_eq!(parse_sha256_for_asset("not-a-hash  foo.zip", "foo.zip"), None);
    }

    #[test]
    fn detect_windows_install_kind_distinguishes_installer_portable_and_standalone() {
        let base = PathBuf::from("./test_results/install_kind_detection_test");
        let _ = fs::remove_dir_all(&base);

        let installer_dir = base.join("installer");
        let portable_marker_dir = base.join("portable_marker");
        let portable_data_dir = base.join("portable_data");
        let standalone_dir = base.join("standalone");

        fs::create_dir_all(&installer_dir).unwrap();
        fs::create_dir_all(&portable_marker_dir).unwrap();
        fs::create_dir_all(portable_data_dir.join("data")).unwrap();
        fs::create_dir_all(&standalone_dir).unwrap();

        fs::write(installer_dir.join("unins000.exe"), b"uninstaller").unwrap();
        fs::write(portable_marker_dir.join("portable.txt"), b"portable").unwrap();

        assert_eq!(detect_windows_install_kind(&installer_dir.join("manga-tui.exe")), InstallKind::WindowsInstaller);
        assert_eq!(detect_windows_install_kind(&portable_marker_dir.join("manga-tui.exe")), InstallKind::WindowsPortable);
        assert_eq!(detect_windows_install_kind(&portable_data_dir.join("manga-tui.exe")), InstallKind::WindowsPortable);
        assert_eq!(detect_windows_install_kind(&standalone_dir.join("manga-tui.exe")), InstallKind::WindowsStandalone);

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn download_and_verify_handles_valid_and_corrupted_sha256() {
        let server = MockServer::start_async().await;
        let base_url: Url = server.base_url().parse().unwrap();
        let client = UpdateClient::new_no_proxy(base_url.clone());

        let payload = b"fake-manga-tui-installer-binary-payload-1234567890";
        let expected_sha: String = Sha256::digest(payload).iter().map(|b| format!("{b:02x}")).collect();
        let wrong_sha = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

        server
            .mock_async(|when, then| {
                when.method(GET).path("/assets/setup.exe");
                then.status(200).body(payload);
            })
            .await;

        server
            .mock_async(|when, then| {
                when.method(GET).path("/assets/setup.exe.sha256");
                then.status(200).body(format!("{expected_sha}  setup.exe\n"));
            })
            .await;

        server
            .mock_async(|when, then| {
                when.method(GET).path("/assets/bad.sha256");
                then.status(200).body(format!("{wrong_sha}  setup.exe\n"));
            })
            .await;

        let temp_dir = PathBuf::from("./test_results/updater_download_verify_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let valid_update = AvailableUpdate {
            tag_name: "v0.2.1".into(),
            display_version: "v0.2.1".into(),
            channel: UpdateChannel::Stable,
            commit_short: None,
            published_at: None,
            release_notes: String::new(),
            html_url: "https://example.invalid".into(),
            install_kind: InstallKind::WindowsInstaller,
            download_asset: Some(ReleaseAsset {
                name: "setup.exe".into(),
                download_url: format!("{base_url}assets/setup.exe"),
                size: payload.len() as u64,
            }),
            sha256_asset: Some(ReleaseAsset {
                name: "setup.exe.sha256".into(),
                download_url: format!("{base_url}assets/setup.exe.sha256"),
                size: 80,
            }),
            checksum_txt_asset: None,
        };

        let mut stages = Vec::new();
        let prepared = client
            .download_and_verify_in_dir(&valid_update, temp_dir.join("target/manga-tui.exe"), &temp_dir, |p| {
                stages.push(p.stage);
            })
            .await
            .expect("valid download should succeed");

        assert_eq!(prepared.verified_sha256, expected_sha);
        assert!(prepared.downloaded_file.is_file());
        assert!(stages.contains(&UpdateProgressStage::Downloading));
        assert!(stages.contains(&UpdateProgressStage::Verifying));
        assert!(stages.contains(&UpdateProgressStage::Preparing));

        // Corrupted checksum must fail and clean up .part file
        let mut bad_update = valid_update.clone();
        bad_update.sha256_asset = Some(ReleaseAsset {
            name: "bad.sha256".into(),
            download_url: format!("{base_url}assets/bad.sha256"),
            size: 80,
        });
        let err = client
            .download_and_verify_in_dir(&bad_update, temp_dir.join("target/manga-tui.exe"), &temp_dir, |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("SHA-256"));
        assert!(!temp_dir.join("setup.exe.part").exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn replace_windows_binary_from_zip_preserves_portable_data_and_replaces_exe() {
        let base = PathBuf::from("./test_results/replace_zip_test");
        let _ = fs::remove_dir_all(&base);
        let install_dir = base.join("portable_app");
        let data_dir = install_dir.join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let target_exe = install_dir.join("manga-tui.exe");
        let portable_marker = install_dir.join("portable.txt");
        let config_file = data_dir.join("config.toml");

        fs::write(&target_exe, b"old-exe-bytes").unwrap();
        fs::write(&portable_marker, b"keep-me").unwrap();
        fs::write(&config_file, b"ui_language = \"es\"\n").unwrap();

        let zip_path = base.join("portable.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip_writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip_writer.start_file("manga-tui.exe", options).unwrap();
            zip_writer.write_all(b"new-exe-bytes-v0.2.1").unwrap();
            zip_writer.start_file("portable.txt", options).unwrap();
            zip_writer.write_all(b"should-not-overwrite-marker").unwrap();
            zip_writer.start_file("README.md", options).unwrap();
            zip_writer.write_all(b"Updated README").unwrap();
            zip_writer.finish().unwrap();
        }

        let old_exe = replace_windows_binary_from_zip(&zip_path, &target_exe).expect("zip replacement should succeed");
        assert_eq!(fs::read(&target_exe).unwrap(), b"new-exe-bytes-v0.2.1");
        assert_eq!(fs::read(&old_exe).unwrap(), b"old-exe-bytes");
        assert_eq!(fs::read(&portable_marker).unwrap(), b"keep-me");
        assert_eq!(fs::read(&config_file).unwrap(), b"ui_language = \"es\"\n");
        assert_eq!(fs::read_to_string(install_dir.join("README.md")).unwrap(), "Updated README");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn windows_helper_scripts_escape_paths_quote_dir_and_clear_wt_env() {
        let install_dir = PathBuf::from(r"D:\Álvaro's Apps\Manga TUI");
        let target_exe = PathBuf::from(r"D:\Álvaro's Apps\Manga TUI\manga-tui.exe");
        let setup_path = PathBuf::from(r"C:\Temp\manga-tui-setup.exe");
        let old_exe = PathBuf::from(r"D:\Álvaro's Apps\Manga TUI\manga-tui.exe.old");

        let installer_ps1 = build_windows_installer_ps1(4242, &setup_path, &install_dir, &target_exe);
        assert!(installer_ps1.contains("$oldPid = 4242"));
        assert!(installer_ps1.contains(r"$installDir = 'D:\Álvaro''s Apps\Manga TUI'"));
        assert!(installer_ps1.contains(r#"$dirArg = '/DIR="' + $installDir + '"'"#));
        assert!(installer_ps1.contains("'/VERYSILENT'"));
        assert!(installer_ps1.contains("'/CLOSEAPPLICATIONS'"));
        assert!(installer_ps1.contains("Remove-Item Env:MANGA_TUI_NO_WT_RELAUNCH"));
        assert!(installer_ps1.contains("Remove-Item Env:WT_SESSION"));

        let relaunch_ps1 = build_windows_relaunch_ps1(999, &old_exe, &install_dir, &target_exe);
        assert!(relaunch_ps1.contains("$oldPid = 999"));
        assert!(relaunch_ps1.contains(r"$oldExe = 'D:\Álvaro''s Apps\Manga TUI\manga-tui.exe.old'"));
        assert!(relaunch_ps1.contains("Remove-Item Env:MANGA_TUI_NO_WT_RELAUNCH"));
        assert!(relaunch_ps1.contains("Remove-Item Env:WT_SESSION"));
    }
}
