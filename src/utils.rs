use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget, Wrap};
use tui_input::Input;

use crate::backend::manga_provider::{Genres, MangaStatus, Rating};
use crate::i18n::tr;

pub fn rating_style(r: &Rating) -> Style {
    match r {
        Rating::Moderate => Style::new().black().bg(Color::Yellow),
        Rating::Doujinshi => Style::new().black().bg(Color::Blue),
        Rating::Normal => Style::new(),
        Rating::Nsfw => Style::new().black().bg(Color::Red),
    }
}

pub fn genre_span(g: Genres) -> Span<'static> {
    Span::styled(format!(" {} ", g.title), rating_style(&g.rating))
}

pub fn status_span(s: MangaStatus) -> Span<'static> {
    match s {
        MangaStatus::Hiatus => Span::raw(" 🟡 hiatus"),
        MangaStatus::Ongoing => Span::raw(" 🟢 ongoing"),
        MangaStatus::Cancelled => Span::raw(" 🟠 cancelled"),
        MangaStatus::Completed => Span::raw(" 🔵 completed"),
    }
}

/// Convert a `NaiveDate` to a user friendly date, like: "2 days ago" or "hace 2 días"
pub fn display_dates_since_publication(date: NaiveDate) -> String {
    let today = chrono::offset::Local::now().date_naive();

    let difference = today - date;
    let day = difference.num_days();

    if day <= 0 {
        return tr("hoy", "today").to_string();
    }

    let month = (day as f64 / 30.44) as i64;
    let year = (day as f64 / 364.0) as i64;

    if day == 1 {
        tr("hace 1 día", "1 day ago").to_string()
    } else if day <= 31 {
        if crate::i18n::is_es() { format!("hace {day} días") } else { format!("{day} days ago") }
    } else if month <= 1 {
        tr("hace 1 mes", "1 month ago").to_string()
    } else if month <= 12 {
        if crate::i18n::is_es() { format!("hace {month} meses") } else { format!("{month} months ago") }
    } else if year <= 1 {
        tr("hace 1 año", "1 year ago").to_string()
    } else if crate::i18n::is_es() {
        format!("hace {year} años")
    } else {
        format!("{year} years ago")
    }
}

pub fn centered_rect(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn render_search_bar(is_typing: bool, input_help: Line<'_>, input: &Input, frame: &mut Frame<'_>, area: Rect) {
    let t = crate::theme::theme();
    let border_style = if is_typing {
        Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(t.accent_primary)
    };

    let content = if input.value().is_empty() && !is_typing {
        Line::from(vec![Span::styled(
            tr(" 🔍 Pulsa <s> para escribir el nombre de un manga...", " 🔍 Press <s> to type a manga title..."),
            Style::default().fg(t.text_dim).add_modifier(Modifier::ITALIC),
        )])
    } else {
        Line::from(vec![Span::styled(input.value(), Style::default().fg(t.text_primary).add_modifier(Modifier::BOLD))])
    };

    let input_bar = Paragraph::new(content).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .title(input_help)
            .border_style(border_style),
    );

    input_bar.render(area, frame.buffer_mut());

    let width = area.width.max(3) - 3;

    let scroll = input.visual_scroll(width as usize);

    if is_typing {
        frame.set_cursor_position((area.x + ((input.visual_cursor()).max(scroll) - scroll) as u16 + 1, area.y + 1));
    }
}

/// Renders a themed informational card inside the advanced filters modal for providers that use direct search.
pub fn render_no_advanced_filters_card(provider_name: &str, area: Rect, buf: &mut ratatui::buffer::Buffer) {
    let t = crate::theme::theme();
    let card_area = area.inner(Margin {
        horizontal: 3,
        vertical: 2,
    });
    if card_area.width < 12 || card_area.height < 4 {
        return;
    }

    let title = Line::from(vec![
        Span::styled(format!(" ℹ {provider_name} "), Style::default().fg(t.accent_primary).bold()),
        Span::styled(tr("— Búsqueda directa ", "— Direct search "), Style::default().fg(t.text_muted)),
    ]);

    let block = t.rounded_block().title(title);
    let inner = block.inner(card_area).inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    block.render(card_area, buf);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let header_msg = if crate::i18n::is_es() {
        format!("{provider_name} utiliza búsqueda directa por título sin filtros remotos de etiquetas.")
    } else {
        format!("{provider_name} uses direct title search without remote tag filters.")
    };

    let lines = vec![
        Line::from(Span::styled(header_msg, Style::default().fg(t.text_primary).bold())),
        Line::raw(""),
        Line::from(Span::styled(
            tr(
                "Filtros rápidos disponibles directamente en la barra de búsqueda:",
                "Quick filters available right in the search bar:",
            ),
            Style::default().fg(t.accent_highlight).bold(),
        )),
        Line::raw(""),
        Line::from(vec![
            t.key_chip("Tab / m"),
            Span::styled(
                format!(
                    "  {}",
                    tr(
                        "Alternar alcance entre Todos los proveedores y el proveedor activo",
                        "Toggle scope between All Providers and active provider"
                    )
                ),
                Style::default().fg(t.text_secondary),
            ),
        ]),
        Line::from(vec![
            t.key_chip("l"),
            Span::styled(
                format!(
                    "  {}",
                    tr("Filtrar resultados por idioma (Todos / 🇪🇸 ES / 🇬🇧 EN)", "Filter results by language (All / 🇪🇸 ES / 🇬🇧 EN)")
                ),
                Style::default().fg(t.text_secondary),
            ),
        ]),
        Line::from(vec![
            t.key_chip("c / F5"),
            Span::styled(
                format!(
                    "  {}",
                    tr("Abrir Ajustes para cambiar el proveedor predeterminado", "Open Settings to change default provider")
                ),
                Style::default().fg(t.text_secondary),
            ),
        ]),
    ];

    Paragraph::new(lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .render(inner, buf);
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;

    use super::*;

    #[test]
    fn no_advanced_filters_card_renders_themed_instructions() {
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);

        render_no_advanced_filters_card("InManga", area, &mut buf);

        let rendered: String = (0..area.height)
            .map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains('╭'), "card should use rounded border");
        assert!(rendered.contains("InManga"));
        assert!(rendered.contains("Tab / m"));
        assert!(rendered.contains("c / F5"));

        // Small area should safely no-op without panicking
        let tiny = Rect::new(0, 0, 8, 4);
        let mut tiny_buf = Buffer::empty(tiny);
        render_no_advanced_filters_card("InManga", tiny, &mut tiny_buf);
    }
}
