use std::fmt::Display;

use chrono::NaiveDate;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, StatefulWidget, Widget, WidgetRef, Wrap};
use tui_widget_list::PreRender;

use crate::backend::database::MangaHistoryResponse;
use crate::backend::manga_provider::{Languages, LatestChapter, MangaProviders};
use crate::global::CURRENT_LIST_ITEM_STYLE;
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::display_dates_since_publication;
use crate::view::widgets::ModalBuilder;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum FeedTabs {
    #[default]
    Timeline,
    ContinueReading,
    Library,
}

impl Display for FeedTabs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeline => write!(f, "{}", tr("Novedades", "Updates")),
            Self::ContinueReading => write!(f, "{}", tr("Continuar leyendo", "Continue Reading")),
            Self::Library => write!(f, "{}", tr("Biblioteca", "Library")),
        }
    }
}

impl FeedTabs {
    #[inline]
    pub fn cycle(self) -> Self {
        match self {
            Self::Timeline => Self::ContinueReading,
            Self::ContinueReading => Self::Library,
            Self::Library => Self::Timeline,
        }
    }
}

pub fn provider_badge_style(provider: MangaProviders) -> Style {
    match provider {
        MangaProviders::Mangadex => Style::default().fg(Color::Rgb(255, 120, 70)),
        MangaProviders::Leercapitulo => Style::default().fg(Color::Rgb(0, 210, 255)),
        MangaProviders::Mangaoni => Style::default().fg(Color::Rgb(235, 95, 190)),
        MangaProviders::Inmanga => Style::default().fg(Color::Rgb(95, 155, 255)),
        MangaProviders::Weebcentral => Style::default().fg(Color::Rgb(80, 220, 135)),
        MangaProviders::Mangapill => Style::default().fg(Color::Rgb(245, 205, 65)),
        MangaProviders::Comizy => Style::default().fg(Color::Rgb(240, 100, 85)),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimelineChapterItem {
    pub manga_id: String,
    pub manga_title: String,
    pub provider: MangaProviders,
    pub chapter_id: String,
    pub chapter_number: String,
    pub chapter_title: String,
    pub publication_date: Option<NaiveDate>,
    pub language: Languages,
    pub is_read: bool,
    pub style: Style,
}

impl PreRender for TimelineChapterItem {
    fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
        if context.is_selected {
            self.style = *CURRENT_LIST_ITEM_STYLE;
        }
        4
    }
}

impl Widget for TimelineChapterItem {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let is_selected = self.style == *CURRENT_LIST_ITEM_STYLE;

        let block = if is_selected {
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.accent_primary).bold())
        } else {
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(t.border))
        };
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if is_selected {
            for x in inner.left()..inner.right() {
                for y in inner.top()..inner.bottom() {
                    buf[(x, y)].set_bg(t.row_selected_bg);
                }
            }
        }

        let row_bg = if is_selected { t.row_selected_bg } else { Color::Reset };

        let cursor_span = if is_selected {
            Span::styled(" ▸ ", Style::default().fg(t.accent_primary).bg(row_bg).bold())
        } else {
            Span::styled("   ", Style::default().bg(row_bg))
        };

        let provider_span = Span::styled(format!("[{}] ", self.provider), provider_badge_style(self.provider).bg(row_bg).bold());
        let title_span = Span::styled(format!("{} ", self.manga_title), Style::default().fg(t.text_primary).bg(row_bg).bold());
        let ch_num_span = Span::styled(
            format!("{} {} ", tr("Cap.", "Ch."), self.chapter_number),
            Style::default().fg(t.accent_highlight).bg(row_bg).bold(),
        );
        let lang_span = Span::styled(
            format!("│ {} {}", self.language.as_emoji(), self.language.as_human_readable()),
            Style::default().fg(t.text_muted).bg(row_bg),
        );

        let line1 = Line::from(vec![cursor_span, provider_span, title_span, ch_num_span, lang_span]);

        let status_span = if self.is_read {
            Span::styled(format!(" {} ", tr("✓ LEÍDO", "✓ READ")), Style::default().fg(t.text_muted).bg(t.chip_bg))
        } else {
            Span::styled(format!(" {} ", tr("● NUEVO", "● NEW")), Style::default().fg(Color::Black).bg(t.status_success).bold())
        };

        let ch_desc = if !self.chapter_title.is_empty() {
            Span::styled(format!("  \"{}\" ", self.chapter_title), Style::default().fg(t.text_secondary).bg(row_bg).italic())
        } else {
            Span::styled("  ", Style::default().bg(row_bg))
        };

        let date_str = match self.publication_date {
            Some(d) => format!("• {}", display_dates_since_publication(d)),
            None => String::new(),
        };
        let date_span = Span::styled(date_str, Style::default().fg(t.text_muted).bg(row_bg));

        let line2 = Line::from(vec![Span::styled("   ", Style::default().bg(row_bg)), status_span, ch_desc, date_span]);

        let layout = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
        Paragraph::new(line1).render(layout[0], buf);
        if layout.len() > 1 {
            Paragraph::new(line2).render(layout[1], buf);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TimelineWidget {
    pub items: Vec<TimelineChapterItem>,
    pub state: tui_widget_list::ListState,
    pub is_loading: bool,
}

impl TimelineWidget {
    #[inline]
    pub fn select_next(&mut self) {
        self.state.next();
    }

    #[inline]
    pub fn select_previous(&mut self) {
        self.state.previous();
    }

    #[inline]
    pub fn get_current_selected(&self) -> Option<&TimelineChapterItem> {
        self.state.selected.and_then(|index| self.items.get(index))
    }
}

impl StatefulWidget for TimelineWidget {
    type State = tui_widget_list::ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let count_unread = self.items.iter().filter(|i| !i.is_read).count();

        let mut top_left_spans = vec![
            Span::styled(
                tr(" ⚡ Novedades ", " ⚡ Updates "),
                Style::default().fg(t.accent_primary).bg(t.title_chip_primary_bg).bold(),
            ),
            Span::styled(
                format!(" ({} {}) ", self.items.len(), tr("caps", "chaps")),
                Style::default().fg(t.accent_highlight).bold(),
            ),
        ];
        if count_unread > 0 {
            top_left_spans.push(Span::styled(
                format!(" ● {count_unread} {} ", tr("nuevos", "new")),
                Style::default().fg(Color::Black).bg(t.status_success).bold(),
            ));
        }

        let bottom_instructions = Line::from(vec![
            Span::raw(" "),
            t.key_chip("j / k"),
            Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)),
            t.action_chip_primary("Enter"),
            Span::styled(tr(" Leer cap.  ", " Read chap.  "), Style::default().fg(t.text_primary).bold()),
            t.key_chip("r / m"),
            Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_muted)),
            t.key_chip("f"),
            Span::styled(tr(" Favorito  ", " Favorite  "), Style::default().fg(t.text_muted)),
            t.key_chip("d"),
            Span::styled(tr(" Descargar  ", " Download  "), Style::default().fg(t.text_muted)),
            t.key_chip("u"),
            Span::styled(tr(" Actualizar ", " Refresh "), Style::default().fg(t.text_muted)),
        ]);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(Line::from(top_left_spans))
            .title_bottom(bottom_instructions);

        let inner = block.inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.items.is_empty() {
            if self.is_loading {
                Paragraph::new(tr(
                    " ⏳ Buscando novedades en tus mangas favoritos... Por favor, espera.",
                    " ⏳ Checking updates for your favorite mangas... Please wait.",
                ))
                .style(Style::default().fg(t.accent_highlight).bold())
                .alignment(Alignment::Center)
                .render(inner, buf);
            } else {
                Paragraph::new(tr(
                    "No hay novedades recientes.\nConsejo: Pulsa <f> en cualquier ficha de manga para añadirlo a Favoritos y ver sus nuevos capítulos aquí.",
                    "No recent updates.\nTip: Press <f> on any manga page to add it to Favorites and track new chapters here.",
                ))
                .style(Style::default().fg(t.text_muted))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
            }
        } else {
            let list = tui_widget_list::List::new(self.items);
            StatefulWidget::render(list, inner, buf, state);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentChapters {
    pub id: String,
    pub title: String,
    pub number: String,
    pub translated_language: Languages,
    pub readeable_at: Option<NaiveDate>,
}

impl From<RecentChapters> for ListItem<'_> {
    fn from(value: RecentChapters) -> Self {
        let t = theme();
        let mut spans = vec![
            Span::styled(format!("Ch. {} ", value.number), Style::default().fg(t.accent_highlight).bold()),
            Span::styled(value.title, Style::default().fg(t.text_primary).bold()),
            Span::styled(" │ ", Style::default().fg(t.border_muted)),
            Span::raw(value.translated_language.as_emoji()),
            Span::raw(" "),
            Span::styled(value.translated_language.as_human_readable(), Style::default().fg(t.text_muted)),
        ];

        if let Some(date) = value.readeable_at {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("• {}", display_dates_since_publication(date)), Style::default().fg(t.text_dim)));
        }

        ListItem::new(Line::from(spans))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MangasRead {
    pub id: String,
    pub title: String,
    pub provider: MangaProviders,
    pub style: Style,
    pub recent_chapters: Vec<RecentChapters>,
    pub last_read_date: Option<String>,
    pub last_read_chapter: Option<String>,
    pub total_read_count: u32,
    pub bookmark_chapter: Option<String>,
    pub bookmark_page: Option<u32>,
    pub is_favorite: bool,
    pub is_plan_to_read: bool,
    pub downloaded_count: u32,
}

impl From<LatestChapter> for RecentChapters {
    fn from(value: LatestChapter) -> Self {
        let id = value.id;
        let translated_language = value.language;

        Self {
            id,
            title: value.title,
            number: value.chapter_number,
            readeable_at: value.publication_date,
            translated_language,
        }
    }
}

impl Widget for MangasRead {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let t = theme();
        let is_selected = self.style == *CURRENT_LIST_ITEM_STYLE;

        let card_title = Line::from(vec![
            if is_selected {
                Span::styled(" ▸ ", Style::default().fg(Color::Black).bg(t.accent_primary).bold())
            } else {
                Span::raw(" ")
            },
            Span::styled(format!(" [{}] ", self.provider), provider_badge_style(self.provider).bold()),
            Span::styled(
                format!("{} ", self.title),
                if is_selected { Style::default().fg(t.accent_primary).bold() } else { Style::default().fg(t.text_primary).bold() },
            ),
        ]);

        let outer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(if is_selected { Style::default().fg(t.accent_primary).bold() } else { Style::default().fg(t.border) })
            .title(card_title);

        let inner_card = outer_block.inner(area);
        outer_block.render(area, buf);

        if inner_card.width == 0 || inner_card.height == 0 {
            return;
        }

        let [info_area, recent_chapters_area] =
            Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).areas(inner_card);

        let mut info_lines = Vec::new();

        // 1. Badges: [★ FAVORITO] [📋 PENDIENTE] [💾 X caps]
        let mut badges = Vec::new();
        if self.is_favorite {
            badges.push(Span::styled(
                format!(" ★ {} ", tr("FAVORITO", "FAVORITE")),
                Style::default().fg(Color::Black).bg(t.status_warning).bold(),
            ));
            badges.push(Span::raw(" "));
        }
        if self.is_plan_to_read {
            badges.push(Span::styled(
                format!(" 📋 {} ", tr("PENDIENTE", "PLAN TO READ")),
                Style::default().fg(Color::Black).bg(t.accent_primary).bold(),
            ));
            badges.push(Span::raw(" "));
        }
        if self.downloaded_count > 0 {
            badges.push(Span::styled(
                format!(" 💾 {} {} ", self.downloaded_count, tr("caps", "chaps")),
                Style::default().fg(Color::Black).bg(t.accent_secondary).bold(),
            ));
        }
        if !badges.is_empty() {
            info_lines.push(Line::from(badges));
        }

        // 2. Last read chapter
        if let Some(ch) = &self.last_read_chapter {
            info_lines.push(Line::from(vec![
                Span::styled(tr("📖 Último leído: ", "📖 Last read: "), Style::default().fg(t.accent_primary).bold()),
                Span::styled(ch.clone(), Style::default().fg(t.text_primary)),
            ]));
        } else if self.is_plan_to_read {
            info_lines.push(Line::from(vec![
                Span::styled(tr("📌 Estado: ", "📌 Status: "), Style::default().fg(t.text_dim)),
                Span::styled(tr("En lista para leer más tarde", "Saved in Plan to Read"), Style::default().fg(t.text_secondary)),
            ]));
        }

        // 3. Progress / Bookmark
        let mut progress_spans = Vec::new();
        if self.total_read_count > 0 {
            progress_spans.push(Span::styled(
                format!("📊 {} {} ", self.total_read_count, tr("caps leídos", "chapters read")),
                Style::default().fg(t.text_secondary),
            ));
        }
        if let Some(page) = self.bookmark_page {
            progress_spans.push(Span::styled(
                format!("🔖 {} {page}", tr("Marcador en pág.", "Bookmark at page")),
                Style::default().fg(t.accent_highlight).bold(),
            ));
        }
        if !progress_spans.is_empty() {
            info_lines.push(Line::from(progress_spans));
        }

        // 4. Action hint
        info_lines.push(Line::from(vec![
            t.key_chip("Enter / r"),
            Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_muted)),
            t.key_chip("f"),
            Span::styled(tr(" Favorito", " Favorite"), Style::default().fg(t.text_muted)),
        ]));

        Paragraph::new(info_lines)
            .block(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(t.border_muted)))
            .wrap(Wrap { trim: true })
            .render(
                info_area.inner(Margin {
                    horizontal: 1,
                    vertical: 0,
                }),
                buf,
            );

        let recent_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_muted))
            .title(Span::styled(
                tr(" 🕒 Capítulos recientes ", " 🕒 Recent chapters "),
                Style::default().fg(t.accent_secondary).bold(),
            ));

        if !self.recent_chapters.is_empty() {
            Widget::render(List::new(self.recent_chapters).block(recent_block), recent_chapters_area, buf);
        } else {
            Paragraph::new(tr(" Sin capítulos recientes cargados", " No recent chapters loaded"))
                .style(Style::default().fg(t.text_dim))
                .block(recent_block)
                .render(recent_chapters_area, buf);
        }
    }
}

impl PreRender for MangasRead {
    fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
        if context.is_selected {
            self.style = *CURRENT_LIST_ITEM_STYLE;
        }
        8
    }
}

#[derive(Clone, Debug, Default)]
pub struct HistoryWidget {
    pub page: u32,
    pub total_results: u32,
    pub mangas: Vec<MangasRead>,
    pub state: tui_widget_list::ListState,
    pub active_tab: FeedTabs,
}

impl HistoryWidget {
    #[inline]
    pub fn select_next(&mut self) {
        self.state.next();
    }

    #[inline]
    pub fn select_previous(&mut self) {
        self.state.previous();
    }

    #[inline]
    pub fn get_current_manga_selected(&self) -> Option<&MangasRead> {
        self.state.selected.and_then(|index| self.mangas.get(index))
    }

    #[inline]
    pub fn next_page(&mut self) {
        self.page += 1
    }

    #[inline]
    pub fn previous_page(&mut self) {
        self.page -= 1
    }

    pub fn set_chapter(&mut self, manga_id: String, response: Vec<LatestChapter>) {
        if let Some(manga) = self.mangas.iter_mut().find(|manga| manga.id == manga_id) {
            for chapter in response {
                manga.recent_chapters.push(RecentChapters::from(chapter));
            }
        }
    }

    pub fn can_search_next_page(&self, total_items: f64) -> bool {
        self.page as f64 != (self.total_results as f64 / total_items).ceil() && !self.mangas.is_empty()
    }

    pub fn can_search_previous_page(&self) -> bool {
        !self.mangas.is_empty() && self.page != 1
    }

    pub fn from_database_response(response: MangaHistoryResponse) -> Self {
        Self {
            page: response.page,
            total_results: response.total_items,
            mangas: response
                .mangas
                .into_iter()
                .map(|history| MangasRead {
                    id: history.id,
                    title: history.title,
                    provider: history.provider,
                    recent_chapters: vec![],
                    style: Style::default(),
                    last_read_date: history.last_read,
                    last_read_chapter: history.last_read_chapter,
                    total_read_count: history.total_read_count,
                    bookmark_chapter: history.bookmark_chapter,
                    bookmark_page: history.bookmark_page,
                    is_favorite: history.is_favorite,
                    is_plan_to_read: history.is_plan_to_read,
                    downloaded_count: history.downloaded_count,
                })
                .collect(),
            state: tui_widget_list::ListState::default(),
            active_tab: FeedTabs::ContinueReading,
        }
    }
}

impl StatefulWidget for HistoryWidget {
    type State = tui_widget_list::ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let amount_pages = (self.total_results as f64 / 5_f64).ceil().max(1.0) as u32;

        let tab_label = match self.active_tab {
            FeedTabs::Library => tr(" 📚 Biblioteca ", " 📚 Library "),
            _ => tr(" 📖 Continuar leyendo ", " 📖 Continue Reading "),
        };

        let top_left = Line::from(vec![
            Span::styled(tab_label, Style::default().fg(t.accent_primary).bg(t.title_chip_primary_bg).bold()),
            Span::styled(format!(" ({}) ", self.total_results), Style::default().fg(t.accent_highlight).bold()),
        ]);

        let pagination_top = Line::from(vec![
            Span::styled(" ◀ <b> ", Style::default().fg(t.accent_primary).bold()),
            Span::styled(
                format!(" {} {}/{amount_pages} ", tr("Pág.", "Page"), self.page),
                Style::default().fg(Color::Black).bg(t.accent_primary).bold(),
            ),
            Span::styled(" <w> ▶ ", Style::default().fg(t.accent_primary).bold()),
        ])
        .right_aligned();

        let bottom_instructions = Line::from(vec![
            Span::raw(" "),
            t.key_chip("j / k"),
            Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)),
            t.action_chip_primary("Enter / r"),
            Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_primary).bold()),
            t.key_chip("w / b"),
            Span::styled(tr(" Página  ", " Page  "), Style::default().fg(t.text_muted)),
            t.key_chip("f"),
            Span::styled(tr(" Favorito  ", " Favorite  "), Style::default().fg(t.text_muted)),
            t.key_chip("d"),
            Span::styled(tr(" Quitar  ", " Remove  "), Style::default().fg(t.text_muted)),
            Span::styled(" X ", Style::default().fg(Color::White).bg(t.status_error).bold()),
            Span::styled(tr(" Borrar todo ", " Clear all "), Style::default().fg(t.text_muted)),
        ]);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(top_left)
            .title_top(pagination_top)
            .title_bottom(bottom_instructions);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let list = tui_widget_list::List::new(self.mangas);
        StatefulWidget::render(list, inner, buf, state);
    }
}

#[derive(Debug)]
struct AskConfirmationDeleteAllModalBody {
    manga_provider: MangaProviders,
    tab: FeedTabs,
}

impl WidgetRef for AskConfirmationDeleteAllModalBody {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.status_error))
            .title(Span::styled(
                tr(" ⚠ Confirmar borrado masivo ", " ⚠ Confirm bulk deletion "),
                Style::default().fg(t.status_error).bold(),
            ));

        let inner = block.inner(area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(area, buf);

        let [warning_area, options_area] = Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(65)]).areas(inner);

        let warning = format!(
            "¿Estás seguro de que quieres eliminar TODOS los mangas del proveedor {} en la sección {}?",
            self.manga_provider, self.tab
        );

        Paragraph::new(warning)
            .style(Style::default().fg(t.text_primary).bold())
            .wrap(Wrap { trim: true })
            .render(warning_area, buf);

        let lines = vec![
            Line::styled(
                "Tu historial de lectura y descargas se mantendrán pero no se mostrarán en esta página.",
                Style::default().fg(t.text_muted),
            ),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(" w ", Style::default().fg(Color::White).bg(t.status_error).bold()),
                Span::styled(tr(" Sí, eliminar todo", " Yes, delete all"), Style::default().fg(t.text_primary).bold()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(" q / Esc ", Style::default().fg(Color::Black).bg(t.status_success).bold()),
                Span::styled(tr(" No, cancelar", " No, cancel"), Style::default().fg(t.text_primary).bold()),
            ]),
        ];

        Paragraph::new(lines).wrap(Wrap { trim: true }).render(options_area, buf);
    }
}

#[derive(Debug)]
pub struct AskConfirmationDeleteAllModal {
    manga_provider: MangaProviders,
    tab: FeedTabs,
}

impl AskConfirmationDeleteAllModal {
    pub fn new(tab: FeedTabs, provider: MangaProviders) -> Self {
        Self {
            manga_provider: provider,
            tab,
        }
    }
}

impl Widget for AskConfirmationDeleteAllModal {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let modal = ModalBuilder::new(AskConfirmationDeleteAllModalBody {
            manga_provider: self.manga_provider,
            tab: self.tab,
        })
        .with_dimensions(super::ModalDimensions {
            width: 70,
            height: 60,
        })
        .build();

        modal.render(area, buf);
    }
}
