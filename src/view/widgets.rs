#[cfg(test)]
use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget, WidgetRef, Wrap};

use crate::backend::manga_provider::{LatestChapter, MangaProviders};
use crate::backend::tui::Events;
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::centered_rect;

pub mod feed;
pub mod home;
pub mod manga;
pub mod reader;
pub mod search;
pub mod settings;
pub mod updater;

pub use settings::{SettingsModal, SettingsModalState, SettingsOutcome};
pub use updater::{AppUpdateModal, AppUpdateModalState, AppUpdateStage};

pub trait Component {
    type Actions;
    ///Handles the logic for drawing to the screen
    fn render(&mut self, area: Rect, frame: &mut Frame<'_>);
    fn handle_events(&mut self, events: Events);
    fn update(&mut self, action: Self::Actions);

    /// This is intended for stuff like aborting tasks and clearing vec's
    fn clean_up(&mut self);
}

pub trait StatefulWidgetFrame {
    type State;
    fn render(&mut self, area: Rect, frame: &mut Frame<'_>, state: &mut Self::State);
}

#[derive(Debug, Default)]
pub struct ModalDimensions {
    width: u16,
    height: u16,
}

#[derive(Debug)]
pub struct Modal<T: WidgetRef> {
    dimensions: ModalDimensions,
    child_widget: T,
}

#[derive(Debug, Default)]
pub struct ModalBuilder<T: WidgetRef> {
    dimensions: Option<ModalDimensions>,
    child_widget: T,
}

impl<T: WidgetRef> ModalBuilder<T> {
    #[inline]
    pub fn new(child_widget: T) -> Self {
        Self {
            dimensions: None,
            child_widget,
        }
    }

    #[inline]
    pub fn with_dimensions(mut self, dimensions: ModalDimensions) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    #[inline]
    pub fn build(self) -> Modal<T> {
        Modal::new(
            self.dimensions.unwrap_or(ModalDimensions {
                width: 50,
                height: 50,
            }),
            self.child_widget,
        )
    }
}

impl<T: WidgetRef> Modal<T> {
    pub fn new(dimensions: ModalDimensions, child_widget: T) -> Self {
        Self {
            dimensions,
            child_widget,
        }
    }
}

impl<T: WidgetRef> WidgetRef for Modal<T> {
    fn render_ref(&self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let area = centered_rect(area, self.dimensions.width, self.dimensions.height);
        Clear.render(area, buf);

        self.child_widget.render_ref(area, buf);
    }
}

impl<T: WidgetRef> Widget for Modal<T> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        WidgetRef::render_ref(&self, area, buf);
    }
}

#[derive(Debug)]
struct ErrorModalBody<'a> {
    message: &'a str,
}

impl<'a> Widget for ErrorModalBody<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        self.render_ref(area, buf);
    }
}

impl<'a> WidgetRef for ErrorModalBody<'a> {
    fn render_ref(&self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let t = theme();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.status_error))
            .title(Line::from(vec![Span::styled(
                tr(" ⚠ Error ", " ⚠ Error "),
                Style::default().fg(t.status_error).add_modifier(Modifier::BOLD),
            )]))
            .title_bottom(
                Line::from(vec![Span::styled(
                    tr(" Pulsa <q> o <Esc> para cerrar ", " Press <q> or <Esc> to close "),
                    Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                )])
                .alignment(Alignment::Center),
            );

        let inner = block.inner(area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(area, buf);

        Paragraph::new(self.message)
            .style(Style::default().fg(t.text_primary))
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }
}

#[derive(Debug)]
pub struct ErrorModal<'a> {
    message: &'a str,
}

impl<'a> ErrorModal<'a> {
    #[inline]
    pub fn new(message: &'a str) -> Self {
        Self { message }
    }
}

impl<'a> Widget for ErrorModal<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let min_w = 42.min(area.width).max(1);
        let max_w = area.width.saturating_sub(4).max(min_w);
        let target_width = (area.width.saturating_mul(60) / 100).clamp(min_w, max_w);
        let desired_height = 9.min(area.height.max(1));
        let modal_area = centered_modal_rect(area, target_width, desired_height);

        Clear.render(modal_area, buf);
        ErrorModalBody {
            message: self.message,
        }
        .render_ref(modal_area, buf);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewChapterItem {
    pub manga_id: String,
    pub manga_title: String,
    pub provider: MangaProviders,
    pub chapter_numbers: String,
    pub chapters: Vec<LatestChapter>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewChaptersModalState {
    pub items: Vec<NewChapterItem>,
    pub cursor: usize,
    pub status_message: Option<String>,
}

impl NewChaptersModalState {
    pub fn new(items: Vec<NewChapterItem>) -> Self {
        Self {
            items,
            cursor: 0,
            status_message: None,
        }
    }
}

#[derive(Debug)]
pub struct NewChaptersModal<'a> {
    state: &'a NewChaptersModalState,
}

impl<'a> NewChaptersModal<'a> {
    #[inline]
    pub fn new(state: &'a NewChaptersModalState) -> Self {
        Self { state }
    }
}

fn centered_modal_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width).max(1);
    let h = height.min(area.height).max(1);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

impl<'a> Widget for NewChaptersModal<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let t = theme();
        let target_width = (area.width.saturating_mul(68) / 100).clamp(52.min(area.width), area.width.saturating_sub(4).max(1));

        if let Some(status) = &self.state.status_message {
            let modal_area = centered_modal_rect(area, target_width, 9);
            Clear.render(modal_area, buf);

            let block = t
                .rounded_accent_block()
                .title(Line::from(vec![Span::styled(
                    tr(" ★ Nuevos capítulos ", " ★ New chapters "),
                    Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
                )]))
                .title_bottom(
                    Line::from(vec![Span::styled(
                        tr(" Pulsa <q> o <Esc> para cerrar ", " Press <q> or <Esc> to close "),
                        Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                    )])
                    .alignment(Alignment::Center),
                );
            let inner = block.inner(modal_area).inner(Margin {
                horizontal: 2,
                vertical: 1,
            });
            block.render(modal_area, buf);
            Paragraph::new(status.as_str())
                .style(Style::default().fg(t.status_success).add_modifier(Modifier::BOLD))
                .wrap(Wrap { trim: true })
                .render(inner, buf);
            return;
        }

        let selected_count = self.state.items.iter().filter(|i| i.selected).count();
        let total_count = self.state.items.len();

        let max_modal_height = (area.height.saturating_mul(65) / 100).max(10);
        let desired_height = (total_count as u16).saturating_add(7).clamp(10, max_modal_height);
        let modal_area = centered_modal_rect(area, target_width, desired_height);
        Clear.render(modal_area, buf);

        let title = if crate::i18n::is_es() {
            format!(" ★ Nuevos capítulos disponibles ({selected_count}/{total_count}) ")
        } else {
            format!(" ★ New chapters available ({selected_count}/{total_count}) ")
        };

        let block = t
            .rounded_accent_block()
            .title(Line::from(vec![Span::styled(title, Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD))]))
            .title_bottom(
                Line::from(vec![Span::styled(
                    tr(
                        " <j/k> Mover | <Espacio> Marcar | <a> Todos | <y/d> Descargar | <q> Cerrar ",
                        " <j/k> Move | <Space> Toggle | <a> All | <y/d> Download | <q> Close ",
                    ),
                    Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                )])
                .alignment(Alignment::Center),
            );

        let inner = block.inner(modal_area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(modal_area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        let header = Line::from(vec![Span::styled(
            tr("Selecciona mangas para descargar nuevos capítulos:", "Select manga to download new chapters:"),
            Style::default().fg(t.text_secondary).add_modifier(Modifier::BOLD),
        )]);

        let max_items_visible = (inner.height.saturating_sub(2)) as usize;
        let cursor = self.state.cursor;
        let scroll = if max_items_visible == 0 || total_count <= max_items_visible || cursor < max_items_visible {
            0
        } else {
            cursor + 1 - max_items_visible
        };

        let mut lines = vec![header, Line::raw("")];

        for (idx, item) in self.state.items.iter().enumerate().skip(scroll).take(max_items_visible) {
            let is_focused = idx == cursor;
            let cursor_span = if is_focused {
                Span::styled("▸ ", Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD))
            } else {
                Span::raw("  ")
            };

            let check_span = if item.selected {
                Span::styled("[✓] ", Style::default().fg(t.status_success).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("[ ] ", Style::default().fg(t.text_dim))
            };

            let title_style = if is_focused {
                if item.selected {
                    Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text_muted).add_modifier(Modifier::BOLD)
                }
            } else if item.selected {
                Style::default().fg(t.text_primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.text_dim)
            };

            let ch_style = if item.selected { Style::default().fg(t.accent_highlight) } else { Style::default().fg(t.text_dim) };

            let mut spans = vec![
                cursor_span,
                check_span,
                Span::styled(item.manga_title.as_str(), title_style),
                Span::styled(format!(": {} {}", tr("Cap.", "Ch."), item.chapter_numbers), ch_style),
                Span::styled(format!(" [{}]", item.provider), Style::default().fg(t.text_muted)),
            ];
            if !item.selected {
                spans.push(Span::styled(
                    tr(" (omitido)", " (skipped)"),
                    Style::default().fg(t.text_muted).add_modifier(Modifier::ITALIC),
                ));
            }

            let mut line = Line::from(spans);
            if is_focused {
                line = line.style(Style::default().bg(t.row_selected_bg));
            }
            lines.push(line);
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

#[derive(Debug)]
pub struct DownloadQueueModal<'a> {
    progress: Option<&'a crate::backend::download_queue::DownloadQueueProgress>,
}

impl<'a> DownloadQueueModal<'a> {
    pub fn new(progress: Option<&'a crate::backend::download_queue::DownloadQueueProgress>) -> Self {
        Self { progress }
    }
}

impl<'a> Widget for DownloadQueueModal<'a> {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let t = theme();
        let area = centered_rect(area, 60, 40);
        Clear.render(area, buf);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent))
            .title(Line::from(vec![Span::styled(
                tr(" ⬇ Cola de Descargas ", " ⬇ Download Queue "),
                Style::default().add_modifier(Modifier::BOLD).fg(t.accent_primary),
            )]))
            .title_bottom(Line::from(vec![Span::styled(
                tr(" <c> Cancelar todo | <q>, <Esc> o <D> Cerrar ", " <c> Cancel All | <q> or <Esc> or <D> Close "),
                Style::default().fg(t.accent_highlight),
            )]));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 || inner.width < 10 {
            return;
        }

        let mut lines = Vec::new();
        match self.progress {
            Some(p) if p.is_downloading => {
                lines.push(Line::from(vec![
                    Span::styled(
                        tr("Descargando actualmente: ", "Currently downloading: "),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&p.current_manga_title, Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(vec![Span::raw(format!(
                    "{} {}  |  {} {:.0}%",
                    tr("Capítulo:", "Chapter:"),
                    p.current_chapter_number,
                    tr("Progreso pág.:", "Page progress:"),
                    p.current_page_percentage
                ))]));

                let mut batch_text = format!(
                    "{} {} {} {} {} ({} {})",
                    tr("Lote:", "Batch:"),
                    p.completed_in_batch,
                    tr("de", "of"),
                    p.total_in_batch,
                    tr("completados", "completed"),
                    p.remaining_items,
                    tr("restantes en cola", "remaining in queue")
                );
                if p.failed_in_batch > 0 {
                    batch_text.push_str(&format!(" | {}: {}", tr("Fallidos", "Failed"), p.failed_in_batch));
                }
                lines.push(Line::from(vec![Span::styled(
                    batch_text,
                    Style::default().fg(if p.failed_in_batch > 0 { t.status_error } else { t.accent_primary }),
                )]));
                lines.push(Line::raw(""));

                // Gauge representation
                let gauge_width = (inner.width.saturating_sub(10)) as usize;
                let filled = ((p.current_page_percentage / 100.0) * gauge_width as f64).round() as usize;
                let filled = filled.min(gauge_width);
                let empty = gauge_width.saturating_sub(filled);
                let bar = format!("[{}{}] {:.0}%", "█".repeat(filled), "░".repeat(empty), p.current_page_percentage);
                lines.push(Line::from(vec![Span::styled(bar, Style::default().fg(t.accent_primary))]));
            },
            Some(p) if p.remaining_items > 0 => {
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        "{} ({} {})",
                        tr("Cola en espera", "Queue waiting"),
                        p.remaining_items,
                        tr("elementos restantes", "items remaining")
                    ),
                    Style::default().fg(t.accent_highlight),
                )]));
            },
            _ => {
                lines.push(Line::raw(tr("No hay descargas activas en la cola.", "No active downloads in queue.")));
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![
                    Span::styled(tr("Consejo: ", "Tip: "), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(tr(
                        "¡Selecciona capítulos con <Espacio> y pulsa <d> para añadirlos a la cola!",
                        "Select chapters with <Space> and press <d> to add them to the queue!",
                    )),
                ]));
            },
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

#[derive(Debug, Default)]
pub struct HelpModal;

impl HelpModal {
    pub fn new() -> Self {
        Self
    }

    fn key_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
        let t = theme();
        Line::from(vec![
            Span::styled(format!(" {:<10} ", key), Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD)),
            Span::styled(desc, Style::default().fg(t.text_primary)),
        ])
    }

    fn section_title<'a>(title: &'a str) -> Line<'a> {
        let t = theme();
        Line::from(vec![Span::styled(format!(" ▸ {title}"), Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD))])
    }
}

impl Widget for HelpModal {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer) {
        let t = theme();
        let modal_area = centered_rect(area, 82, 88);
        Clear.render(modal_area, buf);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent))
            .title(Line::from(vec![Span::styled(
                tr(" ⌨  Atajos de Teclado y Ayuda Rápida ", " ⌨  Keyboard Shortcuts & Quick Help "),
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
            )]))
            .title_bottom(
                Line::from(vec![Span::styled(
                    tr(" Pulsa <n>, <q> o <Esc> para cerrar ", " Press <n>, <q> or <Esc> to close "),
                    Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                )])
                .alignment(Alignment::Center),
            );

        let inner = block.inner(modal_area).inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        block.render(modal_area, buf);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        let [left_col, right_col] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(inner);

        let left_lines = vec![
            Self::section_title(tr("Navegación Global", "Global Navigation")),
            Self::key_line("u / F1", tr("Ir a Inicio", "Go to Home")),
            Self::key_line("i / F2", tr("Ir a Buscar", "Go to Search")),
            Self::key_line("o / F3", tr("Ir a Seguimiento (Biblioteca)", "Go to Feed (Library)")),
            Self::key_line("c / F5", tr("Abrir menú de Ajustes", "Open Settings menu")),
            Self::key_line(
                "T",
                tr("Alternar tema (Tokyo Night/Catppuccin/Clásico)", "Cycle theme (Tokyo Night/Catppuccin/Classic)"),
            ),
            Self::key_line("D", tr("Abrir cola de descargas", "Open download queue")),
            Self::key_line("n / F4", tr("Mostrar/ocultar esta ayuda", "Toggle this help modal")),
            Self::key_line("Ctrl+C", tr("Salir de manga-tui", "Quit manga-tui")),
            Line::raw(""),
            Self::section_title(tr("Buscador (Search)", "Search Page")),
            Self::key_line("s", tr("Escribir en barra de búsqueda", "Focus search input")),
            Self::key_line("Enter", tr("Ejecutar búsqueda / Abrir", "Run search / Open")),
            Self::key_line("1-3 / ←/→", tr("Explorar Ranking Mundial", "Browse Global Ranking")),
            Self::key_line("j / k", tr("Moverse por resultados", "Navigate results")),
            Self::key_line("w / b", tr("Página siguiente / anterior", "Next / previous page")),
            Self::key_line("Tab / m", tr("Alternar Todos / Proveedor actual", "Toggle All / Current provider")),
            Self::key_line("l", tr("Filtrar idioma (Todos/ES/EN)", "Filter language (All/ES/EN)")),
            Self::key_line("p", tr("Guardar en Planeado (Plan to read)", "Save to Plan to read")),
            Self::key_line("P", tr("Cambiar proveedor agrupado (Shift+P)", "Switch grouped provider (Shift+P)")),
            Self::key_line("f", tr("Abrir filtros avanzados", "Open advanced filters")),
            Self::key_line("r / Enter", tr("Abrir ficha del manga", "Open manga page")),
            Self::key_line("Esc", tr("Volver al Ranking Mundial", "Return to Global Ranking")),
        ];

        let right_lines = vec![
            Self::section_title(tr("Ficha del Manga", "Manga Page")),
            Self::key_line("j / k", tr("Subir / bajar en capítulos", "Scroll chapters up / down")),
            Self::key_line("/ o s", tr("Ir a capítulo o página", "Jump to chapter or page")),
            Self::key_line("g / G", tr("Primer / último capítulo", "First / last chapter")),
            Self::key_line("r / v", tr("Leer en TUI / Abrir Visor Web HD", "Read in TUI / Open HD Web Viewer")),
            Self::key_line("x", tr("Marcar/desmarcar como leído", "Toggle chapter read")),
            Self::key_line("Space", tr("Seleccionar capítulo para lote", "Toggle select chapter")),
            Self::key_line("V / c", tr("Seleccionar página / Limpiar", "Select page / Clear selection")),
            Self::key_line("d / a", tr("Descargar selección / todos", "Download selected / all")),
            Self::key_line("f", tr("Marcar/desmarcar Favorito ★", "Toggle Favorite ★")),
            Self::key_line("P", tr("Migrar a otro proveedor", "Switch manga provider")),
            Self::key_line("l / t", tr("Cambiar idioma / orden", "Change language / order")),
            Self::key_line("Tab / m", tr("Ir a marcador / Guardar marcador", "Read / Save bookmark")),
            Line::raw(""),
            Self::section_title(tr("Biblioteca y Lector", "Library & Reader")),
            Self::key_line(
                "Tab / 1-4",
                tr("Cambiar pestaña en Seguimiento (4: Descargas Offline)", "Switch tab in Feed (4: Offline Downloads)"),
            ),
            Self::key_line("R", tr("Actualizar novedades (Seguimiento)", "Refresh updates (Feed)")),
            Self::key_line("d / X", tr("Descargar o quitar / Vaciar lista", "Download or remove / Clear list")),
            Self::key_line("j / k", tr("Pasar página en el lector", "Next / prev page in reader")),
            Self::key_line("g / G", tr("Primera / última pág. en lector", "First / last page in reader")),
            Self::key_line("v", tr("Abrir en navegador (Scroll HD)", "Open in browser (HD Scroll)")),
            Self::key_line("r", tr("Recargar pág. fallida (lector)", "Reload failed page (reader)")),
            Self::key_line("w / b", tr("Siguiente / anterior capítulo", "Next / prev chapter in reader")),
            Self::key_line("Esc / q", tr("Volver atrás / Salir del lector", "Go back / Exit reader")),
        ];

        Paragraph::new(left_lines).wrap(Wrap { trim: false }).render(left_col, buf);
        Paragraph::new(right_lines).wrap(Wrap { trim: false }).render(right_col, buf);
    }
}

#[cfg(test)]
// Use in testing
pub fn press_key<T>(page: &mut dyn Component<Actions = T>, key: KeyCode) {
    page.handle_events(Events::Key(key.into()));
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;

    use super::*;

    #[test]
    fn new_chapters_modal_uses_rounded_themed_border_and_compact_height() {
        let state = NewChaptersModalState::new(vec![NewChapterItem {
            manga_id: "mieruko".into(),
            manga_title: "Mieruko-chan".into(),
            provider: MangaProviders::Leercapitulo,
            chapter_numbers: "74".into(),
            chapters: vec![],
            selected: true,
        }]);
        let area = Rect::new(0, 0, 120, 40);
        let mut buf = Buffer::empty(area);

        NewChaptersModal::new(&state).render(area, &mut buf);

        let rendered: String = (0..area.height)
            .map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains('╭'), "modal should use rounded borders");
        assert!(rendered.contains('╰'), "modal should use rounded bottom borders");
        assert!(rendered.contains("Mieruko-chan"));
        assert!(rendered.contains("▸ [✓] Mieruko-chan"));
        assert!(rendered.contains("[leercapitulo]"));

        // Verify the modal is compact rather than spanning 65% height for 1 item
        let top_border_y = (0..area.height)
            .find(|&y| (0..area.width).any(|x| buf[(x, y)].symbol() == "╭"))
            .expect("top border exists");
        let bottom_border_y = (0..area.height)
            .find(|&y| (0..area.width).any(|x| buf[(x, y)].symbol() == "╰"))
            .expect("bottom border exists");
        let modal_height = bottom_border_y - top_border_y + 1;
        assert!(modal_height <= 12, "single-item modal should stay compact, got {modal_height}");

        // Verify themed accent border color
        let top_left_x = (0..area.width)
            .find(|&x| buf[(x, top_border_y)].symbol() == "╭")
            .expect("top-left corner");
        assert_eq!(buf[(top_left_x, top_border_y)].fg, theme().border_accent);
    }

    #[test]
    fn error_modal_renders_rounded_themed_border_and_close_hint() {
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);

        ErrorModal::new("Connection timed out").render(area, &mut buf);

        let rendered: String = (0..area.height)
            .map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains('╭'), "error modal should use rounded borders");
        assert!(rendered.contains('╰'), "error modal should use rounded bottom borders");
        assert!(rendered.contains("Connection timed out"));
        assert!(
            rendered.contains("Pulsa <q> o <Esc> para cerrar") || rendered.contains("Press <q> or <Esc> to close"),
            "error modal should render localized close hint, got:\n{rendered}"
        );

        let top_border_y = (0..area.height)
            .find(|&y| (0..area.width).any(|x| buf[(x, y)].symbol() == "╭"))
            .expect("top border exists");
        let top_left_x = (0..area.width)
            .find(|&x| buf[(x, top_border_y)].symbol() == "╭")
            .expect("top-left corner");
        assert_eq!(buf[(top_left_x, top_border_y)].fg, theme().status_error);
    }
}
