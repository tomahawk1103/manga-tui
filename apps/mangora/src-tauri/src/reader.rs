use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use manga_core::backend::database::{
    Bookmark, ChapterToBookmark, ChapterToSaveHistory, Database, MangaReadingHistorySave, RetrieveBookmark,
    extract_normalized_chapter_number,
};
use manga_core::backend::manga_downloader::chapter_download_id_candidates;
use manga_core::backend::manga_provider::{ChapterPageUrl, Languages, MangaProviders};
use manga_core::backend::multi_provider::{get_chapter_pages_url_for_provider, get_raw_image_for_provider};
use manga_core::backend::offline::{LocalChapterHandle, LocalChapterSource, OfflineManga, find_offline_manga_in_dir};
use manga_core::backend::web_viewer::detect_image_mime;
use manga_core::config::MangaTuiConfig;
use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::state::AppState;

const MAX_ACTIVE_SESSIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderChapterNeighbor {
    pub id: String,
    pub title: String,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub language: String,
    pub is_downloaded: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenReaderChapterRequest {
    pub manga_id: String,
    pub provider: String,
    pub manga_title: String,
    pub cover_url: Option<String>,
    pub chapter_id: String,
    pub chapter_title: Option<String>,
    pub chapter_number: Option<String>,
    pub volume_number: Option<String>,
    pub language: Option<String>,
    pub chapters_context: Option<Vec<ReaderChapterNeighbor>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderSessionResponse {
    pub session_id: String,
    pub manga_id: String,
    pub provider: String,
    pub provider_label: String,
    pub manga_title: String,
    pub cover_url: Option<String>,
    pub chapter_id: String,
    pub chapter_title: String,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub language: String,
    pub page_count: usize,
    pub initial_page: u32,
    pub is_offline: bool,
    pub offline_format: Option<String>,
    pub prev_chapter: Option<ReaderChapterNeighbor>,
    pub next_chapter: Option<ReaderChapterNeighbor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReaderProgressPayload {
    pub manga_id: String,
    pub provider: String,
    pub manga_title: String,
    pub cover_url: Option<String>,
    pub chapter_id: String,
    pub chapter_title: String,
    pub language: String,
    pub page_index: u32,
    pub total_pages: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReaderProgressResult {
    pub page_index: u32,
    pub is_read: bool,
}

pub struct ActiveReaderSession {
    pub provider: MangaProviders,
    pub local_handle: Option<Arc<LocalChapterHandle>>,
    pub remote_urls: Vec<ChapterPageUrl>,
    pub page_count: usize,
}

#[derive(Default)]
struct ReaderStoreInner {
    next_seq: u64,
    order: VecDeque<String>,
    sessions: HashMap<String, Arc<ActiveReaderSession>>,
}

#[derive(Default)]
pub struct ReaderSessionStore {
    inner: Mutex<ReaderStoreInner>,
}

impl ReaderSessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, session: ActiveReaderSession) -> String {
        let mut guard = self.inner.lock().expect("ReaderSessionStore lock poisoned");
        guard.next_seq += 1;
        let id = format!("rs-{}", guard.next_seq);
        guard.order.push_back(id.clone());
        guard.sessions.insert(id.clone(), Arc::new(session));
        while guard.order.len() > MAX_ACTIVE_SESSIONS {
            if let Some(evicted) = guard.order.pop_front() {
                guard.sessions.remove(&evicted);
            }
        }
        id
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<ActiveReaderSession>> {
        let guard = self.inner.lock().ok()?;
        guard.sessions.get(session_id).cloned()
    }
}

fn parse_provider_for_reader(raw: &str, manga_id: &str) -> Result<MangaProviders, String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if let Ok(provider) = MangaProviders::from_str(&trimmed) {
        return Ok(provider);
    }
    if manga_id.starts_with("offline::") {
        return Ok(MangaProviders::default());
    }
    Err(format!("Unsupported manga provider: '{raw}'"))
}

fn find_matching_local_source(
    downloads_dir: &std::path::Path,
    manga_id: &str,
    manga_title: &str,
    provider: MangaProviders,
    chapter_id: &str,
    chapter_number: Option<&str>,
    offline_manga: Option<&OfflineManga>,
) -> Option<(LocalChapterSource, Option<String>)> {
    if let Some(off) = offline_manga {
        if let Some(matched) = off.chapters.iter().find(|ch| ch.chapter.id == chapter_id) {
            return Some((matched.source.clone(), Some(matched.chapter.title.clone())));
        }

        let candidates = chapter_download_id_candidates(provider, chapter_id);
        if let Some(matched) = off.chapters.iter().find(|ch| {
            candidates
                .iter()
                .any(|cand| ch.chapter.id == *cand || ch.chapter.id_safe_for_download.contains(cand.as_str()))
        }) {
            return Some((matched.source.clone(), Some(matched.chapter.title.clone())));
        }

        if let Some(target_num) = chapter_number.and_then(extract_normalized_chapter_number)
            && let Some(matched) = off
                .chapters
                .iter()
                .find(|ch| extract_normalized_chapter_number(&ch.chapter.chapter_number).as_deref() == Some(target_num.as_str()))
        {
            return Some((matched.source.clone(), Some(matched.chapter.title.clone())));
        }
    }

    LocalChapterSource::find_in_dir(downloads_dir, manga_title, chapter_id)
        .or_else(
            || {
                if manga_id != manga_title { LocalChapterSource::find_in_dir(downloads_dir, manga_id, chapter_id) } else { None }
            },
        )
        .map(|src| (src, None))
}

fn compare_neighbors(a: &ReaderChapterNeighbor, b: &ReaderChapterNeighbor) -> Ordering {
    let na = a.chapter_number.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    let nb = b.chapter_number.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    match (na, nb) {
        (Some(va), Some(vb)) => va.total_cmp(&vb).then_with(|| a.id.cmp(&b.id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.chapter_number.cmp(&b.chapter_number).then_with(|| a.id.cmp(&b.id)),
    }
}

fn resolve_neighbors(
    current_id: &str,
    current_number: &str,
    context: Option<Vec<ReaderChapterNeighbor>>,
    offline_manga: Option<&OfflineManga>,
) -> (Option<ReaderChapterNeighbor>, Option<ReaderChapterNeighbor>) {
    let mut list: Vec<ReaderChapterNeighbor> = match context {
        Some(items) if !items.is_empty() => items,
        _ => offline_manga
            .map(|off| {
                off.chapters
                    .iter()
                    .map(|ch| ReaderChapterNeighbor {
                        id: ch.chapter.id.clone(),
                        title: ch.chapter.title.clone(),
                        chapter_number: ch.chapter.chapter_number.clone(),
                        volume_number: ch.chapter.volume_number.clone(),
                        language: ch.chapter.language.as_iso_code().to_string(),
                        is_downloaded: true,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };

    if list.is_empty() {
        return (None, None);
    }

    list.sort_by(compare_neighbors);
    list.dedup_by(|a, b| a.id == b.id);

    let current_norm = extract_normalized_chapter_number(current_number);
    let idx = list.iter().position(|c| c.id == current_id).or_else(|| {
        current_norm.as_deref().and_then(|norm| {
            list.iter()
                .position(|c| extract_normalized_chapter_number(&c.chapter_number).as_deref() == Some(norm))
        })
    });

    match idx {
        Some(pos) => {
            let prev = if pos > 0 { Some(list[pos - 1].clone()) } else { None };
            let next = list.get(pos + 1).cloned();
            (prev, next)
        },
        None => (None, None),
    }
}

pub async fn resolve_and_open_chapter(
    state: &AppState,
    reader_store: &ReaderSessionStore,
    request: OpenReaderChapterRequest,
) -> Result<ReaderSessionResponse, String> {
    let manga_id = request.manga_id.trim().to_string();
    let chapter_id = request.chapter_id.trim().to_string();
    if manga_id.is_empty() || chapter_id.is_empty() {
        return Err("Missing manga or chapter identifier".to_string());
    }

    let provider = parse_provider_for_reader(&request.provider, &manga_id)?;
    let manga_title = if request.manga_title.trim().is_empty() { manga_id.clone() } else { request.manga_title.trim().to_string() };
    let downloads_dir = state.downloads_dir()?;
    let default_lang = state.default_chapter_language().unwrap_or_default();

    let (offline_manga, local_source_opt) = {
        let conn = state.open_db_connection()?;
        let db = Database::new(&conn);
        let offline = find_offline_manga_in_dir(&downloads_dir, &manga_id, &manga_title, Some(provider), Some(&db));
        let source_opt = find_matching_local_source(
            &downloads_dir,
            &manga_id,
            &manga_title,
            provider,
            &chapter_id,
            request.chapter_number.as_deref(),
            offline.as_ref(),
        );
        (offline, source_opt)
    };

    let mut chapter_number = request
        .chapter_number
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("?")
        .to_string();
    let mut chapter_title = request
        .chapter_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();

    let mut local_handle_opt: Option<Arc<LocalChapterHandle>> = None;
    let mut offline_format: Option<String> = None;

    if let Some((src, offline_title)) = local_source_opt {
        let opened = tokio::task::spawn_blocking(move || LocalChapterHandle::open(&src).map_err(|e| e.to_string()))
            .await
            .map_err(|e| e.to_string())?;
        if let Ok(handle) = opened
            && handle.page_count() > 0
        {
            offline_format = Some(handle.format_label().to_string());
            if chapter_title.is_empty()
                && let Some(t) = offline_title
            {
                chapter_title = t;
            }
            local_handle_opt = Some(Arc::new(handle));
        }
    }

    let (active_session, page_count, is_offline) = if let Some(handle) = local_handle_opt {
        let count = handle.page_count();
        (
            ActiveReaderSession {
                provider,
                local_handle: Some(handle),
                remote_urls: Vec::new(),
                page_count: count,
            },
            count,
            true,
        )
    } else {
        if manga_id.starts_with("offline::") {
            return Err("No pages found in the local archive for this chapter".to_string());
        }
        let quality = MangaTuiConfig::get().effective().image_quality;
        let urls = tokio::time::timeout(
            Duration::from_secs(20),
            get_chapter_pages_url_for_provider(provider, &chapter_id, &manga_id, quality),
        )
        .await
        .map_err(|_| "Timed out while loading chapter pages from provider".to_string())??;

        if urls.is_empty() {
            return Err("This chapter returned 0 pages from the selected source".to_string());
        }

        let count = urls.len();
        (
            ActiveReaderSession {
                provider,
                local_handle: None,
                remote_urls: urls,
                page_count: count,
            },
            count,
            false,
        )
    };

    if chapter_number == "?" {
        if let Some(off_ch) = offline_manga
            .as_ref()
            .and_then(|m| m.chapters.iter().find(|c| c.chapter.id == chapter_id))
        {
            chapter_number = off_ch.chapter.chapter_number.clone();
        }
    }
    if chapter_title.is_empty() {
        chapter_title = if chapter_number == "?" { "Capítulo".to_string() } else { format!("Cap. {chapter_number}") };
    }

    let language_code = request
        .language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| default_lang.as_iso_code().to_string());

    let cover_url = request
        .cover_url
        .as_deref()
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_owned);

    let initial_page = {
        let conn = state.open_db_connection()?;
        let initial = {
            let db = Database::new(&conn);
            let bookmarked = db
                .get_bookmarked(&manga_id)
                .ok()
                .flatten()
                .filter(|b| b.id == chapter_id)
                .and_then(|b| b.number_page_bookmarked)
                .unwrap_or(0);

            let _ = db.save_history(MangaReadingHistorySave {
                id: &manga_id,
                title: &manga_title,
                img_url: cover_url.as_deref(),
                chapter: ChapterToSaveHistory {
                    id: &chapter_id,
                    title: &chapter_title,
                    translated_language: &language_code,
                },
                provider,
            });
            bookmarked
        };
        initial.min(page_count.saturating_sub(1) as u32)
    };

    let (prev_chapter, next_chapter) =
        resolve_neighbors(&chapter_id, &chapter_number, request.chapters_context, offline_manga.as_ref());

    let session_id = reader_store.insert(active_session);

    Ok(ReaderSessionResponse {
        session_id,
        manga_id,
        provider: provider.to_string(),
        provider_label: if is_offline && request.manga_id.starts_with("offline::") {
            "Local".to_string()
        } else {
            provider.badge_label().to_string()
        },
        manga_title,
        cover_url,
        chapter_id,
        chapter_title,
        chapter_number,
        volume_number: request.volume_number,
        language: language_code,
        page_count,
        initial_page,
        is_offline,
        offline_format,
        prev_chapter,
        next_chapter,
    })
}

pub fn save_progress_in_db(state: &AppState, payload: SaveReaderProgressPayload) -> Result<SaveReaderProgressResult, String> {
    let manga_id = payload.manga_id.trim();
    let chapter_id = payload.chapter_id.trim();
    if manga_id.is_empty() || chapter_id.is_empty() {
        return Err("Missing manga or chapter identifier".to_string());
    }

    let provider = parse_provider_for_reader(&payload.provider, manga_id)?;
    let manga_title = if payload.manga_title.trim().is_empty() { manga_id } else { payload.manga_title.trim() };
    let chapter_title = if payload.chapter_title.trim().is_empty() { "Capítulo" } else { payload.chapter_title.trim() };
    let cover_url = payload
        .cover_url
        .as_deref()
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
    let language_code = if payload.language.trim().is_empty() { "es" } else { payload.language.trim() };
    let translated_language = Languages::from_str_lenient(language_code).unwrap_or_default();

    let clamped_page = match payload.total_pages {
        Some(total) if total > 0 => payload.page_index.min(total - 1),
        _ => payload.page_index,
    };

    let conn = state.open_db_connection()?;
    {
        let mut db = Database::new(&conn);
        db.bookmark(ChapterToBookmark {
            chapter_id,
            manga_id,
            chapter_title,
            manga_title,
            manga_cover_url: cover_url,
            translated_language,
            page_number: Some(clamped_page),
            provider,
        })
        .map_err(|e| format!("Could not save reader bookmark: {e}"))?;
    }

    Ok(SaveReaderProgressResult {
        page_index: clamped_page,
        is_read: true,
    })
}

fn percent_decode_path(raw: &str) -> String {
    let src = raw.as_bytes();
    let mut bytes = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'%'
            && i + 2 < src.len()
            && let Ok(hex_str) = std::str::from_utf8(&src[i + 1..i + 3])
            && let Ok(val) = u8::from_str_radix(hex_str, 16)
        {
            bytes.push(val);
            i += 3;
            continue;
        }
        bytes.push(src[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn parse_page_route(raw_path: &str) -> Option<(String, usize)> {
    let decoded = percent_decode_path(raw_path);
    let trimmed = decoded.trim_matches('/');
    let mut parts = trimmed.split('/');
    let prefix = parts.next()?;
    if prefix != "page" {
        return None;
    }
    let session_id = parts.next()?.trim();
    let index_str = parts.next()?.trim();
    if session_id.is_empty() || parts.next().is_some() {
        return None;
    }
    let index = index_str.parse::<usize>().ok()?;
    Some((session_id.to_string(), index))
}

async fn load_session_page_bytes(
    session: Arc<ActiveReaderSession>,
    index: usize,
) -> Result<(Vec<u8>, &'static str), (tauri::http::StatusCode, String)> {
    if index >= session.page_count {
        return Err((tauri::http::StatusCode::NOT_FOUND, format!("Page {index} out of bounds")));
    }

    if let Some(handle) = session.local_handle.clone() {
        let bytes = tokio::task::spawn_blocking(move || handle.load_page_bytes(index).map_err(|e| e.to_string()))
            .await
            .map_err(|e| (tauri::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map_err(|e| (tauri::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let mime = detect_image_mime(&bytes, "");
        return Ok((bytes, mime));
    }

    let page_url = session
        .remote_urls
        .get(index)
        .ok_or_else(|| (tauri::http::StatusCode::NOT_FOUND, format!("Remote URL missing for page {index}")))?;
    let url_str = page_url.url.as_str().to_string();

    let mut last_err = String::new();
    for attempt in 0..3u8 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
        }
        let res = get_raw_image_for_provider(session.provider, &url_str).await.map_err(|e| e.to_string());
        match res {
            Ok(bytes) if !bytes.is_empty() => {
                let raw = bytes.to_vec();
                let mime = detect_image_mime(&raw, &url_str);
                return Ok((raw, mime));
            },
            Ok(_) => {
                last_err = "Empty image response".to_string();
            },
            Err(err_msg) => {
                last_err = err_msg;
            },
        }
    }

    Err((tauri::http::StatusCode::BAD_GATEWAY, format!("Failed to fetch page {index}: {last_err}")))
}

fn plain_error_response(status: tauri::http::StatusCode, message: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header(tauri::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

pub fn handle_mangora_protocol_request<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let raw_path = request.uri().path().to_string();
    let Some((session_id, page_index)) = parse_page_route(&raw_path) else {
        responder.respond(plain_error_response(tauri::http::StatusCode::NOT_FOUND, "Invalid mangora page URI"));
        return;
    };

    let app_handle = ctx.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let session = {
            let store = app_handle.state::<ReaderSessionStore>();
            store.get(&session_id)
        };
        let Some(session) = session else {
            responder.respond(plain_error_response(tauri::http::StatusCode::NOT_FOUND, "Reader session expired"));
            return;
        };

        match load_session_page_bytes(session, page_index).await {
            Ok((bytes, mime)) => {
                let response = tauri::http::Response::builder()
                    .status(tauri::http::StatusCode::OK)
                    .header(tauri::http::header::CONTENT_TYPE, mime)
                    .header(tauri::http::header::CACHE_CONTROL, "public, max-age=3600")
                    .header(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .body(bytes)
                    .unwrap_or_else(|_| plain_error_response(tauri::http::StatusCode::INTERNAL_SERVER_ERROR, "Build error"));
                responder.respond(response);
            },
            Err((status, message)) => {
                responder.respond(plain_error_response(status, &message));
            },
        }
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use manga_core::backend::StorageMode;
    use manga_core::backend::manga_downloader::cbz_downloader::CbzDownloader;
    use manga_core::backend::manga_downloader::{ChapterToDownloadSanitized, MangaDownloader};
    use manga_core::backend::manga_provider::ChapterPage;
    use manga_core::config::{DownloadType, UiLanguage, build_config_file_in_dir};
    use pretty_assertions::assert_eq;

    use super::*;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = std::env::temp_dir().join(format!("mangora_reader_{name}_{nanos}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn make_state(base: &std::path::Path) -> AppState {
        let data_dir = base.join("data");
        let config_dir = base.join("config");
        let downloads_dir = data_dir.join("mangaDownloads");
        let db_path = data_dir.join("history").join("manga-tui-history.db");

        fs::create_dir_all(&downloads_dir).unwrap();
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        build_config_file_in_dir(&config_dir).unwrap();

        AppState::from_paths(
            data_dir,
            config_dir,
            downloads_dir,
            db_path,
            StorageMode::EnvOverride,
            UiLanguage::Es,
            MangaProviders::Inmanga,
            Languages::Spanish,
        )
    }

    fn fake_png_bytes() -> Vec<u8> {
        vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R']
    }

    fn fake_jpg_bytes() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01]
    }

    #[test]
    fn parses_plain_and_percent_encoded_mangora_page_routes() {
        assert_eq!(parse_page_route("/page/rs-1/0"), Some(("rs-1".to_string(), 0)));
        assert_eq!(parse_page_route("/page%2Frs-12%2F4"), Some(("rs-12".to_string(), 4)));
        assert_eq!(parse_page_route("page/rs-2/19/"), Some(("rs-2".to_string(), 19)));
        assert_eq!(parse_page_route("/other/rs-1/0"), None);
        assert_eq!(parse_page_route("/page/rs-1/notanumber"), None);
        assert_eq!(parse_page_route("/page//0"), None);
    }

    #[tokio::test]
    async fn opens_offline_cbz_and_raw_chapters_serves_bytes_and_saves_bookmark_progress() {
        let temp = TempTestDir::new("cbz_and_raw");
        let state = make_state(&temp.path);
        let downloads_dir = state.downloads_dir().unwrap();

        // Series folder with Ch 1 (CBZ via CbzDownloader) and Ch 2 (RAW)
        CbzDownloader::new()
            .save_chapter_in_file_system(&downloads_dir, ChapterToDownloadSanitized {
                chapter_id: "ch-dd-1".to_string(),
                manga_id: "m-dd".to_string(),
                manga_title: "Dandadan".into(),
                chapter_title: "Intro".into(),
                chapter_number: "1".to_string(),
                volume_number: None,
                language: Languages::Spanish,
                scanlator: "Scan".into(),
                download_type: DownloadType::Cbz,
                pages: vec![
                    ChapterPage {
                        bytes: fake_png_bytes().into(),
                        extension: "png".to_string(),
                    },
                    ChapterPage {
                        bytes: fake_jpg_bytes().into(),
                        extension: "jpg".to_string(),
                    },
                ],
                provider: Some(MangaProviders::Inmanga),
            })
            .unwrap();

        let series_dir = downloads_dir.join("Dandadan");
        let raw_dir = series_dir.join("Ch 2 Vol none Segundo  ch-dd-2");
        fs::create_dir_all(&raw_dir).unwrap();
        fs::write(raw_dir.join("001.jpg"), fake_jpg_bytes()).unwrap();

        let store = ReaderSessionStore::new();
        let opened = resolve_and_open_chapter(&state, &store, OpenReaderChapterRequest {
            manga_id: "offline::Dandadan".to_string(),
            provider: "inmanga".to_string(),
            manga_title: "Dandadan".to_string(),
            cover_url: None,
            chapter_id: "offline::Dandadan::Ch 1 Vol none Intro Scan ch-dd-1.cbz".to_string(),
            chapter_title: None,
            chapter_number: Some("1".to_string()),
            volume_number: None,
            language: Some("es".to_string()),
            chapters_context: None,
        })
        .await
        .unwrap();

        assert!(opened.is_offline);
        assert_eq!(opened.offline_format.as_deref(), Some("CBZ"));
        assert_eq!(opened.page_count, 2);
        assert_eq!(opened.initial_page, 0);
        assert!(opened.prev_chapter.is_none());
        assert_eq!(opened.next_chapter.as_ref().map(|c| c.chapter_number.as_str()), Some("2"));

        let session = store.get(&opened.session_id).unwrap();
        let (p0, mime0) = load_session_page_bytes(Arc::clone(&session), 0).await.unwrap();
        assert_eq!(mime0, "image/png");
        assert_eq!(p0, fake_png_bytes());

        let (p1, mime1) = load_session_page_bytes(Arc::clone(&session), 1).await.unwrap();
        assert_eq!(mime1, "image/jpeg");
        assert_eq!(p1, fake_jpg_bytes());

        // Save bookmark progress at page 1
        let saved = save_progress_in_db(&state, SaveReaderProgressPayload {
            manga_id: opened.manga_id.clone(),
            provider: opened.provider.clone(),
            manga_title: opened.manga_title.clone(),
            cover_url: None,
            chapter_id: opened.chapter_id.clone(),
            chapter_title: opened.chapter_title.clone(),
            language: opened.language.clone(),
            page_index: 1,
            total_pages: Some(2),
        })
        .unwrap();
        assert_eq!(saved.page_index, 1);

        // Re-opening restores initial_page == 1 from SQLite bookmark
        let reopened = resolve_and_open_chapter(&state, &store, OpenReaderChapterRequest {
            manga_id: opened.manga_id.clone(),
            provider: "inmanga".to_string(),
            manga_title: "Dandadan".to_string(),
            cover_url: None,
            chapter_id: opened.chapter_id.clone(),
            chapter_title: None,
            chapter_number: Some("1".to_string()),
            volume_number: None,
            language: Some("es".to_string()),
            chapters_context: None,
        })
        .await
        .unwrap();
        assert_eq!(reopened.initial_page, 1);

        // Next chapter (RAW folder) also opens offline
        let next_opened = resolve_and_open_chapter(&state, &store, OpenReaderChapterRequest {
            manga_id: opened.manga_id.clone(),
            provider: "inmanga".to_string(),
            manga_title: "Dandadan".to_string(),
            cover_url: None,
            chapter_id: opened.next_chapter.as_ref().unwrap().id.clone(),
            chapter_title: None,
            chapter_number: Some("2".to_string()),
            volume_number: None,
            language: Some("es".to_string()),
            chapters_context: None,
        })
        .await
        .unwrap();
        assert!(next_opened.is_offline);
        assert_eq!(next_opened.offline_format.as_deref(), Some("RAW"));
        assert_eq!(next_opened.page_count, 1);
    }
}
