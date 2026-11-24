use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap};

use super::{AnimePage, Focus, Hitboxes, View};
use crate::common::render_stateful_image_cleared;
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::centered_rect;

impl AnimePage {
    pub(crate) fn render_page(&mut self, area: Rect, frame: &mut Frame<'_>, overlay: bool) {
        self.hitboxes = Hitboxes::default();
        if area.width < 24 || area.height < 8 {
            return;
        }
        let [header, toolbar, content, status] =
            Layout::vertical([Constraint::Length(3), Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)])
                .areas(area);
        self.render_search(header, frame, overlay);
        self.render_toolbar(toolbar, frame);
        let [results, details] = if content.width >= 72 {
            Layout::horizontal([Constraint::Percentage(39), Constraint::Percentage(61)]).areas(content)
        } else {
            Layout::vertical([Constraint::Percentage(43), Constraint::Percentage(57)]).areas(content)
        };
        self.render_results(results, frame);
        self.render_details(details, frame, overlay || self.mirrors.is_some());
        let color = if self.status_is_error { theme().status_warning } else { theme().text_secondary };
        frame.render_widget(Paragraph::new(self.status.as_str()).style(Style::default().fg(color)), status);
        if !overlay {
            self.render_mirrors(area, frame);
        }
    }

    fn render_search(&mut self, area: Rect, frame: &mut Frame<'_>, overlay: bool) {
        let [query, button] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(12)]).areas(area);
        let border = section(tr("Buscar anime", "Search anime"), self.focus == Focus::Query);
        let inner = border.inner(query);
        let width = usize::from(inner.width.saturating_sub(1));
        let scroll = self.input.visual_scroll(width);
        let hint = tr("Título del anime", "Anime title");
        let text = if self.input.value().is_empty() && self.focus != Focus::Query { hint } else { self.input.value() };
        frame.render_widget(
            Paragraph::new(text)
                .scroll((0, scroll as u16))
                .block(border)
                .style(Style::default().fg(theme().text_primary)),
            query,
        );
        self.hitboxes.query = query;
        self.hitboxes.search = button;
        frame.render_widget(
            Paragraph::new(tr("Buscar", "Search"))
                .centered()
                .block(section("", false))
                .style(Style::default().fg(theme().accent_primary)),
            button,
        );
        if self.focus == Focus::Query && self.mirrors.is_none() && !overlay && inner.width > 0 && inner.height > 0 {
            let x = self.input.visual_cursor().saturating_sub(scroll).min(width) as u16;
            frame.set_cursor_position((inner.x + x, inner.y));
        }
    }

    fn render_toolbar(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let [recent, favorites, provider] =
            Layout::horizontal([Constraint::Percentage(32), Constraint::Percentage(34), Constraint::Percentage(34)]).areas(area);
        for (space, label, active) in [
            (recent, tr(" [h] Historial ", " [h] History "), self.mode == View::Recent),
            (favorites, tr(" [b] Favoritos ", " [b] Favorites "), self.mode == View::Favorites),
            (provider, &format!(" [p] {} ", self.provider), false),
        ] {
            let style = if active {
                Style::default().fg(theme().accent_primary).bg(theme().row_selected_bg).bold()
            } else {
                Style::default().fg(theme().text_secondary)
            };
            frame.render_widget(Paragraph::new(label).style(style), space);
        }
        self.hitboxes.recent = recent;
        self.hitboxes.favorites = favorites;
        self.hitboxes.provider = provider;
    }

    fn render_results(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let title = match self.mode {
            View::Search => tr("Resultados", "Results"),
            View::Recent => tr("Historial reciente", "Recent history"),
            View::Favorites => tr("Favoritos", "Favorites"),
        };
        let pagination = self.mode == View::Search && self.total_pages > 1 && area.height >= 4;
        let [list_area, pages] = Layout::vertical([Constraint::Fill(1), Constraint::Length(u16::from(pagination))]).areas(area);
        let items: Vec<ListItem<'_>> = self.results.iter().map(|anime| ListItem::new(single_line(&anime.title))).collect();
        self.hitboxes.results = draw_list(frame, list_area, title, self.focus == Focus::Results, items, &mut self.results_list);
        if pagination {
            let [prev, label, next] =
                Layout::horizontal([Constraint::Length(4), Constraint::Fill(1), Constraint::Length(4)]).areas(pages);
            frame.render_widget(Paragraph::new(format!("{} / {}", self.page, self.total_pages)).centered(), label);
            if self.page > 1 {
                frame.render_widget(Paragraph::new(" ‹ ").style(Style::default().fg(theme().accent_primary)), prev);
                self.hitboxes.previous = prev;
            }
            if self.page < self.total_pages {
                frame.render_widget(Paragraph::new(" › ").style(Style::default().fg(theme().accent_primary)), next);
                self.hitboxes.next = next;
            }
        }
    }

    fn render_details(&mut self, area: Rect, frame: &mut Frame<'_>, overlay: bool) {
        let Some(details) = self.details.as_ref() else {
            frame.render_widget(
                Paragraph::new(tr("Selecciona un resultado para ver sus episodios.", "Select a result to see its episodes."))
                    .style(Style::default().fg(theme().text_muted))
                    .wrap(Wrap { trim: true })
                    .block(section(tr("Ficha", "Details"), false)),
                area,
            );
            return;
        };

        let summary_height = if area.height >= 9 { (area.height / 2).clamp(4, 13) } else { 0 };
        let [summary, episodes] = Layout::vertical([Constraint::Length(summary_height), Constraint::Fill(1)]).areas(area);
        if summary_height > 0 {
            let block = section(tr("Ficha", "Details"), false);
            let inner = block.inner(summary);
            frame.render_widget(block, summary);
            let text_area = if !overlay
                && inner.width >= 35
                && inner.height >= 4
                && let Some(protocol) = self.cover_protocol.as_mut()
            {
                let width = (inner.width / 4).clamp(8, 18);
                let [image, _, text] =
                    Layout::horizontal([Constraint::Length(width), Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
                let image = Rect {
                    height: image.height.saturating_sub(1),
                    ..image
                };
                render_stateful_image_cleared(protocol, image, image, frame.buffer_mut(), theme().row_selected_bg);
                text
            } else {
                inner
            };
            let status = match details.anime.status {
                anime_core::AnimeStatus::Ongoing => tr("En emisión", "Airing"),
                anime_core::AnimeStatus::Completed => tr("Finalizado", "Completed"),
                anime_core::AnimeStatus::Upcoming => tr("Próximamente", "Upcoming"),
                anime_core::AnimeStatus::Unknown => tr("Estado desconocido", "Unknown status"),
            };
            let mut metadata = vec![status.to_owned()];
            if let Some(score) = details.anime.score {
                metadata.push(format!("{score:.1}/10"));
            }
            if !details.anime.genres.is_empty() {
                metadata.push(details.anime.genres.join(", "));
            }
            let title = if details.favorite { format!("★ {}", details.anime.title) } else { details.anime.title.clone() };
            let lines = vec![
                Line::styled(title, Style::default().fg(theme().accent_primary).bold()),
                Line::styled(metadata.join(" · "), Style::default().fg(theme().text_secondary)),
                Line::from(single_line(&details.anime.synopsis)),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(theme().text_primary)),
                text_area,
            );
        }

        let history: std::collections::HashMap<_, _> =
            details.history.iter().map(|entry| (entry.episode_number.as_str(), entry)).collect();
        let entries = details
            .episodes
            .iter()
            .map(|episode| {
                let entry = history.get(episode.number.as_str());
                let icon = if entry.is_some_and(|entry| entry.is_watched) { "✓" } else { "·" };
                let progress = entry
                    .filter(|entry| entry.progress_seconds > 0)
                    .map(|entry| format!("  {} / {}", timestamp(entry.progress_seconds), timestamp(entry.duration_seconds)))
                    .unwrap_or_default();
                let title = episode
                    .title
                    .as_deref()
                    .map(|title| format!(" · {}", single_line(title)))
                    .unwrap_or_default();
                ListItem::new(format!("{icon} Ep. {}{progress}{title}", single_line(&episode.number)))
            })
            .collect();
        let title = format!("{} ({})", tr("Episodios", "Episodes"), details.episodes.len());
        self.hitboxes.episodes =
            draw_list(frame, episodes, &title, self.focus == Focus::Episodes, entries, &mut self.episodes_list);
    }

    fn render_mirrors(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let Some(menu) = self.mirrors.as_mut() else { return };
        let popup = centered_rect(area, 66, 66);
        frame.render_widget(Clear, popup);
        let items = menu
            .mirrors
            .iter()
            .map(|mirror| ListItem::new(format!("{} · {}", single_line(&mirror.server_name), mirror.variant)))
            .collect();
        self.hitboxes.mirrors = draw_list(
            frame,
            popup,
            tr("Servidor · Enter abre · Esc cancela", "Mirror · Enter opens · Esc cancels"),
            true,
            items,
            &mut menu.list,
        );
    }
}

fn section(title: &str, selected: bool) -> Block<'_> {
    let border = if selected { theme().border_accent } else { theme().border_muted };
    Block::bordered().title(title).border_style(Style::default().fg(border))
}

fn draw_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    focused: bool,
    items: Vec<ListItem<'_>>,
    state: &mut ListState,
) -> Rect {
    let block = section(title, focused);
    let inner = block.inner(area);
    let highlight = if focused {
        Style::default()
            .fg(theme().accent_primary)
            .bg(theme().row_selected_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme().text_primary).bg(theme().row_selected_bg)
    };
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .style(Style::default().fg(theme().text_primary))
            .highlight_symbol("› ")
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_style(highlight),
        area,
        state,
    );
    inner
}

fn timestamp(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
