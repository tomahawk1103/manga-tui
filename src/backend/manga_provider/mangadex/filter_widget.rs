use filter_provider::{FILTERS, FilterListItem, ListItemId, MangaFilters, MangadexFilterProvider, TagListItem, TagListItemState};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{HighlightSpacing, List, ListItem, ListState, Paragraph, StatefulWidget, Tabs, Widget, Wrap};

use super::StatefulWidgetFrame;
use super::filters::*;
use crate::backend::manga_provider::FiltersWidget;
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::render_search_bar;

#[derive(Clone)]
pub struct MangadexFilterWidget {
    pub _style: Style,
}

impl FiltersWidget for MangadexFilterWidget {
    type FilterState = MangadexFilterProvider;
}

impl From<MangaFilters> for Line<'_> {
    fn from(value: MangaFilters) -> Self {
        Line::from(value.to_string())
    }
}

impl From<FilterListItem> for ListItem<'_> {
    fn from(value: FilterListItem) -> Self {
        let t = theme();
        let line = if value.is_selected {
            Line::from(vec![
                Span::styled(" ✓ ", Style::default().fg(t.status_success).bold()),
                Span::styled(value.name, Style::default().fg(t.accent_highlight).bold()),
            ])
        } else {
            Line::from(vec![Span::raw("   "), Span::styled(value.name, Style::default().fg(t.text_secondary))])
        };
        ListItem::new(line)
    }
}

impl From<ListItemId> for ListItem<'_> {
    fn from(value: ListItemId) -> Self {
        let t = theme();
        let line = if value.is_selected {
            Line::from(vec![
                Span::styled(" ✓ ", Style::default().fg(t.status_success).bold()),
                Span::styled(value.name, Style::default().fg(t.accent_highlight).bold()),
            ])
        } else {
            Line::from(vec![Span::raw("   "), Span::styled(value.name, Style::default().fg(t.text_secondary))])
        };
        ListItem::new(line)
    }
}

impl From<TagListItem> for ListItem<'_> {
    fn from(value: TagListItem) -> Self {
        let t = theme();
        let line = match value.state {
            TagListItemState::Included => Line::from(Span::styled(
                format!(" + {} ", value.name),
                Style::default().fg(Color::Black).bg(t.status_success).bold(),
            )),
            TagListItemState::Excluded => {
                Line::from(Span::styled(format!(" - {} ", value.name), Style::default().fg(Color::White).bg(t.status_error).bold()))
            },
            TagListItemState::NotSelected => {
                Line::from(Span::styled(format!("   {}", value.name), Style::default().fg(t.text_secondary)))
            },
        };

        ListItem::new(line)
    }
}

impl StatefulWidgetFrame for MangadexFilterWidget {
    type State = MangadexFilterProvider;

    fn render(&mut self, area: Rect, frame: &mut Frame<'_>, state: &mut Self::State) {
        let t = theme();
        let buf = frame.buffer_mut();
        let [tabs_area, current_filter_area] = Layout::vertical([Constraint::Length(3), Constraint::Min(4)]).margin(2).areas(area);

        let tabs: Vec<Line<'_>> = FILTERS
            .iter()
            .map(|filters| {
                let num_filters = match filters {
                    MangaFilters::ContentRating => state.content_rating.num_filters_active(),
                    MangaFilters::SortBy => state.sort_by_state.num_filters_active(),
                    MangaFilters::Languages => state.lang_state.num_filters_active(),
                    MangaFilters::PublicationStatus => state.publication_status.num_filters_active(),
                    MangaFilters::MagazineDemographic => state.magazine_demographic.num_filters_active(),
                    MangaFilters::Tags => state.tags_state.num_filters_active(),
                    MangaFilters::Authors => state.author_state.num_filters_active(),
                    MangaFilters::Artists => state.artist_state.num_filters_active(),
                };

                let mut spans = vec![Span::raw(format!(" {} ", filters))];
                if num_filters != 0 {
                    spans.push(Span::styled(format!("({num_filters}) "), Style::default().fg(t.accent_highlight).bold()));
                }
                Line::from(spans)
            })
            .collect();

        let tabs_block = t.rounded_block().title(Span::styled(
            tr(" Categorías (<Tab> / <Shift+Tab>) ", " Categories (<Tab> / <Shift+Tab>) "),
            Style::default().fg(t.accent_primary).bold(),
        ));

        Tabs::new(tabs)
            .block(tabs_block)
            .select(state.id_filter)
            .style(Style::default().fg(t.text_muted))
            .highlight_style(Style::default().fg(Color::Black).bg(t.accent_primary).bold())
            .divider(Span::styled("│", Style::default().fg(t.border_muted)))
            .render(tabs_area, buf);

        if let Some(filter) = FILTERS.get(state.id_filter) {
            match filter {
                MangaFilters::PublicationStatus => {
                    render_filter_list(
                        state.publication_status.items.clone(),
                        current_filter_area,
                        buf,
                        &mut state.publication_status.state,
                    );
                },
                MangaFilters::ContentRating => {
                    render_filter_list(
                        state.content_rating.items.clone(),
                        current_filter_area,
                        buf,
                        &mut state.content_rating.state,
                    );
                },
                MangaFilters::SortBy => {
                    render_filter_list(state.sort_by_state.items.clone(), current_filter_area, buf, &mut state.sort_by_state.state);
                },
                MangaFilters::Tags => self.render_tags_list(current_filter_area, frame, state),
                MangaFilters::MagazineDemographic => {
                    render_filter_list(
                        state.magazine_demographic.items.clone(),
                        current_filter_area,
                        buf,
                        &mut state.magazine_demographic.state,
                    );
                },
                MangaFilters::Authors => {
                    let [list_area, input_area] =
                        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(current_filter_area);

                    match state.author_state.items.as_mut() {
                        Some(authors) => {
                            render_filter_list(authors.clone(), list_area, buf, &mut state.author_state.state);
                        },
                        None => {
                            Paragraph::new(tr(
                                "Busca autores por nombre en el campo de la derecha.",
                                "Search authors by name on the right.",
                            ))
                            .style(Style::default().fg(t.text_muted))
                            .block(
                                t.rounded_block().title(Span::styled(
                                    tr(" Autores ", " Authors "),
                                    Style::default().fg(t.accent_primary).bold(),
                                )),
                            )
                            .wrap(Wrap { trim: true })
                            .render(list_area, buf);
                        },
                    }

                    let input_help = if state.is_typing {
                        Line::from(vec![
                            Span::styled(" <Enter> ", Style::default().fg(t.accent_highlight).bold()),
                            Span::styled(tr("Buscar ", "Search "), Style::default().fg(t.text_muted)),
                            Span::styled("<Esc> ", Style::default().fg(t.accent_highlight).bold()),
                            Span::styled(tr("Salir ", "Stop "), Style::default().fg(t.text_muted)),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled(" <l> ", Style::default().fg(t.accent_highlight).bold()),
                            Span::styled(tr("Buscar autores ", "Search authors "), Style::default().fg(t.text_muted)),
                        ])
                    };

                    render_search_bar(state.is_typing, input_help, &state.author_state.search_bar, frame, input_area);
                },
                MangaFilters::Artists => {
                    let [list_area, input_area] =
                        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(current_filter_area);

                    match state.artist_state.items.as_mut() {
                        Some(authors) => {
                            render_filter_list(authors.clone(), list_area, buf, &mut state.artist_state.state);
                        },
                        None => {
                            Paragraph::new(tr(
                                "Busca artistas por nombre en el campo de la derecha.",
                                "Search artists by name on the right.",
                            ))
                            .style(Style::default().fg(t.text_muted))
                            .block(
                                t.rounded_block().title(Span::styled(
                                    tr(" Artistas ", " Artists "),
                                    Style::default().fg(t.accent_primary).bold(),
                                )),
                            )
                            .wrap(Wrap { trim: true })
                            .render(list_area, buf);
                        },
                    }

                    let input_help = if state.is_typing {
                        Line::from(vec![
                            Span::styled(" <Enter> ", Style::default().fg(t.accent_highlight).bold()),
                            Span::styled(tr("Buscar ", "Search "), Style::default().fg(t.text_muted)),
                            Span::styled("<Esc> ", Style::default().fg(t.accent_highlight).bold()),
                            Span::styled(tr("Salir ", "Stop "), Style::default().fg(t.text_muted)),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled(" <l> ", Style::default().fg(t.accent_highlight).bold()),
                            Span::styled(tr("Buscar artistas ", "Search artists "), Style::default().fg(t.text_muted)),
                        ])
                    };

                    render_search_bar(state.is_typing, input_help, &state.artist_state.search_bar, frame, input_area);
                },
                MangaFilters::Languages => {
                    render_filter_list(state.lang_state.items.clone(), current_filter_area, buf, &mut state.lang_state.state);
                },
            }
        }
    }
}

impl MangadexFilterWidget {
    pub fn new() -> Self {
        Self {
            _style: Style::default(),
        }
    }

    fn render_tags_list(&mut self, area: Rect, frame: &mut Frame<'_>, state: &mut MangadexFilterProvider) {
        let t = theme();
        let buf = frame.buffer_mut();
        let [list_area, input_area] = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);

        let [input_area, current_tags_area] = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(input_area);

        if let Some(tags) = state.tags_state.tags.as_ref().cloned() {
            let mut tags_filtered: Vec<Span<'_>> = Vec::new();
            for tag in tags.iter().filter(|tag| tag.state != TagListItemState::NotSelected) {
                tags_filtered.push(tag.set_filter_tags_style());
                tags_filtered.push(Span::raw(" "));
            }

            let active_tags_block = t
                .rounded_block()
                .title(Span::styled(tr(" Etiquetas activas ", " Active tags "), Style::default().fg(t.accent_secondary).bold()));

            if tags_filtered.is_empty() {
                Paragraph::new(Span::styled(
                    tr("Ninguna etiqueta seleccionada todavía.", "No tags selected yet."),
                    Style::default().fg(t.text_dim).italic(),
                ))
                .block(active_tags_block)
                .wrap(Wrap { trim: true })
                .render(current_tags_area, buf);
            } else {
                Paragraph::new(Line::from(tags_filtered))
                    .block(active_tags_block)
                    .wrap(Wrap { trim: true })
                    .render(current_tags_area, buf);
            }

            if state.tags_state.is_filter_empty() {
                render_tags_list(tags, list_area, buf, &mut state.tags_state.state);
            } else {
                let filtered_tags: Vec<TagListItem> = tags
                    .iter()
                    .filter_map(|tag| {
                        if tag.name.to_lowercase().contains(&state.tags_state.filter_input.value().to_lowercase()) {
                            return Some(tag.clone());
                        }
                        None
                    })
                    .collect();

                render_tags_list(filtered_tags, list_area, buf, &mut state.tags_state.state);
            }

            let input_help = if state.is_typing {
                Line::from(vec![
                    Span::styled(" <Esc> ", Style::default().fg(t.accent_highlight).bold()),
                    Span::styled(tr("Salir ", "Stop "), Style::default().fg(t.text_muted)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(" <l> ", Style::default().fg(t.accent_highlight).bold()),
                    Span::styled(tr("Filtrar etiquetas ", "Filter tags "), Style::default().fg(t.text_muted)),
                ])
            };

            render_search_bar(state.is_typing, input_help, &state.tags_state.filter_input, frame, input_area);
        }
    }
}

fn render_filter_list<'a, T>(items: T, area: Rect, buf: &mut Buffer, state: &mut ListState)
where
    T: IntoIterator,
    T::Item: Into<ListItem<'a>>,
{
    let t = theme();
    let list_block = t.rounded_block().title(Line::from(vec![Span::styled(
        tr(" <j/k> Mover · <s> Seleccionar ", " <j/k> Move · <s> Toggle "),
        Style::default().fg(t.accent_highlight).bold(),
    )]));
    let list = List::new(items)
        .block(list_block)
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(Style::default().fg(t.text_primary).bg(t.row_selected_bg).bold());

    StatefulWidget::render(list, area, buf, state);
}

fn render_tags_list<'a, T>(items: T, area: Rect, buf: &mut Buffer, state: &mut ListState)
where
    T: IntoIterator,
    T::Item: Into<ListItem<'a>>,
{
    let t = theme();
    let list_block = t.rounded_block().title(Line::from(vec![Span::styled(
        tr(" <j/k> Mover · <s> Incluir · <d> Excluir ", " <j/k> Move · <s> Include · <d> Exclude "),
        Style::default().fg(t.accent_highlight).bold(),
    )]));
    let list = List::new(items)
        .block(list_block)
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(Style::default().fg(t.text_primary).bg(t.row_selected_bg).bold());
    StatefulWidget::render(list, area, buf, state);
}
