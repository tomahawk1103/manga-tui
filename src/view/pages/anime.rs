mod catalog;
mod render;

use std::collections::HashSet;
use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

use anime_core::{
    AnimeDetails, AnimeError, AnimeHomeFeed, AnimePlaybackServer, AnimeProviders, AnimeSearchResult, AnimeStatus, AnimeSummary,
    AnimeType, ExternalPlayer, ExternalPlayerLauncher, RecentAnimeEpisode, VideoMirror,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use futures::FutureExt;
use image::DynamicImage;
use manga_core::backend::anime_history::{AnimeEpisodeHistory, AnimeHistoryEntry};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::widgets::ListState;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use tokio::task::JoinHandle;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::backend::tui::Events;
use crate::config::{AnimeConfig, AnimePlayer};
use crate::i18n::tr;
use crate::runtime_settings::active_anime_config;
use crate::view::widgets::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Query,
    Results,
    RecentEpisodes,
    Recommendations,
    Episodes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Discover,
    Search,
    Recent,
    Favorites,
}

#[derive(Default)]
struct Hitboxes {
    query: Rect,
    search: Rect,
    discover: Rect,
    recent: Rect,
    favorites: Rect,
    refresh: Rect,
    provider: Rect,
    results: Rect,
    recent_episodes: Rect,
    recommendations: Rect,
    episodes: Rect,
    previous: Rect,
    next: Rect,
    mirrors: Rect,
}

struct MirrorMenu {
    mirrors: Vec<VideoMirror>,
    list: ListState,
}

struct Playback {
    _server: AnimePlaybackServer,
    child: Option<Child>,
    provider: AnimeProviders,
    slug: String,
}

pub enum AnimeAction {
    HomeFeed(AnimeProviders, AnimeHomeFeed, HashSet<String>),
    Search(AnimeSearchResult),
    Library(Vec<AnimeHistoryEntry>),
    Details(Box<catalog::Details>),
    History(Vec<AnimeEpisodeHistory>, bool),
    Mirrors(Vec<VideoMirror>, usize),
    Playback(AnimePlaybackServer, Option<Child>, AnimeProviders, String, AnimeProviders),
    Error(String),
}

pub struct AnimePage {
    provider: AnimeProviders,
    configured_provider: AnimeProviders,
    mode: View,
    focus: Focus,
    input: Input,
    searched_query: String,
    page: u32,
    total_pages: u32,
    results: Vec<AnimeSummary>,
    results_list: ListState,
    recent_episodes: Vec<RecentAnimeEpisode>,
    recent_episodes_list: ListState,
    recommendations: Vec<AnimeSummary>,
    recommendations_list: ListState,
    favorite_slugs: HashSet<String>,
    home_loaded_for: Option<AnimeProviders>,
    pending_episode_target: Option<(String, Option<AnimePlayer>)>,
    details: Option<catalog::Details>,
    episodes_list: ListState,
    mirrors: Option<MirrorMenu>,
    picker: Option<Picker>,
    cover: Option<DynamicImage>,
    cover_protocol: Option<Box<dyn StatefulProtocol>>,
    cover_task: Option<JoinHandle<Option<DynamicImage>>>,
    task: Option<JoinHandle<AnimeAction>>,
    playback: Option<Playback>,
    previous_children: Vec<Child>,
    history_refreshed: Instant,
    status: String,
    status_is_error: bool,
    hitboxes: Hitboxes,
}

impl AnimePage {
    pub fn new(picker: Option<Picker>) -> Self {
        let provider = active_anime_config().preferred_provider;
        Self {
            provider,
            configured_provider: provider,
            mode: View::Discover,
            focus: Focus::Query,
            input: Input::default(),
            searched_query: String::new(),
            page: 1,
            total_pages: 1,
            results: Vec::new(),
            results_list: ListState::default(),
            recent_episodes: Vec::new(),
            recent_episodes_list: ListState::default(),
            recommendations: Vec::new(),
            recommendations_list: ListState::default(),
            favorite_slugs: HashSet::new(),
            home_loaded_for: None,
            pending_episode_target: None,
            details: None,
            episodes_list: ListState::default(),
            mirrors: None,
            picker,
            cover: None,
            cover_protocol: None,
            cover_task: None,
            task: None,
            playback: None,
            previous_children: Vec::new(),
            history_refreshed: Instant::now(),
            status: String::new(),
            status_is_error: false,
            hitboxes: Hitboxes::default(),
        }
    }

    pub fn enter(&mut self) {
        if self.apply_settings() {
            return;
        }
        match self.mode {
            View::Discover => {
                if self.home_loaded_for != Some(self.provider) && self.task.is_none() {
                    self.load_home_feed();
                } else {
                    self.refresh_history();
                }
            },
            View::Search => self.refresh_history(),
            View::Recent | View::Favorites => self.load_library(self.mode),
        }
    }

    pub fn apply_settings(&mut self) -> bool {
        let configured = active_anime_config().preferred_provider;
        if configured != self.configured_provider {
            self.configured_provider = configured;
            self.change_provider(configured);
            true
        } else {
            false
        }
    }

    pub fn provider(&self) -> AnimeProviders {
        self.provider
    }

    pub fn shortcuts(&self) -> Vec<(&'static str, &'static str)> {
        if self.mirrors.is_some() {
            return vec![("↑/↓", tr("Servidor", "Mirror")), ("Enter", tr("Ver", "Play")), ("Esc", tr("Cerrar", "Close"))];
        }
        match self.focus {
            Focus::Query => vec![
                ("Enter", tr("Buscar", "Search")),
                ("Tab", tr("Explorar", "Explore")),
                ("Esc", tr("Salir del texto", "Exit input")),
            ],
            Focus::RecentEpisodes => vec![
                ("Enter", tr("Ficha", "Details")),
                ("w", tr("Ver ep. en Web", "Watch in Web")),
                ("Tab", tr("Recomendados", "Recommended")),
                ("r", tr("Actualizar", "Refresh")),
                ("/", tr("Buscar", "Search")),
            ],
            Focus::Recommendations | Focus::Results => vec![
                ("Enter", tr("Ficha", "Details")),
                ("Tab", tr("Cambiar foco", "Switch focus")),
                ("d", tr("Explorar", "Explore")),
                ("PgUp/PgDn", tr("Pág.", "Page")),
                ("/", tr("Buscar", "Search")),
            ],
            Focus::Episodes => vec![
                ("Enter", tr("Ver", "Play")),
                ("w/v", tr("Web/externo", "Web/external")),
                ("s", tr("Servidor", "Mirror")),
                ("m", tr("Visto", "Watched")),
                ("f", tr("Favorito", "Favorite")),
                ("Tab", tr("Foco", "Focus")),
            ],
        }
    }

    pub fn captures_keys(&self) -> bool {
        self.focus == Focus::Query || self.mirrors.is_some()
    }

    pub fn mirror_menu_open(&self) -> bool {
        self.mirrors.is_some()
    }

    pub fn set_picker(&mut self, picker: Option<Picker>) {
        self.picker = picker;
        if picker.is_none() {
            if let Some(task) = self.cover_task.take() {
                task.abort();
            }
            self.cover_protocol = None;
        } else {
            self.prepare_cover();
        }
    }

    fn abort_task(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }

    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_is_error = error;
    }

    fn clear_details(&mut self) {
        if let Some(task) = self.cover_task.take() {
            task.abort();
        }
        self.details = None;
        self.episodes_list.select(None);
        self.mirrors = None;
        self.cover = None;
        self.cover_protocol = None;
    }

    fn clear_results(&mut self) {
        self.results.clear();
        self.results_list.select(None);
        self.total_pages = 1;
        self.clear_details();
    }

    fn change_provider(&mut self, provider: AnimeProviders) {
        if self.provider == provider {
            return;
        }
        self.provider = provider;
        match self.mode {
            View::Discover => {
                self.abort_task();
                self.clear_details();
                self.recent_episodes.clear();
                self.recent_episodes_list.select(None);
                self.recommendations.clear();
                self.recommendations_list.select(None);
                self.load_home_feed();
            },
            View::Search => {
                if self.searched_query.is_empty() {
                    self.abort_task();
                    self.clear_results();
                    self.set_status("", false);
                } else {
                    self.search(self.searched_query.clone(), 1);
                }
            },
            View::Recent | View::Favorites => self.load_library(self.mode),
        }
    }

    fn toggle_provider(&mut self) {
        self.change_provider(match self.provider {
            AnimeProviders::AnimeAv1 => AnimeProviders::JkAnime,
            AnimeProviders::JkAnime => AnimeProviders::AnimeAv1,
        });
    }

    fn submit_search(&mut self) {
        let query = self.input.value().trim().to_owned();
        if query.is_empty() {
            self.open_discover(false);
            return;
        }
        self.search(query, 1);
    }

    fn open_discover(&mut self, force_refresh: bool) {
        self.mode = View::Discover;
        self.focus = if !self.recent_episodes.is_empty() {
            Focus::RecentEpisodes
        } else if !self.recommendations.is_empty() {
            Focus::Recommendations
        } else {
            Focus::Query
        };
        if force_refresh || (self.home_loaded_for != Some(self.provider) && self.task.is_none()) {
            self.load_home_feed();
        }
    }

    fn load_home_feed(&mut self) {
        self.abort_task();
        let provider = self.provider;
        self.set_status(tr("Cargando capítulos recientes y recomendaciones…", "Loading recent episodes & recommendations…"), false);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.task = Some(handle.spawn(async move {
            match catalog::home_feed(provider).await {
                Ok((feed, favorites)) => AnimeAction::HomeFeed(provider, feed, favorites),
                Err(error) => failed(tr("No se pudo cargar la portada de anime.", "Unable to load anime feed."), error),
            }
        }));
    }

    fn search(&mut self, query: String, page: u32) {
        self.abort_task();
        self.clear_results();
        self.mode = View::Search;
        self.focus = Focus::Results;
        self.searched_query.clone_from(&query);
        let provider = self.provider;
        self.set_status(tr("Buscando…", "Searching…"), false);
        self.task = Some(tokio::spawn(async move {
            match catalog::search(provider, query, page).await {
                Ok(result) => AnimeAction::Search(result),
                Err(error) => failed(tr("No se pudo buscar anime.", "Unable to search anime."), error),
            }
        }));
    }

    fn change_page(&mut self, next: bool) {
        if self.mode == View::Search && !self.searched_query.is_empty() && self.task.is_none() {
            let page = if next { self.page.saturating_add(1) } else { self.page.saturating_sub(1) };
            if (1..=self.total_pages).contains(&page) {
                self.search(self.searched_query.clone(), page);
            }
        }
    }

    fn load_library(&mut self, mode: View) {
        self.abort_task();
        self.clear_results();
        self.mode = mode;
        self.focus = Focus::Results;
        let provider = self.provider;
        self.set_status(tr("Cargando biblioteca…", "Loading library…"), false);
        self.task = Some(tokio::spawn(async move {
            match catalog::db(move |db| db.anime_history(provider, mode == View::Favorites)).await {
                Ok(result) => AnimeAction::Library(result),
                Err(error) => failed(tr("No se pudo leer la biblioteca.", "Unable to read the library."), error),
            }
        }));
    }

    fn open_details(&mut self) {
        let Some(summary) = self.results_list.selected().and_then(|index| self.results.get(index)) else { return };
        self.pending_episode_target = None;
        self.open_details_for(summary.provider, summary.slug.clone());
    }

    fn open_recent_episode(&mut self, auto_player: Option<AnimePlayer>) {
        let Some(ep) = self.recent_episodes_list.selected().and_then(|index| self.recent_episodes.get(index)) else {
            return;
        };
        self.pending_episode_target = Some((ep.episode_number.clone(), auto_player));
        self.open_details_for(ep.provider, ep.slug.clone());
    }

    fn open_recommendation(&mut self) {
        let Some(summary) = self.recommendations_list.selected().and_then(|index| self.recommendations.get(index)) else {
            return;
        };
        self.pending_episode_target = None;
        self.open_details_for(summary.provider, summary.slug.clone());
    }

    fn open_details_for(&mut self, provider: AnimeProviders, slug: String) {
        self.abort_task();
        self.clear_details();
        self.set_status(tr("Cargando ficha y episodios…", "Loading details and episodes…"), false);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.task = Some(handle.spawn(async move {
            match catalog::details(provider, slug).await {
                Ok(details) => AnimeAction::Details(Box::new(details)),
                Err(error) => failed(tr("No se pudo cargar la ficha.", "Unable to load anime details."), error),
            }
        }));
    }

    fn prepare_cover(&mut self) {
        let Some(picker) = self.picker.as_mut() else { return };
        if let Some(image) = self.cover.as_ref() {
            self.cover_protocol = Some(picker.new_resize_protocol(image.clone()));
        } else if self.cover_task.is_none()
            && let Some(url) = self.details.as_ref().and_then(|details| details.anime.cover_url.clone())
        {
            self.cover_task = Some(tokio::spawn(catalog::cover(url)));
        }
    }

    fn open_mirrors(&mut self) {
        let Some((details, index)) = self.selected_episode() else { return };
        let anime = details.anime.clone();
        let number = details.episodes[index].number.clone();
        self.abort_task();
        self.set_status(tr("Buscando servidores…", "Finding mirrors…"), false);
        self.task = Some(tokio::spawn(async move {
            match catalog::mirrors(anime, number).await {
                Ok(mirrors) => AnimeAction::Mirrors(mirrors, index),
                Err(error) => failed(tr("No se pudieron cargar los servidores.", "Unable to load mirrors."), error),
            }
        }));
    }

    fn selected_episode(&self) -> Option<(&catalog::Details, usize)> {
        let details = self.details.as_ref()?;
        let index = self.episodes_list.selected()?;
        details.episodes.get(index)?;
        Some((details, index))
    }

    fn play(&mut self, player: Option<AnimePlayer>, mirror: Option<VideoMirror>) {
        let Some((details, index)) = self.selected_episode() else { return };
        let anime = details.anime.clone();
        let episodes = details.episodes.clone();
        let mut config = active_anime_config();
        if let Some(player) = player {
            config.player = player;
        }
        self.abort_task();
        self.set_status(tr("Preparando reproducción…", "Preparing playback…"), false);
        self.task = Some(tokio::spawn(async move {
            let provider = anime.provider;
            let slug = anime.slug.clone();
            match catalog::playback(anime, episodes, index, &config, mirror).await {
                Ok((server, source)) => match launch(&server, &config).await {
                    Ok(child) => AnimeAction::Playback(server, child, provider, slug, source),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound && config.player != AnimePlayer::Web => {
                        AnimeAction::Error(format!(
                            "{} {}",
                            config.player.label(),
                            tr(
                                "no está disponible. Revisa [anime].custom_player_path o pulsa w para abrir el navegador.",
                                "is unavailable. Check [anime].custom_player_path or press w to open the browser."
                            )
                        ))
                    },
                    Err(error) => failed(tr("No se pudo abrir el reproductor.", "Unable to open the player."), error),
                },
                Err(error) => playback_failure(error),
            }
        }));
    }

    fn change_history(&mut self, watched: bool) {
        let Some(details) = self.details.as_ref() else { return };
        let anime = details.anime.clone();
        let favorite = !details.favorite;
        let episode = if watched {
            let Some((_, index)) = self.selected_episode() else { return };
            let episode = details.episodes[index].clone();
            let marked = details
                .history
                .iter()
                .any(|entry| entry.episode_number == episode.number && entry.is_watched);
            Some((episode, !marked))
        } else {
            None
        };
        self.abort_task();
        self.set_status("", false);
        self.task = Some(tokio::spawn(async move {
            match catalog::db(move |db| {
                if let Some((episode, marked)) = episode {
                    db.set_anime_episode_watched(&anime, &episode, marked)?;
                } else {
                    db.set_anime_favorite(&anime, favorite)?;
                }
                Ok((db.anime_episode_history(anime.provider, &anime.slug)?, db.is_anime_favorite(anime.provider, &anime.slug)?))
            })
            .await
            {
                Ok((history, favorite)) => AnimeAction::History(history, favorite),
                Err(error) => failed(tr("No se pudo actualizar el historial.", "Unable to update the history."), error),
            }
        }));
    }

    fn refresh_history(&mut self) {
        if self.task.is_some() || self.mirrors.is_some() {
            return;
        }
        let Some(details) = self.details.as_ref() else { return };
        let provider = details.anime.provider;
        let slug = details.anime.slug.clone();
        self.history_refreshed = Instant::now();
        self.task = Some(tokio::spawn(async move {
            match catalog::db(move |db| Ok((db.anime_episode_history(provider, &slug)?, db.is_anime_favorite(provider, &slug)?)))
                .await
            {
                Ok((history, favorite)) => AnimeAction::History(history, favorite),
                Err(error) => failed(tr("No se pudo leer el progreso.", "Unable to read playback progress."), error),
            }
        }));
    }

    pub fn tick_background(&mut self) {
        if let Some(child) = self.playback.as_mut().and_then(|playback| playback.child.as_mut())
            && child.try_wait().is_ok_and(|status| status.is_some())
        {
            self.playback.as_mut().unwrap().child = None;
        }
        self.previous_children
            .retain_mut(|child| !child.try_wait().is_ok_and(|status| status.is_some()));
        // Complete launches on the UI thread even if the user has already switched tabs.
        if let Some(task) = self.task.as_mut()
            && let Some(result) = task.now_or_never()
        {
            self.task = None;
            match result {
                Ok(action) => self.update(action),
                Err(error) => self.update(failed(tr("La operación no se completó.", "The operation did not finish."), error)),
            }
        }
    }

    fn tick_page(&mut self) {
        if let Some(task) = self.cover_task.as_mut()
            && let Some(result) = task.now_or_never()
        {
            self.cover_task = None;
            if let Ok(Some(image)) = result {
                self.cover = Some(image);
                self.prepare_cover();
            }
        }
        if self.history_refreshed.elapsed() >= Duration::from_secs(3)
            && let (Some(playback), Some(details)) = (&self.playback, &self.details)
            && playback.provider == details.anime.provider
            && playback.slug == details.anime.slug
        {
            self.refresh_history();
        }
    }

    fn mirror_key(&mut self, key: KeyCode) {
        let Some(menu) = self.mirrors.as_mut() else { return };
        match key {
            KeyCode::Esc | KeyCode::Char('q') => self.mirrors = None,
            KeyCode::Down | KeyCode::Char('j') => move_selection(&mut menu.list, menu.mirrors.len(), true),
            KeyCode::Up | KeyCode::Char('k') => move_selection(&mut menu.list, menu.mirrors.len(), false),
            KeyCode::Enter => {
                let mirror = menu.list.selected().and_then(|index| menu.mirrors.get(index).cloned());
                self.mirrors = None;
                if let Some(mirror) = mirror {
                    self.play(None, Some(mirror));
                }
            },
            _ => {},
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let has_details = self.details.is_some();
        let focus = if self.mode == View::Discover {
            let mut order = vec![Focus::Query, Focus::RecentEpisodes, Focus::Recommendations];
            if has_details {
                order.push(Focus::Episodes);
            }
            let idx = order.iter().position(|f| *f == self.focus).unwrap_or(0);
            if forward { order[(idx + 1) % order.len()] } else { order[(idx + order.len() - 1) % order.len()] }
        } else {
            match (self.focus, forward) {
                (Focus::Query, true) | (Focus::Episodes, false) => Focus::Results,
                (Focus::Results, true) | (Focus::Query, false) if has_details => Focus::Episodes,
                (Focus::Query, false) => Focus::Results,
                _ => Focus::Query,
            }
        };
        self.focus = focus;
    }

    fn key(&mut self, key: KeyEvent) {
        if self.mirrors.is_some() {
            self.mirror_key(key.code);
            return;
        }
        match key.code {
            KeyCode::Tab => self.cycle_focus(!key.modifiers.contains(KeyModifiers::SHIFT)),
            KeyCode::BackTab => self.cycle_focus(false),
            KeyCode::Char('/') if self.focus != Focus::Query => self.focus = Focus::Query,
            KeyCode::Enter if self.focus == Focus::Query => self.submit_search(),
            KeyCode::Esc if self.focus == Focus::Query => {
                self.focus = if self.mode == View::Discover { Focus::RecentEpisodes } else { Focus::Results };
            },
            _ if self.focus == Focus::Query => {
                self.input.handle_event(&Event::Key(key));
            },
            KeyCode::Up | KeyCode::Char('k') => self.move_list(false),
            KeyCode::Down | KeyCode::Char('j') => self.move_list(true),
            KeyCode::PageUp => self.change_page(false),
            KeyCode::PageDown => self.change_page(true),
            KeyCode::Enter if self.focus == Focus::Results => self.open_details(),
            KeyCode::Enter if self.focus == Focus::RecentEpisodes => self.open_recent_episode(None),
            KeyCode::Enter if self.focus == Focus::Recommendations => self.open_recommendation(),
            KeyCode::Enter => self.play(None, None),
            KeyCode::Char('w') if self.focus == Focus::RecentEpisodes => self.open_recent_episode(Some(AnimePlayer::Web)),
            KeyCode::Char('w') if self.focus == Focus::Episodes => self.play(Some(AnimePlayer::Web), None),
            KeyCode::Char('v') if self.focus == Focus::Episodes => {
                let config = active_anime_config();
                let player = match config.player {
                    AnimePlayer::Web => {
                        if ExternalPlayerLauncher::detect(ExternalPlayer::Vlc, None).is_some() {
                            AnimePlayer::Vlc
                        } else {
                            AnimePlayer::Mpv
                        }
                    },
                    player => player,
                };
                self.play(Some(player), None);
            },
            KeyCode::Char('s') if self.focus == Focus::Episodes => self.open_mirrors(),
            KeyCode::Char('m') if self.focus == Focus::Episodes => self.change_history(true),
            KeyCode::Char('f') => self.change_history(false),
            KeyCode::Char('d') => self.open_discover(false),
            KeyCode::Char('r') | KeyCode::Char('R') if self.mode == View::Discover => self.open_discover(true),
            KeyCode::Char('h') => self.load_library(View::Recent),
            KeyCode::Char('b') => self.load_library(View::Favorites),
            KeyCode::Char('p') => self.toggle_provider(),
            _ => {},
        }
    }

    fn move_list(&mut self, next: bool) {
        match self.focus {
            Focus::Results => move_selection(&mut self.results_list, self.results.len(), next),
            Focus::RecentEpisodes => move_selection(&mut self.recent_episodes_list, self.recent_episodes.len(), next),
            Focus::Recommendations => move_selection(&mut self.recommendations_list, self.recommendations.len(), next),
            Focus::Episodes => {
                if let Some(details) = self.details.as_ref() {
                    move_selection(&mut self.episodes_list, details.episodes.len(), next);
                }
            },
            Focus::Query => {},
        }
    }

    fn mouse(&mut self, event: MouseEvent) {
        let pos = Position::new(event.column, event.row);
        if let Some(menu) = self.mirrors.as_mut() {
            if let Some(index) = clicked_row(self.hitboxes.mirrors, &menu.list, pos, menu.mirrors.len()) {
                match event.kind {
                    MouseEventKind::Down(MouseButton::Left) if menu.list.selected() == Some(index) => {
                        self.mirror_key(KeyCode::Enter)
                    },
                    MouseEventKind::Down(MouseButton::Left) => menu.list.select(Some(index)),
                    MouseEventKind::ScrollDown => self.mirror_key(KeyCode::Down),
                    MouseEventKind::ScrollUp => self.mirror_key(KeyCode::Up),
                    _ => {},
                }
            }
            return;
        }
        let result = clicked_row(self.hitboxes.results, &self.results_list, pos, self.results.len());
        let recent_ep = clicked_row(self.hitboxes.recent_episodes, &self.recent_episodes_list, pos, self.recent_episodes.len());
        let recommendation =
            clicked_row(self.hitboxes.recommendations, &self.recommendations_list, pos, self.recommendations.len());
        let episode =
            clicked_row(self.hitboxes.episodes, &self.episodes_list, pos, self.details.as_ref().map_or(0, |d| d.episodes.len()));
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.hitboxes.query.contains(pos) {
                    self.focus = Focus::Query;
                } else if self.hitboxes.search.contains(pos) {
                    self.submit_search();
                } else if self.hitboxes.discover.contains(pos) {
                    self.open_discover(false);
                } else if self.hitboxes.recent.contains(pos) {
                    self.load_library(View::Recent);
                } else if self.hitboxes.favorites.contains(pos) {
                    self.load_library(View::Favorites);
                } else if self.hitboxes.refresh.contains(pos) {
                    self.open_discover(true);
                } else if self.hitboxes.provider.contains(pos) {
                    self.toggle_provider();
                } else if self.hitboxes.previous.contains(pos) {
                    self.change_page(false);
                } else if self.hitboxes.next.contains(pos) {
                    self.change_page(true);
                } else if let Some(index) = result {
                    let activate = self.focus == Focus::Results && self.results_list.selected() == Some(index);
                    self.focus = Focus::Results;
                    self.results_list.select(Some(index));
                    if activate {
                        self.open_details();
                    }
                } else if let Some(index) = recent_ep {
                    let activate = self.focus == Focus::RecentEpisodes && self.recent_episodes_list.selected() == Some(index);
                    self.focus = Focus::RecentEpisodes;
                    self.recent_episodes_list.select(Some(index));
                    if activate {
                        self.open_recent_episode(None);
                    }
                } else if let Some(index) = recommendation {
                    let activate = self.focus == Focus::Recommendations && self.recommendations_list.selected() == Some(index);
                    self.focus = Focus::Recommendations;
                    self.recommendations_list.select(Some(index));
                    if activate {
                        self.open_recommendation();
                    }
                } else if let Some(index) = episode {
                    let activate = self.focus == Focus::Episodes && self.episodes_list.selected() == Some(index);
                    self.focus = Focus::Episodes;
                    self.episodes_list.select(Some(index));
                    if activate {
                        self.play(None, None);
                    }
                }
            },
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                if result.is_some() || recent_ep.is_some() || recommendation.is_some() || episode.is_some() =>
            {
                self.focus = if recent_ep.is_some() {
                    Focus::RecentEpisodes
                } else if recommendation.is_some() {
                    Focus::Recommendations
                } else if result.is_some() {
                    Focus::Results
                } else {
                    Focus::Episodes
                };
                self.move_list(event.kind == MouseEventKind::ScrollDown);
            },
            _ => {},
        }
    }
}

impl Component for AnimePage {
    type Actions = AnimeAction;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        self.render_page(area, frame, false);
    }

    fn handle_events(&mut self, events: Events) {
        match events {
            Events::Key(key) => self.key(key),
            Events::Mouse(mouse) => self.mouse(mouse),
            Events::Tick => self.tick_page(),
            _ => {},
        }
    }

    fn update(&mut self, action: AnimeAction) {
        match action {
            AnimeAction::HomeFeed(provider, feed, favorites) => {
                if provider != self.provider {
                    return;
                }
                self.home_loaded_for = Some(provider);
                self.favorite_slugs = favorites;
                self.recent_episodes_list = ListState::default().with_selected((!feed.recent_episodes.is_empty()).then_some(0));
                self.recommendations_list = ListState::default().with_selected((!feed.recommendations.is_empty()).then_some(0));
                let empty = feed.recent_episodes.is_empty() && feed.recommendations.is_empty();
                self.recent_episodes = feed.recent_episodes;
                self.recommendations = feed.recommendations;
                if self.mode == View::Discover && self.focus == Focus::Query && !self.recent_episodes.is_empty() {
                    self.focus = Focus::RecentEpisodes;
                }
                self.set_status(if empty { tr("Sin novedades disponibles.", "No recent updates.") } else { "" }, false);
            },
            AnimeAction::Search(result) => {
                self.page = result.current_page;
                self.total_pages = result.total_pages.max(1);
                self.set_results(result.items);
            },
            AnimeAction::Library(entries) => {
                let provider = self.provider;
                self.set_results(
                    entries
                        .into_iter()
                        .map(|entry| AnimeSummary {
                            id: entry.slug.clone(),
                            slug: entry.slug,
                            title: entry.title,
                            synopsis: None,
                            cover_url: entry.cover_url,
                            anime_type: AnimeType::Unknown,
                            status: AnimeStatus::Unknown,
                            provider,
                        })
                        .collect(),
                );
            },
            AnimeAction::Details(details) => {
                let has_episodes = !details.episodes.is_empty();
                let target = self.pending_episode_target.take();
                let selected_idx = target
                    .as_ref()
                    .and_then(|(num, _)| details.episodes.iter().position(|ep| ep.number == *num))
                    .or_else(|| {
                        (!details.episodes.is_empty()).then(|| {
                            details
                                .episodes
                                .iter()
                                .position(|episode| {
                                    details.history.iter().any(|entry| {
                                        entry.episode_number == episode.number && !entry.is_watched && entry.progress_seconds > 0
                                    })
                                })
                                .unwrap_or(0)
                        })
                    });
                self.episodes_list.select(selected_idx);
                self.focus = if has_episodes { Focus::Episodes } else { Focus::Results };
                self.details = Some(*details);
                self.prepare_cover();
                let status = if has_episodes { "" } else { tr("Sin episodios disponibles.", "No episodes are available.") };
                self.set_status(status, false);
                if let Some((_, Some(player))) = target
                    && has_episodes
                {
                    self.play(Some(player), None);
                }
            },
            AnimeAction::History(history, favorite) => {
                if let Some(details) = self.details.as_mut() {
                    details.history = history;
                    details.favorite = favorite;
                    if favorite {
                        self.favorite_slugs.insert(details.anime.slug.clone());
                    } else {
                        self.favorite_slugs.remove(&details.anime.slug);
                    }
                    if !favorite && self.mode == View::Favorites {
                        self.results.retain(|anime| anime.slug != details.anime.slug);
                        let selected = self.results_list.selected().unwrap_or(0).min(self.results.len().saturating_sub(1));
                        self.results_list.select((!self.results.is_empty()).then_some(selected));
                    }
                }
            },
            AnimeAction::Mirrors(mut mirrors, episode) => {
                if self.episodes_list.selected() != Some(episode) {
                    self.set_status("", false);
                    return;
                }
                if mirrors.is_empty() {
                    self.set_status(tr("No hay servidores compatibles.", "No supported mirrors are available."), true);
                    return;
                }
                let preferred = active_anime_config().preferred_variant;
                mirrors.sort_by_key(|mirror| mirror.variant != preferred);
                self.mirrors = Some(MirrorMenu {
                    mirrors,
                    list: ListState::default().with_selected(Some(0)),
                });
                self.set_status("", false);
            },
            AnimeAction::Playback(server, child, provider, slug, source) => {
                if let Some(previous) = self.playback.take()
                    && let Some(child) = previous.child
                {
                    self.previous_children.push(child);
                }
                self.playback = Some(Playback {
                    _server: server,
                    child,
                    provider,
                    slug,
                });
                self.history_refreshed = Instant::now();
                let status = if source == provider {
                    tr("Reproductor abierto.", "Player opened.").to_owned()
                } else {
                    format!("{} {source}", tr("Reproduciendo desde", "Playing from"))
                };
                self.set_status(status, false);
            },
            AnimeAction::Error(error) => self.set_status(error, true),
        }
    }

    fn clean_up(&mut self) {
        self.abort_task();
        self.clear_details();
        self.playback = None;
    }
}

impl AnimePage {
    fn set_results(&mut self, entries: Vec<AnimeSummary>) {
        if self.mode == View::Discover {
            self.mode = View::Search;
        }
        if matches!(self.focus, Focus::RecentEpisodes | Focus::Recommendations) {
            self.focus = Focus::Results;
        }
        self.results_list = ListState::default().with_selected((!entries.is_empty()).then_some(0));
        self.set_status(if entries.is_empty() { tr("Sin resultados.", "No results.") } else { "" }, false);
        self.results = entries;
    }
}

fn move_selection(list: &mut ListState, count: usize, next: bool) {
    if count == 0 {
        return;
    }
    let index = list
        .selected()
        .map_or(0, |index| if next { index.saturating_add(1).min(count - 1) } else { index.saturating_sub(1) });
    list.select(Some(index));
}

fn clicked_row(area: Rect, list: &ListState, position: Position, count: usize) -> Option<usize> {
    if !area.contains(position) {
        return None;
    }
    let index = list.offset() + usize::from(position.y - area.y);
    (index < count).then_some(index)
}

fn failed(message: &str, error: impl std::fmt::Display) -> AnimeAction {
    log::debug!("Anime: {error}");
    AnimeAction::Error(message.to_owned())
}

fn playback_failure(error: AnimeError) -> AnimeAction {
    let message = match &error {
        AnimeError::MirrorsFailed(failures) => failures
            .iter()
            .map(|(server, cause)| {
                let reason = match cause {
                    AnimeError::Http {
                        status: Some(status),
                        ..
                    } => format!("HTTP {status}"),
                    AnimeError::Http { .. } => tr("fallo de conexión", "connection failed").into(),
                    AnimeError::InvalidInput(_) => tr("enlace rechazado", "link rejected").into(),
                    AnimeError::Parse(_) => tr("formato inválido", "invalid format").into(),
                    _ => tr("vídeo no disponible", "video unavailable").into(),
                };
                let server: String = server.chars().filter(|ch| !ch.is_control()).take(32).collect();
                format!("{server}: {reason}")
            })
            .collect::<Vec<_>>()
            .join(" · "),
        AnimeError::UnsupportedMirror(_) => tr("No hay servidores compatibles.", "No supported mirrors.").into(),
        _ => tr("No se pudo iniciar la reproducción.", "Unable to start playback.").into(),
    };
    failed(&message, error)
}

async fn launch(server: &AnimePlaybackServer, config: &AnimeConfig) -> std::io::Result<Option<Child>> {
    let player = match config.player {
        AnimePlayer::Web => {
            server.open_web()?;
            return Ok(None);
        },
        AnimePlayer::Vlc => ExternalPlayer::Vlc,
        AnimePlayer::Mpv => ExternalPlayer::Mpv,
    };
    let path = config.custom_player_path.trim();
    let path = (!path.is_empty()).then(|| Path::new(path));
    ExternalPlayerLauncher::launch(player, path, server).await.map(Some)
}

#[cfg(test)]
mod tests {
    use anime_core::{AnimeEpisode, AnimeVariant};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn summary(provider: AnimeProviders, index: usize) -> AnimeSummary {
        AnimeSummary {
            id: index.to_string(),
            slug: index.to_string(),
            title: format!("Anime {index}"),
            synopsis: None,
            cover_url: None,
            anime_type: AnimeType::Tv,
            status: AnimeStatus::Completed,
            provider,
        }
    }

    #[test]
    fn scrolled_lists_and_mirror_popup_use_rendered_hitboxes() {
        let mut page = AnimePage::new(None);
        page.set_results((0..40).map(|index| summary(page.provider, index)).collect());
        page.results_list.select(Some(25));
        let mut terminal = Terminal::new(TestBackend::new(92, 22)).unwrap();
        terminal.draw(|frame| page.render(frame.area(), frame)).unwrap();
        let offset = page.results_list.offset();
        assert!(offset > 0);
        let click = |area: Rect, row: u16| MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x,
            row: area.y + row,
            modifiers: KeyModifiers::NONE,
        };
        page.mouse(click(page.hitboxes.results, 0));
        assert_eq!(page.results_list.selected(), Some(offset));

        let anime = AnimeDetails {
            id: "series".into(),
            slug: "series".into(),
            title: "Serie".into(),
            synopsis: "Resumen de anime".into(),
            cover_url: None,
            genres: vec![],
            status: AnimeStatus::Completed,
            anime_type: AnimeType::Tv,
            score: None,
            mal_id: None,
            episodes_count: None,
            provider: page.provider,
            episodes: vec![],
            csrf_token: None,
        };
        let episodes = (0..40)
            .map(|number| AnimeEpisode {
                id: None,
                number: number.to_string(),
                title: None,
                thumbnail_url: None,
                aired_at: None,
            })
            .collect();
        page.update(AnimeAction::Details(Box::new(catalog::Details {
            anime,
            episodes,
            history: vec![AnimeEpisodeHistory {
                episode_number: "29".into(),
                is_watched: false,
                progress_seconds: 71,
                duration_seconds: 100,
            }],
            favorite: false,
        })));
        assert_eq!(page.episodes_list.selected(), Some(29));
        terminal.draw(|frame| page.render(frame.area(), frame)).unwrap();
        let offset = page.episodes_list.offset();
        assert!(offset > 0);
        let buffer = terminal.backend().buffer();
        let row_has_progress = buffer
            .content
            .chunks(92)
            .any(|row| row.iter().map(|cell| cell.symbol()).collect::<String>().contains("1:11 / 1:40"));
        assert!(row_has_progress);

        let mut picker = Picker::new((8, 16));
        picker.protocol_type = ratatui_image::picker::ProtocolType::Sixel;
        page.cover = Some(DynamicImage::new_rgb8(96, 96));
        page.set_picker(Some(picker));
        terminal.draw(|frame| page.render(frame.area(), frame)).unwrap();
        let encoded_rows: Vec<_> = terminal
            .backend()
            .buffer()
            .content
            .chunks(92)
            .enumerate()
            .filter(|(_, row)| row.iter().any(|cell| cell.symbol().contains("\x1bP")))
            .map(|(y, _)| y)
            .collect();
        assert!(!encoded_rows.is_empty());
        assert!(encoded_rows.iter().all(|&y| y < usize::from(page.hitboxes.episodes.top())));

        page.mouse(click(page.hitboxes.episodes, 0));
        assert_eq!(page.episodes_list.selected(), Some(offset));
        let mirror = VideoMirror {
            server_name: "HLS".into(),
            embed_url: "https://example.test/".into(),
            variant: AnimeVariant::Sub,
            referer: String::new(),
            size: None,
        };
        page.update(AnimeAction::Mirrors(vec![mirror.clone()], 29));
        assert!(!page.mirror_menu_open());
        page.update(AnimeAction::Mirrors(vec![mirror], offset));
        terminal.draw(|frame| page.render(frame.area(), frame)).unwrap();
        assert!(page.mirror_menu_open());
        page.mouse(click(page.hitboxes.query, 0));
        assert!(page.mirror_menu_open());
        page.key(KeyCode::Esc.into());
        assert!(!page.mirror_menu_open());
    }

    #[test]
    fn discover_renders_airing_episodes_recommendations_and_preselects_episode() {
        let mut page = AnimePage::new(None);
        let feed = AnimeHomeFeed {
            recent_episodes: vec![
                RecentAnimeEpisode {
                    anime_id: "1".into(),
                    slug: "frieren-2".into(),
                    anime_title: "Frieren 2nd Season".into(),
                    episode_number: "12".into(),
                    aired_label: Some("Ayer".into()),
                    cover_url: None,
                    provider: page.provider,
                },
                RecentAnimeEpisode {
                    anime_id: "2".into(),
                    slug: "one-piece".into(),
                    anime_title: "One Piece".into(),
                    episode_number: "1180".into(),
                    aired_label: None,
                    cover_url: None,
                    provider: page.provider,
                },
            ],
            recommendations: vec![AnimeSummary {
                id: "10".into(),
                slug: "solo-leveling".into(),
                title: "Solo Leveling".into(),
                synopsis: Some("Cazadores de mazmorras.".into()),
                cover_url: None,
                anime_type: AnimeType::Tv,
                status: AnimeStatus::Ongoing,
                provider: page.provider,
            }],
        };
        let mut favs = HashSet::new();
        favs.insert("frieren-2".into());
        page.update(AnimeAction::HomeFeed(page.provider, feed, favs));
        assert_eq!(page.focus, Focus::RecentEpisodes);

        let mut terminal = Terminal::new(TestBackend::new(100, 26)).unwrap();
        terminal.draw(|frame| page.render(frame.area(), frame)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content
            .chunks(100)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("Ep. 12"));
        assert!(screen.contains("Frieren 2nd Season"));
        assert!(screen.contains("Solo Leveling"));

        // Tab cycles from RecentEpisodes -> Recommendations -> Query
        page.key(KeyCode::Tab.into());
        assert_eq!(page.focus, Focus::Recommendations);

        // Selecting a recent episode and receiving Details pre-selects that exact episode
        page.pending_episode_target = Some(("12".into(), None));
        let anime = AnimeDetails {
            id: "1".into(),
            slug: "frieren-2".into(),
            title: "Frieren 2nd Season".into(),
            synopsis: "Viaje al norte.".into(),
            cover_url: None,
            genres: vec![],
            status: AnimeStatus::Ongoing,
            anime_type: AnimeType::Tv,
            score: Some(9.2),
            mal_id: None,
            episodes_count: Some(2),
            provider: page.provider,
            episodes: vec![],
            csrf_token: None,
        };
        let episodes = ["11", "12"]
            .into_iter()
            .map(|number| AnimeEpisode {
                id: None,
                number: number.into(),
                title: None,
                thumbnail_url: None,
                aired_at: None,
            })
            .collect();
        page.update(AnimeAction::Details(Box::new(catalog::Details {
            anime,
            episodes,
            history: vec![],
            favorite: true,
        })));
        assert_eq!(page.episodes_list.selected(), Some(1));
    }
}
