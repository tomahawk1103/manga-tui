//! Feed page module for displaying and managing manga reading history and plan-to-read lists.
//!
//! This module provides the main feed interface where users can:
//! - View their reading history and plan-to-read manga
//! - Search and filter manga by title
//! - Navigate between different manga providers
//! - Delete individual manga or clear entire collections
//! - Navigate to detailed manga pages
//! - View latest chapters for each manga
//!
//! ## Key Features
//!
//! ### Dual Tab Interface
//! - **Reading History**: Shows manga the user has actually read
//! - **Plan to Read**: Shows manga the user wants to read later
//!
//! ### Search and Navigation
//! - Real-time search filtering by manga title
//! - Pagination support for large collections
//! - Keyboard navigation (j/k for up/down, w/b for next/previous page)
//! - Mouse wheel scrolling support
//!
//! ### Manga Management
//! - Delete individual manga from history (`d` key)
//! - Bulk delete all manga from a provider (`D` key with confirmation)
//! - Navigate to detailed manga pages (`r` key)
//!
//! ### Async Data Loading
//! - Background loading of latest chapters for each manga
//! - Loading states with visual feedback
//! - Error handling for failed API requests
//!
//! ## State Management
//!
//! The feed uses a state machine to manage different UI states:
//! - `DisplayingHistory`: Normal display mode
//! - `SearchingHistory`: Loading history from database
//! - `SearchingMangaPage`: Loading detailed manga data
//! - `AskingDeleteAllConfirmation`: Confirmation dialog for bulk deletion
//! - Various error states for failed operations
//!
//! ## Event System
//!
//! Uses a dual-channel event system:
//! - **Actions**: User interactions (key presses, mouse events)
//! - **Events**: Internal state changes and async results
//!
//! This separation allows for responsive UI while handling async operations.
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use manga_tui::SearchTerm;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, ToSpan};
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, Widget, Wrap};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinSet;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::backend::database::{Database, GetHistoryArgs, MangaHistoryResponse, MangaHistoryType};
use crate::backend::error_log::{ErrorType, write_to_error_log};
use crate::backend::manga_provider::{Chapter, ChapterToRead, FeedPageProvider, LatestChapter, MangaProviders};
use crate::backend::offline::{LocalChapterSource, OfflineManga, find_offline_manga, scan_offline_mangas};
use crate::backend::tui::Events;
use crate::i18n::tr;
use crate::theme::{ERROR_STYLE, theme};
use crate::utils::render_search_bar;
use crate::view::app::MangaToRead;
use crate::view::widgets::Component;
use crate::view::widgets::feed::{
    AskConfirmationDeleteAllModal, FeedBottomAction, FeedTabs, HISTORY_ITEM_HEIGHT, HistoryWidget, MangasRead, OFFLINE_ITEM_HEIGHT,
    OfflineDownloadsWidget, OfflineMangaItem, TIMELINE_ITEM_HEIGHT, TimelineChapterItem, TimelineWidget,
    build_bottom_instructions_with_hitboxes,
};

/// Represents the current state of the feed page UI.
///
/// The feed uses a state machine to manage different UI modes and loading states.
/// This ensures proper user feedback and prevents invalid interactions during async operations.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FeedState {
    /// Currently searching/filtering the reading history
    SearchingHistory,
    /// An error occurred while searching history
    ErrorSearchingHistory,
    /// No manga found in the current history tab
    HistoryNotFound,
    /// Loading detailed manga page data
    SearchingMangaPage,
    /// Failed to load manga page data
    MangaPageNotFound,
    /// Normal display mode showing manga history
    DisplayingHistory,
    /// Showing confirmation dialog for bulk deletion
    AskingDeleteAllConfirmation,
}

/// User actions that can be triggered by keyboard or mouse input.
///
/// These actions are sent through the local action channel and processed
/// by the `update()` method to modify the feed state.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FeedActions {
    /// Scroll up in the manga or timeline list (k key or mouse wheel up)
    ScrollHistoryUp,
    /// Scroll down in the manga or timeline list (j key or mouse wheel down)
    ScrollHistoryDown,
    /// Toggle search bar focus (s key)
    ToggleSearchBar,
    /// Go to next page of results (w key)
    NextPage,
    /// Go to previous page of results (b key)
    PreviousPage,
    /// Switch between tabs (Tab key)
    SwitchTab,
    /// Set a specific tab directly (1, 2, 3 keys)
    SetTab(FeedTabs),
    /// Navigate to detailed manga page (r or Enter key)
    GoToMangaPage,
    /// Toggle favorite on the selected manga (f key)
    ToggleFavorite,
    /// Cycle the custom shelf/category assigned to the selected manga (e key)
    CycleMangaShelf,
    /// Cycle the active shelf filter in the Library tab (Shift+E key)
    CycleShelfFilter,
    /// Refresh timeline feed (Shift+R key)
    RefreshTimeline,
    /// Open selected downloaded manga directly in the local Web Viewer HD (v key)
    OpenOfflineWebViewer,
}

/// Internal events for managing async operations and state changes.
///
/// These events are sent through the local event channel and processed
/// by the `tick()` method to handle async results and state transitions.
#[derive(Debug, PartialEq)]
pub enum FeedEvents {
    /// Trigger a search of the reading history
    SearchHistory,
    /// Start loading latest chapters for all displayed manga
    SearchRecentChapters,
    /// Load latest chapters for a specific manga
    LoadLatestChapters(String, Option<Vec<LatestChapter>>),
    /// Handle error when searching manga data
    ErrorSearchingMangaData,
    /// Load history results from database
    LoadHistory(Option<MangaHistoryResponse>),
    /// Trigger a search for timeline updates
    SearchTimeline,
    /// Load timeline items
    LoadTimeline(Vec<TimelineChapterItem>),
    /// Trigger scanning downloaded chapters on disk
    SearchOfflineDownloads,
    /// Load offline downloaded mangas scanned from disk
    LoadOfflineDownloads(Vec<OfflineManga>),
}

/// Main feed page component for displaying and managing manga collections.
///
/// The `Feed` struct manages the complete feed page interface, including:
/// - 3 tabs: Timeline (Novedades), Continuar leyendo (Historial), Biblioteca (Favoritos & Plan to Read)
/// - UI state management and rendering
/// - User input handling (keyboard and mouse)
/// - Async data loading and caching
/// - Navigation between different views
/// - Search and filtering functionality
pub struct Feed<T>
where
    T: FeedPageProvider,
{
    /// Current active tab (Timeline, ContinueReading, or Library)
    pub tabs: FeedTabs,
    /// Current UI state
    state: FeedState,
    /// Widget for displaying manga history or library
    pub history: Option<HistoryWidget>,
    /// Widget for displaying chronological updates feed
    pub timeline: Option<TimelineWidget>,
    /// Widget for displaying locally downloaded mangas for 100% offline reading
    pub offline_downloads: Option<OfflineDownloadsWidget>,
    /// Loading animation state
    pub loading_state: Option<ThrobberState>,
    /// Channel for sending events to the main application
    pub global_event_tx: Option<UnboundedSender<Events>>,
    /// Channel for sending user actions
    pub local_action_tx: UnboundedSender<FeedActions>,
    /// Channel for receiving user actions
    pub local_action_rx: UnboundedReceiver<FeedActions>,
    /// Channel for sending internal events
    pub local_event_tx: UnboundedSender<FeedEvents>,
    /// Channel for receiving internal events
    pub local_event_rx: UnboundedReceiver<FeedEvents>,
    /// Search input field
    search_bar: Input,
    /// Whether the search bar is currently focused
    is_typing: bool,
    /// Number of manga to display per page
    items_per_page: u32,
    /// Background tasks for async operations
    tasks: JoinSet<()>,
    /// Manga provider for fetching data
    manga_provider: Option<Arc<T>>,
    /// Active shelf filter in Library tab (`None` = show all shelves)
    pub active_shelf_filter: Option<String>,
    tabs_area: Rect,
    tab_hit_boxes: Vec<(Rect, FeedTabs)>,
    search_bar_area: Rect,
    refresh_chip_rect: Option<Rect>,
    content_area: Rect,
    list_inner_area: Rect,
    pagination_prev_rect: Option<Rect>,
    pagination_next_rect: Option<Rect>,
    bottom_action_hit_boxes: Vec<(Rect, FeedBottomAction)>,
    delete_modal_rects: Option<(Rect, Rect, Rect)>,
}

impl<T> Feed<T>
where
    T: FeedPageProvider,
{
    /// Creates a new feed page with default settings.
    pub fn new() -> Self {
        let (local_action_tx, local_action_rx) = mpsc::unbounded_channel::<FeedActions>();
        let (local_event_tx, local_event_rx) = mpsc::unbounded_channel::<FeedEvents>();
        Self {
            tabs: FeedTabs::Timeline,
            loading_state: None,
            history: None,
            timeline: None,
            offline_downloads: None,
            state: FeedState::DisplayingHistory,
            global_event_tx: None,
            local_action_tx,
            local_action_rx,
            local_event_tx,
            local_event_rx,
            tasks: JoinSet::new(),
            search_bar: Input::default(),
            items_per_page: 5,
            is_typing: false,
            manga_provider: None,
            active_shelf_filter: None,
            tabs_area: Rect::default(),
            tab_hit_boxes: Vec::new(),
            search_bar_area: Rect::default(),
            refresh_chip_rect: None,
            content_area: Rect::default(),
            list_inner_area: Rect::default(),
            pagination_prev_rect: None,
            pagination_next_rect: None,
            bottom_action_hit_boxes: Vec::new(),
            delete_modal_rects: None,
        }
    }

    /// Returns whether the search bar is currently focused for text input.
    #[inline]
    pub fn is_typing(&self) -> bool {
        self.is_typing
    }

    /// Returns `true` when the bulk-deletion confirmation modal is displayed.
    #[inline]
    pub fn is_overlay_open(&self) -> bool {
        self.state == FeedState::AskingDeleteAllConfirmation
    }

    /// Sets the global event sender for communicating with the main application.
    pub fn with_global_sender(mut self, sender: UnboundedSender<Events>) -> Self {
        self.global_event_tx = Some(sender);
        self
    }

    /// Sets the manga provider for fetching data.
    pub fn with_api_client(mut self, api_client: Arc<T>) -> Self {
        self.manga_provider = Some(api_client);
        self
    }

    fn render_empty_history_card(&mut self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let tab_title = if self.tabs == FeedTabs::Library {
            tr(" 📚 Biblioteca ", " 📚 Library ")
        } else {
            tr(" 📖 Continuar leyendo ", " 📖 Continue Reading ")
        };
        let mut top_left_spans =
            vec![Span::styled(tab_title, Style::default().fg(t.accent_primary).bg(t.title_chip_primary_bg).bold())];
        if self.tabs == FeedTabs::Library {
            let shelf_text = match &self.active_shelf_filter {
                Some(s) => format!(" 🏷 {s} <E> "),
                None => format!(" 🏷 {} <E> ", tr("Todas", "All Shelves")),
            };
            top_left_spans.push(Span::styled(shelf_text, Style::default().fg(Color::Black).bg(t.accent_secondary).bold()));
        }
        let msg = if self.tabs == FeedTabs::Library {
            if let Some(shelf) = self.active_shelf_filter.as_deref() {
                tr(
                    "La estantería «{shelf}» está vacía.\nConsejo: Pulsa <E> para cambiar de estantería o volver a «Todas», o asigna mangas con <e>.",
                    "The \"{shelf}\" shelf is empty.\nTip: Press <E> to switch shelves or return to \"All Shelves\", or assign mangas with <e>.",
                )
                .replace("{shelf}", shelf)
            } else {
                tr(
                    "Tu biblioteca está vacía.\nConsejo: Marca cualquier manga como favorito con <f> o guárdalo como Pendiente (<p> en Buscar).",
                    "Your library is empty.\nTip: Mark any manga as favorite with <f> or save it as Plan to Read (<p> in Search).",
                )
                .to_string()
            }
        } else {
            tr(
                "No tienes historial de lectura todavía.\nConsejo: Comienza a leer cualquier manga para ver aquí tu progreso.",
                "You have no reading history yet.\nTip: Start reading any manga to track your progress here.",
            )
            .to_string()
        };
        let mut instructions = vec![(
            t.key_chip("Tab / 1-4"),
            Span::styled(tr(" Cambiar pestaña  ", " Switch tab  "), Style::default().fg(t.text_muted)),
            Some(FeedBottomAction::CycleTab),
        )];
        if self.tabs == FeedTabs::Library {
            instructions.push((
                t.key_chip("E"),
                Span::styled(tr(" Cambiar estantería  ", " Switch shelf  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::CycleShelfFilter),
            ));
        }
        instructions.push((
            t.key_chip("i"),
            Span::styled(tr(" Ir a Buscar ", " Go to Search "), Style::default().fg(t.text_muted)),
            Some(FeedBottomAction::GoToSearch),
        ));
        let (bottom_line, hit_boxes) = build_bottom_instructions_with_hitboxes(area, instructions);
        self.list_inner_area = Rect::default();
        self.pagination_prev_rect = None;
        self.pagination_next_rect = None;
        self.bottom_action_hit_boxes = hit_boxes;

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(Line::from(top_left_spans))
            .title_bottom(bottom_line);
        let inner = block.inner(area).inner(Margin {
            horizontal: 2,
            vertical: 2,
        });
        block.render(area, buf);
        Paragraph::new(msg)
            .style(Style::default().fg(t.text_muted))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }

    fn render_history(&mut self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        if self.state == FeedState::ErrorSearchingHistory {
            self.list_inner_area = Rect::default();
            self.pagination_prev_rect = None;
            self.pagination_next_rect = None;
            self.bottom_action_hit_boxes.clear();

            let block = Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.status_error))
                .title(Span::styled(tr(" ⚠ Error al cargar el historial ", " ⚠ Failed to load history "), *ERROR_STYLE));
            let inner = block.inner(area).inner(Margin {
                horizontal: 2,
                vertical: 1,
            });
            block.render(area, buf);
            Paragraph::new(
                tr(
                    "No se pudo cargar el historial debido a un error, revisa los registros de error.",
                    "Could not load history due to an error, please check the error logs.",
                )
                .to_span()
                .style(Style::default().fg(t.status_error)),
            )
            .wrap(Wrap { trim: true })
            .render(inner, buf);
            return;
        }
        let active_tab = self.tabs;
        let is_not_found = self.state == FeedState::HistoryNotFound;
        if let Some(history) = self.history.as_mut() {
            if is_not_found {
                self.render_empty_history_card(area, buf);
            } else {
                history.active_tab = active_tab;
                history.active_shelf_filter = self.active_shelf_filter.clone();
                let widget_snapshot = history.clone();
                let (prev_rect, next_rect) =
                    HistoryWidget::pagination_hit_boxes(area, widget_snapshot.page, widget_snapshot.total_results);
                StatefulWidget::render(widget_snapshot, area, buf, &mut history.state);
                self.list_inner_area = HistoryWidget::list_inner_area(area);
                self.pagination_prev_rect = prev_rect;
                self.pagination_next_rect = next_rect;
                self.bottom_action_hit_boxes = HistoryWidget::bottom_action_hit_boxes(area);
            }
        } else {
            self.render_empty_history_card(area, buf);
        }
    }

    fn render_timeline(&mut self, area: Rect, buf: &mut Buffer) {
        self.list_inner_area = TimelineWidget::list_inner_area(area);
        self.pagination_prev_rect = None;
        self.pagination_next_rect = None;
        self.bottom_action_hit_boxes = TimelineWidget::bottom_action_hit_boxes(area);

        if let Some(timeline) = self.timeline.as_mut() {
            StatefulWidget::render(timeline.clone(), area, buf, &mut timeline.state);
        } else {
            let mut empty_widget = TimelineWidget {
                items: Vec::new(),
                state: tui_widget_list::ListState::default(),
                is_loading: self.state == FeedState::SearchingHistory,
            };
            StatefulWidget::render(empty_widget.clone(), area, buf, &mut empty_widget.state);
        }
    }

    fn render_offline_downloads(&mut self, area: Rect, buf: &mut Buffer) {
        self.list_inner_area = OfflineDownloadsWidget::list_inner_area(area);
        self.pagination_prev_rect = None;
        self.pagination_next_rect = None;
        self.bottom_action_hit_boxes = OfflineDownloadsWidget::bottom_action_hit_boxes(area);

        if let Some(offline) = self.offline_downloads.as_mut() {
            StatefulWidget::render(offline.clone(), area, buf, &mut offline.state);
        } else {
            let mut empty_widget = OfflineDownloadsWidget {
                is_loading: self.state == FeedState::SearchingHistory,
                ..Default::default()
            };
            StatefulWidget::render(empty_widget.clone(), area, buf, &mut empty_widget.state);
        }
    }

    fn render_tabs_and_search_bar(&mut self, area: Rect, frame: &mut Frame) {
        let t = theme();
        let [tabs_area, search_bar_area] = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(area);
        self.tabs_area = tabs_area;
        self.search_bar_area = search_bar_area;
        self.tab_hit_boxes.clear();

        let tabs_title = Line::from(vec![
            Span::styled(tr(" 📚 Seguimiento ", " 📚 Feed "), Style::default().fg(t.accent_primary).bold()),
            Span::styled("<Tab | 1-4> ", Style::default().fg(t.accent_highlight).bold()),
            Span::styled(tr("Pestaña ", "Tab "), Style::default().fg(t.text_muted)),
        ]);

        let mut tabs_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(tabs_title);

        if self.state == FeedState::MangaPageNotFound {
            tabs_block = tabs_block.title_top(
                Line::from(vec![Span::styled(
                    tr(" ⚠ Error al abrir manga ", " ⚠ Failed to open manga "),
                    Style::default().fg(t.status_error).bold(),
                )])
                .right_aligned(),
            );
        }

        let inner_tabs = tabs_block.inner(tabs_area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        tabs_block.render(tabs_area, frame.buffer_mut());

        if inner_tabs.width > 0 && inner_tabs.height > 0 {
            let [tab_row, status_row] = if inner_tabs.height >= 2 {
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner_tabs)
            } else {
                [inner_tabs, Rect::default()]
            };

            let full_defs = [
                (FeedTabs::Timeline, tr(" ⚡ Novedades ", " ⚡ Updates ")),
                (FeedTabs::ContinueReading, tr(" 📖 Continuar ", " 📖 Continue ")),
                (FeedTabs::Library, tr(" 📚 Biblioteca ", " 📚 Library ")),
                (FeedTabs::Downloads, tr(" 💾 Descargas ", " 💾 Downloads ")),
            ];
            let compact_defs = [
                (FeedTabs::Timeline, tr(" 1:Noved. ", " 1:Upd. ")),
                (FeedTabs::ContinueReading, tr(" 2:Cont. ", " 2:Cont. ")),
                (FeedTabs::Library, tr(" 3:Bibl. ", " 3:Lib. ")),
                (FeedTabs::Downloads, tr(" 4:Desc. ", " 4:Down. ")),
            ];
            let full_total_w: usize = full_defs.iter().map(|(_, lbl)| Span::raw(*lbl).width()).sum::<usize>() + 3;
            let tab_defs = if full_total_w <= tab_row.width as usize { full_defs } else { compact_defs };

            let mut cursor_x = tab_row.x;
            let max_x = tab_row.right();
            let mut spans: Vec<Span<'_>> = Vec::with_capacity(7);
            for (idx, (tab, label)) in tab_defs.into_iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::styled("│", Style::default().fg(t.border_muted)));
                    cursor_x = cursor_x.saturating_add(1);
                }
                let is_active = self.tabs == tab;
                let style = if is_active {
                    Style::default().fg(Color::Black).bg(t.accent_primary).bold()
                } else {
                    Style::default().fg(t.text_muted)
                };
                let span = Span::styled(label, style);
                let span_w = span.width() as u16;
                let avail = max_x.saturating_sub(cursor_x);
                let hit_w = span_w.min(avail);
                if hit_w > 0 {
                    self.tab_hit_boxes.push((Rect::new(cursor_x, tab_row.y, hit_w, 1), tab));
                }
                cursor_x = cursor_x.saturating_add(span_w);
                spans.push(span);
            }
            Paragraph::new(Line::from(spans)).render(tab_row, frame.buffer_mut());

            if status_row.height > 0 {
                self.render_searching_status(status_row, frame.buffer_mut());
            }
        }

        let input_help = if self.is_typing {
            self.refresh_chip_rect = None;
            Line::from(vec![
                Span::styled(tr(" ⌨ Filtrando ", " ⌨ Filtering "), Style::default().fg(Color::Black).bg(t.accent_highlight).bold()),
                Span::raw(" "),
                Span::styled("<Enter> ", Style::default().fg(t.accent_highlight).bold()),
                Span::styled(tr("Buscar ", "Search "), Style::default().fg(t.text_muted)),
                Span::styled("<Esc> ", Style::default().fg(t.accent_highlight).bold()),
                Span::styled(tr("Salir ", "Stop "), Style::default().fg(t.text_muted)),
            ])
        } else {
            let filter_span = Span::styled(tr(" 🔍 Filtrar ", " 🔍 Filter "), Style::default().fg(t.accent_primary).bold());
            let s_chip = Span::styled("<s> ", Style::default().fg(t.accent_highlight).bold());
            let type_span = Span::styled(tr("Escribir ", "Type "), Style::default().fg(t.text_muted));
            let r_chip = Span::styled("<R> ", Style::default().fg(t.accent_highlight).bold());
            let refresh_span = Span::styled(tr("Actualizar ", "Refresh "), Style::default().fg(t.text_muted));

            let prefix_w = (filter_span.width() + s_chip.width() + type_span.width()) as u16;
            let refresh_w = (r_chip.width() + refresh_span.width()).saturating_sub(1) as u16;
            let rx = search_bar_area.x.saturating_add(1).saturating_add(prefix_w);
            let max_sx = search_bar_area.right().saturating_sub(1);
            self.refresh_chip_rect = (search_bar_area.width > 2 && rx < max_sx && refresh_w > 0)
                .then(|| Rect::new(rx, search_bar_area.y, refresh_w.min(max_sx.saturating_sub(rx)), 1));

            Line::from(vec![filter_span, s_chip, type_span, r_chip, refresh_span])
        };

        render_search_bar(self.is_typing, input_help, &self.search_bar, frame, search_bar_area);
    }

    fn render_searching_status(&mut self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        if let Some(state) = self.loading_state.as_mut() {
            let loader = Throbber::default()
                .label(tr("Cargando ficha del manga, por favor espera... ", "Loading manga page, please wait... "))
                .style(Style::default().fg(t.accent_highlight).bold())
                .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                .use_type(throbber_widgets_tui::WhichUse::Spin);

            StatefulWidget::render(loader, area, buf, state);
        } else if self.state == FeedState::MangaPageNotFound {
            Paragraph::new(
                tr(
                    "⚠ No se pudieron obtener los datos del manga, inténtalo de nuevo más tarde.",
                    "⚠ Could not fetch manga data, please try again later.",
                )
                .to_span()
                .style(Style::default().fg(t.status_error).bold()),
            )
            .render(area, buf);
        } else {
            let hint = match self.tabs {
                FeedTabs::Timeline => tr(
                    "💡 Capítulos nuevos de tus favoritos e historial reciente",
                    "💡 Latest chapters from your favorites and recent history",
                ),
                FeedTabs::ContinueReading => {
                    tr("💡 Progreso de lectura guardado automáticamente", "💡 Automatically saved reading progress")
                },
                FeedTabs::Library => tr(
                    "💡 Tus mangas favoritos (★) y pendientes por leer (📋)",
                    "💡 Your favorite (★) and plan-to-read (📋) mangas",
                ),
                FeedTabs::Downloads => tr(
                    "💡 Capítulos guardados en disco (100% sin conexión · CBZ, RAW, EPUB, PDF)",
                    "💡 Chapters saved on disk (100% offline · CBZ, RAW, EPUB, PDF)",
                ),
            };
            Paragraph::new(Line::from(Span::styled(hint, Style::default().fg(t.text_dim)))).render(area, buf);
        }
    }

    fn render_top_area(&mut self, area: Rect, frame: &mut Frame) {
        self.render_tabs_and_search_bar(area, frame);
    }

    fn render_ask_modal_confirmation_delete_all_mangas(&mut self, area: Rect, buf: &mut Buffer) {
        if self.state == FeedState::AskingDeleteAllConfirmation {
            let provider = self.manga_provider.as_ref().map(|p| p.name()).unwrap_or_default();
            self.delete_modal_rects = Some(AskConfirmationDeleteAllModal::hit_boxes(area));
            AskConfirmationDeleteAllModal::new(self.tabs, provider).render(area, buf);
        } else {
            self.delete_modal_rects = None;
        }
    }

    #[inline]
    fn get_currently_selected_manga(&self) -> Option<&MangasRead> {
        self.history.as_ref().and_then(|widg| widg.get_current_manga_selected())
    }

    /// Initiates a search of the feed based on the active tab.
    pub fn init_search(&mut self) {
        match self.tabs {
            FeedTabs::Timeline => {
                self.local_event_tx.send(FeedEvents::SearchTimeline).ok();
            },
            FeedTabs::Downloads => {
                self.local_event_tx.send(FeedEvents::SearchOfflineDownloads).ok();
            },
            _ => {
                self.local_event_tx.send(FeedEvents::SearchHistory).ok();
            },
        }
    }

    /// Removes the currently selected manga from history or library.
    fn remove_currently_selected_manga(&mut self) {
        if matches!(self.tabs, FeedTabs::Timeline | FeedTabs::Downloads) {
            return;
        }

        if let Some(manga) = self.get_currently_selected_manga() {
            let connection = match Database::get_connection() {
                Ok(c) => c,
                Err(e) => {
                    if let Some(tx) = &self.global_event_tx {
                        tx.send(Events::Error(e.to_string())).ok();
                    }
                    return;
                },
            };
            let database = Database::new(&connection);

            if self.tabs == FeedTabs::Library {
                if manga.is_favorite {
                    let _ = database.toggle_favorite(&manga.id, manga.provider, &manga.title);
                }
                if manga.is_plan_to_read {
                    let _ = database.remove_from_history(&manga.id, MangaHistoryType::PlanToRead);
                }
                if manga.shelf_name.is_some() {
                    let _ = database.set_manga_shelf(&manga.id, manga.provider, &manga.title, None);
                }
                self.init_search();
            } else if let Err(err) = database.remove_from_history(&manga.id, MangaHistoryType::ReadingHistory) {
                if let Some(tx) = &self.global_event_tx {
                    tx.send(Events::Error(err.to_string())).ok();
                }
            } else {
                self.init_search();
            }
        }
    }

    /// Toggles the favorite status of the currently selected manga.
    fn toggle_favorite_selected_manga(&mut self) {
        let manga_info = match self.tabs {
            FeedTabs::Timeline => self
                .timeline
                .as_ref()
                .and_then(|t| t.get_current_selected())
                .map(|item| (item.manga_id.clone(), item.provider, item.manga_title.clone())),
            FeedTabs::Downloads => self
                .offline_downloads
                .as_ref()
                .and_then(|w| w.get_current_selected())
                .and_then(|m| m.provider.map(|p| (m.manga_id.clone(), p, m.title.clone()))),
            _ => self
                .history
                .as_ref()
                .and_then(|h| h.get_current_manga_selected())
                .map(|manga| (manga.id.clone(), manga.provider, manga.title.clone())),
        };

        if let Some((manga_id, provider, title)) = manga_info {
            if let Ok(conn) = Database::get_connection() {
                let db = Database::new(&conn);
                let _ = db.toggle_favorite(&manga_id, provider, &title);
            }
            self.init_search();
        }
    }

    /// Cycles the custom shelf/category assigned to the currently selected manga.
    fn cycle_selected_manga_shelf(&mut self) {
        let manga_info = match self.tabs {
            FeedTabs::Timeline => self
                .timeline
                .as_ref()
                .and_then(|t| t.get_current_selected())
                .map(|item| (item.manga_id.clone(), item.provider, item.manga_title.clone())),
            FeedTabs::Downloads => self
                .offline_downloads
                .as_ref()
                .and_then(|w| w.get_current_selected())
                .and_then(|m| m.provider.map(|p| (m.manga_id.clone(), p, m.title.clone()))),
            _ => self
                .history
                .as_ref()
                .and_then(|h| h.get_current_manga_selected())
                .map(|manga| (manga.id.clone(), manga.provider, manga.title.clone())),
        };

        if let Some((manga_id, provider, title)) = manga_info {
            if let Ok(conn) = Database::get_connection() {
                let db = Database::new(&conn);
                let _ = db.cycle_manga_shelf(&manga_id, provider, &title);
            }
            self.init_search();
        }
    }

    /// Cycles the active shelf filter in the Library tab (`None` -> each shelf -> `None`).
    fn cycle_shelf_filter(&mut self) {
        if self.tabs != FeedTabs::Library {
            self.set_tab(FeedTabs::Library);
        }
        let all_shelves: Vec<String> = Database::get_connection()
            .ok()
            .and_then(|conn| Database::new(&conn).list_all_shelves().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(s, _)| s)
            .collect();

        self.active_shelf_filter = match self.active_shelf_filter.as_deref() {
            None => all_shelves.first().cloned(),
            Some(curr) => {
                if let Some(pos) = all_shelves.iter().position(|s| s == curr) {
                    all_shelves.get(pos + 1).cloned()
                } else {
                    None
                }
            },
        };
        if let Some(h) = self.history.as_mut() {
            h.page = 1;
        }
        self.init_search();
    }

    /// Removes all manga from the current provider and history type.
    fn remove_all_mangas(&mut self) {
        if matches!(self.tabs, FeedTabs::Timeline | FeedTabs::Downloads) {
            self.state = FeedState::DisplayingHistory;
            return;
        }

        let connection = match Database::get_connection() {
            Ok(c) => c,
            Err(e) => {
                if let Some(tx) = &self.global_event_tx {
                    tx.send(Events::Error(e.to_string())).ok();
                }
                return;
            },
        };
        let database = Database::new(&connection);
        let hist_type = match self.tabs {
            FeedTabs::Library => MangaHistoryType::PlanToRead,
            FeedTabs::ContinueReading => MangaHistoryType::ReadingHistory,
            FeedTabs::Timeline | FeedTabs::Downloads => return,
        };
        let provider = self.manga_provider.as_ref().map(|p| p.name()).unwrap_or_default();

        if let Err(e) = database.remove_all_from_history(hist_type, provider) {
            if let Some(tx) = &self.global_event_tx {
                tx.send(Events::Error(e.to_string())).ok();
            }
        };
        self.state = FeedState::DisplayingHistory;
        self.init_search();
    }

    /// Handles keyboard input based on the current state and focus.
    fn handle_key_events(&mut self, key_event: KeyEvent) {
        if self.is_typing {
            match key_event.code {
                KeyCode::Enter => {
                    self.init_search();
                },
                KeyCode::Esc => {
                    self.local_action_tx.send(FeedActions::ToggleSearchBar).ok();
                },
                _ => {
                    self.search_bar.handle_event(&crossterm::event::Event::Key(key_event));
                },
            };
        } else if self.state == FeedState::AskingDeleteAllConfirmation {
            match key_event.code {
                KeyCode::Char('w') => self.remove_all_mangas(),
                KeyCode::Char('q') | KeyCode::Esc => self.state = FeedState::DisplayingHistory,
                _ => {},
            }
        } else {
            match key_event.code {
                KeyCode::Tab => {
                    self.local_action_tx.send(FeedActions::SwitchTab).ok();
                },
                KeyCode::Char('1') => {
                    self.local_action_tx.send(FeedActions::SetTab(FeedTabs::Timeline)).ok();
                },
                KeyCode::Char('2') => {
                    self.local_action_tx.send(FeedActions::SetTab(FeedTabs::ContinueReading)).ok();
                },
                KeyCode::Char('3') => {
                    self.local_action_tx.send(FeedActions::SetTab(FeedTabs::Library)).ok();
                },
                KeyCode::Char('4') => {
                    self.local_action_tx.send(FeedActions::SetTab(FeedTabs::Downloads)).ok();
                },
                KeyCode::Char('j') | KeyCode::Down => {
                    self.local_action_tx.send(FeedActions::ScrollHistoryDown).ok();
                },
                KeyCode::Char('k') | KeyCode::Up => {
                    self.local_action_tx.send(FeedActions::ScrollHistoryUp).ok();
                },
                KeyCode::Char('w') => {
                    self.local_action_tx.send(FeedActions::NextPage).ok();
                },
                KeyCode::Char('b') => {
                    self.local_action_tx.send(FeedActions::PreviousPage).ok();
                },
                KeyCode::Enter => {
                    if self.tabs == FeedTabs::Timeline {
                        self.read_timeline_chapter();
                    } else {
                        self.local_action_tx.send(FeedActions::GoToMangaPage).ok();
                    }
                },
                KeyCode::Char('r') | KeyCode::Char('m') => {
                    self.local_action_tx.send(FeedActions::GoToMangaPage).ok();
                },
                KeyCode::Char('v') if self.tabs == FeedTabs::Downloads => {
                    self.local_action_tx.send(FeedActions::OpenOfflineWebViewer).ok();
                },
                KeyCode::Char('s') => {
                    self.local_action_tx.send(FeedActions::ToggleSearchBar).ok();
                },
                KeyCode::Char('R') => {
                    self.local_action_tx.send(FeedActions::RefreshTimeline).ok();
                },
                KeyCode::Char('f') => {
                    self.local_action_tx.send(FeedActions::ToggleFavorite).ok();
                },
                KeyCode::Char('e') => {
                    self.local_action_tx.send(FeedActions::CycleMangaShelf).ok();
                },
                KeyCode::Char('E') => {
                    self.local_action_tx.send(FeedActions::CycleShelfFilter).ok();
                },
                KeyCode::Char('d') => {
                    if self.tabs == FeedTabs::Timeline {
                        self.download_timeline_chapter();
                    } else if self.tabs != FeedTabs::Downloads {
                        self.remove_currently_selected_manga();
                    }
                },
                KeyCode::Char('X') if !matches!(self.tabs, FeedTabs::Timeline | FeedTabs::Downloads) => {
                    self.state = FeedState::AskingDeleteAllConfirmation;
                },
                _ => {},
            }
        }
    }

    /// Processes async events and updates the feed state.
    pub fn tick(&mut self) {
        if let Some(loader_state) = self.loading_state.as_mut() {
            loader_state.calc_next();
        }
        if let Ok(local_event) = self.local_event_rx.try_recv() {
            match local_event {
                FeedEvents::ErrorSearchingMangaData => self.display_error_searching_manga(),
                FeedEvents::SearchHistory => self.search_history(),
                FeedEvents::LoadHistory(maybe_history) => self.load_history(maybe_history),
                FeedEvents::SearchRecentChapters => self.search_latest_chapters(),
                FeedEvents::LoadLatestChapters(manga_id, maybe_chapters) => {
                    self.load_recent_chapters(manga_id, maybe_chapters);
                },
                FeedEvents::SearchTimeline => self.search_timeline(),
                FeedEvents::LoadTimeline(items) => self.load_timeline(items),
                FeedEvents::SearchOfflineDownloads => self.search_offline_downloads(),
                FeedEvents::LoadOfflineDownloads(items) => self.load_offline_downloads(items),
            }
        }
    }

    fn load_recent_chapters(&mut self, manga_id: String, maybe_history: Option<Vec<LatestChapter>>) {
        if let Some(chapters_response) = maybe_history
            && let Some(history) = self.history.as_mut()
        {
            history.set_chapter(manga_id, chapters_response);
        }
    }

    /// Loads latest chapters for all displayed manga asynchronously.
    fn search_latest_chapters(&mut self) {
        if let Some(history) = self.history.as_mut() {
            let client = self.manga_provider.as_ref().cloned();
            for manga in history.mangas.clone() {
                let manga_id = manga.id;
                let provider = manga.provider;
                let sender = self.local_event_tx.clone();
                let client_clone = client.clone();
                self.tasks.spawn(async move {
                    let response = if let Some(c) = &client_clone {
                        if c.name() == provider || cfg!(test) {
                            c.get_latest_chapters(&manga_id).await
                        } else {
                            crate::backend::multi_provider::get_latest_chapters_for_provider(provider, &manga_id).await
                        }
                    } else {
                        crate::backend::multi_provider::get_latest_chapters_for_provider(provider, &manga_id).await
                    };
                    match response {
                        Ok(res) => {
                            sender.send(FeedEvents::LoadLatestChapters(manga_id, Some(res))).ok();
                        },
                        Err(e) => {
                            write_to_error_log(format!("{e}").into());
                            sender.send(FeedEvents::LoadLatestChapters(manga_id, None)).ok();
                        },
                    }
                });
            }
        }
    }

    /// Searches recent chapters from favorites and history to build the chronological timeline.
    fn search_timeline(&mut self) {
        self.state = FeedState::SearchingHistory;
        let tx = self.local_event_tx.clone();
        self.tasks.abort_all();
        let search_filter = self.search_bar.value().to_lowercase();

        self.tasks.spawn(async move {
            let (favorites, recent) = match Database::get_connection() {
                Ok(conn) => {
                    let db = Database::new(&conn);
                    let favs = db.get_all_favorite_mangas().unwrap_or_default();
                    let recs = db.get_recent_reading_mangas(8).unwrap_or_default();
                    (favs, recs)
                },
                Err(_) => (Vec::new(), Vec::new()),
            };

            let mut seen = std::collections::HashSet::new();
            let mut mangas_to_check = Vec::new();

            for m in favorites.into_iter().chain(recent) {
                if seen.insert((m.provider, m.manga_id.clone())) {
                    mangas_to_check.push(m);
                }
            }

            mangas_to_check.truncate(12);

            let mut all_chapters = Vec::new();

            for manga in mangas_to_check {
                if let Ok(chapters) =
                    crate::backend::multi_provider::get_latest_chapters_for_provider(manga.provider, &manga.manga_id).await
                {
                    let conn_res = Database::get_connection();
                    for chap in chapters {
                        let is_read = if let Ok(conn) = &conn_res {
                            Database::new(conn).check_chapter_is_already_reading(&chap.id).unwrap_or(false)
                        } else {
                            false
                        };

                        all_chapters.push(TimelineChapterItem {
                            manga_id: manga.manga_id.clone(),
                            manga_title: manga.title.clone(),
                            provider: manga.provider,
                            chapter_id: chap.id,
                            chapter_number: chap.chapter_number,
                            chapter_title: chap.title,
                            publication_date: chap.publication_date,
                            language: chap.language,
                            is_read,
                            style: Style::default(),
                        });
                    }
                }
            }

            if !search_filter.is_empty() {
                all_chapters.retain(|c| {
                    c.manga_title.to_lowercase().contains(&search_filter)
                        || c.chapter_title.to_lowercase().contains(&search_filter)
                        || c.chapter_number.contains(&search_filter)
                });
            }

            all_chapters.sort_by(|a, b| match (a.is_read, b.is_read) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => b.publication_date.cmp(&a.publication_date),
            });

            tx.send(FeedEvents::LoadTimeline(all_chapters)).ok();
        });
    }

    fn load_timeline(&mut self, items: Vec<TimelineChapterItem>) {
        let mut widget = TimelineWidget {
            items,
            state: tui_widget_list::ListState::default(),
            is_loading: false,
        };
        if !widget.items.is_empty() {
            widget.state.select(Some(0));
        }
        self.timeline = Some(widget);
        self.state = FeedState::DisplayingHistory;
    }

    fn search_offline_downloads(&mut self) {
        self.state = FeedState::SearchingHistory;
        let tx = self.local_event_tx.clone();
        self.tasks.abort_all();
        let search_filter = self.search_bar.value().trim().to_lowercase();

        self.tasks.spawn(async move {
            let mut mangas = tokio::task::spawn_blocking(scan_offline_mangas).await.unwrap_or_default();

            if !search_filter.is_empty() {
                mangas.retain(|m| {
                    m.title.to_lowercase().contains(&search_filter) || m.folder_name.to_lowercase().contains(&search_filter)
                });
            }

            tx.send(FeedEvents::LoadOfflineDownloads(mangas)).ok();
        });
    }

    fn load_offline_downloads(&mut self, mangas: Vec<OfflineManga>) {
        self.offline_downloads = Some(OfflineDownloadsWidget::from_offline_mangas(mangas));
        self.state = FeedState::DisplayingHistory;
    }

    fn display_error_searching_manga(&mut self) {
        self.loading_state = None;
        self.state = FeedState::MangaPageNotFound;
    }

    /// Initiates a search of the reading history or library from the database.
    fn search_history(&mut self) {
        if self.tabs == FeedTabs::Timeline {
            self.search_timeline();
            return;
        }
        if self.tabs == FeedTabs::Downloads {
            self.search_offline_downloads();
            return;
        }

        self.state = FeedState::SearchingHistory;
        let tx = self.local_event_tx.clone();
        self.tasks.abort_all();
        let search_term = self.search_bar.value().to_string();

        let page = match &self.history {
            Some(history) => history.page,
            None => 1,
        };

        let items_per_page = self.items_per_page;
        let tab = self.tabs;
        let history_type: MangaHistoryType = self.tabs.into();
        let provider = match &self.manga_provider {
            Some(provider) => provider.name(),
            None => MangaProviders::default(),
        };
        let shelf_filter = self.active_shelf_filter.clone();

        self.tasks.spawn(async move {
            let connection = match Database::get_connection() {
                Ok(conn) => conn,
                Err(e) => {
                    write_to_error_log(ErrorType::Error(Box::new(e)));
                    tx.send(FeedEvents::LoadHistory(None)).ok();
                    return;
                },
            };
            let database = Database::new(&connection);

            let maybe_reading_history = if tab == FeedTabs::Library {
                database.get_library_with_shelf(
                    GetHistoryArgs {
                        hist_type: history_type,
                        page,
                        search: SearchTerm::trimmed_lowercased(&search_term),
                        items_per_page,
                        provider,
                    },
                    shelf_filter.as_deref(),
                )
            } else {
                database.get_continue_reading(GetHistoryArgs {
                    hist_type: history_type,
                    page,
                    search: SearchTerm::trimmed_lowercased(&search_term),
                    items_per_page,
                    provider,
                })
            };

            match maybe_reading_history {
                Ok(history) => {
                    tx.send(FeedEvents::LoadHistory(Some(history))).ok();
                },
                Err(e) => {
                    write_to_error_log(ErrorType::Error(Box::new(e)));
                    tx.send(FeedEvents::LoadHistory(None)).ok();
                },
            }
        });
    }

    /// Navigates to the next page of results if available.
    fn search_next_page(&mut self) {
        if let Some(history) = self.history.as_mut()
            && history.can_search_next_page(self.items_per_page as f64)
        {
            history.next_page();
            self.search_history();
        }
    }

    /// Navigates to the previous page of results if available.
    fn search_previous_page(&mut self) {
        if let Some(history) = self.history.as_mut()
            && history.can_search_previous_page()
        {
            history.previous_page();
            self.search_history();
        }
    }

    /// Loads and displays the search results from the database.
    fn load_history(&mut self, maybe_history: Option<MangaHistoryResponse>) {
        match maybe_history.filter(|history| !history.mangas.is_empty()) {
            Some(history) => {
                self.history = Some(HistoryWidget::from_database_response(history));
                self.state = FeedState::DisplayingHistory;
                self.local_event_tx.send(FeedEvents::SearchRecentChapters).ok();
            },
            None => {
                self.state = FeedState::HistoryNotFound;
                self.history = None;
            },
        }
    }

    fn select_next_manga(&mut self) {
        if let Some(mangas) = self.history.as_mut() {
            mangas.select_next();
        }
    }

    fn select_previous_manga(&mut self) {
        if let Some(mangas) = self.history.as_mut() {
            mangas.select_previous();
        }
    }

    /// Navigates to the detailed manga page for the currently selected item.
    pub fn go_to_manga_page(&mut self) {
        if self.tabs == FeedTabs::Downloads {
            if let Some(offline_manga) = self.offline_downloads.as_ref().and_then(|w| w.get_current_selected()).cloned()
                && let Some(tx) = &self.global_event_tx
            {
                tx.send(Events::GoToOfflineMangaPage(offline_manga)).ok();
            }
            self.state = FeedState::DisplayingHistory;
            return;
        }

        self.state = FeedState::SearchingMangaPage;
        let manga_and_provider = if self.tabs == FeedTabs::Timeline && self.timeline.is_some() {
            self.timeline
                .as_ref()
                .and_then(|t| t.get_current_selected())
                .map(|item| (item.manga_id.clone(), item.provider, item.manga_title.clone()))
        } else {
            self.history
                .as_ref()
                .and_then(|h| h.get_current_manga_selected())
                .map(|manga| (manga.id.clone(), manga.provider, manga.title.clone()))
        };

        if let Some((manga_id, provider, manga_title)) = manga_and_provider {
            let tx = self.global_event_tx.as_ref().cloned().unwrap();
            let local_tx = self.local_event_tx.clone();

            self.loading_state = Some(ThrobberState::default());

            let client = self.manga_provider.as_ref().cloned();

            self.tasks.spawn(async move {
                let response = if let Some(c) = &client {
                    if c.name() == provider || cfg!(test) {
                        c.get_manga_by_id(&manga_id).await
                    } else {
                        crate::backend::multi_provider::get_manga_by_id_for_provider(provider, &manga_id)
                            .await
                            .map_err(|e| e as Box<dyn std::error::Error>)
                    }
                } else {
                    crate::backend::multi_provider::get_manga_by_id_for_provider(provider, &manga_id)
                        .await
                        .map_err(|e| e as Box<dyn std::error::Error>)
                }
                .map_err(|e| e.to_string());
                match response {
                    Ok(res) => {
                        tx.send(Events::GoToMangaPage(res)).ok();
                    },
                    Err(err_msg) => {
                        let offline_fallback =
                            tokio::task::spawn_blocking(move || find_offline_manga(&manga_id, &manga_title, Some(provider)))
                                .await
                                .ok()
                                .flatten();

                        if let Some(offline_manga) = offline_fallback {
                            tx.send(Events::GoToOfflineMangaPage(offline_manga)).ok();
                        } else {
                            write_to_error_log(err_msg.into());
                            local_tx.send(FeedEvents::ErrorSearchingMangaData).ok();
                        }
                    },
                }
            });
        } else {
            self.state = FeedState::DisplayingHistory;
        }
    }

    /// Opens the chapter directly in the reader from the timeline.
    fn read_timeline_chapter(&mut self) {
        if let Some(timeline) = &self.timeline {
            if let Some(item) = timeline.get_current_selected() {
                let manga_id = item.manga_id.clone();
                let chapter_id = item.chapter_id.clone();
                let chapter_title = item.chapter_title.clone();
                let chapter_number = item.chapter_number.parse::<f64>().unwrap_or(0.0);
                let chapter_lang = item.language;
                let manga_title = item.manga_title.clone();
                let provider = item.provider;

                let tx = match self.global_event_tx.as_ref().cloned() {
                    Some(tx) => tx,
                    None => return,
                };
                let local_tx = self.local_event_tx.clone();
                self.loading_state = Some(ThrobberState::default());

                self.tasks.spawn(async move {
                    let response = crate::backend::multi_provider::read_chapter_for_provider(provider, &chapter_id, &manga_id)
                        .await
                        .map_err(|e| e.to_string());
                    match response {
                        Ok((chapter_to_read, list)) => {
                            tx.send(Events::ReadChapter(chapter_to_read, MangaToRead {
                                title: manga_title,
                                manga_id,
                                list,
                                provider: Some(provider),
                                ..Default::default()
                            }))
                            .ok();
                        },
                        Err(err_msg) => {
                            let title_clone = manga_title.clone();
                            let ch_id_clone = chapter_id.clone();
                            let local_src = tokio::task::spawn_blocking(move || {
                                LocalChapterSource::find(&title_clone, &ch_id_clone)
                                    .map(|src| {
                                        let count = src.page_count();
                                        (src, count)
                                    })
                                    .filter(|(_, count)| *count > 0)
                            })
                            .await
                            .ok()
                            .flatten();

                            if let Some((source, count)) = local_src {
                                let dummy_urls = (0..count)
                                    .filter_map(|i| reqwest::Url::parse(&format!("http://localhost/local/{i}")).ok())
                                    .collect();
                                let chapter_to_read = ChapterToRead {
                                    id: chapter_id.clone(),
                                    title: chapter_title,
                                    number: chapter_number,
                                    volume_number: Some("none".to_string()),
                                    num_page_bookmarked: None,
                                    language: chapter_lang,
                                    pages_url: dummy_urls,
                                };
                                let mut offline_sources = std::collections::HashMap::new();
                                offline_sources.insert(chapter_id, source);
                                tx.send(Events::ReadChapter(chapter_to_read, MangaToRead {
                                    title: manga_title,
                                    manga_id,
                                    list: Default::default(),
                                    provider: Some(provider),
                                    offline_sources,
                                    is_offline_mode: true,
                                }))
                                .ok();
                                return;
                            }

                            write_to_error_log(err_msg.into());
                            local_tx.send(FeedEvents::ErrorSearchingMangaData).ok();
                        },
                    }
                });
            }
        }
    }

    fn open_offline_web_viewer(&mut self) {
        if self.tabs != FeedTabs::Downloads {
            return;
        }
        let Some(offline_manga) = self.offline_downloads.as_ref().and_then(|w| w.get_current_selected()).cloned() else {
            return;
        };
        let Some(target_chapter_id) = offline_manga
            .chapters
            .iter()
            .rev()
            .find(|c| !c.is_read)
            .or_else(|| offline_manga.chapters.last())
            .map(|c| c.chapter.id.clone())
        else {
            return;
        };

        self.tasks.spawn(async move {
            let session_opt = tokio::task::spawn_blocking(move || {
                let chapter = offline_manga.chapter_to_read(&target_chapter_id)?;
                let offline_sources = offline_manga.sources_map();
                let local_source = offline_sources.get(&target_chapter_id).cloned();
                let list_of_chapters = offline_manga.build_list_of_chapters();
                Some(crate::backend::web_viewer::WebViewerSession {
                    manga_id: offline_manga.manga_id,
                    manga_title: offline_manga.title,
                    manga_cover_url: None,
                    provider: offline_manga.provider.unwrap_or_default(),
                    chapter,
                    list_of_chapters,
                    local_source,
                    offline_sources,
                    is_offline_mode: true,
                })
            })
            .await
            .ok()
            .flatten();

            if let Some(session) = session_opt
                && let Err(e) = crate::backend::web_viewer::open_chapter_in_browser(session).await
            {
                write_to_error_log(ErrorType::String(&e));
            }
        });
    }

    /// Enqueues the selected timeline chapter for download.
    fn download_timeline_chapter(&mut self) {
        if let Some(timeline) = &self.timeline {
            if let Some(item) = timeline.get_current_selected() {
                let tx = match self.global_event_tx.as_ref().cloned() {
                    Some(tx) => tx,
                    None => return,
                };
                let chapter = Chapter {
                    id: item.chapter_id.clone(),
                    id_safe_for_download: item.chapter_id.replace(['/', ':', '\\', '?', '*', '"', '<', '>', '|'], "_"),
                    manga_id: item.manga_id.clone(),
                    title: item.chapter_title.clone(),
                    language: item.language,
                    chapter_number: item.chapter_number.clone(),
                    volume_number: None,
                    scanlator: None,
                    publication_date: item.publication_date,
                };
                let batch = crate::backend::download_queue::EnqueueDownloadBatch {
                    manga_id: item.manga_id.clone(),
                    manga_id_safe_for_download: item.manga_id.replace(['/', ':', '\\', '?', '*', '"', '<', '>', '|'], "_"),
                    manga_title: item.manga_title.clone(),
                    manga_cover_url: None,
                    lang: item.language,
                    chapters: vec![chapter],
                    provider: item.provider,
                };
                tx.send(Events::EnqueueDownloadBatch(batch)).ok();
            }
        }
    }

    fn toggle_focus_search_bar(&mut self) {
        self.is_typing = !self.is_typing;
    }

    /// Switches between tabs in sequence.
    fn switch_tabs(&mut self) {
        self.tabs = self.tabs.cycle();
        self.clean_up();
        self.state = FeedState::SearchingHistory;
        self.init_search();
    }

    /// Sets the active tab directly.
    fn set_tab(&mut self, tab: FeedTabs) {
        if self.tabs != tab {
            self.tabs = tab;
            self.clean_up();
            self.state = FeedState::SearchingHistory;
            self.init_search();
        }
    }

    fn effective_list_inner_area(&self) -> Rect {
        if self.list_inner_area.width > 0 && self.list_inner_area.height > 0 {
            return self.list_inner_area;
        }
        match self.tabs {
            FeedTabs::Timeline => TimelineWidget::list_inner_area(self.content_area),
            FeedTabs::Downloads => OfflineDownloadsWidget::list_inner_area(self.content_area),
            _ => HistoryWidget::list_inner_area(self.content_area),
        }
    }

    fn compute_clicked_card_target(
        inner: Rect,
        pos: Position,
        scroll_offset: usize,
        scroll_truncation: u16,
        card_h: u16,
        len: usize,
    ) -> Option<(usize, usize)> {
        if !inner.contains(pos) || card_h == 0 || len == 0 {
            return None;
        }
        let trunc = if inner.height < card_h { 0 } else { scroll_truncation as usize };
        let effective_y = (pos.y - inner.y) as usize + trunc;
        let h = card_h as usize;
        let target = scroll_offset + effective_y / h;
        let local_y = effective_y % h;
        (target < len).then_some((target, local_y))
    }

    fn select_card_at_pos(&mut self, pos: Position) -> Option<(usize, usize)> {
        let inner = self.effective_list_inner_area();
        match self.tabs {
            FeedTabs::Timeline => {
                let timeline = self.timeline.as_mut()?;
                let hit = Self::compute_clicked_card_target(
                    inner,
                    pos,
                    timeline.state.scroll_offset_index(),
                    timeline.state.scroll_truncation(),
                    TIMELINE_ITEM_HEIGHT,
                    timeline.items.len(),
                )?;
                timeline.state.select(Some(hit.0));
                Some(hit)
            },
            FeedTabs::Downloads => {
                let offline = self.offline_downloads.as_mut()?;
                let hit = Self::compute_clicked_card_target(
                    inner,
                    pos,
                    offline.state.scroll_offset_index(),
                    offline.state.scroll_truncation(),
                    OFFLINE_ITEM_HEIGHT,
                    offline.items.len(),
                )?;
                offline.state.select(Some(hit.0));
                Some(hit)
            },
            FeedTabs::ContinueReading | FeedTabs::Library => {
                let history = self.history.as_mut()?;
                let hit = Self::compute_clicked_card_target(
                    inner,
                    pos,
                    history.state.scroll_offset_index(),
                    history.state.scroll_truncation(),
                    HISTORY_ITEM_HEIGHT,
                    history.mangas.len(),
                )?;
                history.state.select(Some(hit.0));
                Some(hit)
            },
        }
    }

    fn dispatch_bottom_action(&mut self, action: FeedBottomAction) {
        match action {
            FeedBottomAction::OpenManga => {
                self.local_action_tx.send(FeedActions::GoToMangaPage).ok();
            },
            FeedBottomAction::ReadTimelineChapter => {
                if self.loading_state.is_none() {
                    self.read_timeline_chapter();
                }
            },
            FeedBottomAction::OpenOfflineWebViewer => {
                self.local_action_tx.send(FeedActions::OpenOfflineWebViewer).ok();
            },
            FeedBottomAction::ToggleFavorite => {
                self.local_action_tx.send(FeedActions::ToggleFavorite).ok();
            },
            FeedBottomAction::CycleMangaShelf => {
                self.local_action_tx.send(FeedActions::CycleMangaShelf).ok();
            },
            FeedBottomAction::CycleShelfFilter => {
                self.local_action_tx.send(FeedActions::CycleShelfFilter).ok();
            },
            FeedBottomAction::DownloadTimelineChapter => {
                self.download_timeline_chapter();
            },
            FeedBottomAction::RemoveSelected => {
                self.remove_currently_selected_manga();
            },
            FeedBottomAction::AskDeleteAll => {
                if !matches!(self.tabs, FeedTabs::Timeline | FeedTabs::Downloads) {
                    self.state = FeedState::AskingDeleteAllConfirmation;
                }
            },
            FeedBottomAction::ToggleSearch => {
                self.local_action_tx.send(FeedActions::ToggleSearchBar).ok();
            },
            FeedBottomAction::Refresh => {
                self.local_action_tx.send(FeedActions::RefreshTimeline).ok();
            },
            FeedBottomAction::NextPage => {
                self.local_action_tx.send(FeedActions::NextPage).ok();
            },
            FeedBottomAction::PreviousPage => {
                self.local_action_tx.send(FeedActions::PreviousPage).ok();
            },
            FeedBottomAction::CycleTab => {
                self.local_action_tx.send(FeedActions::SwitchTab).ok();
            },
            FeedBottomAction::GoToSearch => {
                if let Some(tx) = &self.global_event_tx {
                    tx.send(Events::GoSearchPage).ok();
                }
            },
        }
    }

    fn handle_mouse_event(&mut self, mouse_event: MouseEvent) {
        if self.state == FeedState::AskingDeleteAllConfirmation {
            if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
                let pos = Position::new(mouse_event.column, mouse_event.row);
                if let Some((modal_rect, confirm_rect, cancel_rect)) = self.delete_modal_rects {
                    if confirm_rect.contains(pos) {
                        self.remove_all_mangas();
                    } else if cancel_rect.contains(pos) || !modal_rect.contains(pos) {
                        self.state = FeedState::DisplayingHistory;
                    }
                }
            }
            return;
        }

        let pos = Position::new(mouse_event.column, mouse_event.row);
        match mouse_event.kind {
            MouseEventKind::ScrollUp if !self.is_typing => {
                self.local_action_tx.send(FeedActions::ScrollHistoryUp).ok();
            },
            MouseEventKind::ScrollDown if !self.is_typing => {
                self.local_action_tx.send(FeedActions::ScrollHistoryDown).ok();
            },
            MouseEventKind::Down(MouseButton::Right) if !self.is_typing => {
                self.select_card_at_pos(pos);
            },
            MouseEventKind::Down(MouseButton::Left) => {
                if !self.tab_hit_boxes.is_empty() {
                    if let Some(&(_, tab)) = self.tab_hit_boxes.iter().find(|&&(rect, _)| rect.contains(pos)) {
                        self.is_typing = false;
                        self.local_action_tx.send(FeedActions::SetTab(tab)).ok();
                        return;
                    }
                } else if self.tabs_area.contains(pos) {
                    self.is_typing = false;
                    let idx =
                        ((pos.x.saturating_sub(self.tabs_area.x) as usize) * 4 / (self.tabs_area.width.max(1) as usize)).min(3);
                    let tab = match idx {
                        0 => FeedTabs::Timeline,
                        1 => FeedTabs::ContinueReading,
                        2 => FeedTabs::Library,
                        _ => FeedTabs::Downloads,
                    };
                    self.local_action_tx.send(FeedActions::SetTab(tab)).ok();
                    return;
                }

                if !self.is_typing && self.refresh_chip_rect.is_some_and(|r| r.contains(pos)) {
                    self.local_action_tx.send(FeedActions::RefreshTimeline).ok();
                    return;
                }

                if self.search_bar_area.contains(pos) {
                    if !self.is_typing {
                        self.local_action_tx.send(FeedActions::ToggleSearchBar).ok();
                    }
                    return;
                }

                if self.is_typing {
                    self.is_typing = false;
                    return;
                }

                if matches!(self.tabs, FeedTabs::ContinueReading | FeedTabs::Library) {
                    if self.pagination_prev_rect.is_some_and(|r| r.contains(pos)) {
                        self.local_action_tx.send(FeedActions::PreviousPage).ok();
                        return;
                    }
                    if self.pagination_next_rect.is_some_and(|r| r.contains(pos)) {
                        self.local_action_tx.send(FeedActions::NextPage).ok();
                        return;
                    }
                }

                if let Some(&(_, action)) = self.bottom_action_hit_boxes.iter().find(|&&(rect, _)| rect.contains(pos)) {
                    self.dispatch_bottom_action(action);
                    return;
                }

                if let Some((_target, local_y)) = self.select_card_at_pos(pos) {
                    match self.tabs {
                        FeedTabs::Timeline => {
                            if self.loading_state.is_none() {
                                self.read_timeline_chapter();
                            }
                        },
                        FeedTabs::Downloads => {
                            let inner = self.effective_list_inner_area();
                            let has_provider = self
                                .offline_downloads
                                .as_ref()
                                .and_then(|w| w.get_current_selected())
                                .is_some_and(|m| m.provider.is_some());
                            if local_y == OfflineMangaItem::action_row_local_y() {
                                let ranges = OfflineMangaItem::inline_action_x_ranges(inner.x, inner.width, has_provider);
                                if ranges.web_viewer.is_some_and(|(s, e)| pos.x >= s && pos.x < e) {
                                    self.local_action_tx.send(FeedActions::OpenOfflineWebViewer).ok();
                                    return;
                                }
                                if ranges.favorite.is_some_and(|(s, e)| pos.x >= s && pos.x < e) {
                                    self.local_action_tx.send(FeedActions::ToggleFavorite).ok();
                                    return;
                                }
                            }
                            self.local_action_tx.send(FeedActions::GoToMangaPage).ok();
                        },
                        FeedTabs::ContinueReading | FeedTabs::Library => {
                            if local_y == MangasRead::action_row_local_y() {
                                let inner = self.effective_list_inner_area();
                                if MangasRead::inline_favorite_x_range(inner.x, inner.width)
                                    .is_some_and(|(s, e)| pos.x >= s && pos.x < e)
                                {
                                    self.local_action_tx.send(FeedActions::ToggleFavorite).ok();
                                    return;
                                }
                            }
                            self.local_action_tx.send(FeedActions::GoToMangaPage).ok();
                        },
                    }
                }
            },
            _ => {},
        }
    }

    #[cfg(test)]
    fn get_history(&self) -> HistoryWidget {
        self.history.as_ref().cloned().unwrap()
    }
}

impl<T> Component for Feed<T>
where
    T: FeedPageProvider,
{
    type Actions = FeedActions;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let top_bar_height = if area.height < 18 { 3 } else { 4 };
        let layout = Layout::vertical([Constraint::Length(top_bar_height), Constraint::Fill(1)]);

        let [tabs_area, content_area] = layout.areas(area);
        self.content_area = content_area;

        self.render_top_area(tabs_area, frame);

        if self.tabs == FeedTabs::Timeline {
            self.render_timeline(content_area, frame.buffer_mut());
        } else if self.tabs == FeedTabs::Downloads {
            self.render_offline_downloads(content_area, frame.buffer_mut());
        } else {
            self.render_history(content_area, frame.buffer_mut());
        }

        self.render_ask_modal_confirmation_delete_all_mangas(area, frame.buffer_mut());
    }

    fn update(&mut self, action: Self::Actions) {
        match action {
            FeedActions::ToggleSearchBar => self.toggle_focus_search_bar(),
            FeedActions::NextPage => self.search_next_page(),
            FeedActions::PreviousPage => self.search_previous_page(),
            FeedActions::GoToMangaPage => self.go_to_manga_page(),
            FeedActions::ScrollHistoryUp => {
                if self.tabs == FeedTabs::Timeline
                    && let Some(timeline) = &mut self.timeline
                {
                    timeline.select_previous();
                } else if self.tabs == FeedTabs::Downloads
                    && let Some(offline) = &mut self.offline_downloads
                {
                    offline.select_previous();
                } else {
                    self.select_previous_manga();
                }
            },
            FeedActions::ScrollHistoryDown => {
                if self.tabs == FeedTabs::Timeline
                    && let Some(timeline) = &mut self.timeline
                {
                    timeline.select_next();
                } else if self.tabs == FeedTabs::Downloads
                    && let Some(offline) = &mut self.offline_downloads
                {
                    offline.select_next();
                } else {
                    self.select_next_manga();
                }
            },
            FeedActions::SwitchTab => self.switch_tabs(),
            FeedActions::SetTab(tab) => self.set_tab(tab),
            FeedActions::ToggleFavorite => self.toggle_favorite_selected_manga(),
            FeedActions::CycleMangaShelf => self.cycle_selected_manga_shelf(),
            FeedActions::CycleShelfFilter => self.cycle_shelf_filter(),
            FeedActions::RefreshTimeline => self.init_search(),
            FeedActions::OpenOfflineWebViewer => self.open_offline_web_viewer(),
        }
    }

    fn clean_up(&mut self) {
        self.tasks.abort_all();
        self.search_bar.reset();
        self.history = None;
        self.timeline = None;
        self.offline_downloads = None;
        self.loading_state = None;
        self.is_typing = false;
        self.list_inner_area = Rect::default();
        self.pagination_prev_rect = None;
        self.pagination_next_rect = None;
        self.bottom_action_hit_boxes.clear();
        self.delete_modal_rects = None;
    }

    fn handle_events(&mut self, events: crate::backend::tui::Events) {
        match events {
            Events::Key(key_event) if self.state != FeedState::SearchingMangaPage => {
                self.handle_key_events(key_event);
            },
            Events::Mouse(mouse_event) if self.state != FeedState::SearchingMangaPage => {
                self.handle_mouse_event(mouse_event);
            },
            Events::Tick => self.tick(),
            _ => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use pretty_assertions::{assert_eq, assert_ne};

    use self::mpsc::unbounded_channel;
    use super::*;
    use crate::backend::database::MangaHistory;
    use crate::backend::manga_provider::Languages;
    use crate::backend::manga_provider::mock::MockMangaPageProvider;
    use crate::view::widgets::press_key;

    fn manga_history_response() -> MangaHistoryResponse {
        let mangas_in_history = vec![MangaHistory::default(), MangaHistory::default()];
        let total_items = mangas_in_history.len();

        MangaHistoryResponse {
            mangas: mangas_in_history,
            page: 1,
            total_items: total_items as u32,
        }
    }

    fn render_history_and_select(feed_page: &mut Feed<MockMangaPageProvider>) {
        feed_page.load_history(Some(manga_history_response()));

        let area = Rect::new(0, 0, 20, 20);
        let mut buf = Buffer::empty(area);

        feed_page.render_history(area, &mut buf);

        feed_page.select_next_manga();
    }

    #[test]
    fn search_for_history_when_instantiated() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let expected_event = FeedEvents::SearchTimeline;

        feed_page.init_search();

        let event = feed_page.local_event_rx.blocking_recv().expect("the event was not sent");

        assert_eq!(expected_event, event);
    }

    #[test]
    fn history_is_loaded() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();
        let response_from_database = manga_history_response();

        let expected_widget = HistoryWidget::from_database_response(response_from_database.clone());

        feed_page.load_history(Some(response_from_database));

        assert!(feed_page.history.is_some());

        assert_eq!(FeedState::DisplayingHistory, feed_page.state);

        let history = feed_page.get_history();

        assert_eq!(expected_widget.mangas, history.mangas);
        assert_eq!(expected_widget.total_results, history.total_results);
        assert_eq!(expected_widget.page, history.page);
    }

    #[test]
    fn send_events_after_history_is_loaded() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();
        let response_from_database = manga_history_response();

        feed_page.load_history(Some(response_from_database));

        let expected_event = FeedEvents::SearchRecentChapters;

        let event_sent = feed_page.local_event_rx.blocking_recv().expect("no event was sent");

        assert_eq!(expected_event, event_sent);
    }

    #[test]
    fn load_no_mangas_found_from_database() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let mut some_empty_response = manga_history_response();

        some_empty_response.mangas = vec![];

        feed_page.load_history(Some(some_empty_response));

        assert_eq!(feed_page.state, FeedState::HistoryNotFound);

        feed_page.load_history(None);

        assert_eq!(feed_page.state, FeedState::HistoryNotFound);
    }

    #[test]
    fn load_chapters_of_manga() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let mut history = manga_history_response();

        let manga_id = "some_manga_id";
        let chapter_id = "some_chapter_id";

        history.mangas.push(MangaHistory {
            id: "some_manga_id".to_string(),
            ..Default::default()
        });

        feed_page.load_history(Some(history));

        let chapter_response = vec![
            LatestChapter {
                id: chapter_id.to_string(),
                ..Default::default()
            },
            LatestChapter::default(),
        ];

        feed_page.load_recent_chapters(manga_id.to_string(), Some(chapter_response));

        let expected_result = feed_page.get_history();
        let expected_result = expected_result
            .mangas
            .iter()
            .find(|manga| manga.id == manga_id)
            .expect("manga was not loaded");

        assert!(!expected_result.recent_chapters.is_empty());

        let chapter_loaded = expected_result.recent_chapters.iter().find(|chap| chap.id == chapter_id);

        assert!(chapter_loaded.is_some())
    }

    #[tokio::test]
    async fn load_chapters_of_manga_with_event() {
        let manga_id = "some_manga_id".to_string();

        let api_client = MockMangaPageProvider::with_latest_chapter_response(vec![LatestChapter {
            id: manga_id.clone(),
            ..Default::default()
        }]);

        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(api_client.into());

        let mut history = manga_history_response();

        history.mangas.push(MangaHistory {
            id: manga_id.clone(),
            ..Default::default()
        });

        feed_page.load_history(Some(history));

        let max_amounts_ticks = 10;
        let mut count = 0;

        loop {
            if count > max_amounts_ticks {
                break;
            }
            feed_page.tasks.join_next().await;
            feed_page.tick();
            count += 1;
        }
        let history = feed_page.get_history();

        let manga_with_chapters = history
            .mangas
            .iter()
            .find(|manga| manga.id == manga_id)
            .expect("the manga was not loaded");

        assert!(!manga_with_chapters.recent_chapters.is_empty())
    }

    #[tokio::test]
    async fn goes_to_next_page() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let mut history = manga_history_response();

        history.mangas = vec![MangaHistory::default(), MangaHistory::default(), MangaHistory::default(), MangaHistory::default()];
        history.total_items = history.mangas.len() as u32;
        history.page = 1;

        feed_page.items_per_page = 3;

        feed_page.load_history(Some(history));

        feed_page.search_next_page();

        assert_eq!(feed_page.get_history().page, 2);
    }

    #[tokio::test]
    async fn goes_to_previous_history_page() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let mut history = manga_history_response();

        history.mangas = vec![MangaHistory::default(), MangaHistory::default(), MangaHistory::default(), MangaHistory::default()];
        history.total_items = history.mangas.len() as u32;
        history.page = 2;

        feed_page.items_per_page = 3;

        feed_page.load_history(Some(history));

        feed_page.search_previous_page();

        assert_eq!(feed_page.get_history().page, 1);
    }

    #[tokio::test]
    async fn switch_between_tabs() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        assert_eq!(feed_page.tabs, FeedTabs::Timeline);

        feed_page.switch_tabs();

        assert_eq!(feed_page.tabs, FeedTabs::ContinueReading);

        feed_page.switch_tabs();

        assert_eq!(feed_page.tabs, FeedTabs::Library);

        feed_page.switch_tabs();

        assert_eq!(feed_page.tabs, FeedTabs::Downloads);

        feed_page.switch_tabs();

        assert_eq!(feed_page.tabs, FeedTabs::Timeline);
    }

    #[tokio::test]
    async fn search_history_in_database() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();
        feed_page.tabs = FeedTabs::ContinueReading;

        feed_page.search_history();

        assert_eq!(feed_page.state, FeedState::SearchingHistory);

        let event_sent = feed_page.local_event_rx.recv().await.expect("no event was sent");

        match event_sent {
            FeedEvents::LoadHistory(_) => {},
            _ => panic!("expected event LoadHistory "),
        }
    }

    #[tokio::test]
    async fn listen_key_event_to_switch_tabs() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let initial_tab = feed_page.tabs;

        press_key(&mut feed_page, KeyCode::Tab);

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert_eq!(feed_page.state, FeedState::SearchingHistory);
        assert_ne!(feed_page.tabs, initial_tab);

        assert!(feed_page.history.is_none());

        let current_tab = feed_page.tabs;

        press_key(&mut feed_page, KeyCode::Tab);

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert_ne!(feed_page.tabs, current_tab);
    }

    #[tokio::test]
    async fn when_switching_tabs_remove_previous_history() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        let manga_history = MangaHistoryResponse {
            mangas: vec![MangaHistory::default()],
            ..Default::default()
        };

        feed_page.load_history(Some(manga_history));

        press_key(&mut feed_page, KeyCode::Tab);

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert!(feed_page.history.is_none());
    }

    #[tokio::test]
    async fn scrolls_history_up_and_down() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();
        feed_page.tabs = FeedTabs::ContinueReading;

        let manga_history = MangaHistoryResponse {
            mangas: vec![MangaHistory::default(), MangaHistory::default(), MangaHistory::default()],
            ..Default::default()
        };

        feed_page.load_history(Some(manga_history));

        assert!(feed_page.get_history().state.selected.is_none());

        let area = Rect::new(0, 0, 20, 20);
        let mut buf = Buffer::empty(area);

        feed_page.render_history(area, &mut buf);

        // Scroll up
        press_key(&mut feed_page, KeyCode::Char('j'));

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert!(feed_page.get_history().state.selected.is_some_and(|index| index == 0));

        // index selected should be 1
        feed_page.select_next_manga();
        // index selected should be 2
        feed_page.select_next_manga();

        // Scroll up
        press_key(&mut feed_page, KeyCode::Char('k'));

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert!(feed_page.get_history().state.selected.is_some_and(|index| index == 1));
    }

    #[tokio::test]
    async fn focus_search_bar_when_pressing_s_and_unfocus_when_pressing_esc() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        assert!(!feed_page.is_typing(), "search_bar should not be focused by default");

        press_key(&mut feed_page, KeyCode::Char('s'));

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert!(feed_page.is_typing(), "search_bar shoud be focused");

        press_key(&mut feed_page, KeyCode::Esc);

        let action_sent = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(action_sent);

        assert!(!feed_page.is_typing(), "should have unfocused the search bar");
    }

    #[tokio::test]
    async fn type_into_search_bar_when_focused() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new();

        feed_page.toggle_focus_search_bar();

        press_key(&mut feed_page, KeyCode::Char('s'));
        press_key(&mut feed_page, KeyCode::Char('e'));
        press_key(&mut feed_page, KeyCode::Char('a'));

        while let Ok(action) = feed_page.local_action_rx.try_recv() {
            feed_page.update(action);
        }

        let expected = "sea";

        assert_eq!(expected, feed_page.search_bar.value());
    }

    #[tokio::test]
    async fn when_searching_manga_page_should_not_listen_to_key_events() {
        let (tx, _) = unbounded_channel::<Events>();
        let mut feed_page: Feed<MockMangaPageProvider> =
            Feed::new().with_global_sender(tx).with_api_client(MockMangaPageProvider::new().into());

        render_history_and_select(&mut feed_page);

        feed_page.go_to_manga_page();

        assert_eq!(feed_page.state, FeedState::SearchingMangaPage);

        press_key(&mut feed_page, KeyCode::Char('j'));

        if feed_page.local_action_rx.try_recv().is_ok() {
            panic!("should not receive events")
        }
    }

    #[tokio::test]
    async fn goes_to_manga_page_when_pressing_r_with_selected_manga() {
        let (tx, mut rx) = unbounded_channel::<Events>();
        let mut feed_page: Feed<MockMangaPageProvider> =
            Feed::new().with_global_sender(tx).with_api_client(MockMangaPageProvider::new().into());

        render_history_and_select(&mut feed_page);
        press_key(&mut feed_page, KeyCode::Char('r'));

        let key_event = feed_page.local_action_rx.recv().await.expect("no key event was sent");

        feed_page.update(key_event);

        let event_sent = rx.recv().await.expect("no event was sent");

        match event_sent {
            Events::GoToMangaPage(_) => {},
            _ => panic!("wrong event was sent"),
        }
    }

    #[test]
    fn when_pressed_shift_x_it_ask_for_confirmation() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());
        feed_page.tabs = FeedTabs::Library;

        press_key(&mut feed_page, KeyCode::Char('X'));

        assert_eq!(FeedState::AskingDeleteAllConfirmation, feed_page.state)
    }

    #[test]
    fn mouse_click_on_tabs_switches_feed_tabs() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());
        feed_page.tabs_area = Rect::new(0, 0, 60, 4);
        assert_eq!(feed_page.tabs, FeedTabs::Timeline);

        // Click second quarter -> ContinueReading
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 20,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        let action = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, FeedActions::SetTab(FeedTabs::ContinueReading));
        feed_page.update(action);
        assert_eq!(feed_page.tabs, FeedTabs::ContinueReading);

        // Click third quarter -> Library
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 35,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        let action = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, FeedActions::SetTab(FeedTabs::Library));
        feed_page.update(action);
        assert_eq!(feed_page.tabs, FeedTabs::Library);

        // Click fourth quarter -> Downloads
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 50,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        let action = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, FeedActions::SetTab(FeedTabs::Downloads));
        feed_page.update(action);
        assert_eq!(feed_page.tabs, FeedTabs::Downloads);

        // Click first quarter -> Timeline
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        let action = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, FeedActions::SetTab(FeedTabs::Timeline));
        feed_page.update(action);
        assert_eq!(feed_page.tabs, FeedTabs::Timeline);
    }

    #[test]
    fn shift_r_refreshes_timeline_is_overlay_open_and_mouse_click_selects_exact_row() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());
        assert!(!feed_page.is_overlay_open());

        // Shift+R triggers RefreshTimeline (leaving 'u' free for global Home navigation)
        press_key(&mut feed_page, KeyCode::Char('R'));
        let action = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(action, FeedActions::RefreshTimeline);

        feed_page.tabs = FeedTabs::Library;
        press_key(&mut feed_page, KeyCode::Char('X'));
        assert!(feed_page.is_overlay_open());
        press_key(&mut feed_page, KeyCode::Esc);
        assert!(!feed_page.is_overlay_open());

        // Clicking inside second History card (item height = 8) selects index 1 before opening
        let mut hist = manga_history_response();
        hist.mangas = vec![
            MangaHistory {
                id: "m0".into(),
                title: "First".into(),
                ..Default::default()
            },
            MangaHistory {
                id: "m1".into(),
                title: "Second".into(),
                ..Default::default()
            },
        ];
        feed_page.load_history(Some(hist));
        feed_page.content_area = Rect::new(0, 5, 80, 20);
        feed_page.history.as_mut().unwrap().state.select(Some(0));

        // Top border is y=5, card 0 is rows 6..14, card 1 is rows 14..22 -> y=16 hits card 1
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 16,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(feed_page.history.as_ref().unwrap().state.selected, Some(1));
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::GoToMangaPage);
    }

    #[tokio::test]
    async fn downloads_tab_key_4_opens_offline_manga_without_network_and_ignores_delete() {
        let (tx, mut rx) = unbounded_channel::<Events>();
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new()
            .with_global_sender(tx)
            .with_api_client(MockMangaPageProvider::with_failing_response().into());

        // Press '4' -> switches to Downloads tab
        press_key(&mut feed_page, KeyCode::Char('4'));
        let action = feed_page.local_action_rx.recv().await.unwrap();
        assert_eq!(action, FeedActions::SetTab(FeedTabs::Downloads));
        feed_page.update(action);
        assert_eq!(feed_page.tabs, FeedTabs::Downloads);

        let offline_manga = OfflineManga {
            manga_id: "offline::Berserk".into(),
            title: "Berserk".into(),
            folder_name: "Berserk".into(),
            provider: Some(MangaProviders::Inmanga),
            is_favorite: false,
            chapters: vec![],
            total_size_bytes: 1024,
            languages: vec![Languages::Spanish],
        };
        feed_page.load_offline_downloads(vec![offline_manga.clone()]);
        assert_eq!(feed_page.state, FeedState::DisplayingHistory);

        // 'd' and 'X' do not trigger deletion or bulk confirmation modal on Downloads tab
        press_key(&mut feed_page, KeyCode::Char('d'));
        press_key(&mut feed_page, KeyCode::Char('X'));
        assert!(!feed_page.is_overlay_open());
        assert_eq!(feed_page.state, FeedState::DisplayingHistory);

        // Enter / 'r' opens Events::GoToOfflineMangaPage immediately with zero network
        press_key(&mut feed_page, KeyCode::Enter);
        let open_action = feed_page.local_action_rx.recv().await.unwrap();
        assert_eq!(open_action, FeedActions::GoToMangaPage);
        feed_page.update(open_action);

        let global_evt = rx.recv().await.expect("should send GoToOfflineMangaPage");
        match global_evt {
            Events::GoToOfflineMangaPage(m) => assert_eq!(m.title, "Berserk"),
            other => panic!("expected GoToOfflineMangaPage, got {other:?}"),
        }
    }

    fn find_text_in_buffer(buf: &Buffer, needle: &str) -> Option<(u16, u16)> {
        find_all_text_in_buffer(buf, needle).into_iter().next()
    }

    fn find_all_text_in_buffer(buf: &Buffer, needle: &str) -> Vec<(u16, u16)> {
        let mut hits = Vec::new();
        for y in buf.area.top()..buf.area.bottom() {
            let mut cols: Vec<(u16, String)> = Vec::new();
            let mut x = buf.area.left();
            while x < buf.area.right() {
                let sym = buf[(x, y)].symbol().to_string();
                let w = (Span::raw(sym.as_str()).width()).max(1) as u16;
                cols.push((x, sym));
                x = x.saturating_add(w);
            }
            for start in 0..cols.len() {
                let mut acc = String::new();
                for (_, sym) in &cols[start..] {
                    acc.push_str(sym);
                    if acc == needle {
                        hits.push((cols[start].0, y));
                        break;
                    }
                    if acc.len() >= needle.len() && !needle.starts_with(&acc) {
                        break;
                    }
                }
            }
        }
        hits
    }

    fn render_feed_on_test_backend<T: FeedPageProvider>(feed_page: &mut Feed<T>, width: u16, height: u16) -> Buffer {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let completed = terminal
            .draw(|frame| {
                feed_page.render(frame.area(), frame);
            })
            .unwrap();
        completed.buffer.clone()
    }

    fn click_left<T: FeedPageProvider>(feed_page: &mut Feed<T>, column: u16, row: u16) {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        });
    }

    fn click_right<T: FeedPageProvider>(feed_page: &mut Feed<T>, column: u16, row: u16) {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        });
    }

    #[test]
    fn rendered_tabs_align_mouse_clicks_at_wide_and_narrow_widths_without_false_hits() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());

        // Wide terminal (180 cols): previously broke because 111-col tabs_area was split into 27-col quarters
        let buf = render_feed_on_test_backend(&mut feed_page, 180, 40);

        let descargas_label = tr("Descargas", "Downloads");
        let (dx, dy) = find_text_in_buffer(&buf, descargas_label).expect("Downloads tab must be rendered");
        click_left(&mut feed_page, dx + 2, dy);
        let act = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(act, FeedActions::SetTab(FeedTabs::Downloads));
        feed_page.update(act);
        assert_eq!(feed_page.tabs, FeedTabs::Downloads);

        let lib_label = tr("Biblioteca", "Library");
        let (lx, ly) = find_text_in_buffer(&buf, lib_label).expect("Library tab must be rendered");
        click_left(&mut feed_page, lx + 1, ly);
        let act = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(act, FeedActions::SetTab(FeedTabs::Library));
        feed_page.update(act);

        let cont_label = tr("Continuar", "Continue");
        let (cx, cy) = find_text_in_buffer(&buf, cont_label).expect("Continue tab must be rendered");
        click_left(&mut feed_page, cx + 1, cy);
        let act = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(act, FeedActions::SetTab(FeedTabs::ContinueReading));
        feed_page.update(act);

        let upd_label = tr("Novedades", "Updates");
        let (ux, uy) = find_text_in_buffer(&buf, upd_label).expect("Timeline tab must be rendered");
        click_left(&mut feed_page, ux + 1, uy);
        let act = feed_page.local_action_rx.try_recv().unwrap();
        assert_eq!(act, FeedActions::SetTab(FeedTabs::Timeline));
        feed_page.update(act);

        // Clicking empty space to the right of the tab labels or on the hint row below MUST NOT switch tabs
        click_left(&mut feed_page, 95, dy);
        click_left(&mut feed_page, dx + 2, dy + 1);
        assert!(feed_page.local_action_rx.try_recv().is_err());

        // Narrow terminal (92 cols): switches automatically to compact labels and stays clickable
        let narrow_buf = render_feed_on_test_backend(&mut feed_page, 92, 30);
        let compact_dl = tr("4:Desc.", "4:Down.");
        let (ndx, ndy) = find_text_in_buffer(&narrow_buf, compact_dl).expect("Compact Downloads tab must fit on 92 cols");
        click_left(&mut feed_page, ndx + 1, ndy);
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::SetTab(FeedTabs::Downloads));
    }

    #[test]
    fn downloads_tab_scrolled_mouse_clamps_wheel_clicks_exact_card_and_inline_actions() {
        use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};

        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());
        feed_page.tabs = FeedTabs::Downloads;

        let mangas: Vec<OfflineManga> = (0..6)
            .map(|i| OfflineManga {
                manga_id: format!("id-{i}"),
                title: format!("Downloaded Manga {i}"),
                folder_name: format!("Downloaded Manga {i}"),
                provider: Some(MangaProviders::Inmanga),
                is_favorite: false,
                chapters: vec![],
                total_size_bytes: 1024,
                languages: vec![Languages::Spanish],
            })
            .collect();
        feed_page.load_offline_downloads(mangas);

        let _ = render_feed_on_test_backend(&mut feed_page, 120, 24);

        // Mouse wheel up at top clamps to 0 (no infinite wrap to 5)
        feed_page.handle_mouse_event(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        });
        let up_act = feed_page.local_action_rx.try_recv().unwrap();
        feed_page.update(up_act);
        assert_eq!(feed_page.offline_downloads.as_ref().unwrap().state.selected, Some(0));

        // Mouse wheel down past end clamps to 5 (no infinite wrap to 0)
        for _ in 0..8 {
            feed_page.handle_mouse_event(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 10,
                modifiers: KeyModifiers::NONE,
            });
            let act = feed_page.local_action_rx.try_recv().unwrap();
            feed_page.update(act);
        }
        assert_eq!(feed_page.offline_downloads.as_ref().unwrap().state.selected, Some(5));

        // Scroll back to item 2 so top card is partially truncated (scroll_truncation > 0)
        feed_page.offline_downloads.as_mut().unwrap().state.select(Some(0));
        let _ = render_feed_on_test_backend(&mut feed_page, 120, 24);
        feed_page.offline_downloads.as_mut().unwrap().state.select(Some(2));
        let scrolled_buf = render_feed_on_test_backend(&mut feed_page, 120, 24);
        assert!(
            feed_page.offline_downloads.as_ref().unwrap().state.scroll_truncation() > 0,
            "card 0 should be partially scrolled off top"
        );

        // Right-click on card 1 selects card 1 without opening it
        let (m1x, m1y) = find_text_in_buffer(&scrolled_buf, "Downloaded Manga 1").unwrap();
        click_right(&mut feed_page, m1x, m1y);
        assert_eq!(feed_page.offline_downloads.as_ref().unwrap().state.selected, Some(1));
        assert!(feed_page.local_action_rx.try_recv().is_err());

        // Restore scrolled state and click inline "Visor Web" / "Web HD" on card 2
        feed_page.offline_downloads.as_mut().unwrap().state.select(Some(2));
        let scrolled_buf = render_feed_on_test_backend(&mut feed_page, 120, 24);
        let web_label = tr("Visor Web", "Web HD");
        let web_hits = find_all_text_in_buffer(&scrolled_buf, web_label);
        // Pick the inline hit that belongs to card 2 (lowest card row above the bottom border of content_area)
        let &(wx, wy) = web_hits
            .iter()
            .filter(|&&(_, y)| y < feed_page.content_area.bottom() - 1)
            .max_by_key(|&&(_, y)| y)
            .expect("inline Web Viewer button on card 2");
        click_left(&mut feed_page, wx + 1, wy);
        assert_eq!(feed_page.offline_downloads.as_ref().unwrap().state.selected, Some(2));
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::OpenOfflineWebViewer);

        // Click inline "Fav." button on card 1 (the upper card)
        let fav_hits = find_all_text_in_buffer(&scrolled_buf, "Fav.");
        let &(fx, fy) = fav_hits
            .iter()
            .filter(|&&(_, y)| y < wy)
            .max_by_key(|&&(_, y)| y)
            .expect("inline Fav button on card 1");
        click_left(&mut feed_page, fx + 1, fy);
        assert_eq!(feed_page.offline_downloads.as_ref().unwrap().state.selected, Some(1));
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::ToggleFavorite);

        // Click body/title of card 2 -> opens GoToMangaPage for card 2
        let (m2x, m2y) = find_text_in_buffer(&scrolled_buf, "Downloaded Manga 2").unwrap();
        click_left(&mut feed_page, m2x, m2y);
        assert_eq!(feed_page.offline_downloads.as_ref().unwrap().state.selected, Some(2));
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::GoToMangaPage);

        // Click bottom bar Rescan chip -> RefreshTimeline
        let rescan_label = tr("Re-escanear", "Rescan");
        let (rx_pos, ry_pos) = find_text_in_buffer(&scrolled_buf, rescan_label).unwrap();
        click_left(&mut feed_page, rx_pos, ry_pos);
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::RefreshTimeline);
    }

    #[test]
    fn history_pagination_arrows_typing_unfocus_and_delete_modal_respond_to_mouse() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());
        feed_page.tabs = FeedTabs::ContinueReading;

        let mut hist = manga_history_response();
        hist.page = 2;
        hist.total_items = 15;
        hist.mangas = (0..5)
            .map(|i| MangaHistory {
                id: format!("m-{i}"),
                title: format!("History Manga {i}"),
                ..Default::default()
            })
            .collect();
        feed_page.load_history(Some(hist));

        let buf = render_feed_on_test_backend(&mut feed_page, 120, 30);

        // Click ◀ <b> pagination arrow -> PreviousPage
        let (px, py) = find_text_in_buffer(&buf, "◀ <b>").expect("prev arrow must be rendered");
        click_left(&mut feed_page, px, py);
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::PreviousPage);

        // Click <w> ▶ pagination arrow -> NextPage
        let (nx, ny) = find_text_in_buffer(&buf, "<w> ▶").expect("next arrow must be rendered");
        click_left(&mut feed_page, nx, ny);
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::NextPage);

        // Click <R> Refresh chip inside search bar top border -> RefreshTimeline
        let (rx, ry) = find_text_in_buffer(&buf, "<R>").expect("<R> refresh chip must be rendered");
        click_left(&mut feed_page, rx, ry);
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::RefreshTimeline);

        // Click inline "Favorito"/"Favorite" on first card -> ToggleFavorite
        let fav_label = tr("Favorito", "Favorite");
        let fav_hits = find_all_text_in_buffer(&buf, fav_label);
        let &(cfx, cfy) = fav_hits
            .iter()
            .find(|&&(_, y)| y >= feed_page.list_inner_area.y && y < feed_page.list_inner_area.bottom())
            .expect("inline card Favorite chip must be rendered");
        click_left(&mut feed_page, cfx, cfy);
        assert_eq!(feed_page.local_action_rx.try_recv().unwrap(), FeedActions::ToggleFavorite);

        // Focus search bar, then click directly over card 0: must ONLY exit typing without opening card 0
        feed_page.is_typing = true;
        let _ = render_feed_on_test_backend(&mut feed_page, 120, 30);
        let (m0x, m0y) = find_text_in_buffer(&buf, "History Manga 0").unwrap();
        click_left(&mut feed_page, m0x, m0y);
        assert!(!feed_page.is_typing);
        assert!(feed_page.local_action_rx.try_recv().is_err());

        // Clicking Cancel on delete-all modal closes the modal cleanly
        feed_page.state = FeedState::AskingDeleteAllConfirmation;
        let modal_buf = render_feed_on_test_backend(&mut feed_page, 120, 30);
        let cancel_label = tr("No, cancelar", "No, cancel");
        let (cx, cy) = find_text_in_buffer(&modal_buf, cancel_label).expect("modal cancel button");
        click_left(&mut feed_page, cx, cy);
        assert_eq!(feed_page.state, FeedState::DisplayingHistory);
    }

    #[test]
    fn empty_library_card_displays_active_shelf_badge_and_message() {
        let mut feed_page: Feed<MockMangaPageProvider> = Feed::new().with_api_client(MockMangaPageProvider::new().into());
        feed_page.tabs = FeedTabs::Library;
        feed_page.state = FeedState::HistoryNotFound;
        feed_page.active_shelf_filter = None;

        let all_buf = render_feed_on_test_backend(&mut feed_page, 120, 30);
        let all_label = format!("🏷 {} <E>", tr("Todas", "All Shelves"));
        assert!(find_text_in_buffer(&all_buf, &all_label).is_some(), "expected all-shelves badge in empty library");

        feed_page.active_shelf_filter = Some("Seinen".to_string());
        let shelf_buf = render_feed_on_test_backend(&mut feed_page, 120, 30);
        assert!(find_text_in_buffer(&shelf_buf, "🏷 Seinen <E>").is_some(), "expected active shelf badge in empty library");
    }
}
