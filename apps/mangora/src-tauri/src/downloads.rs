use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use manga_core::SanitizedFilename;
use manga_core::backend::download_queue::{
    DownloadChapterFinishedEvent, DownloadTaskStatus, EnqueueDownloadBatch, ManagedDownloadQueue, ManagedQueueSnapshot,
};
use manga_core::backend::manga_provider::{Chapter, Languages, MangaProviders};
use manga_core::backend::offline::{format_size_bytes, scan_offline_mangas_in_dir};
use manga_core::config::DownloadType;
use manga_core::runtime_settings::download_type_toml_value;
use serde::{Deserialize, Serialize};

use crate::state::{AppState, InitialAppState};

pub const DOWNLOAD_QUEUE_EVENT: &str = "mangora://download-queue";
pub const DOWNLOAD_FINISHED_EVENT: &str = "mangora://download-chapter-finished";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTaskDto {
    pub task_id: String,
    pub manga_id: String,
    pub manga_title: String,
    pub manga_cover_url: Option<String>,
    pub provider: String,
    pub provider_label: String,
    pub language: String,
    pub chapter_id: String,
    pub chapter_title: String,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub format: String,
    pub status: DownloadTaskStatus,
    pub progress_percent: f64,
    pub pages_downloaded: usize,
    pub total_pages: usize,
    pub bytes_downloaded: u64,
    pub speed_bytes_per_sec: f64,
    pub error_message: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQueueSnapshotDto {
    pub is_paused: bool,
    pub is_downloading: bool,
    pub active_format: String,
    pub downloads_dir: String,
    pub queued_count: usize,
    pub downloading_count: usize,
    pub paused_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub overall_progress_percent: f64,
    pub speed_bytes_per_sec: f64,
    pub items: Vec<DownloadTaskDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadChapterFinishedDto {
    pub task_id: String,
    pub manga_id: String,
    pub manga_title: String,
    pub chapter_id: String,
    pub chapter_number: String,
    pub format: String,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueChapterInput {
    pub id: String,
    pub title: String,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub language: String,
    pub scanlator: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueChaptersRequest {
    pub manga_id: String,
    pub provider: String,
    pub manga_title: String,
    pub cover_url: Option<String>,
    pub format_override: Option<String>,
    pub chapters: Vec<EnqueueChapterInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQueueControlRequest {
    pub action: String,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDownloadFormatResult {
    pub queue: DownloadQueueSnapshotDto,
    pub app_state: InitialAppState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineSeriesItemDto {
    pub manga_id: String,
    pub title: String,
    pub provider: String,
    pub provider_label: String,
    pub is_offline_only: bool,
    pub chapter_count: usize,
    pub read_count: usize,
    pub formats: Vec<String>,
    pub total_size_bytes: u64,
    pub total_size_label: String,
    pub cover_url: Option<String>,
}

pub fn parse_download_type(raw: &str) -> Result<DownloadType, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cbz" => Ok(DownloadType::Cbz),
        "pdf" => Ok(DownloadType::Pdf),
        "epub" => Ok(DownloadType::Epub),
        "raw" => Ok(DownloadType::Raw),
        other => Err(format!("Unsupported download format: '{other}'")),
    }
}

pub fn safe_chapter_download_id(provider: MangaProviders, chapter_id: &str) -> String {
    if provider == MangaProviders::Mangapill
        && let Some(segment) = chapter_id.split('/').nth(1).filter(|s| !s.is_empty())
    {
        return SanitizedFilename::new(segment).to_string();
    }
    SanitizedFilename::new(chapter_id).to_string()
}

pub fn map_queue_snapshot(snap: ManagedQueueSnapshot) -> DownloadQueueSnapshotDto {
    DownloadQueueSnapshotDto {
        is_paused: snap.is_paused,
        is_downloading: snap.is_downloading,
        active_format: download_type_toml_value(snap.active_format).to_string(),
        downloads_dir: snap.downloads_dir,
        queued_count: snap.queued_count,
        downloading_count: snap.downloading_count,
        paused_count: snap.paused_count,
        completed_count: snap.completed_count,
        failed_count: snap.failed_count,
        overall_progress_percent: snap.overall_progress_percent,
        speed_bytes_per_sec: snap.speed_bytes_per_sec,
        items: snap
            .items
            .into_iter()
            .map(|item| DownloadTaskDto {
                task_id: item.task_id,
                manga_id: item.manga_id,
                manga_title: item.manga_title,
                manga_cover_url: item.manga_cover_url,
                provider: item.provider.to_string(),
                provider_label: item.provider.badge_label().to_string(),
                language: item.lang.as_iso_code().to_string(),
                chapter_id: item.chapter.id,
                chapter_title: item.chapter.title,
                chapter_number: item.chapter.chapter_number,
                volume_number: item.chapter.volume_number,
                format: download_type_toml_value(item.format).to_string(),
                status: item.status,
                progress_percent: item.progress_percent,
                pages_downloaded: item.pages_downloaded,
                total_pages: item.total_pages,
                bytes_downloaded: item.bytes_downloaded,
                speed_bytes_per_sec: item.speed_bytes_per_sec,
                error_message: item.error_message,
                output_path: item.output_path,
            })
            .collect(),
    }
}

pub fn map_finished_event(ev: DownloadChapterFinishedEvent) -> DownloadChapterFinishedDto {
    DownloadChapterFinishedDto {
        task_id: ev.task_id,
        manga_id: ev.manga_id,
        manga_title: ev.manga_title,
        chapter_id: ev.chapter_id,
        chapter_number: ev.chapter_number,
        format: download_type_toml_value(ev.format).to_string(),
        output_path: ev.output_path,
    }
}

pub fn enqueue_chapters(queue: &ManagedDownloadQueue, payload: EnqueueChaptersRequest) -> Result<DownloadQueueSnapshotDto, String> {
    let manga_id = payload.manga_id.trim().to_string();
    let manga_title = payload.manga_title.trim().to_string();
    if manga_id.is_empty() || manga_title.is_empty() {
        return Err("Missing manga identifier or title".to_string());
    }
    if manga_id.starts_with("offline::") {
        return Err("Cannot enqueue downloads for local-only archive series".to_string());
    }
    if payload.chapters.is_empty() {
        return Err("Select at least one chapter to download".to_string());
    }

    let provider = MangaProviders::from_str(&payload.provider.trim().to_ascii_lowercase())
        .map_err(|_| format!("Unsupported manga provider: '{}'", payload.provider))?;
    let format_override = payload.format_override.as_deref().map(parse_download_type).transpose()?;
    let safe_manga_id = SanitizedFilename::new(&manga_id).to_string();
    let cover_url = payload
        .cover_url
        .as_deref()
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_owned);

    let mut grouped_by_lang: BTreeMap<String, (Languages, Vec<Chapter>)> = BTreeMap::new();
    for ch in payload.chapters {
        let ch_id = ch.id.trim().to_string();
        if ch_id.is_empty() {
            continue;
        }
        let lang = Languages::from_str_lenient(ch.language.trim()).unwrap_or_default();
        let lang_key = lang.as_iso_code().to_string();
        let chapter_num = if ch.chapter_number.trim().is_empty() { "?".to_string() } else { ch.chapter_number.trim().to_string() };
        let title = if ch.title.trim().is_empty() { format!("Cap. {chapter_num}") } else { ch.title.trim().to_string() };

        let chapter = Chapter {
            id: ch_id.clone(),
            id_safe_for_download: safe_chapter_download_id(provider, &ch_id),
            manga_id: manga_id.clone(),
            title,
            language: lang,
            chapter_number: chapter_num,
            volume_number: ch.volume_number.filter(|v| !v.trim().is_empty()),
            scanlator: ch.scanlator.filter(|s| !s.trim().is_empty()),
            publication_date: None,
        };
        grouped_by_lang.entry(lang_key).or_insert_with(|| (lang, Vec::new())).1.push(chapter);
    }

    if grouped_by_lang.is_empty() {
        return Err("No valid chapters found to enqueue".to_string());
    }

    let mut last_snap = queue.snapshot();
    for (_, (lang, chapters)) in grouped_by_lang {
        last_snap = queue.enqueue_batch(
            EnqueueDownloadBatch {
                manga_id: manga_id.clone(),
                manga_id_safe_for_download: safe_manga_id.clone(),
                manga_title: manga_title.clone(),
                manga_cover_url: cover_url.clone(),
                lang,
                chapters,
                provider,
            },
            format_override,
        );
    }

    Ok(map_queue_snapshot(last_snap))
}

pub fn apply_queue_control(
    queue: &ManagedDownloadQueue,
    request: DownloadQueueControlRequest,
) -> Result<DownloadQueueSnapshotDto, String> {
    let snap = match request.action.trim() {
        "pauseAll" => queue.pause_all(),
        "resumeAll" => queue.resume_all(),
        "cancelAll" => queue.cancel_all(),
        "retryAllFailed" => queue.retry_all_failed(),
        "clearFinished" => queue.clear_finished(),
        "pauseTask" => {
            let task_id = request
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("Missing taskId")?;
            queue.pause_task(task_id)
        },
        "resumeTask" => {
            let task_id = request
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("Missing taskId")?;
            queue.resume_task(task_id)
        },
        "cancelTask" => {
            let task_id = request
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("Missing taskId")?;
            queue.cancel_task(task_id)
        },
        "retryTask" => {
            let task_id = request
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("Missing taskId")?;
            queue.retry_task(task_id)
        },
        "removeTask" => {
            let task_id = request
                .task_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or("Missing taskId")?;
            queue.remove_task(task_id)
        },
        other => return Err(format!("Unknown queue control action: '{other}'")),
    };

    Ok(map_queue_snapshot(snap))
}

pub fn scan_offline_series(state: &AppState) -> Result<Vec<OfflineSeriesItemDto>, String> {
    let downloads_dir = state.downloads_dir()?;
    let active_provider = state.snapshot()?.active_provider;

    let conn = state.open_db_connection()?;
    let mut covers: HashMap<String, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, img_url FROM mangas WHERE img_url IS NOT NULL AND TRIM(img_url) != ''")
        && let Ok(rows) = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
    {
        for (id, url) in rows.flatten() {
            covers.insert(id, url);
        }
    }

    let db = manga_core::backend::database::Database::new(&conn);
    let offline = scan_offline_mangas_in_dir(&downloads_dir, Some(&db));

    Ok(offline
        .into_iter()
        .map(|item| {
            let mut formats = Vec::new();
            for ch in &item.chapters {
                let fmt = ch.format_label.to_string();
                if !formats.contains(&fmt) {
                    formats.push(fmt);
                }
            }
            let read_count = item.read_count();
            let is_offline_only = item.manga_id.starts_with("offline::");
            let provider = item.provider.map(|p| p.to_string()).unwrap_or_else(|| active_provider.clone());
            let provider_label = item.provider.map(|p| p.badge_label().to_string()).unwrap_or_else(|| "Local".to_string());
            let cover_url = covers.get(&item.manga_id).cloned();

            OfflineSeriesItemDto {
                manga_id: item.manga_id,
                title: item.title,
                provider,
                provider_label,
                is_offline_only,
                chapter_count: item.chapters.len(),
                read_count,
                formats,
                total_size_bytes: item.total_size_bytes,
                total_size_label: format_size_bytes(item.total_size_bytes),
                cover_url,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use manga_core::backend::StorageMode;
    use manga_core::config::{UiLanguage, build_config_file_in_dir};
    use pretty_assertions::assert_eq;

    use super::*;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = std::env::temp_dir().join(format!("mangora_downloads_{name}_{nanos}"));
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

    #[test]
    fn enqueues_controls_and_scans_offline_series_via_dtos() {
        let temp = TempTestDir::new("dtos");
        let data_dir = temp.path.join("data");
        let config_dir = temp.path.join("config");
        let downloads_dir = data_dir.join("mangaDownloads");
        let db_path = data_dir.join("history").join("manga-tui-history.db");

        fs::create_dir_all(&downloads_dir).unwrap();
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        build_config_file_in_dir(&config_dir).unwrap();

        let state = AppState::from_paths(
            data_dir,
            config_dir.clone(),
            downloads_dir.clone(),
            db_path.clone(),
            StorageMode::EnvOverride,
            UiLanguage::Es,
            MangaProviders::Inmanga,
            Languages::Spanish,
        );

        let queue = ManagedDownloadQueue::new_without_worker(downloads_dir.clone(), config_dir, db_path);

        let snap = enqueue_chapters(&queue, EnqueueChaptersRequest {
            manga_id: "m-101".to_string(),
            provider: "inmanga".to_string(),
            manga_title: "Frieren".to_string(),
            cover_url: Some("https://example.com/frieren.jpg".to_string()),
            format_override: Some("epub".to_string()),
            chapters: vec![
                EnqueueChapterInput {
                    id: "ch-1".to_string(),
                    title: "El inicio".to_string(),
                    chapter_number: "1".to_string(),
                    volume_number: None,
                    language: "es".to_string(),
                    scanlator: None,
                },
                EnqueueChapterInput {
                    id: "ch-2".to_string(),
                    title: "El viaje".to_string(),
                    chapter_number: "2".to_string(),
                    volume_number: None,
                    language: "es".to_string(),
                    scanlator: None,
                },
            ],
        })
        .unwrap();

        assert_eq!(snap.queued_count, 2);
        assert_eq!(snap.items.len(), 2);
        assert_eq!(snap.items[0].format, "epub");

        let paused = apply_queue_control(&queue, DownloadQueueControlRequest {
            action: "pauseTask".to_string(),
            task_id: Some(snap.items[0].task_id.clone()),
        })
        .unwrap();
        assert_eq!(paused.paused_count, 1);
        assert_eq!(paused.queued_count, 1);

        let cleared = apply_queue_control(&queue, DownloadQueueControlRequest {
            action: "cancelAll".to_string(),
            task_id: None,
        })
        .and_then(|_| {
            apply_queue_control(&queue, DownloadQueueControlRequest {
                action: "clearFinished".to_string(),
                task_id: None,
            })
        })
        .unwrap();
        assert!(cleared.items.is_empty());

        // Offline folder check
        let frieren_dir = downloads_dir.join("Frieren");
        fs::create_dir_all(&frieren_dir).unwrap();
        fs::write(frieren_dir.join("Ch 1 Vol none El inicio  ch-1.cbz"), b"dummy-cbz").unwrap();

        let offline_series = scan_offline_series(&state).unwrap();
        assert_eq!(offline_series.len(), 1);
        assert_eq!(offline_series[0].title, "Frieren");
        assert_eq!(offline_series[0].chapter_count, 1);
        assert_eq!(offline_series[0].formats, vec!["CBZ".to_string()]);
        assert!(offline_series[0].is_offline_only);
    }
}
