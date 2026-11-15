use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crossterm::event::{self, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use image::DynamicImage;
use manga_tui::SearchTerm;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, StatefulWidget, StatefulWidgetRef, Widget, Wrap};
use ratatui_image::Resize;
use ratatui_image::picker::Picker;
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;
use tui_widget_list::ListState;

use crate::backend::database::{Database, MangaPlanToReadSave};
use crate::backend::error_log::{ErrorType, write_to_error_log};
use crate::backend::manga_provider::{
    EventHandler as FilterEventHandler, FiltersHandler, GetMangasResponse, Manga, MangaProviders, Pagination, SearchManga,
    SearchPageProvider,
};
use crate::backend::tracker::{MangaTracker, track_manga_plan_to_read};
use crate::backend::tui::Events;
use crate::common::ImageState;
use crate::global::{ERROR_STYLE, INSTRUCTIONS_STYLE};
use crate::i18n::tr;
use crate::utils::{centered_rect, render_search_bar};
use crate::view::widgets::search::*;
use crate::view::widgets::{Component, StatefulWidgetFrame};

/// The state in which `search` page is currently in
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum PageState {
    SearchingMangas,
    DisplayingMangasFound,
    NotFound,
    ErrorOcurredSearchingMangas,
    #[default]
    Normal,
}

/// An item displayed in the 3-column Discovery Hub on the Search page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryItem {
    pub title: String,
    pub badge: String,
    pub tag: String,
}

impl DiscoveryItem {
    pub fn new(title: impl Into<String>, badge: impl Into<String>, tag: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            badge: badge.into(),
            tag: tag.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryRankingKind {
    MostRead,
    TopRated,
}

impl DiscoveryRankingKind {
    fn build_url(self, page: u32) -> String {
        match self {
            Self::MostRead => format!("https://api.jikan.moe/v4/top/manga?filter=bypopularity&page={page}&limit=25"),
            Self::TopRated => format!("https://api.jikan.moe/v4/top/manga?page={page}&limit=25"),
        }
    }

    fn is_popular_mode(self) -> bool {
        matches!(self, Self::MostRead)
    }
}

/// Paginated state for a remote Discovery column (Most Read / Top Rated).
#[derive(Debug, Clone)]
pub struct DiscoveryFeed {
    pub items: Vec<DiscoveryItem>,
    pub current_page: u32,
    pub has_next_page: bool,
    pub is_fetching: bool,
    pub is_live: bool,
}

impl DiscoveryFeed {
    const MAX_AUTO_PAGES: u32 = 10;

    fn new(fallback_items: Vec<DiscoveryItem>) -> Self {
        Self {
            items: fallback_items,
            current_page: 0,
            has_next_page: true,
            is_fetching: false,
            is_live: false,
        }
    }

    /// Merges a fetched page into this feed cleanly, deduplicating by title and updating flags.
    pub fn apply_page(&mut self, page: u32, data: Option<(Vec<DiscoveryItem>, bool)>, initial_batch_continues: bool) {
        let Some((new_items, has_next)) = data else {
            self.is_fetching = false;
            return;
        };

        if new_items.is_empty() {
            self.has_next_page = false;
            self.is_fetching = false;
            return;
        }

        if page == 1 {
            self.items = new_items;
            self.current_page = 1;
            self.has_next_page = has_next;
            self.is_live = true;
            self.is_fetching = initial_batch_continues;
        } else if page == self.current_page + 1 {
            let mut seen: HashSet<String> = self.items.iter().map(|i| i.title.to_lowercase()).collect();
            for item in new_items {
                if seen.insert(item.title.to_lowercase()) {
                    self.items.push(item);
                }
            }
            self.current_page = page;
            self.has_next_page = has_next && page < Self::MAX_AUTO_PAGES;
            self.is_live = true;
            self.is_fetching = false;
        } else {
            self.is_fetching = false;
        }
    }

    fn should_fetch_next(&self, selected_row: usize) -> bool {
        self.is_live
            && self.has_next_page
            && !self.is_fetching
            && self.current_page < Self::MAX_AUTO_PAGES
            && selected_row.saturating_add(6) >= self.items.len()
    }
}

fn default_global_most_read() -> Vec<DiscoveryItem> {
    vec![
        DiscoveryItem::new("Berserk", "★ 9.47", "700K lectores"),
        DiscoveryItem::new("Shingeki no Kyojin", "★ 8.55", "650K lectores"),
        DiscoveryItem::new("One Piece", "★ 9.22", "620K lectores"),
        DiscoveryItem::new("Tokyo Ghoul", "★ 8.71", "540K lectores"),
        DiscoveryItem::new("Solo Leveling", "★ 8.66", "510K lectores"),
        DiscoveryItem::new("Chainsaw Man", "★ 8.73", "490K lectores"),
        DiscoveryItem::new("One-Punch Man", "★ 8.75", "450K lectores"),
        DiscoveryItem::new("Jujutsu Kaisen", "★ 8.50", "430K lectores"),
        DiscoveryItem::new("Kimetsu no Yaiba", "★ 8.25", "410K lectores"),
        DiscoveryItem::new("Naruto", "★ 8.08", "400K lectores"),
        DiscoveryItem::new("Vagabond", "★ 9.25", "390K lectores"),
        DiscoveryItem::new("Oyasumi Punpun", "★ 9.01", "380K lectores"),
        DiscoveryItem::new("Boku no Hero Academia", "★ 8.15", "370K lectores"),
        DiscoveryItem::new("Horimiya", "★ 8.39", "350K lectores"),
        DiscoveryItem::new("Kaguya-sama: Love is War", "★ 8.91", "340K lectores"),
        DiscoveryItem::new("Death Note", "★ 8.69", "335K lectores"),
        DiscoveryItem::new("Bleach", "★ 7.98", "325K lectores"),
        DiscoveryItem::new("Vinland Saga", "★ 9.07", "320K lectores"),
        DiscoveryItem::new("Fullmetal Alchemist", "★ 9.03", "310K lectores"),
        DiscoveryItem::new("Monster", "★ 9.16", "295K lectores"),
        DiscoveryItem::new("Haikyu!!", "★ 8.85", "285K lectores"),
        DiscoveryItem::new("Spy x Family", "★ 8.35", "280K lectores"),
        DiscoveryItem::new("Hunter x Hunter", "★ 8.74", "270K lectores"),
        DiscoveryItem::new("Blue Lock", "★ 8.28", "260K lectores"),
        DiscoveryItem::new("Dandadan", "★ 8.45", "250K lectores"),
    ]
}

fn default_global_top_rated() -> Vec<DiscoveryItem> {
    vec![
        DiscoveryItem::new("Berserk", "★ 9.47", "Seinen"),
        DiscoveryItem::new("Steel Ball Run", "★ 9.30", "Aventura"),
        DiscoveryItem::new("Vagabond", "★ 9.25", "Histórico"),
        DiscoveryItem::new("One Piece", "★ 9.22", "Shonen"),
        DiscoveryItem::new("Monster", "★ 9.16", "Thriller"),
        DiscoveryItem::new("Slam Dunk", "★ 9.08", "Deportes"),
        DiscoveryItem::new("Vinland Saga", "★ 9.07", "Épico"),
        DiscoveryItem::new("Fullmetal Alchemist", "★ 9.03", "Fantasía"),
        DiscoveryItem::new("Grand Blue", "★ 9.03", "Comedia"),
        DiscoveryItem::new("Kingdom", "★ 9.00", "Guerra"),
        DiscoveryItem::new("Omniscient Reader", "★ 8.96", "Manhwa"),
        DiscoveryItem::new("20th Century Boys", "★ 8.95", "Misterio"),
        DiscoveryItem::new("Real", "★ 8.95", "Drama"),
        DiscoveryItem::new("Oyasumi Punpun", "★ 9.01", "Psicológico"),
        DiscoveryItem::new("Ashita no Joe", "★ 8.94", "Deportes"),
        DiscoveryItem::new("Kaguya-sama: Love is War", "★ 8.91", "Romance"),
        DiscoveryItem::new("Yotsuba to!", "★ 8.89", "Slice of Life"),
        DiscoveryItem::new("3-gatsu no Lion", "★ 8.88", "Drama"),
        DiscoveryItem::new("Nana", "★ 8.87", "Drama"),
        DiscoveryItem::new("Haikyu!!", "★ 8.85", "Deportes"),
        DiscoveryItem::new("Yokohama Kaidashi Kikou", "★ 8.83", "Sci-Fi"),
        DiscoveryItem::new("Akatsuki no Yona", "★ 8.81", "Fantasía"),
        DiscoveryItem::new("Houseki no Kuni", "★ 8.80", "Fantasía"),
        DiscoveryItem::new("Kaze no Tani no Nausicaa", "★ 8.79", "Aventura"),
        DiscoveryItem::new("The Climber", "★ 8.78", "Seinen"),
    ]
}

fn build_personal_and_trending_discovery(search_history: &[String]) -> Vec<DiscoveryItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(conn) = Database::get_connection() {
        let db = Database::new(&conn);
        if let Ok(most_read) = db.get_most_read_user_mangas(12) {
            for (title, provider, count) in most_read {
                if seen.insert(title.to_lowercase()) {
                    items.push(DiscoveryItem::new(title, format!("📖 {count} caps"), provider.to_string()));
                }
            }
        }
        if let Ok(favs) = db.get_all_favorite_mangas() {
            for fav in favs.into_iter().take(8) {
                if seen.insert(fav.title.to_lowercase()) {
                    items.push(DiscoveryItem::new(fav.title, "★ Favorito".to_string(), fav.provider.to_string()));
                }
            }
        }
    }

    for query in search_history.iter().take(6) {
        if seen.insert(query.to_lowercase()) {
            items.push(DiscoveryItem::new(query.clone(), tr("🕒 Reciente", "🕒 Recent"), tr("Búsqueda", "Search")));
        }
    }

    let trending = [
        ("Dandadan", "🔥 Tendencia", "Acción / Sobrenatural"),
        ("Sakamoto Days", "🔥 Tendencia", "Acción / Comedia"),
        ("Kagurabachi", "🔥 Tendencia", "Fantasía Oscura"),
        ("Blue Lock", "🔥 Tendencia", "Deportes"),
        ("Sousou no Frieren", "🔥 Tendencia", "Fantasía"),
        ("Kaiju No. 8", "🔥 Tendencia", "Sci-Fi / Acción"),
        ("Gachiakuta", "🔥 Tendencia", "Shonen"),
        ("Hunter x Hunter", "🔥 Clásico", "Aventura"),
        ("Spy x Family", "🔥 Tendencia", "Comedia"),
        ("The Climber", "💎 Recomendado", "Seinen"),
        ("Homunculus", "💎 Recomendado", "Psicológico"),
        ("Real", "💎 Recomendado", "Drama"),
        ("Usogui", "💎 Recomendado", "Apuestas"),
        ("Blame!", "💎 Clásico", "Cyberpunk"),
        ("Dorohedoro", "💎 Recomendado", "Fantasía Oscura"),
        ("Golden Kamuy", "💎 Recomendado", "Histórico"),
        ("Billy Bat", "💎 Recomendado", "Misterio"),
        ("Pluto", "💎 Recomendado", "Sci-Fi /Thriller"),
        ("Claymore", "💎 Recomendado", "Fantasía Oscura"),
        ("Made in Abyss", "💎 Recomendado", "Aventura"),
        ("Tower of God", "💎 Manhwa", "Fantasía"),
        ("The Horizon", "💎 Manhwa", "Drama"),
        ("TBATE", "💎 Manhwa", "Isekai"),
        ("Gantz", "💎 Clásico", "Acción / Sci-Fi"),
        ("Hellsing", "💎 Clásico", "Acción"),
        ("Innocent", "💎 Recomendado", "Histórico"),
        ("Blue Box", "🔥 Tendencia", "Romance / Deportes"),
        ("Witch Hat Atelier", "💎 Recomendado", "Fantasía"),
        ("Dungeon Meshi", "🔥 Tendencia", "Fantasía / Comedia"),
        ("Tengoku Daimakyou", "💎 Recomendado", "Misterio / Sci-Fi"),
    ];

    for (title, badge, tag) in trending {
        if items.len() >= 40 {
            break;
        }
        if seen.insert(title.to_lowercase()) {
            items.push(DiscoveryItem::new(title, badge, tag));
        }
    }

    items
}

fn jikan_http_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(7))
                .user_agent("manga-tui/0.10.0")
                .build()
                .ok()
        })
        .as_ref()
}

fn parse_jikan_page(json: &serde_json::Value, mode_popular: bool) -> Option<(Vec<DiscoveryItem>, bool)> {
    let data = json.get("data")?.as_array()?;
    let has_next_page = json
        .get("pagination")
        .and_then(|p| p.get("has_next_page"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut out = Vec::with_capacity(data.len());
    for (idx, entry) in data.iter().enumerate() {
        let title = entry.get("title")?.as_str()?.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let score = entry
            .get("score")
            .and_then(|v| v.as_f64())
            .map(|s| format!("★ {s:.2}"))
            .unwrap_or_else(|| format!("#{}", idx + 1));

        let tag = if mode_popular {
            let members = entry.get("members").and_then(|v| v.as_u64()).unwrap_or(0);
            if members >= 1_000 {
                format!("{}K lectores", members / 1_000)
            } else {
                entry.get("type").and_then(|v| v.as_str()).unwrap_or("Manga").to_string()
            }
        } else {
            entry.get("type").and_then(|v| v.as_str()).unwrap_or("Manga").to_string()
        };

        out.push(DiscoveryItem::new(title, score, tag));
    }

    if out.is_empty() { None } else { Some((out, has_next_page)) }
}

async fn fetch_jikan_page(kind: DiscoveryRankingKind, page: u32) -> Option<(Vec<DiscoveryItem>, bool)> {
    let client = jikan_http_client()?;
    let url = kind.build_url(page);
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    parse_jikan_page(&json, kind.is_popular_mode())
}

fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max_chars {
        text.to_string()
    } else if max_chars == 1 {
        "…".to_string()
    } else {
        let mut s: String = text.chars().take(max_chars - 1).collect();
        s.push('…');
        s
    }
}

/// These are events that do not require user input, like mouse or key events
#[derive(Debug, PartialEq)]
pub enum SearchPageEvents {
    /// Indicate to search manga covers, if the terminal supports it
    SearchCovers,
    LoadCover(Option<DynamicImage>, String),
    LoadMangasFound(Option<GetMangasResponse>),
    FailedGoToMangaPage(String),
    /// This is a `Box` because of `[clippy::large_enum_variant](clippy::large_enum_variant)`
    GoToMangaPage(Box<Manga>),
    LoadGlobalDiscoveryPage {
        kind: DiscoveryRankingKind,
        page: u32,
        data: Option<(Vec<DiscoveryItem>, bool)>,
        initial_batch_continues: bool,
    },
}

/// These are actions that the user actively via key events or mouse events
#[derive(Debug, PartialEq)]
pub enum SearchPageActions {
    StartTyping,
    StopTyping,
    Search,
    ScrollUp,
    ScrollDown,
    MoveDiscoveryRows(isize),
    JumpDiscoveryStart,
    JumpDiscoveryEnd,
    ToggleFilters,
    NextPage,
    PreviousPage,
    SearchMangaPage,
    PlanToRead,
    ToggleScope,
    ToggleLanguageFilter,
    DiscoveryLeft,
    DiscoveryRight,
    SelectDiscoveryCol(usize),
    ReturnToDiscovery,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SearchLanguageFilter {
    #[default]
    All,
    Spanish,
    English,
}

impl SearchLanguageFilter {
    pub fn next(&self) -> Self {
        match self {
            Self::All => Self::Spanish,
            Self::Spanish => Self::English,
            Self::English => Self::All,
        }
    }

    pub fn to_multi_search_language(self) -> crate::backend::multi_provider::MultiSearchLanguage {
        match self {
            Self::All => crate::backend::multi_provider::MultiSearchLanguage::All,
            Self::Spanish => crate::backend::multi_provider::MultiSearchLanguage::Spanish,
            Self::English => crate::backend::multi_provider::MultiSearchLanguage::English,
        }
    }

    pub fn matches_provider(&self, provider: Option<crate::backend::manga_provider::MangaProviders>) -> bool {
        match self {
            Self::All => true,
            Self::Spanish => matches!(
                provider,
                Some(crate::backend::manga_provider::MangaProviders::Inmanga)
                    | Some(crate::backend::manga_provider::MangaProviders::Mangaoni)
                    | Some(crate::backend::manga_provider::MangaProviders::Leercapitulo)
                    | Some(crate::backend::manga_provider::MangaProviders::Mangadex)
            ),
            Self::English => matches!(
                provider,
                Some(crate::backend::manga_provider::MangaProviders::Weebcentral)
                    | Some(crate::backend::manga_provider::MangaProviders::Mangapill)
                    | Some(crate::backend::manga_provider::MangaProviders::Comizy)
                    | Some(crate::backend::manga_provider::MangaProviders::Mangadex)
            ),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    #[default]
    AllProviders,
    CurrentProvider,
}

#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputMode {
    Typing,
    #[default]
    Idle,
}

pub struct SearchPage<T, S>
where
    T: SearchPageProvider,
    S: MangaTracker,
{
    /// This tx "talks" to the app
    global_event_tx: Option<UnboundedSender<Events>>,
    local_action_tx: UnboundedSender<SearchPageActions>,
    pub local_action_rx: UnboundedReceiver<SearchPageActions>,
    local_event_tx: UnboundedSender<SearchPageEvents>,
    pub local_event_rx: UnboundedReceiver<SearchPageEvents>,
    pub input_mode: InputMode,
    search_bar: Input,
    state: PageState,
    loader_state: ThrobberState,
    mangas_found_list: MangasFoundList,
    filter_state: T::FiltersHandler,
    filter_widget: T::Widget,
    manga_added_to_plan_to_read: Option<String>,
    picker: Option<Picker>,
    manga_cover_state: ImageState,
    tasks: JoinSet<()>,
    pub search_scope: SearchScope,
    pub language_filter: SearchLanguageFilter,
    pub search_history: Vec<String>,
    pub history_index: Option<usize>,
    pub stashed_input: String,
    manga_provider: Arc<T>,
    manga_tracker: Option<S>,
    all_providers_results: Vec<SearchManga>,
    all_providers_unfiltered_results: Vec<SearchManga>,
    manga_area: Rect,
    pub most_read_feed: DiscoveryFeed,
    pub top_rated_feed: DiscoveryFeed,
    pub discovery_personal: Vec<DiscoveryItem>,
    pub discovery_col: usize,
    pub discovery_row: [usize; 3],
    pub discovery_scroll_offset: [usize; 3],
    pub discovery_loaded: bool,
    discovery_cols_area: [Rect; 3],
}

/// This contains the data the application gets when doing a search
#[derive(Debug)]
struct MangasFoundList {
    widget: ListMangasFoundWidget,
    state: tui_widget_list::ListState,
    pagination: Pagination,
    next_page: bool,
}

impl MangasFoundList {
    fn new(pagination: Pagination) -> Self {
        Self {
            widget: ListMangasFoundWidget::default(),
            state: tui_widget_list::ListState::default(),
            pagination,
            next_page: false,
        }
    }
}

impl<T, S> Component for SearchPage<T, S>
where
    T: SearchPageProvider,
    S: MangaTracker,
{
    type Actions = SearchPageActions;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        if !self.discovery_loaded {
            self.discovery_loaded = true;
            self.spawn_global_discovery_fetch();
        }

        let search_page_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Max(4), Constraint::Fill(1)]);

        let [input_area, manga_area] = search_page_layout.areas(area);
        self.manga_area = manga_area;

        self.render_input_area(input_area, frame);

        self.render_manga_found_area(manga_area, frame);
    }

    fn update(&mut self, action: SearchPageActions) {
        match action {
            SearchPageActions::ToggleFilters => self.open_advanced_filters(),
            SearchPageActions::StartTyping => self.focus_search_bar(),
            SearchPageActions::StopTyping => self.input_mode = InputMode::Idle,
            SearchPageActions::Search => {
                self.reset_search();
                self.search_mangas();
            },
            SearchPageActions::ScrollUp => self.scroll_up(),
            SearchPageActions::ScrollDown => self.scroll_down(),
            SearchPageActions::MoveDiscoveryRows(delta) => self.move_discovery_rows(delta),
            SearchPageActions::JumpDiscoveryStart => {
                if self.state == PageState::Normal {
                    let col = self.discovery_col.min(2);
                    self.discovery_row[col] = 0;
                }
            },
            SearchPageActions::JumpDiscoveryEnd => {
                if self.state == PageState::Normal {
                    let col = self.discovery_col.min(2);
                    let len = self.current_discovery_col_len();
                    if len > 0 {
                        self.discovery_row[col] = len - 1;
                        self.maybe_fetch_next_discovery_page();
                    }
                }
            },
            SearchPageActions::NextPage => {
                if self.state == PageState::Normal {
                    self.discovery_col = (self.discovery_col + 1) % 3;
                } else {
                    self.search_next_page();
                }
            },
            SearchPageActions::PreviousPage => {
                if self.state == PageState::Normal {
                    self.discovery_col = (self.discovery_col + 2) % 3;
                } else {
                    self.search_previous_page();
                }
            },
            SearchPageActions::SearchMangaPage => {
                if self.state == PageState::Normal {
                    self.activate_selected_discovery_item();
                } else {
                    self.search_manga_page();
                }
            },
            SearchPageActions::PlanToRead => self.plan_to_read(),
            SearchPageActions::ToggleScope => self.toggle_scope(),
            SearchPageActions::ToggleLanguageFilter => self.toggle_language_filter(),
            SearchPageActions::DiscoveryLeft => {
                if self.state == PageState::Normal {
                    self.discovery_col = (self.discovery_col + 2) % 3;
                }
            },
            SearchPageActions::DiscoveryRight => {
                if self.state == PageState::Normal {
                    self.discovery_col = (self.discovery_col + 1) % 3;
                }
            },
            SearchPageActions::SelectDiscoveryCol(col) => {
                if self.state == PageState::Normal && col < 3 {
                    self.discovery_col = col;
                }
            },
            SearchPageActions::ReturnToDiscovery => {
                self.clean_up();
                self.discovery_personal = build_personal_and_trending_discovery(&self.search_history);
            },
        }
    }

    fn handle_events(&mut self, events: Events) {
        if self.filter_state.is_open() {
            self.filter_state.handle_events(events);
        } else {
            match events {
                Events::Key(key_event) => {
                    self.handle_key_events(key_event);
                },
                Events::Mouse(mouse_event) => self.handle_mouse_events(mouse_event),
                Events::Tick => self.tick(),
                _ => {},
            }
        }
    }

    fn clean_up(&mut self) {
        self.abort_tasks();
        self.manga_cover_state = ImageState::default();
        self.state = PageState::default();
        self.manga_added_to_plan_to_read = None;
        self.input_mode = InputMode::Idle;
        self.mangas_found_list.state = ListState::default();
        if !self.mangas_found_list.widget.mangas.is_empty() {
            self.mangas_found_list.widget.mangas = vec![];
        }
    }
}

impl<T, S> SearchPage<T, S>
where
    T: SearchPageProvider,
    S: MangaTracker,
{
    pub fn new(
        picker: Option<Picker>,
        api_client: Arc<T>,
        manga_tracker: Option<S>,
        filter_state: T::FiltersHandler,
        filter_widget: T::Widget,
        pagination: Pagination,
    ) -> Self {
        let (action_tx, action_rx) = mpsc::unbounded_channel::<SearchPageActions>();
        let (local_event_tx, local_event) = mpsc::unbounded_channel::<SearchPageEvents>();

        let search_history = Database::get_connection()
            .and_then(|conn| Database::new(&conn).get_recent_searches(30))
            .unwrap_or_default();

        let discovery_personal = build_personal_and_trending_discovery(&search_history);

        Self {
            global_event_tx: None,
            local_action_tx: action_tx,
            local_action_rx: action_rx,
            local_event_tx,
            local_event_rx: local_event,
            input_mode: InputMode::default(),
            search_bar: Input::default(),
            state: PageState::default(),
            mangas_found_list: MangasFoundList::new(pagination),
            tasks: JoinSet::new(),
            filter_state,
            filter_widget,
            loader_state: ThrobberState::default(),
            manga_added_to_plan_to_read: None,
            picker,
            manga_cover_state: ImageState::default(),
            search_scope: SearchScope::default(),
            language_filter: SearchLanguageFilter::default(),
            search_history,
            history_index: None,
            stashed_input: String::new(),
            manga_provider: api_client,
            manga_tracker,
            all_providers_results: Vec::new(),
            all_providers_unfiltered_results: Vec::new(),
            manga_area: Rect::default(),
            most_read_feed: DiscoveryFeed::new(default_global_most_read()),
            top_rated_feed: DiscoveryFeed::new(default_global_top_rated()),
            discovery_personal,
            discovery_col: 0,
            discovery_row: [0, 0, 0],
            discovery_scroll_offset: [0, 0, 0],
            discovery_loaded: false,
            discovery_cols_area: [Rect::default(); 3],
        }
    }

    pub fn with_global_sender(mut self, sender: UnboundedSender<Events>) -> Self {
        self.global_event_tx = Some(sender);
        self
    }

    fn render_input_area(&self, area: Rect, frame: &mut Frame<'_>) {
        let [input_area, information_area] =
            Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).areas(area);

        let input_help = match self.input_mode {
            InputMode::Idle => Line::from(vec![
                Span::styled(tr(" 🔍 Buscador ", " 🔍 Search "), Style::default().fg(Color::Cyan).bold()),
                Span::styled("<s> ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tr("Escribir ", "Type "), Style::default().fg(Color::Gray)),
                Span::styled("<f> ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tr("Filtros ", "Filters "), Style::default().fg(Color::Gray)),
            ]),
            InputMode::Typing => Line::from(vec![
                Span::styled(tr(" ⌨ Escribiendo ", " ⌨ Typing "), Style::default().fg(Color::Black).bg(Color::Yellow).bold()),
                Span::raw(" "),
                Span::styled("<Enter> ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tr("Buscar ", "Search "), Style::default().fg(Color::Gray)),
                Span::styled("<↑/↓> ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tr("Historial ", "History "), Style::default().fg(Color::Gray)),
                Span::styled("<Esc> ", Style::default().fg(Color::Yellow).bold()),
                Span::styled(tr("Salir ", "Stop "), Style::default().fg(Color::Gray)),
            ]),
        };

        render_search_bar(self.input_mode == InputMode::Typing, input_help, &self.search_bar, frame, input_area);

        let scope_span = match self.search_scope {
            SearchScope::AllProviders => Span::styled(
                format!(" 🌐 {} ", tr("Todos", "All Providers")),
                Style::default().fg(Color::Black).bg(Color::Yellow).bold(),
            ),
            SearchScope::CurrentProvider => Span::styled(
                format!(" 🎯 {} ", self.manga_provider.name()),
                Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
            ),
        };

        let lang_span = match self.language_filter {
            SearchLanguageFilter::All => Span::styled(
                format!(" 🌍 {} ", tr("Todos", "All")),
                Style::default().fg(Color::White).bg(Color::Rgb(55, 62, 82)).bold(),
            ),
            SearchLanguageFilter::Spanish => Span::styled(" 🇪🇸 ES ", Style::default().fg(Color::Black).bg(Color::Green).bold()),
            SearchLanguageFilter::English => Span::styled(" 🇬🇧 EN ", Style::default().fg(Color::White).bg(Color::Blue).bold()),
        };

        let mut info_lines = vec![Line::from(vec![
            Span::styled(tr("Alcance: ", "Scope: "), Style::default().fg(Color::Gray)),
            scope_span,
            Span::styled(" <Tab/m>  ", Style::default().fg(Color::Cyan).bold()),
            Span::styled(tr("Idioma: ", "Lang: "), Style::default().fg(Color::Gray)),
            lang_span,
            Span::styled(" <l>", Style::default().fg(Color::Cyan).bold()),
        ])];

        if let Some(idx) = self.history_index {
            info_lines.push(Line::from(vec![
                Span::styled("🕒 ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{}: {}/{} (<↑/↓>)", tr("Historial", "History"), idx + 1, self.search_history.len()),
                    Style::default().fg(Color::Yellow).bold(),
                ),
            ]));
        } else if let Some(name) = self.manga_added_to_plan_to_read.as_ref() {
            info_lines.push(Line::from(vec![
                Span::styled("✔ ", Style::default().fg(Color::Green).bold()),
                Span::styled(
                    format!("{}: {name} 📖", tr("Añadido a Planeado", "Added to Plan to read")),
                    Style::default().fg(Color::Green).bold(),
                ),
            ]));
        }

        let info_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(75, 88, 120)))
            .title(Line::from(vec![Span::styled(
                tr(" ⚙ Filtros Rápidos ", " ⚙ Quick Filters "),
                Style::default().fg(Color::Cyan).bold(),
            )]));

        let inner_info = info_block.inner(information_area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        info_block.render(information_area, frame.buffer_mut());
        Paragraph::new(info_lines)
            .wrap(Wrap { trim: true })
            .render(inner_info, frame.buffer_mut());
    }

    fn render_manga_found_area(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let buf = frame.buffer_mut();
        let [manga_list_area, preview_area] =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).areas(area);

        match self.state {
            PageState::Normal => {
                let is_live = self.most_read_feed.is_live || self.top_rated_feed.is_live;
                let sync_badge = if is_live {
                    Span::styled(
                        tr(" 🌐 Ranking Mundial en Vivo ", " 🌐 Live Global Ranking "),
                        Style::default().fg(Color::Black).bg(Color::Green).bold(),
                    )
                } else {
                    Span::styled(
                        tr(" ⚡ Ranking Global ", " ⚡ Global Ranking "),
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
                    )
                };

                let bottom_help = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(" <←/→ | 1-3> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                    Span::styled(tr(" Columna  ", " Column  "), Style::default().fg(Color::Gray)),
                    Span::styled(" <↑/↓ | PgDn> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                    Span::styled(tr(" Explorar Top  ", " Browse Top  "), Style::default().fg(Color::Gray)),
                    Span::styled(" <Enter> ", Style::default().fg(Color::Black).bg(Color::Cyan).bold()),
                    Span::styled(tr(" Buscar manga  ", " Search manga  "), Style::default().fg(Color::White).bold()),
                    Span::styled(" <s> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                    Span::styled(tr(" Escribir ", " Type "), Style::default().fg(Color::Gray)),
                ]);

                let welcome_block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Rgb(75, 88, 120)))
                    .title(Line::from(vec![
                        Span::styled(tr(" 🧭 Descubrir Mangas ", " 🧭 Discover Mangas "), Style::default().fg(Color::Cyan).bold()),
                        sync_badge,
                    ]))
                    .title_bottom(bottom_help);

                let inner = welcome_block.inner(area).inner(Margin {
                    horizontal: 1,
                    vertical: 0,
                });
                welcome_block.render(area, buf);

                if inner.width == 0 || inner.height == 0 {
                    return;
                }

                let [top_bar_area, columns_area] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner);

                let all_providers = [
                    MangaProviders::Mangadex,
                    MangaProviders::Inmanga,
                    MangaProviders::Mangaoni,
                    MangaProviders::Leercapitulo,
                    MangaProviders::Weebcentral,
                    MangaProviders::Mangapill,
                    MangaProviders::Comizy,
                ];

                let mut provider_spans: Vec<Span<'static>> =
                    vec![Span::styled(format!(" {} ", tr("Fuentes:", "Sources:")), Style::default().fg(Color::Gray).bold())];
                for prov in all_providers {
                    let health = crate::backend::multi_provider::get_provider_health_status(prov);
                    provider_spans.push(Span::styled(
                        format!(" {} {} ", health.indicator(), prov),
                        Style::default().fg(Color::White).bg(Color::Rgb(42, 48, 66)).bold(),
                    ));
                    provider_spans.push(Span::raw(" "));
                }

                Paragraph::new(vec![Line::from(provider_spans)])
                    .alignment(Alignment::Center)
                    .render(top_bar_area, buf);

                let cols: [Rect; 3] =
                    Layout::horizontal([Constraint::Ratio(1, 3), Constraint::Ratio(1, 3), Constraint::Ratio(1, 3)])
                        .areas(columns_area);

                self.discovery_cols_area = cols;

                let col_configs: [(usize, &str, Color, &[DiscoveryItem], bool); 3] = [
                    (
                        0usize,
                        tr(" 🌍 1. Más Leídos", " 🌍 1. Most Read"),
                        Color::Cyan,
                        &self.most_read_feed.items,
                        self.most_read_feed.is_fetching,
                    ),
                    (
                        1usize,
                        tr(" ⭐ 2. Top Valorados", " ⭐ 2. Top Rated"),
                        Color::Yellow,
                        &self.top_rated_feed.items,
                        self.top_rated_feed.is_fetching,
                    ),
                    (2usize, tr(" 🔥 3. Tuyos & Recs", " 🔥 3. Yours & Recs"), Color::Magenta, &self.discovery_personal, false),
                ];

                for (col_idx, label, accent_color, items, is_loading_more) in col_configs {
                    let is_active_col = self.discovery_col == col_idx;
                    let border_style = if is_active_col {
                        Style::default().fg(accent_color).bold()
                    } else {
                        Style::default().fg(Color::Rgb(52, 60, 86))
                    };
                    let title_style = if is_active_col {
                        Style::default().fg(Color::Black).bg(accent_color).bold()
                    } else {
                        Style::default().fg(accent_color).bold()
                    };

                    let selected_row = self.discovery_row[col_idx].min(items.len().saturating_sub(1));
                    let pos_num = if items.is_empty() { 0 } else { selected_row + 1 };
                    let loading_suffix = if is_loading_more { " ⏳" } else { "" };
                    let header_title = format!("{label} [{pos_num}/{}{loading_suffix}] ", items.len());

                    let col_block = Block::bordered()
                        .border_type(BorderType::Rounded)
                        .border_style(border_style)
                        .title(Line::from(vec![Span::styled(header_title, title_style)]));

                    let col_inner = col_block.inner(cols[col_idx]);
                    col_block.render(cols[col_idx], buf);

                    if col_inner.width == 0 || col_inner.height == 0 {
                        continue;
                    }

                    let max_visible = col_inner.height as usize;
                    let scroll_offset =
                        if selected_row >= max_visible && max_visible > 0 { selected_row + 1 - max_visible } else { 0 };
                    self.discovery_scroll_offset[col_idx] = scroll_offset;

                    let col_w = col_inner.width as usize;
                    let mut row_lines = Vec::with_capacity(max_visible);
                    for (idx, item) in items.iter().enumerate().skip(scroll_offset).take(max_visible) {
                        let is_selected = is_active_col && idx == selected_row;
                        let rank_str = format!("{:>3}. ", idx + 1);

                        let (cursor, row_bg, title_fg) = if is_selected {
                            ("▸ ", Some(Color::Rgb(42, 49, 72)), Color::White)
                        } else if idx == selected_row {
                            ("• ", None, Color::White)
                        } else {
                            ("  ", None, Color::Gray)
                        };

                        let base_style = match row_bg {
                            Some(bg) => Style::default().bg(bg),
                            None => Style::default(),
                        };

                        let badge_style = if item.badge.starts_with('★') {
                            base_style.fg(Color::Yellow).bold()
                        } else if item.badge.starts_with("📖") {
                            base_style.fg(Color::Green).bold()
                        } else if item.badge.starts_with("🕒") {
                            base_style.fg(Color::Cyan)
                        } else {
                            base_style.fg(Color::LightRed).bold()
                        };

                        let badge_text = format!("[{}]", item.badge);
                        let prefix_w = 2 + rank_str.chars().count();
                        let badge_w = badge_text.chars().count();
                        let avail = col_w.saturating_sub(prefix_w + badge_w + 2);
                        let title_chars = item.title.chars().count();
                        let tag_chars = item.tag.chars().count();

                        let show_tag = !item.tag.is_empty() && avail >= title_chars + 1 + tag_chars;
                        let display_title = truncate_with_ellipsis(&item.title, avail.max(4));

                        let mut spans = vec![
                            Span::styled(
                                cursor,
                                if is_selected { base_style.fg(accent_color).bold() } else { base_style.fg(Color::DarkGray) },
                            ),
                            Span::styled(rank_str, base_style.fg(Color::DarkGray)),
                            Span::styled(
                                display_title,
                                if is_selected { base_style.fg(title_fg).bold() } else { base_style.fg(title_fg) },
                            ),
                            Span::styled("  ", base_style),
                            Span::styled(badge_text, badge_style),
                        ];

                        if show_tag {
                            spans.push(Span::styled(format!(" {}", item.tag), base_style.fg(Color::DarkGray)));
                        }

                        row_lines.push(Line::from(spans));
                    }

                    Paragraph::new(row_lines).render(col_inner, buf);
                }
            },
            PageState::SearchingMangas => {
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(Line::from(vec![Span::styled(
                        tr(" ⏳ Buscando... ", " ⏳ Searching... "),
                        Style::default().fg(Color::Yellow).bold(),
                    )]));
                let inner = block.inner(area).inner(Margin {
                    horizontal: 2,
                    vertical: 2,
                });
                block.render(area, buf);

                let loader = Throbber::default()
                    .label(tr(
                        "Buscando mangas en los proveedores seleccionados...",
                        "Searching mangas across selected providers...",
                    ))
                    .style(Style::default().fg(Color::Yellow).bold())
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);

                StatefulWidget::render(loader, inner, buf, &mut self.loader_state);
            },
            PageState::NotFound => {
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(Line::from(vec![Span::styled(
                        tr(" 📭 Sin resultados ", " 📭 No results "),
                        Style::default().fg(Color::Yellow).bold(),
                    )]))
                    .title_top(
                        Line::from(vec![
                            Span::styled(" <Esc> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                            Span::styled(tr(" Volver a Descubrir ", " Back to Discover "), Style::default().fg(Color::Gray)),
                        ])
                        .right_aligned(),
                    );
                let inner = block.inner(area).inner(Margin {
                    horizontal: 2,
                    vertical: 2,
                });
                block.render(area, buf);
                Paragraph::new(tr(
                    "No se encontraron mangas con ese nombre. Prueba con otro término, cambia el filtro (<l>/<Tab>) o pulsa <Esc> para volver al Ranking Mundial.",
                    "No mangas were found. Try another search term, toggle filters (<l>/<Tab>), or press <Esc> to return to Global Rankings.",
                ))
                .style(Style::default().fg(Color::Gray))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
            },
            PageState::ErrorOcurredSearchingMangas => {
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Red))
                    .title(Span::styled(
                        tr(
                            " ⚠ Error al buscar mangas, inténtalo de nuevo (<Esc> Volver) ",
                            " ⚠ An error occurred when searching mangas, please try again (<Esc> Back) ",
                        ),
                        *ERROR_STYLE,
                    ));
                block.render(area, buf);
            },
            PageState::DisplayingMangasFound => {
                let total_pages = self.mangas_found_list.pagination.get_total_pages();
                let current_page = self.mangas_found_list.pagination.current_page;
                let total_items = self.mangas_found_list.pagination.total_items;

                let list_title = Line::from(vec![
                    Span::styled(format!(" 📋 {} ", tr("Resultados", "Results")), Style::default().fg(Color::Cyan).bold()),
                    Span::styled(format!("({total_items}) "), Style::default().fg(Color::Yellow).bold()),
                ]);

                let back_hint = Line::from(vec![
                    Span::styled(" <Esc> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                    Span::styled(tr(" 🧭 Top/Recs ", " 🧭 Top/Recs "), Style::default().fg(Color::Gray)),
                ])
                .right_aligned();

                let pagination_instructions = Line::from(vec![
                    Span::styled(" ◀ <b> ", Style::default().fg(Color::Cyan).bold()),
                    Span::styled(
                        format!(" {} {current_page}/{total_pages} ", tr("Pág.", "Page")),
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
                    ),
                    Span::styled(" <w> ▶ ", Style::default().fg(Color::Cyan).bold()),
                ]);

                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title_top(list_title)
                    .title_top(back_hint)
                    .title_bottom(pagination_instructions.alignment(Alignment::Center))
                    .render(manga_list_area, buf);

                let inner_list_area = manga_list_area.inner(Margin {
                    horizontal: 1,
                    vertical: 1,
                });

                if !self.filter_state.is_open() {
                    StatefulWidgetRef::render_ref(
                        &self.mangas_found_list.widget,
                        inner_list_area,
                        buf,
                        &mut self.mangas_found_list.state,
                    );

                    let loader_state = self.loader_state.clone();
                    if let Some(index) = self.mangas_found_list.state.selected {
                        let manga_selected = &self.mangas_found_list.widget.mangas[index];
                        StatefulWidget::render(
                            MangaPreview::new(
                                &manga_selected.manga.id,
                                &manga_selected.manga.title,
                                manga_selected.manga.description.as_ref().unwrap_or(&"No description".to_string()),
                                &manga_selected.manga.genres,
                                manga_selected.manga.status,
                                manga_selected.manga.provider,
                                self.picker.is_some(),
                                loader_state,
                            ),
                            preview_area,
                            buf,
                            &mut self.manga_cover_state,
                        )
                    }
                }
            },
        }
        if self.filter_state.is_open() {
            self.render_filters(area, frame);
        }
    }

    fn render_filters(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let buf = frame.buffer_mut();
        let popup_area = centered_rect(area, 80, 70);

        Clear.render(popup_area, buf);

        let filter_instructions = Line::from(vec![
            " ".into(),
            tr("Cerrar ", "Close ").into(),
            Span::raw("<f>").style(*INSTRUCTIONS_STYLE),
            " • ".into(),
            tr("Reiniciar filtros ", "Reset filters ").into(),
            Span::raw("<r>").style(*INSTRUCTIONS_STYLE),
            " ".into(),
        ]);

        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Magenta))
            .title(filter_instructions)
            .render(popup_area, buf);

        self.filter_widget.render(popup_area, frame, &mut self.filter_state);
    }

    fn focus_search_bar(&mut self) {
        self.input_mode = InputMode::Typing;
        self.history_index = None;
        self.stashed_input = self.search_bar.value().to_string();
    }

    fn col_kind(col: usize) -> Option<DiscoveryRankingKind> {
        match col {
            0 => Some(DiscoveryRankingKind::MostRead),
            1 => Some(DiscoveryRankingKind::TopRated),
            _ => None,
        }
    }

    fn feed_ref(&self, kind: DiscoveryRankingKind) -> &DiscoveryFeed {
        match kind {
            DiscoveryRankingKind::MostRead => &self.most_read_feed,
            DiscoveryRankingKind::TopRated => &self.top_rated_feed,
        }
    }

    fn feed_mut(&mut self, kind: DiscoveryRankingKind) -> &mut DiscoveryFeed {
        match kind {
            DiscoveryRankingKind::MostRead => &mut self.most_read_feed,
            DiscoveryRankingKind::TopRated => &mut self.top_rated_feed,
        }
    }

    fn current_discovery_col_len(&self) -> usize {
        match Self::col_kind(self.discovery_col) {
            Some(kind) => self.feed_ref(kind).items.len(),
            None => self.discovery_personal.len(),
        }
    }

    fn activate_selected_discovery_item(&mut self) {
        let col = self.discovery_col.min(2);
        let row = self.discovery_row[col];
        let maybe_title = match Self::col_kind(col) {
            Some(kind) => self.feed_ref(kind).items.get(row).map(|i| i.title.clone()),
            None => self.discovery_personal.get(row).map(|i| i.title.clone()),
        };
        if let Some(title) = maybe_title {
            self.search_with_query_all_providers(&title);
        }
    }

    fn spawn_global_discovery_fetch(&self) {
        let tx = self.local_event_tx.clone();
        tokio::spawn(async move {
            let (pop1, rated1) = tokio::join!(
                fetch_jikan_page(DiscoveryRankingKind::MostRead, 1),
                fetch_jikan_page(DiscoveryRankingKind::TopRated, 1),
            );
            let pop_has_next = pop1.as_ref().map(|(_, has_next)| *has_next).unwrap_or(false);
            let rated_has_next = rated1.as_ref().map(|(_, has_next)| *has_next).unwrap_or(false);

            tx.send(SearchPageEvents::LoadGlobalDiscoveryPage {
                kind: DiscoveryRankingKind::MostRead,
                page: 1,
                data: pop1,
                initial_batch_continues: pop_has_next,
            })
            .ok();
            tx.send(SearchPageEvents::LoadGlobalDiscoveryPage {
                kind: DiscoveryRankingKind::TopRated,
                page: 1,
                data: rated1,
                initial_batch_continues: rated_has_next,
            })
            .ok();

            if pop_has_next || rated_has_next {
                tokio::time::sleep(std::time::Duration::from_millis(450)).await;
                let (pop2, rated2) = tokio::join!(
                    async { if pop_has_next { fetch_jikan_page(DiscoveryRankingKind::MostRead, 2).await } else { None } },
                    async { if rated_has_next { fetch_jikan_page(DiscoveryRankingKind::TopRated, 2).await } else { None } }
                );
                if pop_has_next {
                    tx.send(SearchPageEvents::LoadGlobalDiscoveryPage {
                        kind: DiscoveryRankingKind::MostRead,
                        page: 2,
                        data: pop2,
                        initial_batch_continues: false,
                    })
                    .ok();
                }
                if rated_has_next {
                    tx.send(SearchPageEvents::LoadGlobalDiscoveryPage {
                        kind: DiscoveryRankingKind::TopRated,
                        page: 2,
                        data: rated2,
                        initial_batch_continues: false,
                    })
                    .ok();
                }
            }
        });
    }

    fn maybe_fetch_next_discovery_page(&mut self) {
        let Some(kind) = Self::col_kind(self.discovery_col.min(2)) else {
            return;
        };
        let row = self.discovery_row[self.discovery_col.min(2)];
        let next_page = {
            let feed = self.feed_mut(kind);
            if !feed.should_fetch_next(row) {
                return;
            }
            feed.is_fetching = true;
            feed.current_page + 1
        };
        let tx = self.local_event_tx.clone();
        tokio::spawn(async move {
            let data = fetch_jikan_page(kind, next_page).await;
            tx.send(SearchPageEvents::LoadGlobalDiscoveryPage {
                kind,
                page: next_page,
                data,
                initial_batch_continues: false,
            })
            .ok();
        });
    }

    fn move_discovery_rows(&mut self, delta: isize) {
        if self.state != PageState::Normal {
            return;
        }
        let len = self.current_discovery_col_len();
        if len == 0 {
            return;
        }
        let col = self.discovery_col.min(2);
        let max_idx = (len - 1) as isize;
        let target = (self.discovery_row[col] as isize + delta).clamp(0, max_idx) as usize;
        self.discovery_row[col] = target;
        self.maybe_fetch_next_discovery_page();
    }

    fn scroll_down(&mut self) {
        if self.state == PageState::Normal {
            let len = self.current_discovery_col_len();
            if len > 0 {
                let col = self.discovery_col.min(2);
                let has_more_remote = Self::col_kind(col)
                    .map(|kind| {
                        let feed = self.feed_ref(kind);
                        feed.has_next_page || feed.is_fetching
                    })
                    .unwrap_or(false);
                if self.discovery_row[col] + 1 < len {
                    self.discovery_row[col] += 1;
                } else if !has_more_remote {
                    self.discovery_row[col] = 0;
                }
                self.maybe_fetch_next_discovery_page();
            }
        } else {
            self.mangas_found_list.state.next();
        }
    }

    fn scroll_up(&mut self) {
        if self.state == PageState::Normal {
            let len = self.current_discovery_col_len();
            if len > 0 {
                let col = self.discovery_col.min(2);
                if self.discovery_row[col] == 0 {
                    let has_more_remote = Self::col_kind(col)
                        .map(|kind| {
                            let feed = self.feed_ref(kind);
                            feed.has_next_page || feed.is_fetching
                        })
                        .unwrap_or(false);
                    if !has_more_remote {
                        self.discovery_row[col] = len - 1;
                    }
                } else {
                    self.discovery_row[col] -= 1;
                }
            }
        } else {
            self.mangas_found_list.state.previous();
        }
    }

    fn open_advanced_filters(&mut self) {
        self.filter_state.toggle();
    }

    #[inline]
    fn reset_search(&mut self) {
        self.mangas_found_list.pagination.reset();
        self.mangas_found_list.next_page = false;
        self.all_providers_results.clear();
        self.all_providers_unfiltered_results.clear();
    }

    fn get_current_manga_selected_mut(&mut self) -> Option<&mut MangaItem> {
        if let Some(index) = self.mangas_found_list.state.selected {
            return self.mangas_found_list.widget.mangas.get_mut(index);
        }
        None
    }

    fn get_current_manga_selected(&self) -> Option<&MangaItem> {
        if let Some(index) = self.mangas_found_list.state.selected {
            return self.mangas_found_list.widget.mangas.get(index);
        }
        None
    }

    fn plan_to_read(&mut self) {
        if let Some(item) = self.get_current_manga_selected() {
            let manga_selected = item.clone();
            track_manga_plan_to_read(self.manga_tracker.clone(), manga_selected.manga.title.clone(), move |error| {
                write_to_error_log(
                    format!(
                        "Could not add manga {} as plan to read, more details about the error : \n {}",
                        manga_selected.manga.title.clone(),
                        error
                    )
                    .into(),
                );
            });

            let conn = Database::get_connection().unwrap();
            let database = Database::new(&conn);
            let plan_to_read_operation = database.save_plan_to_read(MangaPlanToReadSave {
                id: &item.manga.id,
                title: &item.manga.title,
                img_url: Some(&item.manga.cover_img_url),
                provider: item.manga.provider.unwrap_or_else(|| self.manga_provider.name()),
            });

            match plan_to_read_operation {
                Ok(()) => {
                    self.manga_added_to_plan_to_read = Some(item.manga.title.clone());
                },
                Err(e) => write_to_error_log(ErrorType::Error(Box::new(e))),
            }
        }
    }

    fn search_manga_page(&mut self) {
        let manga_selected = self.get_current_manga_selected();
        if let Some(manga) = manga_selected {
            let client = Arc::clone(&self.manga_provider);
            let id = manga.manga.id.clone();
            let provider = manga.manga.provider;
            let tx = self.local_event_tx.clone();
            self.tasks.spawn(async move {
                let response = match provider {
                    Some(p) => crate::backend::multi_provider::get_manga_by_id_for_provider(p, &id)
                        .await
                        .map_err(|e| e.to_string()),
                    None => client.get_manga_by_id(&id).await.map_err(|e| e.to_string()),
                };
                match response {
                    Ok(mut res) => {
                        if res.provider.is_none() {
                            res.provider = provider;
                        }
                        tx.send(SearchPageEvents::GoToMangaPage(Box::new(res))).ok();
                    },
                    Err(e) => {
                        tx.send(SearchPageEvents::FailedGoToMangaPage(e.clone())).ok();
                        write_to_error_log(e.into());
                    },
                }
            });
        }
    }

    fn abort_tasks(&mut self) {
        self.tasks.abort_all();
    }

    fn handle_key_events(&mut self, key_event: KeyEvent) {
        match self.input_mode {
            InputMode::Idle => match key_event.code {
                KeyCode::Char('s') => {
                    self.local_action_tx.send(SearchPageActions::StartTyping).ok();
                },
                KeyCode::Char('l') => {
                    self.local_action_tx.send(SearchPageActions::ToggleLanguageFilter).ok();
                },
                KeyCode::Char('j') | KeyCode::Down => {
                    self.local_action_tx.send(SearchPageActions::ScrollDown).ok();
                },

                KeyCode::Char('k') | KeyCode::Up => {
                    self.local_action_tx.send(SearchPageActions::ScrollUp).ok();
                },
                KeyCode::Left | KeyCode::Char('h') => {
                    self.local_action_tx.send(SearchPageActions::DiscoveryLeft).ok();
                },
                KeyCode::Right => {
                    self.local_action_tx.send(SearchPageActions::DiscoveryRight).ok();
                },
                KeyCode::Char('1') => {
                    self.local_action_tx.send(SearchPageActions::SelectDiscoveryCol(0)).ok();
                },
                KeyCode::Char('2') => {
                    self.local_action_tx.send(SearchPageActions::SelectDiscoveryCol(1)).ok();
                },
                KeyCode::Char('3') => {
                    self.local_action_tx.send(SearchPageActions::SelectDiscoveryCol(2)).ok();
                },
                KeyCode::PageDown => {
                    if self.state == PageState::Normal {
                        self.local_action_tx.send(SearchPageActions::MoveDiscoveryRows(10)).ok();
                    } else {
                        self.local_action_tx.send(SearchPageActions::NextPage).ok();
                    }
                },
                KeyCode::PageUp => {
                    if self.state == PageState::Normal {
                        self.local_action_tx.send(SearchPageActions::MoveDiscoveryRows(-10)).ok();
                    } else {
                        self.local_action_tx.send(SearchPageActions::PreviousPage).ok();
                    }
                },
                KeyCode::Home => {
                    if self.state == PageState::Normal {
                        self.local_action_tx.send(SearchPageActions::JumpDiscoveryStart).ok();
                    }
                },
                KeyCode::End => {
                    if self.state == PageState::Normal {
                        self.local_action_tx.send(SearchPageActions::JumpDiscoveryEnd).ok();
                    }
                },
                KeyCode::Esc | KeyCode::Backspace => {
                    if self.state != PageState::Normal {
                        self.local_action_tx.send(SearchPageActions::ReturnToDiscovery).ok();
                    }
                },
                KeyCode::Char('w') => {
                    self.local_action_tx.send(SearchPageActions::NextPage).ok();
                },
                KeyCode::Char('p') => {
                    self.local_action_tx.send(SearchPageActions::PlanToRead).ok();
                },
                KeyCode::Char('b') => {
                    self.local_action_tx.send(SearchPageActions::PreviousPage).ok();
                },
                KeyCode::Char('f') => {
                    self.local_action_tx.send(SearchPageActions::ToggleFilters).ok();
                },
                KeyCode::Tab | KeyCode::Char('m') => {
                    self.local_action_tx.send(SearchPageActions::ToggleScope).ok();
                },
                KeyCode::Char('r') | KeyCode::Enter => {
                    self.local_action_tx.send(SearchPageActions::SearchMangaPage).ok();
                },

                _ => {},
            },
            InputMode::Typing => match key_event.code {
                KeyCode::Enter => {
                    if self.state != PageState::SearchingMangas {
                        self.local_action_tx.send(SearchPageActions::Search).ok();
                    }
                },
                KeyCode::Tab => {
                    self.local_action_tx.send(SearchPageActions::ToggleScope).ok();
                },
                KeyCode::Esc => {
                    self.local_action_tx.send(SearchPageActions::StopTyping).ok();
                },
                KeyCode::Up => {
                    self.cycle_search_history_up();
                },
                KeyCode::Down => {
                    self.cycle_search_history_down();
                },
                _ => {
                    if self.history_index.is_some() && key_event.code != KeyCode::Left && key_event.code != KeyCode::Right {
                        self.history_index = None;
                    }
                    self.search_bar.handle_event(&event::Event::Key(key_event));
                },
            },
        }
    }

    pub fn cycle_search_history_up(&mut self) {
        if self.search_history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.stashed_input = self.search_bar.value().to_string();
                self.history_index = Some(0);
                self.search_bar = Input::default().with_value(self.search_history[0].clone());
            },
            Some(idx) => {
                if idx + 1 < self.search_history.len() {
                    let next_idx = idx + 1;
                    self.history_index = Some(next_idx);
                    self.search_bar = Input::default().with_value(self.search_history[next_idx].clone());
                }
            },
        }
    }

    pub fn cycle_search_history_down(&mut self) {
        match self.history_index {
            Some(0) => {
                self.history_index = None;
                self.search_bar = Input::default().with_value(self.stashed_input.clone());
            },
            Some(idx) => {
                let prev_idx = idx - 1;
                self.history_index = Some(prev_idx);
                self.search_bar = Input::default().with_value(self.search_history[prev_idx].clone());
            },
            None => {},
        }
    }

    fn toggle_language_filter(&mut self) {
        self.language_filter = self.language_filter.next();
        if self.search_scope == SearchScope::AllProviders && !self.all_providers_unfiltered_results.is_empty() {
            let filtered: Vec<SearchManga> = self
                .all_providers_unfiltered_results
                .iter()
                .filter(|m| self.language_filter.matches_provider(m.provider))
                .cloned()
                .collect();
            self.all_providers_results = filtered;
            self.mangas_found_list.pagination.total_items = self.all_providers_results.len() as u32;
            self.mangas_found_list.pagination.current_page = 1;
            self.load_all_providers_page();
            self.state = if self.all_providers_results.is_empty() { PageState::NotFound } else { PageState::DisplayingMangasFound };
        }
    }

    fn handle_mouse_events(&mut self, mouse_event: MouseEvent) {
        if self.input_mode == InputMode::Typing || self.filter_state.is_open() {
            return;
        }

        let pos = Position::new(mouse_event.column, mouse_event.row);
        let focus_hovered_discovery_col = |page: &mut Self| {
            if page.state == PageState::Normal {
                for (col_idx, col_rect) in page.discovery_cols_area.iter().enumerate() {
                    if col_rect.contains(pos) {
                        page.discovery_col = col_idx;
                        break;
                    }
                }
            }
        };

        match mouse_event.kind {
            MouseEventKind::ScrollDown => {
                focus_hovered_discovery_col(self);
                self.local_action_tx.send(SearchPageActions::ScrollDown).ok();
            },
            MouseEventKind::ScrollUp => {
                focus_hovered_discovery_col(self);
                self.local_action_tx.send(SearchPageActions::ScrollUp).ok();
            },
            MouseEventKind::Down(MouseButton::Left) => {
                if self.state == PageState::Normal {
                    for (col_idx, col_rect) in self.discovery_cols_area.iter().enumerate() {
                        if col_rect.contains(pos) {
                            self.discovery_col = col_idx;
                            let first_inner_y = col_rect.y.saturating_add(1);
                            let last_inner_y = col_rect.y.saturating_add(col_rect.height.saturating_sub(1));
                            if pos.y >= first_inner_y && pos.y < last_inner_y {
                                let visual_y = (pos.y - first_inner_y) as usize;
                                let item_idx = self.discovery_scroll_offset[col_idx] + visual_y;
                                let col_len = self.current_discovery_col_len();
                                if item_idx < col_len {
                                    self.discovery_row[col_idx] = item_idx;
                                    self.local_action_tx.send(SearchPageActions::SearchMangaPage).ok();
                                }
                            }
                            break;
                        }
                    }
                } else if self.manga_area.contains(pos) {
                    self.local_action_tx.send(SearchPageActions::SearchMangaPage).ok();
                }
            },
            _ => {},
        }
    }

    pub fn is_typing_filter(&mut self) -> bool {
        self.filter_state.is_typing()
    }

    pub fn is_filter_open(&self) -> bool {
        self.filter_state.is_open()
    }

    fn toggle_scope(&mut self) {
        self.search_scope = match self.search_scope {
            SearchScope::AllProviders => SearchScope::CurrentProvider,
            SearchScope::CurrentProvider => SearchScope::AllProviders,
        };
    }

    pub fn search_with_query_all_providers(&mut self, query: &str) {
        self.search_bar = Input::default().with_value(query.to_string());
        self.search_scope = SearchScope::AllProviders;
        self.reset_search();
        self.search_mangas();
    }

    fn search_mangas(&mut self) {
        let query_str = self.search_bar.value().trim().to_string();
        if !query_str.is_empty() {
            if let Ok(conn) = Database::get_connection() {
                let db = Database::new(&conn);
                db.save_search_history(&query_str).ok();
            }
            self.search_history.retain(|q| q != &query_str);
            self.search_history.insert(0, query_str);
            self.history_index = None;
            self.stashed_input.clear();
        }

        self.clean_up();

        self.state = PageState::SearchingMangas;

        let page = self.mangas_found_list.pagination.clone();
        let tx = self.local_event_tx.clone();
        let search_term = SearchTerm::trimmed_lowercased(self.search_bar.value());
        let filters = self.filter_state.get_state().clone();
        let client = Arc::clone(&self.manga_provider);
        let scope = self.search_scope;
        let lang = self.language_filter.to_multi_search_language();

        self.tasks.spawn(async move {
            match scope {
                SearchScope::AllProviders => {
                    let results = if let Some(term) = search_term {
                        crate::backend::multi_provider::search_all_providers_with_lang(term.get(), lang).await
                    } else {
                        vec![]
                    };
                    let total = results.len() as u32;
                    let next_page = (page.current_page * page.items_per_page) < total;
                    let response = GetMangasResponse {
                        mangas: results,
                        total_mangas: total,
                        next_page,
                    };
                    tx.send(SearchPageEvents::LoadMangasFound(Some(response))).ok();
                },
                SearchScope::CurrentProvider => {
                    let response = client.search_mangas(search_term, filters, page).await;
                    match response {
                        Ok(res) => {
                            tx.send(SearchPageEvents::LoadMangasFound(Some(res))).ok();
                        },
                        Err(e) => {
                            write_to_error_log(e.into());
                            tx.send(SearchPageEvents::LoadMangasFound(None)).ok();
                        },
                    }
                },
            }
        });
    }

    fn search_next_page(&mut self) {
        if self.state == PageState::DisplayingMangasFound
            && self.state != PageState::SearchingMangas
            && self.mangas_found_list.pagination.can_go_next_page()
            && (self.mangas_found_list.next_page || !self.all_providers_results.is_empty())
        {
            if !self.all_providers_results.is_empty() {
                self.mangas_found_list.pagination.go_next_page();
                self.load_all_providers_page();
            } else {
                self.mangas_found_list.pagination.go_next_page();
                self.search_mangas();
            }
        }
    }

    fn search_previous_page(&mut self) {
        if self.state == PageState::DisplayingMangasFound
            && self.state != PageState::SearchingMangas
            && self.mangas_found_list.pagination.can_go_previous_page()
        {
            if !self.all_providers_results.is_empty() {
                self.mangas_found_list.pagination.go_previous_page();
                self.load_all_providers_page();
            } else {
                self.mangas_found_list.pagination.go_previous_page();
                self.search_mangas();
            }
        }
    }

    fn go_to_manga_page(&self, manga: Manga) {
        self.global_event_tx.as_ref().unwrap().send(Events::GoToMangaPage(manga)).ok();
    }

    fn load_all_providers_page(&mut self) {
        let total = self.all_providers_results.len();
        let from = self.mangas_found_list.pagination.index_to_slice_from();
        let to = self.mangas_found_list.pagination.to_index(total);

        let slice = if from < total { self.all_providers_results[from..to].to_vec() } else { vec![] };

        self.abort_tasks();
        self.manga_cover_state = ImageState::default();
        self.mangas_found_list.widget = ListMangasFoundWidget::from_response(slice.clone());
        if !slice.is_empty() {
            self.mangas_found_list.state.select(Some(0));
        } else {
            self.mangas_found_list.state.select(None);
        }
        self.mangas_found_list.next_page = self.mangas_found_list.pagination.can_go_next_page();
        self.init_search_manga_covers();
    }

    fn load_mangas_found(&mut self, response: Option<GetMangasResponse>) {
        match response {
            Some(response) => {
                if response.mangas.is_empty() {
                    self.state = PageState::NotFound;
                    self.mangas_found_list.pagination.total_items = 0;
                    self.all_providers_results.clear();
                    self.all_providers_unfiltered_results.clear();
                    return;
                }
                if self.search_scope == SearchScope::AllProviders {
                    self.all_providers_unfiltered_results = response.mangas.clone();
                    let filtered: Vec<SearchManga> = response
                        .mangas
                        .into_iter()
                        .filter(|m| self.language_filter.matches_provider(m.provider))
                        .collect();
                    self.all_providers_results = filtered;
                    self.mangas_found_list.pagination.total_items = self.all_providers_results.len() as u32;
                    self.mangas_found_list.pagination.current_page = 1;
                    self.load_all_providers_page();
                    self.state =
                        if self.all_providers_results.is_empty() { PageState::NotFound } else { PageState::DisplayingMangasFound };
                } else {
                    self.all_providers_results.clear();
                    self.all_providers_unfiltered_results.clear();
                    self.mangas_found_list.pagination.total_items = response.total_mangas;
                    self.mangas_found_list.state.select(Some(0));
                    self.mangas_found_list.widget = ListMangasFoundWidget::from_response(response.mangas);
                    self.mangas_found_list.next_page = response.next_page;
                    self.state = PageState::DisplayingMangasFound;
                    self.init_search_manga_covers();
                }
            },
            None => {
                self.state = PageState::ErrorOcurredSearchingMangas;
                self.mangas_found_list.pagination.total_items = 0;
                self.all_providers_results.clear();
                self.all_providers_unfiltered_results.clear();
            },
        }
    }

    fn init_search_manga_covers(&self) {
        if self.picker.is_some() {
            self.local_event_tx.send(SearchPageEvents::SearchCovers).ok();
        }
    }

    fn search_covers(&mut self) {
        let cover_items: Vec<(String, String, Option<crate::backend::manga_provider::MangaProviders>)> = self
            .mangas_found_list
            .widget
            .mangas
            .iter()
            .map(|item| (item.manga.cover_img_url.clone(), item.manga.id.clone(), item.manga.provider))
            .collect();

        let tx = self.local_event_tx.clone();
        let client = Arc::clone(&self.manga_provider);

        self.tasks.spawn(async move {
            for (cover, id, provider) in cover_items {
                let response = match provider {
                    Some(p) => crate::backend::multi_provider::get_image_for_provider(p, &cover)
                        .await
                        .map_err(|e| e as Box<dyn std::error::Error>),
                    None => client.get_image(&cover).await,
                };
                if let Ok(res) = response {
                    tx.send(SearchPageEvents::LoadCover(Some(res), id)).ok();
                }
            }
        });
    }

    fn display_error_message(&self, error_message: String) {
        self.global_event_tx.as_ref().unwrap().send(Events::Error(error_message)).ok();
    }

    fn load_cover(&mut self, maybe_cover: Option<DynamicImage>, manga_id: String) {
        if let Some(cover) = maybe_cover
            && let Some(picker) = self.picker.as_mut()
        {
            let protocol = picker.new_resize_protocol(cover);
            self.manga_cover_state.insert_manga(protocol, manga_id);
        }
    }

    pub fn tick(&mut self) {
        self.loader_state.calc_next();
        while let Ok(event) = self.local_event_rx.try_recv() {
            match event {
                SearchPageEvents::LoadMangasFound(response) => self.load_mangas_found(response),
                SearchPageEvents::SearchCovers => {
                    if self.picker.is_some() {
                        self.search_covers();
                    }
                },
                SearchPageEvents::LoadCover(maybe_image, manga_id) => self.load_cover(maybe_image, manga_id),
                SearchPageEvents::FailedGoToMangaPage(error_message) => self.display_error_message(error_message),
                SearchPageEvents::GoToMangaPage(manga) => self.go_to_manga_page(*manga),
                SearchPageEvents::LoadGlobalDiscoveryPage {
                    kind,
                    page,
                    data,
                    initial_batch_continues,
                } => {
                    let col = match kind {
                        DiscoveryRankingKind::MostRead => 0,
                        DiscoveryRankingKind::TopRated => 1,
                    };
                    let new_len = {
                        let feed = self.feed_mut(kind);
                        feed.apply_page(page, data, initial_batch_continues);
                        feed.items.len()
                    };
                    if new_len == 0 {
                        self.discovery_row[col] = 0;
                    } else if self.discovery_row[col] >= new_len {
                        self.discovery_row[col] = new_len - 1;
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod test {
    //use ratatui::buffer::Buffer;
    //
    //use super::*;
    //use crate::backend::manga_provider::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
    //use crate::backend::manga_provider::SearchManga;
    //use crate::global::test_utils::TrackerTest;
    //use crate::view::widgets::press_key;
    //
    //#[tokio::test]
    //async fn search_page_events() {
    //    let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
    //        Some(Picker::new((8, 9))),
    //        MockMangaPageProvider::new().into(),
    //        None,
    //        MockFiltersHandler::new(MockFilterState {}),
    //        MockWidgetFilter {},
    //    );
    //
    //    let mock_search_result = GetMangasResponse {
    //        mangas: vec![SearchManga {
    //            id: "manga_id_2".to_string(),
    //            ..Default::default()
    //        }],
    //        total_mangas: 3,
    //    };
    //    // assuming a search was made
    //    search_page
    //        .local_event_tx
    //        .send(SearchPageEvents::LoadMangasFound(Some(mock_search_result)))
    //        .ok();
    //
    //    search_page.tick();
    //
    //    // On first tick page should receive the SearchCovers event
    //    search_page.tick();
    //
    //    tokio::time::sleep(Duration::from_millis(100)).await;
    //    // load first manga cover
    //    search_page.tick();
    //
    //    // load second manga cover
    //    tokio::time::sleep(Duration::from_millis(100)).await;
    //    search_page.tick();
    //
    //    assert!(!search_page.manga_cover_state.is_empty());
    //    assert!(search_page.manga_cover_state.get_image_state("manga_id_2").is_some())
    //}
    //
    //#[tokio::test]
    //async fn search_page_key_events() {
    //    let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
    //        Some(Picker::new((8, 9))),
    //        MockMangaPageProvider::new().into(),
    //        None,
    //        MockFiltersHandler::new(MockFilterState {}),
    //        MockWidgetFilter {},
    //    );
    //
    //    assert!(search_page.state == PageState::Normal);
    //    assert!(!search_page.filter_state.is_open());
    //
    //    // focus search_bar
    //    press_key(&mut search_page, KeyCode::Char('s'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        search_page.update(action)
    //    }
    //
    //    assert!(search_page.input_mode == InputMode::Typing);
    //
    //    // user is typing in the search_bar
    //    press_key(&mut search_page, KeyCode::Char('t'));
    //    press_key(&mut search_page, KeyCode::Char('e'));
    //
    //    assert_eq!("te", search_page.search_bar.value());
    //
    //    // unfocus search_bar
    //    press_key(&mut search_page, KeyCode::Esc);
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        search_page.update(action)
    //    }
    //
    //    assert!(search_page.input_mode == InputMode::Idle);
    //
    //    // Assuming a search was made and some mangas were found
    //    search_page.state = PageState::DisplayingMangasFound;
    //    search_page.mangas_found_list.widget.mangas = vec![MangaItem::default(), MangaItem::default()];
    //    search_page.mangas_found_list.pagination.total_items = 20;
    //    search_page.mangas_found_list.pagination.current_page = 1;
    //
    //    let area = Rect::new(0, 0, 50, 50);
    //    let mut buf = Buffer::empty(area);
    //
    //    // Render the list of mangas found
    //    StatefulWidgetRef::render_ref(
    //        &search_page.mangas_found_list.widget,
    //        area,
    //        &mut buf,
    //        &mut search_page.mangas_found_list.state,
    //    );
    //
    //    // scroll down the list
    //    press_key(&mut search_page, KeyCode::Char('j'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        search_page.update(action)
    //    }
    //
    //    assert!(search_page.mangas_found_list.state.selected.is_some());
    //
    //    // open filters
    //    press_key(&mut search_page, KeyCode::Char('f'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        search_page.update(action)
    //    }
    //
    //    assert!(search_page.filter_state.is_open());
    //
    //    search_page.filter_state.toggle();
    //
    //    // // Add a manga to plan to read
    //    // To test the actual funcionality it's necessary the database, so let's assert the right
    //    // event is called in the meantime
    //    // press_key(&mut search_page, KeyCode::Char('p'));
    //    //
    //    // if let Some(action) = search_page.local_action_rx.recv().await {
    //    //     search_page.update(action)
    //    // }
    //    //
    //    // assert!(search_page.manga_added_to_plan_to_read.is_some());
    //
    //    // Add a manga to plan to read
    //
    //    press_key(&mut search_page, KeyCode::Char('p'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        assert_eq!(SearchPageActions::PlanToRead, action);
    //    } else {
    //        panic!("Add plan to read functionality is not being called");
    //    }
    //
    //    // Go next page
    //    press_key(&mut search_page, KeyCode::Char('w'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        search_page.update(action)
    //    }
    //
    //    assert_eq!(2, search_page.mangas_found_list.pagination.current_page);
    //
    //    search_page.state = PageState::DisplayingMangasFound;
    //
    //    // Go previous page
    //    press_key(&mut search_page, KeyCode::Char('b'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        search_page.update(action)
    //    }
    //    assert_eq!(1, search_page.mangas_found_list.pagination.current_page);
    //
    //    // Go to manga page
    //    press_key(&mut search_page, KeyCode::Char('r'));
    //
    //    if let Some(action) = search_page.local_action_rx.recv().await {
    //        assert_eq!(SearchPageActions::SearchMangaPage, action);
    //    } else {
    //        panic!("The action `go to manga page` is not working");
    //    }
    //}
    //
    //#[test]
    //fn search_manga_cover_if_picker_is_some_after_mangas_were_found() {
    //    let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
    //        Some(Picker::new((8, 9))),
    //        MockMangaPageProvider::new().into(),
    //        None,
    //        MockFiltersHandler::new(MockFilterState {}),
    //        MockWidgetFilter {},
    //    );
    //
    //    search_page.load_mangas_found(Some(GetMangasResponse {
    //        mangas: vec![SearchManga::default()],
    //        total_mangas: 3,
    //    }));
    //
    //    let event = search_page.local_event_rx.blocking_recv().expect("no event was sent");
    //
    //    assert_eq!(event, SearchPageEvents::SearchCovers);
    //}
    //
    //#[test]
    //fn doesnt_search_cover_if_picker_is_none_after_mangas_were_found() {
    //    let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
    //        None,
    //        MockMangaPageProvider::new().into(),
    //        None,
    //        MockFiltersHandler::new(MockFilterState {}),
    //        MockWidgetFilter {},
    //    );
    //
    //    search_page.load_mangas_found(Some(GetMangasResponse {
    //        mangas: vec![SearchManga::default()],
    //        total_mangas: 3,
    //    }));
    //
    //    assert!(search_page.local_event_rx.is_empty());
    //}

    #[tokio::test]
    async fn test_all_providers_pagination_w_and_b() {
        use super::*;
        use crate::backend::manga_provider::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
        use crate::global::test_utils::TrackerTest;
        use crate::view::widgets::press_key;

        let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
            None,
            MockMangaPageProvider::new().into(),
            None,
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
            Pagination::from_first_page(20),
        );

        search_page.search_scope = SearchScope::AllProviders;

        // Create 25 mock mangas
        let mock_mangas: Vec<SearchManga> = (0..25)
            .map(|i| SearchManga {
                id: format!("manga_{i}"),
                title: format!("Manga {i}"),
                ..Default::default()
            })
            .collect();

        // Simulate search response in AllProviders
        search_page.load_mangas_found(Some(GetMangasResponse {
            mangas: mock_mangas,
            total_mangas: 25,
            next_page: true,
        }));

        assert_eq!(PageState::DisplayingMangasFound, search_page.state);
        assert_eq!(1, search_page.mangas_found_list.pagination.current_page);
        assert_eq!(2, search_page.mangas_found_list.pagination.get_total_pages());
        assert_eq!(20, search_page.mangas_found_list.widget.mangas.len());
        assert!(search_page.mangas_found_list.next_page);
        assert!(search_page.mangas_found_list.pagination.can_go_next_page());
        assert!(!search_page.mangas_found_list.pagination.can_go_previous_page());

        // Press 'w' (NextPage)
        press_key(&mut search_page, KeyCode::Char('w'));
        if let Ok(action) = search_page.local_action_rx.try_recv() {
            assert_eq!(SearchPageActions::NextPage, action);
            search_page.update(action);
        } else {
            panic!("NextPage action was not sent on 'w'");
        }

        assert_eq!(2, search_page.mangas_found_list.pagination.current_page);
        assert_eq!(5, search_page.mangas_found_list.widget.mangas.len());
        assert!(!search_page.mangas_found_list.next_page);
        assert!(!search_page.mangas_found_list.pagination.can_go_next_page());
        assert!(search_page.mangas_found_list.pagination.can_go_previous_page());

        // Press 'b' (PreviousPage)
        press_key(&mut search_page, KeyCode::Char('b'));
        if let Ok(action) = search_page.local_action_rx.try_recv() {
            assert_eq!(SearchPageActions::PreviousPage, action);
            search_page.update(action);
        } else {
            panic!("PreviousPage action was not sent on 'b'");
        }

        assert_eq!(1, search_page.mangas_found_list.pagination.current_page);
        assert_eq!(20, search_page.mangas_found_list.widget.mangas.len());
        assert!(search_page.mangas_found_list.next_page);
        assert!(search_page.mangas_found_list.pagination.can_go_next_page());
    }

    #[tokio::test]
    async fn test_search_history_cycling() {
        use super::*;
        use crate::backend::manga_provider::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
        use crate::global::test_utils::TrackerTest;
        use crate::view::widgets::press_key;

        let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
            None,
            MockMangaPageProvider::new().into(),
            None,
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
            Pagination::from_first_page(20),
        );

        search_page.search_history = vec!["One Piece".to_string(), "Naruto".to_string(), "Bleach".to_string()];
        search_page.input_mode = InputMode::Typing;
        search_page.search_bar = Input::default().with_value("My draft".to_string());

        // Press Up -> should recall "One Piece"
        press_key(&mut search_page, KeyCode::Up);
        assert_eq!(search_page.search_bar.value(), "One Piece");
        assert_eq!(search_page.history_index, Some(0));

        // Press Up again -> should recall "Naruto"
        press_key(&mut search_page, KeyCode::Up);
        assert_eq!(search_page.search_bar.value(), "Naruto");
        assert_eq!(search_page.history_index, Some(1));

        // Press Up again -> should recall "Bleach"
        press_key(&mut search_page, KeyCode::Up);
        assert_eq!(search_page.search_bar.value(), "Bleach");
        assert_eq!(search_page.history_index, Some(2));

        // Press Up again at the end of history -> stays on "Bleach"
        press_key(&mut search_page, KeyCode::Up);
        assert_eq!(search_page.search_bar.value(), "Bleach");

        // Press Down -> back to "Naruto"
        press_key(&mut search_page, KeyCode::Down);
        assert_eq!(search_page.search_bar.value(), "Naruto");

        // Press Down -> back to "One Piece"
        press_key(&mut search_page, KeyCode::Down);
        assert_eq!(search_page.search_bar.value(), "One Piece");

        // Press Down -> back to "My draft"
        press_key(&mut search_page, KeyCode::Down);
        assert_eq!(search_page.search_bar.value(), "My draft");
        assert_eq!(search_page.history_index, None);
    }

    #[tokio::test]
    async fn test_language_filter_toggle_and_filtering() {
        use super::*;
        use crate::backend::manga_provider::MangaProviders;
        use crate::backend::manga_provider::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
        use crate::global::test_utils::TrackerTest;
        use crate::view::widgets::press_key;

        let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
            None,
            MockMangaPageProvider::new().into(),
            None,
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
            Pagination::from_first_page(20),
        );

        search_page.search_scope = SearchScope::AllProviders;
        assert_eq!(search_page.language_filter, SearchLanguageFilter::All);

        let results = vec![
            SearchManga {
                id: "1".to_string(),
                title: "Spanish 1".to_string(),
                provider: Some(MangaProviders::Inmanga),
                ..Default::default()
            },
            SearchManga {
                id: "2".to_string(),
                title: "English 1".to_string(),
                provider: Some(MangaProviders::Weebcentral),
                ..Default::default()
            },
            SearchManga {
                id: "3".to_string(),
                title: "Global 1".to_string(),
                provider: Some(MangaProviders::Mangadex),
                ..Default::default()
            },
        ];

        search_page.load_mangas_found(Some(GetMangasResponse {
            mangas: results,
            total_mangas: 3,
            next_page: false,
        }));

        assert_eq!(search_page.all_providers_results.len(), 3);

        // Press 'l' -> ToggleLanguageFilter (to Spanish)
        press_key(&mut search_page, KeyCode::Char('l'));
        if let Ok(action) = search_page.local_action_rx.try_recv() {
            assert_eq!(SearchPageActions::ToggleLanguageFilter, action);
            search_page.update(action);
        } else {
            panic!("ToggleLanguageFilter action not sent on 'l'");
        }

        assert_eq!(search_page.language_filter, SearchLanguageFilter::Spanish);
        // Spanish filter matches Inmanga + Mangadex -> 2 results
        assert_eq!(search_page.all_providers_results.len(), 2);
        assert_eq!(search_page.all_providers_results[0].title, "Spanish 1");
        assert_eq!(search_page.all_providers_results[1].title, "Global 1");

        // Press 'l' again -> to English
        press_key(&mut search_page, KeyCode::Char('l'));
        let action = search_page.local_action_rx.try_recv().unwrap();
        search_page.update(action);

        assert_eq!(search_page.language_filter, SearchLanguageFilter::English);
        // English filter matches Weebcentral + Mangadex -> 2 results
        assert_eq!(search_page.all_providers_results.len(), 2);
        assert_eq!(search_page.all_providers_results[0].title, "English 1");
        assert_eq!(search_page.all_providers_results[1].title, "Global 1");

        // Press 'l' again -> to All
        press_key(&mut search_page, KeyCode::Char('l'));
        let action = search_page.local_action_rx.try_recv().unwrap();
        search_page.update(action);

        assert_eq!(search_page.language_filter, SearchLanguageFilter::All);
        assert_eq!(search_page.all_providers_results.len(), 3);
    }

    #[test]
    fn test_parse_jikan_page_and_pagination() {
        use super::*;
        let entries: Vec<serde_json::Value> = (1..=25)
            .map(|i| {
                serde_json::json!({
                    "title": format!("Manga Global #{i}"),
                    "score": 9.50 - (i as f64 * 0.02),
                    "members": 500_000 + (i as u64 * 1_000),
                    "type": if i % 2 == 0 { "Manhwa" } else { "Manga" }
                })
            })
            .collect();

        let json_has_next = serde_json::json!({
            "pagination": { "has_next_page": true },
            "data": entries
        });

        let (pop_items, pop_next) = parse_jikan_page(&json_has_next, true).expect("popular page should parse");
        assert_eq!(pop_items.len(), 25);
        assert!(pop_next);
        assert_eq!(pop_items[0].title, "Manga Global #1");
        assert_eq!(pop_items[0].badge, "★ 9.48");
        assert_eq!(pop_items[0].tag, "501K lectores");

        let (rated_items, rated_next) = parse_jikan_page(&json_has_next, false).expect("rated page should parse");
        assert_eq!(rated_items.len(), 25);
        assert!(rated_next);
        assert_eq!(rated_items[1].tag, "Manhwa");

        let json_last_page = serde_json::json!({
            "pagination": { "has_next_page": false },
            "data": [
                { "title": "Final Entry", "members": 250, "type": "One-shot" }
            ]
        });

        let (last_items, last_next) = parse_jikan_page(&json_last_page, true).expect("final page should parse");
        assert_eq!(last_items.len(), 1);
        assert!(!last_next);
        assert_eq!(last_items[0].badge, "#1");
        assert_eq!(last_items[0].tag, "One-shot");
    }

    #[test]
    fn test_discovery_feed_apply_page_dedup_and_should_fetch_next() {
        use super::*;
        let fallback = vec![DiscoveryItem::new("Fallback", "★ 8.00", "Local")];
        let mut feed = DiscoveryFeed::new(fallback);
        assert!(!feed.is_live);
        assert!(!feed.should_fetch_next(0));

        let page1: Vec<DiscoveryItem> = (1..=25)
            .map(|i| DiscoveryItem::new(format!("Title {i}"), "★ 9.00", "500K lectores"))
            .collect();
        feed.apply_page(1, Some((page1, true)), true);
        assert_eq!(feed.items.len(), 25);
        assert_eq!(feed.current_page, 1);
        assert!(feed.is_live);
        assert!(feed.has_next_page);
        assert!(feed.is_fetching);
        // While initial batch (page 2) is in flight, should not trigger another auto-fetch
        assert!(!feed.should_fetch_next(24));

        // Page 2 includes 1 duplicate ("title 1" case-insensitive) + 24 new -> 49 unique total
        let mut page2 = vec![DiscoveryItem::new("title 1", "★ 9.00", "500K lectores")];
        for i in 26..=49 {
            page2.push(DiscoveryItem::new(format!("Title {i}"), "★ 8.80", "300K lectores"));
        }
        feed.apply_page(2, Some((page2, true)), false);
        assert_eq!(feed.items.len(), 49);
        assert_eq!(feed.current_page, 2);
        assert!(!feed.is_fetching);
        assert!(feed.has_next_page);

        // Threshold check: 6 rows before bottom triggers next page
        assert!(!feed.should_fetch_next(42)); // 42 + 6 = 48 < 49
        assert!(feed.should_fetch_next(43)); // 43 + 6 = 49 >= 49
        assert!(feed.should_fetch_next(48));

        // Verify MAX_AUTO_PAGES cap stops auto pagination at page 10
        for page in 3..=10 {
            let item = DiscoveryItem::new(format!("Page {page} Item"), "★ 8.50", "Manga");
            feed.apply_page(page, Some((vec![item], true)), false);
        }
        assert_eq!(feed.current_page, 10);
        assert!(!feed.has_next_page);
        assert!(!feed.should_fetch_next(feed.items.len().saturating_sub(1)));
    }

    #[test]
    fn test_truncate_with_ellipsis_utf8_safety() {
        use super::*;
        assert_eq!(truncate_with_ellipsis("Berserk", 0), "");
        assert_eq!(truncate_with_ellipsis("Berserk", 1), "…");
        assert_eq!(truncate_with_ellipsis("Berserk", 7), "Berserk");
        assert_eq!(truncate_with_ellipsis("Berserk", 20), "Berserk");

        let utf8_title = "進撃の巨人 ñandú café ★";
        let truncated = truncate_with_ellipsis(utf8_title, 8);
        assert_eq!(truncated.chars().count(), 8);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncated, "進撃の巨人 ñ…");
    }

    #[tokio::test]
    async fn test_discovery_navigation_keys_and_tick() {
        use super::*;
        use crate::backend::manga_provider::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
        use crate::global::test_utils::TrackerTest;
        use crate::view::widgets::press_key;

        let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
            None,
            MockMangaPageProvider::new().into(),
            None,
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
            Pagination::from_first_page(20),
        );

        let page1: Vec<DiscoveryItem> = (1..=25)
            .map(|i| DiscoveryItem::new(format!("Pop {i}"), "★ 9.00", "400K lectores"))
            .collect();
        let page2: Vec<DiscoveryItem> = (26..=50)
            .map(|i| DiscoveryItem::new(format!("Pop {i}"), "★ 8.70", "250K lectores"))
            .collect();

        search_page
            .local_event_tx
            .send(SearchPageEvents::LoadGlobalDiscoveryPage {
                kind: DiscoveryRankingKind::MostRead,
                page: 1,
                data: Some((page1, true)),
                initial_batch_continues: true,
            })
            .unwrap();
        search_page
            .local_event_tx
            .send(SearchPageEvents::LoadGlobalDiscoveryPage {
                kind: DiscoveryRankingKind::MostRead,
                page: 2,
                data: Some((page2, true)),
                initial_batch_continues: false,
            })
            .unwrap();
        search_page.tick();

        assert_eq!(search_page.most_read_feed.items.len(), 50);
        assert!(search_page.most_read_feed.is_live);
        assert_eq!(search_page.discovery_col, 0);
        assert_eq!(search_page.discovery_row[0], 0);

        press_key(&mut search_page, KeyCode::PageDown);
        let action = search_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, SearchPageActions::MoveDiscoveryRows(10));
        search_page.update(action);
        assert_eq!(search_page.discovery_row[0], 10);

        press_key(&mut search_page, KeyCode::End);
        let action = search_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, SearchPageActions::JumpDiscoveryEnd);
        search_page.update(action);
        assert_eq!(search_page.discovery_row[0], 49);

        press_key(&mut search_page, KeyCode::PageUp);
        let action = search_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, SearchPageActions::MoveDiscoveryRows(-10));
        search_page.update(action);
        assert_eq!(search_page.discovery_row[0], 39);

        press_key(&mut search_page, KeyCode::Home);
        let action = search_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, SearchPageActions::JumpDiscoveryStart);
        search_page.update(action);
        assert_eq!(search_page.discovery_row[0], 0);

        press_key(&mut search_page, KeyCode::Char('2'));
        let action = search_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, SearchPageActions::SelectDiscoveryCol(1));
        search_page.update(action);
        assert_eq!(search_page.discovery_col, 1);
    }

    #[tokio::test]
    async fn test_discovery_mouse_hover_wheel_and_scrolled_click() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        use super::*;
        use crate::backend::manga_provider::mock::{MockFilterState, MockFiltersHandler, MockMangaPageProvider, MockWidgetFilter};
        use crate::global::test_utils::TrackerTest;

        let mut search_page: SearchPage<MockMangaPageProvider, TrackerTest> = SearchPage::new(
            None,
            MockMangaPageProvider::new().into(),
            None,
            MockFiltersHandler::new(MockFilterState {}),
            MockWidgetFilter {},
            Pagination::from_first_page(20),
        );

        search_page.discovery_cols_area = [Rect::new(0, 10, 40, 12), Rect::new(40, 10, 40, 12), Rect::new(80, 10, 40, 12)];
        search_page.discovery_scroll_offset = [15, 0, 0];
        search_page.most_read_feed.items = (1..=50).map(|i| DiscoveryItem::new(format!("Item {i}"), "★ 8.50", "Manga")).collect();

        // Wheel over column 1 switches focus to column 1 and scrolls down
        search_page.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 45,
            row: 14,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(search_page.discovery_col, 1);
        assert_eq!(search_page.local_action_rx.try_recv().unwrap(), SearchPageActions::ScrollDown);

        // Clicking top border of column 0 focuses column 0 without opening any item
        search_page.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 10,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(search_page.discovery_col, 0);
        assert!(search_page.local_action_rx.try_recv().is_err());

        // Clicking bottom border of column 0 does not open any item
        search_page.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 21,
            modifiers: KeyModifiers::NONE,
        });
        assert!(search_page.local_action_rx.try_recv().is_err());

        // Clicking inner row 13 (top border=10, first inner=11 -> visual_y=2 -> 15+2 = 17)
        search_page.handle_mouse_events(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 13,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(search_page.discovery_row[0], 17);
        assert_eq!(search_page.local_action_rx.try_recv().unwrap(), SearchPageActions::SearchMangaPage);
    }
}
