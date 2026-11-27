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
use crate::backend::tracker::myanimelist::{MAL_APICONFIG_URL, MalConnectionState, MalSyncReport};
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
    LinkMyAnimeList,
    SyncMyAnimeList,
    Disconnect,
}

impl CloudActionRow {
    pub const ALL: [CloudActionRow; 9] = [
        CloudActionRow::SyncLibrary,
        CloudActionRow::FullSync,
        CloudActionRow::PushDownloads,
        CloudActionRow::PullDownloads,
        CloudActionRow::LinkGoogleDriveApi,
        CloudActionRow::LinkLocalCloudFolder,
        CloudActionRow::LinkMyAnimeList,
        CloudActionRow::SyncMyAnimeList,
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
            Self::LinkMyAnimeList => "m",
            Self::SyncMyAnimeList => "a",
            Self::Disconnect => "x",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SyncLibrary => tr("🔄 Sincronizar progreso y biblioteca", "🔄 Sync reading progress & library"),
            Self::FullSync => {
                tr("☁  Sincronizar todo (Progreso + Capítulos descargados)", "☁  Sync everything (Progress + Downloaded chapters)")
            },
            Self::PushDownloads => tr("⬆  Subir mis capítulos descargados a la nube", "⬆  Upload downloaded chapters to cloud"),
            Self::PullDownloads => tr("⬇  Descargar capítulos guardados en la nube", "⬇  Download chapters saved in the cloud"),
            Self::LinkGoogleDriveApi => tr("🌐 Iniciar sesión con Google Drive", "🌐 Sign in with Google Drive"),
            Self::LinkLocalCloudFolder => {
                tr("📁 Conectar carpeta de iCloud Drive / Google Drive del PC", "📁 Connect iCloud Drive / Google Drive PC folder")
            },
            Self::LinkMyAnimeList => tr("🌐 Iniciar sesión con MyAnimeList", "🌐 Sign in with MyAnimeList"),
            Self::SyncMyAnimeList => {
                tr("📲 Sincronizar lista con MyAnimeList (Móvil ⇄ PC)", "📲 Sync manga list with MyAnimeList (Mobile ⇄ PC)")
            },
            Self::Disconnect => tr("🚪 Cerrar sesión en la nube / MyAnimeList", "🚪 Sign out from cloud / MyAnimeList"),
        }
    }

    pub fn description_for(self, has_google_ready: bool, has_mal_ready: bool) -> &'static str {
        match self {
            Self::SyncLibrary => tr(
                "Mantiene al día tu historial, la página exacta donde te quedaste, favoritos y Plan to Read",
                "Keeps your reading history, exact bookmarked page, favorites, and Plan to Read up to date",
            ),
            Self::FullSync => tr(
                "Sincroniza tu progreso y además copia tus capítulos (.cbz, .epub, .pdf) entre este PC y la nube",
                "Syncs your progress and copies your chapters (.cbz, .epub, .pdf) between this PC and the cloud",
            ),
            Self::PushDownloads => tr(
                "Sube a tu nube los capítulos descargados en este PC para poder leerlos en otros dispositivos",
                "Uploads downloaded chapters from this PC to your cloud so you can read them on other devices",
            ),
            Self::PullDownloads => tr(
                "Descarga a este PC los capítulos que ya tengas guardados en la nube para leer sin conexión",
                "Downloads chapters already saved in the cloud to this PC for offline reading",
            ),
            Self::LinkGoogleDriveApi => {
                if has_google_ready {
                    tr(
                        "Abre Google en tu navegador para vincular tu cuenta en 1 clic",
                        "Opens Google in your browser to link your account in 1 click",
                    )
                } else {
                    tr(
                        "Inicia sesión en Google Drive desde el navegador (en compilación manual pide Client ID; o usa <f> para iCloud/Drive local)",
                        "Sign in to Google Drive in your browser (manual builds ask for a Client ID; or use <f> for local iCloud/Drive)",
                    )
                }
            },
            Self::LinkLocalCloudFolder => tr(
                "Detecta iCloud/OneDrive o carpeta local (si usas Google Drive en móvil/web, usa <g> para evitar carpetas MangaTUI (1) duplicadas)",
                "Detects iCloud/OneDrive or local folder (if syncing Google Drive with mobile/web, use <g> to avoid duplicate MangaTUI (1) folders)",
            ),
            Self::LinkMyAnimeList => {
                if has_mal_ready {
                    tr(
                        "Abre MyAnimeList en tu navegador para iniciar sesión con tu cuenta en 1 clic",
                        "Opens MyAnimeList in your browser to sign in with your account in 1 click",
                    )
                } else {
                    tr(
                        "Inicia sesión en MyAnimeList desde el navegador (en compilación manual pide Client ID)",
                        "Sign in to MyAnimeList in your browser (manual builds ask for a Client ID)",
                    )
                }
            },
            Self::SyncMyAnimeList => tr(
                "Importa tus mangas del móvil (Reading / Plan to Read) y sube a MyAnimeList tu progreso de capítulos leídos",
                "Imports your mobile mangas (Reading / Plan to Read) and pushes your read chapter progress to MyAnimeList",
            ),
            Self::Disconnect => tr(
                "Cierra la sesión en la nube y MyAnimeList en este dispositivo sin borrar tus mangas ni tu historial local",
                "Signs out of the cloud and MyAnimeList on this device without deleting your local mangas or reading history",
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
    MalCredentials {
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
    StartMalOAuthLogin {
        client_id: String,
        client_secret: Option<String>,
    },
    StartSync(CloudSyncMode),
    StartMalSync,
    CancelRunningTask,
}

#[derive(Debug, Clone)]
pub struct CloudModalState {
    pub selected_index: usize,
    pub connection: CloudConnectionState,
    pub mal_connection: MalConnectionState,
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
        let mal_connection = MalConnectionState::load();
        let detected_folders = detect_installed_cloud_folders();
        let selected_index = if connection.is_connected() {
            0
        } else if mal_connection.is_connected() {
            7
        } else {
            4
        };
        Self {
            selected_index,
            connection,
            mal_connection,
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
        self.mal_connection = MalConnectionState::load();
        self.detected_folders = detect_installed_cloud_folders();
        self.selected_index = if self.connection.is_connected() {
            0
        } else if self.mal_connection.is_connected() {
            7
        } else {
            4
        };
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

    pub fn on_mal_oauth_finished(&mut self, result: Result<MalConnectionState, String>) {
        self.is_busy = false;
        self.progress = None;
        match result {
            Ok(new_state) => {
                self.mal_connection = new_state;
                let summary = self.mal_connection.connection_summary();
                self.set_status(
                    format!("✓ {}: {summary}", tr("Sesión iniciada en MyAnimeList", "Signed in to MyAnimeList")),
                    false,
                );
            },
            Err(err) => {
                self.set_status(format!("⚠ {err}"), true);
            },
        }
    }

    pub fn on_mal_sync_finished(&mut self, result: Result<MalSyncReport, String>) {
        self.is_busy = false;
        self.progress = None;
        match result {
            Ok(report) => {
                self.mal_connection.last_sync_at = Some(report.synced_at.clone());
                self.mal_connection.last_sync_report = Some(report.clone());
                let _ = self.mal_connection.save();
                let msg = if crate::i18n::is_es() {
                    format!(
                        "✓ MyAnimeList sincronizado: {} mangas en lista · ↓{} importados al PC · ↑{} actualizados en MAL",
                        report.total_mal_entries, report.imported_from_mal, report.pushed_to_mal
                    )
                } else {
                    format!(
                        "✓ MyAnimeList synced: {} list entries · ↓{} imported to PC · ↑{} updated on MAL",
                        report.total_mal_entries, report.imported_from_mal, report.pushed_to_mal
                    )
                };
                self.set_status(msg, false);
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

    fn open_mal_credentials_popup(&mut self) {
        let existing_id = self.mal_connection.get_client_id().unwrap_or_default();
        let existing_secret = self.mal_connection.get_client_secret().unwrap_or_default();
        self.input_popup = CloudInputPopup::MalCredentials {
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
                            "⚠ Primero inicia sesión con Google <g> o conecta una carpeta <f>.",
                            "⚠ Please sign in with Google <g> or connect a folder <f> first.",
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
                            "⚠ Primero inicia sesión con Google <g> o conecta una carpeta <f>.",
                            "⚠ Please sign in with Google <g> or connect a folder <f> first.",
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
                            "⚠ Primero inicia sesión con Google <g> o conecta una carpeta <f>.",
                            "⚠ Please sign in with Google <g> or connect a folder <f> first.",
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
                            "⚠ Primero inicia sesión con Google <g> o conecta una carpeta <f>.",
                            "⚠ Please sign in with Google <g> or connect a folder <f> first.",
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
            CloudActionRow::LinkMyAnimeList => {
                if let Some(client_id) = self.mal_connection.get_client_id() {
                    let client_secret = self.mal_connection.get_client_secret();
                    CloudModalOutcome::StartMalOAuthLogin {
                        client_id,
                        client_secret,
                    }
                } else {
                    self.open_mal_credentials_popup();
                    CloudModalOutcome::None
                }
            },
            CloudActionRow::SyncMyAnimeList => {
                if !self.mal_connection.is_connected() {
                    self.set_status(
                        tr(
                            "⚠ Primero inicia sesión en MyAnimeList pulsando <m>.",
                            "⚠ Please sign in to MyAnimeList first by pressing <m>.",
                        ),
                        true,
                    );
                    return CloudModalOutcome::None;
                }
                CloudModalOutcome::StartMalSync
            },
            CloudActionRow::Disconnect => {
                self.connection.clear_credentials();
                let _ = self.connection.save();
                self.mal_connection.clear_credentials();
                let _ = self.mal_connection.save();
                self.set_status(
                    tr(
                        "✓ Cuentas de la nube y MyAnimeList desvinculadas de forma segura.",
                        "✓ Cloud and MyAnimeList accounts unlinked safely.",
                    ),
                    false,
                );
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
                                    "⚠ Introduce un Client ID válido (termina en .apps.googleusercontent.com)",
                                    "⚠ Enter a valid Client ID (ends with .apps.googleusercontent.com)",
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
            CloudInputPopup::MalCredentials {
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
                    KeyCode::F(1) => {
                        #[cfg(not(test))]
                        let _ = open::that(MAL_APICONFIG_URL);
                        self.set_status(
                            tr(
                                "🌐 Abriendo https://myanimelist.net/apiconfig en tu navegador...",
                                "🌐 Opening https://myanimelist.net/apiconfig in your browser...",
                            ),
                            false,
                        );
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
                                    "⚠ Introduce tu Client ID personal de MyAnimeList (pulsa F1 para crearlo en 1 min)",
                                    "⚠ Enter your personal MyAnimeList Client ID (press F1 to create one in 1 min)",
                                ),
                                true,
                            );
                            return CloudModalOutcome::None;
                        }
                        self.mal_connection.client_id = Some(cid.clone());
                        let secret_opt = (!csec.is_empty()).then_some(csec);
                        self.mal_connection.fallback_client_secret = secret_opt.clone();
                        let _ = self.mal_connection.save();
                        self.input_popup = CloudInputPopup::None;
                        return CloudModalOutcome::StartMalOAuthLogin {
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
            KeyCode::Char('m') | KeyCode::Char('M') => {
                self.selected_index = 6;
                self.execute_row_action(CloudActionRow::LinkMyAnimeList)
            },
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.selected_index = 7;
                self.execute_row_action(CloudActionRow::SyncMyAnimeList)
            },
            KeyCode::Char('x') | KeyCode::Char('X') => {
                self.selected_index = 8;
                self.execute_row_action(CloudActionRow::Disconnect)
            },
            KeyCode::Char('e') | KeyCode::Char('E') => {
                match self.selected_row() {
                    CloudActionRow::LinkLocalCloudFolder => self.open_custom_folder_popup(),
                    CloudActionRow::LinkMyAnimeList | CloudActionRow::SyncMyAnimeList => self.open_mal_credentials_popup(),
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

        let has_google_ready = self.state.connection.get_client_id().is_some();
        let has_mal_ready = self.state.mal_connection.get_client_id().is_some();

        let bottom_bar = Line::from(vec![
            Span::raw(" "),
            t.key_chip("j/k · ↑/↓"),
            Span::styled(tr(" Mover  ", " Move  "), Style::default().fg(t.text_muted)),
            t.highlight_key_chip("Enter"),
            Span::styled(tr(" Elegir  ", " Select  "), Style::default().fg(t.text_muted)),
            t.key_chip("m"),
            Span::styled(" MyAnimeList  ", Style::default().fg(t.text_muted)),
            t.key_chip("g"),
            Span::styled(" Google  ", Style::default().fg(t.text_muted)),
            t.key_chip("f"),
            Span::styled(" iCloud/Drive  ", Style::default().fg(t.text_muted)),
            t.key_chip("Esc/q"),
            Span::styled(tr(" Cerrar ", " Close "), Style::default().fg(t.text_muted)),
        ])
        .alignment(Alignment::Center);

        let outer_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.accent_primary))
            .style(Style::default().bg(t.bar_bg))
            .title(Line::from(vec![Span::styled(
                tr(
                    " ☁  Nube y Sincronización · Google Drive, iCloud & MyAnimeList ",
                    " ☁  Cloud & Sync · Google Drive, iCloud & MyAnimeList ",
                ),
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
            )]))
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
            None => {
                if is_connected {
                    Line::from(vec![
                        Span::styled(format!("  {}: ", tr("Listo para usar", "Ready")), Style::default().fg(t.text_muted)),
                        Span::styled(
                            tr(
                                "Pulsa <s> para sincronizar tu progreso o <d> para incluir descargas",
                                "Press <s> to sync your progress or <d> to include downloads",
                            ),
                            Style::default().fg(t.text_secondary),
                        ),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {}: ", tr("Primer paso", "First step")), Style::default().fg(t.text_muted)),
                        Span::styled(
                            tr(
                                "Pulsa <g> (o Enter) para iniciar sesión con Google, o <f> para iCloud/Drive del PC",
                                "Press <g> (or Enter) to sign in with Google, or <f> for PC iCloud/Drive",
                            ),
                            Style::default().fg(t.text_secondary),
                        ),
                    ])
                }
            },
        };

        let status_block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_muted))
            .style(Style::default().bg(t.chip_bg));
        let status_inner = status_block.inner(chunks[0]);
        status_block.render(chunks[0], buf);

        let mal_connected = self.state.mal_connection.is_connected();
        let mal_dot = if mal_connected { "🟢" } else { "⚪" };
        let mal_color = if mal_connected { t.status_success } else { t.text_muted };
        let mal_summary = self.state.mal_connection.connection_summary();

        let status_lines = vec![
            Line::from(vec![
                Span::raw(format!("  {status_dot} ")),
                Span::styled(format!("{}: ", tr("Nube", "Cloud")), Style::default().fg(t.text_muted)),
                Span::styled(summary_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                Span::styled("   │   ", Style::default().fg(t.border_muted)),
                Span::raw(format!("{mal_dot} ")),
                Span::styled("MyAnimeList: ", Style::default().fg(t.text_muted)),
                Span::styled(mal_summary, Style::default().fg(mal_color).add_modifier(Modifier::BOLD)),
            ]),
            last_sync_line,
        ];
        Paragraph::new(status_lines).render(status_inner, buf);

        // 2. Interactive actions list (adapts row spacing automatically to terminal height so all 9 rows always fit)
        let list_area = chunks[1];
        let total_rows = CloudActionRow::ALL.len() as u16;
        let row_height = if list_area.height >= total_rows * 2 { 2u16 } else { 1u16 };
        let visible_capacity = (list_area.height / row_height).max(1) as usize;
        let scroll_offset =
            if self.state.selected_index >= visible_capacity { self.state.selected_index + 1 - visible_capacity } else { 0 };
        for (visible_idx, (idx, row)) in CloudActionRow::ALL
            .iter()
            .copied()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_capacity)
            .enumerate()
        {
            let y = list_area.y + (visible_idx as u16) * row_height;
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
                    format!("  {}", self.state.selected_row().description_for(has_google_ready, has_mal_ready)),
                    Style::default().fg(t.text_muted),
                )]),
            ]
        } else {
            let tip_line = if has_google_ready {
                tr(
                    "  Tus mangas y progreso locales nunca se borran; al sincronizar se combinan ambos dispositivos.",
                    "  Your local mangas and progress are never deleted; syncing merges both devices safely.",
                )
            } else {
                tr(
                    "  ¿Prefieres 1 clic sin claves? Pulsa <f> para usar tu carpeta de iCloud Drive o Google Drive del PC.",
                    "  Prefer 1-click with no keys? Press <f> to use your PC's iCloud Drive or Google Drive folder.",
                )
            };
            vec![
                Line::from(vec![Span::styled(
                    format!("  💡 {}", self.state.selected_row().description_for(has_google_ready, has_mal_ready)),
                    Style::default().fg(t.text_secondary),
                )]),
                Line::from(vec![Span::styled(tip_line, Style::default().fg(t.text_muted))]),
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
                let popup_area = centered_rect(modal_area, 82, 58);
                Clear.render(popup_area, buf);
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.accent_highlight))
                    .style(Style::default().bg(t.bar_bg))
                    .title(Line::from(vec![Span::styled(
                        tr(" ⚙ Configuración avanzada: Google Drive manual ", " ⚙ Advanced setup: Manual Google Drive "),
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
                            "Build sin credenciales de Google integradas (solo para compilar a mano).",
                            "This build has no bundled Google credentials (only for manual builds).",
                        ),
                        Style::default().fg(t.text_muted),
                    )),
                    Line::from(Span::styled(
                        tr(
                            "Consejo: pulsa <Esc> y luego <f> para iCloud o Google Drive en 1 clic.",
                            "Tip: press <Esc> then <f> for 1-click iCloud or Google Drive folder sync.",
                        ),
                        Style::default().fg(t.accent_secondary),
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
            CloudInputPopup::MalCredentials {
                client_id,
                client_secret,
                editing_secret,
            } => {
                let popup_area = centered_rect(modal_area, 86, 68);
                Clear.render(popup_area, buf);
                let block = Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.accent_highlight))
                    .style(Style::default().bg(t.bar_bg))
                    .title(Line::from(vec![Span::styled(
                        tr(" ⚙ Configuración avanzada: MyAnimeList manual ", " ⚙ Advanced setup: Manual MyAnimeList "),
                        Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD),
                    )]))
                    .title_bottom(
                        Line::from(vec![
                            Span::raw(" "),
                            t.key_chip("F1"),
                            Span::styled(
                                tr(" Abrir myanimelist.net/apiconfig  ", " Open myanimelist.net/apiconfig  "),
                                Style::default().fg(t.text_muted),
                            ),
                            t.key_chip("Tab"),
                            Span::styled(tr(" Cambiar campo  ", " Switch field  "), Style::default().fg(t.text_muted)),
                            t.highlight_key_chip("Enter"),
                            Span::styled(
                                tr(" Autorizar en navegador  ", " Authorize in browser  "),
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
                            "Build sin credenciales de MyAnimeList integradas (solo para compilar a mano o usar tu propio ID).",
                            "This build has no bundled MyAnimeList credentials (only for manual builds or using your own ID).",
                        ),
                        Style::default().fg(t.text_muted),
                    )),
                    Line::from(Span::styled(
                        tr(
                            "1. Pulsa <F1> para abrir https://myanimelist.net/apiconfig y haz clic en 'Create ID'.",
                            "1. Press <F1> to open https://myanimelist.net/apiconfig and click 'Create ID'.",
                        ),
                        Style::default().fg(t.text_secondary),
                    )),
                    Line::from(Span::styled(
                        tr(
                            "2. En 'App Type' elige 'other' y en 'App Redirect URL' pon: http://127.0.0.1:8765/callback",
                            "2. Set 'App Type' to 'other' and 'App Redirect URL' to: http://127.0.0.1:8765/callback",
                        ),
                        Style::default().fg(t.accent_secondary),
                    )),
                    Line::from(Span::styled(
                        tr(
                            "3. Pega el Client ID abajo y pulsa <Enter> para iniciar sesión en tu navegador:",
                            "3. Paste the Client ID below and press <Enter> to sign in via your browser:",
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
                            tr("Client Secret (opcional si elegiste 'other'): ", "Client Secret (optional if 'other'): "),
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
            mal_connection: MalConnectionState::default(),
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

    #[test]
    fn cloud_modal_starts_one_click_google_login_when_client_id_is_configured() {
        let mut state = CloudModalState {
            selected_index: 4,
            connection: CloudConnectionState {
                client_id: Some("bundled-client.apps.googleusercontent.com".to_string()),
                ..Default::default()
            },
            mal_connection: MalConnectionState::default(),
            detected_folders: Vec::new(),
            detected_cursor: 0,
            input_popup: CloudInputPopup::None,
            is_busy: false,
            progress: None,
            status_message: None,
            status_is_error: false,
        };

        assert_eq!(state.selected_row(), CloudActionRow::LinkGoogleDriveApi);
        let outcome = state.handle_key_event(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(outcome, CloudModalOutcome::StartOAuthLogin {
            client_id: "bundled-client.apps.googleusercontent.com".to_string(),
            client_secret: None,
        });
        assert!(matches!(state.input_popup, CloudInputPopup::None));

        let mut buf = Buffer::empty(Rect::new(0, 0, 120, 36));
        CloudModal::new(&state).render(buf.area, &mut buf);
    }

    #[test]
    fn cloud_modal_starts_one_click_mal_login_when_client_id_is_configured() {
        let mut state = CloudModalState {
            selected_index: 6,
            connection: CloudConnectionState::default(),
            mal_connection: MalConnectionState {
                client_id: Some("bundled_mal_client_id".to_string()),
                ..Default::default()
            },
            detected_folders: Vec::new(),
            detected_cursor: 0,
            input_popup: CloudInputPopup::None,
            is_busy: false,
            progress: None,
            status_message: None,
            status_is_error: false,
        };

        assert_eq!(state.selected_row(), CloudActionRow::LinkMyAnimeList);
        let outcome = state.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(outcome, CloudModalOutcome::StartMalOAuthLogin {
            client_id: "bundled_mal_client_id".to_string(),
            client_secret: None,
        });
        assert!(matches!(state.input_popup, CloudInputPopup::None));

        // Pressing 'e' still opens manual credentials popup
        state.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(matches!(state.input_popup, CloudInputPopup::MalCredentials { .. }));
    }

    #[test]
    fn cloud_modal_opens_mal_credentials_popup_when_no_client_id_configured() {
        let mut state = CloudModalState {
            selected_index: 6,
            connection: CloudConnectionState::default(),
            mal_connection: MalConnectionState::default(),
            detected_folders: Vec::new(),
            detected_cursor: 0,
            input_popup: CloudInputPopup::None,
            is_busy: false,
            progress: None,
            status_message: None,
            status_is_error: false,
        };

        let outcome = state.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(outcome, CloudModalOutcome::None);
        assert!(matches!(state.input_popup, CloudInputPopup::MalCredentials { .. }));
    }
}
