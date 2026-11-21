use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

use crate::backend::cloud::{
    CLOUD_ROOT_FOLDER_NAME, CloudBackendKind, CloudConnectionState, CloudSyncMode, CloudSyncProgress, CloudSyncReport,
    DetectedCloudFolder, detect_installed_cloud_folders,
};
use crate::i18n::tr;
use crate::theme::theme;
use crate::utils::centered_rect;
use crate::view::widgets::updater::format_byte_size;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudActionRow {
    SyncLibrary,
    FullSync,
    PushDownloads,
    PullDownloads,
    LinkGoogleDriveApi,
    LinkLocalCloudFolder,
    Disconnect,
}

impl CloudActionRow {
    pub const ALL: [CloudActionRow; 7] = [
        CloudActionRow::SyncLibrary,
        CloudActionRow::FullSync,
        CloudActionRow::PushDownloads,
        CloudActionRow::PullDownloads,
        CloudActionRow::LinkGoogleDriveApi,
        CloudActionRow::LinkLocalCloudFolder,
        CloudActionRow::Disconnect,
    ];

    pub fn hotkey(self) -> &'static str {
        match self {
            Self::SyncLibrary => "s",
            Self::FullSync => "d",
            Self::PushDownloads => "u",
            Self::PullDownloads => "p",
            Self::LinkGoogleDriveApi => "g",
            Self::LinkLocalCloudFolder => "f",
            Self::Disconnect => "x",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SyncLibrary => tr("🔄 Sincronizar progreso y biblioteca ahora", "🔄 Sync reading progress & library now"),
            Self::FullSync => tr(
                "☁  Sincronización completa (Biblioteca + Capítulos .cbz/.epub/.pdf)",
                "☁  Full sync (Library + Downloaded chapters .cbz/.epub/.pdf)",
            ),
            Self::PushDownloads => {
                tr("⬆  Subir capítulos descargados locales a la nube", "⬆  Upload local downloaded chapters to cloud")
            },
            Self::PullDownloads => {
                tr("⬇  Bajar capítulos de la nube a este dispositivo", "⬇  Download cloud chapters to this device")
            },
            Self::LinkGoogleDriveApi => tr(
                "🔐 Vincular Google Drive API (OAuth 2.0 en navegador · Android/iOS Ready)",
                "🔐 Link Google Drive API (Browser OAuth 2.0 · Android/iOS Ready)",
            ),
            Self::LinkLocalCloudFolder => tr(
                "📁 Autodetectar carpeta de Google Drive Desktop / iCloud Drive",
                "📁 Auto-detect Google Drive Desktop / iCloud Drive folder",
            ),
            Self::Disconnect => tr("🚪 Desvincular cuenta en la nube", "🚪 Unlink cloud account"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::SyncLibrary => tr(
                "Fusiona historial, páginas marcadas (bookmarks), favoritos y Plan to Read vía library_sync.json",
                "Merges reading history, page bookmarks, favorites, and Plan to Read via library_sync.json",
            ),
            Self::FullSync => tr(
                "Sincroniza tu progreso y además sube/descarga capítulos entre mangaDownloads y MangaTUI/downloads",
                "Syncs your progress and uploads/downloads chapter archives between mangaDownloads and MangaTUI/downloads",
            ),
            Self::PushDownloads => tr(
                "Sube a la carpeta MangaTUI/downloads los archivos .cbz, .epub y .pdf que aún no estén en la nube",
                "Uploads local .cbz, .epub, and .pdf files not yet present in MangaTUI/downloads",
            ),
            Self::PullDownloads => tr(
                "Descarga a mangaDownloads los capítulos .cbz, .epub y .pdf de la nube para leerlos 100% offline",
                "Downloads .cbz, .epub, and .pdf chapters from the cloud into mangaDownloads for offline reading",
            ),
            Self::LinkGoogleDriveApi => tr(
                "Abre el login oficial de Google en tu navegador (PKCE). Pulsa <e> para configurar tu Client ID",
                "Opens official Google login in your browser (PKCE). Press <e> to configure your Client ID",
            ),
            Self::LinkLocalCloudFolder => tr(
                "Detecta iCloud Drive, Google Drive Desktop o OneDrive en tu PC (o pulsa <e> para escribir una ruta)",
                "Detects iCloud Drive, Google Drive Desktop, or OneDrive on your PC (or press <e> to type a path)",
            ),
            Self::Disconnect => tr(
                "Borra los tokens OAuth guardados y desvincula la nube actual sin borrar tus mangas locales",
                "Clears stored OAuth tokens and unlinks the current cloud without deleting local mangas",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudInputPopup {
    None,
    GoogleCredentials {
        client_id: String,
        client_secret: String,
        editing_secret: bool,
    },
    CustomFolderPath {
        path_input: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudModalOutcome {
    None,
    Close,
    StartOAuthLogin {
        client_id: String,
        client_secret: Option<String>,
    },
    StartSync(CloudSyncMode),
    CancelRunningTask,
}

#[derive(Debug, Clone)]
pub struct CloudModalState {
    pub selected_index: usize,
    pub connection: CloudConnectionState,
    pub detected_folders: Vec<DetectedCloudFolder>,
    pub detected_cursor: usize,
    pub input_popup: CloudInputPopup,
    pub is_busy: bool,
    pub progress: Option<CloudSyncProgress>,
    pub status_message: Option<String>,
    pub status_is_error: bool,
}

impl Default for CloudModalState {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudModalState {
    pub fn new() -> Self {
        let connection = CloudConnectionState::load();
        let detected_folders = detect_installed_cloud_folders();
        Self {
            selected_index: 0,
            connection,
            detected_folders,
            detected_cursor: 0,
            input_popup: CloudInputPopup::None,
            is_busy: false,
            progress: None,
            status_message: None,
            status_is_error: false,
        }
    }

    pub fn refresh_from_disk(&mut self) {
        self.connection = CloudConnectionState::load();
        self.detected_folders = detect_installed_cloud_folders();
    }

    pub fn selected_row(&self) -> CloudActionRow {
        CloudActionRow::ALL
            .get(self.selected_index)
            .copied()
            .unwrap_or(CloudActionRow::SyncLibrary)
    }

    pub fn is_typing_popup(&self) -> bool {
        !matches!(self.input_popup, CloudInputPopup::None)
    }

    pub fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_message = Some(msg.into());
        self.status_is_error = is_error;
    }

    pub fn on_sync_finished(&mut self, result: Result<CloudSyncReport, String>) {
        self.is_busy = false;
        self.progress = None;
        match result {
            Ok(report) => {
                self.connection.last_sync_at = Some(report.synced_at.clone());
                self.connection.last_sync_report = Some(report.clone());
                let _ = self.connection.save();
                let msg = if crate::i18n::is_es() {
                    format!(
                        "✓ Sincronizado: {} mangas · {} cap. leídos · ↑{} subidos · ↓{} bajados ({})",
                        report.total_mangas,
                        report.total_read_chapters,
                        report.chapters_uploaded,
                        report.chapters_downloaded,
                        format_byte_size(report.bytes_transferred)
                    )
                } else {
                    format!(
                        "✓ Synced: {} mangas · {} read ch. · ↑{} uploaded · ↓{} downloaded ({})",
                        report.total_mangas,
                        report.total_read_chapters,
                        report.chapters_uploaded,
                        report.chapters_downloaded,
                        format_byte_size(report.bytes_transferred)
                    )
                };
                self.set_status(msg, false);
            },
            Err(err) => {
                self.set_status(format!("⚠ {err}"), true);
            },
        }
    }

    pub fn on_oauth_finished(&mut self, result: Result<CloudConnectionState, String>) {
        self.is_busy = false;
        self.progress = None;
        match result {
            Ok(new_state) => {
                self.connection = new_state;
                let summary = self.connection.connection_summary();
                self.set_status(
                    format!("✓ {}: {summary}", tr("Cuenta vinculada correctamente", "Account linked successfully")),
                    false,
                );
            },
            Err(err) => {
                self.set_status(format!("⚠ {err}"), true);
            },
        }
    }

    fn open_google_credentials_popup(&mut self) {
        let existing_id = self.connection.get_client_id().unwrap_or_default();
        let existing_secret = self.connection.get_client_secret().unwrap_or_default();
        self.input_popup = CloudInputPopup::GoogleCredentials {
            client_id: existing_id,
            client_secret: existing_secret,
            editing_secret: false,
        };
    }

    fn open_custom_folder_popup(&mut self) {
        let existing = self
            .connection
            .local_folder_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.input_popup = CloudInputPopup::CustomFolderPath {
            path_input: existing,
        };
    }

    fn link_detected_or_prompt_folder(&mut self) {
        self.detected_folders = detect_installed_cloud_folders();
        if self.detected_folders.is_empty() {
            self.open_custom_folder_popup();
            return;
        }
        let idx = self.detected_cursor % self.detected_folders.len();
        let chosen = self.detected_folders[idx].clone();
        self.detected_cursor = (idx + 1) % self.detected_folders.len();

        let _ = std::fs::create_dir_all(&chosen.manga_tui_folder);
        self.connection.backend = Some(CloudBackendKind::LocalCloudFolder);
        self.connection.local_folder_path = Some(chosen.manga_tui_folder.clone());
        self.connection.local_folder_provider_label = Some(chosen.provider_label.clone());
        let _ = self.connection.save();

        let msg = if crate::i18n::is_es() {
            format!("✓ Vinculado con {} en {}", chosen.provider_label, chosen.manga_tui_folder.display())
        } else {
            format!("✓ Linked with {} at {}", chosen.provider_label, chosen.manga_tui_folder.display())
        };
        self.set_status(msg, false);
    }

    fn execute_row_action(&mut self, row: CloudActionRow) -> CloudModalOutcome {
        match row {
            CloudActionRow::SyncLibrary => {
                if !self.connection.is_connected() {
                    self.set_status(
                        tr(
                            "⚠ Primero vincula Google Drive API <g> o una carpeta en la nube <f>.",
                            "⚠ Please link Google Drive API <g> or a cloud folder <f> first.",
                        ),
                        true,
                    );
                    return CloudModalOutcome::None;
                }
                CloudModalOutcome::StartSync(CloudSyncMode::LibraryOnly)
            },
            CloudActionRow::FullSync => {
                if !self.connection.is_connected() {
                    self.set_status(
                        tr(
                            "⚠ Primero vincula Google Drive API <g> o una carpeta en la nube <f>.",
                            "⚠ Please link Google Drive API <g> or a cloud folder <f> first.",
                        ),
                        true,
                    );
                    return CloudModalOutcome::None;
                }
                CloudModalOutcome::StartSync(CloudSyncMode::FullSync)
            },
            CloudActionRow::PushDownloads => {
                if !self.connection.is_connected() {
                    self.set_status(
                        tr(
                            "⚠ Primero vincula Google Drive API <g> o una carpeta en la nube <f>.",
                            "⚠ Please link Google Drive API <g> or a cloud folder <f> first.",
                        ),
                        true,
                    );
                    return CloudModalOutcome::None;
                }
                CloudModalOutcome::StartSync(CloudSyncMode::PushDownloadsOnly)
            },
            CloudActionRow::PullDownloads => {
                if !self.connection.is_connected() {
                    self.set_status(
                        tr(
                            "⚠ Primero vincula Google Drive API <g> o una carpeta en la nube <f>.",
                            "⚠ Please link Google Drive API <g> or a cloud folder <f> first.",
                        ),
                        true,
                    );
                    return CloudModalOutcome::None;
                }
                CloudModalOutcome::StartSync(CloudSyncMode::PullDownloadsOnly)
            },
            CloudActionRow::LinkGoogleDriveApi => {
                if let Some(client_id) = self.connection.get_client_id() {
                    let client_secret = self.connection.get_client_secret();
                    CloudModalOutcome::StartOAuthLogin {
                        client_id,
                        client_secret,
                    }
                } else {
                    self.open_google_credentials_popup();
                    CloudModalOutcome::None
                }
            },
            CloudActionRow::LinkLocalCloudFolder => {
                self.link_detected_or_prompt_folder();
                CloudModalOutcome::None
            },
            CloudActionRow::Disconnect => {
                self.connection.clear_credentials();
                let _ = self.connection.save();
                self.set_status(tr("✓ Cuenta de la nube desvinculada.", "✓ Cloud account unlinked."), false);
                CloudModalOutcome::None
            },
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> CloudModalOutcome {
        if self.is_busy {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                self.is_busy = false;
                self.progress = None;
                self.set_status(tr("Operación cancelada.", "Operation cancelled."), false);
                return CloudModalOutcome::CancelRunningTask;
            }
            return CloudModalOutcome::None;
        }

        // Handle inline popup inputs if active
        match &mut self.input_popup {
            CloudInputPopup::GoogleCredentials {
                client_id,
                client_secret,
                editing_secret,
            } => {
                match key.code {
                    KeyCode::Esc => {
                        self.input_popup = CloudInputPopup::None;
                    },
                    KeyCode::Tab | KeyCode::Up | KeyCode::Down => {
                        *editing_secret = !*editing_secret;
                    },
                    KeyCode::Backspace => {
                        if *editing_secret {
                            client_secret.pop();
                        } else {
                            client_id.pop();
                        }
                    },
                    KeyCode::Enter => {
                        let cid = client_id.trim().to_string();
                        let csec = client_secret.trim().to_string();
                        if cid.is_empty() {
                            self.set_status(
                                tr(
                                    "⚠ Introduce tu Google OAuth Client ID (termina en .apps.googleusercontent.com)",
                                    "⚠ Please enter your Google OAuth Client ID (ends with .apps.googleusercontent.com)",
                                ),
                                true,
                            );
                            return CloudModalOutcome::None;
                        }
                        self.connection.client_id = Some(cid.clone());
                        let secret_opt = (!csec.is_empty()).then_some(csec);
                        self.connection.fallback_client_secret = secret_opt.clone();
                        let _ = self.connection.save();
                        self.input_popup = CloudInputPopup::None;
                        return CloudModalOutcome::StartOAuthLogin {
                            client_id: cid,
                            client_secret: secret_opt,
                        };
                    },
                    KeyCode::Char(ch) => {
                        if *editing_secret {
                            client_secret.push(ch);
                        } else {
                            client_id.push(ch);
                        }
                    },
                    _ => {},
                }
                return CloudModalOutcome::None;
            },
            CloudInputPopup::CustomFolderPath { path_input } => {
                match key.code {
                    KeyCode::Esc => {
                        self.input_popup = CloudInputPopup::None;
                    },
                    KeyCode::Backspace => {
                        path_input.pop();
                    },
                    KeyCode::Enter => {
                        let raw = path_input.trim().trim_matches('"').to_string();
                        if raw.is_empty() {
                            self.input_popup = CloudInputPopup::None;
                            return CloudModalOutcome::None;
                        }
                        let base = PathBuf::from(&raw);
                        let target = if base.file_name().is_some_and(|n| n == CLOUD_ROOT_FOLDER_NAME) {
                            base
                        } else {
                            base.join(CLOUD_ROOT_FOLDER_NAME)
                        };
                        match std::fs::create_dir_all(&target) {
                            Ok(()) => {
                                self.connection.backend = Some(CloudBackendKind::LocalCloudFolder);
                                self.connection.local_folder_path = Some(target.clone());
                                self.connection.local_folder_provider_label =
                                    Some(tr("Carpeta en la nube", "Cloud Folder").to_string());
                                let _ = self.connection.save();
                                self.input_popup = CloudInputPopup::None;
                                self.set_status(
                                    format!("✓ {}: {}", tr("Carpeta vinculada", "Linked folder"), target.display()),
                                    false,
                                );
                            },
                            Err(e) => {
                                self.set_status(format!("⚠ {e}"), true);
                            },
                        }
                    },
                    KeyCode::Char(ch) => {
                        path_input.push(ch);
                    },
                    _ => {},
                }
                return CloudModalOutcome::None;
            },
            CloudInputPopup::None => {},
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('C') | KeyCode::F(6) => CloudModalOutcome::Close,
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_index = (self.selected_index + 1) % CloudActionRow::ALL.len();
                CloudModalOutcome::None
            },
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_index = (self.selected_index + CloudActionRow::ALL.len() - 1) % CloudActionRow::ALL.len();
                CloudModalOutcome::None
            },
            KeyCode::Enter => {
                let row = self.selected_row();
                self.execute_row_action(row)
            },
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.selected_index = 0;
                self.execute_row_action(CloudActionRow::SyncLibrary)
            },
            KeyCode::Char('d') => {
                self.selected_index = 1;
                self.execute_row_action(CloudActionRow::FullSync)
            },
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.selected_index = 2;
                self.execute_row_action(CloudActionRow::PushDownloads)
            },
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.selected_index = 3;
                self.execute_row_action(CloudActionRow::PullDownloads)
            },
            KeyCode::Char('g') | KeyCode::Char('G') => {
                self.selected_index = 4;
                self.execute_row_action(CloudActionRow::LinkGoogleDriveApi)
            },
            KeyCode::Char('f') | KeyCode::Char('F') => {
                self.selected_index = 5;
                self.execute_row_action(CloudActionRow::LinkLocalCloudFolder)
            },
            KeyCode::Char('x') | KeyCode::Char('X') => {
                self.selected_index = 6;
                self.execute_row_action(CloudActionRow::Disconnect)
            },
            KeyCode::Char('e') | KeyCode::Char('E') => {
                match self.selected_row() {
                    CloudActionRow::LinkLocalCloudFolder => self.open_custom_folder_popup(),
                    _ => self.open_google_credentials_popup(),
                }
                CloudModalOutcome::None
            },
            _ => CloudModalOutcome::None,
        }
    }
}

pub struct CloudModal<'a> {
    state: &'a CloudModalState,
}

impl<'a> CloudModal<'a> {
    pub fn new(state: &'a CloudModalState) -> Self {
        Self { state }
    }
}

impl Widget for CloudModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let modal_area = centered_rect(area, 84, 86);
        Clear.render(modal_area, buf);

        let bottom_bar = Line::from(vec![
            Span::raw(" "),
            t.key_chip("j/k · ↑/↓"),
            Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)),
            t.highlight_key_chip("Enter"),
            Span::styled(tr(" Ejecutar  ", " Run  "), Style::default().fg(t.text_muted)),
            t.key_chip("s"),
            Span::styled(tr(" Sync  ", " Sync  "), Style::default().fg(t.text_muted)),
            t.key_chip("g"),
            Span::styled(" Google API  ", Style::default().fg(t.text_muted)),
            t.key_chip("f"),
            Span::styled(" Drive/iCloud  ", Style::default().fg(t.text_muted)),
            t.key_chip("e"),
            Span::styled(tr(" Configurar  ", " Configure  "), Style::default().fg(t.text_muted)),
            t.key_chip("Esc/q"),
            Span::styled(tr(" Cerrar ", " Close "), Style::default().fg(t.text_muted)),
        ])
        .alignment(Alignment::Center);

        let outer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.accent_primary))
            .style(Style::default().bg(t.bar_bg))
            .title(Line::from(vec![
                Span::styled(
                    tr(" ☁  Nube y Sincronización · Google Drive & iCloud ", " ☁  Cloud Sync · Google Drive & iCloud "),
                    Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tr(" [Compatible Desktop / Android / iOS] ", " [Desktop / Android / iOS Ready] "),
                    Style::default().fg(t.accent_secondary),
                ),
            ]))
            .title_bottom(bottom_bar);

        let inner = outer_block.inner(modal_area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        outer_block.render(modal_area, buf);

        if inner.height < 10 || inner.width < 30 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4), // Status summary card
                Constraint::Min(8),    // Actions list
                Constraint::Length(4), // Description / Live progress / Toast
            ])
            .split(inner);

        // 1. Connection status summary card
        let is_connected = self.state.connection.is_connected();
        let status_dot = if is_connected { "🟢" } else { "⚪" };
        let status_color = if is_connected { t.status_success } else { t.status_warning };
        let summary_text = self.state.connection.connection_summary();

        let last_sync_line = match &self.state.connection.last_sync_report {
            Some(rep) => Line::from(vec![
                Span::styled(format!("  {}: ", tr("Última sincronización", "Last sync")), Style::default().fg(t.text_muted)),
                Span::styled(
                    format!(
                        "{} · {} mangas · {} {} · ↑{} / ↓{}",
                        rep.synced_at,
                        rep.total_mangas,
                        rep.total_read_chapters,
                        tr("cap. leídos", "read ch."),
                        rep.chapters_uploaded,
                        rep.chapters_downloaded
                    ),
                    Style::default().fg(t.text_secondary),
                ),
            ]),
            None => Line::from(vec![
                Span::styled(format!("  {}: ", tr("Estructura en la nube", "Cloud structure")), Style::default().fg(t.text_muted)),
                Span::styled(
                    "MangaTUI/library_sync.json  +  MangaTUI/manga-tui-history.db  +  MangaTUI/downloads/",
                    Style::default().fg(t.text_secondary),
                ),
            ]),
        };

        let status_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_muted))
            .style(Style::default().bg(t.chip_bg));
        let status_inner = status_block.inner(chunks[0]);
        status_block.render(chunks[0], buf);

        let status_lines = vec![
            Line::from(vec![
                Span::raw(format!("  {status_dot} ")),
                Span::styled(format!("{}: ", tr("Estado de la nube", "Cloud status")), Style::default().fg(t.text_muted)),
                Span::styled(summary_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            ]),
            last_sync_line,
        ];
        Paragraph::new(status_lines).render(status_inner, buf);

        // 2. Interactive actions list
        let list_area = chunks[1];
        let row_height = 2u16;
        for (idx, row) in CloudActionRow::ALL.iter().copied().enumerate() {
            let y = list_area.y + (idx as u16) * row_height;
            if y + 1 > list_area.bottom() {
                break;
            }
            let row_rect = Rect::new(list_area.x, y, list_area.width, 1);
            let is_selected = idx == self.state.selected_index;

            if is_selected {
                for x in row_rect.left()..row_rect.right() {
                    buf[(x, row_rect.y)].set_bg(t.row_selected_bg);
                }
            }

            let pointer = if is_selected { " ▸ " } else { "   " };
            let title_style = if is_selected {
                Style::default().fg(t.accent_highlight).bg(t.row_selected_bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.text_primary)
            };

            let line = Line::from(vec![
                Span::styled(
                    pointer,
                    Style::default()
                        .fg(t.accent_primary)
                        .bg(if is_selected { t.row_selected_bg } else { t.bar_bg })
                        .bold(),
                ),
                Span::styled(
                    format!(" {} ", row.hotkey()),
                    Style::default()
                        .fg(Color::Black)
                        .bg(if is_selected { t.accent_primary } else { t.border_muted })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", title_style),
                Span::styled(row.title(), title_style),
            ]);
            Paragraph::new(line).render(row_rect, buf);
        }

        // 3. Description / Progress / Status toast box
        let footer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if self.state.is_busy { t.accent_highlight } else { t.border_muted }))
            .style(Style::default().bg(t.chip_bg));
        let footer_inner = footer_block.inner(chunks[2]);
        footer_block.render(chunks[2], buf);

        let footer_lines = if self.state.is_busy {
            let (label, completed, total) = match &self.state.progress {
                Some(p) => (p.step_label.as_str(), p.completed_items, p.total_items.max(1)),
                None => (tr("Procesando en la nube...", "Processing in the cloud..."), 1, 2),
            };
            let bar_w = (footer_inner.width.saturating_sub(16) as usize).clamp(10, 48);
            let filled = ((completed as f64 / total as f64).clamp(0.0, 1.0) * bar_w as f64).round() as usize;
            let empty = bar_w.saturating_sub(filled);
            let bar_str = format!("  {}{}", "█".repeat(filled), "░".repeat(empty));

            vec![
                Line::from(vec![
                    Span::styled("  ⏳ ", Style::default().fg(t.accent_highlight)),
                    Span::styled(label, Style::default().fg(t.text_primary).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(bar_str, Style::default().fg(t.accent_primary)),
                    Span::styled(
                        format!("  {}", tr("Pulsa <Esc> para cancelar", "Press <Esc> to cancel")),
                        Style::default().fg(t.text_muted),
                    ),
                ]),
            ]
        } else if let Some(msg) = &self.state.status_message {
            let color = if self.state.status_is_error { t.status_error } else { t.status_success };
            vec![
                Line::from(vec![Span::styled(format!("  {msg}"), Style::default().fg(color).add_modifier(Modifier::BOLD))]),
                Line::from(vec![Span::styled(
                    format!("  {}", self.state.selected_row().description()),
                    Style::default().fg(t.text_muted),
                )]),
            ]
        } else {
            vec![
                Line::from(vec![Span::styled(
                    format!("  💡 {}", self.state.selected_row().description()),
                    Style::default().fg(t.text_secondary),
                )]),
                Line::from(vec![Span::styled(
                    format!(
                        "  {}",
                        tr(
                            "CLI equivalente: `manga-tui cloud login`, `manga-tui cloud sync --downloads`, `manga-tui cloud status`",
                            "Equivalent CLI: `manga-tui cloud login`, `manga-tui cloud sync --downloads`, `manga-tui cloud status`"
                        )
                    ),
                    Style::default().fg(t.text_muted),
                )]),
            ]
        };

        Paragraph::new(footer_lines).render(footer_inner, buf);

        // Render inline input popup if open
        match &self.state.input_popup {
            CloudInputPopup::None => {},
            CloudInputPopup::GoogleCredentials {
                client_id,
                client_secret,
                editing_secret,
            } => {
                let popup_area = centered_rect(modal_area, 78, 52);
                Clear.render(popup_area, buf);
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.accent_highlight))
                    .style(Style::default().bg(t.bar_bg))
                    .title(Line::from(vec![Span::styled(
                        tr(
                            " 🔐 Configurar Google Drive OAuth 2.0 (Desktop / Mobile API) ",
                            " 🔐 Configure Google Drive OAuth 2.0 (Desktop / Mobile API) ",
                        ),
                        Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                    )]))
                    .title_bottom(
                        Line::from(vec![
                            Span::raw(" "),
                            t.key_chip("Tab"),
                            Span::styled(tr(" Cambiar campo  ", " Switch field  "), Style::default().fg(t.text_muted)),
                            t.highlight_key_chip("Enter"),
                            Span::styled(
                                tr(" Guardar y abrir navegador  ", " Save & open browser  "),
                                Style::default().fg(t.text_muted),
                            ),
                            t.key_chip("Esc"),
                            Span::styled(tr(" Cancelar ", " Cancel "), Style::default().fg(t.text_muted)),
                        ])
                        .alignment(Alignment::Center),
                    );
                let p_inner = block.inner(popup_area).inner(Margin {
                    horizontal: 2,
                    vertical: 1,
                });
                block.render(popup_area, buf);

                let id_style = if !*editing_secret {
                    Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text_primary)
                };
                let sec_style = if *editing_secret {
                    Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.text_primary)
                };

                let id_cursor = if !*editing_secret { "█" } else { "" };
                let sec_cursor = if *editing_secret { "█" } else { "" };

                let lines = vec![
                    Line::from(Span::styled(
                        tr(
                            "Crea un 'OAuth 2.0 Client ID' (tipo Desktop App) en Google Cloud Console con Google Drive API:",
                            "Create an 'OAuth 2.0 Client ID' (Desktop App) in Google Cloud Console with Google Drive API:",
                        ),
                        Style::default().fg(t.text_muted),
                    )),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("Client ID:     ", Style::default().fg(t.accent_primary).bold()),
                        Span::styled(format!("{client_id}{id_cursor}"), id_style),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled(
                            tr("Client Secret (opcional): ", "Client Secret (optional): "),
                            Style::default().fg(t.accent_secondary).bold(),
                        ),
                        Span::styled(format!("{client_secret}{sec_cursor}"), sec_style),
                    ]),
                ];
                Paragraph::new(lines).render(p_inner, buf);
            },
            CloudInputPopup::CustomFolderPath { path_input } => {
                let popup_area = centered_rect(modal_area, 76, 42);
                Clear.render(popup_area, buf);
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.accent_highlight))
                    .style(Style::default().bg(t.bar_bg))
                    .title(Line::from(vec![Span::styled(
                        tr(" 📁 Ruta de carpeta de Google Drive / iCloud Drive ", " 📁 Google Drive / iCloud Drive Folder Path "),
                        Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                    )]))
                    .title_bottom(
                        Line::from(vec![
                            Span::raw(" "),
                            t.highlight_key_chip("Enter"),
                            Span::styled(tr(" Vincular carpeta  ", " Link folder  "), Style::default().fg(t.text_muted)),
                            t.key_chip("Esc"),
                            Span::styled(tr(" Cancelar ", " Cancel "), Style::default().fg(t.text_muted)),
                        ])
                        .alignment(Alignment::Center),
                    );
                let p_inner = block.inner(popup_area).inner(Margin {
                    horizontal: 2,
                    vertical: 1,
                });
                block.render(popup_area, buf);

                let lines = vec![
                    Line::from(Span::styled(
                        tr(
                            "Escribe la ruta de tu unidad de Google Drive (ej. G:\\Mi unidad) o iCloud Drive:",
                            "Enter the path to your Google Drive (e.g. G:\\My Drive) or iCloud Drive folder:",
                        ),
                        Style::default().fg(t.text_muted),
                    )),
                    Line::default(),
                    Line::from(vec![
                        Span::styled(tr("Ruta: ", "Path: "), Style::default().fg(t.accent_primary).bold()),
                        Span::styled(
                            format!("{path_input}█"),
                            Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                        ),
                    ]),
                ];
                Paragraph::new(lines).render(p_inner, buf);
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    #[test]
    fn cloud_modal_state_navigates_and_handles_google_credentials_popup() {
        let mut state = CloudModalState {
            selected_index: 0,
            connection: CloudConnectionState::default(),
            detected_folders: Vec::new(),
            detected_cursor: 0,
            input_popup: CloudInputPopup::None,
            is_busy: false,
            progress: None,
            status_message: None,
            status_is_error: false,
        };

        assert_eq!(state.selected_row(), CloudActionRow::SyncLibrary);
        state.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(state.selected_row(), CloudActionRow::FullSync);

        // Pressing 'e' opens Google OAuth Credentials popup
        state.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(matches!(state.input_popup, CloudInputPopup::GoogleCredentials { .. }));

        for ch in "12345.apps.googleusercontent.com".chars() {
            state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        state.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        for ch in "my_secret".chars() {
            state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }

        let outcome = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(outcome, CloudModalOutcome::StartOAuthLogin {
            client_id: "12345.apps.googleusercontent.com".to_string(),
            client_secret: Some("my_secret".to_string()),
        });

        let mut buf = Buffer::empty(Rect::new(0, 0, 120, 36));
        CloudModal::new(&state).render(buf.area, &mut buf);
    }
}
