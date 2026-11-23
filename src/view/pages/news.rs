//! 📰 News page: manga & anime headlines aggregated from public RSS feeds.

use std::collections::HashMap;

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::backend::news::{NewsItem, NewsSource, fetch_source, sort_newest_first};
use crate::backend::tui::Events;
use crate::i18n::tr;
use crate::theme::theme;

#[derive(Debug, Clone, PartialEq)]
enum SourceStatus {
    Loading,
    Loaded(usize),
    Failed(String),
}

type FetchResult = (NewsSource, Result<Vec<NewsItem>, String>);

pub struct NewsPage {
    items: Vec<NewsItem>,
    statuses: HashMap<NewsSource, SourceStatus>,
    /// `None` = all sources
    filter: Option<NewsSource>,
    selected: usize,
    scroll_offset: usize,
    list_area: Rect,
    result_tx: UnboundedSender<FetchResult>,
    result_rx: UnboundedReceiver<FetchResult>,
    has_loaded: bool,
}

impl Default for NewsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl NewsPage {
    pub fn new() -> Self {
        let (result_tx, result_rx) = unbounded_channel();
        Self {
            items: Vec::new(),
            statuses: HashMap::new(),
            filter: None,
            selected: 0,
            scroll_offset: 0,
            list_area: Rect::default(),
            result_tx,
            result_rx,
            has_loaded: false,
        }
    }

    /// Fetches the feeds the first time the page is opened.
    pub fn ensure_loaded(&mut self) {
        if !self.has_loaded {
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        self.has_loaded = true;
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        for source in NewsSource::ALL {
            self.statuses.insert(source, SourceStatus::Loading);
            let tx = self.result_tx.clone();
            handle.spawn(async move {
                let result = fetch_source(source).await;
                tx.send((source, result)).ok();
            });
        }
    }

    fn receive_results(&mut self) {
        while let Ok((source, result)) = self.result_rx.try_recv() {
            self.apply_result(source, result);
        }
    }

    fn apply_result(&mut self, source: NewsSource, result: Result<Vec<NewsItem>, String>) {
        let selected_link = self.visible().get(self.selected).map(|i| i.link.clone());
        match result {
            Ok(new_items) => {
                self.items.retain(|i| i.source != source);
                self.statuses.insert(source, SourceStatus::Loaded(new_items.len()));
                self.items.extend(new_items);
                sort_newest_first(&mut self.items);
            },
            Err(e) => {
                self.statuses.insert(source, SourceStatus::Failed(e));
            },
        }
        // Keep the cursor on the same article while other feeds stream in
        if let Some(link) = selected_link
            && let Some(pos) = self.visible().iter().position(|i| i.link == link)
        {
            self.selected = pos;
        }
        self.clamp_selection();
    }

    fn visible(&self) -> Vec<&NewsItem> {
        self.items.iter().filter(|i| self.filter.is_none_or(|f| i.source == f)).collect()
    }

    fn clamp_selection(&mut self) {
        let len = self.visible().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn cycle_filter(&mut self, forward: bool) {
        let options: Vec<Option<NewsSource>> = std::iter::once(None).chain(NewsSource::ALL.into_iter().map(Some)).collect();
        let idx = options.iter().position(|o| *o == self.filter).unwrap_or(0);
        let next = if forward { (idx + 1) % options.len() } else { (idx + options.len() - 1) % options.len() };
        self.set_filter(options[next]);
    }

    fn set_filter(&mut self, filter: Option<NewsSource>) {
        self.filter = filter;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
    }

    fn open_selected(&self) {
        if let Some(item) = self.visible().get(self.selected)
            && !item.link.is_empty()
        {
            let _ = open::that(&item.link);
        }
    }

    pub fn handle_events(&mut self, events: Events) {
        self.receive_results();
        match events {
            Events::Key(key) => self.handle_key(key),
            Events::Mouse(mouse) => self.handle_mouse(mouse),
            _ => {},
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.move_selection(isize::MAX / 2),
            KeyCode::Enter | KeyCode::Char('r') => self.open_selected(),
            KeyCode::Char('R') => self.refresh(),
            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => self.cycle_filter(true),
            KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => self.cycle_filter(false),
            KeyCode::Char('1') => self.set_filter(None),
            KeyCode::Char('2') => self.set_filter(Some(NewsSource::RamenParaDos)),
            KeyCode::Char('3') => self.set_filter(Some(NewsSource::AnimeNewsNetwork)),
            KeyCode::Char('4') => self.set_filter(Some(NewsSource::MyAnimeList)),
            _ => {},
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollDown => self.move_selection(1),
            MouseEventKind::ScrollUp => self.move_selection(-1),
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let a = self.list_area;
                if mouse.column >= a.x && mouse.column < a.right() && mouse.row >= a.y && mouse.row < a.bottom() {
                    let idx = self.scroll_offset + ((mouse.row - a.y) / ROW_HEIGHT) as usize;
                    if idx < self.visible().len() {
                        if idx == self.selected {
                            self.open_selected();
                        } else {
                            self.selected = idx;
                        }
                    }
                }
            },
            _ => {},
        }
    }

    pub fn render(&mut self, area: Rect, frame: &mut Frame<'_>) {
        self.receive_results();
        let buf = frame.buffer_mut();
        if area.height < 6 || area.width < 30 {
            return;
        }
        let [header, body] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);
        self.render_header(header, buf);

        let [list_area, preview_area] = if body.width >= 90 {
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(body)
        } else {
            Layout::vertical([Constraint::Fill(3), Constraint::Fill(2)]).areas(body)
        };
        self.render_list(list_area, buf);
        self.render_preview(preview_area, buf);
    }

    fn source_color(source: NewsSource) -> Color {
        let t = theme();
        match source {
            NewsSource::RamenParaDos => t.accent_highlight,
            NewsSource::AnimeNewsNetwork => t.accent_primary,
            NewsSource::MyAnimeList => t.accent_secondary,
        }
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let mut spans = vec![
            Span::styled(" 📰 ", Style::default().fg(t.accent_primary)),
            Span::styled(tr("Noticias Manga & Anime ", "Manga & Anime News "), Style::default().fg(t.text_primary).bold()),
            Span::raw("  "),
        ];
        let mut options: Vec<(Option<NewsSource>, String, Color)> =
            vec![(None, format!("1 {}", tr("Todas", "All")), t.accent_primary)];
        for (i, s) in NewsSource::ALL.iter().enumerate() {
            options.push((Some(*s), format!("{} {} {}", i + 2, s.short(), s.language()), Self::source_color(*s)));
        }
        for (filter, label, color) in options {
            let active = filter == self.filter;
            let style =
                if active { Style::default().fg(Color::Black).bg(color).bold() } else { Style::default().fg(color).bg(t.chip_bg) };
            spans.push(Span::styled("", Style::default().fg(if active { color } else { t.chip_bg })));
            spans.push(Span::styled(format!(" {label} "), style));
            spans.push(Span::styled("", Style::default().fg(if active { color } else { t.chip_bg })));
            spans.push(Span::raw(" "));
        }
        Paragraph::new(Line::from(spans)).render(Rect { height: 1, ..area }, buf);

        // Second line: feed status
        let mut status = vec![Span::raw(" ")];
        for s in NewsSource::ALL {
            let (icon, color, text) = match self.statuses.get(&s) {
                None | Some(SourceStatus::Loading) => ("◌", t.text_muted, tr("cargando…", "loading…").to_string()),
                Some(SourceStatus::Loaded(n)) => ("●", t.status_success, format!("{n}")),
                Some(SourceStatus::Failed(_)) => ("✕", t.status_error, tr("sin conexión", "offline").to_string()),
            };
            status.push(Span::styled(format!("{icon} "), Style::default().fg(color)));
            status.push(Span::styled(format!("{} {text}   ", s.short()), Style::default().fg(t.text_muted)));
        }
        if area.height >= 2 {
            Paragraph::new(Line::from(status)).render(
                Rect {
                    y: area.y + 1,
                    height: 1,
                    ..area
                },
                buf,
            );
        }
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent))
            .title(Line::from(vec![
                Span::styled(" ", Style::default().fg(t.title_chip_primary_bg)),
                Span::styled(
                    format!("🔥 {} ", tr("Últimas noticias", "Latest news")),
                    Style::default().fg(Color::Black).bg(t.title_chip_primary_bg).bold(),
                ),
                Span::styled(" ", Style::default().fg(t.title_chip_primary_bg)),
            ]));
        let inner = block.inner(area);
        block.render(area, buf);
        let inner = inner.inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        self.list_area = inner;

        let selected = self.selected;
        let visible_rows = (inner.height / ROW_HEIGHT).max(1) as usize;
        if selected < self.scroll_offset {
            self.scroll_offset = selected;
        } else if selected >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected + 1 - visible_rows;
        }
        let scroll_offset = self.scroll_offset;

        let items = self.visible();
        if items.is_empty() {
            let loading = self.statuses.values().any(|s| *s == SourceStatus::Loading);
            let msg = if loading {
                tr("⏳ Cargando noticias…", "⏳ Loading news…")
            } else {
                tr("No hay noticias. Pulsa <R> para reintentar.", "No news. Press <R> to retry.")
            };
            Paragraph::new(Line::styled(msg, Style::default().fg(t.text_muted))).render(inner, buf);
            return;
        }

        let now = Utc::now();
        for (row, (idx, item)) in items.iter().enumerate().skip(scroll_offset).take(visible_rows).enumerate() {
            let y = inner.y + row as u16 * ROW_HEIGHT;
            let is_sel = idx == selected;
            let row_area = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: ROW_HEIGHT.min(inner.bottom().saturating_sub(y)),
            };
            if is_sel {
                for yy in row_area.y..row_area.bottom().min(row_area.y + 2) {
                    for xx in row_area.left()..row_area.right() {
                        buf[(xx, yy)].set_bg(t.row_selected_bg);
                    }
                }
            }
            let color = Self::source_color(item.source);
            let marker = if is_sel { "▌" } else { " " };
            let title_style = if is_sel {
                Style::default().fg(t.text_primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.text_secondary)
            };
            let badge = format!(" {} ", item.source.short());
            let avail = (row_area.width as usize).saturating_sub(badge.chars().count() + 3);
            let title_line = Line::from(vec![
                Span::styled(marker, Style::default().fg(color)),
                Span::styled(badge, Style::default().fg(Color::Black).bg(color).bold()),
                Span::raw(" "),
                Span::styled(truncate(&item.title, avail), title_style),
            ]);
            Paragraph::new(title_line).render(
                Rect {
                    height: 1,
                    ..row_area
                },
                buf,
            );
            if row_area.height >= 2 {
                let ago = item.published.map(|d| time_ago(now.signed_duration_since(d))).unwrap_or_default();
                let meta = Line::from(vec![
                    Span::styled(marker, Style::default().fg(color)),
                    Span::styled(
                        format!("     🕒 {ago}  ·  {}  ·  {}", item.source.name(), item.source.language()),
                        Style::default().fg(t.text_muted),
                    ),
                ]);
                Paragraph::new(meta).render(
                    Rect {
                        y: y + 1,
                        height: 1,
                        ..row_area
                    },
                    buf,
                );
            }
        }
    }

    fn render_preview(&self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_secondary))
            .title(Line::from(vec![
                Span::styled(" ", Style::default().fg(t.title_chip_secondary_bg)),
                Span::styled(
                    format!("📖 {} ", tr("Resumen", "Summary")),
                    Style::default().fg(Color::Black).bg(t.title_chip_secondary_bg).bold(),
                ),
                Span::styled(" ", Style::default().fg(t.title_chip_secondary_bg)),
            ]));
        let inner = block.inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        block.render(area, buf);

        let items = self.visible();
        let Some(item) = items.get(self.selected) else {
            return;
        };
        let color = Self::source_color(item.source);
        let date = item
            .published
            .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
            .unwrap_or_default();
        let summary = if item.summary.is_empty() {
            tr("(Sin resumen disponible)", "(No summary available)").to_string()
        } else {
            truncate(&item.summary, 900)
        };
        let lines = vec![
            Line::styled(item.title.clone(), Style::default().fg(t.text_primary).bold()),
            Line::raw(""),
            Line::from(vec![
                Span::styled(format!(" {} ", item.source.name()), Style::default().fg(Color::Black).bg(color).bold()),
                Span::styled(format!("  {}  ·  {date}", item.source.language()), Style::default().fg(t.text_muted)),
            ]),
            Line::raw(""),
            Line::styled(summary, Style::default().fg(t.text_secondary)),
            Line::raw(""),
            Line::from(vec![
                Span::styled(" Enter ", Style::default().fg(Color::Black).bg(t.accent_primary).bold()),
                Span::styled(
                    tr(" Leer artículo completo en el navegador", " Read full article in browser"),
                    Style::default().fg(t.accent_primary),
                ),
            ]),
            Line::styled(truncate(&item.link, inner.width as usize), Style::default().fg(t.text_dim).italic()),
        ];
        Paragraph::new(lines).wrap(Wrap { trim: true }).render(inner, buf);
    }
}

const ROW_HEIGHT: u16 = 3;

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn time_ago(d: chrono::Duration) -> String {
    let mins = d.num_minutes().max(0);
    let es = crate::i18n::is_es();
    match mins {
        0 => tr("ahora", "now").to_string(),
        1..=59 => {
            if es {
                format!("hace {mins} min")
            } else {
                format!("{mins} min ago")
            }
        },
        60..=1439 => {
            let h = mins / 60;
            if es { format!("hace {h} h") } else { format!("{h}h ago") }
        },
        _ => {
            let days = mins / 1440;
            if es { format!("hace {days} d") } else { format!("{days}d ago") }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(source: NewsSource, link: &str, hours_ago: i64) -> NewsItem {
        NewsItem {
            source,
            title: format!("t {link}"),
            link: link.to_string(),
            summary: String::new(),
            published: Some((Utc::now() - chrono::Duration::hours(hours_ago)).fixed_offset()),
        }
    }

    #[test]
    fn merges_sources_sorted_and_filters() {
        let mut page = NewsPage::new();
        page.apply_result(NewsSource::AnimeNewsNetwork, Ok(vec![item(NewsSource::AnimeNewsNetwork, "a", 5)]));
        page.apply_result(NewsSource::RamenParaDos, Ok(vec![item(NewsSource::RamenParaDos, "b", 1)]));
        assert_eq!(page.visible()[0].link, "b");
        assert_eq!(page.visible().len(), 2);

        page.set_filter(Some(NewsSource::AnimeNewsNetwork));
        assert_eq!(page.visible().len(), 1);
        page.cycle_filter(true);
        assert_eq!(page.filter, Some(NewsSource::MyAnimeList));
        page.cycle_filter(true);
        assert_eq!(page.filter, None);
    }

    #[test]
    fn keeps_selection_when_new_feed_arrives() {
        let mut page = NewsPage::new();
        page.apply_result(NewsSource::AnimeNewsNetwork, Ok(vec![item(NewsSource::AnimeNewsNetwork, "a", 5)]));
        assert_eq!(page.selected, 0);
        page.apply_result(NewsSource::RamenParaDos, Ok(vec![item(NewsSource::RamenParaDos, "b", 1)]));
        assert_eq!(page.visible()[page.selected].link, "a");
    }

    #[test]
    fn failed_source_is_reported() {
        let mut page = NewsPage::new();
        page.apply_result(NewsSource::MyAnimeList, Err("x".into()));
        assert!(matches!(page.statuses.get(&NewsSource::MyAnimeList), Some(SourceStatus::Failed(_))));
    }
}
