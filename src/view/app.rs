use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ::crossterm::event::KeyCode;
use crossterm::event::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Widget};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use self::feed::Feed;
use self::home::Home;
use self::manga::MangaPage;
use self::reader::MangaReader;
use self::search::{InputMode, SearchPage};
use super::widgets::Component;
use crate::backend::AppDirectories;
use crate::backend::database::{Database, DownloadedMangaWithProvider};
use crate::backend::download_queue::{
    DownloadQueueCommand, DownloadQueueEventSink, DownloadQueueProgress, DownloadQueueWorker, EnqueueDownloadBatch,
};
use crate::backend::error_log::write_to_error_log;
use crate::backend::manga_downloader::LocalDownloadsIndex;
use crate::backend::manga_provider::{
    ChapterToRead, FeedPageProvider, LatestChapter, ListOfChapters, Manga, MangaProvider, MangaProviders, MangaStatus, Pagination,
};
use crate::backend::tracker::MangaTracker;
use crate::backend::tui::{Action, Events, NewChapterNotificationGroup};
use crate::config::MangaTuiConfig;
use crate::i18n::tr;
use crate::theme::{INSTRUCTIONS_STYLE, current_theme, cycle_theme, theme};
use crate::view::pages::search::SearchScope;
use crate::view::pages::*;
use crate::view::provider_filters::TuiSearchProvider;
use crate::view::tasks::manga::{AutoDownloadItem, download_new_chapters, download_new_favorite_chapters};
use crate::view::widgets::{
    AppUpdateModal, AppUpdateModalState, AppUpdateStage, CloudModal, CloudModalOutcome, CloudModalState, DownloadQueueModal,
    ErrorModal, HelpModal, NewChapterItem, NewChaptersModal, NewChaptersModalState, SettingsModal, SettingsModalState,
    SettingsOutcome,
};

const NEW_CHAPTER_CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);

fn strip_mangaoni_type_prefix(value: &str) -> &str {
    for prefix in ["manga/", "manhwa/", "manhua/", "novela/"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return rest;
        }
    }
    value
}

fn canonical_notification_manga_key(provider: MangaProviders, manga_id: &str) -> (MangaProviders, String) {
    let canonical_id = if provider == MangaProviders::Mangaoni { strip_mangaoni_type_prefix(manga_id) } else { manga_id };
    (provider, canonical_id.to_string())
}

fn canonical_notification_chapter_id(provider: MangaProviders, chapter_id: &str) -> &str {
    if provider == MangaProviders::Mangaoni { strip_mangaoni_type_prefix(chapter_id) } else { chapter_id }
}

fn find_new_chapters_for_provider(
    provider: MangaProviders,
    latest_chapters: &[LatestChapter],
    previous_ids: &[String],
    downloaded_ids: &[String],
) -> Vec<LatestChapter> {
    let mut known_ids: HashSet<&str> = previous_ids.iter().map(|id| canonical_notification_chapter_id(provider, id)).collect();
    known_ids.extend(downloaded_ids.iter().map(|id| canonical_notification_chapter_id(provider, id)));

    latest_chapters
        .iter()
        .filter(|chapter| !known_ids.contains(canonical_notification_chapter_id(provider, &chapter.id)))
        .cloned()
        .collect()
}

#[cfg(test)]
fn find_new_chapters(latest_chapters: &[LatestChapter], previous_ids: &[String], downloaded_ids: &[String]) -> Vec<LatestChapter> {
    find_new_chapters_for_provider(MangaProviders::Mangadex, latest_chapters, previous_ids, downloaded_ids)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckableDownloadedManga {
    id: String,
    alias_ids: Vec<String>,
    title: String,
    provider: MangaProviders,
    downloaded_chapter_ids: Vec<String>,
}

fn filter_and_dedupe_downloaded_mangas(
    downloaded_mangas: Vec<DownloadedMangaWithProvider>,
    disk_index: &LocalDownloadsIndex,
    already_checked: &HashSet<(MangaProviders, String)>,
) -> Vec<CheckableDownloadedManga> {
    let mut merged = BTreeMap::<(MangaProviders, String), CheckableDownloadedManga>::new();

    for manga in downloaded_mangas {
        if !disk_index.has_any_chapter(manga.provider, &manga.downloaded_chapter_ids) {
            continue;
        }

        let key = canonical_notification_manga_key(manga.provider, &manga.id);
        if already_checked.contains(&key) {
            continue;
        }

        match merged.get_mut(&key) {
            Some(existing) => {
                if manga.provider == MangaProviders::Mangaoni && manga.id.contains('/') && !existing.id.contains('/') {
                    if !existing.alias_ids.contains(&existing.id) {
                        existing.alias_ids.push(existing.id.clone());
                    }
                    existing.id = manga.id;
                    existing.title = manga.title;
                } else if manga.id != existing.id && !existing.alias_ids.contains(&manga.id) {
                    existing.alias_ids.push(manga.id);
                }
                existing.downloaded_chapter_ids.extend(manga.downloaded_chapter_ids);
            },
            None => {
                merged.insert(key, CheckableDownloadedManga {
                    id: manga.id,
                    alias_ids: Vec::new(),
                    title: manga.title,
                    provider: manga.provider,
                    downloaded_chapter_ids: manga.downloaded_chapter_ids,
                });
            },
        }
    }

    merged.into_values().collect()
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum AppState {
    Runnning,
    Done,
}

#[derive(PartialEq, Eq, Debug, Clone, Default)]
pub struct MangaToRead {
    pub title: String,
    pub manga_id: String,
    pub list: ListOfChapters,
    pub provider: Option<MangaProviders>,
    pub offline_sources: HashMap<String, crate::backend::offline::LocalChapterSource>,
    pub is_offline_mode: bool,
}

/// The application which owns every `page` or section in manga-tui
/// its job mainly consists of displaying the selected page, passing the `manga_provider` to each
/// page handling when to quit and switching between pages based on user inputs
pub struct App<T, S>
where
    T: MangaProvider + TuiSearchProvider,
    S: MangaTracker,
{
    pub global_action_tx: UnboundedSender<Action>,
    pub global_action_rx: UnboundedReceiver<Action>,
    pub global_event_tx: UnboundedSender<Events>,
    pub global_event_rx: UnboundedReceiver<Events>,
    pub state: AppState,
    pub current_tab: SelectedPage,
    pub previous_tab: SelectedPage,
    pub manga_page: Option<MangaPage<T, S>>,
    pub manga_reader_page: Option<MangaReader<T, S>>,
    pub search_page: SearchPage<T, S>,
    pub home_page: Home<T>,
    pub feed_page: Feed<T>,
    manga_provider: Arc<T>,
    pub active_provider: MangaProviders,
    pub is_checking_provider: bool,
    manga_tracker: Option<S>,
    error_message: Option<String>,
    new_chapters_state: Option<NewChaptersModalState>,
    favorite_status_toast: Option<(String, Instant)>,
    pub download_queue_tx: UnboundedSender<DownloadQueueCommand>,
    pub download_queue_progress: Option<DownloadQueueProgress>,
    pub queue_status_toast: Option<(String, Instant)>,
    pub is_queue_modal_open: bool,
    pub is_help_modal_open: bool,
    pub is_settings_modal_open: bool,
    pub settings_state: SettingsModalState,
    pub is_cloud_modal_open: bool,
    pub cloud_state: CloudModalState,
    cloud_task: Option<tokio::task::JoinHandle<()>>,
    pub app_update_state: Option<AppUpdateModalState>,
    pub pending_update_install: Option<crate::backend::updater::PreparedUpdate>,
    app_update_task: Option<tokio::task::JoinHandle<()>>,
    last_new_chapter_check: Instant,
    // The picker is what decides how big a image needs to be rendered depending on the user's
    // terminal font size and the graphics it supports
    // if the terminal doesn't support any graphics protocol the picker is `None`
    picker: Option<Picker>,
    pub needs_terminal_clear: bool,
    top_tabs_area: Rect,
    tab_hit_boxes: Vec<(u16, u16, SelectedPage)>,
}

impl<T, S> Component for App<T, S>
where
    T: MangaProvider + TuiSearchProvider,
    S: MangaTracker,
{
    type Actions = Action;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        if let Some(manga_reader) = &mut self.manga_reader_page
            && self.current_tab == SelectedPage::ReaderTab
        {
            manga_reader.render(area, frame);
        } else {
            let main_layout = Layout::vertical([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1)]);

            let [top_tabs_area, page_area, status_bar_area] = main_layout.areas(area);

            self.top_tabs_area = top_tabs_area;
            self.render_top_tabs(top_tabs_area, frame.buffer_mut());

            self.render_pages(page_area, frame);

            self.render_status_bar(status_bar_area, frame.buffer_mut());
        }

        if let Some(message) = self.error_message.as_ref().cloned() {
            self.render_modal_error_message(area, frame.buffer_mut(), &message);
        } else if let Some(update_state) = self.app_update_state.as_ref() {
            AppUpdateModal::new(update_state).render(area, frame.buffer_mut());
        } else if let Some(state) = self.new_chapters_state.as_ref() {
            NewChaptersModal::new(state).render(area, frame.buffer_mut());
        } else if self.is_queue_modal_open {
            DownloadQueueModal::new(self.download_queue_progress.as_ref()).render(area, frame.buffer_mut());
        } else if self.is_help_modal_open {
            HelpModal::new().render(area, frame.buffer_mut());
        } else if self.is_settings_modal_open {
            SettingsModal::new(&self.settings_state).render(area, frame.buffer_mut());
        } else if self.is_cloud_modal_open {
            CloudModal::new(&self.cloud_state).render(area, frame.buffer_mut());
        }
    }

    fn handle_events(&mut self, events: Events) {
        match events {
            Events::Key(key_event) => self.handle_key_events(key_event),
            Events::Mouse(mouse_event) => self.handle_mouse_events(mouse_event),
            Events::GoToMangaPage(manga) => self.go_to_manga_page(manga),
            Events::GoToOfflineMangaPage(offline_manga) => self.go_to_offline_manga_page(offline_manga),
            Events::ReadChapter(chapter_response, manga_to_read) => {
                self.go_to_read_chapter(chapter_response, manga_to_read, self.manga_tracker.clone())
            },
            Events::GoSearchPage => {
                self.go_search_page();
            },
            Events::GoToHome => self.go_to_home(),
            Events::GoFeedPage => self.go_feed_page(),

            Events::Error(message) => self.display_error_message(message),
            Events::CheckForNewDownloadedChapters => {
                self.last_new_chapter_check = Instant::now();
                self.check_for_new_downloaded_chapters();
            },
            Events::NewChaptersNotification(groups) => {
                let items = groups
                    .into_iter()
                    .map(|g| NewChapterItem {
                        manga_id: g.manga_id,
                        manga_title: g.manga_title,
                        provider: g.provider,
                        chapter_numbers: g.chapter_numbers,
                        chapters: g.chapters,
                        selected: true,
                    })
                    .collect();
                self.new_chapters_state = Some(NewChaptersModalState::new(items));
            },
            Events::NewChaptersDownloadFinished(result) => match result {
                Ok(count) => {
                    let msg = if crate::i18n::is_es() {
                        format!("Descargado(s) {count} nuevo(s) capítulo(s). Pulsa <q> para cerrar.")
                    } else {
                        format!("Downloaded {count} new chapter(s). Press <q> to close.")
                    };
                    if let Some(state) = &mut self.new_chapters_state {
                        state.status_message = Some(msg);
                        state.items.clear();
                    } else {
                        self.new_chapters_state = Some(NewChaptersModalState {
                            items: Vec::new(),
                            cursor: 0,
                            status_message: Some(msg),
                        });
                    }
                },
                Err(error) => self.display_error_message(error),
            },
            Events::AutoDownloadedFavorites { count, details } => {
                let summary = if details.is_empty() {
                    format!("{count} {}", tr("capítulo(s)", "chapter(s)"))
                } else if details.len() <= 2 {
                    details.join(", ")
                } else {
                    format!("{}, {} (+{} {})", details[0], details[1], details.len() - 2, tr("más", "more"))
                };
                self.favorite_status_toast =
                    Some((format!("★ {}: {summary}", tr("Auto-descargado", "Auto-downloaded")), Instant::now()));
            },
            Events::EnqueueDownloadBatch(batch) => {
                let count = batch.chapters.len();
                let title = batch.manga_title.clone();
                self.download_queue_tx.send(DownloadQueueCommand::Enqueue(batch)).ok();
                self.queue_status_toast = Some((
                    format!("+ {} {count} {} {title}", tr("En cola", "Queued"), tr("capítulo(s) de", "chapter(s) of")),
                    Instant::now(),
                ));
            },
            Events::DownloadQueueProgress(progress) => {
                self.download_queue_progress = Some(progress);
            },
            Events::DownloadQueueFinished(total) => {
                self.download_queue_progress = None;
                self.queue_status_toast = Some((
                    format!(
                        "✓ {} {total} {}",
                        tr("Finalizada la descarga de", "Finished downloading"),
                        tr("capítulo(s)", "chapter(s)")
                    ),
                    Instant::now(),
                ));
            },
            Events::DownloadQueueChapterFinished {
                manga_id,
                chapter_id,
            } => {
                if let Some(page) = &mut self.manga_page {
                    if page.manga.id == manga_id {
                        page.set_chapter_finished_downloading(&chapter_id);
                    }
                }
            },

            Events::GoBackMangaPage => {
                if self.current_tab == SelectedPage::ReaderTab
                    && let Some(manga_reader) = &mut self.manga_reader_page
                {
                    manga_reader.clean_up();
                    if self.manga_page.is_some() {
                        self.current_tab = SelectedPage::MangaTab;
                    } else {
                        match self.previous_tab {
                            SelectedPage::Home => self.go_to_home(),
                            SelectedPage::Feed => self.go_feed_page(),
                            SelectedPage::Search => self.go_search_page(),
                            _ => self.go_search_page(),
                        }
                    }
                } else if self.current_tab == SelectedPage::MangaTab {
                    if let Some(manga_page) = self.manga_page.as_mut() {
                        manga_page.clean_up();
                    }
                    match self.previous_tab {
                        SelectedPage::Home => self.go_to_home(),
                        SelectedPage::Feed => self.go_feed_page(),
                        SelectedPage::Search => self.go_search_page(),
                        _ => self.go_search_page(),
                    }
                }
            },
            Events::SearchAllProviders(query) => {
                if let Some(manga_page) = self.manga_page.as_mut() {
                    manga_page.clean_up();
                    self.manga_page = None;
                }
                if let Some(manga_reader) = self.manga_reader_page.as_mut() {
                    manga_reader.clean_up();
                    self.manga_reader_page = None;
                }
                self.feed_page.clean_up();
                self.current_tab = SelectedPage::Search;
                self.search_page.search_with_query_all_providers(&query);
            },
            Events::ProviderCheckTrying(candidate) => {
                self.is_checking_provider = true;
                self.active_provider = candidate;
            },
            Events::ImageProtocolChanged(proto) => {
                self.apply_image_protocol(proto);
            },
            Events::AppUpdateAvailable(update) => {
                self.is_settings_modal_open = false;
                self.app_update_state = Some(AppUpdateModalState::available(update));
            },
            Events::AppUpdateCheckStatus { message, is_error } => {
                if self.is_settings_modal_open {
                    self.settings_state.status_message = Some(message);
                    self.settings_state.status_is_error = is_error;
                } else {
                    self.queue_status_toast = Some((message, Instant::now()));
                }
            },
            Events::AppUpdateDownloadProgress(progress) => {
                if let Some(state) = &mut self.app_update_state {
                    state.stage = match progress.stage {
                        crate::backend::updater::UpdateProgressStage::Downloading => AppUpdateStage::Downloading {
                            downloaded_bytes: progress.downloaded_bytes,
                            total_bytes: progress.total_bytes,
                        },
                        crate::backend::updater::UpdateProgressStage::Verifying => AppUpdateStage::Verifying,
                        crate::backend::updater::UpdateProgressStage::Preparing => AppUpdateStage::Applying,
                    };
                }
            },
            Events::AppUpdateReadyToInstall(prepared) => {
                self.app_update_task = None;
                if let Some(state) = &mut self.app_update_state {
                    state.stage = AppUpdateStage::Applying;
                }
                self.pending_update_install = Some(prepared);
                self.quit();
            },
            Events::AppUpdateFailed(err) => {
                self.app_update_task = None;
                if let Some(state) = &mut self.app_update_state {
                    state.stage = AppUpdateStage::Error(err);
                } else {
                    self.display_error_message(err);
                }
            },
            Events::CloudSyncProgress(progress) => {
                self.cloud_state.progress = Some(progress);
            },
            Events::CloudSyncFinished(result) => {
                self.cloud_task = None;
                let ok = result.is_ok();
                self.cloud_state.on_sync_finished(result);
                if ok {
                    self.home_page.refresh_stats();
                    self.search_page.refresh_personal_discovery();
                    if self.current_tab == SelectedPage::Feed {
                        self.feed_page.init_search();
                    }
                }
            },
            Events::CloudOAuthFinished(result) => {
                self.cloud_task = None;
                self.cloud_state.on_oauth_finished(result);
            },
            Events::ProviderCheckFinished {
                active_provider,
                switched_from,
                all_failed,
            } => {
                self.is_checking_provider = false;
                self.active_provider = active_provider;
                self.home_page.set_active_provider(active_provider);
                self.search_page.set_active_provider(active_provider);

                if let Some(original) = switched_from {
                    self.home_page.clean_up();
                    self.home_page.init_search();
                    let msg = if crate::i18n::is_es() {
                        format!("✓ {original} no disponible → usando {active_provider}")
                    } else {
                        format!("✓ {original} unavailable → switched to {active_provider}")
                    };
                    self.queue_status_toast = Some((msg, Instant::now()));
                } else if all_failed {
                    let msg = if crate::i18n::is_es() {
                        format!("⚠ {active_provider} sin conexión · Pulsa <o> → <4> para leer tus Descargas Offline")
                    } else {
                        format!("⚠ {active_provider} unreachable · Press <o> → <4> to read Offline Downloads")
                    };
                    self.favorite_status_toast = Some((msg, Instant::now()));
                }
            },
            _ => {},
        }
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.state = AppState::Done;
            },
        }
    }

    fn clean_up(&mut self) {}
}

impl<T, S> App<T, S>
where
    T: MangaProvider + TuiSearchProvider,
    S: MangaTracker,
{
    pub fn new(
        api_client: T,
        manga_tracker: Option<S>,
        picker: Option<Picker>,
        filters_state: T::FiltersHandler,
        filter_widget: T::Widget,
    ) -> Self {
        let (global_action_tx, global_action_rx) = unbounded_channel::<Action>();
        let (global_event_tx, global_event_rx) = unbounded_channel::<Events>();

        global_event_tx.send(Events::GoToHome).ok();

        let which_provider = api_client.name();

        let provider = Arc::new(api_client);

        App {
            picker,
            current_tab: SelectedPage::default(),
            previous_tab: SelectedPage::default(),
            search_page: SearchPage::new(
                picker,
                Arc::clone(&provider),
                manga_tracker.clone(),
                filters_state,
                filter_widget,
                Pagination::from_first_page(match which_provider {
                    MangaProviders::Mangadex => 10,
                    MangaProviders::Weebcentral => 24,
                    MangaProviders::Mangapill => 50,
                    MangaProviders::Inmanga => 10,
                    MangaProviders::Mangaoni => 20,
                    MangaProviders::Leercapitulo => 20,
                    MangaProviders::Comizy => 24,
                }),
            )
            .with_global_sender(global_event_tx.clone()),
            feed_page: Feed::new()
                .with_global_sender(global_event_tx.clone())
                .with_api_client(Arc::clone(&provider)),
            home_page: Home::new(picker, Arc::clone(&provider)).with_global_sender(global_event_tx.clone()),
            manga_page: None,
            manga_reader_page: None,
            global_action_tx,
            global_action_rx,
            global_event_tx: global_event_tx.clone(),
            global_event_rx,
            manga_tracker,
            state: AppState::Runnning,
            error_message: None,
            new_chapters_state: None,
            favorite_status_toast: None,
            download_queue_tx: {
                let (queue_tx, queue_rx) = tokio::sync::mpsc::unbounded_channel();
                let tx = global_event_tx.clone();
                let sink: DownloadQueueEventSink = Arc::new(move |e| {
                    tx.send(Events::from(e)).ok();
                });
                let worker = DownloadQueueWorker::new(Arc::clone(&provider), sink, queue_rx);
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(worker.run());
                }
                queue_tx
            },
            download_queue_progress: None,
            queue_status_toast: None,
            is_queue_modal_open: false,
            is_help_modal_open: false,
            is_settings_modal_open: false,
            settings_state: SettingsModalState::new(),
            is_cloud_modal_open: false,
            cloud_state: CloudModalState::new(),
            cloud_task: None,
            app_update_state: None,
            pending_update_install: None,
            app_update_task: None,
            last_new_chapter_check: Instant::now(),
            manga_provider: Arc::clone(&provider),
            active_provider: which_provider,
            is_checking_provider: false,
            needs_terminal_clear: false,
            top_tabs_area: Rect::default(),
            tab_hit_boxes: Vec::new(),
        }
    }

    pub fn take_needs_terminal_clear(&mut self) -> bool {
        let mut should_clear = self.needs_terminal_clear;
        self.needs_terminal_clear = false;
        if let Some(reader) = self.manga_reader_page.as_mut()
            && reader.needs_terminal_clear
        {
            reader.needs_terminal_clear = false;
            should_clear = true;
        }
        should_clear
    }

    pub fn spawn_startup_provider_check(&mut self) {
        let requested_provider = self.manga_provider.name();
        let event_tx = self.global_event_tx.clone();
        let image_quality = MangaTuiConfig::get().image_quality;

        if let Some(candidates) = crate::backend::multi_provider::spanish_provider_candidates(requested_provider) {
            self.is_checking_provider = true;
            tokio::spawn(async move {
                let mut available_provider = None;
                for candidate in candidates {
                    let _ = event_tx.send(Events::ProviderCheckTrying(candidate));
                    match tokio::time::timeout(
                        Duration::from_secs(15),
                        crate::backend::multi_provider::check_spanish_provider(candidate, image_quality),
                    )
                    .await
                    {
                        Ok(Ok(())) => {
                            crate::backend::multi_provider::POOL.health.record_success(candidate);
                            available_provider = Some(candidate);
                            break;
                        },
                        Ok(Err(err)) => {
                            write_to_error_log(format!("El proveedor {candidate} no está disponible al iniciar ({err}).").into());
                            crate::backend::multi_provider::POOL.health.record_startup_check_failure(candidate, &err);
                        },
                        Err(_) => {
                            write_to_error_log(format!("El proveedor {candidate} no está disponible al iniciar (timeout).").into());
                            crate::backend::multi_provider::POOL
                                .health
                                .record_startup_check_failure(candidate, "startup check timed out");
                        },
                    }
                }

                match available_provider {
                    Some(active) => {
                        let switched_from = if active != requested_provider { Some(requested_provider) } else { None };
                        let _ = event_tx.send(Events::ProviderCheckFinished {
                            active_provider: active,
                            switched_from,
                            all_failed: false,
                        });
                    },
                    None => {
                        let _ = event_tx.send(Events::ProviderCheckFinished {
                            active_provider: requested_provider,
                            switched_from: None,
                            all_failed: true,
                        });
                    },
                }
            });
        } else if requested_provider == MangaProviders::Mangadex {
            self.is_checking_provider = true;
            tokio::spawn(async move {
                let _ = event_tx.send(Events::ProviderCheckTrying(MangaProviders::Mangadex));
                let is_ok = matches!(
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        crate::backend::multi_provider::POOL.mangadex.check_status(),
                    )
                    .await,
                    Ok(Ok(response)) if response.status() == reqwest::StatusCode::OK
                );
                if is_ok {
                    crate::backend::multi_provider::POOL.health.record_success(MangaProviders::Mangadex);
                } else {
                    crate::backend::multi_provider::POOL
                        .health
                        .record_startup_check_failure(MangaProviders::Mangadex, "mangadex status check failed");
                }
                let _ = event_tx.send(Events::ProviderCheckFinished {
                    active_provider: MangaProviders::Mangadex,
                    switched_from: None,
                    all_failed: !is_ok,
                });
            });
        }
    }

    pub fn spawn_startup_update_check(&self) {
        if !MangaTuiConfig::get().check_new_updates {
            return;
        }
        let event_tx = self.global_event_tx.clone();
        tokio::spawn(async move {
            let current = crate::backend::updater::BuildIdentity::current();
            let install_kind = crate::backend::updater::detect_current_install_kind();
            let client = crate::backend::updater::UpdateClient::default_github();
            match client.check_for_update(&current, install_kind).await {
                Ok(Some(update)) => {
                    let _ = event_tx.send(Events::AppUpdateAvailable(update));
                },
                Ok(None) => {},
                Err(err) => {
                    write_to_error_log(format!("Startup update check failed: {err}").into());
                },
            }
        });
    }

    pub fn spawn_manual_update_check(&mut self) {
        let event_tx = self.global_event_tx.clone();
        tokio::spawn(async move {
            let current = crate::backend::updater::BuildIdentity::current();
            let install_kind = crate::backend::updater::detect_current_install_kind();
            let client = crate::backend::updater::UpdateClient::default_github();
            match client.check_for_update(&current, install_kind).await {
                Ok(Some(update)) => {
                    let _ = event_tx.send(Events::AppUpdateAvailable(update));
                },
                Ok(None) => {
                    let current_label = if current.commit_short != "unknown" {
                        format!("v{} ({})", current.version, current.commit_short)
                    } else {
                        format!("v{}", current.version)
                    };
                    let msg = if crate::i18n::is_es() {
                        format!("✓ Ya estás en la última versión ({current_label})")
                    } else {
                        format!("✓ Already on the latest version ({current_label})")
                    };
                    let _ = event_tx.send(Events::AppUpdateCheckStatus {
                        message: msg,
                        is_error: false,
                    });
                },
                Err(err) => {
                    let _ = event_tx.send(Events::AppUpdateCheckStatus {
                        message: format!("⚠ {err}"),
                        is_error: true,
                    });
                },
            }
        });
    }

    fn start_app_update_download(&mut self) {
        let Some(state) = &mut self.app_update_state else {
            return;
        };
        if let Some(handle) = self.app_update_task.take() {
            handle.abort();
        }

        let Some(asset) = state.available.download_asset.as_ref() else {
            let url = state.available.html_url.clone();
            #[cfg(not(test))]
            let _ = open::that(&url);
            state.stage = AppUpdateStage::Error(format!(
                "{}: {url}",
                tr(
                    "No hay paquete directo compatible; abriendo release en navegador",
                    "No compatible direct package found; opening release in browser",
                )
            ));
            return;
        };

        let update = state.available.clone();
        state.stage = AppUpdateStage::Downloading {
            downloaded_bytes: 0,
            total_bytes: (asset.size > 0).then_some(asset.size),
        };

        let event_tx = self.global_event_tx.clone();
        let target_exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("manga-tui.exe"));

        let handle = tokio::spawn(async move {
            let client = crate::backend::updater::UpdateClient::default_github();
            let tx_progress = event_tx.clone();
            match client
                .download_and_verify(&update, target_exe, move |progress| {
                    let _ = tx_progress.send(Events::AppUpdateDownloadProgress(progress));
                })
                .await
            {
                Ok(prepared) => {
                    let _ = event_tx.send(Events::AppUpdateReadyToInstall(prepared));
                },
                Err(err) => {
                    let _ = event_tx.send(Events::AppUpdateFailed(err));
                },
            }
        });
        self.app_update_task = Some(handle);
    }

    pub fn open_cloud_modal(&mut self) {
        self.is_settings_modal_open = false;
        self.cloud_state.refresh_from_disk();
        self.is_cloud_modal_open = true;
    }

    pub fn spawn_cloud_oauth_login(&mut self, client_id: String, client_secret: Option<String>) {
        if let Some(handle) = self.cloud_task.take() {
            handle.abort();
        }
        self.cloud_state.is_busy = true;
        self.cloud_state.progress = Some(crate::backend::cloud::CloudSyncProgress {
            step_label: tr(
                "Abriendo navegador para autorizar Google Drive (OAuth 2.0)...",
                "Opening browser to authorize Google Drive (OAuth 2.0)...",
            )
            .to_string(),
            completed_items: 1,
            total_items: 3,
        });

        let event_tx = self.global_event_tx.clone();
        let handle = tokio::spawn(async move {
            let client = crate::backend::cloud::GoogleDriveClient::new();
            let tx_url = event_tx.clone();
            let token_res = client
                .run_loopback_oauth_flow(&client_id, client_secret.as_deref(), move |auth_url| {
                    #[cfg(not(test))]
                    let _ = open::that(auth_url);
                    #[cfg(test)]
                    let _ = auth_url;
                    let _ = tx_url.send(Events::CloudSyncProgress(crate::backend::cloud::CloudSyncProgress {
                        step_label: tr(
                            "Esperando que completes el inicio de sesión de Google en tu navegador...",
                            "Waiting for you to complete Google sign-in in your browser...",
                        )
                        .to_string(),
                        completed_items: 2,
                        total_items: 3,
                    }));
                })
                .await;

            match token_res {
                Ok(tokens) => {
                    let mut state = crate::backend::cloud::CloudConnectionState::load();
                    state.backend = Some(crate::backend::cloud::CloudBackendKind::GoogleDriveApi);
                    state.save_oauth_tokens(
                        &client_id,
                        client_secret.as_deref(),
                        &tokens.access_token,
                        tokens.refresh_token.as_deref(),
                    );
                    if let Ok(about) = client.get_about_user(&tokens.access_token).await
                        && let Some(user) = about.user
                    {
                        state.account_email = user.email_address;
                        state.account_display_name = user.display_name;
                    }
                    let _ = state.save();
                    let _ = event_tx.send(Events::CloudOAuthFinished(Ok(state)));
                },
                Err(err) => {
                    let _ = event_tx.send(Events::CloudOAuthFinished(Err(err)));
                },
            }
        });
        self.cloud_task = Some(handle);
    }

    pub fn spawn_cloud_sync(&mut self, mode: crate::backend::cloud::CloudSyncMode) {
        if let Some(handle) = self.cloud_task.take() {
            handle.abort();
        }
        self.cloud_state.is_busy = true;
        self.cloud_state.progress = Some(crate::backend::cloud::CloudSyncProgress {
            step_label: format!("{} ({})...", tr("Iniciando sincronización", "Starting sync"), mode.label()),
            completed_items: 0,
            total_items: 4,
        });

        let mut state = self.cloud_state.connection.clone();
        let event_tx = self.global_event_tx.clone();
        let handle = tokio::spawn(async move {
            let client = crate::backend::cloud::GoogleDriveClient::new();
            let tx_progress = event_tx.clone();
            let res = crate::backend::cloud::run_cloud_sync(&mut state, mode, &client, move |progress| {
                let _ = tx_progress.send(Events::CloudSyncProgress(progress));
            })
            .await;
            let _ = event_tx.send(Events::CloudSyncFinished(res));
        });
        self.cloud_task = Some(handle);
    }

    pub fn check_for_new_downloaded_chapters(&self) {
        let provider = Arc::clone(&self.manga_provider);
        let provider_name = provider.name();
        let event_tx = self.global_event_tx.clone();

        tokio::spawn(async move {
            // 1. Get all favorite mangas across all providers
            let all_favorites = match Database::get_connection().and_then(|conn| {
                let db = Database::new(&conn);
                db.get_all_favorite_mangas()
            }) {
                Ok(favs) => favs,
                Err(e) => {
                    write_to_error_log(format!("Failed to query favorites: {e}").into());
                    Vec::new()
                },
            };

            // 2. Get downloaded mangas across all providers (for modal notification of non-favorites)
            let downloaded_mangas = match Database::get_connection().and_then(|connection| {
                let database = Database::new(&connection);
                database.get_all_downloaded_mangas()
            }) {
                Ok(mangas) => mangas,
                Err(error) => {
                    event_tx.send(Events::Error(error.to_string())).ok();
                    Vec::new()
                },
            };

            let mut groups = Vec::new();
            let mut auto_download_items = Vec::new();
            let mut checked_mangas = HashSet::new();

            // Check all favorites across all providers
            for fav in all_favorites {
                checked_mangas.insert(canonical_notification_manga_key(fav.provider, &fav.manga_id));

                let manga_details = match crate::backend::multi_provider::get_manga_by_id_for_provider(fav.provider, &fav.manga_id)
                    .await
                {
                    Ok(details) => details,
                    Err(error) => {
                        write_to_error_log(format!("Could not check favorite {} on {}: {error}", fav.title, fav.provider).into());
                        continue;
                    },
                };
                if manga_details.status != MangaStatus::Ongoing {
                    continue;
                }

                let latest_chapters =
                    match crate::backend::multi_provider::get_latest_chapters_for_provider(fav.provider, &fav.manga_id).await {
                        Ok(chapters) if !chapters.is_empty() => chapters,
                        _ => continue,
                    };

                let state_result = Database::get_connection().and_then(|connection| {
                    let database = Database::new(&connection);
                    let dismissed_ids = database.get_dismissed_chapter_ids(&fav.manga_id, fav.provider)?;
                    let downloaded_ids = database.get_downloaded_chapter_ids(&fav.manga_id, fav.provider)?;
                    Ok((dismissed_ids, downloaded_ids))
                });

                let (dismissed_ids, downloaded_ids) = match state_result {
                    Ok(ids) => ids,
                    Err(error) => {
                        write_to_error_log(format!("Database error checking favorite chapters for {}: {error}", fav.title).into());
                        continue;
                    },
                };

                let newly_available =
                    find_new_chapters_for_provider(fav.provider, &latest_chapters, &dismissed_ids, &downloaded_ids);

                if !newly_available.is_empty() {
                    let chapters_to_auto_download =
                        if dismissed_ids.is_empty() && downloaded_ids.is_empty() && newly_available.len() > 1 {
                            let to_download = vec![newly_available[0].clone()];
                            let older_ids: Vec<String> = newly_available[1..].iter().map(|c| c.id.clone()).collect();
                            if let Ok(connection) = Database::get_connection() {
                                let _ =
                                    Database::new(&connection).save_dismissed_chapter_ids(&fav.manga_id, fav.provider, &older_ids);
                            }
                            to_download
                        } else {
                            newly_available
                        };

                    for chapter in chapters_to_auto_download {
                        auto_download_items.push(AutoDownloadItem {
                            provider: fav.provider,
                            manga_id: fav.manga_id.clone(),
                            chapter,
                        });
                    }
                }
            }

            // Check downloaded non-favorites across all providers whose files still exist on disk
            let checkable_mangas = if downloaded_mangas.is_empty() {
                Vec::new()
            } else {
                let download_dir = AppDirectories::MangaDownloads.get_full_path();
                let disk_index = tokio::task::spawn_blocking(move || LocalDownloadsIndex::scan(&download_dir))
                    .await
                    .ok()
                    .flatten();
                match disk_index {
                    Some(index) => filter_and_dedupe_downloaded_mangas(downloaded_mangas, &index, &checked_mangas),
                    None => {
                        write_to_error_log(
                            "Download directory is not accessible; skipping downloaded chapter notifications".into(),
                        );
                        Vec::new()
                    },
                }
            };

            for manga in checkable_mangas {
                let manga_details_res = if manga.provider == provider_name {
                    provider.get_manga_by_id(&manga.id).await.map_err(|e| e.to_string())
                } else {
                    crate::backend::multi_provider::get_manga_by_id_for_provider(manga.provider, &manga.id)
                        .await
                        .map_err(|e| e.to_string())
                };

                let manga_details = match manga_details_res {
                    Ok(details) => details,
                    Err(error) => {
                        write_to_error_log(
                            format!("Could not check downloaded {} on {}: {error}", manga.title, manga.provider).into(),
                        );
                        continue;
                    },
                };
                if manga_details.status != MangaStatus::Ongoing {
                    continue;
                }

                let latest_chapters_res = if manga.provider == provider_name {
                    provider.get_latest_chapters(&manga.id).await.map_err(|e| e.to_string())
                } else {
                    crate::backend::multi_provider::get_latest_chapters_for_provider(manga.provider, &manga.id)
                        .await
                        .map_err(|e| e.to_string())
                };

                let latest_chapters = match latest_chapters_res {
                    Ok(chapters) if !chapters.is_empty() => chapters,
                    Ok(_) => continue,
                    Err(error) => {
                        write_to_error_log(
                            format!("Could not check for new chapters of {} on {}: {error}", manga.title, manga.provider).into(),
                        );
                        continue;
                    },
                };

                let state_result = Database::get_connection().and_then(|connection| {
                    let database = Database::new(&connection);
                    let mut dismissed_ids = database.get_dismissed_chapter_ids(&manga.id, manga.provider)?;
                    for alias_id in &manga.alias_ids {
                        dismissed_ids.extend(database.get_dismissed_chapter_ids(alias_id, manga.provider)?);
                    }
                    Ok(find_new_chapters_for_provider(
                        manga.provider,
                        &latest_chapters,
                        &dismissed_ids,
                        &manga.downloaded_chapter_ids,
                    ))
                });

                let newly_available = match state_result {
                    Ok(chapters) => chapters,
                    Err(error) => {
                        event_tx.send(Events::Error(error.to_string())).ok();
                        return;
                    },
                };

                if !newly_available.is_empty() {
                    let chapter_numbers = newly_available
                        .iter()
                        .map(|chapter| chapter.chapter_number.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    groups.push(NewChapterNotificationGroup {
                        manga_id: manga.id,
                        manga_title: manga.title,
                        provider: manga.provider,
                        chapter_numbers,
                        chapters: newly_available,
                    });
                }
            }

            if !auto_download_items.is_empty() {
                let config = MangaTuiConfig::get().effective();
                let tx = event_tx.clone();
                tokio::spawn(async move {
                    let result = download_new_favorite_chapters(auto_download_items, config).await;
                    match result {
                        Ok((count, details)) if count > 0 => {
                            tx.send(Events::AutoDownloadedFavorites { count, details }).ok();
                        },
                        Ok(_) => {},
                        Err(e) => {
                            tx.send(Events::Error(format!("Auto-download of favorite chapters failed: {e}"))).ok();
                        },
                    }
                });
            }

            if !groups.is_empty() {
                event_tx.send(Events::NewChaptersNotification(groups)).ok();
            }
        });
    }

    pub fn render_top_tabs(&mut self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let t = theme();
        let bar_bg = t.bar_bg;
        let inactive_pill_bg = t.chip_bg;
        let active_pill_bg = t.accent_primary;
        let brand_bg = t.accent_secondary;

        let top_row = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };

        // Fill top row background
        for x in top_row.left()..top_row.right() {
            buf[(x, top_row.y)].set_bg(bar_bg).set_symbol(" ");
        }

        self.tab_hit_boxes.clear();

        // Right-aligned status / provider / download badges on top row
        let mut right_spans: Vec<Span<'static>> = Vec::new();

        if let Some(progress) = &self.download_queue_progress {
            if progress.is_downloading {
                let short_title: String = progress.current_manga_title.chars().take(14).collect();
                let dl_text = format!(
                    " ⬇ [{}/{}] {} {} {} ({:.0}%) <D> ",
                    progress.completed_in_batch + 1,
                    progress.total_in_batch,
                    short_title,
                    tr("Cap.", "Ch."),
                    progress.current_chapter_number,
                    progress.current_page_percentage
                );
                right_spans.push(Span::styled("", Style::default().fg(t.accent_primary).bg(bar_bg)));
                right_spans.push(Span::styled(dl_text, Style::default().fg(Color::Black).bg(t.accent_primary).bold()));
                right_spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
            }
        } else if let Some((msg, time)) = &self.queue_status_toast
            && time.elapsed().as_secs() < 8
        {
            right_spans.push(Span::styled("", Style::default().fg(t.status_success).bg(bar_bg)));
            right_spans.push(Span::styled(format!(" {msg} "), Style::default().fg(Color::Black).bg(t.status_success).bold()));
            right_spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
        } else if let Some((msg, time)) = &self.favorite_status_toast
            && time.elapsed().as_secs() < 15
        {
            right_spans.push(Span::styled("", Style::default().fg(t.accent_highlight).bg(bar_bg)));
            right_spans.push(Span::styled(format!(" {msg} "), Style::default().fg(Color::Black).bg(t.accent_highlight).bold()));
            right_spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
        }

        // Active Provider / Scope badge
        let active_provider = if self.current_tab == SelectedPage::MangaTab {
            self.manga_page.as_ref().and_then(|p| p.manga.provider).unwrap_or(self.active_provider)
        } else {
            self.active_provider
        };

        let pill_color = t.title_chip_primary_bg;
        if self.current_tab == SelectedPage::Search && self.search_page.search_scope == SearchScope::AllProviders {
            let lang_tag = match self.search_page.language_filter {
                crate::view::pages::search::SearchLanguageFilter::All => "ALL",
                crate::view::pages::search::SearchLanguageFilter::Spanish => "ES",
                crate::view::pages::search::SearchLanguageFilter::English => "EN",
            };
            right_spans.push(Span::styled("", Style::default().fg(pill_color).bg(bar_bg)));
            right_spans.push(Span::styled(" 🌐 ", Style::default().fg(t.accent_primary).bg(pill_color).bold()));
            right_spans.push(Span::styled(
                format!("{} ({lang_tag}) ", tr("Todos", "All")),
                Style::default().fg(t.text_primary).bg(pill_color).bold(),
            ));
        } else {
            let health = crate::backend::multi_provider::get_provider_health_status(active_provider);
            let dot_color = if self.is_checking_provider && active_provider == self.active_provider {
                t.status_warning
            } else {
                match health {
                    crate::backend::multi_provider::ProviderHealthStatus::Healthy => t.status_success,
                    crate::backend::multi_provider::ProviderHealthStatus::Degraded => t.status_warning,
                    crate::backend::multi_provider::ProviderHealthStatus::CircuitOpen => t.status_error,
                }
            };
            right_spans.push(Span::styled("", Style::default().fg(pill_color).bg(bar_bg)));
            right_spans.push(Span::styled(" ● ", Style::default().fg(dot_color).bg(pill_color).bold()));
            right_spans
                .push(Span::styled(format!("{active_provider:?} "), Style::default().fg(t.text_primary).bg(pill_color).bold()));
        }

        let right_line = Line::from(right_spans);
        let right_w = (right_line.width() as u16).min(top_row.width);
        let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w)]).areas(top_row);

        Paragraph::new(right_line).alignment(Alignment::Right).render(right_area, buf);

        let max_left_x = left_area.right();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut cursor_x = left_area.x;

        let brand_span = Span::styled(" ⛩ MANGA-TUI ", Style::default().fg(Color::Black).bg(brand_bg).bold());
        let arrow_span = Span::styled(" ", Style::default().fg(brand_bg).bg(bar_bg));
        cursor_x = cursor_x.saturating_add((brand_span.width() + arrow_span.width()) as u16);
        spans.push(brand_span);
        spans.push(arrow_span);

        let mut tab_defs: Vec<(SelectedPage, String)> = vec![
            (SelectedPage::Home, tr(" 🏠 Inicio <u/F1> ", " 🏠 Home <u/F1> ").to_string()),
            (SelectedPage::Search, tr(" 🔍 Buscar <i/F2> ", " 🔍 Search <i/F2> ").to_string()),
            (SelectedPage::Feed, tr(" 📚 Seguimiento <o/F3> ", " 📚 Feed <o/F3> ").to_string()),
        ];

        if self.current_tab == SelectedPage::MangaTab {
            let manga_label = self
                .manga_page
                .as_ref()
                .map(|p| {
                    let short: String = p.manga.title.chars().take(18).collect();
                    if p.manga.title.chars().count() > 18 { format!(" 📖 {short}… ") } else { format!(" 📖 {short} ") }
                })
                .unwrap_or_else(|| tr(" 📖 Ficha ", " 📖 Manga ").to_string());
            tab_defs.push((SelectedPage::MangaTab, manga_label));
        }

        let mut active_tab_range: Option<(u16, u16)> = None;

        for (page, label) in tab_defs {
            let is_active = self.current_tab == page;
            let pill_bg = if is_active { active_pill_bg } else { inactive_pill_bg };
            let pill_fg = if is_active { Color::Black } else { t.text_primary };

            let left_cap = Span::styled("", Style::default().fg(pill_bg).bg(bar_bg));
            let mut text_style = Style::default().fg(pill_fg).bg(pill_bg);
            if is_active {
                text_style = text_style.bold();
            }
            let text_span = Span::styled(label, text_style);
            let right_cap = Span::styled("", Style::default().fg(pill_bg).bg(bar_bg));

            let start_x = cursor_x;
            let pill_w = (left_cap.width() + text_span.width() + right_cap.width()) as u16;
            let end_x = start_x.saturating_add(pill_w);
            let clamped_end = end_x.min(max_left_x);

            if start_x < max_left_x && clamped_end > start_x {
                self.tab_hit_boxes.push((start_x, clamped_end, page));
                if is_active {
                    active_tab_range = Some((start_x, clamped_end));
                }
            }

            spans.push(left_cap);
            spans.push(text_span);
            spans.push(right_cap);
            spans.push(Span::styled(" ", Style::default().bg(bar_bg)));
            cursor_x = end_x.saturating_add(1);
        }

        Paragraph::new(Line::from(spans)).render(left_area, buf);

        // Second row: subtle horizontal rule with active tab highlight bar
        if area.height >= 2 {
            let divider_y = area.y + 1;
            let rule_style = Style::default().fg(t.border_muted);
            let active_rule_style = Style::default().fg(active_pill_bg).bold();
            for x in area.left()..area.right() {
                if let Some((start_x, end_x)) = active_tab_range
                    && x >= start_x
                    && x < end_x
                {
                    buf[(x, divider_y)].set_symbol("━").set_style(active_rule_style);
                } else {
                    buf[(x, divider_y)].set_symbol("─").set_style(rule_style);
                }
            }
        }
    }

    pub fn render_status_bar(&mut self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let t = theme();
        let bar_bg = t.bar_bg;
        let chip_bg = t.chip_bg;

        for x in area.left()..area.right() {
            buf[(x, area.y)].set_bg(bar_bg).set_symbol(" ");
        }

        let is_typing = self.search_page.input_mode == InputMode::Typing
            || self.search_page.is_typing_filter()
            || self.feed_page.is_typing()
            || self.manga_page.as_ref().is_some_and(|p| p.is_typing_chapter_search());

        let (mode_label, mode_color) = if self.is_help_modal_open {
            (tr(" ❓ AYUDA ", " ❓ HELP "), t.accent_primary)
        } else if self.is_settings_modal_open {
            (tr(" ⚙  AJUSTES ", " ⚙  SETTINGS "), t.accent_primary)
        } else if self.is_cloud_modal_open {
            (tr(" ☁  NUBE ", " ☁  CLOUD "), t.accent_primary)
        } else if self.is_queue_modal_open {
            (tr(" ⬇ COLA ", " ⬇ QUEUE "), t.accent_primary)
        } else if self.app_update_state.is_some() {
            (tr(" ⬆ ACTUALIZACIÓN ", " ⬆ UPDATE "), t.accent_highlight)
        } else if is_typing {
            (tr(" ⌨  ESCRIBIENDO ", " ⌨  TYPING "), t.accent_highlight)
        } else if self.search_page.is_filter_open() {
            (tr(" ⚙  FILTROS ", " ⚙  FILTERS "), t.accent_secondary)
        } else {
            match self.current_tab {
                SelectedPage::Home => (tr(" 🏠 INICIO ", " 🏠 HOME "), t.accent_primary),
                SelectedPage::Search => (tr(" 🔍 EXPLORAR ", " 🔍 SEARCH "), t.accent_primary),
                SelectedPage::Feed => (tr(" 📚 BIBLIOTECA ", " 📚 LIBRARY "), t.status_success),
                SelectedPage::MangaTab => (tr(" 📖 FICHA ", " 📖 MANGA "), t.accent_secondary),
                SelectedPage::ReaderTab => (tr(" 👁 LECTOR ", " 👁 READER "), t.accent_highlight),
            }
        };

        let shortcuts: Vec<(&str, &str)> = if is_typing {
            vec![
                ("Enter", tr("Confirmar", "Confirm")),
                ("↑/↓", tr("Historial", "History")),
                ("Esc", tr("Salir texto", "Exit input")),
            ]
        } else {
            match self.current_tab {
                SelectedPage::Home => vec![
                    ("w/b", tr("Popular", "Popular")),
                    ("h/l", tr("Recientes", "Recent")),
                    ("r/Enter", tr("Abrir", "Open")),
                    ("v/V", tr("Escena/Cine", "Scene/TV")),
                    ("i", tr("Buscar", "Search")),
                    ("o", tr("Biblioteca", "Library")),
                    ("g", "GitHub"),
                ],
                SelectedPage::Search => vec![
                    ("s", tr("Buscar", "Search")),
                    ("j/k", tr("Mover", "Move")),
                    ("r", tr("Abrir", "Open")),
                    ("P", tr("Fuente", "Source")),
                    ("p", tr("Planear", "Plan")),
                    ("w/b", tr("Pág.", "Page")),
                    ("Tab", tr("Alcance", "Scope")),
                    ("l", tr("Idioma", "Lang")),
                    ("f", tr("Filtros", "Filters")),
                ],
                SelectedPage::Feed => match self.feed_page.tabs {
                    crate::view::widgets::feed::FeedTabs::Timeline => vec![
                        ("Tab/1-4", tr("Pestaña", "Tab")),
                        ("j/k", tr("Mover", "Move")),
                        ("Enter", tr("Leer", "Read")),
                        ("r", tr("Ficha", "Manga")),
                        ("d", tr("Descargar", "Download")),
                        ("f", tr("Favorito", "Favorite")),
                        ("s", tr("Filtrar", "Filter")),
                        ("R", tr("Actualizar", "Refresh")),
                    ],
                    crate::view::widgets::feed::FeedTabs::Downloads => vec![
                        ("Tab/1-4", tr("Pestaña", "Tab")),
                        ("j/k", tr("Mover", "Move")),
                        ("Enter/r", tr("Abrir", "Open")),
                        ("v", tr("Visor Web", "Web Viewer")),
                        ("s", tr("Filtrar", "Filter")),
                        ("f", tr("Favorito", "Favorite")),
                        ("R", tr("Escanear", "Rescan")),
                    ],
                    _ => vec![
                        ("Tab/1-4", tr("Pestaña", "Tab")),
                        ("j/k", tr("Mover", "Move")),
                        ("r/Enter", tr("Abrir", "Open")),
                        ("w/b", tr("Pág.", "Page")),
                        ("s", tr("Filtrar", "Filter")),
                        ("f", tr("Favorito", "Favorite")),
                        ("d", tr("Quitar", "Remove")),
                        ("X", tr("Vaciar", "Clear")),
                    ],
                },
                SelectedPage::MangaTab => {
                    if let Some(page) = &self.manga_page {
                        if page.is_provider_switcher_open {
                            vec![
                                ("j/k", tr("Elegir", "Select")),
                                ("Enter", tr("Migrar", "Switch")),
                                ("Esc/P", tr("Cerrar", "Close")),
                            ]
                        } else {
                            vec![
                                ("j/k", tr("Capítulos", "Chapters")),
                                ("/", tr("Ir a cap.", "Jump")),
                                ("r/v", tr("Leer/Web", "Read/Web")),
                                ("x", tr("Leído", "Read")),
                                ("Space", tr("Selec.", "Select")),
                                ("d/a", tr("Descargar", "Download")),
                                ("f", tr("Favorito", "Favorite")),
                                ("t", tr("Orden", "Order")),
                                ("l", tr("Idioma", "Lang")),
                                ("P", tr("Proveedor", "Provider")),
                            ]
                        }
                    } else {
                        vec![]
                    }
                },
                SelectedPage::ReaderTab => vec![
                    ("j/k", tr("Pág.", "Page")),
                    ("g/G", tr("Inicio/Fin", "Start/End")),
                    ("w/b", tr("Cap.", "Chap.")),
                    ("v", tr("Visor Web", "Web HD")),
                    ("z/f", tr("Sin HUD", "Zen")),
                    ("Esc/q", tr("Volver", "Back")),
                ],
            }
        };

        let active_theme_label = current_theme().label();
        let right_spans: Vec<Span<'static>> = if area.width >= 100 {
            vec![
                Span::styled(" c ", Style::default().fg(t.accent_primary).bg(chip_bg).bold()),
                Span::styled(format!(" {}  ", tr("Ajustes", "Settings")), Style::default().fg(t.text_muted).bg(bar_bg)),
                Span::styled(" C ", Style::default().fg(t.accent_highlight).bg(chip_bg).bold()),
                Span::styled(format!(" {}  ", tr("Nube", "Cloud")), Style::default().fg(t.text_muted).bg(bar_bg)),
                Span::styled(" T ", Style::default().fg(t.accent_secondary).bg(chip_bg).bold()),
                Span::styled(format!(" {active_theme_label}  "), Style::default().fg(t.text_muted).bg(bar_bg)),
                Span::styled(" D ", Style::default().fg(t.accent_primary).bg(chip_bg).bold()),
                Span::styled(format!(" {}  ", tr("Cola", "Queue")), Style::default().fg(t.text_muted).bg(bar_bg)),
                Span::styled(" n ", Style::default().fg(t.accent_highlight).bg(chip_bg).bold()),
                Span::styled(format!(" {}  ", tr("Ayuda", "Help")), Style::default().fg(t.text_muted).bg(bar_bg)),
                Span::styled(" ^C ", Style::default().fg(t.status_error).bg(chip_bg).bold()),
                Span::styled(format!(" {} ", tr("Salir", "Quit")), Style::default().fg(t.text_muted).bg(bar_bg)),
            ]
        } else {
            vec![
                Span::styled(" c ", Style::default().fg(t.accent_primary).bg(chip_bg).bold()),
                Span::styled(" ", Style::default().bg(bar_bg)),
                Span::styled(" C ", Style::default().fg(t.accent_highlight).bg(chip_bg).bold()),
                Span::styled(" ", Style::default().bg(bar_bg)),
                Span::styled(" T ", Style::default().fg(t.accent_secondary).bg(chip_bg).bold()),
                Span::styled(" ", Style::default().bg(bar_bg)),
                Span::styled(" D ", Style::default().fg(t.accent_primary).bg(chip_bg).bold()),
                Span::styled(" ", Style::default().bg(bar_bg)),
                Span::styled(" n ", Style::default().fg(t.accent_highlight).bg(chip_bg).bold()),
                Span::styled(" ", Style::default().bg(bar_bg)),
                Span::styled(" ^C ", Style::default().fg(t.status_error).bg(chip_bg).bold()),
            ]
        };

        let right_line = Line::from(right_spans);
        let right_w = (right_line.width() as u16).min(area.width);
        let [left_area, right_area] = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w)]).areas(area);

        Paragraph::new(right_line).alignment(Alignment::Right).render(right_area, buf);

        let avail_left = left_area.width.saturating_sub(u16::from(right_w > 0 && left_area.width > 2)) as usize;
        if avail_left == 0 {
            return;
        }

        let mut left_spans: Vec<Span<'static>> = Vec::new();
        let mut used_w = 0usize;

        let mode_span = Span::styled(mode_label.to_string(), Style::default().fg(Color::Black).bg(mode_color).bold());
        let arrow_span = Span::styled(" ", Style::default().fg(mode_color).bg(bar_bg));
        let prefix_w = mode_span.width() + arrow_span.width();
        if prefix_w <= avail_left {
            used_w += prefix_w;
            left_spans.push(mode_span);
            left_spans.push(arrow_span);
        }

        for (key, action) in shortcuts {
            let key_span = Span::styled(format!(" {key} "), Style::default().fg(t.accent_primary).bg(chip_bg).bold());
            let act_span = Span::styled(format!(" {action}  "), Style::default().fg(t.text_muted).bg(bar_bg));
            let w = key_span.width() + act_span.width();
            if used_w + w <= avail_left {
                used_w += w;
                left_spans.push(key_span);
                left_spans.push(act_span);
            }
        }

        Paragraph::new(Line::from(left_spans)).render(left_area, buf);
    }

    fn render_modal_error_message(&self, area: Rect, buf: &mut Buffer, message: &str) {
        ErrorModal::new(message).render(area, buf);
    }

    fn display_error_message(&mut self, error_message: String) {
        self.error_message = Some(error_message);
    }

    fn close_modal(&mut self) {
        self.error_message = None;
        if let Some(handle) = self.app_update_task.take() {
            handle.abort();
        }
        if let Some(handle) = self.cloud_task.take() {
            handle.abort();
        }
        self.cloud_state.is_busy = false;
        self.cloud_state.progress = None;
        self.app_update_state = None;
        self.new_chapters_state = None;
        self.is_queue_modal_open = false;
        self.is_help_modal_open = false;
        self.is_settings_modal_open = false;
        self.is_cloud_modal_open = false;
    }

    fn dismiss_new_chapters_notification(&mut self) -> Result<(), String> {
        if let Some(state) = &self.new_chapters_state {
            let mut chapter_ids_by_manga = std::collections::BTreeMap::<(MangaProviders, String), Vec<String>>::new();
            for item in &state.items {
                for chapter in &item.chapters {
                    chapter_ids_by_manga
                        .entry((item.provider, item.manga_id.clone()))
                        .or_default()
                        .push(chapter.id.clone());
                }
            }

            let connection = Database::get_connection().map_err(|error| error.to_string())?;
            let database = Database::new(&connection);
            for ((provider, manga_id), chapter_ids) in chapter_ids_by_manga {
                database
                    .save_dismissed_chapter_ids(&manga_id, provider, &chapter_ids)
                    .map_err(|error| error.to_string())?;
            }
        }

        Ok(())
    }

    fn is_displaying_modal(&self) -> bool {
        self.error_message.is_some()
            || self.app_update_state.is_some()
            || self.new_chapters_state.is_some()
            || self.is_queue_modal_open
            || self.is_help_modal_open
            || self.is_settings_modal_open
            || self.is_cloud_modal_open
    }

    /// Returns `true` when any global modal or page-level popup overlay is currently open.
    pub fn is_overlay_open(&self) -> bool {
        self.is_displaying_modal()
            || self.home_page.is_overlay_open()
            || self.search_page.is_filter_open()
            || self.feed_page.is_overlay_open()
            || self.manga_page.as_ref().is_some_and(|page| {
                page.is_provider_switcher_open
                    || page.is_list_languages_open
                    || page.is_chapter_search_open
                    || page.is_download_modal_open()
            })
    }

    fn apply_image_protocol(&mut self, proto: crate::config::ImageProtocol) {
        let new_picker = crate::backend::tui::picker_for_protocol(proto);
        self.picker = new_picker;
        self.home_page.set_picker(new_picker);
        self.search_page.set_picker(new_picker);
        if let Some(manga_page) = self.manga_page.as_mut() {
            manga_page.set_picker(new_picker);
        }
        if let (Some(picker), Some(reader_page)) = (new_picker, self.manga_reader_page.as_mut())
            && reader_page.picker_protocol_type() != picker.protocol_type
        {
            reader_page.set_picker(picker);
        }
        self.needs_terminal_clear = true;
    }

    pub fn render_pages(&mut self, area: Rect, frame: &mut Frame<'_>) {
        match self.current_tab {
            SelectedPage::Search => self.render_search_page(area, frame),
            SelectedPage::MangaTab => self.render_manga_page(area, frame),
            SelectedPage::Home => self.render_home_page(area, frame),
            SelectedPage::Feed => self.render_feed_page(area, frame),
            // Reader tab should be on full screen
            SelectedPage::ReaderTab => {},
        }
    }

    fn render_feed_page(&mut self, area: Rect, frame: &mut Frame<'_>) {
        self.feed_page.render(area, frame);
    }

    pub fn render_search_page(&mut self, area: Rect, frame: &mut Frame<'_>) {
        self.search_page.render(area, frame);
    }

    pub fn render_manga_page(&mut self, area: Rect, frame: &mut Frame<'_>) {
        if let Some(page) = self.manga_page.as_mut() {
            page.render(area, frame);
        }
    }

    pub fn render_home_page(&mut self, area: Rect, frame: &mut Frame<'_>) {
        self.home_page.render(area, frame);
    }

    /// This method ensures a chapter is bookmarked on quit as well
    /// only if auto_bookmark = true
    fn auto_bookmark_on_quit(&mut self) {
        if let Some(reader_page) = self.manga_reader_page.as_mut()
            && reader_page.auto_bookmark
        {
            reader_page.bookmark_current_chapter();
        }
    }

    fn quit(&mut self) {
        self.auto_bookmark_on_quit();
        self.global_action_tx.send(Action::Quit).ok();
    }

    fn key_events_modal(&mut self, key_event: KeyEvent) {
        if key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL {
            self.quit();
            return;
        }

        if self.error_message.is_some() {
            if matches!(key_event.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.error_message = None;
            }
            return;
        }

        if self.is_help_modal_open {
            if matches!(
                key_event.code,
                KeyCode::Char('n')
                    | KeyCode::Char('N')
                    | KeyCode::Char('?')
                    | KeyCode::Char('¿')
                    | KeyCode::F(4)
                    | KeyCode::Char('q')
                    | KeyCode::Esc
            ) {
                self.is_help_modal_open = false;
            }
            return;
        }

        if self.is_settings_modal_open {
            match self.settings_state.handle_key_event(key_event) {
                SettingsOutcome::Close => {
                    self.is_settings_modal_open = false;
                },
                SettingsOutcome::ProviderChanged(new_provider) => {
                    self.is_checking_provider = false;
                    self.active_provider = new_provider;
                    self.home_page.set_active_provider(new_provider);
                    self.search_page.set_active_provider(new_provider);
                    self.home_page.clean_up();
                    if self.current_tab == SelectedPage::Home {
                        self.home_page.init_search();
                    }
                },
                SettingsOutcome::ChapterLanguageChanged(new_lang) => {
                    if let Some(manga_page) = self.manga_page.as_mut() {
                        manga_page.apply_preferred_language(new_lang);
                    }
                },
                SettingsOutcome::UiLanguageChanged(_) => {
                    self.search_page.refresh_personal_discovery();
                },
                SettingsOutcome::ImageProtocolChanged(proto) => {
                    self.apply_image_protocol(proto);
                },
                SettingsOutcome::CheckForUpdates => {
                    self.spawn_manual_update_check();
                },
                SettingsOutcome::OpenCloudSync => {
                    self.open_cloud_modal();
                },
                _ => {},
            }
            return;
        }

        if self.is_cloud_modal_open {
            match self.cloud_state.handle_key_event(key_event) {
                CloudModalOutcome::Close => {
                    self.is_cloud_modal_open = false;
                },
                CloudModalOutcome::StartOAuthLogin {
                    client_id,
                    client_secret,
                } => {
                    self.spawn_cloud_oauth_login(client_id, client_secret);
                },
                CloudModalOutcome::StartSync(mode) => {
                    self.spawn_cloud_sync(mode);
                },
                CloudModalOutcome::CancelRunningTask => {
                    if let Some(handle) = self.cloud_task.take() {
                        handle.abort();
                    }
                },
                CloudModalOutcome::None => {},
            }
            return;
        }

        if self.is_queue_modal_open {
            match key_event.code {
                KeyCode::Char('c') => {
                    self.download_queue_tx.send(DownloadQueueCommand::CancelAll).ok();
                    self.download_queue_progress = None;
                    self.is_queue_modal_open = false;
                    self.queue_status_toast = Some((tr("Descargas canceladas", "Downloads cancelled").to_string(), Instant::now()));
                },
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('D') => {
                    self.is_queue_modal_open = false;
                },
                _ => {},
            }
            return;
        }

        if let Some(state) = &self.app_update_state {
            match &state.stage {
                AppUpdateStage::Available => match key_event.code {
                    KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.start_app_update_download();
                    },
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        let url = state.available.html_url.clone();
                        #[cfg(not(test))]
                        let _ = open::that(url);
                        #[cfg(test)]
                        let _ = url;
                    },
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        self.app_update_state = None;
                    },
                    _ => {},
                },
                AppUpdateStage::Downloading { .. } => {
                    if matches!(key_event.code, KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc) {
                        if let Some(handle) = self.app_update_task.take() {
                            handle.abort();
                        }
                        self.app_update_state = None;
                    }
                },
                AppUpdateStage::Verifying | AppUpdateStage::Applying => {},
                AppUpdateStage::Error(_) => match key_event.code {
                    KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => {
                        self.start_app_update_download();
                    },
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        let url = state.available.html_url.clone();
                        #[cfg(not(test))]
                        let _ = open::that(url);
                        #[cfg(test)]
                        let _ = url;
                    },
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        self.app_update_state = None;
                    },
                    _ => {},
                },
            }
            return;
        }

        if let Some(state) = &mut self.new_chapters_state {
            if state.status_message.is_none() {
                match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.cursor = state.cursor.saturating_sub(1);
                    },
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.cursor + 1 < state.items.len() {
                            state.cursor += 1;
                        }
                    },
                    KeyCode::Char(' ') => {
                        if let Some(item) = state.items.get_mut(state.cursor) {
                            item.selected = !item.selected;
                        }
                    },
                    KeyCode::Char('a') | KeyCode::Char('v') => {
                        let all_selected = state.items.iter().all(|i| i.selected);
                        for item in &mut state.items {
                            item.selected = !all_selected;
                        }
                    },
                    KeyCode::Char('y') | KeyCode::Char('d') | KeyCode::Enter => {
                        self.download_new_chapters();
                    },
                    KeyCode::Char('q') | KeyCode::Esc => {
                        let dismissal_result = self.dismiss_new_chapters_notification();
                        self.close_modal();
                        if let Err(error) = dismissal_result {
                            self.display_error_message(error);
                        }
                    },
                    _ => {},
                }
                return;
            } else {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        self.close_modal();
                    },
                    _ => {},
                }
                return;
            }
        }

        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.close_modal();
            },
            _ => {},
        }
    }

    fn download_new_chapters(&mut self) {
        if let Some(state) = &mut self.new_chapters_state {
            let mut items_to_download = Vec::new();
            let mut unselected_by_manga = std::collections::BTreeMap::<(MangaProviders, String), Vec<String>>::new();

            for item in &state.items {
                if item.selected {
                    for chapter in &item.chapters {
                        items_to_download.push(AutoDownloadItem {
                            provider: item.provider,
                            manga_id: item.manga_id.clone(),
                            chapter: chapter.clone(),
                        });
                    }
                } else {
                    for chapter in &item.chapters {
                        unselected_by_manga
                            .entry((item.provider, item.manga_id.clone()))
                            .or_default()
                            .push(chapter.id.clone());
                    }
                }
            }

            if items_to_download.is_empty() {
                return;
            }

            if !unselected_by_manga.is_empty() {
                if let Ok(connection) = Database::get_connection() {
                    let database = Database::new(&connection);
                    for ((provider, manga_id), chapter_ids) in unselected_by_manga {
                        let _ = database.save_dismissed_chapter_ids(&manga_id, provider, &chapter_ids);
                    }
                }
            }

            let count = items_to_download.len();
            state.status_message = Some(if crate::i18n::is_es() {
                format!("Descargando {count} nuevo(s) capítulo(s)... Pulsa <q> para cerrar.")
            } else {
                format!("Downloading {count} new chapter(s)... Press <q> to close.")
            });

            let config = MangaTuiConfig::get().effective();
            let event_tx = self.global_event_tx.clone();

            tokio::spawn(async move {
                let result = download_new_chapters(items_to_download, config).await;
                event_tx.send(Events::NewChaptersDownloadFinished(result)).ok();
            });
        }
    }

    fn handle_key_events(&mut self, key_event: KeyEvent) {
        if key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL {
            self.quit();
            return;
        }

        if self.is_displaying_modal() {
            return;
        }

        let is_typing = self.search_page.input_mode == InputMode::Typing
            || self.search_page.is_typing_filter()
            || self.feed_page.is_typing()
            || self.manga_page.as_ref().is_some_and(|p| p.is_typing_chapter_search());

        if !is_typing && key_event.code == KeyCode::Char('T') && self.current_tab != SelectedPage::ReaderTab {
            cycle_theme();
            let _ = crate::view::widgets::settings::persist_setting_to_disk(
                "theme",
                crate::view::widgets::settings::theme_toml_value(current_theme()),
            );
            return;
        }

        let manga_page_blocks_c = self.current_tab == SelectedPage::MangaTab
            && self
                .manga_page
                .as_ref()
                .is_some_and(|p| !p.selected_chapters.is_empty() || p.is_provider_switcher_open || p.is_list_languages_open);

        if !is_typing
            && !self.search_page.is_filter_open()
            && self.current_tab != SelectedPage::ReaderTab
            && (key_event.code == KeyCode::F(5) || (key_event.code == KeyCode::Char('c') && !manga_page_blocks_c))
        {
            self.settings_state.status_message = None;
            self.settings_state.status_is_error = false;
            self.is_settings_modal_open = true;
            return;
        }

        if !is_typing
            && !self.search_page.is_filter_open()
            && self.current_tab != SelectedPage::ReaderTab
            && matches!(key_event.code, KeyCode::Char('C') | KeyCode::F(6))
        {
            self.open_cloud_modal();
            return;
        }

        if !is_typing && key_event.code == KeyCode::Char('D') && self.current_tab != SelectedPage::ReaderTab {
            self.is_queue_modal_open = true;
            return;
        }

        if !is_typing
            && matches!(
                key_event.code,
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('?') | KeyCode::Char('¿') | KeyCode::F(4)
            )
            && self.current_tab != SelectedPage::ReaderTab
        {
            self.is_help_modal_open = true;
            return;
        }

        if !is_typing && !self.search_page.is_filter_open() {
            match key_event.code {
                KeyCode::Char('u') | KeyCode::F(1) if self.current_tab != SelectedPage::ReaderTab => {
                    self.global_event_tx.send(Events::GoToHome).ok();
                },
                KeyCode::Char('i') | KeyCode::F(2) if self.current_tab != SelectedPage::ReaderTab => {
                    self.global_event_tx.send(Events::GoSearchPage).ok();
                },
                KeyCode::Char('o') | KeyCode::F(3) if self.current_tab != SelectedPage::ReaderTab => {
                    self.global_event_tx.send(Events::GoFeedPage).ok();
                },

                _ => {},
            }
        }
    }

    fn handle_mouse_events(&mut self, mouse_event: MouseEvent) {
        if self.current_tab == SelectedPage::ReaderTab || self.is_displaying_modal() {
            return;
        }
        if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
            let pos = Position::new(mouse_event.column, mouse_event.row);
            if !self.top_tabs_area.contains(pos) {
                return;
            }
            for &(start_x, end_x, page) in &self.tab_hit_boxes {
                if mouse_event.column >= start_x && mouse_event.column < end_x {
                    match page {
                        SelectedPage::Home => {
                            self.global_event_tx.send(Events::GoToHome).ok();
                        },
                        SelectedPage::Search => {
                            self.global_event_tx.send(Events::GoSearchPage).ok();
                        },
                        SelectedPage::Feed => {
                            self.global_event_tx.send(Events::GoFeedPage).ok();
                        },
                        _ => {},
                    }
                    return;
                }
            }
            if self.tab_hit_boxes.is_empty() {
                let num_tabs: u16 = if self.current_tab == SelectedPage::MangaTab { 4 } else { 3 };
                let tab_width = self.top_tabs_area.width / num_tabs;
                if tab_width == 0 {
                    return;
                }
                let tab_index = ((mouse_event.column.saturating_sub(self.top_tabs_area.x)) / tab_width).min(num_tabs - 1);
                match tab_index {
                    0 => self.global_event_tx.send(Events::GoToHome).ok(),
                    1 => self.global_event_tx.send(Events::GoSearchPage).ok(),
                    2 => self.global_event_tx.send(Events::GoFeedPage).ok(),
                    _ => None,
                };
            }
        }
    }

    fn go_search_page(&mut self) {
        if self.manga_page.is_some() {
            self.manga_page.as_mut().unwrap().clean_up();
            self.manga_page = None;
        }
        self.feed_page.clean_up();
        self.search_page.refresh_personal_discovery();
        self.current_tab = SelectedPage::Search;
    }

    fn open_manga_page(&mut self, manga_page: MangaPage<T, S>) {
        if self.current_tab != SelectedPage::MangaTab && self.current_tab != SelectedPage::ReaderTab {
            self.previous_tab = self.current_tab;
        }

        if self.manga_reader_page.is_some() {
            self.manga_reader_page.as_mut().unwrap().clean_up();
            self.manga_reader_page = None;
        }

        self.feed_page.clean_up();

        self.current_tab = SelectedPage::MangaTab;
        self.manga_page = Some(manga_page);
    }

    fn go_to_manga_page(&mut self, manga: Manga) {
        let config = MangaTuiConfig::get();
        let manga_page = MangaPage::new(manga, self.picker, Arc::clone(&self.manga_provider))
            .with_global_sender(self.global_event_tx.clone())
            .auto_bookmark(config.auto_bookmark)
            .with_manga_tracker(self.manga_tracker.clone());
        self.open_manga_page(manga_page);
    }

    fn go_to_offline_manga_page(&mut self, offline_manga: crate::backend::offline::OfflineManga) {
        let config = MangaTuiConfig::get();
        let manga_page = MangaPage::new_offline(offline_manga, self.picker, Arc::clone(&self.manga_provider))
            .with_global_sender(self.global_event_tx.clone())
            .auto_bookmark(config.auto_bookmark)
            .with_manga_tracker(self.manga_tracker.clone());
        self.open_manga_page(manga_page);
    }

    fn go_to_read_chapter(&mut self, chapter_to_read: ChapterToRead, manga_to_read: MangaToRead, manga_tracker: Option<S>) {
        let Some(picker) = self.picker.as_ref().cloned() else {
            let expected_pages = if manga_to_read.is_offline_mode { 0 } else { chapter_to_read.pages_url.len() };
            let local_source = manga_to_read.offline_sources.get(&chapter_to_read.id).cloned().or_else(|| {
                crate::view::pages::reader::LocalChapterSource::find_for_chapter(
                    &manga_to_read.title,
                    &chapter_to_read.id,
                    expected_pages,
                )
            });
            let session = crate::backend::web_viewer::WebViewerSession {
                manga_id: manga_to_read.manga_id,
                manga_title: manga_to_read.title,
                manga_cover_url: None,
                provider: manga_to_read.provider.unwrap_or_else(|| self.manga_provider.name()),
                chapter: chapter_to_read,
                list_of_chapters: manga_to_read.list,
                local_source,
                offline_sources: manga_to_read.offline_sources,
                is_offline_mode: manga_to_read.is_offline_mode,
            };
            let tx = self.global_event_tx.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::backend::web_viewer::open_chapter_in_browser(session).await {
                    let _ = tx.send(Events::Error(e));
                }
            });
            self.queue_status_toast = Some((
                tr(
                    "🌐 Protocolo TUI desactivado: abriendo capítulo en Visor Web HD...",
                    "🌐 TUI protocol disabled: opening chapter in Web HD Viewer...",
                )
                .to_string(),
                Instant::now(),
            ));
            return;
        };
        self.home_page.clean_up();
        self.feed_page.clean_up();
        self.current_tab = SelectedPage::ReaderTab;

        let mut manga_reader = MangaReader::new(chapter_to_read, manga_to_read.manga_id, picker, Arc::clone(&self.manga_provider))
            .with_global_sender(self.global_event_tx.clone())
            .with_list_of_chapters(manga_to_read.list)
            .with_offline_sources(manga_to_read.offline_sources)
            .with_offline_mode(manga_to_read.is_offline_mode)
            .with_manga_title(manga_to_read.title)
            .with_manga_tracker(manga_tracker)
            .with_provider(manga_to_read.provider);

        let config = MangaTuiConfig::get();

        if config.auto_bookmark {
            manga_reader.set_auto_bookmark();
        }

        manga_reader.init_fetching_pages();

        self.manga_reader_page = Some(manga_reader);
    }

    fn go_to_home(&mut self) {
        if self.manga_page.is_some() {
            self.manga_page.as_mut().unwrap().clean_up();
            self.manga_page = None;
        }

        self.feed_page.clean_up();

        if self.home_page.require_search() {
            self.home_page.init_search();
        } else {
            self.home_page.refresh_stats();
        }

        self.current_tab = SelectedPage::Home;
    }

    fn go_feed_page(&mut self) {
        if self.manga_page.is_some() {
            self.manga_page.as_mut().unwrap().clean_up();
            self.manga_page = None;
        }
        self.feed_page.init_search();
        self.current_tab = SelectedPage::Feed;
    }

    pub fn process_global_event(&mut self, event: Events) {
        let was_displaying_modal = self.is_displaying_modal();

        self.handle_events(event.clone());

        if event == Events::Tick && self.last_new_chapter_check.elapsed() >= NEW_CHAPTER_CHECK_INTERVAL {
            self.global_event_tx.send(Events::CheckForNewDownloadedChapters).ok();
        }

        // If a modal was already open, route key events to the modal handler;
        // if handle_events just opened a modal on this event, consume the event without closing it immediately.
        if was_displaying_modal {
            if let Events::Key(key_event) = event {
                self.key_events_modal(key_event);
            }
            return;
        } else if self.is_displaying_modal() {
            return;
        }

        match self.current_tab {
            SelectedPage::Search => {
                self.search_page.handle_events(event);
            },
            SelectedPage::MangaTab => {
                if let Some(manga_page) = self.manga_page.as_mut() {
                    manga_page.handle_events(event);
                }
            },
            SelectedPage::ReaderTab => {
                if let Some(reader_page) = self.manga_reader_page.as_mut() {
                    reader_page.handle_events(event);
                }
            },
            SelectedPage::Home => {
                self.home_page.handle_events(event);
            },
            SelectedPage::Feed => {
                self.feed_page.handle_events(event);
            },
        };
    }

    pub async fn listen_to_event(&mut self) {
        if let Some(event) = self.global_event_rx.recv().await {
            self.process_global_event(event);
        }
    }

    pub fn drain_pending_events(&mut self) {
        let mut drained = 0usize;
        while drained < 32 && self.state == AppState::Runnning {
            let Ok(event) = self.global_event_rx.try_recv() else {
                break;
            };
            // Skip redundant Tick events when draining a backlog of input events
            if event == Events::Tick && !self.global_event_rx.is_empty() {
                drained += 1;
                continue;
            }
            self.process_global_event(event);
            self.update_based_on_action();
            drained += 1;
        }
    }

    pub fn update_based_on_action(&mut self) {
        while let Ok(app_action) = self.global_action_rx.try_recv() {
            self.update(app_action);
        }

        match self.current_tab {
            SelectedPage::Search => {
                while let Ok(search_page_action) = self.search_page.local_action_rx.try_recv() {
                    self.search_page.update(search_page_action);
                }
            },
            SelectedPage::MangaTab => {
                if let Some(manga_page) = self.manga_page.as_mut() {
                    while let Ok(action) = manga_page.local_action_rx.try_recv() {
                        manga_page.update(action);
                    }
                }
            },
            SelectedPage::ReaderTab => {
                if let Some(reader_page) = self.manga_reader_page.as_mut() {
                    while let Ok(reader_action) = reader_page.local_action_rx.try_recv() {
                        reader_page.update(reader_action);
                    }
                }
            },
            SelectedPage::Home => {
                while let Ok(home_action) = self.home_page.local_action_rx.try_recv() {
                    self.home_page.update(home_action);
                }
            },
            SelectedPage::Feed => {
                while let Ok(feed_event) = self.feed_page.local_action_rx.try_recv() {
                    self.feed_page.update(feed_event);
                }
            },
        };
    }

    #[cfg(test)]
    fn with_manga_page(mut self) -> Self {
        self.manga_page =
            Some(MangaPage::new(crate::backend::manga_provider::Manga::default(), None, Arc::clone(&self.manga_provider)));

        self
    }
}

#[cfg(test)]
mod tests {

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::manga_provider::{Languages, SortedVolumes, Volumes};
    use crate::backend::tracker::MangaTracker;
    use crate::global::test_utils::TrackerTest;
    use crate::view::provider_filters::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
    use crate::view::widgets::press_key;

    fn tick<T: MangaProvider + TuiSearchProvider, S: MangaTracker>(app: &mut App<T, S>) {
        let max_amoun_ticks = 10;
        let mut count = 0;

        loop {
            if let Ok(event) = app.global_event_rx.try_recv() {
                app.handle_events(event);
            }

            if count > max_amoun_ticks {
                break;
            }
            count += 1;
        }
    }

    #[test]
    fn new_chapters_remain_pending_until_dismissed() {
        let latest = vec![
            LatestChapter {
                id: "chapter-2".into(),
                chapter_number: "2".into(),
                ..LatestChapter::default()
            },
            LatestChapter {
                id: "chapter-1".into(),
                chapter_number: "1".into(),
                ..LatestChapter::default()
            },
        ];

        let first_check = find_new_chapters(&latest, &[], &["chapter-1".into()]);
        assert_eq!(first_check.len(), 1);
        assert_eq!(first_check[0].id, "chapter-2");

        let repeated_check = find_new_chapters(&latest, &[], &["chapter-1".into()]);
        assert_eq!(repeated_check.len(), 1);
        assert_eq!(repeated_check[0].id, "chapter-2");

        let dismissed = find_new_chapters(&latest, &["chapter-2".into()], &["chapter-1".into()]);
        assert!(dismissed.is_empty());
        assert!(find_new_chapters(&[latest[0].clone()], &[], &["chapter-2".into()]).is_empty());

        let latest_after_release = vec![
            LatestChapter {
                id: "chapter-3".into(),
                chapter_number: "3".into(),
                ..LatestChapter::default()
            },
            latest[0].clone(),
        ];
        let new_release = find_new_chapters(&latest_after_release, &["chapter-2".into()], &[]);
        assert_eq!(new_release.len(), 1);
        assert_eq!(new_release[0].id, "chapter-3");
    }

    #[test]
    fn test_new_chapters_modal_selection_toggle() {
        let items = vec![
            NewChapterItem {
                manga_id: "one-piece".into(),
                manga_title: "One Piece".into(),
                provider: MangaProviders::Mangadex,
                chapter_numbers: "1196, 1195".into(),
                chapters: vec![LatestChapter {
                    id: "ch-1196".into(),
                    manga_id: "one-piece".into(),
                    chapter_number: "1196".into(),
                    ..Default::default()
                }],
                selected: true,
            },
            NewChapterItem {
                manga_id: "dandadan".into(),
                manga_title: "Dandadan".into(),
                provider: MangaProviders::Leercapitulo,
                chapter_numbers: "249".into(),
                chapters: vec![LatestChapter {
                    id: "ch-249".into(),
                    manga_id: "dandadan".into(),
                    chapter_number: "249".into(),
                    ..Default::default()
                }],
                selected: true,
            },
        ];

        let mut state = NewChaptersModalState::new(items);
        assert_eq!(state.cursor, 0);
        assert!(state.items[0].selected);
        assert!(state.items[1].selected);

        // Toggle first item (e.g. One Piece)
        state.items[0].selected = !state.items[0].selected;
        assert!(!state.items[0].selected);
        assert!(state.items[1].selected);

        // Toggle all
        let all_selected = state.items.iter().all(|i| i.selected);
        for item in &mut state.items {
            item.selected = !all_selected;
        }
        assert!(state.items[0].selected);
        assert!(state.items[1].selected);
    }

    #[test]
    fn goes_to_home_page() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        let first_event = app.global_event_rx.blocking_recv().expect("no event was sent");

        assert_eq!(Events::GoToHome, first_event);
        assert_eq!(app.current_tab, SelectedPage::Home);
    }

    #[test]
    fn can_go_to_search_page_by_pressing_i() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        press_key(&mut app, KeyCode::Char('i'));

        tick(&mut app);

        assert_eq!(app.current_tab, SelectedPage::Search);
    }

    #[test]
    fn can_go_to_home_by_pressing_u() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        app.go_search_page();

        press_key(&mut app, KeyCode::Char('u'));

        tick(&mut app);

        assert_eq!(app.current_tab, SelectedPage::Home);
    }

    #[test]
    fn can_go_to_feed_by_pressing_o() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        press_key(&mut app, KeyCode::Char('o'));

        tick(&mut app);

        assert_eq!(app.current_tab, SelectedPage::Feed);
    }

    #[test]
    fn can_navigate_tabs_while_downloading_all_chapters() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {})
                .with_manga_page();

        app.manga_page.as_mut().unwrap().start_downloading_all_chapters();

        press_key(&mut app, KeyCode::Char('i'));

        tick(&mut app);

        assert_eq!(app.current_tab, SelectedPage::Search)
    }

    #[test]
    fn reader_page_is_initialized_corectly() {
        let mut app: App<MockMangaPageProvider, TrackerTest> = App::new(
            MockMangaPageProvider::new(),
            None,
            Some(Picker::new((8, 8))),
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
        )
        .with_manga_page();

        let chapter_to_read = ChapterToRead {
            id: "some_id".to_string(),
            title: "some_title".to_string(),
            number: 1.0,
            volume_number: Some("1".to_string()),
            num_page_bookmarked: None,
            language: Languages::default(),
            pages_url: vec!["http://localhost:3000".parse().unwrap()],
        };

        let list_of_chapter: ListOfChapters = ListOfChapters {
            volumes: SortedVolumes::new(vec![Volumes {
                volume: "1".to_string(),
                ..Default::default()
            }]),
        };

        let manga_tracker = TrackerTest::new();

        app.go_to_read_chapter(
            chapter_to_read,
            MangaToRead {
                title: "some_title".to_string(),
                manga_id: "some_manga_id".to_string(),
                list: list_of_chapter.clone(),
                provider: None,
                ..Default::default()
            },
            Some(manga_tracker),
        );

        let reader_page = app.manga_reader_page.unwrap();

        assert!(reader_page.global_event_tx.is_some());
        assert_eq!(reader_page.list_of_chapters, list_of_chapter);
        assert_eq!(SelectedPage::ReaderTab, app.current_tab);
        assert!(reader_page.manga_tracker.is_some());
    }

    #[tokio::test]
    async fn test_powerline_header_status_bar_and_help_modal() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        // Drain initial GoToHome event
        app.listen_to_event().await;
        assert!(!app.is_help_modal_open);
        assert!(!app.is_overlay_open());

        // Send 'n' through global_event_tx and ensure listen_to_event opens the modal without immediately closing it
        app.global_event_tx.send(Events::Key(KeyCode::Char('n').into())).unwrap();
        app.listen_to_event().await;
        assert!(app.is_help_modal_open);
        assert!(app.is_overlay_open());

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 100, 30);
        terminal
            .draw(|f| {
                app.render(area, f);
            })
            .unwrap();

        assert_eq!(app.tab_hit_boxes.len(), 3);

        // Sending 'n' again while modal is open closes it
        app.global_event_tx.send(Events::Key(KeyCode::Char('n').into())).unwrap();
        app.listen_to_event().await;
        assert!(!app.is_help_modal_open);
        assert!(!app.is_overlay_open());
    }

    #[tokio::test]
    async fn test_live_provider_check_updates_top_bar_indicator() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        // Drain initial GoToHome event
        app.listen_to_event().await;

        app.global_event_tx.send(Events::ProviderCheckTrying(MangaProviders::Inmanga)).unwrap();
        app.listen_to_event().await;
        assert!(app.is_checking_provider);
        assert_eq!(app.active_provider, MangaProviders::Inmanga);

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 120, 30);
        terminal.draw(|f| app.render(area, f)).unwrap();

        let top_row_text: String = (0..120).map(|x| terminal.backend().buffer()[(x, 0)].symbol()).collect();
        assert!(top_row_text.contains("● Inmanga"), "Top bar should show '● Inmanga', got: {top_row_text}");

        app.global_event_tx
            .send(Events::ProviderCheckFinished {
                active_provider: MangaProviders::Mangaoni,
                switched_from: Some(MangaProviders::Inmanga),
                all_failed: false,
            })
            .unwrap();
        app.listen_to_event().await;
        assert!(!app.is_checking_provider);
        assert_eq!(app.active_provider, MangaProviders::Mangaoni);
        assert_eq!(app.home_page.active_provider, MangaProviders::Mangaoni);
        assert_eq!(app.search_page.active_provider, MangaProviders::Mangaoni);
    }

    #[tokio::test]
    async fn test_settings_modal_opens_with_c_and_f5_and_updates_provider() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        // Drain initial GoToHome event
        app.listen_to_event().await;
        assert!(!app.is_settings_modal_open);

        // Press 'c' to open settings modal
        app.global_event_tx.send(Events::Key(KeyCode::Char('c').into())).unwrap();
        app.listen_to_event().await;
        assert!(app.is_settings_modal_open);

        // Render modal
        let backend = TestBackend::new(110, 32);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 110, 32);
        terminal.draw(|f| app.render(area, f)).unwrap();
        let rendered: String = (0..32)
            .map(|y| (0..110).map(|x| terminal.backend().buffer()[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            rendered.contains("Protocolo") || rendered.contains("Protocol"),
            "Settings modal should render the 6th ImageProtocol row"
        );

        // Cycle default provider right ('l') inside settings modal
        let initial_provider = crate::view::widgets::settings::active_default_provider();
        app.global_event_tx.send(Events::Key(KeyCode::Char('l').into())).unwrap();
        app.listen_to_event().await;
        let updated_provider = crate::view::widgets::settings::active_default_provider();
        assert_ne!(initial_provider, updated_provider);
        assert_eq!(app.active_provider, updated_provider);
        assert_eq!(app.home_page.active_provider, updated_provider);
        assert_eq!(app.search_page.active_provider, updated_provider);

        // Jump to row 6 (ImageProtocol) and cycle right ('l')
        let initial_proto = crate::view::widgets::settings::active_image_protocol();
        app.global_event_tx.send(Events::Key(KeyCode::Char('6').into())).unwrap();
        app.listen_to_event().await;
        app.global_event_tx.send(Events::Key(KeyCode::Char('l').into())).unwrap();
        app.listen_to_event().await;
        assert_ne!(initial_proto, crate::view::widgets::settings::active_image_protocol());
        assert!(app.take_needs_terminal_clear());
        assert!(!app.take_needs_terminal_clear());

        // Close with 'c'
        app.global_event_tx.send(Events::Key(KeyCode::Char('c').into())).unwrap();
        app.listen_to_event().await;
        assert!(!app.is_settings_modal_open);

        // Open with F5 and close with Esc
        app.global_event_tx.send(Events::Key(KeyCode::F(5).into())).unwrap();
        app.listen_to_event().await;
        assert!(app.is_settings_modal_open);

        app.global_event_tx.send(Events::Key(KeyCode::Esc.into())).unwrap();
        app.listen_to_event().await;
        assert!(!app.is_settings_modal_open);
    }

    #[tokio::test]
    async fn test_reader_falls_back_to_web_viewer_when_picker_is_none() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});

        let chapter_to_read = ChapterToRead {
            id: "ch-1".to_string(),
            title: "Capitulo 1".to_string(),
            number: 1.0,
            volume_number: None,
            num_page_bookmarked: None,
            language: Languages::Spanish,
            pages_url: vec!["http://localhost/page1.jpg".parse().unwrap()],
        };

        app.go_to_read_chapter(
            chapter_to_read,
            MangaToRead {
                title: "Test Manga".to_string(),
                manga_id: "m-1".to_string(),
                list: ListOfChapters::default(),
                provider: Some(MangaProviders::Inmanga),
                ..Default::default()
            },
            None,
        );

        assert!(app.error_message.is_none(), "must not block with an error modal when picker is None");
        assert!(app.queue_status_toast.is_some(), "should show informative Web HD fallback toast");
        assert_ne!(app.current_tab, SelectedPage::ReaderTab);
    }

    #[test]
    fn filter_and_dedupe_skips_missing_disk_files_and_dedupes_legacy_mangaoni() {
        struct TempDir(std::path::PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let temp = TempDir(std::env::temp_dir().join(format!("manga_tui_notif_test_{}", uuid::Uuid::new_v4())));
        let root = &temp.0;

        let haimiya_dir = root.join("Haimiya-senpai wa Kowakute Kawaii");
        std::fs::create_dir_all(&haimiya_dir).unwrap();
        std::fs::write(haimiya_dir.join("Ch 48 Vol none Capitulo 48  manga_haimiya-senpai_750000.cbz"), b"").unwrap();

        let madan_dir = root.join("Madan no Ichi");
        std::fs::create_dir_all(&madan_dir).unwrap();
        std::fs::write(madan_dir.join("Ch 10 Vol none Capitulo 10  madan-no-ichi_10010.cbz"), b"").unwrap();
        std::fs::write(madan_dir.join("Ch 20 Vol none Capitulo 20  manga_madan-no-ichi_20020.cbz"), b"").unwrap();

        let str_dir = root.join("ST_R Strike it Rich");
        std::fs::create_dir_all(&str_dir).unwrap();
        std::fs::write(str_dir.join("Ch 89 Vol none Capitulo 89  str-strike-it-rich_824059.cbz"), b"").unwrap();

        let after_god_dir = root.join("After God");
        std::fs::create_dir_all(&after_god_dir).unwrap();
        std::fs::write(after_god_dir.join("Ch 48 Vol none  Weeb central 01J76XZ7JNB5QTARKZWHCBTF6T.cbz"), b"").unwrap();

        let index = LocalDownloadsIndex::scan(root).unwrap();

        let mut already_checked = HashSet::new();
        already_checked.insert(canonical_notification_manga_key(MangaProviders::Mangaoni, "manga/str-strike-it-rich"));

        let downloaded = vec![
            DownloadedMangaWithProvider {
                id: "01J76XY7E9FNDZ1DBBM6PBJPFK".into(),
                title: "One Piece".into(),
                provider: MangaProviders::Weebcentral,
                downloaded_chapter_ids: vec!["01M27CS4DQ1JVQDGARQJVK76EQ".into()],
            },
            DownloadedMangaWithProvider {
                id: "dfc7ecb5-e9b3-4aa5-a61b-a498993cd935".into(),
                title: "One Piece".into(),
                provider: MangaProviders::Inmanga,
                downloaded_chapter_ids: vec!["B9C9F061-4400-4813-A70A-9256FC102E05".into()],
            },
            DownloadedMangaWithProvider {
                id: "psvkfbmjgo/one-piece".into(),
                title: "One Piece".into(),
                provider: MangaProviders::Leercapitulo,
                downloaded_chapter_ids: vec!["/leer/psvkfbmjgo/one-piece/1195/".into()],
            },
            DownloadedMangaWithProvider {
                id: "str-strike-it-rich".into(),
                title: "ST✰R: Strike it Rich".into(),
                provider: MangaProviders::Mangaoni,
                downloaded_chapter_ids: vec!["str-strike-it-rich|824059".into()],
            },
            DownloadedMangaWithProvider {
                id: "madan-no-ichi".into(),
                title: "Madan no Ichi".into(),
                provider: MangaProviders::Mangaoni,
                downloaded_chapter_ids: vec!["madan-no-ichi|10010".into(), "madan-no-ichi|deleted-old-9".into()],
            },
            DownloadedMangaWithProvider {
                id: "manga/madan-no-ichi".into(),
                title: "Madan no Ichi".into(),
                provider: MangaProviders::Mangaoni,
                downloaded_chapter_ids: vec!["manga/madan-no-ichi|20020".into()],
            },
            DownloadedMangaWithProvider {
                id: "manga/haimiya-senpai".into(),
                title: "Haimiya-senpai wa Kowakute Kawaii".into(),
                provider: MangaProviders::Mangaoni,
                downloaded_chapter_ids: vec!["manga/haimiya-senpai|750000".into()],
            },
            DownloadedMangaWithProvider {
                id: "01J76XYFSKMEZ2KH54TAC7P3SK".into(),
                title: "After God".into(),
                provider: MangaProviders::Weebcentral,
                downloaded_chapter_ids: vec!["01J76XZ7JNB5QTARKZWHCBTF6T".into()],
            },
        ];

        let result = filter_and_dedupe_downloaded_mangas(downloaded, &index, &already_checked);

        // Missing-from-disk entries (One Piece on weebcentral/inmanga/leercapitulo) and favorite-checked legacy str-strike-it-rich
        // are excluded
        assert_eq!(result.len(), 3);
        assert!(!result.iter().any(|m| m.title == "One Piece"));
        assert!(!result.iter().any(|m| m.id.contains("str-strike-it-rich")));

        let madan = result.iter().find(|m| m.title == "Madan no Ichi").expect("Madan no Ichi present");
        assert_eq!(madan.id, "manga/madan-no-ichi");
        assert_eq!(madan.alias_ids, vec!["madan-no-ichi".to_string()]);
        assert!(madan.downloaded_chapter_ids.contains(&"madan-no-ichi|deleted-old-9".to_string()));
        assert!(madan.downloaded_chapter_ids.contains(&"madan-no-ichi|10010".to_string()));
        assert!(madan.downloaded_chapter_ids.contains(&"manga/madan-no-ichi|20020".to_string()));
    }

    #[test]
    fn find_new_chapters_for_provider_normalizes_mangaoni_prefixes() {
        let latest = vec![
            LatestChapter {
                id: "manga/madan-no-ichi|824060".into(),
                chapter_number: "60".into(),
                ..Default::default()
            },
            LatestChapter {
                id: "manga/madan-no-ichi|824059".into(),
                chapter_number: "59".into(),
                ..Default::default()
            },
            LatestChapter {
                id: "manga/madan-no-ichi|824058".into(),
                chapter_number: "58".into(),
                ..Default::default()
            },
        ];

        let newly_available =
            find_new_chapters_for_provider(MangaProviders::Mangaoni, &latest, &["madan-no-ichi|824058".to_string()], &[
                "madan-no-ichi|824059".to_string(),
            ]);

        assert_eq!(newly_available.len(), 1);
        assert_eq!(newly_available[0].id, "manga/madan-no-ichi|824060");
    }

    #[tokio::test]
    async fn top_tabs_hitboxes_use_unicode_display_width_and_dont_overlap_right_badge() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});
        app.listen_to_event().await; // Home

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 100, 30);
        terminal.draw(|f| app.render(area, f)).unwrap();

        assert_eq!(app.tab_hit_boxes.len(), 3);
        let (home_start, home_end, _) = app.tab_hit_boxes[0];
        let (search_start, search_end, _) = app.tab_hit_boxes[1];
        let (feed_start, feed_end, _) = app.tab_hit_boxes[2];

        let top_row: String = (0..100).map(|x| terminal.backend().buffer()[(x, 0)].symbol()).collect();
        assert!(top_row.contains("●"), "Provider badge on the right must stay visible: {top_row}");

        // Second row highlight '━' must cover the exact column range of the active Home tab (which includes a 2-col emoji)
        let active_bar_cols: Vec<u16> = (0..100).filter(|&x| terminal.backend().buffer()[(x, 1)].symbol() == "━").collect();
        assert_eq!(active_bar_cols.first().copied(), Some(home_start));
        assert_eq!(active_bar_cols.last().map(|x| x + 1), Some(home_end));
        assert_eq!(terminal.backend().buffer()[(home_start, 0)].symbol(), "");
        assert_eq!(terminal.backend().buffer()[(home_end - 1, 0)].symbol(), "");
        assert_eq!(search_start, home_end + 1);
        assert_eq!(feed_start, search_end + 1);

        // Clicking empty space past feed_end (on right side near provider pill) must NOT switch tabs
        app.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 92,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.global_event_rx.try_recv().is_err());

        // Clicking exact Search hitbox triggers GoSearchPage
        app.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: search_start + 2,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.global_event_rx.try_recv().unwrap(), Events::GoSearchPage);
    }

    #[tokio::test]
    async fn image_protocol_event_syncs_app_picker_and_reader_for_subsequent_chapters() {
        use ratatui_image::picker::{Picker, ProtocolType};

        let mut initial_picker = Picker::new((10, 22));
        initial_picker.protocol_type = ProtocolType::Sixel;

        let mut app: App<MockMangaPageProvider, TrackerTest> = App::new(
            MockMangaPageProvider::new(),
            None,
            Some(initial_picker),
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
        );
        app.listen_to_event().await; // initial GoToHome

        let make_chapter = || ChapterToRead {
            id: "c1".into(),
            pages_url: vec!["http://localhost/1".parse().unwrap()],
            ..Default::default()
        };
        app.go_to_read_chapter(
            make_chapter(),
            MangaToRead {
                title: "M".into(),
                manga_id: "m1".into(),
                list: ListOfChapters::default(),
                provider: None,
                ..Default::default()
            },
            None,
        );

        app.process_global_event(Events::ImageProtocolChanged(crate::config::ImageProtocol::Kitty));

        assert_eq!(app.picker.as_ref().unwrap().protocol_type, ProtocolType::Kitty);
        assert_eq!(app.manga_reader_page.as_ref().unwrap().picker_protocol_type(), ProtocolType::Kitty);

        // Opening another chapter retains Kitty instead of reverting to Sixel
        app.go_to_read_chapter(
            make_chapter(),
            MangaToRead {
                title: "M".into(),
                manga_id: "m2".into(),
                list: ListOfChapters::default(),
                provider: None,
                ..Default::default()
            },
            None,
        );
        assert_eq!(app.manga_reader_page.as_ref().unwrap().picker_protocol_type(), ProtocolType::Kitty);

        app.process_global_event(Events::ImageProtocolChanged(crate::config::ImageProtocol::None));
        assert!(app.picker.is_none());

        crate::view::widgets::settings::set_active_image_protocol(crate::config::ImageProtocol::Auto);
    }

    #[tokio::test]
    async fn feed_u_goes_home_and_shift_r_refreshes_timeline() {
        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});
        app.listen_to_event().await; // GoToHome
        app.current_tab = SelectedPage::Feed;

        // Pressing 'u' on Feed triggers global GoToHome (no conflict with timeline refresh)
        app.process_global_event(Events::Key(KeyCode::Char('u').into()));
        assert_eq!(app.global_event_rx.try_recv().unwrap(), Events::GoToHome);

        // Pressing 'R' on Feed goes to FeedPage (RefreshTimeline) without switching away from Feed
        app.process_global_event(Events::Key(KeyCode::Char('R').into()));
        assert!(app.global_event_rx.try_recv().is_err());
        assert_eq!(app.current_tab, SelectedPage::Feed);
        let action = app.feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, crate::view::pages::feed::FeedActions::RefreshTimeline);
    }

    #[tokio::test]
    async fn app_update_events_manage_modal_lifecycle_and_ready_quit() {
        use crate::backend::updater::{
            AvailableUpdate, InstallKind, PreparedUpdate, ReleaseAsset, UpdateChannel, UpdateProgress, UpdateProgressStage,
        };

        let mut app: App<MockMangaPageProvider, TrackerTest> =
            App::new(MockMangaPageProvider::new(), None, None, MockFiltersHandler::new(MockFilterState {}), MockWidgetFilter {});
        app.listen_to_event().await; // GoToHome

        app.is_settings_modal_open = true;

        let update = AvailableUpdate {
            tag_name: "v0.2.1".into(),
            display_version: "v0.2.1".into(),
            channel: UpdateChannel::Stable,
            commit_short: None,
            published_at: Some("2026-11-21T12:00:00Z".into()),
            release_notes: "Fixes".into(),
            html_url: "https://github.com/tomahawk1103/manga-tui/releases/tag/v0.2.1".into(),
            install_kind: InstallKind::WindowsInstaller,
            download_asset: Some(ReleaseAsset {
                name: "setup.exe".into(),
                download_url: "https://example.invalid/setup.exe".into(),
                size: 1000,
            }),
            sha256_asset: None,
            checksum_txt_asset: None,
        };

        // Opening update modal closes settings modal
        app.process_global_event(Events::AppUpdateAvailable(update.clone()));
        assert!(!app.is_settings_modal_open);
        assert!(app.app_update_state.is_some());

        // Dismissing with 'q' closes update modal cleanly
        app.process_global_event(Events::Key(KeyCode::Char('q').into()));
        assert!(app.app_update_state.is_none());

        // Progress and ready events update stage, store PreparedUpdate, and request quit
        app.process_global_event(Events::AppUpdateAvailable(update.clone()));
        app.process_global_event(Events::AppUpdateDownloadProgress(UpdateProgress {
            stage: UpdateProgressStage::Downloading,
            downloaded_bytes: 500,
            total_bytes: Some(1000),
        }));
        assert!(matches!(
            app.app_update_state.as_ref().map(|s| &s.stage),
            Some(AppUpdateStage::Downloading {
                downloaded_bytes: 500,
                total_bytes: Some(1000)
            })
        ));

        let prepared = PreparedUpdate {
            available: update,
            downloaded_file: std::path::PathBuf::from("setup.exe"),
            verified_sha256: "abc".into(),
            target_exe: std::path::PathBuf::from("manga-tui.exe"),
        };
        app.process_global_event(Events::AppUpdateReadyToInstall(prepared.clone()));
        assert_eq!(app.pending_update_install, Some(prepared));
        app.update_based_on_action();
        assert!(app.state == AppState::Done);
    }
}
