use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, StatefulWidgetRef, Widget, Wrap};
use ratatui_image::{Resize, StatefulImage};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tui_widget_list::PreRender;

use crate::backend::manga_provider::{Genres, MangaProviders, MangaStatus, SearchManga};
use crate::common::ImageState;
use crate::config::MangaTuiConfig;
use crate::global::CURRENT_LIST_ITEM_STYLE;
use crate::i18n::tr;

fn provider_pill_colors(provider: MangaProviders) -> (Color, Color) {
    match provider {
        MangaProviders::Inmanga => (Color::Black, Color::Cyan),
        MangaProviders::Mangaoni => (Color::White, Color::Red),
        MangaProviders::Leercapitulo => (Color::Black, Color::Green),
        MangaProviders::Mangadex => (Color::Black, Color::Yellow),
        MangaProviders::Weebcentral => (Color::White, Color::Magenta),
        MangaProviders::Mangapill => (Color::White, Color::Blue),
        MangaProviders::Comizy => (Color::Black, Color::LightYellow),
    }
}

fn status_pill_span(status: MangaStatus) -> Span<'static> {
    match status {
        MangaStatus::Ongoing => {
            Span::styled(format!(" ● {} ", tr("En emisión", "Ongoing")), Style::default().fg(Color::Black).bg(Color::Green).bold())
        },
        MangaStatus::Completed => {
            Span::styled(format!(" ✔ {} ", tr("Finalizado", "Completed")), Style::default().fg(Color::White).bg(Color::Blue).bold())
        },
        MangaStatus::Hiatus => {
            Span::styled(format!(" ⏸ {} ", tr("En pausa", "Hiatus")), Style::default().fg(Color::Black).bg(Color::Yellow).bold())
        },
        MangaStatus::Cancelled => {
            Span::styled(format!(" ✖ {} ", tr("Cancelado", "Cancelled")), Style::default().fg(Color::White).bg(Color::Red).bold())
        },
    }
}

pub struct MangaPreview<'a> {
    id: &'a str,
    title: &'a str,
    description: &'a str,
    genres: &'a Vec<Genres>,
    status: Option<MangaStatus>,
    provider: Option<MangaProviders>,
    can_display_images: bool,
    loader_state: ThrobberState,
}

#[allow(clippy::too_many_arguments)]
impl<'a> MangaPreview<'a> {
    pub fn new(
        id: &'a str,
        title: &'a str,
        description: &'a str,
        genres: &'a Vec<Genres>,
        status: Option<MangaStatus>,
        provider: Option<MangaProviders>,
        can_display_images: bool,
        loader_state: ThrobberState,
    ) -> Self {
        Self {
            id,
            title,
            description,
            genres,
            status,
            provider,
            can_display_images,
            loader_state,
        }
    }

    pub fn render_cover_and_details_area(&mut self, area: Rect, buf: &mut Buffer, state: &mut ImageState) {
        let layout = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]);

        let [cover_area, details_area] = layout.areas(area);

        if self.can_display_images {
            self.render_details(
                details_area.inner(Margin {
                    horizontal: 1,
                    vertical: 0,
                }),
                buf,
            );
            state.set_area(cover_area);

            match state.get_image_state(self.id) {
                Some(image_state) => {
                    let filter = MangaTuiConfig::get().image_resize_filter.to_filter_type();
                    let cover = StatefulImage::new(None).resize(Resize::Fit(Some(filter)));
                    StatefulWidget::render(cover, cover_area, buf, image_state);
                },
                None => {
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::DarkGray))
                        .render(cover_area, buf);
                    let loader = Throbber::default()
                        .label(tr("Cargando...", "Loading..."))
                        .style(Style::default().fg(Color::Cyan))
                        .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                        .use_type(throbber_widgets_tui::WhichUse::Spin);

                    StatefulWidget::render(
                        loader,
                        cover_area.inner(Margin {
                            horizontal: 1,
                            vertical: 1,
                        }),
                        buf,
                        &mut self.loader_state,
                    );
                },
            };
        } else {
            self.render_details(
                area.inner(Margin {
                    horizontal: 1,
                    vertical: 1,
                }),
                buf,
            );
        }
    }

    pub fn render_description_area(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(75, 88, 120)))
            .title(Line::from(vec![Span::styled(tr(" 📝 Sinopsis ", " 📝 Synopsis "), Style::default().fg(Color::Yellow).bold())]))
            .title_bottom(Line::from(vec![
                Span::styled(" <r/Enter> ", Style::default().fg(Color::Cyan).bold()),
                Span::styled(tr("Abrir ficha  ", "Open manga  "), Style::default().fg(Color::Gray)),
                Span::styled(" <p> ", Style::default().fg(Color::Cyan).bold()),
                Span::styled(tr("Guardar en Planeado ", "Save to Plan to read "), Style::default().fg(Color::Gray)),
            ]));

        let inner = block.inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        block.render(area, buf);

        let desc_text = if self.description.trim().is_empty() || self.description == "No description" {
            tr("Sin descripción disponible para este manga.", "No description available for this manga.")
        } else {
            self.description
        };

        Paragraph::new(desc_text)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }

    pub fn render_details(&mut self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::vertical([Constraint::Length(2), Constraint::Length(2), Constraint::Min(1)]);
        let [title_area, badges_area, tags_area] = layout.areas(area);

        Paragraph::new(Line::from(vec![Span::styled(self.title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))]))
            .wrap(Wrap { trim: true })
            .render(title_area, buf);

        let mut status_spans = Vec::new();
        if let Some(provider) = self.provider {
            let (fg, bg) = provider_pill_colors(provider);
            status_spans.push(Span::styled(format!(" {} ", provider), Style::default().fg(fg).bg(bg).bold()));
            status_spans.push(Span::raw(" "));
        }

        if let Some(status) = self.status {
            status_spans.push(status_pill_span(status));
        }

        Paragraph::new(Line::from(status_spans)).render(badges_area, buf);

        if self.genres.is_empty() {
            Paragraph::new(Line::from(vec![Span::styled(
                tr("🏷 Sin etiquetas", "🏷 No tags"),
                Style::default().fg(Color::DarkGray).italic(),
            )]))
            .render(tags_area, buf);
        } else {
            let mut tag_spans: Vec<Span<'static>> =
                vec![Span::styled(format!("{} ", tr("🏷 Géneros:", "🏷 Genres:")), Style::default().fg(Color::Cyan).bold())];
            for genre in self.genres {
                tag_spans.push(genre.clone().into());
                tag_spans.push(Span::raw(" "));
            }
            Paragraph::new(Line::from(tag_spans)).wrap(Wrap { trim: true }).render(tags_area, buf);
        }
    }
}

impl<'a> StatefulWidget for MangaPreview<'a> {
    type State = ImageState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let outer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Line::from(vec![Span::styled(
                tr(" 📖 Vista Previa ", " 📖 Manga Preview "),
                Style::default().fg(Color::Cyan).bold(),
            )]));

        let inner = outer_block.inner(area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        outer_block.render(area, buf);

        if inner.height < 4 || inner.width < 10 {
            return;
        }

        let [cover_details_area, description_area] =
            Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(inner);

        self.render_cover_and_details_area(cover_details_area, buf, state);
        self.render_description_area(description_area, buf);
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct MangaItem {
    pub manga: SearchManga,
    pub style: Style,
    pub is_selected: bool,
}

impl Widget for MangaItem {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let row_bg = if self.is_selected { Color::Rgb(42, 49, 72) } else { Color::Reset };

        if self.is_selected {
            for x in area.left()..area.right() {
                for y in area.top()..area.bottom() {
                    buf[(x, y)].set_bg(row_bg);
                }
            }
        }

        let cursor_span = if self.is_selected {
            Span::styled("▸ ", Style::default().fg(Color::Cyan).bg(row_bg).bold())
        } else {
            Span::raw("  ")
        };

        let title_style =
            if self.is_selected { Style::default().fg(Color::White).bg(row_bg).bold() } else { Style::default().fg(Color::Gray) };

        let line = if let Some(provider) = self.manga.provider {
            let (fg, bg) = provider_pill_colors(provider);
            let badge = Span::styled(format!(" {} ", provider.badge_label()), Style::default().fg(fg).bg(bg).bold());
            Line::from(vec![
                cursor_span,
                badge,
                Span::styled(" ", Style::default().bg(row_bg)),
                Span::styled(self.manga.title, title_style),
            ])
        } else {
            Line::from(vec![cursor_span, Span::styled(self.manga.title, title_style)])
        };

        Paragraph::new(line).wrap(Wrap { trim: true }).render(area, buf);
    }
}

impl PreRender for MangaItem {
    fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
        self.is_selected = context.is_selected;
        if context.is_selected {
            self.style = *CURRENT_LIST_ITEM_STYLE;
        }

        1
    }
}

impl From<SearchManga> for MangaItem {
    fn from(value: SearchManga) -> Self {
        Self::new(value)
    }
}

impl MangaItem {
    pub fn new(manga: SearchManga) -> Self {
        Self {
            manga,
            style: Style::default(),
            is_selected: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ListMangasFoundWidget {
    pub mangas: Vec<MangaItem>,
}

impl ListMangasFoundWidget {
    pub fn from_response(search_response: Vec<SearchManga>) -> Self {
        let mut mangas: Vec<MangaItem> = vec![];

        for manga in search_response {
            mangas.push(MangaItem::from(manga));
        }

        Self { mangas }
    }
}

impl StatefulWidgetRef for ListMangasFoundWidget {
    type State = tui_widget_list::ListState;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list = tui_widget_list::List::new(self.mangas.clone());
        StatefulWidget::render(list, area, buf, state);
    }
}
