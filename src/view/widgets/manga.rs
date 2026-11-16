use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::symbols::line::THICK;
use ratatui::text::{Line, Span, ToSpan};
use ratatui::widgets::{Block, BorderType, LineGauge, Paragraph, StatefulWidget, Widget, Wrap};
use throbber_widgets_tui::{Throbber, ThrobberState};
use tokio::sync::mpsc::UnboundedSender;
use tui_widget_list::PreRender;

use crate::backend::manga_provider::Chapter;
use crate::global::ERROR_STYLE;
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::display_dates_since_publication;
use crate::view::pages::manga::MangaPageEvents;

#[derive(Clone, Debug, Default)]
pub enum ChapterItemState {
    #[default]
    Normal,
    /// When the user tried to download a chapter and there was an error
    DownloadError,
    /// When the user tried to read a chapter and there was an error
    ReadError,
}

#[derive(Clone, Debug, Default)]
pub struct ChapterItem {
    pub chapter: Chapter,
    pub is_read: bool,
    pub is_downloaded: bool,
    pub is_bookmarked: bool,
    pub is_selected: bool,
    pub state: ChapterItemState,
    pub download_loading_state: Option<f64>,
    pub style: Style,
}

impl Widget for ChapterItem {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let t = theme();
        let is_highlighted = self.style != Style::default();
        if is_highlighted {
            buf.set_style(area, self.style);
        }

        let layout = Layout::horizontal([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Fill(50),
            Constraint::Fill(30),
            Constraint::Fill(20),
        ]);

        let [select_area, is_read_area, is_downloaded_area, title_area, scanlator_area, readable_at_area] = layout.areas(area);

        let select_span = if self.is_selected {
            Span::styled("[✓] ", Style::default().fg(t.status_success).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("[ ] ", Style::default().fg(t.text_dim))
        };
        Line::from(select_span).style(self.style).render(select_area, buf);

        let is_read_icon = if self.is_read { "👀" } else { " " };
        let is_downloaded_icon = if self.is_downloaded { "📥" } else { " " };

        Line::from(is_read_icon).style(self.style).render(is_read_area, buf);
        Line::from(is_downloaded_icon).style(self.style).render(is_downloaded_area, buf);

        let vol_prefix = self
            .chapter
            .volume_number
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty() && *v != "0" && !v.eq_ignore_ascii_case("none"))
            .map(|v| format!("Vol. {v} · "))
            .unwrap_or_default();

        let chapter_num = self.chapter.chapter_number.trim();
        let chapter_tag = if self.is_bookmarked {
            format!("{} | ", tr("Marcado", "Bookmarked"))
        } else if !chapter_num.is_empty() {
            format!("{vol_prefix}{} {chapter_num} | ", tr("Cap.", "Ch."))
        } else {
            vol_prefix
        };

        let title_style = if is_highlighted { self.style } else { Style::default().fg(t.text_primary) };
        let meta_style = if is_highlighted { self.style } else { Style::default().fg(t.text_muted) };

        let chapter_title = self.chapter.title.trim();
        let title_spans = if chapter_title.is_empty() {
            vec![Span::styled(chapter_tag.trim_end_matches(" | ").to_string(), title_style)]
        } else {
            vec![
                Span::styled(
                    chapter_tag,
                    Style::default()
                        .fg(if is_highlighted { t.accent_primary } else { t.accent_highlight })
                        .add_modifier(if is_highlighted { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(chapter_title.to_string(), title_style),
            ]
        };

        Paragraph::new(Line::from(title_spans))
            .wrap(Wrap { trim: true })
            .style(self.style)
            .render(title_area, buf);

        match self.download_loading_state.as_ref() {
            Some(progress) => {
                LineGauge::default()
                    .block(t.rounded_block().title(tr(" Descargando... ", " Downloading... ")))
                    .filled_style(Style::default().fg(t.accent_primary).bg(t.bar_bg).add_modifier(Modifier::BOLD))
                    .line_set(THICK)
                    .ratio(*progress)
                    .render(scanlator_area, buf);
            },
            None => match self.state {
                ChapterItemState::Normal => {
                    Paragraph::new(self.chapter.scanlator.unwrap_or_default())
                        .style(meta_style)
                        .wrap(Wrap { trim: true })
                        .render(scanlator_area, buf);

                    if let Some(date) = self.chapter.publication_date {
                        Paragraph::new(display_dates_since_publication(date))
                            .style(meta_style)
                            .wrap(Wrap { trim: true })
                            .render(readable_at_area, buf);
                    }
                },
                ChapterItemState::DownloadError => {
                    Paragraph::new(
                        tr(
                            "Error al descargar el capítulo, inténtalo de nuevo",
                            "Cannot download this chapter due to an error, please try again",
                        )
                        .to_span()
                        .style(*ERROR_STYLE),
                    )
                    .render(
                        Rect::new(
                            scanlator_area.x,
                            scanlator_area.y,
                            scanlator_area.width + readable_at_area.width,
                            scanlator_area.height,
                        ),
                        buf,
                    );
                },
                ChapterItemState::ReadError => {
                    Paragraph::new(
                        tr(
                            "Error al abrir el capítulo, inténtalo de nuevo",
                            "Cannot read this chapter due to an error, please try again",
                        )
                        .to_span()
                        .style(*ERROR_STYLE),
                    )
                    .render(
                        Rect::new(
                            scanlator_area.x,
                            scanlator_area.y,
                            scanlator_area.width + readable_at_area.width,
                            scanlator_area.height,
                        ),
                        buf,
                    );
                },
            },
        }
    }
}

impl PreRender for ChapterItem {
    fn pre_render(&mut self, context: &tui_widget_list::PreRenderContext) -> u16 {
        if context.is_selected {
            let t = theme();
            self.style = Style::default().fg(t.text_primary).bg(t.row_selected_bg).add_modifier(Modifier::BOLD);
        }

        if self.download_loading_state.is_some() { 3 } else { 1 }
    }
}

impl ChapterItem {
    pub fn new(chapter: Chapter) -> Self {
        Self {
            chapter,
            is_read: false,
            is_downloaded: false,
            is_bookmarked: false,
            is_selected: false,
            download_loading_state: None,
            style: Style::default(),
            state: ChapterItemState::Normal,
        }
    }

    pub fn set_download_error(&mut self) {
        self.download_loading_state = None;
        self.state = ChapterItemState::DownloadError;
    }

    pub fn set_read_error(&mut self) {
        self.state = ChapterItemState::ReadError;
    }

    pub fn set_normal_state(&mut self) {
        self.state = ChapterItemState::Normal;
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChaptersListWidget {
    pub chapters: Vec<ChapterItem>,
}

impl ChaptersListWidget {
    pub fn from_response(response: Vec<Chapter>) -> Self {
        let mut chapters: Vec<ChapterItem> = vec![];

        for chap in response {
            chapters.push(ChapterItem::new(chap));
        }

        Self { chapters }
    }
}

impl StatefulWidget for ChaptersListWidget {
    type State = tui_widget_list::ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let chapters_list = tui_widget_list::List::new(self.chapters);
        StatefulWidget::render(chapters_list, area, buf, state);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DownloadPhase {
    #[default]
    ProccessNotStarted,
    Asking,
    FetchingChaptersData,
    DownloadingChapters,
    AskAbortProcess,
    ErrorChaptersData,
}

#[derive(Debug)]
pub struct DownloadAllChaptersState {
    pub phase: DownloadPhase,
    pub total_chapters: f64,
    pub loader_state: ThrobberState,
    pub download_progress: f64,
    pub download_location: PathBuf,
    pub tx: UnboundedSender<MangaPageEvents>,
    pub is_selected_mode: bool,
}

impl DownloadAllChaptersState {
    pub fn new(tx: UnboundedSender<MangaPageEvents>) -> Self {
        Self {
            phase: DownloadPhase::default(),
            total_chapters: 0.0,
            loader_state: ThrobberState::default(),
            download_progress: 0.0,
            download_location: PathBuf::default(),
            tx,
            is_selected_mode: false,
        }
    }

    pub fn is_downloading(&self) -> bool {
        self.phase == DownloadPhase::DownloadingChapters || self.phase == DownloadPhase::AskAbortProcess
    }

    pub fn process_started(&self) -> bool {
        self.phase != DownloadPhase::ProccessNotStarted
    }

    /// Either phase can start download
    pub fn is_ready_to_fetch_data(&self) -> bool {
        self.phase == DownloadPhase::Asking || self.phase == DownloadPhase::ErrorChaptersData
    }

    pub fn set_download_progress(&mut self) {
        self.download_progress += 1.0;
    }

    pub fn ask_for_confirmation(&mut self) {
        if !self.is_downloading() {
            self.phase = DownloadPhase::Asking;
            self.is_selected_mode = false;
        }
    }

    pub fn ask_for_confirmation_selected(&mut self, total_selected: f64) {
        if !self.is_downloading() {
            self.phase = DownloadPhase::Asking;
            self.is_selected_mode = true;
            self.total_chapters = total_selected;
        }
    }

    pub fn fetch_chapters_data(&mut self) {
        if !self.is_downloading() {
            self.phase = DownloadPhase::FetchingChaptersData;
        }
    }

    pub fn start_download(&mut self) {
        if !self.is_downloading() {
            self.phase = DownloadPhase::DownloadingChapters;
            self.total_chapters = 0.0;
            self.download_progress = 0.0;
        }
    }

    pub fn cancel(&mut self) {
        if !self.is_downloading() {
            self.phase = DownloadPhase::ProccessNotStarted;
            self.is_selected_mode = false;
        }
    }

    pub fn reset(&mut self) {
        self.phase = DownloadPhase::ProccessNotStarted;
        self.total_chapters = 0.0;
        self.download_progress = 0.0;
        self.is_selected_mode = false;
    }

    pub fn ask_abort_proccess(&mut self) {
        if self.is_downloading() {
            self.phase = DownloadPhase::AskAbortProcess;
        }
    }

    pub fn abort_proccess(&mut self) {
        if self.is_downloading() || self.phase == DownloadPhase::AskAbortProcess {
            self.reset();
        }
    }

    pub fn continue_download(&mut self) {
        if self.phase == DownloadPhase::AskAbortProcess {
            self.phase = DownloadPhase::DownloadingChapters;
        }
    }

    pub fn set_total_chapters(&mut self, total_chapters: f64) {
        self.total_chapters = total_chapters;
    }

    pub fn finished_downloading(&self) -> bool {
        self.download_progress == self.total_chapters
    }

    pub fn set_download_error(&mut self) {
        self.phase = DownloadPhase::ErrorChaptersData;
    }

    pub fn set_download_location(&mut self, location: PathBuf) {
        self.download_location = location
    }

    pub fn tick(&mut self) {
        self.loader_state.calc_next();
    }
}

pub struct DownloadAllChaptersWidget<'a> {
    pub manga_title: &'a str,
}

impl<'a> DownloadAllChaptersWidget<'a> {
    pub fn new(manga_title: &'a str) -> Self {
        Self { manga_title }
    }
}

impl<'a> DownloadAllChaptersWidget<'a> {
    fn render_download_information(&mut self, area: Rect, buf: &mut Buffer, state: &mut DownloadAllChaptersState) {
        let t = theme();
        let [information_area, loader_area] = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).areas(area);

        let download_location =
            format!("{}: {}", tr("Carpeta de destino", "Download location"), state.download_location.as_path().display(),);

        let title_text = if state.is_selected_mode {
            tr(
                "Descargando capítulos seleccionados, esto tardará un poco · ",
                "Downloading selected chapters, this will take a while · ",
            )
        } else {
            tr("Descargando todos los capítulos, esto tardará un poco · ", "Downloading all chapters, this will take a while · ")
        };

        Paragraph::new(Line::from(vec![
            Span::styled(title_text, Style::default().fg(t.text_primary).bold()),
            Span::styled(download_location, Style::default().fg(t.text_muted)),
        ]))
        .wrap(Wrap { trim: true })
        .render(information_area, buf);

        let loader = Throbber::default()
            .label(tr("Descarga en curso", "Download in progress"))
            .style(Style::default().fg(t.accent_highlight))
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .use_type(throbber_widgets_tui::WhichUse::Spin);
        StatefulWidget::render(loader, loader_area, buf, &mut state.loader_state);
    }
}

impl<'a> StatefulWidget for DownloadAllChaptersWidget<'a> {
    type State = DownloadAllChaptersState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let t = theme();
        let block = t.rounded_accent_block().title(Line::from(vec![Span::styled(
            format!(" ⬇ {} — {} ", tr("Gestor de descargas", "Download manager"), self.manga_title),
            Style::default().fg(t.accent_primary).bold(),
        )]));
        let download_information_area = block.inner(area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(area, buf);

        match state.phase {
            DownloadPhase::ProccessNotStarted => {},
            DownloadPhase::Asking => {
                let prompt = if state.is_selected_mode {
                    if crate::i18n::is_es() {
                        format!("¿Descargar los {} capítulos seleccionados?  ", state.total_chapters as usize)
                    } else {
                        format!("Download {} selected chapters?  ", state.total_chapters as usize)
                    }
                } else {
                    tr("¿Descargar todos los capítulos?  ", "Download all chapters?  ").to_string()
                };
                let instructions = vec![
                    Span::styled(prompt, Style::default().fg(t.text_primary).bold()),
                    t.key_chip("Enter"),
                    Span::styled(format!(" {}   ", tr("Confirmar", "Yes")), Style::default().fg(t.text_secondary)),
                    t.highlight_key_chip("Esc"),
                    Span::styled(format!(" {}", tr("Cancelar", "No")), Style::default().fg(t.text_secondary)),
                ];

                Paragraph::new(Line::from(instructions))
                    .wrap(Wrap { trim: true })
                    .render(download_information_area, buf);
            },
            DownloadPhase::AskAbortProcess => {
                let instructions = vec![
                    Span::styled(
                        tr("¿Seguro que quieres cancelar la descarga?  ", "Are you sure you want to cancel?  "),
                        Style::default().fg(t.status_warning).bold(),
                    ),
                    t.key_chip("Enter"),
                    Span::styled(format!(" {}   ", tr("Sí, cancelar", "Yes")), Style::default().fg(t.text_secondary)),
                    t.highlight_key_chip("Esc"),
                    Span::styled(format!(" {}", tr("Continuar", "No")), Style::default().fg(t.text_secondary)),
                ];

                Paragraph::new(Line::from(instructions))
                    .wrap(Wrap { trim: true })
                    .render(download_information_area, buf);
            },
            DownloadPhase::FetchingChaptersData => {
                let loader = Throbber::default()
                    .label(tr(
                        "Obteniendo datos de los capítulos antes de descargar...",
                        "Fetching chapter data before downloading...",
                    ))
                    .style(Style::default().fg(t.accent_highlight))
                    .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
                    .use_type(throbber_widgets_tui::WhichUse::Spin);

                StatefulWidget::render(loader, download_information_area, buf, &mut state.loader_state);
            },
            DownloadPhase::ErrorChaptersData => {
                Span::styled(
                    tr(
                        "⚠ No se pudieron obtener los capítulos, pulsa <Enter> para reintentar",
                        "⚠ Could not get chapters data, press <Enter> to try again",
                    ),
                    *ERROR_STYLE,
                )
                .render(download_information_area, buf);
            },
            DownloadPhase::DownloadingChapters => {
                if state.finished_downloading() {
                    state.tx.send(MangaPageEvents::FinishedDownloadingAllChapters).ok();
                    return;
                }

                let [information_area, progress_area] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(download_information_area);

                self.render_download_information(information_area, buf, state);

                let progress_label = if crate::i18n::is_es() {
                    format!(" Capítulos: {} / {} descargados · ", state.download_progress, state.total_chapters)
                } else {
                    format!(" Total chapters: {}, downloaded: {} · ", state.total_chapters, state.download_progress)
                };
                let download_progress_title = vec![
                    Span::styled(progress_label, Style::default().fg(t.text_primary).bold()),
                    t.highlight_key_chip("Esc"),
                    Span::styled(format!(" {} ", tr("Cancelar", "Cancel")), Style::default().fg(t.text_muted)),
                ];

                let ratio =
                    if state.total_chapters > 0.0 { (state.download_progress / state.total_chapters).clamp(0.0, 1.0) } else { 0.0 };

                LineGauge::default()
                    .block(t.rounded_block().title(download_progress_title))
                    .filled_style(Style::default().fg(t.accent_primary).bg(t.bar_bg).add_modifier(Modifier::BOLD))
                    .line_set(THICK)
                    .ratio(ratio)
                    .render(progress_area, buf);
            },
        }
    }
}

#[cfg(test)]
mod test {
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn download_state_works() {
        let (tx, mut rx) = mpsc::unbounded_channel::<MangaPageEvents>();
        let mut download_all_chapters_state = DownloadAllChaptersState::new(tx);

        assert_eq!(DownloadPhase::ProccessNotStarted, download_all_chapters_state.phase);
        assert!(!download_all_chapters_state.process_started());
        assert!(!download_all_chapters_state.is_downloading());

        assert_eq!(0.0, download_all_chapters_state.download_progress);

        download_all_chapters_state.ask_for_confirmation();

        assert_eq!(DownloadPhase::Asking, download_all_chapters_state.phase);

        // The user cancelled
        download_all_chapters_state.cancel();

        assert_eq!(DownloadPhase::ProccessNotStarted, download_all_chapters_state.phase);

        download_all_chapters_state.ask_for_confirmation();

        // The user confirmed
        download_all_chapters_state.fetch_chapters_data();

        assert_eq!(DownloadPhase::FetchingChaptersData, download_all_chapters_state.phase);

        download_all_chapters_state.start_download();

        assert_eq!(DownloadPhase::DownloadingChapters, download_all_chapters_state.phase);

        download_all_chapters_state.set_download_error();

        assert_eq!(DownloadPhase::ErrorChaptersData, download_all_chapters_state.phase);

        download_all_chapters_state.fetch_chapters_data();

        assert_eq!(DownloadPhase::FetchingChaptersData, download_all_chapters_state.phase);

        download_all_chapters_state.start_download();

        download_all_chapters_state.set_total_chapters(3.0);

        assert_eq!(DownloadPhase::DownloadingChapters, download_all_chapters_state.phase);
        assert_eq!(3.0, download_all_chapters_state.total_chapters);

        download_all_chapters_state.set_download_progress();
        download_all_chapters_state.set_download_progress();
        download_all_chapters_state.set_download_progress();

        let area = Rect::new(0, 0, 50, 50);
        let mut buf = Buffer::empty(area);

        StatefulWidget::render(DownloadAllChaptersWidget::new("some_title"), area, &mut buf, &mut download_all_chapters_state);

        let download_finished = rx.recv().await.unwrap();

        assert_eq!(MangaPageEvents::FinishedDownloadingAllChapters, download_finished);
    }
}
