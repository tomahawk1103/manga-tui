use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::backend::AppDirectories;
use crate::backend::database::{Database, SetChapterDownloaded};
use crate::backend::error_log::write_to_error_log;
use crate::backend::manga_downloader::cbz_downloader::CbzDownloader;
use crate::backend::manga_downloader::epub_downloader::EpubDownloader;
use crate::backend::manga_downloader::pdf_downloader::PdfDownloader;
use crate::backend::manga_downloader::raw_images::RawImagesDownloader;
use crate::backend::manga_downloader::{ChapterToDownloadSanitized, MangaDownloader};
use crate::backend::manga_provider::{Chapter, ChapterPage, ChapterPageUrl, Languages, MangaProvider, MangaProviders};
use crate::config::{DownloadType, MangaTuiConfig};
use crate::runtime_settings::{active_download_type, download_type_toml_value, persist_setting_in_dir, set_active_download_type};

#[derive(Clone, Debug, PartialEq)]
pub struct EnqueueChapterItem {
    pub manga_id: String,
    pub manga_id_safe_for_download: String,
    pub manga_title: String,
    pub manga_cover_url: Option<String>,
    pub lang: Languages,
    pub chapter: Chapter,
    pub provider: MangaProviders,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnqueueDownloadBatch {
    pub manga_id: String,
    pub manga_id_safe_for_download: String,
    pub manga_title: String,
    pub manga_cover_url: Option<String>,
    pub lang: Languages,
    pub chapters: Vec<Chapter>,
    pub provider: MangaProviders,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DownloadQueueProgress {
    pub is_downloading: bool,
    pub current_manga_title: String,
    pub current_chapter_number: String,
    pub current_page_percentage: f64,
    pub completed_in_batch: usize,
    pub failed_in_batch: usize,
    pub total_in_batch: usize,
    pub remaining_items: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadQueueEvent {
    Progress(DownloadQueueProgress),
    ChapterFinished {
        manga_id: String,
        chapter_id: String,
    },
    Finished(usize),
}

pub type DownloadQueueEventSink = Arc<dyn Fn(DownloadQueueEvent) + Send + Sync>;

#[derive(Debug)]
pub enum DownloadQueueCommand {
    Enqueue(EnqueueDownloadBatch),
    CancelAll,
}

pub struct DownloadQueueWorker<T: MangaProvider> {
    client: Arc<T>,
    event_sink: DownloadQueueEventSink,
    command_rx: UnboundedReceiver<DownloadQueueCommand>,
    queue: VecDeque<EnqueueChapterItem>,
    completed_in_batch: usize,
    failed_in_batch: usize,
    total_in_batch: usize,
}

impl<T: MangaProvider + 'static> DownloadQueueWorker<T> {
    pub fn new(client: Arc<T>, event_sink: DownloadQueueEventSink, command_rx: UnboundedReceiver<DownloadQueueCommand>) -> Self {
        Self {
            client,
            event_sink,
            command_rx,
            queue: VecDeque::new(),
            completed_in_batch: 0,
            failed_in_batch: 0,
            total_in_batch: 0,
        }
    }

    pub async fn run(mut self) {
        loop {
            // Drain any pending commands before starting next download
            while let Ok(cmd) = self.command_rx.try_recv() {
                match cmd {
                    DownloadQueueCommand::Enqueue(batch) => {
                        let count = batch.chapters.len();
                        for chapter in batch.chapters {
                            self.queue.push_back(EnqueueChapterItem {
                                manga_id: batch.manga_id.clone(),
                                manga_id_safe_for_download: batch.manga_id_safe_for_download.clone(),
                                manga_title: batch.manga_title.clone(),
                                manga_cover_url: batch.manga_cover_url.clone(),
                                lang: batch.lang,
                                chapter,
                                provider: batch.provider,
                            });
                        }
                        self.total_in_batch += count;
                        self.send_progress(false, "", "", 0.0);
                    },
                    DownloadQueueCommand::CancelAll => {
                        self.queue.clear();
                        self.completed_in_batch = 0;
                        self.failed_in_batch = 0;
                        self.total_in_batch = 0;
                        self.send_progress(false, "", "", 0.0);
                    },
                }
            }

            if let Some(item) = self.queue.pop_front() {
                self.send_progress(true, &item.manga_title, &item.chapter.chapter_number, 0.0);

                let config = MangaTuiConfig::get().effective();
                let event_sink_clone = Arc::clone(&self.event_sink);
                let manga_title_clone = item.manga_title.clone();
                let chapter_num_clone = item.chapter.chapter_number.clone();
                let completed = self.completed_in_batch;
                let failed = self.failed_in_batch;
                let total = self.total_in_batch;
                let remaining = self.queue.len();

                let download_fut = async {
                    if item.provider != self.client.name() {
                        crate::backend::multi_provider::get_chapter_pages_for_provider(
                            item.provider,
                            &item.chapter.id,
                            &item.manga_id,
                            config.image_quality,
                            move |percent, _| {
                                (event_sink_clone)(DownloadQueueEvent::Progress(DownloadQueueProgress {
                                    is_downloading: true,
                                    current_manga_title: manga_title_clone.clone(),
                                    current_chapter_number: chapter_num_clone.clone(),
                                    current_page_percentage: percent,
                                    completed_in_batch: completed,
                                    failed_in_batch: failed,
                                    total_in_batch: total,
                                    remaining_items: remaining + 1,
                                }));
                            },
                        )
                        .await
                        .map_err(|e| e.to_string())
                    } else {
                        self.client
                            .get_chapter_pages(&item.chapter.id, &item.manga_id, config.image_quality, move |percent, _| {
                                (event_sink_clone)(DownloadQueueEvent::Progress(DownloadQueueProgress {
                                    is_downloading: true,
                                    current_manga_title: manga_title_clone.clone(),
                                    current_chapter_number: chapter_num_clone.clone(),
                                    current_page_percentage: percent,
                                    completed_in_batch: completed,
                                    failed_in_batch: failed,
                                    total_in_batch: total,
                                    remaining_items: remaining + 1,
                                }));
                            })
                            .await
                            .map_err(|e| e.to_string())
                    }
                };

                let pages_result = tokio::select! {
                    res = download_fut => res,
                    Some(cmd) = self.command_rx.recv() => {
                        match cmd {
                            DownloadQueueCommand::CancelAll => {
                                self.queue.clear();
                                self.completed_in_batch = 0;
                                self.failed_in_batch = 0;
                                self.total_in_batch = 0;
                                self.send_progress(false, "", "", 0.0);
                                continue;
                            },
                            DownloadQueueCommand::Enqueue(batch) => {
                                let count = batch.chapters.len();
                                for chapter in batch.chapters {
                                    self.queue.push_back(EnqueueChapterItem {
                                        manga_id: batch.manga_id.clone(),
                                        manga_id_safe_for_download: batch.manga_id_safe_for_download.clone(),
                                        manga_title: batch.manga_title.clone(),
                                        manga_cover_url: batch.manga_cover_url.clone(),
                                        lang: batch.lang,
                                        chapter,
                                        provider: batch.provider,
                                    });
                                }
                                self.total_in_batch += count;
                                continue;
                            }
                        }
                    }
                };

                match pages_result {
                    Ok(pages) => {
                        let original_chapter_title = item.chapter.title.clone();
                        let chapter_id = item.chapter.id.clone();
                        let chapter_to_download = ChapterToDownloadSanitized {
                            chapter_id: item.chapter.id_safe_for_download,
                            manga_id: item.manga_id_safe_for_download,
                            manga_title: item.manga_title.clone().into(),
                            chapter_title: item.chapter.title.clone().into(),
                            chapter_number: item.chapter.chapter_number.clone(),
                            volume_number: item.chapter.volume_number,
                            language: item.lang,
                            scanlator: item.chapter.scanlator.unwrap_or_default().into(),
                            download_type: config.download_type,
                            pages,
                            provider: Some(item.provider),
                        };

                        let save_base = AppDirectories::MangaDownloads.get_full_path();
                        let download_type = config.download_type;
                        let download_res = tokio::task::spawn_blocking(move || -> Result<(), String> {
                            match download_type {
                                DownloadType::Cbz => CbzDownloader::new()
                                    .save_chapter_in_file_system(&save_base, chapter_to_download)
                                    .map_err(|e| e.to_string()),
                                DownloadType::Raw => RawImagesDownloader::new()
                                    .save_chapter_in_file_system(&save_base, chapter_to_download)
                                    .map_err(|e| e.to_string()),
                                DownloadType::Epub => EpubDownloader::new()
                                    .save_chapter_in_file_system(&save_base, chapter_to_download)
                                    .map_err(|e| e.to_string()),
                                DownloadType::Pdf => PdfDownloader::new()
                                    .save_chapter_in_file_system(&save_base, chapter_to_download)
                                    .map_err(|e| e.to_string()),
                            }
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|res| res);

                        if let Err(e) = download_res {
                            self.failed_in_batch += 1;
                            write_to_error_log(format!("Failed to download chapter: {original_chapter_title}, error: {e}").into());
                        } else {
                            if let Ok(conn) = Database::get_connection() {
                                let database = Database::new(&conn);
                                let _ = database.set_chapter_downloaded(SetChapterDownloaded {
                                    id: &chapter_id,
                                    title: &original_chapter_title,
                                    manga_id: &item.manga_id,
                                    manga_title: &item.manga_title,
                                    img_url: item.manga_cover_url.as_deref(),
                                    provider: item.provider,
                                });
                            }

                            self.completed_in_batch += 1;
                            (self.event_sink)(DownloadQueueEvent::ChapterFinished {
                                manga_id: item.manga_id,
                                chapter_id,
                            });
                        }
                    },
                    Err(e) => {
                        self.failed_in_batch += 1;
                        write_to_error_log(format!("Failed to fetch pages for chapter: {}, error: {e}", item.chapter.title).into());
                    },
                }

                // Rate limiting delay between chapters (2 seconds)
                tokio::time::sleep(Duration::from_secs(2)).await;

                if self.queue.is_empty() {
                    let total = self.completed_in_batch;
                    self.completed_in_batch = 0;
                    self.failed_in_batch = 0;
                    self.total_in_batch = 0;
                    self.send_progress(false, "", "", 0.0);
                    (self.event_sink)(DownloadQueueEvent::Finished(total));
                } else {
                    self.send_progress(false, "", "", 0.0);
                }
            } else {
                // Queue is empty: wait for next command
                match self.command_rx.recv().await {
                    Some(DownloadQueueCommand::Enqueue(batch)) => {
                        let count = batch.chapters.len();
                        for chapter in batch.chapters {
                            self.queue.push_back(EnqueueChapterItem {
                                manga_id: batch.manga_id.clone(),
                                manga_id_safe_for_download: batch.manga_id_safe_for_download.clone(),
                                manga_title: batch.manga_title.clone(),
                                manga_cover_url: batch.manga_cover_url.clone(),
                                lang: batch.lang,
                                chapter,
                                provider: batch.provider,
                            });
                        }
                        self.total_in_batch += count;
                        self.send_progress(false, "", "", 0.0);
                    },
                    Some(DownloadQueueCommand::CancelAll) => {
                        self.queue.clear();
                        self.completed_in_batch = 0;
                        self.failed_in_batch = 0;
                        self.total_in_batch = 0;
                        self.send_progress(false, "", "", 0.0);
                    },
                    None => {
                        // Channel sender dropped, terminate worker
                        break;
                    },
                }
            }
        }
    }

    fn send_progress(&self, is_downloading: bool, title: &str, ch_num: &str, percent: f64) {
        (self.event_sink)(DownloadQueueEvent::Progress(DownloadQueueProgress {
            is_downloading,
            current_manga_title: title.to_string(),
            current_chapter_number: ch_num.to_string(),
            current_page_percentage: percent,
            completed_in_batch: self.completed_in_batch,
            failed_in_batch: self.failed_in_batch,
            total_in_batch: self.total_in_batch,
            remaining_items: self.queue.len(),
        }));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadTaskStatus {
    Queued,
    Downloading,
    Packaging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadTaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Downloading | Self::Packaging | Self::Paused)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTaskItem {
    pub task_id: String,
    pub manga_id: String,
    pub manga_id_safe_for_download: String,
    pub manga_title: String,
    pub manga_cover_url: Option<String>,
    pub provider: MangaProviders,
    pub lang: Languages,
    pub chapter: Chapter,
    pub format: DownloadType,
    pub status: DownloadTaskStatus,
    pub progress_percent: f64,
    pub pages_downloaded: usize,
    pub total_pages: usize,
    pub bytes_downloaded: u64,
    pub speed_bytes_per_sec: f64,
    pub error_message: Option<String>,
    pub output_path: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedQueueSnapshot {
    pub is_paused: bool,
    pub is_downloading: bool,
    pub active_format: DownloadType,
    pub downloads_dir: String,
    pub queued_count: usize,
    pub downloading_count: usize,
    pub paused_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub overall_progress_percent: f64,
    pub speed_bytes_per_sec: f64,
    pub items: Vec<DownloadTaskItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadChapterFinishedEvent {
    pub task_id: String,
    pub manga_id: String,
    pub manga_title: String,
    pub chapter_id: String,
    pub chapter_number: String,
    pub format: DownloadType,
    pub output_path: Option<String>,
}

pub type ManagedQueueSnapshotSink = Arc<dyn Fn(ManagedQueueSnapshot) + Send + Sync>;
pub type ManagedChapterFinishedSink = Arc<dyn Fn(DownloadChapterFinishedEvent) + Send + Sync>;

#[derive(Default)]
struct PartialTaskDownload {
    page_urls: Vec<ChapterPageUrl>,
    downloaded_pages: Vec<ChapterPage>,
    next_page_index: usize,
    bytes_downloaded: u64,
}

struct ManagedQueueInner {
    items: Vec<DownloadTaskItem>,
    partials: HashMap<String, PartialTaskDownload>,
    is_paused: bool,
    active_format: DownloadType,
    next_seq: u64,
    downloads_dir: PathBuf,
    config_dir: PathBuf,
    db_path: PathBuf,
}

#[derive(Clone)]
pub struct ManagedDownloadQueue {
    inner: Arc<Mutex<ManagedQueueInner>>,
    notify: Arc<Notify>,
    snapshot_sink: ManagedQueueSnapshotSink,
    finished_sink: ManagedChapterFinishedSink,
}

impl std::fmt::Debug for ManagedDownloadQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedDownloadQueue").finish_non_exhaustive()
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn build_snapshot(inner: &ManagedQueueInner) -> ManagedQueueSnapshot {
    let mut queued_count = 0usize;
    let mut downloading_count = 0usize;
    let mut paused_count = 0usize;
    let mut completed_count = 0usize;
    let mut failed_count = 0usize;
    let mut speed_bytes_per_sec = 0.0f64;
    let mut progress_sum = 0.0f64;
    let mut progress_items = 0usize;

    for item in &inner.items {
        match item.status {
            DownloadTaskStatus::Queued => {
                queued_count += 1;
                progress_sum += item.progress_percent;
                progress_items += 1;
            },
            DownloadTaskStatus::Downloading | DownloadTaskStatus::Packaging => {
                downloading_count += 1;
                speed_bytes_per_sec += item.speed_bytes_per_sec;
                progress_sum += item.progress_percent;
                progress_items += 1;
            },
            DownloadTaskStatus::Paused => {
                paused_count += 1;
                progress_sum += item.progress_percent;
                progress_items += 1;
            },
            DownloadTaskStatus::Completed => {
                completed_count += 1;
                progress_sum += 100.0;
                progress_items += 1;
            },
            DownloadTaskStatus::Failed => {
                failed_count += 1;
            },
            DownloadTaskStatus::Cancelled => {},
        }
    }

    let overall_progress_percent = if progress_items > 0 { (progress_sum / progress_items as f64).clamp(0.0, 100.0) } else { 0.0 };

    ManagedQueueSnapshot {
        is_paused: inner.is_paused,
        is_downloading: downloading_count > 0,
        active_format: inner.active_format,
        downloads_dir: inner.downloads_dir.display().to_string(),
        queued_count,
        downloading_count,
        paused_count,
        completed_count,
        failed_count,
        overall_progress_percent,
        speed_bytes_per_sec,
        items: inner.items.clone(),
    }
}

pub fn expected_download_output_path(downloads_dir: &Path, chapter: &ChapterToDownloadSanitized) -> PathBuf {
    match chapter.download_type {
        DownloadType::Raw => RawImagesDownloader::new().make_manga_base_directory_name(downloads_dir, chapter),
        DownloadType::Cbz => {
            let dl = CbzDownloader::new();
            let base = dl.make_manga_base_directory_name(downloads_dir, chapter);
            base.join(format!("{}.cbz", dl.make_chapter_name(chapter).display()))
        },
        DownloadType::Epub => {
            let dl = EpubDownloader::new();
            let base = dl.make_manga_base_directory_name(downloads_dir, chapter);
            base.join(format!("{}.epub", dl.make_chapter_name(chapter).display()))
        },
        DownloadType::Pdf => {
            let dl = PdfDownloader::new();
            let base = dl.make_manga_base_directory_name(downloads_dir, chapter);
            base.join(format!("{}.pdf", dl.make_chapter_name(chapter).display()))
        },
    }
}

fn save_downloaded_chapter_in_db(db_path: &Path, item: &DownloadTaskItem) {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let _ = conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;",
        );
        let database = Database::new(&conn);
        let _ = database.setup();
        let _ = database.set_chapter_downloaded(SetChapterDownloaded {
            id: &item.chapter.id,
            title: &item.chapter.title,
            manga_id: &item.manga_id,
            manga_title: &item.manga_title,
            img_url: item.manga_cover_url.as_deref(),
            provider: item.provider,
        });
    }
}

impl ManagedDownloadQueue {
    pub fn spawn(
        downloads_dir: PathBuf,
        config_dir: PathBuf,
        db_path: PathBuf,
        snapshot_sink: ManagedQueueSnapshotSink,
        finished_sink: ManagedChapterFinishedSink,
    ) -> Self {
        let queue = Self::new_internal(downloads_dir, config_dir, db_path, snapshot_sink, finished_sink);
        let worker_queue = queue.clone();
        tokio::spawn(async move {
            worker_queue.run_worker_loop().await;
        });
        queue
    }

    pub fn new_without_worker(downloads_dir: PathBuf, config_dir: PathBuf, db_path: PathBuf) -> Self {
        Self::new_internal(downloads_dir, config_dir, db_path, Arc::new(|_| {}), Arc::new(|_| {}))
    }

    fn new_internal(
        downloads_dir: PathBuf,
        config_dir: PathBuf,
        db_path: PathBuf,
        snapshot_sink: ManagedQueueSnapshotSink,
        finished_sink: ManagedChapterFinishedSink,
    ) -> Self {
        let inner = ManagedQueueInner {
            items: Vec::new(),
            partials: HashMap::new(),
            is_paused: false,
            active_format: active_download_type(),
            next_seq: 1,
            downloads_dir,
            config_dir,
            db_path,
        };
        Self {
            inner: Arc::new(Mutex::new(inner)),
            notify: Arc::new(Notify::new()),
            snapshot_sink,
            finished_sink,
        }
    }

    fn emit_snapshot(&self, snapshot: ManagedQueueSnapshot) {
        (self.snapshot_sink)(snapshot);
    }

    pub fn snapshot(&self) -> ManagedQueueSnapshot {
        let guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
        build_snapshot(&guard)
    }

    pub fn enqueue_batch(&self, batch: EnqueueDownloadBatch, format_override: Option<DownloadType>) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let chosen_format = format_override.unwrap_or(guard.active_format);
            let now = now_epoch_ms();
            let default_status = if guard.is_paused { DownloadTaskStatus::Paused } else { DownloadTaskStatus::Queued };

            for chapter in batch.chapters {
                let already_active = guard.items.iter().any(|existing| {
                    existing.status.is_active()
                        && existing.provider == batch.provider
                        && existing.manga_id == batch.manga_id
                        && existing.chapter.id == chapter.id
                });
                if already_active {
                    continue;
                }

                let seq = guard.next_seq;
                guard.next_seq += 1;
                let task_id = format!("dl-{now}-{seq}");
                guard.items.push(DownloadTaskItem {
                    task_id,
                    manga_id: batch.manga_id.clone(),
                    manga_id_safe_for_download: batch.manga_id_safe_for_download.clone(),
                    manga_title: batch.manga_title.clone(),
                    manga_cover_url: batch.manga_cover_url.clone(),
                    provider: batch.provider,
                    lang: batch.lang,
                    chapter,
                    format: chosen_format,
                    status: default_status,
                    progress_percent: 0.0,
                    pages_downloaded: 0,
                    total_pages: 0,
                    bytes_downloaded: 0,
                    speed_bytes_per_sec: 0.0,
                    error_message: None,
                    output_path: None,
                    created_at_ms: now,
                    updated_at_ms: now,
                });
            }

            build_snapshot(&guard)
        };

        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn pause_all(&self) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.is_paused = true;
            let now = now_epoch_ms();
            for item in &mut guard.items {
                if matches!(item.status, DownloadTaskStatus::Queued | DownloadTaskStatus::Downloading) {
                    item.status = DownloadTaskStatus::Paused;
                    item.speed_bytes_per_sec = 0.0;
                    item.updated_at_ms = now;
                }
            }
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn resume_all(&self) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.is_paused = false;
            let now = now_epoch_ms();
            for item in &mut guard.items {
                if item.status == DownloadTaskStatus::Paused {
                    item.status = DownloadTaskStatus::Queued;
                    item.updated_at_ms = now;
                }
            }
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn pause_task(&self, task_id: &str) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let now = now_epoch_ms();
            if let Some(item) = guard.items.iter_mut().find(|i| i.task_id == task_id)
                && matches!(item.status, DownloadTaskStatus::Queued | DownloadTaskStatus::Downloading)
            {
                item.status = DownloadTaskStatus::Paused;
                item.speed_bytes_per_sec = 0.0;
                item.updated_at_ms = now;
            }
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn resume_task(&self, task_id: &str) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let now = now_epoch_ms();
            guard.is_paused = false;
            if let Some(item) = guard.items.iter_mut().find(|i| i.task_id == task_id)
                && item.status == DownloadTaskStatus::Paused
            {
                item.status = DownloadTaskStatus::Queued;
                item.updated_at_ms = now;
            }
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn cancel_task(&self, task_id: &str) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let now = now_epoch_ms();
            if let Some(item) = guard.items.iter_mut().find(|i| i.task_id == task_id)
                && item.status != DownloadTaskStatus::Completed
            {
                item.status = DownloadTaskStatus::Cancelled;
                item.speed_bytes_per_sec = 0.0;
                item.updated_at_ms = now;
            }
            guard.partials.remove(task_id);
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn cancel_all(&self) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.is_paused = false;
            let now = now_epoch_ms();
            for item in &mut guard.items {
                if item.status.is_active() {
                    item.status = DownloadTaskStatus::Cancelled;
                    item.speed_bytes_per_sec = 0.0;
                    item.updated_at_ms = now;
                }
            }
            guard.partials.clear();
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn retry_task(&self, task_id: &str) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let now = now_epoch_ms();
            let target_status = if guard.is_paused { DownloadTaskStatus::Paused } else { DownloadTaskStatus::Queued };
            if let Some(item) = guard.items.iter_mut().find(|i| i.task_id == task_id)
                && matches!(item.status, DownloadTaskStatus::Failed | DownloadTaskStatus::Cancelled)
            {
                item.status = target_status;
                item.progress_percent = 0.0;
                item.pages_downloaded = 0;
                item.total_pages = 0;
                item.bytes_downloaded = 0;
                item.speed_bytes_per_sec = 0.0;
                item.error_message = None;
                item.updated_at_ms = now;
            }
            guard.partials.remove(task_id);
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn retry_all_failed(&self) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let now = now_epoch_ms();
            let target_status = if guard.is_paused { DownloadTaskStatus::Paused } else { DownloadTaskStatus::Queued };
            let mut reset_ids = Vec::new();
            for item in &mut guard.items {
                if item.status == DownloadTaskStatus::Failed {
                    item.status = target_status;
                    item.progress_percent = 0.0;
                    item.pages_downloaded = 0;
                    item.total_pages = 0;
                    item.bytes_downloaded = 0;
                    item.speed_bytes_per_sec = 0.0;
                    item.error_message = None;
                    item.updated_at_ms = now;
                    reset_ids.push(item.task_id.clone());
                }
            }
            for id in reset_ids {
                guard.partials.remove(&id);
            }
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn remove_task(&self, task_id: &str) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.items.retain(|i| i.task_id != task_id);
            guard.partials.remove(task_id);
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        self.notify.notify_waiters();
        snapshot
    }

    pub fn clear_finished(&self) -> ManagedQueueSnapshot {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.items.retain(|i| !i.status.is_terminal());
            let active_ids: Vec<String> = guard.items.iter().map(|i| i.task_id.clone()).collect();
            guard.partials.retain(|id, _| active_ids.contains(id));
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        snapshot
    }

    pub fn set_active_format(&self, format: DownloadType) -> Result<ManagedQueueSnapshot, String> {
        let config_dir = {
            let guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.config_dir.clone()
        };

        set_active_download_type(format);
        persist_setting_in_dir(&config_dir, "download_type", download_type_toml_value(format))?;

        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            guard.active_format = format;
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot.clone());
        Ok(snapshot)
    }

    fn take_next_queued_task(&self) -> Option<(DownloadTaskItem, PartialTaskDownload, PathBuf, PathBuf, ManagedQueueSnapshot)> {
        let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
        if guard.is_paused {
            return None;
        }
        let idx = guard.items.iter().position(|i| i.status == DownloadTaskStatus::Queued)?;
        let now = now_epoch_ms();
        guard.items[idx].status = DownloadTaskStatus::Downloading;
        guard.items[idx].error_message = None;
        guard.items[idx].updated_at_ms = now;
        let item = guard.items[idx].clone();
        let partial = guard.partials.remove(&item.task_id).unwrap_or_default();
        let downloads_dir = guard.downloads_dir.clone();
        let db_path = guard.db_path.clone();
        let snapshot = build_snapshot(&guard);
        Some((item, partial, downloads_dir, db_path, snapshot))
    }

    fn mark_task_failed(&self, task_id: &str, error_message: String) {
        let snapshot = {
            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
            let now = now_epoch_ms();
            if let Some(item) = guard.items.iter_mut().find(|i| i.task_id == task_id)
                && !matches!(item.status, DownloadTaskStatus::Cancelled | DownloadTaskStatus::Completed)
            {
                item.status = DownloadTaskStatus::Failed;
                item.speed_bytes_per_sec = 0.0;
                item.error_message = Some(error_message);
                item.updated_at_ms = now;
            }
            guard.partials.remove(task_id);
            build_snapshot(&guard)
        };
        self.emit_snapshot(snapshot);
    }

    fn has_more_queued(&self) -> bool {
        let guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
        !guard.is_paused && guard.items.iter().any(|i| i.status == DownloadTaskStatus::Queued)
    }

    async fn run_worker_loop(self) {
        loop {
            let Some((mut item, mut partial, downloads_dir, db_path, start_snapshot)) = self.take_next_queued_task() else {
                self.notify.notified().await;
                continue;
            };
            self.emit_snapshot(start_snapshot);

            let quality = MangaTuiConfig::get().effective().image_quality;

            if partial.page_urls.is_empty() {
                match crate::backend::multi_provider::get_chapter_pages_url_for_provider(
                    item.provider,
                    &item.chapter.id,
                    &item.manga_id,
                    quality,
                )
                .await
                {
                    Ok(urls) if urls.is_empty() => {
                        self.mark_task_failed(&item.task_id, "No pages returned for this chapter".to_string());
                        continue;
                    },
                    Ok(urls) => {
                        partial.page_urls = urls;
                        let total = partial.page_urls.len();
                        let snap = {
                            let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
                            if let Some(task) = guard.items.iter_mut().find(|i| i.task_id == item.task_id) {
                                task.total_pages = total;
                                task.updated_at_ms = now_epoch_ms();
                            }
                            build_snapshot(&guard)
                        };
                        self.emit_snapshot(snap);
                    },
                    Err(err_msg) => {
                        self.mark_task_failed(&item.task_id, err_msg);
                        continue;
                    },
                }
            }

            let total_pages = partial.page_urls.len();
            let started_at = Instant::now();
            let session_start_bytes = partial.bytes_downloaded;
            let mut aborted_or_paused = false;
            let mut page_fetch_error: Option<String> = None;

            while partial.next_page_index < total_pages {
                enum ControlAction {
                    Continue,
                    Abort,
                    Pause,
                }

                let (control, maybe_pause_snap) = {
                    let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
                    let global_paused = guard.is_paused;
                    let item_status = guard.items.iter().find(|i| i.task_id == item.task_id).map(|i| i.status);
                    match item_status {
                        None | Some(DownloadTaskStatus::Cancelled) => {
                            guard.partials.remove(&item.task_id);
                            (ControlAction::Abort, None)
                        },
                        Some(DownloadTaskStatus::Paused) => {
                            guard.partials.insert(item.task_id.clone(), std::mem::take(&mut partial));
                            (ControlAction::Pause, Some(build_snapshot(&guard)))
                        },
                        Some(_) if global_paused => {
                            if let Some(task) = guard.items.iter_mut().find(|i| i.task_id == item.task_id) {
                                task.status = DownloadTaskStatus::Paused;
                                task.speed_bytes_per_sec = 0.0;
                                task.updated_at_ms = now_epoch_ms();
                            }
                            guard.partials.insert(item.task_id.clone(), std::mem::take(&mut partial));
                            (ControlAction::Pause, Some(build_snapshot(&guard)))
                        },
                        Some(_) => (ControlAction::Continue, None),
                    }
                };

                if let Some(snap) = maybe_pause_snap {
                    self.emit_snapshot(snap);
                }

                match control {
                    ControlAction::Abort | ControlAction::Pause => {
                        aborted_or_paused = true;
                        break;
                    },
                    ControlAction::Continue => {},
                }

                let page_meta = &partial.page_urls[partial.next_page_index];
                let url_str = page_meta.url.as_str().to_string();
                let ext = page_meta.extension.clone();

                let mut fetched_bytes = None;
                let mut last_err = String::new();
                for attempt in 0..3u32 {
                    if attempt > 0 {
                        let backoff_ms = 200 * (1u64 << (attempt - 1));
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    }
                    let res = crate::backend::multi_provider::get_raw_image_for_provider(item.provider, &url_str)
                        .await
                        .map_err(|e| e.to_string());
                    match res {
                        Ok(bytes) if !bytes.is_empty() => {
                            fetched_bytes = Some(bytes);
                            break;
                        },
                        Ok(_) => {
                            last_err = format!("Empty image payload on page {}", partial.next_page_index + 1);
                        },
                        Err(err_msg) => {
                            last_err = err_msg;
                        },
                    }
                }

                let Some(bytes) = fetched_bytes else {
                    page_fetch_error = Some(last_err);
                    break;
                };

                partial.bytes_downloaded = partial.bytes_downloaded.saturating_add(bytes.len() as u64);
                partial.downloaded_pages.push(ChapterPage {
                    bytes,
                    extension: ext,
                });
                partial.next_page_index += 1;

                let done = partial.next_page_index;
                let elapsed = started_at.elapsed().as_secs_f64().max(0.05);
                let session_bytes = partial.bytes_downloaded.saturating_sub(session_start_bytes);
                let speed = session_bytes as f64 / elapsed;
                let progress_percent = (done as f64 / total_pages as f64) * 92.0;

                let snap = {
                    let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
                    let now = now_epoch_ms();
                    if let Some(task) = guard.items.iter_mut().find(|i| i.task_id == item.task_id)
                        && task.status == DownloadTaskStatus::Downloading
                    {
                        task.pages_downloaded = done;
                        task.total_pages = total_pages;
                        task.bytes_downloaded = partial.bytes_downloaded;
                        task.speed_bytes_per_sec = speed;
                        task.progress_percent = progress_percent;
                        task.updated_at_ms = now;
                    }
                    build_snapshot(&guard)
                };
                self.emit_snapshot(snap);
            }

            if aborted_or_paused {
                continue;
            }

            if let Some(err_msg) = page_fetch_error {
                write_to_error_log(format!("Failed to download chapter {}: {err_msg}", item.chapter.title).into());
                self.mark_task_failed(&item.task_id, err_msg);
                continue;
            }

            let maybe_pack_snap = {
                let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
                let should_pack = guard
                    .items
                    .iter()
                    .find(|i| i.task_id == item.task_id)
                    .is_some_and(|i| i.status == DownloadTaskStatus::Downloading);
                if should_pack {
                    if let Some(task) = guard.items.iter_mut().find(|i| i.task_id == item.task_id) {
                        task.status = DownloadTaskStatus::Packaging;
                        task.progress_percent = 96.0;
                        task.speed_bytes_per_sec = 0.0;
                        task.updated_at_ms = now_epoch_ms();
                    }
                    Some(build_snapshot(&guard))
                } else {
                    None
                }
            };

            let Some(pack_snap) = maybe_pack_snap else {
                continue;
            };
            self.emit_snapshot(pack_snap);

            let chapter_to_download = ChapterToDownloadSanitized {
                chapter_id: item.chapter.id_safe_for_download.clone(),
                manga_id: item.manga_id_safe_for_download.clone(),
                manga_title: item.manga_title.clone().into(),
                chapter_title: item.chapter.title.clone().into(),
                chapter_number: item.chapter.chapter_number.clone(),
                volume_number: item.chapter.volume_number.clone(),
                language: item.lang,
                scanlator: item.chapter.scanlator.clone().unwrap_or_default().into(),
                download_type: item.format,
                pages: partial.downloaded_pages,
                provider: Some(item.provider),
            };
            let output_path = expected_download_output_path(&downloads_dir, &chapter_to_download);
            let output_path_str = output_path.display().to_string();
            let format = item.format;
            let save_base = downloads_dir.clone();

            let pack_res = tokio::task::spawn_blocking(move || -> Result<(), String> {
                match format {
                    DownloadType::Cbz => CbzDownloader::new()
                        .save_chapter_in_file_system(&save_base, chapter_to_download)
                        .map_err(|e| e.to_string()),
                    DownloadType::Raw => RawImagesDownloader::new()
                        .save_chapter_in_file_system(&save_base, chapter_to_download)
                        .map_err(|e| e.to_string()),
                    DownloadType::Epub => EpubDownloader::new()
                        .save_chapter_in_file_system(&save_base, chapter_to_download)
                        .map_err(|e| e.to_string()),
                    DownloadType::Pdf => PdfDownloader::new()
                        .save_chapter_in_file_system(&save_base, chapter_to_download)
                        .map_err(|e| e.to_string()),
                }
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|res| res);

            match pack_res {
                Ok(()) => {
                    item.output_path = Some(output_path_str.clone());
                    let db_item = item.clone();
                    let db_target = db_path.clone();
                    let _ = tokio::task::spawn_blocking(move || save_downloaded_chapter_in_db(&db_target, &db_item)).await;

                    let done_snap = {
                        let mut guard = self.inner.lock().expect("ManagedDownloadQueue lock poisoned");
                        guard.partials.remove(&item.task_id);
                        if let Some(task) = guard.items.iter_mut().find(|i| i.task_id == item.task_id)
                            && task.status != DownloadTaskStatus::Cancelled
                        {
                            task.status = DownloadTaskStatus::Completed;
                            task.progress_percent = 100.0;
                            task.speed_bytes_per_sec = 0.0;
                            task.output_path = Some(output_path_str.clone());
                            task.error_message = None;
                            task.updated_at_ms = now_epoch_ms();
                        }
                        build_snapshot(&guard)
                    };
                    self.emit_snapshot(done_snap);
                    (self.finished_sink)(DownloadChapterFinishedEvent {
                        task_id: item.task_id.clone(),
                        manga_id: item.manga_id.clone(),
                        manga_title: item.manga_title.clone(),
                        chapter_id: item.chapter.id.clone(),
                        chapter_number: item.chapter.chapter_number.clone(),
                        format: item.format,
                        output_path: Some(output_path_str),
                    });
                },
                Err(err_msg) => {
                    write_to_error_log(format!("Failed to package chapter {}: {err_msg}", item.chapter.title).into());
                    self.mark_task_failed(&item.task_id, err_msg);
                },
            }

            if self.has_more_queued() {
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(1200)) => {},
                    () = self.notify.notified() => {},
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::config::build_config_file_in_dir;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}_{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample_batch(chapters: Vec<(&str, &str)>) -> EnqueueDownloadBatch {
        EnqueueDownloadBatch {
            manga_id: "manga-1".to_string(),
            manga_id_safe_for_download: "manga-1".to_string(),
            manga_title: "Frieren".to_string(),
            manga_cover_url: None,
            lang: Languages::Spanish,
            provider: MangaProviders::Inmanga,
            chapters: chapters
                .into_iter()
                .map(|(id, num)| Chapter {
                    id: id.to_string(),
                    id_safe_for_download: id.to_string(),
                    manga_id: "manga-1".to_string(),
                    title: format!("Capítulo {num}"),
                    language: Languages::Spanish,
                    chapter_number: num.to_string(),
                    volume_number: None,
                    scanlator: None,
                    publication_date: None,
                })
                .collect(),
        }
    }

    #[test]
    fn managed_queue_deduplicates_pauses_resumes_cancels_and_persists_format() {
        let temp = TempDir::new("managed_queue_unit");
        let downloads_dir = temp.0.join("mangaDownloads");
        let config_dir = temp.0.join("config");
        let db_path = temp.0.join("history").join("manga-tui-history.db");
        fs::create_dir_all(&downloads_dir).unwrap();
        fs::create_dir_all(&config_dir).unwrap();
        build_config_file_in_dir(&config_dir).unwrap();

        let queue = ManagedDownloadQueue::new_without_worker(downloads_dir, config_dir.clone(), db_path);

        // 1. Enqueue 2 chapters + duplicate chapter should deduplicate active items
        let snap = queue.enqueue_batch(sample_batch(vec![("ch-1", "1"), ("ch-2", "2")]), Some(DownloadType::Cbz));
        assert_eq!(snap.queued_count, 2);
        assert_eq!(snap.items.len(), 2);

        let snap_dup = queue.enqueue_batch(sample_batch(vec![("ch-1", "1"), ("ch-3", "3")]), Some(DownloadType::Cbz));
        assert_eq!(snap_dup.queued_count, 3);
        assert_eq!(snap_dup.items.len(), 3);

        let t1 = snap_dup.items[0].task_id.clone();
        let t2 = snap_dup.items[1].task_id.clone();
        let t3 = snap_dup.items[2].task_id.clone();

        // 2. Pause / resume individual task
        let snap_p1 = queue.pause_task(&t1);
        assert_eq!(snap_p1.paused_count, 1);
        assert_eq!(snap_p1.queued_count, 2);
        assert!(!snap_p1.is_paused);

        let snap_r1 = queue.resume_task(&t1);
        assert_eq!(snap_r1.paused_count, 0);
        assert_eq!(snap_r1.queued_count, 3);

        // 3. Pause all / resume all
        let snap_pall = queue.pause_all();
        assert!(snap_pall.is_paused);
        assert_eq!(snap_pall.paused_count, 3);
        assert_eq!(snap_pall.queued_count, 0);

        let snap_rall = queue.resume_all();
        assert!(!snap_rall.is_paused);
        assert_eq!(snap_rall.paused_count, 0);
        assert_eq!(snap_rall.queued_count, 3);

        // 4. Cancel single task and retry it
        let snap_c1 = queue.cancel_task(&t1);
        assert_eq!(snap_c1.queued_count, 2);
        assert_eq!(snap_c1.items[0].status, DownloadTaskStatus::Cancelled);

        let snap_retry = queue.retry_task(&t1);
        assert_eq!(snap_retry.queued_count, 3);
        assert_eq!(snap_retry.items[0].status, DownloadTaskStatus::Queued);

        // 5. Mark one failed manually, retry all failed, cancel all, clear finished
        {
            let mut guard = queue.inner.lock().unwrap();
            guard.items.iter_mut().find(|i| i.task_id == t2).unwrap().status = DownloadTaskStatus::Failed;
        }
        let snap_retry_failed = queue.retry_all_failed();
        assert_eq!(snap_retry_failed.failed_count, 0);
        assert_eq!(snap_retry_failed.queued_count, 3);

        let snap_call = queue.cancel_all();
        assert_eq!(snap_call.queued_count, 0);
        assert!(snap_call.items.iter().all(|i| i.status == DownloadTaskStatus::Cancelled));

        let snap_cleared = queue.clear_finished();
        assert!(snap_cleared.items.is_empty());

        // 6. Set active format persists to config.toml
        let snap_pdf = queue.set_active_format(DownloadType::Pdf).unwrap();
        assert_eq!(snap_pdf.active_format, DownloadType::Pdf);
        let config_contents = fs::read_to_string(config_dir.join("config.toml")).unwrap();
        assert!(config_contents.contains(r#"download_type = "pdf""#));

        // 7. Re-enqueue after cancellation works and remove_task removes entry
        let snap_re = queue.enqueue_batch(sample_batch(vec![("ch-1", "1")]), Some(DownloadType::Epub));
        assert_eq!(snap_re.items.len(), 1);
        assert_ne!(snap_re.items[0].task_id, t3);
        let removed = queue.remove_task(&snap_re.items[0].task_id);
        assert!(removed.items.is_empty());
    }
}
