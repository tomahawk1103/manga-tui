use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget, Wrap};

use crate::backend::updater::{AvailableUpdate, BuildIdentity, UpdateChannel};
use crate::i18n::tr;
use crate::theme::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppUpdateStage {
    Available,
    Downloading {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Verifying,
    Applying,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppUpdateModalState {
    pub available: AvailableUpdate,
    pub stage: AppUpdateStage,
}

impl AppUpdateModalState {
    pub fn available(update: AvailableUpdate) -> Self {
        Self {
            available: update,
            stage: AppUpdateStage::Available,
        }
    }
}

pub fn format_byte_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn centered_modal_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width).max(1);
    let h = height.min(area.height).max(1);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

#[derive(Debug)]
pub struct AppUpdateModal<'a> {
    state: &'a AppUpdateModalState,
}

impl<'a> AppUpdateModal<'a> {
    #[inline]
    pub fn new(state: &'a AppUpdateModalState) -> Self {
        Self { state }
    }

    fn render_progress_bar(downloaded: u64, total: Option<u64>, bar_width: usize) -> String {
        let filled = match total {
            Some(t) if t > 0 => ((downloaded as f64 / t as f64).clamp(0.0, 1.0) * bar_width as f64).round() as usize,
            _ => (bar_width / 3).min(bar_width),
        };
        let empty = bar_width.saturating_sub(filled);
        let pct_text = match total {
            Some(t) if t > 0 => {
                let pct = ((downloaded as f64 / t as f64).clamp(0.0, 1.0) * 100.0).round() as u64;
                format!("{pct:>3}% · {} / {}", format_byte_size(downloaded), format_byte_size(t))
            },
            _ => format_byte_size(downloaded),
        };
        format!("{}{}  {pct_text}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl<'a> Widget for AppUpdateModal<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let update = &self.state.available;
        let current = BuildIdentity::current();

        let target_width = (area.width.saturating_mul(72) / 100).clamp(54.min(area.width), area.width.saturating_sub(4).max(1));
        let desired_height = 17.min(area.height.saturating_sub(2).max(10)).min(area.height.max(1));
        let modal_area = centered_modal_rect(area, target_width, desired_height);
        Clear.render(modal_area, buf);

        let bottom_hints = match &self.state.stage {
            AppUpdateStage::Available => Line::from(vec![
                Span::raw(" "),
                t.highlight_key_chip("Enter / y"),
                Span::styled(tr(" Actualizar  │  ", " Update  │  "), Style::default().fg(t.text_muted)),
                t.key_chip("b"),
                Span::styled(" GitHub  │  ", Style::default().fg(t.text_muted)),
                t.key_chip("q / Esc"),
                Span::styled(tr(" Más tarde ", " Later "), Style::default().fg(t.text_muted)),
            ]),
            AppUpdateStage::Downloading { .. } => Line::from(vec![
                Span::raw(" "),
                t.key_chip("Esc / q"),
                Span::styled(tr(" Cancelar ", " Cancel "), Style::default().fg(t.text_muted)),
            ]),
            AppUpdateStage::Verifying | AppUpdateStage::Applying => Line::from(vec![Span::styled(
                tr(" Por favor espera... ", " Please wait... "),
                Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
            )]),
            AppUpdateStage::Error(_) => Line::from(vec![
                Span::raw(" "),
                t.highlight_key_chip("r / Enter"),
                Span::styled(tr(" Reintentar  │  ", " Retry  │  "), Style::default().fg(t.text_muted)),
                t.key_chip("b"),
                Span::styled(" GitHub  │  ", Style::default().fg(t.text_muted)),
                t.key_chip("q / Esc"),
                Span::styled(tr(" Cerrar ", " Close "), Style::default().fg(t.text_muted)),
            ]),
        }
        .alignment(Alignment::Center);

        let block = t
            .rounded_accent_block()
            .title(Line::from(vec![Span::styled(
                tr(" ⬆  Actualización disponible para Manga TUI ", " ⬆  Manga TUI Update Available "),
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
            )]))
            .title_bottom(bottom_hints);

        let inner = block.inner(modal_area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(modal_area, buf);

        if inner.height < 4 || inner.width < 12 {
            return;
        }

        let current_label = if current.commit_short != "unknown" {
            format!("v{} ({})", current.version, current.commit_short)
        } else {
            format!("v{}", current.version)
        };

        let channel_bg = match update.channel {
            UpdateChannel::Stable => t.status_success,
            UpdateChannel::Nightly => t.accent_secondary,
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!(" {}: ", tr("Versión actual", "Current version")), Style::default().fg(t.text_muted)),
                Span::styled(current_label, Style::default().fg(t.text_primary)),
                Span::styled("   →   ", Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}: ", tr("Nueva", "New")), Style::default().fg(t.text_muted)),
                Span::styled(update.display_version.as_str(), Style::default().fg(t.status_success).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(
                    format!(" {} ", update.channel.label()),
                    Style::default().fg(Color::Black).bg(channel_bg).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(format!(" {}: ", tr("Modo", "Mode")), Style::default().fg(t.text_muted)),
                Span::styled(update.install_kind.label(), Style::default().fg(t.text_secondary)),
                Span::styled("  │  ", Style::default().fg(t.text_dim)),
                Span::styled(format!("{}: ", tr("Paquete", "Package")), Style::default().fg(t.text_muted)),
                Span::styled(
                    update
                        .download_asset
                        .as_ref()
                        .map(|a| if a.size > 0 { format!("{} ({})", a.name, format_byte_size(a.size)) } else { a.name.clone() })
                        .unwrap_or_else(|| tr("No disponible (abrir GitHub)", "Unavailable (open GitHub)").to_string()),
                    Style::default().fg(t.accent_primary),
                ),
            ]),
            Line::raw(""),
        ];

        match &self.state.stage {
            AppUpdateStage::Available => {
                if update.release_notes.trim().is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        tr(
                            "Pulsa <Enter> o <y> para descargar, verificar SHA-256 e instalar automáticamente.",
                            "Press <Enter> or <y> to download, verify SHA-256, and install automatically.",
                        ),
                        Style::default().fg(t.text_secondary),
                    )]));
                } else {
                    lines.push(Line::from(vec![Span::styled(
                        tr("Novedades:", "Release notes:"),
                        Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                    )]));
                    for note_line in update.release_notes.lines().take(4) {
                        lines.push(Line::from(vec![Span::styled(format!("  {note_line}"), Style::default().fg(t.text_secondary))]));
                    }
                }
            },
            AppUpdateStage::Downloading {
                downloaded_bytes,
                total_bytes,
            } => {
                let bar_w = (inner.width as usize).saturating_sub(28).clamp(12, 34);
                let progress_line = Self::render_progress_bar(*downloaded_bytes, *total_bytes, bar_w);
                lines.push(Line::from(vec![Span::styled(
                    tr("⬇ Descargando paquete de actualización...", "⬇ Downloading update package..."),
                    Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    progress_line,
                    Style::default().fg(t.status_success).add_modifier(Modifier::BOLD),
                )]));
            },
            AppUpdateStage::Verifying => {
                lines.push(Line::from(vec![Span::styled(
                    tr("🔐 Verificando integridad SHA-256...", "🔐 Verifying SHA-256 integrity..."),
                    Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                )]));
            },
            AppUpdateStage::Applying => {
                lines.push(Line::from(vec![Span::styled(
                    tr("⚙ Instalando actualización y reiniciando Manga TUI...", "⚙ Installing update and restarting Manga TUI..."),
                    Style::default().fg(t.status_success).add_modifier(Modifier::BOLD),
                )]));
            },
            AppUpdateStage::Error(err) => {
                lines.push(Line::from(vec![Span::styled(
                    tr("⚠ Error al actualizar:", "⚠ Update error:"),
                    Style::default().fg(t.status_error).add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(vec![Span::styled(err.as_str(), Style::default().fg(t.status_warning))]));
            },
        }

        Paragraph::new(lines).wrap(Wrap { trim: true }).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::updater::{InstallKind, ReleaseAsset};

    fn sample_update() -> AvailableUpdate {
        AvailableUpdate {
            tag_name: "nightly".into(),
            display_version: "nightly (v0.2.0 · abc1234)".into(),
            channel: UpdateChannel::Nightly,
            commit_short: Some("abc1234".into()),
            published_at: Some("2026-11-21T14:00:00Z".into()),
            release_notes: "- Fix auto-update modal\n- Faster download queue".into(),
            html_url: "https://github.com/tomahawk1103/manga-tui/releases/tag/nightly".into(),
            install_kind: InstallKind::WindowsInstaller,
            download_asset: Some(ReleaseAsset {
                name: "manga-tui-nightly-x86_64-pc-windows-msvc-setup.exe".into(),
                download_url: "https://example.invalid/setup.exe".into(),
                size: 6_600_000,
            }),
            sha256_asset: None,
            checksum_txt_asset: None,
        }
    }

    fn render_modal(state: &AppUpdateModalState) -> String {
        let area = Rect::new(0, 0, 120, 40);
        let mut buf = Buffer::empty(area);
        AppUpdateModal::new(state).render(area, &mut buf);
        (0..area.height)
            .map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn app_update_modal_renders_all_stages_cleanly() {
        let mut state = AppUpdateModalState::available(sample_update());
        let avail = render_modal(&state);
        assert!(avail.contains("nightly (v0.2.0"));
        assert!(avail.contains("Fix auto-update modal"));
        assert!(avail.contains("6.3 MB"));

        state.stage = AppUpdateStage::Downloading {
            downloaded_bytes: 3_300_000,
            total_bytes: Some(6_600_000),
        };
        let dl = render_modal(&state);
        assert!(dl.contains("50%"));
        assert!(dl.contains("█"));

        state.stage = AppUpdateStage::Verifying;
        let ver = render_modal(&state);
        assert!(ver.contains("SHA-256"));

        state.stage = AppUpdateStage::Applying;
        let app = render_modal(&state);
        assert!(app.contains("Manga TUI"));

        state.stage = AppUpdateStage::Error("Checksum mismatch".into());
        let err = render_modal(&state);
        assert!(err.contains("Checksum mismatch"));

        assert_eq!(format_byte_size(512), "512 B");
        assert_eq!(format_byte_size(2048), "2 KB");
        assert_eq!(format_byte_size(3 * 1024 * 1024), "3.0 MB");
    }
}
