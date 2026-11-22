use std::collections::HashMap;
use std::error::Error;
use std::io::BufRead;
use std::process::exit;

use clap::{Parser, Subcommand, crate_version};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::Display;

use crate::backend::error_log::write_to_error_log;
use crate::backend::manga_provider::{Languages, MangaProviders};
use crate::backend::secrets::SecretStorage;
use crate::backend::secrets::keyring::KeyringStorage;
use crate::backend::tracker::anilist::{self, BASE_ANILIST_API_URL};
pub use crate::backend::tracker::{AnilistTokenChecker, Credentials};
use crate::backend::{APP_DATA_DIR, AppDirectories};
use crate::config::{MangaTuiConfig, get_config_directory_path, read_config_file};
use crate::global::PREFERRED_LANGUAGE;
use crate::logger::{ILogger, Logger};

fn read_input(mut input_reader: impl BufRead, logger: &impl ILogger, message: &str) -> Result<String, Box<dyn Error>> {
    logger.inform(message);
    let mut input_provided = String::new();
    input_reader.read_line(&mut input_provided)?;
    Ok(input_provided)
}

#[derive(Subcommand, Clone, Copy)]
pub enum AnilistCommand {
    /// setup anilist client to be able to sync reading progress
    Init,
    /// check wheter or not anilist is setup correctly
    Check,
}

#[derive(Subcommand, Clone)]
pub enum CloudCommand {
    /// Link Google Drive API (OAuth 2.0 PKCE) or a local Google Drive / iCloud Drive folder
    Login {
        /// Google Cloud OAuth 2.0 Client ID (optional if already saved or set via MANGA_TUI_GDRIVE_CLIENT_ID)
        #[arg(long)]
        client_id: Option<String>,
        /// Google Cloud OAuth 2.0 Client Secret (optional for Desktop PKCE clients)
        #[arg(long)]
        client_secret: Option<String>,
        /// Link a specific local Google Drive Desktop or iCloud Drive folder path
        #[arg(long)]
        folder: Option<String>,
        /// Automatically detect and link an installed Google Drive Desktop / iCloud Drive / OneDrive folder
        #[arg(long)]
        auto_folder: bool,
    },
    /// Show current cloud connection status, detected drives, and last sync summary
    Status,
    /// Synchronize reading progress, bookmarks, favorites, Plan to Read, and optionally downloaded chapters
    Sync {
        /// Also upload and download chapter archives (.cbz, .epub, .pdf)
        #[arg(short, long)]
        downloads: bool,
    },
    /// Upload local downloaded chapters (.cbz, .epub, .pdf) to the cloud
    PushDownloads,
    /// Download cloud chapter archives (.cbz, .epub, .pdf) into local mangaDownloads
    PullDownloads,
    /// Unlink the active cloud account and clear stored OAuth tokens
    Logout,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    Lang {
        #[arg(short, long)]
        print: bool,
        #[arg(short, long)]
        set: Option<String>,
    },

    Anilist {
        #[command(subcommand)]
        command: AnilistCommand,
    },

    /// Cloud synchronization with Google Drive API or iCloud Drive / Google Drive folders
    Cloud {
        #[command(subcommand)]
        command: CloudCommand,
    },

    /// Check for, download, and install application updates
    Update {
        /// Only check if an update is available without installing
        #[arg(long)]
        check: bool,
        /// Skip confirmation prompt and install automatically
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Parser, Clone)]
#[command(version = crate_version!())]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(short, long)]
    pub data_dir: bool,
    #[arg(short, long)]
    pub config_dir: bool,
    #[arg(long = "download-dir")]
    pub download_dir: bool,
    #[arg(short = 'p', long = "provider", visible_alias = "manga-provider")]
    pub manga_provider: Option<MangaProviders>,
    #[arg(short = 'l', long = "lang")]
    pub lang: Option<String>,
}

pub struct AnilistCredentialsProvided<'a> {
    pub access_token: &'a str,
    pub client_id: &'a str,
}

#[derive(Debug, Display, Clone, Copy)]
pub enum AnilistCredentials {
    #[strum(to_string = "anilist_client_id")]
    ClientId,
    #[strum(to_string = "anilist_secret")]
    Secret,
    #[strum(to_string = "anilist_code")]
    Code,
    #[strum(to_string = "anilist_access_token")]
    AccessToken,
}

impl From<AnilistCredentials> for String {
    fn from(value: AnilistCredentials) -> Self {
        value.to_string()
    }
}

pub fn check_anilist_credentials_are_stored(secret_provider: impl SecretStorage) -> Result<Option<Credentials>, Box<dyn Error>> {
    let credentials =
        secret_provider.get_multiple_secrets([AnilistCredentials::ClientId, AnilistCredentials::AccessToken].into_iter())?;

    let client_id = credentials.get(&AnilistCredentials::ClientId.to_string()).cloned();
    let access_token = credentials.get(&AnilistCredentials::AccessToken.to_string()).cloned();

    match client_id.zip(access_token) {
        Some((id, token)) => {
            if id.is_empty() || token.is_empty() {
                return Ok(None);
            }

            Ok(Some(Credentials {
                access_token: token,
                client_id: id.parse().unwrap(),
            }))
        },
        None => Ok(None),
    }
}

impl CliArgs {
    pub fn new() -> Self {
        Self {
            config_dir: false,
            command: None,
            data_dir: false,
            download_dir: false,
            manga_provider: Some(MangaProviders::default()),
            lang: None,
        }
    }

    pub fn with_command(mut self, command: Commands) -> Self {
        self.command = Some(command);
        self
    }

    pub fn print_available_languages() {
        println!("The available languages are:");
        Languages::iter().filter(|lang| *lang != Languages::Unkown).for_each(|lang| {
            println!("{} {} | iso code : {}", lang.as_emoji(), lang.as_human_readable().to_lowercase(), lang.as_iso_code())
        });
    }

    pub fn init_anilist(
        &self,
        mut input_reader: impl BufRead,
        storage: &mut impl SecretStorage,
        logger: impl ILogger,
    ) -> Result<(), Box<dyn Error>> {
        let client_id = read_input(&mut input_reader, &logger, "Provide your client id")?;
        let client_id = client_id.trim();

        let anilist_retrieve_access_token_url =
            format!("https://anilist.co/api/v2/oauth/authorize?client_id={client_id}&response_type=token");

        let open_in_browser_message = format!("Opening {anilist_retrieve_access_token_url}  to get the access token ");

        logger.inform(open_in_browser_message);

        open::that(anilist_retrieve_access_token_url)?;

        let access_token = read_input(&mut input_reader, &logger, "Enter the access token")?;
        let access_token = access_token.trim();

        self.save_anilist_credentials(
            AnilistCredentialsProvided {
                access_token,
                client_id,
            },
            storage,
        )?;

        logger.inform("Anilist was correctly setup :D");

        Ok(())
    }

    fn save_anilist_credentials(
        &self,
        credentials: AnilistCredentialsProvided<'_>,
        storage: &mut impl SecretStorage,
    ) -> Result<(), Box<dyn Error>> {
        storage.save_multiple_secrets(HashMap::from([
            (AnilistCredentials::AccessToken.to_string(), credentials.access_token.to_string()),
            (AnilistCredentials::ClientId.to_string(), credentials.client_id.to_string()),
        ]))?;
        Ok(())
    }

    async fn check_anilist_token(&self, token_checker: &impl AnilistTokenChecker, token: String) -> Result<bool, Box<dyn Error>> {
        token_checker.verify_token(token).await
    }

    async fn check_anilist_status(&self, logger: &impl ILogger, config: MangaTuiConfig) -> Result<(), Box<dyn Error>> {
        let storage = KeyringStorage::new();
        logger.inform("Checking client id and access token are stored");

        let credentials_are_stored = config
            .check_anilist_credentials()
            .or_else(|| check_anilist_credentials_are_stored(storage).ok().flatten());

        if credentials_are_stored.is_none() {
            logger.warn(
                "The client id or the access token are empty, run `manga-tui anilist init` to store your anilist credentials \n or you can store your credentials in your config file",
            );
            exit(0)
        }

        let credentials = credentials_are_stored.unwrap();

        logger.inform("Checking your access token is valid, this may take a while");

        let anilist = anilist::Anilist::new(BASE_ANILIST_API_URL.parse().unwrap())
            .with_token(credentials.access_token.clone())
            .with_client_id(credentials.client_id);

        let access_token_is_valid = self.check_anilist_token(&anilist, credentials.access_token).await?;

        if access_token_is_valid {
            logger.inform("Everything is setup correctly :D");
        } else {
            logger.error("The anilist access token is not valid, please run `manga-tui anilist init` to set a new one \n or you can store your credentials in your config file".into());
            exit(0)
        }

        Ok(())
    }

    /// This method should only return `Ok(())` it the app should keep running, otherwise `exit`
    pub async fn proccess_args(self) -> Result<(), Box<dyn Error>> {
        if self.data_dir {
            let app_dir = APP_DATA_DIR.as_ref().unwrap();
            println!("{}", app_dir.to_str().unwrap());
            exit(0)
        }

        if self.config_dir {
            println!("{}", get_config_directory_path().display());
            exit(0)
        }

        if self.download_dir {
            println!("{}", AppDirectories::MangaDownloads.get_full_path().display());
            exit(0)
        }

        let preferred_lang_to_set = if let Some(cli_lang_str) = &self.lang {
            match Languages::from_str_lenient(cli_lang_str) {
                Some(l) => l,
                None => {
                    println!(
                        "`{}` is not a valid language or ISO code. Run `{} lang --print` to list available languages",
                        cli_lang_str,
                        env!("CARGO_BIN_NAME")
                    );
                    exit(0);
                },
            }
        } else {
            MangaTuiConfig::get().default_language
        };

        match &self.command {
            Some(command) => match command {
                Commands::Lang { print, set } => {
                    if *print {
                        Self::print_available_languages();
                        exit(0)
                    }

                    match set {
                        Some(lang) => {
                            let try_lang = Languages::from_str_lenient(lang.as_str());

                            if try_lang.is_none() {
                                println!(
                                    "`{}` is not a valid language or ISO code, run `{} lang --print` to list available languages and their ISO codes",
                                    lang,
                                    env!("CARGO_BIN_NAME")
                                );

                                exit(0)
                            }

                            let resolved = try_lang.unwrap();
                            crate::view::widgets::settings::set_active_chapter_language(resolved);
                            PREFERRED_LANGUAGE.set(resolved).ok();
                        },
                        None => {
                            crate::view::widgets::settings::set_active_chapter_language(preferred_lang_to_set);
                            PREFERRED_LANGUAGE.set(preferred_lang_to_set).ok();
                        },
                    }
                    Ok(())
                },

                Commands::Anilist { command } => match command {
                    AnilistCommand::Init => {
                        let mut storage = KeyringStorage::new();
                        self.init_anilist(std::io::stdin().lock(), &mut storage, Logger)?;
                        exit(0)
                    },
                    AnilistCommand::Check => {
                        let logger = Logger;

                        let config = read_config_file()?;
                        if let Err(e) = self.check_anilist_status(&logger, config).await {
                            logger.error(format!("Some error ocurred, more details \n {e}").into());
                            write_to_error_log(e.into());
                            exit(1);
                        } else {
                            exit(0)
                        }
                    },
                },

                Commands::Cloud { command } => {
                    if let Err(err) = run_cli_cloud(command).await {
                        eprintln!("Error: {err}");
                        exit(1);
                    }
                    exit(0)
                },

                Commands::Update { check, yes } => {
                    if let Err(err) = run_cli_update(*check, *yes).await {
                        eprintln!("Error: {err}");
                        exit(1);
                    }
                    exit(0)
                },
            },
            None => {
                crate::view::widgets::settings::set_active_chapter_language(preferred_lang_to_set);
                PREFERRED_LANGUAGE.set(preferred_lang_to_set).ok();
                Ok(())
            },
        }
    }
}

async fn run_cli_update(check_only: bool, assume_yes: bool) -> Result<(), String> {
    use std::io::Write;

    use crate::backend::updater::{
        BuildIdentity, UpdateClient, UpdateProgressStage, apply_prepared_update_and_relaunch, detect_current_install_kind,
    };
    use crate::i18n::tr;
    use crate::view::widgets::updater::format_byte_size;

    let current = BuildIdentity::current();
    let install_kind = detect_current_install_kind();
    let current_label = if current.commit_short != "unknown" {
        format!("v{} ({})", current.version, current.commit_short)
    } else {
        format!("v{}", current.version)
    };

    println!("Manga TUI {current_label} [{}]", install_kind.label());
    println!("{}", tr("Buscando actualizaciones en GitHub...", "Checking GitHub for updates..."));

    let client = UpdateClient::default_github();
    let Some(update) = client.check_for_update(&current, install_kind).await? else {
        println!("{}", tr("✓ Ya estás en la última versión disponible.", "✓ Already running the latest available version."));
        return Ok(());
    };

    println!(
        "{}: {} [{}]",
        tr("⬆ Nueva actualización disponible", "⬆ New update available"),
        update.display_version,
        update.channel.label()
    );
    if let Some(asset) = &update.download_asset {
        let size_label = if asset.size > 0 { format!(" ({})", format_byte_size(asset.size)) } else { String::new() };
        println!("{}: {}{}", tr("Paquete", "Package"), asset.name, size_label);
    }
    if !update.release_notes.trim().is_empty() {
        println!("\n{}", update.release_notes);
    }

    if check_only {
        return Ok(());
    }

    if !assume_yes {
        print!(
            "\n{}",
            tr("¿Descargar, verificar SHA-256 e instalar ahora? [S/n]: ", "Download, verify SHA-256, and install now? [Y/n]: ")
        );
        let _ = std::io::stdout().flush();
        let mut answer = String::new();
        std::io::stdin()
            .read_line(&mut answer)
            .map_err(|e| format!("Failed reading confirmation: {e}"))?;
        let trimmed = answer.trim().to_ascii_lowercase();
        if trimmed == "n" || trimmed == "no" {
            println!("{}", tr("Actualización cancelada.", "Update cancelled."));
            return Ok(());
        }
    }

    let target_exe = std::env::current_exe().map_err(|e| format!("Could not resolve current executable: {e}"))?;
    let mut last_percent: Option<u64> = None;

    let prepared = client
        .download_and_verify(&update, target_exe, |progress| match progress.stage {
            UpdateProgressStage::Downloading => {
                if let Some(total) = progress.total_bytes.filter(|&t| t > 0) {
                    let pct = ((progress.downloaded_bytes as f64 / total as f64) * 100.0).round() as u64;
                    if last_percent.is_none_or(|prev| pct >= prev + 10 || pct == 100) {
                        last_percent = Some(pct);
                        println!(
                            "  {} {pct}% ({} / {})",
                            tr("Descargando", "Downloading"),
                            format_byte_size(progress.downloaded_bytes),
                            format_byte_size(total)
                        );
                    }
                }
            },
            UpdateProgressStage::Verifying => {
                println!("{}", tr("🔐 Verificando integridad SHA-256...", "🔐 Verifying SHA-256 integrity..."));
            },
            UpdateProgressStage::Preparing => {
                println!("{}", tr("⚙ Aplicando actualización...", "⚙ Applying update..."));
            },
        })
        .await?;

    println!("✓ SHA-256: {}", prepared.verified_sha256);
    apply_prepared_update_and_relaunch(&prepared)?;
    println!(
        "{}",
        tr(
            "✓ Actualización iniciada en segundo plano. Manga TUI se reiniciará automáticamente.",
            "✓ Update launched in background. Manga TUI will restart automatically."
        )
    );
    Ok(())
}

async fn run_cli_cloud(command: &CloudCommand) -> Result<(), String> {
    use std::io::Write;
    use std::path::PathBuf;

    use crate::backend::cloud::{
        CLOUD_ROOT_FOLDER_NAME, CloudBackendKind, CloudConnectionState, CloudSyncMode, GoogleDriveClient,
        detect_installed_cloud_folders, run_cloud_sync,
    };
    use crate::i18n::tr;
    use crate::view::widgets::updater::format_byte_size;

    let mut state = CloudConnectionState::load();

    match command {
        CloudCommand::Status => {
            println!(
                "☁  Manga TUI Cloud Sync — {}: {}",
                tr("Estado", "Status"),
                if state.is_connected() {
                    format!("🟢 {}", state.connection_summary())
                } else {
                    format!("⚪ {}", tr("Sin vincular", "Not linked"))
                }
            );
            if let Some(report) = &state.last_sync_report {
                println!(
                    "   {}: {} · {} mangas · {} {} · ↑{} / ↓{} ({})",
                    tr("Última sincronización", "Last sync"),
                    report.synced_at,
                    report.total_mangas,
                    report.total_read_chapters,
                    tr("cap. leídos", "read ch."),
                    report.chapters_uploaded,
                    report.chapters_downloaded,
                    format_byte_size(report.bytes_transferred)
                );
            }
            let detected = detect_installed_cloud_folders();
            if !detected.is_empty() {
                println!(
                    "\n{}:",
                    tr("📁 Unidades en la nube detectadas en este equipo", "📁 Cloud drives detected on this machine")
                );
                for d in detected {
                    println!("   • {} -> {}", d.provider_label, d.manga_tui_folder.display());
                }
            }
            Ok(())
        },
        CloudCommand::Logout => {
            state.clear_credentials();
            state.save()?;
            println!("{}", tr("✓ Cuenta en la nube desvinculada correctamente.", "✓ Cloud account unlinked successfully."));
            Ok(())
        },
        CloudCommand::Login {
            client_id,
            client_secret,
            folder,
            auto_folder,
        } => {
            if *auto_folder {
                let detected = detect_installed_cloud_folders();
                let Some(chosen) = detected.into_iter().next() else {
                    return Err(tr(
                        "No se detectó ninguna carpeta local de Google Drive Desktop, iCloud Drive o OneDrive. Usa `--folder <ruta>`.",
                        "No local Google Drive Desktop, iCloud Drive, or OneDrive folder detected. Use `--folder <path>`.",
                    )
                    .to_string());
                };
                std::fs::create_dir_all(&chosen.manga_tui_folder).map_err(|e| e.to_string())?;
                state.backend = Some(CloudBackendKind::LocalCloudFolder);
                state.local_folder_path = Some(chosen.manga_tui_folder.clone());
                state.local_folder_provider_label = Some(chosen.provider_label.clone());
                state.save()?;
                println!(
                    "✓ {}: {} ({})",
                    tr("Vinculado con", "Linked with"),
                    chosen.provider_label,
                    chosen.manga_tui_folder.display()
                );
                return Ok(());
            }

            if let Some(raw_folder) = folder {
                let base = PathBuf::from(raw_folder.trim().trim_matches('"'));
                let target = if base.file_name().is_some_and(|n| n == CLOUD_ROOT_FOLDER_NAME) {
                    base
                } else {
                    base.join(CLOUD_ROOT_FOLDER_NAME)
                };
                std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                state.backend = Some(CloudBackendKind::LocalCloudFolder);
                state.local_folder_path = Some(target.clone());
                state.local_folder_provider_label = Some(tr("Carpeta en la nube", "Cloud Folder").to_string());
                state.save()?;
                println!("✓ {}: {}", tr("Carpeta en la nube vinculada", "Linked cloud folder"), target.display());
                return Ok(());
            }

            let resolved_client_id = match client_id.clone().or_else(|| state.get_client_id()) {
                Some(id) if !id.trim().is_empty() => id.trim().to_string(),
                _ => {
                    print!(
                        "{}",
                        tr(
                            "Introduce tu Google OAuth Client ID (o escribe 'auto' para vincular carpeta local de Google Drive/iCloud): ",
                            "Enter your Google OAuth Client ID (or type 'auto' to link local Google Drive/iCloud folder): "
                        )
                    );
                    let _ = std::io::stdout().flush();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
                    let trimmed = input.trim();
                    if trimmed.eq_ignore_ascii_case("auto") {
                        return Box::pin(run_cli_cloud(&CloudCommand::Login {
                            client_id: None,
                            client_secret: None,
                            folder: None,
                            auto_folder: true,
                        }))
                        .await;
                    }
                    trimmed.to_string()
                },
            };

            let resolved_secret = client_secret.clone().or_else(|| state.get_client_secret());
            let drive_client = GoogleDriveClient::new();
            let tokens = drive_client
                .run_loopback_oauth_flow(&resolved_client_id, resolved_secret.as_deref(), |auth_url| {
                    println!(
                        "\n{}\n{auth_url}\n",
                        tr(
                            "🌐 Abriendo navegador para autorizar Google Drive...",
                            "🌐 Opening browser to authorize Google Drive..."
                        )
                    );
                    let _ = open::that(auth_url);
                })
                .await?;

            state.backend = Some(CloudBackendKind::GoogleDriveApi);
            state.save_oauth_tokens(
                &resolved_client_id,
                resolved_secret.as_deref(),
                &tokens.access_token,
                tokens.refresh_token.as_deref(),
            );
            if let Ok(about) = drive_client.get_about_user(&tokens.access_token).await
                && let Some(user) = about.user
            {
                state.account_email = user.email_address;
                state.account_display_name = user.display_name;
            }
            state.save()?;
            println!("✓ {}: {}", tr("Conectado correctamente", "Successfully connected"), state.connection_summary());
            Ok(())
        },
        CloudCommand::Sync { downloads } => {
            let mode = if *downloads { CloudSyncMode::FullSync } else { CloudSyncMode::LibraryOnly };
            let client = GoogleDriveClient::new();
            let report = run_cloud_sync(&mut state, mode, &client, |p| {
                println!("  • {}", p.step_label);
            })
            .await?;
            println!(
                "✓ {}: {} mangas · {} {} · ↑{} / ↓{} ({})",
                tr("Sincronización completada", "Sync completed"),
                report.total_mangas,
                report.total_read_chapters,
                tr("cap. leídos", "read ch."),
                report.chapters_uploaded,
                report.chapters_downloaded,
                format_byte_size(report.bytes_transferred)
            );
            Ok(())
        },
        CloudCommand::PushDownloads => {
            let client = GoogleDriveClient::new();
            let report = run_cloud_sync(&mut state, CloudSyncMode::PushDownloadsOnly, &client, |p| {
                println!("  • {}", p.step_label);
            })
            .await?;
            println!(
                "✓ {}: ↑{} ({})",
                tr("Capítulos subidos a la nube", "Chapters uploaded to cloud"),
                report.chapters_uploaded,
                format_byte_size(report.bytes_transferred)
            );
            Ok(())
        },
        CloudCommand::PullDownloads => {
            let client = GoogleDriveClient::new();
            let report = run_cloud_sync(&mut state, CloudSyncMode::PullDownloadsOnly, &client, |p| {
                println!("  • {}", p.step_label);
            })
            .await?;
            println!(
                "✓ {}: ↓{} ({})",
                tr("Capítulos descargados de la nube", "Chapters downloaded from cloud"),
                report.chapters_downloaded,
                format_byte_size(report.bytes_transferred)
            );
            Ok(())
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error;
    use std::sync::RwLock;

    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct MockStorage {
        secrets_stored: RwLock<HashMap<String, String>>,
    }

    impl MockStorage {
        fn with_data(secrets_stored: HashMap<String, String>) -> Self {
            Self {
                secrets_stored: RwLock::new(secrets_stored),
            }
        }
    }

    impl SecretStorage for MockStorage {
        fn save_secret<T: Into<String>, S: Into<String>>(&self, name: T, value: S) -> Result<(), Box<dyn Error>> {
            self.secrets_stored.write().unwrap().insert(name.into(), value.into());
            Ok(())
        }

        fn get_secret<T: Into<String>>(&self, secret_name: T) -> Result<Option<String>, Box<dyn Error>> {
            Ok(self.secrets_stored.read().unwrap().get(&secret_name.into()).cloned())
        }

        fn remove_secret<T: AsRef<str>>(&self, secret_name: T) -> Result<(), Box<dyn Error>> {
            match self.secrets_stored.write().unwrap().remove(secret_name.as_ref()) {
                Some(_val) => Ok(()),
                None => Err("secret did not exist".into()),
            }
        }
    }

    #[test]
    fn it_saves_anilist_access_token_and_user_id() {
        let cli = CliArgs::new();
        let acess_token = Uuid::new_v4().to_string();
        let user_id = "120398".to_string();

        let mut storage = MockStorage::default();

        cli.save_anilist_credentials(
            AnilistCredentialsProvided {
                access_token: &acess_token,
                client_id: &user_id,
            },
            &mut storage,
        )
        .expect("should not fail");

        let secrets = storage.secrets_stored.read().unwrap();

        let (secret_name, token) = secrets.get_key_value("anilist_access_token").unwrap();

        assert_eq!("anilist_access_token", secret_name);
        assert_eq!(acess_token, *token);

        let (secret_name, value) = secrets.get_key_value("anilist_client_id").unwrap();

        assert_eq!("anilist_client_id", secret_name);
        assert_eq!(user_id.parse::<u32>().unwrap(), value.parse::<u32>().unwrap());
    }

    #[derive(Debug)]
    struct AnilistCheckerTest {
        should_fail: bool,
        invalid_token: bool,
    }

    impl AnilistCheckerTest {
        fn succesful() -> Self {
            Self {
                should_fail: false,
                invalid_token: false,
            }
        }

        fn failing() -> Self {
            Self {
                should_fail: true,
                invalid_token: true,
            }
        }
    }
    impl AnilistTokenChecker for AnilistCheckerTest {
        async fn verify_token(&self, _token: String) -> Result<bool, Box<dyn Error>> {
            Ok(!self.invalid_token)
        }
    }

    #[tokio::test]
    async fn it_checks_acess_token_is_valid() -> Result<(), Box<dyn Error>> {
        let cli = CliArgs::new();

        let anilist_checker = AnilistCheckerTest::succesful();

        let token_is_valid = cli.check_anilist_token(&anilist_checker, "some_token".to_string()).await?;

        assert!(token_is_valid);

        let anilist_checker = AnilistCheckerTest::failing();

        let token_is_valid = cli.check_anilist_token(&anilist_checker, "some_token".to_string()).await?;

        assert!(!token_is_valid);
        Ok(())
    }

    #[test]
    fn it_check_anilist_credentials_are_stored() -> Result<(), Box<dyn Error>> {
        let expected_credentials = [
            ("anilist_client_id".to_string(), "some_id".to_string()),
            ("anilist_access_token".to_string(), "some_token".to_string()),
        ];

        let storage = MockStorage::with_data(HashMap::from(expected_credentials));

        let credentials =
            check_anilist_credentials_are_stored(storage)?.expect("anilist credentials which should be stored actually aren't");

        assert_eq!("some_id", credentials.client_id);
        assert_eq!("some_token", credentials.access_token);

        Ok(())
    }
}
