use std::sync::Arc;
use std::thread::sleep;
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
}

/// These are actions that the user actively via key events or mouse events
#[derive(Debug, PartialEq)]
pub enum SearchPageActions {
    StartTyping,
    StopTyping,
    Search,
    ScrollUp,
    ScrollDown,
    ToggleFilters,
    NextPage,
    PreviousPage,
    SearchMangaPage,
    PlanToRead,
    ToggleScope,
    ToggleLanguageFilter,
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
            SearchPageActions::NextPage => self.search_next_page(),
            SearchPageActions::PreviousPage => self.search_previous_page(),
            SearchPageActions::SearchMangaPage => self.search_manga_page(),
            SearchPageActions::PlanToRead => self.plan_to_read(),
            SearchPageActions::ToggleScope => self.toggle_scope(),
            SearchPageActions::ToggleLanguageFilter => self.toggle_language_filter(),
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
                let welcome_block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Rgb(75, 88, 120)))
                    .title(Line::from(vec![Span::styled(
                        tr(" 🧭 Explorador de Mangas ", " 🧭 Manga Explorer "),
                        Style::default().fg(Color::Cyan).bold(),
                    )]));

                let inner = welcome_block.inner(area).inner(Margin {
                    horizontal: 2,
                    vertical: 1,
                });
                welcome_block.render(area, buf);

                let all_providers = [
                    MangaProviders::Mangadex,
                    MangaProviders::Inmanga,
                    MangaProviders::Mangaoni,
                    MangaProviders::Leercapitulo,
                    MangaProviders::Weebcentral,
                    MangaProviders::Mangapill,
                    MangaProviders::Comizy,
                ];

                let mut provider_spans: Vec<Span<'static>> = vec![Span::styled(
                    format!("{} ", tr("Proveedores activos:", "Active providers:")),
                    Style::default().fg(Color::Gray).bold(),
                )];
                for prov in all_providers {
                    let health = crate::backend::multi_provider::get_provider_health_status(prov);
                    provider_spans.push(Span::styled(
                        format!(" {} {} ", health.indicator(), prov),
                        Style::default().fg(Color::White).bg(Color::Rgb(42, 48, 66)).bold(),
                    ));
                    provider_spans.push(Span::raw(" "));
                }

                let mut lines = vec![
                    Line::raw(""),
                    Line::from(vec![Span::styled(
                        tr(
                            "🔍 Pulsa <s> para escribir el título de un manga y <Enter> para buscar",
                            "🔍 Press <s> to type a manga title and <Enter> to search",
                        ),
                        Style::default().fg(Color::Cyan).bold(),
                    )]),
                    Line::from(vec![Span::styled(
                        tr(
                            "Puedes buscar en todos los proveedores a la vez (<Tab/m>) o filtrar por idioma (<l>).",
                            "You can search across all providers at once (<Tab/m>) or filter by language (<l>).",
                        ),
                        Style::default().fg(Color::Gray),
                    )]),
                    Line::raw(""),
                    Line::from(provider_spans),
                ];

                if !self.search_history.is_empty() {
                    lines.push(Line::raw(""));
                    let mut hist_spans: Vec<Span<'static>> = vec![Span::styled(
                        format!("🕒 {} ", tr("Búsquedas recientes (<s> + <↑/↓>):", "Recent searches (<s> + <↑/↓>):")),
                        Style::default().fg(Color::Yellow).bold(),
                    )];
                    for term in self.search_history.iter().take(6) {
                        hist_spans
                            .push(Span::styled(format!(" {term} "), Style::default().fg(Color::Cyan).bg(Color::Rgb(38, 44, 60))));
                        hist_spans.push(Span::raw(" "));
                    }
                    lines.push(Line::from(hist_spans));
                }

                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true })
                    .render(inner, buf);
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
                    )]));
                let inner = block.inner(area).inner(Margin {
                    horizontal: 2,
                    vertical: 2,
                });
                block.render(area, buf);
                Paragraph::new(tr(
                    "No se encontraron mangas con ese nombre. Prueba con otro término o cambia el filtro de idioma (<l>) / alcance (<Tab>).",
                    "No mangas were found. Try another search term or toggle the language filter (<l>) / scope (<Tab>).",
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
                            " ⚠ Error al buscar mangas, inténtalo de nuevo ",
                            " ⚠ An error occurred when searching mangas, please try again ",
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

    fn scroll_down(&mut self) {
        self.mangas_found_list.state.next();
    }

    fn scroll_up(&mut self) {
        self.mangas_found_list.state.previous();
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

        match mouse_event.kind {
            MouseEventKind::ScrollDown => {
                self.local_action_tx.send(SearchPageActions::ScrollDown).ok();
            },
            MouseEventKind::ScrollUp => {
                self.local_action_tx.send(SearchPageActions::ScrollUp).ok();
            },
            MouseEventKind::Down(MouseButton::Left) => {
                let pos = Position::new(mouse_event.column, mouse_event.row);
                if self.manga_area.contains(pos) {
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
}
