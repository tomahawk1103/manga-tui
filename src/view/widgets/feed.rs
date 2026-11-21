use std::fmt::Display;

use chrono::NaiveDate;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, StatefulWidget, Widget, WidgetRef, Wrap};
use tui_widget_list::{ListBuilder, ListView};

use crate::backend::database::MangaHistoryResponse;
use crate::backend::manga_provider::{Languages, LatestChapter, MangaProviders};
use crate::backend::offline::{OfflineManga, format_size_bytes};
use crate::global::CURRENT_LIST_ITEM_STYLE;
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::display_dates_since_publication;
use crate::view::widgets::ModalBuilder;

pub const TIMELINE_ITEM_HEIGHT: u16 = 4;
pub const HISTORY_ITEM_HEIGHT: u16 = 8;
pub const OFFLINE_ITEM_HEIGHT: u16 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedBottomAction {
    OpenManga,
    ReadTimelineChapter,
    OpenOfflineWebViewer,
    ToggleFavorite,
    DownloadTimelineChapter,
    RemoveSelected,
    AskDeleteAll,
    ToggleSearch,
    Refresh,
    NextPage,
    PreviousPage,
    CycleTab,
    GoToSearch,
}

pub fn build_bottom_instructions_with_hitboxes(
    area: Rect,
    items: Vec<(Span<'static>, Span<'static>, Option<FeedBottomAction>)>,
) -> (Line<'static>, Vec<(Rect, FeedBottomAction)>) {
    let mut spans = vec![Span::raw(" ")];
    let mut hit_boxes = Vec::new();
    let max_x = area.right().saturating_sub(1);
    let y = area.bottom().saturating_sub(1);
    let mut cursor_x = area.x.saturating_add(2);

    for (key_span, desc_span, maybe_action) in items {
        let key_w = key_span.width() as u16;
        let full_desc_w = desc_span.width() as u16;
        let trimmed_desc_w = Span::raw(desc_span.content.trim_end()).width() as u16;
        let hit_w = key_w.saturating_add(trimmed_desc_w);

        if let Some(action) = maybe_action
            && area.height > 0
            && area.width > 2
            && cursor_x < max_x
            && hit_w > 0
        {
            let clamped_w = hit_w.min(max_x.saturating_sub(cursor_x));
            if clamped_w > 0 {
                hit_boxes.push((Rect::new(cursor_x, y, clamped_w, 1), action));
            }
        }

        cursor_x = cursor_x.saturating_add(key_w).saturating_add(full_desc_w);
        spans.push(key_span);
        spans.push(desc_span);
    }

    (Line::from(spans), hit_boxes)
}

fn clamp_select_next(state: &mut tui_widget_list::ListState, len: usize) {
    if len == 0 {
        return;
    }
    let next = match state.selected {
        Some(idx) => (idx + 1).min(len - 1),
        None => 0,
    };
    state.select(Some(next));
}

fn clamp_select_previous(state: &mut tui_widget_list::ListState, len: usize) {
    if len == 0 {
        return;
    }
    let prev = match state.selected {
        Some(idx) => idx.saturating_sub(1),
        None => 0,
    };
    state.select(Some(prev));
}

fn render_inline_action_segments(area: Rect, buf: &mut Buffer, segments: Vec<(Vec<Span<'static>>, Option<FeedBottomAction>)>) {
    let mut spans = Vec::new();
    for (seg_spans, _) in segments {
        spans.extend(seg_spans);
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn inline_action_hit_ranges(
    info_x: u16,
    info_right: u16,
    segments: Vec<(Vec<Span<'static>>, Option<FeedBottomAction>)>,
) -> Vec<((u16, u16), FeedBottomAction)> {
    let mut ranges = Vec::new();
    let mut cursor_x = info_x;
    for (seg_spans, maybe_action) in segments {
        let full_w: u16 = seg_spans.iter().map(|s| s.width() as u16).sum();
        let trimmed_w: u16 = if let Some(last) = seg_spans.last() {
            let prefix_w: u16 = seg_spans[..seg_spans.len() - 1].iter().map(|s| s.width() as u16).sum();
            let last_trimmed = Span::raw(last.content.trim_end()).width() as u16;
            prefix_w.saturating_add(last_trimmed)
        } else {
            0
        };
        if let Some(action) = maybe_action
            && cursor_x < info_right
            && trimmed_w > 0
        {
            let end_x = cursor_x.saturating_add(trimmed_w).min(info_right);
            if end_x > cursor_x {
                ranges.push(((cursor_x, end_x), action));
            }
        }
        cursor_x = cursor_x.saturating_add(full_w);
    }
    ranges
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum FeedTabs {
    #[default]
    Timeline,
    ContinueReading,
    Library,
    Downloads,
}

impl Display for FeedTabs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeline => write!(f, "{}", tr("Novedades", "Updates")),
            Self::ContinueReading => write!(f, "{}", tr("Continuar leyendo", "Continue Reading")),
            Self::Library => write!(f, "{}", tr("Biblioteca", "Library")),
            Self::Downloads => write!(f, "{}", tr("Descargas", "Downloads")),
        }
    }
}

impl FeedTabs {
    #[inline]
    pub fn cycle(self) -> Self {
        match self {
            Self::Timeline => Self::ContinueReading,
            Self::ContinueReading => Self::Library,
            Self::Library => Self::Downloads,
            Self::Downloads => Self::Timeline,
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
        clamp_select_next(&mut self.state, self.items.len());
    }

    #[inline]
    pub fn select_previous(&mut self) {
        clamp_select_previous(&mut self.state, self.items.len());
    }

    #[inline]
    pub fn get_current_selected(&self) -> Option<&TimelineChapterItem> {
        self.state.selected.and_then(|index| self.items.get(index))
    }

    pub fn list_inner_area(area: Rect) -> Rect {
        Block::bordered().border_type(BorderType::Rounded).inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        })
    }

    fn bottom_items() -> Vec<(Span<'static>, Span<'static>, Option<FeedBottomAction>)> {
        let t = theme();
        vec![
            (t.key_chip("j / k"), Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)), None),
            (
                t.action_chip_primary("Enter"),
                Span::styled(tr(" Leer cap.  ", " Read chap.  "), Style::default().fg(t.text_primary).bold()),
                Some(FeedBottomAction::ReadTimelineChapter),
            ),
            (
                t.key_chip("r / m"),
                Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::OpenManga),
            ),
            (
                t.key_chip("f"),
                Span::styled(tr(" Favorito  ", " Favorite  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::ToggleFavorite),
            ),
            (
                t.key_chip("d"),
                Span::styled(tr(" Descargar  ", " Download  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::DownloadTimelineChapter),
            ),
            (
                t.key_chip("R"),
                Span::styled(tr(" Actualizar ", " Refresh "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::Refresh),
            ),
        ]
    }

    pub fn bottom_action_hit_boxes(area: Rect) -> Vec<(Rect, FeedBottomAction)> {
        build_bottom_instructions_with_hitboxes(area, Self::bottom_items()).1
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

        let (bottom_instructions, _) = build_bottom_instructions_with_hitboxes(area, Self::bottom_items());

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(Line::from(top_left_spans))
            .title_bottom(bottom_instructions);

        let inner = Self::list_inner_area(area);
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
            let items = self.items;
            let item_count = items.len();
            let builder = ListBuilder::new(move |context| {
                let mut item = items[context.index].clone();
                if context.is_selected {
                    item.style = *CURRENT_LIST_ITEM_STYLE;
                }
                (item, TIMELINE_ITEM_HEIGHT)
            });
            let list = ListView::new(builder, item_count).infinite_scrolling(false);
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
            Span::styled(format!("{} {} ", tr("Cap.", "Ch."), value.number), Style::default().fg(t.accent_highlight).bold()),
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

impl MangasRead {
    pub const fn action_row_local_y() -> usize {
        (HISTORY_ITEM_HEIGHT - 2) as usize
    }

    fn action_segments() -> Vec<(Vec<Span<'static>>, Option<FeedBottomAction>)> {
        let t = theme();
        vec![
            (
                vec![
                    t.key_chip("Enter / r"),
                    Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_muted)),
                ],
                Some(FeedBottomAction::OpenManga),
            ),
            (
                vec![t.key_chip("f"), Span::styled(tr(" Favorito", " Favorite"), Style::default().fg(t.text_muted))],
                Some(FeedBottomAction::ToggleFavorite),
            ),
        ]
    }

    pub fn inline_favorite_x_range(list_inner_x: u16, list_inner_width: u16) -> Option<(u16, u16)> {
        let inner_w = list_inner_width.saturating_sub(2);
        if inner_w < 4 {
            return None;
        }
        let inner_rect = Rect::new(list_inner_x.saturating_add(1), 0, inner_w, 1);
        let [info_area, _] = Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).areas(inner_rect);
        let info_inner = Block::default().borders(Borders::RIGHT).inner(info_area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }));
        inline_action_hit_ranges(info_inner.x, info_inner.right(), Self::action_segments())
            .into_iter()
            .find_map(|(range, act)| (act == FeedBottomAction::ToggleFavorite).then_some(range))
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

        let info_outer = info_area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        let info_block = Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(t.border_muted));
        let info_inner = info_block.inner(info_outer);
        info_block.render(info_outer, buf);

        if info_inner.width > 0 && info_inner.height > 0 {
            let [top_info_area, action_row_area] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(info_inner);
            Paragraph::new(info_lines).wrap(Wrap { trim: true }).render(top_info_area, buf);
            render_inline_action_segments(action_row_area, buf, Self::action_segments());
        }

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
        clamp_select_next(&mut self.state, self.mangas.len());
    }

    #[inline]
    pub fn select_previous(&mut self) {
        clamp_select_previous(&mut self.state, self.mangas.len());
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

    pub fn list_inner_area(area: Rect) -> Rect {
        Block::bordered().border_type(BorderType::Rounded).inner(area)
    }

    fn pagination_spans(page: u32, total_results: u32) -> (Span<'static>, Span<'static>, Span<'static>) {
        let t = theme();
        let amount_pages = (total_results as f64 / 5_f64).ceil().max(1.0) as u32;
        (
            Span::styled(" ◀ <b> ", Style::default().fg(t.accent_primary).bold()),
            Span::styled(
                format!(" {} {page}/{amount_pages} ", tr("Pág.", "Page")),
                Style::default().fg(Color::Black).bg(t.accent_primary).bold(),
            ),
            Span::styled(" <w> ▶ ", Style::default().fg(t.accent_primary).bold()),
        )
    }

    pub fn pagination_hit_boxes(area: Rect, page: u32, total_results: u32) -> (Option<Rect>, Option<Rect>) {
        if area.width <= 2 || area.height == 0 {
            return (None, None);
        }
        let (prev_span, page_span, next_span) = Self::pagination_spans(page, total_results);
        let prev_w = prev_span.width() as u16;
        let page_w = page_span.width() as u16;
        let next_w = next_span.width() as u16;
        let total_w = prev_w.saturating_add(page_w).saturating_add(next_w);
        let max_x = area.right().saturating_sub(1);
        let min_x = area.x.saturating_add(1);
        if max_x <= min_x || total_w == 0 || max_x.saturating_sub(min_x) < total_w {
            return (None, None);
        }
        let start_x = max_x.saturating_sub(total_w);
        (Some(Rect::new(start_x, area.y, prev_w, 1)), Some(Rect::new(max_x.saturating_sub(next_w), area.y, next_w, 1)))
    }

    fn bottom_items() -> Vec<(Span<'static>, Span<'static>, Option<FeedBottomAction>)> {
        let t = theme();
        vec![
            (t.key_chip("j / k"), Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)), None),
            (
                t.action_chip_primary("Enter / r"),
                Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_primary).bold()),
                Some(FeedBottomAction::OpenManga),
            ),
            (t.key_chip("w / b"), Span::styled(tr(" Página  ", " Page  "), Style::default().fg(t.text_muted)), None),
            (
                t.key_chip("f"),
                Span::styled(tr(" Favorito  ", " Favorite  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::ToggleFavorite),
            ),
            (
                t.key_chip("d"),
                Span::styled(tr(" Quitar  ", " Remove  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::RemoveSelected),
            ),
            (
                Span::styled(" X ", Style::default().fg(Color::White).bg(t.status_error).bold()),
                Span::styled(tr(" Borrar todo ", " Clear all "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::AskDeleteAll),
            ),
        ]
    }

    pub fn bottom_action_hit_boxes(area: Rect) -> Vec<(Rect, FeedBottomAction)> {
        build_bottom_instructions_with_hitboxes(area, Self::bottom_items()).1
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

        let tab_label = match self.active_tab {
            FeedTabs::Library => tr(" 📚 Biblioteca ", " 📚 Library "),
            _ => tr(" 📖 Continuar leyendo ", " 📖 Continue Reading "),
        };

        let top_left = Line::from(vec![
            Span::styled(tab_label, Style::default().fg(t.accent_primary).bg(t.title_chip_primary_bg).bold()),
            Span::styled(format!(" ({}) ", self.total_results), Style::default().fg(t.accent_highlight).bold()),
        ]);

        let (prev_span, page_span, next_span) = Self::pagination_spans(self.page, self.total_results);
        let pagination_top = Line::from(vec![prev_span, page_span, next_span]).right_aligned();

        let (bottom_instructions, _) = build_bottom_instructions_with_hitboxes(area, Self::bottom_items());

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(top_left)
            .title_top(pagination_top)
            .title_bottom(bottom_instructions);

        let inner = Self::list_inner_area(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mangas = self.mangas;
        let item_count = mangas.len();
        let builder = ListBuilder::new(move |context| {
            let mut item = mangas[context.index].clone();
            if context.is_selected {
                item.style = *CURRENT_LIST_ITEM_STYLE;
            }
            (item, HISTORY_ITEM_HEIGHT)
        });
        let list = ListView::new(builder, item_count).infinite_scrolling(false);
        StatefulWidget::render(list, inner, buf, state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfflineChapterPreview {
    pub chapter_number: String,
    pub chapter_title: String,
    pub format_label: &'static str,
    pub is_read: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfflineMangaItem {
    pub manga_id: String,
    pub title: String,
    pub provider: Option<MangaProviders>,
    pub is_favorite: bool,
    pub chapter_count: usize,
    pub read_count: usize,
    pub total_size_bytes: u64,
    pub formats: Vec<&'static str>,
    pub preview_chapters: Vec<OfflineChapterPreview>,
    pub style: Style,
}

impl From<&OfflineManga> for OfflineMangaItem {
    fn from(manga: &OfflineManga) -> Self {
        let preview_chapters = manga
            .chapters
            .iter()
            .take(4)
            .map(|c| OfflineChapterPreview {
                chapter_number: c.chapter.chapter_number.clone(),
                chapter_title: c.chapter.title.clone(),
                format_label: c.format_label,
                is_read: c.is_read,
            })
            .collect();

        Self {
            manga_id: manga.manga_id.clone(),
            title: manga.title.clone(),
            provider: manga.provider,
            is_favorite: manga.is_favorite,
            chapter_count: manga.chapters.len(),
            read_count: manga.read_count(),
            total_size_bytes: manga.total_size_bytes,
            formats: manga.format_labels(),
            preview_chapters,
            style: Style::default(),
        }
    }
}

impl OfflineMangaItem {
    pub const fn action_row_local_y() -> usize {
        (OFFLINE_ITEM_HEIGHT - 2) as usize
    }

    fn action_segments(has_provider: bool) -> Vec<(Vec<Span<'static>>, Option<FeedBottomAction>)> {
        let t = theme();
        let mut segments = vec![
            (
                vec![t.key_chip("Enter / r"), Span::styled(tr(" Abrir  ", " Open  "), Style::default().fg(t.text_muted))],
                Some(FeedBottomAction::OpenManga),
            ),
            (
                vec![t.key_chip("v"), Span::styled(tr(" Visor Web", " Web HD"), Style::default().fg(t.text_muted))],
                Some(FeedBottomAction::OpenOfflineWebViewer),
            ),
        ];
        if has_provider {
            segments.push((vec![Span::raw("  ")], None));
            segments.push((
                vec![t.key_chip("f"), Span::styled(tr(" Fav.", " Fav."), Style::default().fg(t.text_muted))],
                Some(FeedBottomAction::ToggleFavorite),
            ));
        }
        segments
    }

    pub fn inline_action_x_ranges(list_inner_x: u16, list_inner_width: u16, has_provider: bool) -> OfflineInlineActionRanges {
        let inner_w = list_inner_width.saturating_sub(2);
        if inner_w < 4 {
            return OfflineInlineActionRanges::default();
        }
        let inner_rect = Rect::new(list_inner_x.saturating_add(1), 0, inner_w, 1);
        let [info_area, _] = Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).areas(inner_rect);
        let info_inner = Block::default().borders(Borders::RIGHT).inner(info_area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        }));
        let ranges = inline_action_hit_ranges(info_inner.x, info_inner.right(), Self::action_segments(has_provider));
        OfflineInlineActionRanges {
            web_viewer: ranges
                .iter()
                .find_map(|(r, a)| (*a == FeedBottomAction::OpenOfflineWebViewer).then_some(*r)),
            favorite: ranges.iter().find_map(|(r, a)| (*a == FeedBottomAction::ToggleFavorite).then_some(*r)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OfflineInlineActionRanges {
    pub web_viewer: Option<(u16, u16)>,
    pub favorite: Option<(u16, u16)>,
}

impl Widget for OfflineMangaItem {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let is_selected = self.style == *CURRENT_LIST_ITEM_STYLE;

        let provider_span = match self.provider {
            Some(p) => Span::styled(format!(" [{p}] "), provider_badge_style(p).bold()),
            None => Span::styled(" [LOCAL] ", Style::default().fg(t.accent_secondary).bold()),
        };

        let card_title = Line::from(vec![
            if is_selected {
                Span::styled(" ▸ ", Style::default().fg(Color::Black).bg(t.accent_primary).bold())
            } else {
                Span::raw(" ")
            },
            provider_span,
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

        let [info_area, chapters_area] =
            Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).areas(inner_card);

        let mut info_lines = Vec::new();
        let mut badges = Vec::new();
        if self.is_favorite {
            badges.push(Span::styled(
                format!(" ★ {} ", tr("FAVORITO", "FAVORITE")),
                Style::default().fg(Color::Black).bg(t.status_warning).bold(),
            ));
            badges.push(Span::raw(" "));
        }
        let formats_label = self.formats.join("/");
        badges.push(Span::styled(
            format!(
                " 💾 {} {} · {} · {} ",
                self.chapter_count,
                tr("caps", "chaps"),
                formats_label,
                format_size_bytes(self.total_size_bytes)
            ),
            Style::default().fg(Color::Black).bg(t.accent_secondary).bold(),
        ));
        info_lines.push(Line::from(badges));

        info_lines.push(Line::from(vec![
            Span::styled(tr("✓ Leídos: ", "✓ Read: "), Style::default().fg(t.status_success).bold()),
            Span::styled(format!("{}/{}", self.read_count, self.chapter_count), Style::default().fg(t.text_primary)),
        ]));

        let info_outer = info_area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        let info_block = Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(t.border_muted));
        let info_inner = info_block.inner(info_outer);
        info_block.render(info_outer, buf);

        if info_inner.width > 0 && info_inner.height > 0 {
            let [top_info_area, action_row_area] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(info_inner);
            Paragraph::new(info_lines).wrap(Wrap { trim: true }).render(top_info_area, buf);
            render_inline_action_segments(action_row_area, buf, Self::action_segments(self.provider.is_some()));
        }

        let preview_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_muted))
            .title(Span::styled(
                tr(" 📄 Capítulos en disco ", " 📄 Chapters on disk "),
                Style::default().fg(t.accent_secondary).bold(),
            ));

        let list_items: Vec<ListItem<'_>> = self
            .preview_chapters
            .into_iter()
            .map(|ch| {
                let status = if ch.is_read {
                    Span::styled("✓ ", Style::default().fg(t.status_success).bold())
                } else {
                    Span::styled("○ ", Style::default().fg(t.accent_highlight))
                };
                let label = if ch.chapter_title.trim().is_empty() {
                    format!("{} {} ", tr("Cap.", "Ch."), ch.chapter_number)
                } else {
                    format!("{} {} — {} ", tr("Cap.", "Ch."), ch.chapter_number, ch.chapter_title)
                };
                ListItem::new(Line::from(vec![
                    status,
                    Span::styled(format!("[{}] ", ch.format_label), Style::default().fg(t.accent_primary).bold()),
                    Span::styled(label, Style::default().fg(t.text_primary)),
                ]))
            })
            .collect();

        Widget::render(List::new(list_items).block(preview_block), chapters_area, buf);
    }
}

#[derive(Clone, Debug, Default)]
pub struct OfflineDownloadsWidget {
    pub items: Vec<OfflineMangaItem>,
    pub raw_mangas: Vec<OfflineManga>,
    pub state: tui_widget_list::ListState,
    pub is_loading: bool,
}

impl OfflineDownloadsWidget {
    pub fn from_offline_mangas(mangas: Vec<OfflineManga>) -> Self {
        let items: Vec<OfflineMangaItem> = mangas.iter().map(OfflineMangaItem::from).collect();
        let mut state = tui_widget_list::ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            items,
            raw_mangas: mangas,
            state,
            is_loading: false,
        }
    }

    #[inline]
    pub fn select_next(&mut self) {
        clamp_select_next(&mut self.state, self.items.len());
    }

    #[inline]
    pub fn select_previous(&mut self) {
        clamp_select_previous(&mut self.state, self.items.len());
    }

    #[inline]
    pub fn get_current_selected(&self) -> Option<&OfflineManga> {
        self.state.selected.and_then(|index| self.raw_mangas.get(index))
    }

    pub fn list_inner_area(area: Rect) -> Rect {
        Block::bordered().border_type(BorderType::Rounded).inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        })
    }

    fn bottom_items() -> Vec<(Span<'static>, Span<'static>, Option<FeedBottomAction>)> {
        let t = theme();
        vec![
            (t.key_chip("j / k"), Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)), None),
            (
                t.action_chip_primary("Enter / r"),
                Span::styled(tr(" Abrir ficha  ", " Open manga  "), Style::default().fg(t.text_primary).bold()),
                Some(FeedBottomAction::OpenManga),
            ),
            (
                t.key_chip("v"),
                Span::styled(tr(" Visor Web  ", " Web HD  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::OpenOfflineWebViewer),
            ),
            (
                t.key_chip("s"),
                Span::styled(tr(" Filtrar  ", " Filter  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::ToggleSearch),
            ),
            (
                t.key_chip("R"),
                Span::styled(tr(" Re-escanear  ", " Rescan  "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::Refresh),
            ),
            (
                t.key_chip("f"),
                Span::styled(tr(" Favorito ", " Favorite "), Style::default().fg(t.text_muted)),
                Some(FeedBottomAction::ToggleFavorite),
            ),
        ]
    }

    pub fn bottom_action_hit_boxes(area: Rect) -> Vec<(Rect, FeedBottomAction)> {
        build_bottom_instructions_with_hitboxes(area, Self::bottom_items()).1
    }
}

impl StatefulWidget for OfflineDownloadsWidget {
    type State = tui_widget_list::ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let total_mangas = self.raw_mangas.len();
        let total_chapters: usize = self.raw_mangas.iter().map(|m| m.chapters.len()).sum();
        let total_size: u64 = self.raw_mangas.iter().map(|m| m.total_size_bytes).sum();

        let top_left = Line::from(vec![
            Span::styled(
                tr(" 💾 Descargas Offline ", " 💾 Offline Downloads "),
                Style::default().fg(t.accent_primary).bg(t.title_chip_primary_bg).bold(),
            ),
            Span::styled(
                format!(" ({total_mangas} mangas · {total_chapters} {} · {}) ", tr("caps", "chaps"), format_size_bytes(total_size)),
                Style::default().fg(t.accent_highlight).bold(),
            ),
        ]);

        let (bottom_instructions, _) = build_bottom_instructions_with_hitboxes(area, Self::bottom_items());

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(top_left)
            .title_bottom(bottom_instructions);

        let inner = Self::list_inner_area(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.items.is_empty() {
            if self.is_loading {
                Paragraph::new(tr(
                    " ⏳ Escaneando carpeta de descargas... Por favor, espera.",
                    " ⏳ Scanning downloads directory... Please wait.",
                ))
                .style(Style::default().fg(t.accent_highlight).bold())
                .alignment(Alignment::Center)
                .render(inner, buf);
            } else {
                Paragraph::new(tr(
                    "No hay capítulos descargados en tu carpeta de descargas.\nConsejo: Descarga cualquier capítulo con <d> o <a> y léelo aquí 100% sin conexión (CBZ, RAW, EPUB, PDF).",
                    "No downloaded chapters found in your downloads directory.\nTip: Download any chapter with <d> or <a> and read it here 100% offline (CBZ, RAW, EPUB, PDF).",
                ))
                .style(Style::default().fg(t.text_muted))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
            }
        } else {
            let items = self.items;
            let item_count = items.len();
            let builder = ListBuilder::new(move |context| {
                let mut item = items[context.index].clone();
                if context.is_selected {
                    item.style = *CURRENT_LIST_ITEM_STYLE;
                }
                (item, OFFLINE_ITEM_HEIGHT)
            });
            let list = ListView::new(builder, item_count).infinite_scrolling(false);
            StatefulWidget::render(list, inner, buf, state);
        }
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

        let warning = if crate::i18n::is_es() {
            format!(
                "¿Estás seguro de que quieres eliminar TODOS los mangas del proveedor {} en la sección {}?",
                self.manga_provider, self.tab
            )
        } else {
            format!("Are you sure you want to remove ALL mangas from provider {} in the {} section?", self.manga_provider, self.tab)
        };

        Paragraph::new(warning)
            .style(Style::default().fg(t.text_primary).bold())
            .wrap(Wrap { trim: true })
            .render(warning_area, buf);

        let [note_area, confirm_area, _, cancel_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
                .areas(options_area);

        Paragraph::new(Line::styled(
            tr(
                "Tus archivos descargados en disco se mantendrán, pero se vaciará esta lista.",
                "Your downloaded files on disk will be kept, but this list will be cleared.",
            ),
            Style::default().fg(t.text_muted),
        ))
        .wrap(Wrap { trim: true })
        .render(note_area, buf);

        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(" w ", Style::default().fg(Color::White).bg(t.status_error).bold()),
            Span::styled(tr(" Sí, eliminar todo", " Yes, delete all"), Style::default().fg(t.text_primary).bold()),
        ]))
        .render(confirm_area, buf);

        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(" q / Esc ", Style::default().fg(Color::Black).bg(t.status_success).bold()),
            Span::styled(tr(" No, cancelar", " No, cancel"), Style::default().fg(t.text_primary).bold()),
        ]))
        .render(cancel_area, buf);
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

    pub fn hit_boxes(area: Rect) -> (Rect, Rect, Rect) {
        let modal_rect = crate::utils::centered_rect(area, 62, 42);
        let inner = Block::bordered().border_type(BorderType::Rounded).inner(modal_rect).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        let [_, options_area] = Layout::vertical([Constraint::Percentage(35), Constraint::Percentage(65)]).areas(inner);
        let [_, confirm_area, _, cancel_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
                .areas(options_area);
        (modal_rect, confirm_area, cancel_area)
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
            width: 62,
            height: 42,
        })
        .build();

        modal.render(area, buf);
    }
}
