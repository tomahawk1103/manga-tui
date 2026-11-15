use std::sync::Arc;
use std::time::Duration;

use chrono::Timelike;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use image::DynamicImage;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, Widget, Wrap};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;

use crate::backend::database::{Database, HomeStats};
use crate::backend::error_log::{ErrorType, write_to_error_log};
use crate::backend::manga_provider::{HomePageMangaProvider, PopularManga, RecentlyAddedManga};
use crate::backend::multi_provider::{ProviderHealthStatus, get_provider_health_status};
use crate::backend::tui::Events;
use crate::common::ImageState;
use crate::config::MangaTuiConfig;
use crate::i18n::tr;
use crate::view::widgets::Component;
use crate::view::widgets::home::{CarrouselItemPopularManga, CarrouselState, PopularMangaCarrousel, RecentlyAddedCarrousel};

#[derive(PartialEq, Eq)]
pub enum HomeState {
    Unused,
}

#[derive(Debug, PartialEq)]
pub enum HomeEvents {
    SearchPopularNewMangas,
    SearchPopularMangasCover,
    SearchRecentlyAddedMangas,
    SearchRecentlyCover,
    LoadPopularMangas(Option<Vec<PopularManga>>),
    LoadRecentlyAddedMangas(Option<Vec<RecentlyAddedManga>>),
    LoadCover(Option<DynamicImage>, String),
    LoadRecentlyAddedMangasCover(Option<DynamicImage>, String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum HomeActions {
    SelectNextPopularManga,
    SelectPreviousPopularManga,
    GoToPopularMangaPage,
    GoToRecentlyAddedMangaPage,
    SelectNextRecentlyAddedManga,
    SelectPreviousRecentlyAddedManga,
    SupportProject,
}

/// "ANSI Shadow" glyphs used to compose the big `MANGA-TUI` hero logo.
const LOGO_GLYPHS: [[&str; 6]; 9] = [
    ["███╗   ███╗", "████╗ ████║", "██╔████╔██║", "██║╚██╔╝██║", "██║ ╚═╝ ██║", "╚═╝     ╚═╝"],
    [" █████╗ ", "██╔══██╗", "███████║", "██╔══██║", "██║  ██║", "╚═╝  ╚═╝"],
    ["███╗   ██╗", "████╗  ██║", "██╔██╗ ██║", "██║╚██╗██║", "██║ ╚████║", "╚═╝  ╚═══╝"],
    [" ██████╗ ", "██╔════╝ ", "██║  ███╗", "██║   ██║", "╚██████╔╝", " ╚═════╝ "],
    [" █████╗ ", "██╔══██╗", "███████║", "██╔══██║", "██║  ██║", "╚═╝  ╚═╝"],
    ["      ", "      ", "█████╗", "╚════╝", "      ", "      "],
    ["████████╗", "╚══██╔══╝", "   ██║   ", "   ██║   ", "   ██║   ", "   ╚═╝   "],
    ["██╗   ██╗", "██║   ██║", "██║   ██║", "██║   ██║", "╚██████╔╝", " ╚═════╝ "],
    ["██╗", "██║", "██║", "██║", "██║", "╚═╝"],
];

/// Height (in rows) of the big hero logo.
const LOGO_HEIGHT: u16 = 6;

/// Base surface colors for the modern dark theme of the Home page.
const SURFACE_BORDER: Color = Color::Rgb(58, 64, 92);
const SURFACE_BORDER_ACCENT: Color = Color::Rgb(78, 118, 190);
const SURFACE_BORDER_VIOLET: Color = Color::Rgb(112, 84, 178);
const SURFACE_MUTED: Color = Color::Rgb(128, 136, 168);
const SURFACE_CHIP: Color = Color::Rgb(34, 38, 58);
const ACCENT_CYAN: Color = Color::Rgb(0, 210, 255);
const ACCENT_VIOLET: Color = Color::Rgb(160, 120, 255);

/// Builds the `MANGA-TUI` hero logo, one `String` per row.
fn build_logo_lines() -> Vec<String> {
    (0..LOGO_HEIGHT as usize)
        .map(|row| LOGO_GLYPHS.iter().map(|glyph| glyph[row]).collect::<String>())
        .collect()
}

/// Display width (in columns) of the big hero logo.
fn logo_width() -> u16 {
    LOGO_GLYPHS.iter().map(|glyph| glyph[0].chars().count() as u16).sum()
}

/// Cyan → violet → pink horizontal gradient used by the hero logo and wordmark.
fn gradient_color(index: usize, total: usize) -> Color {
    const STOPS: [(f32, f32, f32); 3] = [(0.0, 210.0, 255.0), (150.0, 110.0, 255.0), (255.0, 95.0, 200.0)];
    let t = if total <= 1 { 0.0 } else { index.min(total - 1) as f32 / (total - 1) as f32 };
    let (from, to, local_t) = if t < 0.5 { (STOPS[0], STOPS[1], t * 2.0) } else { (STOPS[1], STOPS[2], (t - 0.5) * 2.0) };
    let lerp = |a: f32, b: f32| (a + (b - a) * local_t).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(lerp(from.0, to.0), lerp(from.1, to.1), lerp(from.2, to.2))
}

/// Colors every visible character of `text` with the hero gradient.
fn gradient_line(text: &str, bold: bool) -> Line<'static> {
    let total = text.chars().count();
    let spans: Vec<Span<'static>> = text
        .chars()
        .enumerate()
        .map(|(idx, ch)| {
            let mut style = Style::default().fg(gradient_color(idx, total));
            if bold {
                style = style.bold();
            }
            Span::styled(ch.to_string(), style)
        })
        .collect();
    Line::from(spans)
}

/// Returns an icon + greeting for the given local hour (0-23).
fn greeting_for_hour(hour: u32) -> (&'static str, &'static str) {
    match hour {
        5..=11 => ("☕", tr("Buenos días", "Good morning")),
        12..=19 => ("☀", tr("Buenas tardes", "Good afternoon")),
        _ => ("🌙", tr("Buenas noches", "Good evening")),
    }
}

/// Truncates `text` to at most `max_chars` Unicode scalar values, appending `…` if cut.
fn truncate_home_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        text.to_string()
    } else if max_chars == 1 {
        "…".to_string()
    } else {
        let mut out: String = text.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

/// Compacts big numbers: `999` → `999`, `1250` → `1.3K`, `2_300_000` → `2.3M`.
fn format_count(value: u32) -> String {
    match value {
        0..=999 => value.to_string(),
        1_000..=999_999 => format!("{:.1}K", value as f64 / 1_000.0).replace(".0K", "K"),
        _ => format!("{:.1}M", value as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

/// Which hero header variant fits in the available area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeroMode {
    /// Big gradient ASCII logo + greeting + stat cards / 2x2 pills.
    Full,
    /// Single-line gradient wordmark + greeting + inline stats.
    Compact,
}

impl HeroMode {
    fn for_area(area: Rect) -> Self {
        if area.height >= 26 && area.width >= logo_width() + 36 { Self::Full } else { Self::Compact }
    }

    fn height(self) -> u16 {
        match self {
            Self::Full => LOGO_HEIGHT + 1,
            Self::Compact => 2,
        }
    }
}

fn load_home_stats() -> HomeStats {
    Database::get_connection()
        .and_then(|conn| Database::new(&conn).get_home_stats())
        .unwrap_or_default()
}

pub struct Home<T>
where
    T: HomePageMangaProvider + Sync + Send,
{
    carrousel_popular_mangas: PopularMangaCarrousel,
    carrousel_recently_added: RecentlyAddedCarrousel,
    state: HomeState,
    pub global_event_tx: Option<UnboundedSender<Events>>,
    pub local_action_tx: UnboundedSender<HomeActions>,
    pub local_action_rx: UnboundedReceiver<HomeActions>,
    pub local_event_tx: UnboundedSender<HomeEvents>,
    pub local_event_rx: UnboundedReceiver<HomeEvents>,
    popular_manga_carrousel_state: ImageState,
    recently_added_manga_state: ImageState,
    picker: Option<Picker>,
    tasks: JoinSet<()>,
    manga_provider: Arc<T>,
    popular_mangas_area: Rect,
    recently_added_area: Rect,
    stats: HomeStats,
}

impl<T> Component for Home<T>
where
    T: HomePageMangaProvider + Sync + Send,
{
    type Actions = HomeActions;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let buf = frame.buffer_mut();
        let area = area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });

        let hero_mode = HeroMode::for_area(area);
        let [hero_area, carrousel_popular_mangas_area, latest_updates_area, status_strip_area] = Layout::vertical([
            Constraint::Length(hero_mode.height()),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);

        self.popular_mangas_area = carrousel_popular_mangas_area;
        self.recently_added_area = latest_updates_area;

        self.render_hero(hero_area, buf, hero_mode);
        self.render_popular_mangas_carrousel(carrousel_popular_mangas_area, buf);
        self.render_recently_added_mangas_area(latest_updates_area, buf);
        self.render_status_strip(status_strip_area, buf);
    }

    fn update(&mut self, action: Self::Actions) {
        match action {
            HomeActions::SelectNextPopularManga => {
                self.carrousel_popular_mangas.next_item();
            },
            HomeActions::SelectPreviousPopularManga => self.carrousel_popular_mangas.previous_item(),
            HomeActions::GoToPopularMangaPage => self.go_to_manga_page_popular(),
            HomeActions::SelectNextRecentlyAddedManga => self.carrousel_recently_added.select_next(),
            HomeActions::SelectPreviousRecentlyAddedManga => self.carrousel_recently_added.select_previous(),

            HomeActions::GoToRecentlyAddedMangaPage => {
                self.go_to_manga_page_recently_added();
            },
            HomeActions::SupportProject => self.support_project(),
        }
    }

    fn clean_up(&mut self) {
        self.tasks.abort_all();
        self.carrousel_popular_mangas.items = vec![];
        self.carrousel_recently_added.items = vec![];
        self.state = HomeState::Unused;
        self.recently_added_manga_state = ImageState::default();
        self.popular_manga_carrousel_state = ImageState::default();
    }

    fn handle_events(&mut self, events: Events) {
        match events {
            Events::Key(key_event) => self.handle_key_events(key_event),
            Events::Mouse(mouse_event) => self.handle_mouse_events(mouse_event),
            Events::Tick => self.tick(),
            _ => {},
        }
    }
}

impl<T> Home<T>
where
    T: HomePageMangaProvider + Sync + Send,
{
    pub fn new(picker: Option<Picker>, manga_provider: Arc<T>) -> Self {
        let (local_action_tx, local_action_rx) = mpsc::unbounded_channel::<HomeActions>();
        let (local_event_tx, local_event_rx) = mpsc::unbounded_channel::<HomeEvents>();

        Self {
            carrousel_popular_mangas: PopularMangaCarrousel::default(),
            carrousel_recently_added: RecentlyAddedCarrousel::new(picker.is_some()),
            state: HomeState::Unused,
            global_event_tx: None,
            local_event_tx,
            local_event_rx,
            local_action_tx,
            local_action_rx,
            picker,
            popular_manga_carrousel_state: ImageState::default(),
            recently_added_manga_state: ImageState::default(),
            tasks: JoinSet::new(),
            manga_provider,
            popular_mangas_area: Rect::default(),
            recently_added_area: Rect::default(),
            stats: load_home_stats(),
        }
    }

    /// Reloads the user statistics shown in the hero header (library, chapters read, ...).
    pub fn refresh_stats(&mut self) {
        self.stats = load_home_stats();
    }

    pub fn with_global_sender(mut self, tx: UnboundedSender<Events>) -> Self {
        self.global_event_tx = Some(tx);
        self
    }

    pub fn render_popular_mangas_carrousel(&mut self, area: Rect, buf: &mut Buffer) {
        let total = self.carrousel_popular_mangas.items.len();
        let current = if total == 0 { 0 } else { self.carrousel_popular_mangas.current_item_visible_index + 1 };

        let mut top_left_spans = vec![Span::styled(
            tr(" 🔥 Títulos Populares ", " 🔥 Popular New Titles "),
            Style::default().fg(ACCENT_CYAN).bg(Color::Rgb(20, 42, 64)).bold(),
        )];
        if total > 0 {
            top_left_spans.push(Span::styled(format!("  {current}/{total} "), Style::default().fg(SURFACE_MUTED)));
        }

        let mut dot_spans = Vec::new();
        if total > 0 {
            dot_spans.push(Span::raw(" "));
            let max_dots = total.min(15);
            let active_dot = self.carrousel_popular_mangas.current_item_visible_index % max_dots;
            for idx in 0..max_dots {
                if idx == active_dot {
                    dot_spans.push(Span::styled("● ", Style::default().fg(ACCENT_CYAN).bold()));
                } else {
                    dot_spans.push(Span::styled("· ", Style::default().fg(SURFACE_BORDER)));
                }
            }
        }

        let instructions = Line::from(vec![
            Span::raw(" "),
            Self::key_chip("b"),
            Span::styled(tr(" Anterior  ", " Previous  "), Style::default().fg(SURFACE_MUTED)),
            Self::key_chip("w"),
            Span::styled(tr(" Siguiente  ", " Next  "), Style::default().fg(SURFACE_MUTED)),
            Span::styled(" r ", Style::default().fg(Color::Black).bg(ACCENT_CYAN).bold()),
            Span::styled(tr(" Leer manga ", " Read manga "), Style::default().fg(Color::White).bold()),
        ]);

        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(SURFACE_BORDER_ACCENT))
            .title(Line::from(top_left_spans))
            .title_bottom(instructions);

        if !dot_spans.is_empty() {
            block = block.title_top(Line::from(dot_spans).right_aligned());
        }

        let inner = block.inner(area);
        block.render(area, buf);

        StatefulWidget::render(self.carrousel_popular_mangas.clone(), inner, buf, &mut self.popular_manga_carrousel_state);
    }

    fn go_to_manga_page(&self, manga_id: String) {
        let client = Arc::clone(&self.manga_provider);
        let tx = self.global_event_tx.as_ref().unwrap().clone();
        tokio::spawn(async move {
            let response = client.get_manga_by_id(&manga_id).await;
            match response {
                Ok(res) => {
                    tx.send(Events::GoToMangaPage(res)).ok();
                },
                Err(e) => {
                    tx.send(Events::Error(e.to_string())).ok();
                    write_to_error_log(e.into());
                },
            }
        });
    }

    fn go_to_manga_page_popular(&self) {
        if let Some(item) = self.get_current_popular_manga() {
            self.go_to_manga_page(item.manga.id.clone());
        }
    }

    fn go_to_manga_page_recently_added(&self) {
        if let Some(item) = self.carrousel_recently_added.get_current_selected_manga() {
            self.go_to_manga_page(item.manga.id.clone());
        }
    }

    fn get_current_popular_manga(&self) -> Option<&CarrouselItemPopularManga> {
        self.carrousel_popular_mangas.get_current_item()
    }

    pub fn require_search(&mut self) -> bool {
        self.carrousel_popular_mangas.items.is_empty() || self.carrousel_recently_added.items.is_empty()
    }

    pub fn init_search(&mut self) {
        self.refresh_stats();
        self.local_event_tx.send(HomeEvents::SearchPopularNewMangas).ok();

        self.local_event_tx.send(HomeEvents::SearchRecentlyAddedMangas).ok();
    }

    pub fn init_search_popular_mangas_cover(&self) {
        if self.picker.is_some() {
            self.local_event_tx.send(HomeEvents::SearchPopularMangasCover).ok();
        }
    }

    pub fn init_search_recently_added_mangas_cover(&self) {
        if self.picker.is_some() {
            self.local_event_tx.send(HomeEvents::SearchRecentlyCover).ok();
        }
    }

    pub fn tick(&mut self) {
        self.carrousel_popular_mangas.tick();
        self.carrousel_recently_added.tick();
        while let Ok(local_event) = self.local_event_rx.try_recv() {
            match local_event {
                HomeEvents::SearchPopularMangasCover => self.search_popular_mangas_cover(),
                HomeEvents::SearchPopularNewMangas => self.search_popular_mangas(),
                HomeEvents::LoadPopularMangas(maybe_response) => {
                    self.load_popular_mangas(maybe_response);
                },
                HomeEvents::LoadCover(maybe_cover, index) => self.load_popular_manga_cover(maybe_cover, index),
                HomeEvents::SearchRecentlyAddedMangas => {
                    self.search_recently_added_mangas();
                },
                HomeEvents::LoadRecentlyAddedMangas(maybe_response) => {
                    self.load_recently_added_mangas(maybe_response);
                },
                HomeEvents::SearchRecentlyCover => {
                    self.search_recently_added_mangas_cover();
                },
                HomeEvents::LoadRecentlyAddedMangasCover(maybe_image, id) => {
                    self.load_recently_added_mangas_cover(maybe_image, id);
                },
            }
        }
    }

    fn load_popular_mangas(&mut self, maybe_response: Option<Vec<PopularManga>>) {
        match maybe_response {
            Some(response) => {
                self.carrousel_popular_mangas = PopularMangaCarrousel::from_response(response, self.picker.is_some());
                self.init_search_popular_mangas_cover();
            },
            None => {
                self.carrousel_popular_mangas.state = CarrouselState::NotFound;
            },
        }
    }

    fn load_popular_manga_cover(&mut self, maybe_cover: Option<DynamicImage>, id: String) {
        if let Some(cover) = maybe_cover
            && let Some(picker) = self.picker.as_mut()
        {
            let protocol = picker.new_resize_protocol(cover);
            self.popular_manga_carrousel_state.insert_manga(protocol, id);
        }
    }

    fn search_popular_mangas(&mut self) {
        let tx = self.local_event_tx.clone();
        self.carrousel_popular_mangas.state = CarrouselState::Searching;
        let manga_provider = self.manga_provider.clone();
        self.tasks.spawn(async move {
            let response = manga_provider.get_popular_mangas().await;
            match response {
                Ok(res) => {
                    if res.is_empty() {
                        tx.send(HomeEvents::LoadPopularMangas(None)).ok();
                    } else {
                        tx.send(HomeEvents::LoadPopularMangas(Some(res))).ok();
                    }
                },
                Err(e) => {
                    write_to_error_log(ErrorType::Error(e));
                    tx.send(HomeEvents::LoadPopularMangas(None)).ok();
                },
            }
        });
    }

    fn search_popular_mangas_cover(&mut self) {
        let mangas = self.carrousel_popular_mangas.items.clone();

        let tx = self.local_event_tx.clone();
        let client = Arc::clone(&self.manga_provider);
        self.tasks.spawn(async move {
            for item in mangas {
                let response = client.get_image(&item.manga.cover_img_url).await;
                if let Ok(res) = response {
                    tx.send(HomeEvents::LoadCover(Some(res), item.manga.id)).ok();
                }
            }
        });
    }

    fn search_recently_added_mangas(&mut self) {
        let tx = self.local_event_tx.clone();
        self.carrousel_recently_added.state = CarrouselState::Searching;
        let client = Arc::clone(&self.manga_provider);
        self.tasks.spawn(async move {
            let response = client.get_recently_added_mangas().await;
            match response {
                Ok(mangas) => {
                    tx.send(HomeEvents::LoadRecentlyAddedMangas(Some(mangas))).ok();
                },
                Err(e) => {
                    write_to_error_log(e.into());
                    tx.send(HomeEvents::LoadRecentlyAddedMangas(None)).ok();
                },
            }
        });
    }

    fn load_recently_added_mangas(&mut self, maybe_response: Option<Vec<RecentlyAddedManga>>) {
        match maybe_response {
            Some(response) => {
                self.carrousel_recently_added = RecentlyAddedCarrousel::from_response(response, self.picker.is_some());
                self.init_search_recently_added_mangas_cover();
            },
            None => {
                self.carrousel_recently_added.state = CarrouselState::NotFound;
            },
        }
    }

    fn search_recently_added_mangas_cover(&mut self) {
        let mangas = self.carrousel_recently_added.items.clone();
        let tx = self.local_event_tx.clone();
        let client = Arc::clone(&self.manga_provider);
        self.tasks.spawn(async move {
            for item in mangas {
                let response = client.get_image(&item.manga.cover_img_url).await;
                if let Ok(res) = response {
                    tx.send(HomeEvents::LoadRecentlyAddedMangasCover(Some(res), item.manga.id)).ok();
                } else {
                    tx.send(HomeEvents::LoadRecentlyAddedMangasCover(None, item.manga.id)).ok();
                }
            }
        });
    }

    fn load_recently_added_mangas_cover(&mut self, maybe_cover: Option<DynamicImage>, id: String) {
        if let Some(cover) = maybe_cover
            && let Some(picker) = self.picker.as_mut()
        {
            let protocol = picker.new_resize_protocol(cover);
            self.recently_added_manga_state.insert_manga(protocol, id);
        }
    }

    fn support_project(&mut self) {
        open::that("https://github.com/josueBarretogit/manga-tui").ok();
    }

    fn render_recently_added_mangas_area(&mut self, recently_added_mangas_area: Rect, buf: &mut Buffer) {
        self.recently_added_area = recently_added_mangas_area;

        let total = self.carrousel_recently_added.items.len();
        let current = if total == 0 { 0 } else { self.carrousel_recently_added.selected_item_index + 1 };

        let mut top_left_spans = vec![Span::styled(
            tr(" ✨ Añadidos Recientemente ", " ✨ Recently Added "),
            Style::default().fg(ACCENT_VIOLET).bg(Color::Rgb(38, 28, 60)).bold(),
        )];
        if total > 0 {
            top_left_spans.push(Span::styled(format!("  {current}/{total} "), Style::default().fg(SURFACE_MUTED)));
        }

        let mut page_spans = Vec::new();
        if total > 0 {
            let per_page = self.carrousel_recently_added.amount_items_per_page.max(1);
            let total_pages = total.div_ceil(per_page);
            let current_page = (self.carrousel_recently_added.selected_item_index / per_page) + 1;
            page_spans.push(Span::styled(
                format!(" {} {current_page}/{total_pages} ", tr("Pág.", "Page")),
                Style::default().fg(SURFACE_MUTED),
            ));
        }

        let instructions = Line::from(vec![
            Span::raw(" "),
            Self::key_chip("h / ←"),
            Span::styled(tr(" Izquierda  ", " Left  "), Style::default().fg(SURFACE_MUTED)),
            Self::key_chip("l / →"),
            Span::styled(tr(" Derecha  ", " Right  "), Style::default().fg(SURFACE_MUTED)),
            Span::styled(" Enter ", Style::default().fg(Color::Black).bg(ACCENT_VIOLET).bold()),
            Span::styled(tr(" Abrir ficha ", " Open manga "), Style::default().fg(Color::White).bold()),
        ]);

        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(SURFACE_BORDER_VIOLET))
            .title(Line::from(top_left_spans))
            .title_bottom(instructions);

        if !page_spans.is_empty() {
            block = block.title_top(Line::from(page_spans).right_aligned());
        }

        let inner_area = block.inner(recently_added_mangas_area);
        block.render(recently_added_mangas_area, buf);

        StatefulWidget::render(self.carrousel_recently_added.clone(), inner_area, buf, &mut self.recently_added_manga_state);
    }

    fn render_hero(&self, area: Rect, buf: &mut Buffer, mode: HeroMode) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        match mode {
            HeroMode::Full => self.render_full_hero(area, buf),
            HeroMode::Compact => self.render_compact_hero(area, buf),
        }
    }

    fn greeting_spans(&self) -> Vec<Span<'static>> {
        let (icon, greeting) = greeting_for_hour(chrono::Local::now().hour());
        vec![
            Span::styled(format!("{icon}  "), Style::default().fg(Color::Rgb(255, 200, 80))),
            Span::styled(format!("{greeting}, "), Style::default().fg(Color::White).bold()),
            Span::styled(tr("lector", "reader").to_string(), Style::default().fg(gradient_color(1, 2)).bold()),
        ]
    }

    fn stat_entries(&self, compact_labels: bool) -> [(&'static str, String, &'static str, Color); 4] {
        if compact_labels {
            [
                ("📚", format_count(self.stats.library_mangas), tr("Biblio.", "Lib."), ACCENT_CYAN),
                ("✔", format_count(self.stats.chapters_read), tr("Leídos", "Read"), Color::Rgb(110, 220, 140)),
                ("⬇", format_count(self.stats.downloaded_chapters), tr("Desc.", "Down."), ACCENT_VIOLET),
                ("★", format_count(self.stats.favorites), tr("Favs.", "Favs."), Color::Rgb(255, 200, 80)),
            ]
        } else {
            [
                ("📚", format_count(self.stats.library_mangas), tr("Biblioteca", "Library"), ACCENT_CYAN),
                ("✔", format_count(self.stats.chapters_read), tr("Caps. leídos", "Chapters read"), Color::Rgb(110, 220, 140)),
                ("⬇", format_count(self.stats.downloaded_chapters), tr("Descargas", "Downloads"), ACCENT_VIOLET),
                ("★", format_count(self.stats.favorites), tr("Favoritos", "Favorites"), Color::Rgb(255, 200, 80)),
            ]
        }
    }

    fn key_chip(key: &str) -> Span<'static> {
        Span::styled(format!(" {key} "), Style::default().fg(ACCENT_CYAN).bg(SURFACE_CHIP).bold())
    }

    fn continue_reading_line(&self, max_width: u16) -> Line<'static> {
        let muted = Style::default().fg(SURFACE_MUTED);
        match &self.stats.last_read_title {
            Some(title) => {
                // Prefix "📖 Continúa con " (17) + suffix "  ·   o  Biblioteca" (~19) = 36 cols overhead
                let max_title = (max_width as usize).saturating_sub(36).max(8);
                let display_title = truncate_home_text(title, max_title);
                Line::from(vec![
                    Span::styled(tr("📖 Continúa con ", "📖 Continue with "), muted),
                    Span::styled(display_title, Style::default().fg(Color::White).bold().italic()),
                    Span::styled("  ·  ", muted),
                    Self::key_chip("o"),
                    Span::styled(tr(" Biblioteca", " Library"), muted),
                ])
            },
            None => Line::from(vec![
                Span::styled(tr("✨ Descubre algo nuevo hoy  ·  ", "✨ Discover something new today  ·  "), muted),
                Self::key_chip("i"),
                Span::styled(tr(" Buscar", " Search"), muted),
            ]),
        }
    }

    fn inline_stats_line(&self, compact_labels: bool, numbers_only: bool) -> Line<'static> {
        let mut spans = Vec::new();
        for (idx, (icon, value, label, color)) in self.stat_entries(compact_labels).into_iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(if numbers_only { " " } else { "  " }));
            }
            spans.push(Span::styled(format!(" {icon} {value} "), Style::default().fg(color).bg(SURFACE_CHIP).bold()));
            if !numbers_only {
                spans.push(Span::styled(format!(" {label}"), Style::default().fg(SURFACE_MUTED)));
            }
        }
        Line::from(spans)
    }

    fn render_full_hero(&self, area: Rect, buf: &mut Buffer) {
        let [logo_area, info_area] = Layout::horizontal([Constraint::Length(logo_width()), Constraint::Fill(1)])
            .spacing(3)
            .areas(area);

        // One row of breathing space above the logo.
        let logo_area = Rect {
            y: logo_area.y.saturating_add(1),
            height: logo_area.height.saturating_sub(1),
            ..logo_area
        };
        let logo_lines: Vec<Line<'static>> = build_logo_lines().iter().map(|row| gradient_line(row, false)).collect();
        Paragraph::new(logo_lines).render(logo_area, buf);

        let [_pad, greeting_area, continue_area, tagline_area, cards_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .areas(info_area);

        Paragraph::new(Line::from(self.greeting_spans())).render(greeting_area, buf);
        Paragraph::new(self.continue_reading_line(info_area.width)).render(continue_area, buf);
        Paragraph::new(Line::from(Span::styled(
            tr("Tu centro de lectura manga en terminal", "Your terminal manga reading hub"),
            Style::default().fg(Color::Rgb(95, 102, 138)).italic(),
        )))
        .render(tagline_area, buf);

        if cards_area.width >= 74 {
            let card_width = ((cards_area.width.saturating_sub(3)) / 4).min(21);
            let compact_labels = card_width < 20;
            let card_areas = Layout::horizontal([Constraint::Length(card_width); 4])
                .spacing(1)
                .flex(Flex::Start)
                .split(cards_area);
            for ((icon, value, label, color), card_area) in self.stat_entries(compact_labels).into_iter().zip(card_areas.iter()) {
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(SURFACE_BORDER));
                let inner = block.inner(*card_area);
                block.render(*card_area, buf);
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::styled(value, Style::default().fg(color).bold()),
                    Span::styled(format!(" {label}"), Style::default().fg(SURFACE_MUTED)),
                ]))
                .centered()
                .render(inner, buf);
            }
        } else {
            // Standard terminals (~112-150 cols): render a 2x2 grid of clean pills so all 4 stats stay legible.
            let [_gap, row1, row2] =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)]).areas(cards_area);
            let entries = self.stat_entries(cards_area.width < 46);
            for (pair, row_area) in entries.chunks(2).zip([row1, row2]) {
                let [left_col, right_col] =
                    Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(row_area);
                for ((icon, value, label, color), cell) in pair.iter().zip([left_col, right_col]) {
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!(" {icon} {value} "), Style::default().fg(*color).bg(SURFACE_CHIP).bold()),
                        Span::styled(format!(" {label}"), Style::default().fg(SURFACE_MUTED)),
                    ]))
                    .render(cell, buf);
                }
            }
        }
    }

    fn render_compact_hero(&self, area: Rect, buf: &mut Buffer) {
        let [top_row, bottom_row] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

        let mut left_spans = vec![Span::styled("◆ ", Style::default().fg(gradient_color(0, 2)).bold())];
        left_spans.extend(gradient_line("manga-tui", true).spans);
        left_spans.push(Span::styled("   ", Style::default()));
        left_spans.extend(self.greeting_spans());
        let left = Line::from(left_spans);
        let left_width = left.width() as u16;
        Paragraph::new(left).render(top_row, buf);

        let stats = [self.inline_stats_line(false, false), self.inline_stats_line(true, false), self.inline_stats_line(true, true)]
            .into_iter()
            .find(|line| left_width + line.width() as u16 + 2 <= top_row.width);

        if let Some(stats_line) = stats {
            Paragraph::new(stats_line).right_aligned().render(top_row, buf);
        }

        Paragraph::new(self.continue_reading_line(bottom_row.width)).render(bottom_row, buf);
    }

    fn render_status_strip(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let provider_name = self.manga_provider.name();
        let (health_label, health_color) = match get_provider_health_status(provider_name) {
            ProviderHealthStatus::Healthy => ("● Online", Color::Rgb(110, 220, 140)),
            ProviderHealthStatus::Degraded => (tr("◐ Lento", "◐ Slow"), Color::Yellow),
            ProviderHealthStatus::CircuitOpen => (tr("⚠ Caído", "⚠ Down"), Color::LightRed),
        };

        let config = MangaTuiConfig::get();
        let separator = Span::styled("  │  ", Style::default().fg(SURFACE_BORDER));
        let muted = Style::default().fg(SURFACE_MUTED);

        let left = Line::from(vec![
            Span::styled(
                format!(" ⚡ v{} ", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::Black).bg(gradient_color(1, 2)).bold(),
            ),
            Span::raw("  "),
            Span::styled(tr("📡 Fuente ", "📡 Source "), muted),
            Span::styled(format!(" {provider_name} "), Style::default().fg(Color::White).bg(SURFACE_CHIP).bold()),
            Span::styled(format!(" {health_label}"), Style::default().fg(health_color).bold()),
            separator.clone(),
            Span::styled(tr("📦 Formato ", "📦 Format "), muted),
            Span::styled(config.download_type.to_string().to_uppercase(), Style::default().fg(Color::Cyan).bold()),
            separator,
            Span::styled(tr("🌐 Idioma ", "🌐 Language "), muted),
            Span::styled(config.default_language.to_string(), Style::default().fg(Color::Cyan).bold()),
        ]);
        let left_width = left.width() as u16;
        Paragraph::new(left).render(area, buf);

        let right = Line::from(vec![Self::key_chip("g"), Span::styled(tr(" GitHub ", " GitHub "), muted)]);
        if left_width + right.width() as u16 + 2 <= area.width {
            Paragraph::new(right).right_aligned().render(area, buf);
        }
    }

    fn handle_mouse_events(&mut self, mouse_event: MouseEvent) {
        let pos = Position::new(mouse_event.column, mouse_event.row);
        let in_popular = self.popular_mangas_area.contains(pos);

        match mouse_event.kind {
            MouseEventKind::ScrollUp => {
                if in_popular {
                    self.local_action_tx.send(HomeActions::SelectPreviousPopularManga).ok();
                } else {
                    self.local_action_tx.send(HomeActions::SelectPreviousRecentlyAddedManga).ok();
                }
            },
            MouseEventKind::ScrollDown => {
                if in_popular {
                    self.local_action_tx.send(HomeActions::SelectNextPopularManga).ok();
                } else {
                    self.local_action_tx.send(HomeActions::SelectNextRecentlyAddedManga).ok();
                }
            },
            MouseEventKind::Down(MouseButton::Left) => {
                if in_popular {
                    self.local_action_tx.send(HomeActions::GoToPopularMangaPage).ok();
                } else if self.recently_added_area.contains(pos) {
                    let inner_width = self.recently_added_area.width.saturating_sub(2).max(1);
                    let rel_x = pos.x.saturating_sub(self.recently_added_area.x.saturating_add(1));
                    let per_page = self.carrousel_recently_added.amount_items_per_page.max(1);
                    let col_idx = ((rel_x as usize) * per_page / (inner_width as usize)).min(per_page - 1);
                    let page_start = (self.carrousel_recently_added.selected_item_index / per_page) * per_page;
                    let clicked_index = page_start + col_idx;
                    if clicked_index < self.carrousel_recently_added.items.len() {
                        self.carrousel_recently_added.selected_item_index = clicked_index;
                    }
                    self.local_action_tx.send(HomeActions::GoToRecentlyAddedMangaPage).ok();
                }
            },
            _ => {},
        }
    }

    pub fn handle_key_events(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('w') => {
                self.local_action_tx.send(HomeActions::SelectNextPopularManga).ok();
            },

            KeyCode::Char('b') => {
                self.local_action_tx.send(HomeActions::SelectPreviousPopularManga).ok();
            },
            KeyCode::Char('r') => {
                self.local_action_tx.send(HomeActions::GoToPopularMangaPage).ok();
            },
            KeyCode::Char('l') | KeyCode::Right => {
                self.local_action_tx.send(HomeActions::SelectNextRecentlyAddedManga).ok();
            },
            KeyCode::Char('h') | KeyCode::Left => {
                self.local_action_tx.send(HomeActions::SelectPreviousRecentlyAddedManga).ok();
            },
            KeyCode::Enter => {
                self.local_action_tx.send(HomeActions::GoToRecentlyAddedMangaPage).ok();
            },
            KeyCode::Char('g') => {
                self.local_action_tx.send(HomeActions::SupportProject).ok();
            },
            _ => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::manga_provider::mock::MockMangaPageProvider;

    #[test]
    fn searches_popular_manga_cover_after_mangas_are_loaded_if_picker_is_some() {
        let mut home: Home<MockMangaPageProvider> = Home::new(Some(Picker::new((8, 8))), MockMangaPageProvider::new().into());

        home.load_popular_mangas(Some(vec![PopularManga::default()]));

        let event = home.local_event_rx.blocking_recv().expect("no event was");

        assert_eq!(event, HomeEvents::SearchPopularMangasCover)
    }
    #[test]
    fn searches_recently_added_manga_cover_after_mangas_are_loaded_if_picker_is_some() {
        let mut home: Home<MockMangaPageProvider> = Home::new(Some(Picker::new((8, 8))), MockMangaPageProvider::new().into());

        home.load_recently_added_mangas(Some(vec![RecentlyAddedManga::default()]));

        let event = home.local_event_rx.blocking_recv().expect("no event was");

        assert_eq!(event, HomeEvents::SearchRecentlyCover)
    }

    #[test]
    fn doesnt_search_manga_cover_if_picker_is_none() {
        let mut home: Home<MockMangaPageProvider> = Home::new(None, MockMangaPageProvider::new().into());

        home.load_popular_mangas(Some(vec![PopularManga::default()]));

        assert!(home.local_event_rx.is_empty());

        home.load_recently_added_mangas(Some(vec![RecentlyAddedManga::default()]));

        assert!(home.local_event_rx.is_empty());
    }

    #[test]
    fn hero_mode_adapts_to_terminal_size() {
        assert_eq!(HeroMode::for_area(Rect::new(0, 0, 160, 40)), HeroMode::Full);
        assert_eq!(HeroMode::for_area(Rect::new(0, 0, 118, 27)), HeroMode::Full);
        assert_eq!(HeroMode::for_area(Rect::new(0, 0, 90, 35)), HeroMode::Compact);
        assert_eq!(HeroMode::for_area(Rect::new(0, 0, 150, 20)), HeroMode::Compact);
    }

    #[test]
    fn logo_lines_and_gradient_have_consistent_dimensions() {
        let lines = build_logo_lines();
        let expected_width = logo_width() as usize;
        assert_eq!(lines.len(), LOGO_HEIGHT as usize);
        for row in &lines {
            assert_eq!(row.chars().count(), expected_width);
        }

        let styled = gradient_line("manga-tui", true);
        assert_eq!(styled.spans.len(), "manga-tui".chars().count());
    }

    #[test]
    fn greeting_count_and_truncate_helpers() {
        assert_eq!(greeting_for_hour(8).0, "☕");
        assert_eq!(greeting_for_hour(15).0, "☀");
        assert_eq!(greeting_for_hour(23).0, "🌙");

        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_000), "1K");
        assert_eq!(format_count(1_200), "1.2K");
        assert_eq!(format_count(2_300_000), "2.3M");

        assert_eq!(truncate_home_text("Berserk", 10), "Berserk");
        assert_eq!(truncate_home_text("進撃の巨人 ñandú", 5), "進撃の巨…");
    }

    #[test]
    fn home_renders_cleanly_across_sizes_with_stats() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut home: Home<MockMangaPageProvider> = Home::new(None, MockMangaPageProvider::new().into());
        home.stats = HomeStats {
            library_mangas: 14,
            chapters_read: 1_240,
            downloaded_chapters: 38,
            favorites: 7,
            last_read_title: Some("Berserk: Golden Age Arc Edition".to_string()),
        };

        for (width, height) in [(165, 42), (118, 27), (90, 24), (45, 14)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let area = Rect::new(0, 0, width, height);
            terminal
                .draw(|frame| {
                    home.render(area, frame);
                })
                .unwrap();

            let buf = terminal.backend().buffer();
            let text: String = buf.content().iter().map(|c| c.symbol()).collect::<Vec<_>>().join("");
            if width >= 90 {
                assert!(text.contains("1.2K"), "expected formatted chapters_read in {width}x{height}");
                assert!(text.contains("Populares") || text.contains("Popular"), "expected popular section in {width}x{height}");
            }
        }
    }
}
