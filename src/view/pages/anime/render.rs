use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph, Wrap};

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
            Layout::horizontal([Constraint::Fill(2), Constraint::Fill(3)]).spacing(1).areas(content)
        } else {
            Layout::vertical([Constraint::Percentage(43), Constraint::Percentage(57)]).areas(content)
        };
        self.render_results(results, frame);
        self.render_details(details, frame, overlay || self.mirrors.is_some());
        let color = if self.status_is_error { theme().status_warning } else { theme().text_secondary };
        let hint = match self.focus {
            Focus::Query => tr("Enter Buscar", "Enter Search"),
            Focus::RecentEpisodes => tr("Enter Abrir ficha · w Ver ep. en Web", "Enter Open details · w Watch in Web"),
            Focus::Recommendations | Focus::Results => tr("Enter Abrir ficha", "Enter Open details"),
            Focus::Episodes => tr("Enter Elegir servidor · w Ver en Web", "Enter Choose server · w Watch in Web"),
        };
        let message = if self.status.is_empty() { hint } else { self.status.as_str() };
        frame.render_widget(Paragraph::new(message).style(Style::default().fg(color).bg(theme().bar_bg)), status);
        if !overlay {
            self.render_mirrors(area, frame);
        }
    }

    fn render_search(&mut self, area: Rect, frame: &mut Frame<'_>, overlay: bool) {
        let [query, button] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(10)]).spacing(1).areas(area);
        let border = section(tr("Buscar anime", "Search anime"), self.focus == Focus::Query);
        let inner = border.inner(query);
        let width = usize::from(inner.width.saturating_sub(1));
        let scroll = self.input.visual_scroll(width);
        let hint = tr("Título del anime", "Anime title");
        let show_hint = self.input.value().is_empty() && self.focus != Focus::Query;
        let text = if show_hint { hint } else { self.input.value() };
        let color = if show_hint { theme().text_muted } else { theme().text_primary };
        frame.render_widget(
            Paragraph::new(text)
                .scroll((0, scroll as u16))
                .block(border)
                .style(Style::default().fg(color)),
            query,
        );
        self.hitboxes.query = query;
        self.hitboxes.search = button;
        frame.render_widget(
            Paragraph::new(tr("Buscar", "Search"))
                .centered()
                .block(section("", true))
                .style(Style::default().fg(theme().accent_primary).bg(theme().chip_bg).bold()),
            button,
        );
        if self.focus == Focus::Query && self.mirrors.is_none() && !overlay && inner.width > 0 && inner.height > 0 {
            let x = self.input.visual_cursor().saturating_sub(scroll).min(width) as u16;
            frame.set_cursor_position((inner.x + x, inner.y));
        }
    }

    fn render_toolbar(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let compact = area.width < 68;
        let [discover, recent, favorites, refresh, provider] = Layout::horizontal([
            Constraint::Length(if compact { 9 } else { 14 }),
            Constraint::Length(if compact { 8 } else { 14 }),
            Constraint::Length(if compact { 8 } else { 15 }),
            Constraint::Length(if compact { 8 } else { 14 }),
            Constraint::Fill(1),
        ])
        .spacing(1)
        .areas(area);
        for (space, label, active) in [
            (discover, if compact { " d Expl. " } else { tr(" d Explorar ", " d Explore ") }, self.mode == View::Discover),
            (recent, if compact { " h Hist. " } else { tr(" h Historial ", " h History ") }, self.mode == View::Recent),
            (favorites, if compact { " b Fav. " } else { tr(" b Favoritos ", " b Favorites ") }, self.mode == View::Favorites),
            (refresh, if compact { " r Act. " } else { tr(" r Actualizar ", " r Refresh ") }, false),
            (provider, &format!(" [p] {} ", self.provider), false),
        ] {
            let style = if active {
                Style::default().fg(theme().accent_primary).bg(theme().row_selected_bg).bold()
            } else {
                Style::default().fg(theme().text_secondary).bg(theme().bar_bg)
            };
            frame.render_widget(Paragraph::new(label).style(style), space);
        }
        self.hitboxes.discover = discover;
        self.hitboxes.recent = recent;
        self.hitboxes.favorites = favorites;
        self.hitboxes.refresh = refresh;
        self.hitboxes.provider = provider;
    }

    fn render_results(&mut self, area: Rect, frame: &mut Frame<'_>) {
        if self.mode == View::Discover {
            self.render_discover(area, frame);
            return;
        }
        let title = match self.mode {
            View::Discover => tr("Explorar", "Explore"),
            View::Search => tr("Resultados", "Results"),
            View::Recent => tr("Historial reciente", "Recent history"),
            View::Favorites => tr("Favoritos", "Favorites"),
        };
        let pagination = self.mode == View::Search && self.total_pages > 1 && area.height >= 4;
        let [list_area, pages] = Layout::vertical([Constraint::Fill(1), Constraint::Length(u16::from(pagination))]).areas(area);
        let items: Vec<ListItem<'_>> = self.results.iter().map(|anime| ListItem::new(single_line(&anime.title))).collect();
        let title = if self.results.is_empty() { title.to_owned() } else { format!("{title} · {}", self.results.len()) };
        self.hitboxes.results = draw_list(frame, list_area, &title, self.focus == Focus::Results, items, &mut self.results_list);
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

    fn render_discover(&mut self, area: Rect, frame: &mut Frame<'_>) {
        let [recent_area, rec_area] = Layout::vertical([Constraint::Percentage(54), Constraint::Percentage(46)]).areas(area);

        let ep_items: Vec<ListItem<'_>> = self
            .recent_episodes
            .iter()
            .map(|ep| {
                let fav = if self.favorite_slugs.contains(&ep.slug) { "★ " } else { "" };
                let mut spans = vec![
                    Span::styled(
                        format!("{fav}Ep. {} ", single_line(&ep.episode_number)),
                        Style::default().fg(theme().accent_highlight).bold(),
                    ),
                    Span::raw(single_line(&ep.anime_title)),
                ];
                if let Some(aired) = ep.aired_label.as_deref() {
                    spans.push(Span::styled(format!(" · {}", single_line(aired)), Style::default().fg(theme().text_muted)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let ep_title = if self.recent_episodes.is_empty() {
            tr("🔥 En emisión · Capítulos recientes", "🔥 Airing · Recent episodes").to_owned()
        } else {
            format!("{} · {}", tr("🔥 En emisión · Capítulos", "🔥 Airing · Episodes"), self.recent_episodes.len())
        };
        self.hitboxes.recent_episodes =
            draw_list(frame, recent_area, &ep_title, self.focus == Focus::RecentEpisodes, ep_items, &mut self.recent_episodes_list);

        let rec_items: Vec<ListItem<'_>> = self
            .recommendations
            .iter()
            .map(|anime| {
                let (icon, color) = if self.favorite_slugs.contains(&anime.slug) {
                    ("★", theme().accent_highlight)
                } else if anime.status == anime_core::AnimeStatus::Ongoing {
                    ("●", theme().status_success)
                } else {
                    ("◆", theme().accent_secondary)
                };
                let mut spans =
                    vec![Span::styled(format!("{icon} "), Style::default().fg(color)), Span::raw(single_line(&anime.title))];
                let tag = match anime.status {
                    anime_core::AnimeStatus::Ongoing => Some(tr("En emisión", "Airing")),
                    anime_core::AnimeStatus::Upcoming => Some(tr("Estreno", "Upcoming")),
                    _ => None,
                };
                if let Some(tag) = tag {
                    spans.push(Span::styled(format!(" · {tag}"), Style::default().fg(theme().text_muted)));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let rec_title = if self.recommendations.is_empty() {
            tr("✨ Recomendaciones", "✨ Recommendations").to_owned()
        } else {
            format!("{} · {}", tr("✨ Recomendaciones", "✨ Recommendations"), self.recommendations.len())
        };
        self.hitboxes.recommendations =
            draw_list(frame, rec_area, &rec_title, self.focus == Focus::Recommendations, rec_items, &mut self.recommendations_list);
    }

    fn render_details(&mut self, area: Rect, frame: &mut Frame<'_>, overlay: bool) {
        let Some(details) = self.details.as_ref() else {
            self.render_empty_details(area, frame);
            return;
        };

        let summary_height = if area.height >= 9 { (area.height / 2).clamp(4, 13) } else { 0 };
        let [summary, episodes] = Layout::vertical([Constraint::Length(summary_height), Constraint::Fill(1)]).areas(area);
        if summary_height > 0 {
            let block = section(tr("Ficha", "Details"), false).padding(Padding::horizontal(1));
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
                let (icon, color) = if entry.is_some_and(|entry| entry.is_watched) {
                    ("✓", theme().status_success)
                } else if entry.is_some_and(|entry| entry.progress_seconds > 0) {
                    ("◐", theme().accent_secondary)
                } else {
                    ("·", theme().text_muted)
                };
                let progress = entry
                    .filter(|entry| entry.progress_seconds > 0)
                    .map(|entry| format!("  {} / {}", timestamp(entry.progress_seconds), timestamp(entry.duration_seconds)))
                    .unwrap_or_default();
                let title = episode
                    .title
                    .as_deref()
                    .map(|title| format!(" · {}", single_line(title)))
                    .unwrap_or_default();
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::raw(format!("Ep. {}", single_line(&episode.number))),
                    Span::styled(progress, Style::default().fg(theme().text_secondary)),
                    Span::raw(title),
                ]))
            })
            .collect();
        let title = format!("{} ({})", tr("Episodios", "Episodes"), details.episodes.len());
        self.hitboxes.episodes =
            draw_list(frame, episodes, &title, self.focus == Focus::Episodes, entries, &mut self.episodes_list);
    }

    fn render_empty_details(&self, area: Rect, frame: &mut Frame<'_>) {
        if self.mode == View::Discover {
            let preview_lines = if self.focus == Focus::Recommendations
                && let Some(anime) = self.recommendations_list.selected().and_then(|i| self.recommendations.get(i))
            {
                let status = match anime.status {
                    anime_core::AnimeStatus::Ongoing => tr("En emisión", "Airing"),
                    anime_core::AnimeStatus::Completed => tr("Finalizado", "Completed"),
                    anime_core::AnimeStatus::Upcoming => tr("Próximamente", "Upcoming"),
                    anime_core::AnimeStatus::Unknown => tr("Recomendado", "Recommended"),
                };
                let synopsis = anime
                    .synopsis
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .map(single_line)
                    .unwrap_or_else(|| {
                        tr("Pulsa Enter para cargar la ficha y todos sus episodios.", "Press Enter to load details and episodes.")
                            .to_owned()
                    });
                vec![
                    Line::styled(anime.title.clone(), Style::default().fg(theme().accent_primary).bold()),
                    Line::styled(format!("✨ {status} · {}", anime.provider), Style::default().fg(theme().accent_secondary)),
                    Line::raw(""),
                    Line::styled(synopsis, Style::default().fg(theme().text_primary)),
                    Line::raw(""),
                    Line::styled(
                        tr(
                            "Enter Abrir ficha y episodios · Tab Cambiar sección · / Buscar",
                            "Enter Open details · Tab Switch section · / Search",
                        ),
                        Style::default().fg(theme().text_muted),
                    ),
                ]
            } else if let Some(ep) = self.recent_episodes_list.selected().and_then(|i| self.recent_episodes.get(i)) {
                let mut meta = vec![format!("Ep. {}", ep.episode_number), ep.provider.to_string()];
                if let Some(aired) = ep.aired_label.as_deref() {
                    meta.push(aired.to_owned());
                }
                vec![
                    Line::styled(ep.anime_title.clone(), Style::default().fg(theme().accent_primary).bold()),
                    Line::styled(
                        format!("🔥 {} · {}", tr("En emisión", "Airing"), meta.join(" · ")),
                        Style::default().fg(theme().accent_highlight),
                    ),
                    Line::raw(""),
                    Line::styled(
                        tr(
                            "Nuevo capítulo disponible. Puedes abrir la ficha con ese episodio preseleccionado o lanzarlo directo en el reproductor web.",
                            "New episode available. Open its details pre-selected or launch it straight in the web player.",
                        ),
                        Style::default().fg(theme().text_primary),
                    ),
                    Line::raw(""),
                    Line::styled(
                        tr(
                            "Enter Abrir ficha · w Ver directo en Web · Tab Recomendaciones",
                            "Enter Open details · w Watch in Web · Tab Recommendations",
                        ),
                        Style::default().fg(theme().text_muted),
                    ),
                ]
            } else {
                vec![Line::styled(
                    tr("Cargando episodios en emisión y recomendaciones…", "Loading airing episodes & recommendations…"),
                    Style::default().fg(theme().text_muted),
                )]
            };
            frame.render_widget(
                Paragraph::new(preview_lines)
                    .wrap(Wrap { trim: true })
                    .block(section(tr("Vista previa", "Preview"), false).padding(Padding::horizontal(1))),
                area,
            );
            return;
        }
        frame.render_widget(
            Paragraph::new(tr("Selecciona un resultado para ver sus episodios.", "Select a result to see its episodes."))
                .style(Style::default().fg(theme().text_muted))
                .wrap(Wrap { trim: true })
                .block(section(tr("Ficha", "Details"), false)),
            area,
        );
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
    let label = if title.is_empty() { String::new() } else { format!(" {title} ") };
    theme()
        .rounded_block()
        .title(Span::styled(
            label,
            Style::default()
                .fg(if selected { theme().accent_primary } else { theme().text_secondary })
                .bold(),
        ))
        .border_style(Style::default().fg(border))
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
