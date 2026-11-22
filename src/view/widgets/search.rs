use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, StatefulWidget, StatefulWidgetRef, Widget, Wrap};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tui_widget_list::PreRender;

use crate::backend::manga_provider::{Genres, MangaProviders, MangaStatus, SearchManga};
use crate::backend::multi_provider::GroupedSearchManga;
use crate::common::{ImageState, clear_image_canvas, render_stateful_image_cleared};
use crate::i18n::tr;
use crate::theme::{CURRENT_LIST_ITEM_STYLE, theme};

pub(crate) fn provider_pill_colors(provider: MangaProviders) -> (Color, Color) {
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

#[allow(dead_code)]
pub(crate) fn provider_badge_fg(provider: MangaProviders) -> Color {
    match provider {
        MangaProviders::Inmanga => Color::Cyan,
        MangaProviders::Mangaoni => Color::LightRed,
        MangaProviders::Leercapitulo => Color::Green,
        MangaProviders::Mangadex => Color::Yellow,
        MangaProviders::Weebcentral => Color::Magenta,
        MangaProviders::Mangapill => Color::LightBlue,
        MangaProviders::Comizy => Color::LightYellow,
    }
}

pub(crate) fn status_pill_span(status: MangaStatus) -> Span<'static> {
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
    variants: &'a [SearchManga],
    selected_variant_idx: usize,
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
            variants: &[],
            selected_variant_idx: 0,
            can_display_images,
            loader_state,
        }
    }

    pub fn with_variants(mut self, variants: &'a [SearchManga], selected_variant_idx: usize) -> Self {
        self.variants = variants;
        self.selected_variant_idx = selected_variant_idx;
        self
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

            let resolved_id: Option<String> = if state.has_image(self.id) {
                Some(self.id.to_string())
            } else {
                self.variants.iter().find(|v| state.has_image(&v.id)).map(|v| v.id.clone())
            };

            match resolved_id.as_deref().and_then(|id| state.get_image_state(id)) {
                Some(image_state) => {
                    render_stateful_image_cleared(image_state, cover_area, cover_area, buf, Color::Rgb(1, 1, 1));
                },
                None => {
                    clear_image_canvas(cover_area, buf, Color::Rgb(1, 1, 1));
                    let t = theme();
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(t.border_muted))
                        .render(cover_area, buf);
                    let loader = Throbber::default()
                        .label(tr("Cargando...", "Loading..."))
                        .style(Style::default().fg(t.accent_primary))
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
        let t = theme();
        let mut bottom_spans = vec![
            Span::styled(" <r/Enter> ", Style::default().fg(t.accent_primary).bold()),
            Span::styled(tr("Abrir ficha  ", "Open manga  "), Style::default().fg(t.text_muted)),
        ];
        if self.variants.len() > 1 {
            bottom_spans.push(Span::styled(" <P> ", Style::default().fg(t.accent_highlight).bold()));
            bottom_spans.push(Span::styled(tr("Cambiar fuente  ", "Switch source  "), Style::default().fg(t.text_muted)));
        }
        bottom_spans.push(Span::styled(" <p> ", Style::default().fg(t.accent_primary).bold()));
        bottom_spans.push(Span::styled(tr("Guardar en Planeado ", "Save to Plan to read "), Style::default().fg(t.text_muted)));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border))
            .title(Line::from(vec![Span::styled(
                tr(" 📝 Sinopsis ", " 📝 Synopsis "),
                Style::default().fg(t.accent_highlight).bold(),
            )]))
            .title_bottom(Line::from(bottom_spans));

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
            .style(Style::default().fg(t.text_muted))
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }

    pub fn render_details(&mut self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let badges_height = if self.variants.len() > 2 { 3 } else { 2 };
        let layout = Layout::vertical([Constraint::Length(2), Constraint::Length(badges_height), Constraint::Min(1)]);
        let [title_area, badges_area, tags_area] = layout.areas(area);

        Paragraph::new(Line::from(vec![Span::styled(
            self.title,
            Style::default().fg(t.text_primary).add_modifier(Modifier::BOLD),
        )]))
        .wrap(Wrap { trim: true })
        .render(title_area, buf);

        let mut status_spans = Vec::new();
        if self.variants.len() > 1 {
            for (idx, variant) in self.variants.iter().enumerate() {
                if let Some(provider) = variant.provider {
                    let badge_text = format!("[{}]", provider.short_name());
                    if idx == self.selected_variant_idx {
                        let (fg, bg) = provider_pill_colors(provider);
                        status_spans.push(Span::styled(badge_text, Style::default().fg(fg).bg(bg).bold()));
                    } else {
                        let fg = provider_badge_fg(provider);
                        status_spans.push(Span::styled(badge_text, Style::default().fg(fg)));
                    }
                    status_spans.push(Span::raw(" "));
                }
            }
            status_spans.push(Span::styled("<P> ", Style::default().fg(t.accent_highlight).bold()));
        } else if let Some(provider) = self.provider {
            let (fg, bg) = provider_pill_colors(provider);
            status_spans.push(Span::styled(format!("[{}]", provider.short_name()), Style::default().fg(fg).bg(bg).bold()));
            status_spans.push(Span::raw(" "));
        }

        if let Some(status) = self.status {
            status_spans.push(status_pill_span(status));
        }

        Paragraph::new(Line::from(status_spans))
            .wrap(Wrap { trim: true })
            .render(badges_area, buf);

        if self.genres.is_empty() {
            Paragraph::new(Line::from(vec![Span::styled(
                tr("🏷 Sin etiquetas", "🏷 No tags"),
                Style::default().fg(t.text_dim).italic(),
            )]))
            .render(tags_area, buf);
        } else {
            let mut tag_spans: Vec<Span<'static>> =
                vec![Span::styled(format!("{} ", tr("🏷 Géneros:", "🏷 Genres:")), Style::default().fg(t.accent_primary).bold())];
            for genre in self.genres {
                tag_spans.push(crate::utils::genre_span(genre.clone()));
                tag_spans.push(Span::raw(" "));
            }
            Paragraph::new(Line::from(tag_spans)).wrap(Wrap { trim: true }).render(tags_area, buf);
        }
    }
}

impl<'a> StatefulWidget for MangaPreview<'a> {
    type State = ImageState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let outer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent))
            .title(Line::from(vec![Span::styled(
                tr(" 📖 Vista Previa ", " 📖 Manga Preview "),
                Style::default().fg(t.accent_primary).bold(),
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

fn truncate_title_for_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if Span::raw(text).width() <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let target = max_width - 1;
    let mut out = String::new();
    let mut current_w = 0;
    for ch in text.chars() {
        let mut buf = [0u8; 4];
        let ch_str = ch.encode_utf8(&mut buf);
        let ch_w = Span::raw(&*ch_str).width();
        if current_w + ch_w > target {
            break;
        }
        out.push(ch);
        current_w += ch_w;
    }
    out.push('…');
    out
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct MangaItem {
    pub manga: SearchManga,
    pub variants: Vec<SearchManga>,
    pub selected_variant_idx: usize,
    pub style: Style,
    pub is_selected: bool,
}

impl MangaItem {
    fn provider_entries(&self) -> Vec<(usize, MangaProviders)> {
        if !self.variants.is_empty() {
            self.variants
                .iter()
                .enumerate()
                .filter_map(|(idx, variant)| variant.provider.map(|p| (idx, p)))
                .collect()
        } else {
            self.manga.provider.map(|p| vec![(0, p)]).unwrap_or_default()
        }
    }

    fn build_badge_spans(&self, providers: &[(usize, MangaProviders)], max_width: usize, row_bg: Color) -> Vec<Span<'static>> {
        if providers.is_empty() || max_width == 0 {
            return Vec::new();
        }
        let t = theme();
        let badge_width = |p: MangaProviders| Span::raw(format!("[{}]", p.short_name())).width();
        let make_badge = |idx: usize, p: MangaProviders| {
            let text = format!("[{}]", p.short_name());
            if idx == self.selected_variant_idx {
                let (fg, bg) = provider_pill_colors(p);
                Span::styled(text, Style::default().fg(fg).bg(bg).bold())
            } else {
                let fg = provider_badge_fg(p);
                Span::styled(text, Style::default().fg(fg).bg(row_bg))
            }
        };

        let total_width: usize = providers.iter().map(|(_, p)| badge_width(*p)).sum::<usize>() + providers.len().saturating_sub(1);

        let len = providers.len();
        let (start, end) = if total_width <= max_width {
            (0, len)
        } else {
            let sel_pos = providers.iter().position(|(idx, _)| *idx == self.selected_variant_idx).unwrap_or(0);

            let fit_end_from = |start: usize, left_overflow_w: usize| -> usize {
                let mut used = left_overflow_w;
                let mut end = start;
                while end < len {
                    let bw = badge_width(providers[end].1);
                    let sep = usize::from(used > 0);
                    let remaining_after = len - (end + 1);
                    let right_overflow_w = if remaining_after > 0 { 1 + format!("+{remaining_after}").len() } else { 0 };
                    if used + sep + bw + right_overflow_w <= max_width {
                        used += sep + bw;
                        end += 1;
                    } else {
                        break;
                    }
                }
                end
            };

            let prefix_end = fit_end_from(0, 0);
            if sel_pos < prefix_end {
                (0, prefix_end.max(1))
            } else {
                let mut chosen_start = sel_pos;
                let mut chosen_end = sel_pos + 1;
                for s in 1..=sel_pos {
                    let left_w = format!("+{s}").len();
                    let e = fit_end_from(s, left_w);
                    if e > sel_pos {
                        chosen_start = s;
                        chosen_end = e;
                        break;
                    }
                }
                (chosen_start, chosen_end)
            }
        };

        let mut spans = Vec::new();
        if start > 0 {
            spans.push(Span::styled(format!("+{start}"), Style::default().fg(t.text_dim).bg(row_bg).bold()));
            spans.push(Span::styled(" ", Style::default().bg(row_bg)));
        }
        for (offset, (idx, p)) in providers[start..end].iter().enumerate() {
            if offset > 0 {
                spans.push(Span::styled(" ", Style::default().bg(row_bg)));
            }
            spans.push(make_badge(*idx, *p));
        }
        if end < len {
            spans.push(Span::styled(format!(" +{}", len - end), Style::default().fg(t.text_dim).bg(row_bg).bold()));
        }
        spans
    }
}

impl Widget for MangaItem {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let t = theme();
        let row_bg = if self.is_selected { t.row_selected_bg } else { Color::Reset };

        if self.is_selected {
            for x in area.left()..area.right() {
                for y in area.top()..area.bottom() {
                    buf[(x, y)].set_bg(row_bg);
                }
            }
        }

        let cursor_span = if self.is_selected {
            Span::styled("▸ ", Style::default().fg(t.accent_primary).bg(row_bg).bold())
        } else {
            Span::raw("  ")
        };

        let title_style = if self.is_selected {
            Style::default().fg(t.text_primary).bg(row_bg).bold()
        } else {
            Style::default().fg(t.text_muted)
        };

        let providers = self.provider_entries();
        let show_switch_hint = self.is_selected && providers.len() > 1;
        let badge_spans = self.build_badge_spans(&providers, (area.width as usize).saturating_sub(2), row_bg);

        if area.height >= 2 {
            let suffix_w = if show_switch_hint { 4 } else { 0 };
            let max_title_w = (area.width as usize).saturating_sub(2 + suffix_w);
            let display_title = truncate_title_for_width(&self.manga.title, max_title_w);

            let mut line1_spans = vec![cursor_span, Span::styled(display_title, title_style)];
            if show_switch_hint {
                line1_spans.push(Span::styled(" <P>", Style::default().fg(t.accent_highlight).bg(row_bg).bold()));
            }

            if badge_spans.is_empty() {
                Paragraph::new(Line::from(line1_spans)).render(area, buf);
            } else {
                let mut line2_spans = vec![Span::styled("  ", Style::default().bg(row_bg))];
                line2_spans.extend(badge_spans);
                Paragraph::new(vec![Line::from(line1_spans), Line::from(line2_spans)]).render(area, buf);
            }
        } else {
            let mut spans = vec![cursor_span];
            if !badge_spans.is_empty() {
                spans.extend(badge_spans);
                spans.push(Span::styled(" ", Style::default().bg(row_bg)));
            }
            spans.push(Span::styled(self.manga.title, title_style));
            Paragraph::new(Line::from(spans)).render(area, buf);
        }
    }
}

impl PreRender for MangaItem {
    fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
        self.is_selected = context.is_selected;
        if context.is_selected {
            self.style = *CURRENT_LIST_ITEM_STYLE;
        }

        self.item_height()
    }
}

impl From<SearchManga> for MangaItem {
    fn from(value: SearchManga) -> Self {
        Self::new(value)
    }
}

impl MangaItem {
    pub fn item_height(&self) -> u16 {
        if self.provider_entries().is_empty() { 1 } else { 2 }
    }

    pub fn new(manga: SearchManga) -> Self {
        let variants = vec![manga.clone()];
        Self {
            manga,
            variants,
            selected_variant_idx: 0,
            style: Style::default(),
            is_selected: false,
        }
    }

    pub fn from_grouped(group: GroupedSearchManga) -> Self {
        let manga = group.selected_manga().clone();
        Self {
            manga,
            variants: group.variants,
            selected_variant_idx: group.selected_idx,
            style: Style::default(),
            is_selected: false,
        }
    }

    pub fn cycle_provider(&mut self) -> bool {
        if self.variants.len() <= 1 {
            return false;
        }
        self.selected_variant_idx = (self.selected_variant_idx + 1) % self.variants.len();
        if let Some(selected) = self.variants.get(self.selected_variant_idx) {
            self.manga = selected.clone();
        }
        true
    }

    pub fn available_providers(&self) -> Vec<MangaProviders> {
        if self.variants.is_empty() {
            self.manga.provider.into_iter().collect()
        } else {
            self.variants.iter().filter_map(|v| v.provider).collect()
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

    pub fn from_grouped_response(grouped: Vec<GroupedSearchManga>) -> Self {
        Self {
            mangas: grouped.into_iter().map(MangaItem::from_grouped).collect(),
        }
    }
}

impl StatefulWidgetRef for ListMangasFoundWidget {
    type State = tui_widget_list::ListState;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list = tui_widget_list::List::new(self.mangas.clone());
        StatefulWidget::render(list, area, buf, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width).map(|x| buf[(x, y)].symbol()).collect()
    }

    #[test]
    fn single_provider_item_uses_same_two_line_structure_as_grouped_items() {
        let widget = ListMangasFoundWidget::from_grouped_response(vec![
            GroupedSearchManga::new(vec![
                SearchManga {
                    id: "inm_1".to_string(),
                    title: "One Piece".to_string(),
                    provider: Some(MangaProviders::Inmanga),
                    ..Default::default()
                },
                SearchManga {
                    id: "md_1".to_string(),
                    title: "One Piece".to_string(),
                    provider: Some(MangaProviders::Mangadex),
                    ..Default::default()
                },
            ]),
            GroupedSearchManga::new(vec![SearchManga {
                id: "one_shot".to_string(),
                title: "One Piece Oneshot Collection".to_string(),
                provider: Some(MangaProviders::Inmanga),
                ..Default::default()
            }]),
        ]);

        let mut state = tui_widget_list::ListState::default();
        state.select(Some(1));
        let area = Rect::new(0, 0, 60, 4);
        let mut buf = Buffer::empty(area);
        widget.render_ref(area, &mut buf, &mut state);

        // Multi-variant card (rows 0..2)
        let row0 = line_text(&buf, 0, 60);
        let row1 = line_text(&buf, 1, 60);
        assert!(row0.contains("One Piece"), "row0: {row0}");
        assert!(row1.starts_with("  [InManga] [MangaDex]"), "row1: {row1}");

        // Single-variant card (rows 2..4) uses the exact same 2-line layout
        let row2 = line_text(&buf, 2, 60);
        let row3 = line_text(&buf, 3, 60);
        assert!(row2.contains("▸ One Piece Oneshot Collection"), "row2: {row2}");
        assert!(!row2.contains("[InManga]"), "provider badge must be on line 2, not line 1: {row2}");
        assert!(!row2.contains("<P>"), "single provider must not show <P>: {row2}");
        assert!(row3.starts_with("  [InManga]"), "row3 must start with indented badge: {row3}");
    }

    #[test]
    fn long_selected_title_keeps_switch_hint_visible_and_truncates_cleanly() {
        let mut item = MangaItem::from_grouped(GroupedSearchManga::new(vec![
            SearchManga {
                id: "1".to_string(),
                title: "A Very Long Manga Title That Should Truncate Before Hiding The Hint".to_string(),
                provider: Some(MangaProviders::Inmanga),
                ..Default::default()
            },
            SearchManga {
                id: "2".to_string(),
                title: "A Very Long Manga Title That Should Truncate Before Hiding The Hint".to_string(),
                provider: Some(MangaProviders::Mangadex),
                ..Default::default()
            },
        ]));
        item.is_selected = true;

        let area = Rect::new(0, 0, 40, 2);
        let mut buf = Buffer::empty(area);
        item.render(area, &mut buf);

        let line1 = line_text(&buf, 0, 40);
        assert!(line1.contains('…'), "expected ellipsis on truncated title: {line1}");
        assert!(line1.trim_end().ends_with("<P>"), "expected <P> hint to remain visible: {line1}");
    }

    #[test]
    fn overflowing_provider_badges_never_chop_mid_tag_and_keep_selected_visible() {
        let variants = [
            MangaProviders::Inmanga,
            MangaProviders::Mangaoni,
            MangaProviders::Leercapitulo,
            MangaProviders::Mangadex,
            MangaProviders::Weebcentral,
            MangaProviders::Mangapill,
            MangaProviders::Comizy,
        ]
        .into_iter()
        .map(|p| SearchManga {
            id: format!("{p:?}"),
            title: "One Piece".to_string(),
            provider: Some(p),
            ..Default::default()
        })
        .collect();

        let mut item = MangaItem::from_grouped(GroupedSearchManga::new(variants));
        item.is_selected = true;

        let area = Rect::new(0, 0, 68, 2);
        let mut buf = Buffer::empty(area);
        item.clone().render(area, &mut buf);

        let line2_first = line_text(&buf, 1, 68);
        assert!(!line2_first.contains("[Co "), "must not chop [Comizy]: {line2_first}");
        assert!(!line2_first.contains("[Ma "), "must not chop [MangaPill]: {line2_first}");
        assert!(line2_first.contains("[InManga]"), "expected active badge visible: {line2_first}");
        assert!(line2_first.contains('+'), "expected +N overflow indicator: {line2_first}");

        // Cycle selection to the last provider ([Comizy]) and verify it slides into view
        for _ in 0..6 {
            assert!(item.cycle_provider());
        }
        let mut buf_last = Buffer::empty(area);
        item.render(area, &mut buf_last);
        let line2_last = line_text(&buf_last, 1, 68);
        assert!(line2_last.contains("[Comizy]"), "expected selected [Comizy] visible: {line2_last}");
    }
}
