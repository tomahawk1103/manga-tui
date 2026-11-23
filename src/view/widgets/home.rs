use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, Widget, Wrap};
use throbber_widgets_tui::{Throbber, ThrobberState};

use crate::backend::manga_provider::{MangaStatus, PopularManga, Rating, RecentlyAddedManga};
use crate::common::{ImageState, clear_image_canvas, render_stateful_image_cleared};
use crate::i18n::tr;
use crate::theme::theme;

#[derive(Clone, Default, PartialEq, Eq)]
pub enum CarrouselState {
    #[default]
    Searching,
    Displaying,
    NotFound,
}

fn status_pill(status: MangaStatus) -> Span<'static> {
    let t = theme();
    match status {
        MangaStatus::Ongoing => Span::styled(
            format!(" ● {} ", tr("En emisión", "Ongoing")),
            Style::default().fg(Color::Black).bg(t.status_success).bold(),
        ),
        MangaStatus::Completed => Span::styled(
            format!(" ✔ {} ", tr("Finalizado", "Completed")),
            Style::default().fg(Color::Black).bg(t.accent_primary).bold(),
        ),
        MangaStatus::Hiatus => {
            Span::styled(format!(" ⏸ {} ", tr("En pausa", "Hiatus")), Style::default().fg(Color::Black).bg(t.status_warning).bold())
        },
        MangaStatus::Cancelled => Span::styled(
            format!(" ✖ {} ", tr("Cancelado", "Cancelled")),
            Style::default().fg(Color::White).bg(t.status_error).bold(),
        ),
    }
}

#[derive(Clone)]
pub struct CarrouselItemPopularManga {
    pub manga: PopularManga,
    pub loader_state: ThrobberState,
}

impl CarrouselItemPopularManga {
    fn new(manga: PopularManga, loader_state: ThrobberState) -> Self {
        Self {
            manga,
            loader_state,
        }
    }

    fn render_cover(&mut self, area: Rect, buf: &mut Buffer, state: &mut ImageState) {
        let t = theme();
        let cover_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.accent_primary))
            .style(Style::default().bg(t.bar_bg));
        let inner = cover_block.inner(area);
        cover_block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        match state.get_image_state(&self.manga.id) {
            Some(image_state) => {
                render_stateful_image_cleared(image_state, inner, inner, buf, Color::Rgb(12, 14, 24));
            },
            None => {
                clear_image_canvas(inner, buf, Color::Rgb(12, 14, 24));
                let loader = Throbber::default()
                    .label(tr("Cargando portada...", "Loading cover..."))
                    .style(Style::default().fg(t.accent_primary))
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);

                StatefulWidget::render(loader, inner, buf, &mut self.loader_state);
            },
        };
    }

    fn render_details(&mut self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_muted))
            .style(Style::default().bg(t.bar_bg))
            .title(Line::from(vec![
                Span::styled(" ◈ ", Style::default().fg(t.accent_primary).bold()),
                Span::styled(self.manga.title.clone(), Style::default().fg(t.text_primary).bold()),
                Span::raw(" "),
            ]));

        if let Some(status) = self.manga.status {
            block = block.title_top(Line::from(vec![status_pill(status)]).right_aligned());
        }

        let inner = block.inner(area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let [tags_area, description_area] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner);

        let mut genre_spans: Vec<Span<'_>> = Vec::new();
        if self.manga.genres.is_empty() {
            genre_spans.push(Span::styled(
                tr(" ✦ Destacado del catálogo ", " ✦ Featured in catalog "),
                Style::default().fg(t.accent_primary).bg(t.chip_bg).italic(),
            ));
        } else {
            for genre in &self.manga.genres {
                let pill_style = match genre.rating {
                    Rating::Normal => Style::default().fg(t.accent_primary).bg(t.chip_bg),
                    Rating::Moderate => Style::default().fg(t.status_warning).bg(t.chip_bg).bold(),
                    Rating::Doujinshi => Style::default().fg(t.accent_secondary).bg(t.chip_bg).bold(),
                    Rating::Nsfw => Style::default().fg(t.status_error).bg(t.chip_bg).bold(),
                };
                genre_spans.push(Span::styled(format!(" {} ", genre.title), pill_style));
                genre_spans.push(Span::raw(" "));
            }
        }

        Paragraph::new(Line::from(genre_spans)).wrap(Wrap { trim: true }).render(tags_area, buf);

        let synopsis = if self.manga.description.trim().is_empty() {
            Line::from(Span::styled(
                tr(
                    "Sin sinopsis disponible en la vista rápida — pulsa <r> para abrir la ficha completa del manga.",
                    "No synopsis available in quick view — press <r> to open the full manga page.",
                ),
                Style::default().fg(t.text_muted).italic(),
            ))
        } else {
            Line::from(Span::styled(self.manga.description.clone(), Style::default().fg(t.text_secondary)))
        };

        Paragraph::new(synopsis).wrap(Wrap { trim: true }).render(description_area, buf);
    }

    pub fn from_response(value: PopularManga) -> Self {
        Self::new(value, ThrobberState::default())
    }

    pub fn tick(&mut self) {
        self.loader_state.calc_next();
    }
}

#[derive(Default, Clone)]
pub struct PopularMangaCarrousel {
    pub items: Vec<CarrouselItemPopularManga>,
    pub current_item_visible_index: usize,
    pub state: CarrouselState,
    pub can_display_images: bool,
}

impl StatefulWidget for PopularMangaCarrousel {
    type State = ImageState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let layout = Layout::horizontal([Constraint::Percentage(26), Constraint::Percentage(74)]);
        let [cover_area, details_area] = layout.areas(area);
        match self.state {
            CarrouselState::Searching => {
                let block = t.rounded_block();
                let inner = block.inner(area);
                block.render(area, buf);
                Paragraph::new(Line::from(vec![Span::styled(
                    tr(" ⏳ Buscando mangas populares...", " ⏳ Searching popular mangas..."),
                    Style::default().fg(t.accent_primary).bold(),
                )]))
                .render(inner, buf);
                state.set_area(cover_area);
            },
            CarrouselState::Displaying => {
                match self.items.get_mut(self.current_item_visible_index) {
                    Some(item) => {
                        if self.can_display_images {
                            item.render_cover(cover_area, buf, state);
                            item.render_details(details_area, buf);
                        } else {
                            item.render_details(area, buf);
                        }
                    },
                    None => {
                        t.rounded_block().title(tr(" Cargando... ", " Loading... ")).render(area, buf);
                    },
                };
            },
            CarrouselState::NotFound => {
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.status_error));
                let inner = block.inner(area);
                block.render(area, buf);
                Paragraph::new(Line::from(vec![Span::styled(
                    tr(" ⚠ No se encontraron mangas populares", " ⚠ Popular mangas not found"),
                    Style::default().fg(t.status_error).bold(),
                )]))
                .render(inner, buf);
            },
        }
    }
}

impl PopularMangaCarrousel {
    pub fn from_response(response: Vec<PopularManga>, can_display_images: bool) -> Self {
        let mut items: Vec<CarrouselItemPopularManga> = vec![];

        for manga in response {
            items.push(CarrouselItemPopularManga::from_response(manga))
        }

        Self {
            items,
            can_display_images,
            current_item_visible_index: 0,
            state: CarrouselState::Displaying,
        }
    }

    pub fn next_item(&mut self) {
        if self.state == CarrouselState::Displaying && !self.items.is_empty() {
            if self.current_item_visible_index + 1 >= self.items.len() {
                self.current_item_visible_index = 0
            } else {
                self.current_item_visible_index += 1;
            }
        }
    }

    pub fn previous_item(&mut self) {
        if self.state == CarrouselState::Displaying && !self.items.is_empty() {
            if self.current_item_visible_index == 0 {
                self.current_item_visible_index = self.items.len() - 1
            } else {
                self.current_item_visible_index = self.current_item_visible_index.saturating_sub(1);
            }
        }
    }

    pub fn get_current_item(&self) -> Option<&CarrouselItemPopularManga> {
        if self.state == CarrouselState::Displaying { self.items.get(self.current_item_visible_index) } else { None }
    }

    pub fn tick(&mut self) {
        self.items.iter_mut().for_each(|item| item.tick());
    }
}

#[derive(Clone)]
pub struct CarrouselItemRecentlyAddedManga {
    pub manga: RecentlyAddedManga,
    pub loader_state: ThrobberState,
}

impl CarrouselItemRecentlyAddedManga {
    fn render_cover(&mut self, area: Rect, buf: &mut Buffer, state: &mut ImageState) {
        let t = theme();
        match state.get_image_state(&self.manga.id) {
            Some(image_state) => {
                render_stateful_image_cleared(image_state, area, area, buf, Color::Rgb(1, 1, 1));
            },
            None => {
                clear_image_canvas(area, buf, Color::Rgb(1, 1, 1));
                let loader = Throbber::default()
                    .label(tr("Cargando...", "Loading..."))
                    .style(Style::default().fg(t.accent_secondary))
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);

                StatefulWidget::render(loader, area, buf, &mut self.loader_state);
            },
        };
    }

    pub fn tick(&mut self) {
        self.loader_state.calc_next();
    }
}

#[derive(Clone)]
pub struct RecentlyAddedCarrousel {
    pub items: Vec<CarrouselItemRecentlyAddedManga>,
    pub selected_item_index: usize,
    pub amount_items_per_page: usize,
    pub state: CarrouselState,
    pub can_display_images: bool,
}

impl StatefulWidget for RecentlyAddedCarrousel {
    type State = ImageState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .split(area);

        let manga_constraints = if self.can_display_images {
            [Constraint::Percentage(75), Constraint::Percentage(25)]
        } else {
            [Constraint::Percentage(35), Constraint::Percentage(65)]
        };

        match self.state {
            CarrouselState::Displaying => {
                let per_page = self.amount_items_per_page.max(1);
                let page_start = (self.selected_item_index / per_page) * per_page;

                for (col_idx, area_manga) in layout.iter().enumerate() {
                    let item_index = page_start + col_idx;
                    let is_selected = self.selected_item_index == item_index;

                    let card_block = if is_selected {
                        Block::bordered()
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(t.accent_primary).bold())
                            .style(Style::default().bg(t.row_selected_bg))
                            .title(Line::from(vec![Span::styled(
                                format!(" ▸ #{} ", item_index + 1),
                                Style::default().fg(Color::Black).bg(t.accent_primary).bold(),
                            )]))
                    } else {
                        Block::bordered()
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(t.border_muted))
                            .style(Style::default().bg(t.bar_bg))
                            .title(Line::from(vec![Span::styled(
                                format!(" #{} ", item_index + 1),
                                Style::default().fg(t.text_dim),
                            )]))
                    };

                    let inner_card = card_block.inner(*area_manga);

                    if let Some(item) = self.items.get_mut(item_index) {
                        card_block.render(*area_manga, buf);

                        if inner_card.width == 0 || inner_card.height == 0 {
                            continue;
                        }

                        let [top, bottom] = Layout::vertical(manga_constraints).areas(inner_card);

                        let title_style = if is_selected {
                            Style::default().fg(t.text_primary).bg(t.row_selected_bg).bold()
                        } else {
                            Style::default().fg(t.text_secondary)
                        };

                        if self.can_display_images {
                            item.render_cover(top, buf, state);
                            Paragraph::new(Line::from(Span::styled(item.manga.title.clone(), title_style)))
                                .wrap(Wrap { trim: true })
                                .render(bottom, buf);
                        } else {
                            Paragraph::new(Line::from(Span::styled(item.manga.title.clone(), title_style)))
                                .wrap(Wrap { trim: true })
                                .render(top, buf);
                            Paragraph::new(Line::from(Span::styled(
                                item.manga.description.clone(),
                                Style::default().fg(t.text_muted),
                            )))
                            .wrap(Wrap { trim: true })
                            .render(bottom, buf);
                        }
                    } else {
                        clear_image_canvas(*area_manga, buf, Color::Rgb(1, 1, 1));
                    }
                }
            },
            CarrouselState::Searching => {
                let block = t.rounded_block();
                let inner = block.inner(area);
                block.render(area, buf);
                Paragraph::new(Line::from(vec![Span::styled(
                    tr(" ⏳ Buscando mangas recientes...", " ⏳ Searching recent mangas..."),
                    Style::default().fg(t.accent_secondary).bold(),
                )]))
                .render(inner, buf);

                if self.can_display_images {
                    let inner_card = layout[0].inner(Margin {
                        horizontal: 1,
                        vertical: 1,
                    });
                    let [cover_area, _b] = Layout::vertical(manga_constraints).areas(inner_card);
                    state.set_area(cover_area);
                }
            },
            CarrouselState::NotFound => {
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.status_error));
                let inner = block.inner(area);
                block.render(area, buf);
                Paragraph::new(Line::from(vec![Span::styled(
                    tr(" ⚠ No se pudieron obtener los mangas recientes", " ⚠ Could not get recent mangas"),
                    Style::default().fg(t.status_error).bold(),
                )]))
                .render(inner, buf);
            },
        }
    }
}

impl RecentlyAddedCarrousel {
    pub fn new(can_display_images: bool) -> Self {
        Self {
            can_display_images,
            items: vec![],
            selected_item_index: 0,
            amount_items_per_page: 5,
            state: CarrouselState::default(),
        }
    }

    pub fn select_next(&mut self) {
        if self.state == CarrouselState::Displaying && !self.items.is_empty() {
            if self.selected_item_index + 1 < self.items.len() {
                self.selected_item_index += 1;
            } else {
                self.selected_item_index = 0;
            }
        }
    }

    pub fn select_previous(&mut self) {
        if self.state == CarrouselState::Displaying && !self.items.is_empty() {
            if self.selected_item_index == 0 {
                self.selected_item_index = self.items.len() - 1;
            } else {
                self.selected_item_index = self.selected_item_index.saturating_sub(1);
            }
        }
    }

    pub fn get_current_selected_manga(&self) -> Option<&CarrouselItemRecentlyAddedManga> {
        if self.state == CarrouselState::Displaying { self.items.get(self.selected_item_index) } else { None }
    }

    pub fn tick(&mut self) {
        self.items.iter_mut().for_each(|item| item.tick());
    }

    pub fn from_response(response: Vec<RecentlyAddedManga>, can_display_images: bool) -> Self {
        let mut items: Vec<CarrouselItemRecentlyAddedManga> = vec![];

        for mangas in response {
            items.push(CarrouselItemRecentlyAddedManga {
                manga: mangas,
                loader_state: ThrobberState::default(),
            });
        }

        Self {
            can_display_images,
            items,
            selected_item_index: 0,
            amount_items_per_page: 5,
            state: CarrouselState::Displaying,
        }
    }
}
