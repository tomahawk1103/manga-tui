use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};

use crate::backend::database::{Bookmark, ChapterToBookmark, ChapterToSaveHistory, Database, MangaReadingHistorySave};
use crate::backend::error_log::{ErrorType, write_to_error_log};
use crate::backend::manga_provider::{ChapterReader, ChapterToRead, ListOfChapters, MangaProviders};
use crate::backend::offline::LocalChapterSource;
use crate::i18n::is_es;

/// Represents the active manga chapter session served by the local web viewer.
#[derive(Clone, Debug, Default)]
pub struct WebViewerSession {
    pub manga_id: String,
    pub manga_title: String,
    pub manga_cover_url: Option<String>,
    pub provider: MangaProviders,
    pub chapter: ChapterToRead,
    pub list_of_chapters: ListOfChapters,
    pub local_source: Option<LocalChapterSource>,
    pub offline_sources: HashMap<String, LocalChapterSource>,
    pub is_offline_mode: bool,
}

struct ServerInstance {
    port: u16,
    state: Arc<RwLock<WebViewerSession>>,
}

static WEB_VIEWER_SERVER: LazyLock<Mutex<Option<ServerInstance>>> = LazyLock::new(|| Mutex::new(None));

/// Starts (or updates) the local web viewer server with `session` and returns its local URL (`http://127.0.0.1:<port>/`).
pub async fn start_or_update_web_viewer(session: WebViewerSession) -> Result<String, String> {
    let mut guard = WEB_VIEWER_SERVER.lock().await;

    if let Some(instance) = guard.as_ref() {
        let mut state = instance.state.write().await;
        *state = session;
        return Ok(format!("http://127.0.0.1:{}/", instance.port));
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Could not bind local web viewer server: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Could not determine local web viewer port: {e}"))?
        .port();

    let state = Arc::new(RwLock::new(session));
    let server_state = Arc::clone(&state);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let conn_state = Arc::clone(&server_state);
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, conn_state).await {
                            write_to_error_log(ErrorType::String(&format!("Web viewer connection error: {e}")));
                        }
                    });
                },
                Err(e) => {
                    write_to_error_log(ErrorType::String(&format!("Web viewer accept error: {e}")));
                    tokio::time::sleep(Duration::from_millis(100)).await;
                },
            }
        }
    });

    let url = format!("http://127.0.0.1:{port}/");
    *guard = Some(ServerInstance { port, state });
    Ok(url)
}

/// Starts/updates the local web viewer and opens the URL in the user's default web browser.
pub async fn open_chapter_in_browser(session: WebViewerSession) -> Result<String, String> {
    let url = start_or_update_web_viewer(session).await?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if !cfg!(any(test, feature = "test-utils")) {
        open::that(&url).map_err(|e| format!("Could not open browser for {url}: {e}"))?;
    }
    Ok(url)
}

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<RwLock<WebViewerSession>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buf = [0u8; 8192];
    let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf)).await??;
    if n == 0 {
        return Ok(());
    }

    let req_str = String::from_utf8_lossy(&buf[..n]);
    let first_line = req_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_target = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        write_simple_response(&mut stream, "405 Method Not Allowed", "text/plain", b"Method Not Allowed").await?;
        return Ok(());
    }

    let normalized_target = if raw_target.starts_with("http://") || raw_target.starts_with("https://") {
        if let Some(idx) = raw_target.find("://") {
            let after_scheme = &raw_target[idx + 3..];
            match after_scheme.find('/') {
                Some(slash_idx) => &after_scheme[slash_idx..],
                None => "/",
            }
        } else {
            raw_target
        }
    } else {
        raw_target
    };

    let (path, query) = match normalized_target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (normalized_target, ""),
    };

    match path {
        "/" | "/index.html" => {
            let session = state.read().await.clone();
            save_chapter_read_in_db(&session, &session.chapter);
            let prev_chapter = session
                .list_of_chapters
                .get_previous_chapter(session.chapter.volume_number.as_deref(), session.chapter.number);
            let next_chapter = session
                .list_of_chapters
                .get_next_chapter(session.chapter.volume_number.as_deref(), session.chapter.number);

            let html = render_viewer_html(&session, prev_chapter.as_ref(), next_chapter.as_ref());
            write_response_with_headers(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                "no-store, no-cache, must-revalidate",
                html.as_bytes(),
            )
            .await?;
        },
        "/navigate" => {
            let requested_id = query.split('&').find_map(|pair| pair.strip_prefix("id=")).map(percent_decode_query_param);
            let dir = query.split('&').find_map(|pair| pair.strip_prefix("dir=")).unwrap_or("next");

            let current_session = state.read().await.clone();
            let target_chapter = if let Some(target_id) = requested_id {
                find_chapter_reader_by_id(&current_session.list_of_chapters, &target_id)
            } else if dir == "prev" {
                current_session
                    .list_of_chapters
                    .get_previous_chapter(current_session.chapter.volume_number.as_deref(), current_session.chapter.number)
            } else {
                current_session
                    .list_of_chapters
                    .get_next_chapter(current_session.chapter.volume_number.as_deref(), current_session.chapter.number)
            };

            if let Some(target) = target_chapter {
                if let Ok(new_chapter) = resolve_chapter(&current_session, &target).await {
                    let expected_pages = if current_session.is_offline_mode { 0 } else { new_chapter.pages_url.len() };
                    let new_local_source = current_session.offline_sources.get(&new_chapter.id).cloned().or_else(|| {
                        LocalChapterSource::find_for_chapter(&current_session.manga_title, &new_chapter.id, expected_pages)
                    });
                    save_chapter_read_in_db(&current_session, &new_chapter);
                    let mut write_guard = state.write().await;
                    write_guard.chapter = new_chapter;
                    write_guard.local_source = new_local_source;
                }
            }

            let redirect =
                "HTTP/1.1 302 Found\r\nLocation: /\r\nCache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(redirect.as_bytes()).await?;
            stream.flush().await?;
        },
        "/progress" => {
            let page_idx = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("page="))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);

            {
                let mut write_guard = state.write().await;
                write_guard.chapter.num_page_bookmarked = Some(page_idx);
                save_chapter_progress_in_db(&write_guard, page_idx);
            }

            let body = format!(r#"{{"ok":true,"page":{page_idx}}}"#);
            write_simple_response(&mut stream, "200 OK", "application/json", body.as_bytes()).await?;
        },
        p if p.starts_with("/page/") => {
            let idx_str = p.trim_start_matches("/page/").trim_matches('/');
            let Ok(index) = idx_str.parse::<usize>() else {
                write_simple_response(&mut stream, "400 Bad Request", "text/plain", b"Invalid page index").await?;
                return Ok(());
            };

            let (provider, page_url_opt, local_source) = {
                let session = state.read().await;
                (session.provider, session.chapter.pages_url.get(index).cloned(), session.local_source.clone())
            };

            if let Some(local_src) = local_source {
                match tokio::task::spawn_blocking(move || local_src.load_page_bytes(index)).await {
                    Ok(Ok(bytes)) => {
                        let mime = detect_image_mime(&bytes, "");
                        write_response_with_headers(&mut stream, "200 OK", mime, "public, max-age=3600", &bytes).await?;
                        return Ok(());
                    },
                    _ => {},
                }
            }

            let Some(url) = page_url_opt else {
                write_simple_response(&mut stream, "404 Not Found", "text/plain", b"Page not found").await?;
                return Ok(());
            };

            let url_str = url.as_str();
            let mut last_err = String::new();
            for attempt in 0..3u8 {
                if attempt > 0 {
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                }
                let res = crate::backend::multi_provider::get_raw_image_for_provider(provider, url_str)
                    .await
                    .map_err(|e| e.to_string());
                match res {
                    Ok(bytes) if !bytes.is_empty() => {
                        let mime = detect_image_mime(&bytes, url_str);
                        write_response_with_headers(&mut stream, "200 OK", mime, "public, max-age=3600", &bytes).await?;
                        return Ok(());
                    },
                    Ok(_) => {
                        last_err = "Empty image response".to_string();
                    },
                    Err(err_msg) => {
                        last_err = err_msg;
                    },
                }
            }

            write_simple_response(
                &mut stream,
                "502 Bad Gateway",
                "text/plain",
                format!("Failed to load manga page {index}: {last_err}").as_bytes(),
            )
            .await?;
        },
        "/favicon.ico" => {
            write_simple_response(&mut stream, "204 No Content", "text/plain", b"").await?;
        },
        _ => {
            write_simple_response(&mut stream, "404 Not Found", "text/plain", b"Not Found").await?;
        },
    }

    Ok(())
}

async fn resolve_local_chapter(session: &WebViewerSession, target: &ChapterReader) -> Option<ChapterToRead> {
    let source = session
        .offline_sources
        .get(&target.id)
        .cloned()
        .or_else(|| LocalChapterSource::find(&session.manga_title, &target.id))?;

    let count = tokio::task::spawn_blocking(move || source.page_count()).await.ok()?;
    if count == 0 {
        return None;
    }

    let dummy_urls = (0..count)
        .filter_map(|i| reqwest::Url::parse(&format!("http://localhost/local/{i}")).ok())
        .collect();
    let volume = if target.volume.trim().is_empty() { "none".to_string() } else { target.volume.clone() };

    Some(ChapterToRead {
        id: target.id.clone(),
        title: String::new(),
        number: target.number.parse::<f64>().unwrap_or(0.0),
        volume_number: Some(volume),
        num_page_bookmarked: None,
        language: session.chapter.language,
        pages_url: dummy_urls,
    })
}

async fn resolve_chapter(session: &WebViewerSession, target: &ChapterReader) -> Result<ChapterToRead, String> {
    if session.is_offline_mode {
        return resolve_local_chapter(session, target)
            .await
            .ok_or_else(|| format!("Local chapter {} not found on disk", target.id));
    }

    let res = crate::backend::multi_provider::search_chapter_for_provider(session.provider, &target.id, &session.manga_id)
        .await
        .map_err(|e| e.to_string());
    match res {
        Ok(chap) => Ok(chap),
        Err(err_msg) => resolve_local_chapter(session, target).await.ok_or(err_msg),
    }
}

fn percent_decode_query_param(raw: &str) -> String {
    let mut bytes = Vec::with_capacity(raw.len());
    let src = raw.as_bytes();
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
        } else if src[i] == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(src[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn find_chapter_reader_by_id(list: &ListOfChapters, target_id: &str) -> Option<ChapterReader> {
    list.find_chapter_by_id(target_id)
}

fn save_chapter_read_in_db(session: &WebViewerSession, chapter: &ChapterToRead) {
    if let Ok(conn) = Database::get_connection() {
        let database = Database::new(&conn);
        let _ = database.save_history(MangaReadingHistorySave {
            id: &session.manga_id,
            title: &session.manga_title,
            img_url: session.manga_cover_url.as_deref(),
            chapter: ChapterToSaveHistory {
                id: &chapter.id,
                title: &chapter.title,
                translated_language: chapter.language.as_iso_code(),
            },
            provider: session.provider,
        });
    }
}

fn save_chapter_progress_in_db(session: &WebViewerSession, page_idx: u32) {
    save_chapter_read_in_db(session, &session.chapter);
    if let Ok(conn) = Database::get_connection() {
        let mut database = Database::new(&conn);
        let _ = database.bookmark(ChapterToBookmark {
            chapter_id: &session.chapter.id,
            manga_id: &session.manga_id,
            chapter_title: &session.chapter.title,
            manga_title: &session.manga_title,
            manga_cover_url: session.manga_cover_url.as_deref(),
            translated_language: session.chapter.language,
            page_number: Some(page_idx),
            provider: session.provider,
        });
    }
}

async fn write_simple_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    write_response_with_headers(stream, status, content_type, "no-store", body).await
}

async fn write_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    cache_control: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: {cache_control}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    if !body.is_empty() {
        stream.write_all(body).await?;
    }
    stream.flush().await
}

/// Detects the MIME type of an image from its magic bytes or URL extension.
pub fn detect_image_mime(bytes: &[u8], url: &str) -> &'static str {
    if bytes.len() >= 4 {
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return "image/jpeg";
        }
        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            return "image/png";
        }
        if bytes.starts_with(b"GIF8") {
            return "image/gif";
        }
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return "image/webp";
        }
        if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && (&bytes[8..12] == b"avif" || &bytes[8..12] == b"avis") {
            return "image/avif";
        }
    }
    let lower = url.to_lowercase();
    if lower.contains(".png") {
        "image/png"
    } else if lower.contains(".webp") {
        "image/webp"
    } else if lower.contains(".gif") {
        "image/gif"
    } else if lower.contains(".avif") {
        "image/avif"
    } else {
        "image/jpeg"
    }
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Generates the full HTML5 continuous-scroll maximum-resolution reader page.
pub fn render_viewer_html(
    session: &WebViewerSession,
    prev_chapter: Option<&ChapterReader>,
    next_chapter: Option<&ChapterReader>,
) -> String {
    let es = is_es();
    let manga_title = escape_html(&session.manga_title);
    let provider_badge = escape_html(session.provider.badge_label());
    let local_badge_html = if session.local_source.is_some() { r#"<span class="badge">💾 Local</span>"# } else { "" };
    let chapter_num = session.chapter.number;
    let chapter_title = if session.chapter.title.trim().is_empty() {
        if es { format!("Capítulo {chapter_num}") } else { format!("Chapter {chapter_num}") }
    } else {
        format!("{} {chapter_num} — {}", if es { "Cap." } else { "Ch." }, escape_html(&session.chapter.title))
    };

    let page_count = session.chapter.pages_url.len();
    let chapter_cache_buster = escape_html(&session.chapter.id);
    let initial_bookmarked_page = session.chapter.num_page_bookmarked.unwrap_or(0);

    let mut chapter_options = String::new();
    for vol in session.list_of_chapters.volumes.as_slice() {
        for ch in vol.chapters.as_slice() {
            let selected = if ch.id == session.chapter.id { " selected" } else { "" };
            let ch_id_escaped = escape_html(&ch.id);
            let ch_num_escaped = escape_html(&ch.number);
            let vol_prefix = if vol.volume.trim().is_empty() || vol.volume.eq_ignore_ascii_case("none") {
                String::new()
            } else {
                format!("Vol. {} · ", escape_html(&vol.volume))
            };
            let ch_label =
                if es { format!("{vol_prefix}Cap. {ch_num_escaped}") } else { format!("{vol_prefix}Ch. {ch_num_escaped}") };
            chapter_options.push_str(&format!(r#"<option value="{ch_id_escaped}"{selected}>{ch_label}</option>"#));
        }
    }
    let chapter_select_html = if chapter_options.is_empty() {
        String::new()
    } else {
        format!(
            r#"<select id="chapter-select" class="ctrl-select" onchange="jumpToChapter(this.value)" aria-label="{}">{chapter_options}</select>"#,
            if es { "Cambiar de capítulo" } else { "Switch chapter" }
        )
    };

    let mut page_options = String::new();
    let mut pages_html = String::new();

    for i in 0..page_count {
        let page_num = i + 1;
        let label = if es { format!("Pág. {page_num} / {page_count}") } else { format!("Page {page_num} / {page_count}") };
        page_options.push_str(&format!(r#"<option value="{i}">{label}</option>"#));

        let loading_attr = if i < 4 { "eager" } else { "lazy" };
        let retry_text = if es { "Reintentar" } else { "Retry" };
        pages_html.push_str(&format!(
            r#"<div class="page-container" id="page-{i}" data-index="{i}">
                <div class="page-skeleton" id="skel-{i}">
                    <span class="page-badge">#{page_num} / {page_count}</span>
                </div>
                <img
                    id="img-{i}"
                    class="manga-page"
                    src="/page/{i}?ch={chapter_cache_buster}"
                    alt="Page {page_num}"
                    loading="{loading_attr}"
                    decoding="async"
                    onload="onPageLoaded({i})"
                    onerror="onPageError({i})"
                />
                <div class="page-error" id="err-{i}" style="display:none;">
                    <p>⚠ #{page_num}</p>
                    <button onclick="retryPage({i})">↻ {retry_text}</button>
                </div>
            </div>"#
        ));
    }

    let prev_label = if es { "← Cap. Anterior" } else { "← Prev Chapter" };
    let next_label = if es { "Cap. Siguiente →" } else { "Next Chapter →" };
    let fit_orig_label = if es { "Máx. Res (100%)" } else { "Max Res (100%)" };
    let fit_wide_label = if es { "Ancho (1200px)" } else { "Wide (1200px)" };
    let fit_std_label = if es { "Estándar (900px)" } else { "Standard (900px)" };
    let gap_label = if es { "Sin Bordes" } else { "Seamless" };
    let fullscreen_label = if es { "Pantalla completa" } else { "Fullscreen" };
    let mode_cascade_label = if es { "📜 Cascada" } else { "📜 Cascade" };
    let mode_double_rtl_label = if es { "📖 Doble (Manga RTL)" } else { "📖 Double (Manga RTL)" };
    let mode_double_ltr_label = if es { "📖 Doble (LTR)" } else { "📖 Double (LTR)" };
    let mode_single_label = if es { "📄 Página Única" } else { "📄 Single Page" };
    let sync_badge_initial = if es {
        format!("🔖 Pág. {} guardada", initial_bookmarked_page + 1)
    } else {
        format!("🔖 Saved pg. {}", initial_bookmarked_page + 1)
    };
    let end_title = if es { format!("Has completado el {chapter_title}") } else { format!("Finished reading {chapter_title}") };
    let back_top_label = if es { "↑ Volver arriba" } else { "↑ Back to top" };
    let shortcuts_hint = if es {
        "Atajos: [j/k] Scroll • [a/d] Pág. Ant/Sig • [v] Modo Cascada/Doble/Única • [+/-] Zoom • [0] Máx. Res • [m] Bordes • [f] Pantalla Completa • [b/w] Cap. Ant/Sig"
    } else {
        "Shortcuts: [j/k] Scroll • [a/d] Prev/Next Page • [v] View Mode (Cascade/Double/Single) • [+/-] Zoom • [0] Max Res • [m] Seamless/Gaps • [f] Fullscreen • [b/w] Prev/Next Chapter"
    };

    let prev_btn_top = match prev_chapter {
        Some(prev) => format!(
            r#"<a class="nav-btn" id="btn-prev-chap" href="/navigate?dir=prev" title="{} ({})">{prev_label}</a>"#,
            prev_label,
            escape_html(&prev.number)
        ),
        None => format!(r#"<span class="nav-btn disabled">{prev_label}</span>"#),
    };

    let next_btn_top = match next_chapter {
        Some(next) => format!(
            r#"<a class="nav-btn primary" id="btn-next-chap" href="/navigate?dir=next" title="{} ({})">{next_label}</a>"#,
            next_label,
            escape_html(&next.number)
        ),
        None => format!(r#"<span class="nav-btn disabled">{next_label}</span>"#),
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{manga_title} — {chapter_title} | manga-tui Web Viewer</title>
    <style>
        :root {{
            color-scheme: dark;
            --bg-primary: #0d0e16;
            --bg-glass: rgba(24, 26, 39, 0.96);
            --text-primary: #f3f4f6;
            --text-muted: #b5b7cc;
            --accent: #c4b5fd;
            --border: #33364b;
            --hud-height: 84px;
            --reader-max-width: 100%;
            --page-gap: 0px;
        }}
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        html {{
            background-color: var(--bg-primary);
            color: var(--text-primary);
            font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            scroll-behavior: smooth;
        }}
        body {{
            background-color: var(--bg-primary);
            min-height: 100vh;
            overflow-x: hidden;
        }}
        #progress-bar-container {{
            position: fixed;
            top: 0;
            left: 0;
            width: 100%;
            height: 3px;
            background: rgba(255, 255, 255, 0.06);
            z-index: 200;
        }}
        #progress-bar {{
            height: 100%;
            width: 0%;
            background: linear-gradient(90deg, #818cf8, var(--accent));
            transition: width 0.08s ease-out;
        }}
        header.hud {{
            position: fixed;
            top: 3px;
            left: 0;
            right: 0;
            z-index: 150;
            background: var(--bg-glass);
            backdrop-filter: blur(12px);
            -webkit-backdrop-filter: blur(12px);
            border: 1px solid var(--border);
            border-radius: 0 0 14px 14px;
            max-width: 1440px;
            margin-inline: auto;
            box-shadow: 0 10px 30px #0005;
            padding: 12px clamp(10px, 2vw, 24px);
            display: flex;
            flex-wrap: wrap;
            align-items: center;
            justify-content: space-between;
            gap: 12px;
            transition: transform 0.25s ease, opacity 0.25s ease;
        }}
        header.hud.hidden:not(:has(:focus-visible)) {{
            transform: translateY(-110%);
            opacity: 0;
            pointer-events: none;
        }}
        .hud-left {{
            display: flex;
            align-items: center;
            flex-wrap: wrap;
            gap: 6px;
            min-width: 0;
            max-width: 100%;
        }}
        .badge {{
            background: #292641;
            color: var(--accent);
            border: 1px solid #51476f;
            border-radius: 6px;
            padding: 3px 8px;
            font-size: 0.78rem;
            font-weight: 600;
            white-space: nowrap;
        }}
        .manga-info {{
            display: flex;
            flex-direction: column;
            min-width: 0;
            border-inline-start: 3px solid var(--accent);
            padding-inline-start: 10px;
            margin-inline-end: 6px;
        }}
        .manga-title {{
            font-size: 1.05rem;
            font-weight: 700;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            max-width: 340px;
        }}
        .chapter-subtitle {{
            font-size: 0.8rem;
            color: var(--text-muted);
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            max-width: 340px;
        }}
        .hud-controls {{
            display: flex;
            align-items: center;
            gap: 8px;
            flex-wrap: wrap;
            min-width: 0;
        }}
        .control-group {{
            display: flex;
            align-items: center;
            flex-wrap: wrap;
            gap: 4px;
            min-width: 0;
            max-width: 100%;
            padding: 4px;
            background: #10111c;
            border-radius: 10px;
        }}
        .ctrl-btn, .nav-btn, select.ctrl-select {{
            display: inline-flex;
            align-items: center;
            min-height: 36px;
            max-width: 100%;
            background: #222437;
            color: var(--text-primary);
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 6px 11px;
            font: 500 0.82rem system-ui, sans-serif;
            font-weight: 500;
            cursor: pointer;
            text-decoration: none;
            transition: background 0.15s, border-color 0.15s;
            user-select: none;
        }}
        .ctrl-btn:hover, .nav-btn:hover, select.ctrl-select:hover {{
            background: rgba(255, 255, 255, 0.14);
            border-color: rgba(255, 255, 255, 0.25);
        }}
        .ctrl-btn.active, .nav-btn.primary {{
            background: #332b4d;
            border-color: #615684;
            color: var(--accent);
        }}
        :focus-visible {{
            outline: 2px solid var(--accent);
            outline-offset: 2px;
        }}
        @media (max-width: 600px) {{
            .manga-info {{ flex: 0 0 100%; }}
            .hud-controls {{ gap: 5px; }}
            .ctrl-btn, .nav-btn, select.ctrl-select {{ min-height: 40px; padding-inline: 7px; }}
        }}
        .nav-btn.disabled {{
            opacity: 0.38;
            cursor: not-allowed;
            pointer-events: none;
        }}
        select.ctrl-select option {{
            background: #131722;
            color: #f3f4f6;
        }}
        main.reader-stream {{
            display: flex;
            flex-direction: column;
            align-items: center;
            width: 100%;
            padding-top: calc(var(--hud-height) + 16px);
            padding-bottom: 60px;
            gap: var(--page-gap);
        }}
        body.mode-double-rtl main.reader-stream,
        body.mode-double-ltr main.reader-stream {{
            display: grid;
            grid-template-columns: repeat(2, minmax(0, 1fr));
            max-width: 98vw;
            margin: 0 auto;
            align-items: start;
            justify-items: center;
            gap: 12px 6px;
        }}
        body.mode-double-rtl main.reader-stream {{
            direction: rtl;
        }}
        body.mode-double-rtl .page-container {{
            direction: ltr;
        }}
        body.mode-double-rtl img.manga-page,
        body.mode-double-ltr img.manga-page {{
            max-width: 100%;
            max-height: 92vh;
            width: auto;
            object-fit: contain;
        }}
        body.mode-double-rtl .page-container.landscape-spread,
        body.mode-double-ltr .page-container.landscape-spread {{
            grid-column: 1 / -1;
        }}
        body.mode-single .page-container {{
            display: none;
        }}
        body.mode-single .page-container.active-single-page {{
            display: flex;
            min-height: calc(100vh - var(--hud-height) - 76px);
        }}
        body.mode-single img.manga-page {{
            max-height: 90vh;
            width: auto;
            object-fit: contain;
        }}
        .badge.sync-badge {{
            background: rgba(16, 185, 129, 0.16);
            color: #34d399;
            border-color: rgba(16, 185, 129, 0.35);
        }}
        .page-container {{
            width: 100%;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            position: relative;
            min-width: 0;
            scroll-margin-top: calc(var(--hud-height) + 8px);
        }}
        .page-skeleton {{
            width: min(900px, 92vw, 100%);
            height: 420px;
            background: linear-gradient(90deg, #121620 25%, #1a2030 50%, #121620 75%);
            background-size: 200% 100%;
            animation: shimmer 1.6s infinite;
            display: flex;
            align-items: center;
            justify-content: center;
            border-radius: 4px;
            margin: 4px 0;
        }}
        .page-badge {{
            color: var(--text-muted);
            font-size: 0.9rem;
            font-weight: 600;
        }}
        @keyframes shimmer {{
            0% {{ background-position: 200% 0; }}
            100% {{ background-position: -200% 0; }}
        }}
        img.manga-page {{
            display: block;
            width: auto;
            max-width: var(--reader-max-width);
            height: auto;
            image-rendering: high-quality;
            margin: 0 auto;
            user-select: none;
        }}
        .page-error {{
            padding: 32px;
            background: rgba(239, 68, 68, 0.12);
            border: 1px solid rgba(239, 68, 68, 0.4);
            border-radius: 8px;
            text-align: center;
            margin: 12px 0;
        }}
        .page-error button {{
            margin-top: 10px;
            padding: 6px 14px;
            border-radius: 6px;
            border: 1px solid #ef4444;
            background: #ef4444;
            color: #fff;
            cursor: pointer;
            font-weight: 600;
        }}
        footer.chapter-footer {{
            width: min(760px, calc(100% - 24px));
            margin: 40px auto 60px;
            padding: 28px 24px;
            background: var(--bg-glass);
            border: 1px solid var(--border);
            border-radius: 12px;
            text-align: center;
        }}
        footer.chapter-footer h2 {{
            font-size: 1.15rem;
            margin-bottom: 18px;
        }}
        .footer-actions {{
            display: flex;
            justify-content: center;
            gap: 14px;
            flex-wrap: wrap;
            margin-bottom: 16px;
        }}
        .shortcuts-bar {{
            font-size: 0.78rem;
            color: var(--text-muted);
            margin-top: 10px;
        }}
        @media (prefers-reduced-motion: reduce) {{
            html {{ scroll-behavior: auto; }}
            header.hud, #progress-bar {{ transition: none; }}
            .page-skeleton {{ animation: none; }}
        }}
    </style>
</head>
<body>
    <div id="progress-bar-container"><div id="progress-bar"></div></div>
    <header class="hud" id="hud">
        <div class="hud-left">
            <div class="manga-info">
                <span class="manga-title" title="{manga_title}">{manga_title}</span>
                <span class="chapter-subtitle" title="{chapter_title}">{chapter_title}</span>
            </div>
            <span class="badge">{provider_badge}</span>
            {local_badge_html}
            <span class="badge sync-badge" id="sync-badge">{sync_badge_initial}</span>
        </div>
        <div class="hud-controls">
          <div class="control-group">
            {prev_btn_top}
            {chapter_select_html}
            {next_btn_top}
          </div>
          <div class="control-group">
            <select id="page-select" class="ctrl-select" aria-label="{page_select_label}" onchange="jumpToPage(this.value)">
                {page_options}
            </select>
            <select id="view-mode-select" class="ctrl-select" aria-label="{view_mode_label}" onchange="setViewMode(this.value)">
                <option value="cascade">{mode_cascade_label}</option>
                <option value="double-rtl">{mode_double_rtl_label}</option>
                <option value="double-ltr">{mode_double_ltr_label}</option>
                <option value="single">{mode_single_label}</option>
            </select>
          </div>
          <div class="control-group">
            <button class="ctrl-btn active" id="fit-orig" onclick="setFitMode('100%', 'fit-orig')">{fit_orig_label}</button>
            <button class="ctrl-btn" id="fit-wide" onclick="setFitMode('1200px', 'fit-wide')">{fit_wide_label}</button>
            <button class="ctrl-btn" id="fit-std" onclick="setFitMode('900px', 'fit-std')">{fit_std_label}</button>
            <button class="ctrl-btn" onclick="adjustZoom(-10)" title="Zoom -">−</button>
            <button class="ctrl-btn" onclick="adjustZoom(10)" title="Zoom +">+</button>
            <button class="ctrl-btn active" id="gap-btn" onclick="toggleGap()">{gap_label}</button>
            <button class="ctrl-btn" onclick="toggleFullscreen()">{fullscreen_label}</button>
          </div>
        </div>
    </header>

    <main class="reader-stream" id="reader-stream">
        {pages_html}
    </main>

    <footer class="chapter-footer">
        <h2>{end_title}</h2>
        <div class="footer-actions">
            {prev_btn_top}
            <button class="ctrl-btn" onclick="window.scrollTo({{top: 0}})">{back_top_label}</button>
            {next_btn_top}
        </div>
        <div class="shortcuts-bar">{shortcuts_hint}</div>
    </footer>

    <script>
        let currentPage = {initial_bookmarked_page};
        const totalPages = {page_count};
        let zoomPercent = 100;
        let seamlessGap = true;
        let viewMode = 'cascade';
        let lastScrollY = window.scrollY;
        let progressSaveTimer = null;
        let lastSavedPage = -1;
        const isEs = {is_es_js};
        const retryCounts = {{}};

        function scheduleSaveProgress(pageIdx) {{
            if (pageIdx === lastSavedPage) return;
            if (progressSaveTimer) clearTimeout(progressSaveTimer);
            progressSaveTimer = setTimeout(() => {{
                lastSavedPage = pageIdx;
                fetch('/progress?page=' + pageIdx, {{ method: 'POST', keepalive: true }})
                    .then(() => {{
                        const badge = document.getElementById('sync-badge');
                        if (badge) {{
                            badge.textContent = isEs
                                ? ('🔖 Pág. ' + (pageIdx + 1) + ' guardada')
                                : ('🔖 Saved pg. ' + (pageIdx + 1));
                        }}
                    }})
                    .catch(() => {{}});
            }}, 350);
        }}

        function jumpToChapter(chapterId) {{
            if (!chapterId) return;
            window.location.href = '/navigate?id=' + encodeURIComponent(chapterId);
        }}

        function setViewMode(mode) {{
            viewMode = mode;
            document.body.classList.remove('mode-double-rtl', 'mode-double-ltr', 'mode-single');
            if (mode === 'double-rtl') document.body.classList.add('mode-double-rtl');
            else if (mode === 'double-ltr') document.body.classList.add('mode-double-ltr');
            else if (mode === 'single') document.body.classList.add('mode-single');
            const sel = document.getElementById('view-mode-select');
            if (sel) sel.value = mode;
            updateSinglePageVisibility();
        }}

        function cycleViewMode() {{
            const modes = ['cascade', 'double-rtl', 'double-ltr', 'single'];
            const nextIdx = (modes.indexOf(viewMode) + 1) % modes.length;
            setViewMode(modes[nextIdx]);
        }}

        function updateSinglePageVisibility() {{
            document.querySelectorAll('.page-container').forEach(el => {{
                const idx = parseInt(el.getAttribute('data-index'), 10);
                el.classList.toggle('active-single-page', idx === currentPage);
            }});
        }}

        function onPageLoaded(idx) {{
            const skel = document.getElementById('skel-' + idx);
            if (skel) skel.style.display = 'none';
            const err = document.getElementById('err-' + idx);
            if (err) err.style.display = 'none';
            const img = document.getElementById('img-' + idx);
            if (img) {{
                img.style.display = 'block';
                const container = document.getElementById('page-' + idx);
                if (container && img.naturalWidth > img.naturalHeight * 1.15) {{
                    container.classList.add('landscape-spread');
                }}
            }}
        }}

        function onPageError(idx) {{
            retryCounts[idx] = (retryCounts[idx] || 0) + 1;
            if (retryCounts[idx] <= 2) {{
                setTimeout(() => retryPage(idx), 500 * retryCounts[idx]);
                return;
            }}
            const skel = document.getElementById('skel-' + idx);
            if (skel) skel.style.display = 'none';
            const img = document.getElementById('img-' + idx);
            if (img) img.style.display = 'none';
            const err = document.getElementById('err-' + idx);
            if (err) err.style.display = 'block';
        }}

        function retryPage(idx) {{
            const err = document.getElementById('err-' + idx);
            if (err) err.style.display = 'none';
            const skel = document.getElementById('skel-' + idx);
            if (skel) skel.style.display = 'flex';
            const img = document.getElementById('img-' + idx);
            if (img) {{
                img.style.display = 'block';
                img.src = '/page/' + idx + '?t=' + Date.now();
            }}
        }}

        function jumpToPage(idx) {{
            const parsed = parseInt(idx, 10);
            if (isNaN(parsed)) return;
            currentPage = Math.max(0, Math.min(totalPages - 1, parsed));
            updateSinglePageVisibility();
            const sel = document.getElementById('page-select');
            if (sel) sel.value = String(currentPage);
            scheduleSaveProgress(currentPage);
            const el = document.getElementById('page-' + currentPage);
            if (el) {{
                el.scrollIntoView({{ block: 'start' }});
            }}
        }}

        function setFitMode(maxWidth, btnId) {{
            zoomPercent = 100;
            document.documentElement.style.setProperty('--reader-max-width', maxWidth);
            document.querySelectorAll('.manga-page').forEach(img => {{
                img.style.width = 'auto';
            }});
            ['fit-orig', 'fit-wide', 'fit-std'].forEach(id => {{
                const b = document.getElementById(id);
                if (b) b.classList.toggle('active', id === btnId);
            }});
        }}

        function adjustZoom(delta) {{
            zoomPercent = Math.min(200, Math.max(40, zoomPercent + delta));
            document.documentElement.style.setProperty('--reader-max-width', zoomPercent + 'vw');
            document.querySelectorAll('.manga-page').forEach(img => {{
                img.style.width = zoomPercent + 'vw';
            }});
            ['fit-orig', 'fit-wide', 'fit-std'].forEach(id => {{
                const b = document.getElementById(id);
                if (b) b.classList.remove('active');
            }});
        }}

        function toggleGap() {{
            seamlessGap = !seamlessGap;
            document.documentElement.style.setProperty('--page-gap', seamlessGap ? '0px' : '14px');
            const btn = document.getElementById('gap-btn');
            if (btn) btn.classList.toggle('active', seamlessGap);
        }}

        function toggleFullscreen() {{
            if (!document.fullscreenElement) {{
                document.documentElement.requestFullscreen().catch(() => {{}});
            }} else {{
                document.exitFullscreen().catch(() => {{}});
            }}
        }}

        const hud = document.getElementById('hud');
        function updateHudHeight() {{
            document.documentElement.style.setProperty('--hud-height', hud.offsetHeight + 'px');
        }}
        updateHudHeight();
        new ResizeObserver(updateHudHeight).observe(hud);

        window.addEventListener('scroll', () => {{
            const scrollTop = window.scrollY || document.documentElement.scrollTop;
            const docHeight = Math.max(1, document.documentElement.scrollHeight - window.innerHeight);
            const pct = Math.min(100, Math.max(0, (scrollTop / docHeight) * 100));
            const bar = document.getElementById('progress-bar');
            if (bar) bar.style.width = pct + '%';

            if (scrollTop > 120 && scrollTop > lastScrollY + 8) {{
                hud.classList.add('hidden');
            }} else if (scrollTop < lastScrollY - 8 || scrollTop <= 80) {{
                hud.classList.remove('hidden');
            }}
            lastScrollY = scrollTop;
        }}, {{ passive: true }});

        const observer = new IntersectionObserver((entries) => {{
            if (viewMode === 'single') return;
            entries.forEach(entry => {{
                if (entry.isIntersecting) {{
                    const idx = parseInt(entry.target.getAttribute('data-index'), 10);
                    if (!isNaN(idx)) {{
                        currentPage = idx;
                        const sel = document.getElementById('page-select');
                        if (sel) sel.value = String(idx);
                        scheduleSaveProgress(idx);
                    }}
                }}
            }});
        }}, {{ rootMargin: '-35% 0px -55% 0px' }});

        document.querySelectorAll('.page-container').forEach(el => observer.observe(el));
        updateSinglePageVisibility();
        if (currentPage > 0) {{
            setTimeout(() => jumpToPage(currentPage), 120);
        }}

        window.addEventListener('keydown', (e) => {{
            if (e.target && (e.target.tagName === 'SELECT' || e.target.tagName === 'INPUT')) return;
            switch (e.key) {{
                case 'j':
                    window.scrollBy({{ top: Math.round(window.innerHeight * 0.72), behavior: 'smooth' }});
                    break;
                case 'k':
                    window.scrollBy({{ top: -Math.round(window.innerHeight * 0.72), behavior: 'smooth' }});
                    break;
                case 'd':
                case 'ArrowRight':
                    if (currentPage + 1 < totalPages) jumpToPage(currentPage + 1);
                    break;
                case 'a':
                case 'ArrowLeft':
                    if (currentPage > 0) jumpToPage(currentPage - 1);
                    break;
                case 'v':
                case 'V':
                    cycleViewMode();
                    break;
                case 'g':
                    window.scrollTo({{ top: 0, behavior: 'smooth' }});
                    break;
                case 'G':
                    window.scrollTo({{ top: document.documentElement.scrollHeight, behavior: 'smooth' }});
                    break;
                case '+':
                case '=':
                    adjustZoom(10);
                    break;
                case '-':
                    adjustZoom(-10);
                    break;
                case '0':
                    setFitMode('100%', 'fit-orig');
                    break;
                case 'm':
                    toggleGap();
                    break;
                case 'f':
                    toggleFullscreen();
                    break;
                case 'h':
                    document.getElementById('hud')?.classList.toggle('hidden');
                    break;
                case 'w':
                case ']':
                    document.getElementById('btn-next-chap')?.click();
                    break;
                case 'b':
                case '[':
                    document.getElementById('btn-prev-chap')?.click();
                    break;
            }}
        }});
    </script>
</body>
</html>"#,
        lang = if es { "es" } else { "en" },
        page_select_label = if es { "Página" } else { "Page" },
        view_mode_label = if es { "Modo de lectura" } else { "Reading mode" },
        is_es_js = if es { "true" } else { "false" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::manga_provider::{Languages, SortedChapters, SortedVolumes, Volumes};

    #[test]
    fn test_detect_image_mime() {
        assert_eq!(detect_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0], "https://cdn.example/1.bin"), "image/jpeg");
        assert_eq!(detect_image_mime(&[0x89, b'P', b'N', b'G', 0x0D], "https://cdn.example/1.bin"), "image/png");
        assert_eq!(detect_image_mime(b"RIFF\x00\x00\x00\x00WEBPVP8 ", "https://cdn.example/1.bin"), "image/webp");
        assert_eq!(detect_image_mime(b"GIF89a", "https://cdn.example/1.bin"), "image/gif");
        assert_eq!(detect_image_mime(&[0, 1, 2], "https://cdn.example/page.webp?token=1"), "image/webp");
        assert_eq!(detect_image_mime(&[0, 1, 2], "https://cdn.example/page.png"), "image/png");
    }

    #[test]
    fn test_render_viewer_html_contains_pages_and_controls() {
        let session = WebViewerSession {
            manga_id: "berserk-1".to_string(),
            manga_title: "Berserk & Friends".to_string(),
            manga_cover_url: None,
            provider: MangaProviders::Mangaoni,
            chapter: ChapterToRead {
                id: "cap-10".to_string(),
                title: "El Espadachín Negro".to_string(),
                number: 10.0,
                volume_number: Some("1".to_string()),
                num_page_bookmarked: None,
                language: Languages::Spanish,
                pages_url: vec!["https://cdn.example.com/1.jpg".parse().unwrap(), "https://cdn.example.com/2.jpg".parse().unwrap()],
            },
            list_of_chapters: ListOfChapters {
                volumes: SortedVolumes::new(vec![Volumes {
                    volume: "1".to_string(),
                    chapters: SortedChapters::new(vec![
                        ChapterReader {
                            id: "cap-9".to_string(),
                            number: "9".to_string(),
                            volume: "1".to_string(),
                        },
                        ChapterReader {
                            id: "cap-10".to_string(),
                            number: "10".to_string(),
                            volume: "1".to_string(),
                        },
                        ChapterReader {
                            id: "cap-11".to_string(),
                            number: "11".to_string(),
                            volume: "1".to_string(),
                        },
                    ]),
                }]),
            },
            local_source: None,
            ..Default::default()
        };

        let prev = session.list_of_chapters.get_previous_chapter(Some("1"), 10.0);
        let next = session.list_of_chapters.get_next_chapter(Some("1"), 10.0);
        let html = render_viewer_html(&session, prev.as_ref(), next.as_ref());

        assert!(html.contains("Berserk &amp; Friends"));
        assert!(html.contains("El Espadachín Negro"));
        assert!(html.contains("/page/0?ch=cap-10"));
        assert!(html.contains("/page/1?ch=cap-10"));
        assert!(html.contains("/navigate?dir=prev"));
        assert!(html.contains("/navigate?dir=next"));
        assert!(html.contains(r#"id="chapter-select""#));
        assert!(html.contains(r#"id="view-mode-select""#));
        assert!(html.contains("double-rtl"));
    }

    #[tokio::test]
    async fn test_web_viewer_local_server_serves_html() {
        let session = WebViewerSession {
            manga_id: "test-manga".to_string(),
            manga_title: "One Piece".to_string(),
            manga_cover_url: None,
            provider: MangaProviders::Mangadex,
            chapter: ChapterToRead {
                id: "ch-1".to_string(),
                title: "Romance Dawn".to_string(),
                number: 1.0,
                volume_number: Some("1".to_string()),
                num_page_bookmarked: None,
                language: Languages::Spanish,
                pages_url: vec!["https://example.com/p1.jpg".parse().unwrap()],
            },
            list_of_chapters: ListOfChapters::default(),
            local_source: None,
            ..Default::default()
        };

        let url = start_or_update_web_viewer(session).await.expect("server should start");
        assert!(url.starts_with("http://127.0.0.1:"));

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client.get(&url).send().await.expect("GET / should succeed");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert!(body.contains("One Piece"));
        assert!(body.contains("Romance Dawn"));
    }

    #[tokio::test]
    async fn test_web_viewer_offline_resolve_chapter_and_serve_local_pages() {
        use std::io::Cursor;

        use bytes::Bytes;
        use image::{DynamicImage, ImageFormat, RgbImage};

        use crate::backend::manga_downloader::epub_downloader::EpubDownloader;
        use crate::backend::manga_downloader::{ChapterToDownloadSanitized, MangaDownloader};
        use crate::backend::manga_provider::ChapterPage;
        use crate::config::DownloadType;

        struct TempDir(std::path::PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let temp = TempDir(std::env::temp_dir().join(format!("web_viewer_offline_{}", uuid::Uuid::new_v4())));
        let img = RgbImage::from_fn(8, 8, |_, _| image::Rgb([10, 120, 220]));
        let mut png_bytes = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
            .unwrap();

        let epub_ch = ChapterToDownloadSanitized {
            chapter_id: "ch-2".to_string(),
            manga_id: "m-1".to_string(),
            manga_title: "Offline Series".to_string().into(),
            chapter_title: "Chapter 2".to_string().into(),
            chapter_number: "2".to_string(),
            volume_number: None,
            language: Languages::Spanish,
            scanlator: "".to_string().into(),
            download_type: DownloadType::Epub,
            pages: vec![ChapterPage {
                bytes: Bytes::from(png_bytes),
                extension: "png".to_string(),
            }],
            provider: Some(MangaProviders::Inmanga),
        };
        EpubDownloader::new().save_chapter_in_file_system(&temp.0, epub_ch).unwrap();
        let epub_source = LocalChapterSource::find_in_dir(&temp.0, "Offline Series", "ch-2").expect("EPUB exists");

        let mut offline_sources = HashMap::new();
        offline_sources.insert("ch-2".to_string(), epub_source.clone());

        let session = WebViewerSession {
            manga_id: "m-1".to_string(),
            manga_title: "Offline Series".to_string(),
            manga_cover_url: None,
            provider: MangaProviders::Inmanga,
            chapter: ChapterToRead {
                id: "ch-1".to_string(),
                title: "Chapter 1".to_string(),
                number: 1.0,
                volume_number: Some("none".to_string()),
                num_page_bookmarked: None,
                language: Languages::Spanish,
                pages_url: vec!["http://localhost/local/0".parse().unwrap()],
            },
            list_of_chapters: ListOfChapters::default(),
            local_source: Some(epub_source),
            offline_sources,
            is_offline_mode: true,
        };

        let resolved = resolve_chapter(&session, &ChapterReader {
            id: "ch-2".to_string(),
            number: "2".to_string(),
            volume: "none".to_string(),
        })
        .await
        .expect("offline resolve_chapter must succeed without network");
        assert_eq!(resolved.id, "ch-2");
        assert_eq!(resolved.pages_url.len(), 1);

        let html = render_viewer_html(&session, None, None);
        assert!(html.contains("💾 Local"), "local badge should render in HUD");

        // Serve /page/0 via ephemeral local HTTP server without touching global WEB_VIEWER_SERVER
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared_state = Arc::new(RwLock::new(session));
        let srv_state = Arc::clone(&shared_state);
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let _ = handle_connection(stream, srv_state).await;
            }
        });

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/page/0"))
            .send()
            .await
            .expect("GET /page/0 should succeed");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body_bytes = resp.bytes().await.unwrap();
        assert!(body_bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
