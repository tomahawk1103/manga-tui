use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use image::DynamicImage;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, Widget, Wrap};
use ratatui_image::picker::Picker;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;

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
}

impl<T> Component for Home<T>
where
    T: HomePageMangaProvider + Sync + Send,
{
    type Actions = HomeActions;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let layout = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).margin(1);
        let buf = frame.buffer_mut();

        let [carrousel_popular_mangas_area, latest_updates_area] = layout.areas(area);

        self.popular_mangas_area = carrousel_popular_mangas_area;
        self.recently_added_area = latest_updates_area;

        self.render_popular_mangas_carrousel(carrousel_popular_mangas_area, buf);

        self.render_recently_added_mangas_area(latest_updates_area, buf);
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
        }
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
            Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
        )];
        if total > 0 {
            top_left_spans.push(Span::styled(format!(" [ {current} / {total} ] "), Style::default().fg(Color::Yellow).bold()));
        }

        let mut dot_spans = Vec::new();
        if total > 0 {
            dot_spans.push(Span::raw(" "));
            let max_dots = total.min(15);
            let active_dot = self.carrousel_popular_mangas.current_item_visible_index % max_dots;
            for idx in 0..max_dots {
                if idx == active_dot {
                    dot_spans.push(Span::styled("● ", Style::default().fg(Color::Cyan).bold()));
                } else {
                    dot_spans.push(Span::styled("○ ", Style::default().fg(Color::DarkGray)));
                }
            }
        }

        let instructions = Line::from(vec![
            Span::raw(" "),
            Span::styled(" <b> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
            Span::styled(tr(" Anterior  ", " Previous  "), Style::default().fg(Color::Gray)),
            Span::styled(" <w> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
            Span::styled(tr(" Siguiente  ", " Next  "), Style::default().fg(Color::Gray)),
            Span::styled(" <r> ", Style::default().fg(Color::Black).bg(Color::Cyan).bold()),
            Span::styled(tr(" Leer manga ", " Read manga "), Style::default().fg(Color::White).bold()),
        ]);

        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
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

    fn render_recently_added_mangas_area(&mut self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::horizontal([Constraint::Percentage(28), Constraint::Percentage(72)]);
        let [app_info_area, recently_added_mangas_area] = layout.areas(area);

        self.recently_added_area = recently_added_mangas_area;
        self.render_app_information(app_info_area, buf);

        let total = self.carrousel_recently_added.items.len();
        let current = if total == 0 { 0 } else { self.carrousel_recently_added.selected_item_index + 1 };

        let mut top_left_spans = vec![Span::styled(
            tr(" ✨ Añadidos Recientemente ", " ✨ Recently Added "),
            Style::default().fg(Color::Black).bg(Color::Yellow).bold(),
        )];
        if total > 0 {
            top_left_spans.push(Span::styled(format!(" [ {current} / {total} ] "), Style::default().fg(Color::Yellow).bold()));
        }

        let mut page_spans = Vec::new();
        if total > 0 {
            let per_page = self.carrousel_recently_added.amount_items_per_page.max(1);
            let total_pages = total.div_ceil(per_page);
            let current_page = (self.carrousel_recently_added.selected_item_index / per_page) + 1;
            page_spans.push(Span::styled(
                format!(" {} {current_page}/{total_pages} ", tr("Pág.", "Page")),
                Style::default().fg(Color::DarkGray),
            ));
        }

        let instructions = Line::from(vec![
            Span::raw(" "),
            Span::styled(" <h / ←> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
            Span::styled(tr(" Izquierda  ", " Left  "), Style::default().fg(Color::Gray)),
            Span::styled(" <l / →> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
            Span::styled(tr(" Derecha  ", " Right  "), Style::default().fg(Color::Gray)),
            Span::styled(" <Enter> ", Style::default().fg(Color::Black).bg(Color::Yellow).bold()),
            Span::styled(tr(" Abrir ficha ", " Open manga "), Style::default().fg(Color::White).bold()),
        ]);

        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .title(Line::from(top_left_spans))
            .title_bottom(instructions);

        if !page_spans.is_empty() {
            block = block.title_top(Line::from(page_spans).right_aligned());
        }

        let inner_area = block.inner(recently_added_mangas_area);
        block.render(recently_added_mangas_area, buf);

        StatefulWidget::render(self.carrousel_recently_added.clone(), inner_area, buf, &mut self.recently_added_manga_state);
    }

    fn render_app_information(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(80, 90, 125)))
            .title(Line::from(vec![Span::styled(
                format!(" ⚡ manga-tui v{} ", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::Black).bg(Color::Magenta).bold(),
            )]));

        let inner = block.inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let provider_name = self.manga_provider.name();
        let health = get_provider_health_status(provider_name);
        let (health_dot, health_style) = match health {
            ProviderHealthStatus::Healthy => (tr(" ● Online", " ● Online"), Style::default().fg(Color::Green).bold()),
            ProviderHealthStatus::Degraded => (tr(" ◐ Lento", " ◐ Slow"), Style::default().fg(Color::Yellow).bold()),
            ProviderHealthStatus::CircuitOpen => (tr(" ⚠ Caído", " ⚠ Down"), Style::default().fg(Color::Red).bold()),
        };

        let config = MangaTuiConfig::get();
        let format_pill = Span::styled(
            format!(" {} ", config.download_type.to_string().to_uppercase()),
            Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
        );
        let lang_pill = Span::styled(
            format!(" {} ", config.default_language),
            Style::default().fg(Color::White).bg(Color::Rgb(42, 49, 72)).bold(),
        );

        let lines = vec![
            Line::from(vec![
                Span::styled(tr("📡 Fuente: ", "📡 Source: "), Style::default().fg(Color::Gray)),
                Span::styled(format!(" {} ", provider_name), Style::default().fg(Color::White).bg(Color::Blue).bold()),
                Span::styled(health_dot, health_style),
            ]),
            Line::from(vec![
                Span::styled(tr("📦 Config: ", "📦 Config: "), Style::default().fg(Color::Gray)),
                format_pill,
                Span::raw(" "),
                lang_pill,
            ]),
            Line::from(Span::styled("──────────────────────────", Style::default().fg(Color::Rgb(50, 56, 78)))),
            Line::from(vec![
                Span::styled(" <f> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                Span::styled(tr(" Ir al Buscador", " Go to Search"), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled(" <d> ", Style::default().fg(Color::Yellow).bg(Color::DarkGray).bold()),
                Span::styled(tr(" Abrir Biblioteca", " Open Library"), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled(" <n> ", Style::default().fg(Color::Cyan).bg(Color::DarkGray).bold()),
                Span::styled(tr(" Guía de controles", " Controls guide"), Style::default().fg(Color::Gray)),
            ]),
            Line::from(vec![
                Span::styled(" <g> ", Style::default().fg(Color::Magenta).bg(Color::DarkGray).bold()),
                Span::styled(tr(" GitHub del proyecto", " Project GitHub"), Style::default().fg(Color::Gray)),
            ]),
        ];

        Paragraph::new(lines).wrap(Wrap { trim: true }).render(inner, buf);
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
}
