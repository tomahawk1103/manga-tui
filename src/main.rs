#![allow(unused)]
#![allow(dead_code)]
#![allow(deprecated)]
#![allow(clippy::single_match)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::too_many_arguments)]

use std::io::stdout;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;

use backend::cache::Cacher;
use backend::cache::in_memory::InMemoryCache;
use backend::manga_provider::comizy::ComizyProvider;
use backend::manga_provider::comizy::filter_state::ComizyFilterState;
use backend::manga_provider::inmanga::InmangaProvider;
use backend::manga_provider::inmanga::filter_state::InmangaFilterState;
use backend::manga_provider::leercapitulo::LeercapituloProvider;
use backend::manga_provider::leercapitulo::filter_state::LeercapituloFilterState;
use backend::manga_provider::mangadex::{API_URL_BASE, COVER_IMG_URL_BASE, MangadexClient};
use backend::manga_provider::mangaoni::MangaoniProvider;
use backend::manga_provider::mangaoni::filter_state::MangaoniFilterState;
use backend::manga_provider::weebcentral::filter_state::WeebcentralFilterState;
use backend::manga_provider::weebcentral::{WEEBCENTRAL_BASE_URL, WeebcentralProvider};
use backend::manga_provider::{ChapterFilters, GetChapterPages, MangaPageProvider, MangaProviders, Pagination, SearchPageProvider};
use backend::secrets::keyring::KeyringStorage;
use backend::tracker::UnifiedTracker;
use backend::tracker::anilist::{Anilist, BASE_ANILIST_API_URL};
use backend::tracker::myanimelist::MyAnimeListClient;
use clap::Parser;
use cli::{Credentials, check_anilist_credentials_are_stored};
use crossterm::ExecutableCommand;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::SetTitle;
use http::StatusCode;
use log::LevelFilter;
use logger::{ILogger, Logger};
use manga_tui::SearchTerm;

use self::backend::build_data_dir;
use self::backend::database::Database;
use self::backend::migration::update_database_with_migrations;
use self::backend::tui::run_app;
use self::cli::CliArgs;
use self::config::MangaTuiConfig;
use self::i18n::tr;
use crate::backend::manga_provider::mangadex::get_cached_filters;
use crate::backend::manga_provider::mangapill::MangaPillProvider;
use crate::backend::multi_provider::spanish_provider_candidates;
use crate::config::ImageQuality;
use crate::view::provider_filters::comizy::{ComizyFilterWidget, ComizyFiltersProvider};
use crate::view::provider_filters::inmanga::{InmangaFilterWidget, InmangaFiltersProvider};
use crate::view::provider_filters::leercapitulo::{LeercapituloFilterWidget, LeercapituloFiltersProvider};
use crate::view::provider_filters::mangadex::filter_provider::MangadexFilterProvider;
use crate::view::provider_filters::mangadex::filter_widget::MangadexFilterWidget;
use crate::view::provider_filters::mangaoni::{MangaoniFilterWidget, MangaoniFiltersProvider};
use crate::view::provider_filters::mangapill::{MangaPillFilterWidget, MangaPillFiltersProvider};
use crate::view::provider_filters::weebcentral::{WeebcentralFilterWidget, WeebcentralFiltersProvider};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_provider_fallbacks_only_include_spanish_sources() {
        assert_eq!(
            spanish_provider_candidates(MangaProviders::Mangaoni),
            Some([MangaProviders::Mangaoni, MangaProviders::Inmanga, MangaProviders::Leercapitulo,])
        );
        assert_eq!(
            spanish_provider_candidates(MangaProviders::Inmanga),
            Some([MangaProviders::Inmanga, MangaProviders::Mangaoni, MangaProviders::Leercapitulo,])
        );
        assert_eq!(
            spanish_provider_candidates(MangaProviders::Leercapitulo),
            Some([MangaProviders::Leercapitulo, MangaProviders::Inmanga, MangaProviders::Mangaoni,])
        );
        assert_eq!(spanish_provider_candidates(MangaProviders::Mangadex), None);
        assert_eq!(spanish_provider_candidates(MangaProviders::Weebcentral), None);
        assert_eq!(spanish_provider_candidates(MangaProviders::Mangapill), None);
        assert_eq!(spanish_provider_candidates(MangaProviders::Comizy), None);
    }

    #[test]
    fn windows_terminal_relaunch_only_triggers_for_interactive_standalone_console() {
        let exe = vec!["manga-tui.exe".to_string()];
        assert!(should_relaunch_in_windows_terminal(true, false, false, &exe));

        // Not standalone (launched inside an existing shell like cmd/powershell)
        assert!(!should_relaunch_in_windows_terminal(false, false, false, &exe));
        // Already inside a modern terminal (Windows Terminal, WezTerm, VS Code, etc.)
        assert!(!should_relaunch_in_windows_terminal(true, true, false, &exe));
        // Anti-loop guard env var is set
        assert!(!should_relaunch_in_windows_terminal(true, false, true, &exe));

        // Non-interactive CLI arguments should run directly without opening a new tab
        for flag in ["-h", "--help", "-V", "--version", "lang", "anilist", "mal", "update", "cloud"] {
            let args = vec!["manga-tui.exe".to_string(), flag.to_string()];
            assert!(!should_relaunch_in_windows_terminal(true, false, false, &args), "expected no relaunch for CLI arg {flag}");
        }

        // Interactive provider override should still relaunch when opened standalone
        let provider_args = vec!["manga-tui.exe".to_string(), "-p".to_string(), "inmanga".to_string()];
        assert!(should_relaunch_in_windows_terminal(true, false, false, &provider_args));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_terminal_binary_lookup_finds_windowsapps_or_path_and_returns_none_when_missing() {
        let base = std::path::PathBuf::from("./test_results/wt_lookup_test");
        let _ = std::fs::remove_dir_all(&base);
        let local_app_data = base.join("local");
        let win_apps = local_app_data.join("Microsoft").join("WindowsApps");
        let path_dir = base.join("path_bin");
        std::fs::create_dir_all(&win_apps).unwrap();
        std::fs::create_dir_all(&path_dir).unwrap();

        // Missing everywhere -> None
        assert_eq!(find_windows_terminal_binary_in(Some(local_app_data.to_str().unwrap()), Some(path_dir.as_os_str()),), None);

        // Present in PATH -> found in PATH
        let path_wt = path_dir.join("wt.exe");
        std::fs::write(&path_wt, b"").unwrap();
        assert_eq!(
            find_windows_terminal_binary_in(Some(local_app_data.to_str().unwrap()), Some(path_dir.as_os_str()),),
            Some(path_wt)
        );

        // Present in WindowsApps -> WindowsApps wins
        let app_wt = win_apps.join("wt.exe");
        std::fs::write(&app_wt, b"").unwrap();
        assert_eq!(
            find_windows_terminal_binary_in(Some(local_app_data.to_str().unwrap()), Some(path_dir.as_os_str()),),
            Some(app_wt)
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn windows_icon_is_valid_multi_resolution_ico() {
        let icon_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("icon.ico");
        let bytes = std::fs::read(&icon_path).expect("assets/icon.ico must exist");
        assert!(bytes.len() >= 22, "ICO file too small");

        let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
        let icon_type = u16::from_le_bytes([bytes[2], bytes[3]]);
        let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;

        assert_eq!(reserved, 0, "ICO reserved field must be 0");
        assert_eq!(icon_type, 1, "ICO type field must be 1 (icon)");
        assert!(count >= 6, "ICO should contain multiple resolutions, found {count}");
        assert!(bytes.len() >= 6 + count * 16, "ICO directory table truncated");

        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            let entry = &bytes[6 + i * 16..6 + (i + 1) * 16];
            let width = if entry[0] == 0 { 256u16 } else { u16::from(entry[0]) };
            let height = if entry[1] == 0 { 256u16 } else { u16::from(entry[1]) };
            let byte_len = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;

            assert_eq!(width, height, "ICO entries should be square");
            assert!(byte_len > 0, "ICO entry {i} must have non-zero size");
            assert!(offset + byte_len <= bytes.len(), "ICO entry {i} points outside file bounds");
            sizes.push(width);
        }

        for expected in [16u16, 24, 32, 48, 64, 128, 256] {
            assert!(sizes.contains(&expected), "ICO missing {expected}x{expected} resolution (found: {sizes:?})");
        }
    }
}

pub use manga_core::{config, global, i18n, logger, runtime_settings};

mod backend;
mod cli;
mod common;
mod theme;
mod utils;
mod view;

/// Restores the terminal (raw mode, alternate screen and mouse capture) when dropped, so the user's
/// terminal is left in a usable state even if the app returns early with an error
struct TerminalGuard;

impl TerminalGuard {
    fn init() -> Result<(ratatui::DefaultTerminal, Self), Box<dyn std::error::Error>> {
        let terminal = ratatui::init();
        backend::tui::probe_terminal_cell_size();
        stdout().execute(EnableMouseCapture)?;
        let _ = stdout().execute(SetTitle(format!("Manga TUI v{}", env!("CARGO_PKG_VERSION"))));
        Ok((terminal, Self))
    }
}

fn restore_terminal_safely() {
    let _ = stdout().execute(DisableMouseCapture);
    ratatui::restore();
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal_safely();
    }
}

#[cfg(target_os = "windows")]
const NO_WT_RELAUNCH_ENV: &str = "MANGA_TUI_NO_WT_RELAUNCH";

/// Returns `true` when the application should relaunch itself inside Windows Terminal (`wt.exe`).
fn should_relaunch_in_windows_terminal(
    is_standalone_console: bool,
    is_modern_terminal: bool,
    no_relaunch_guard: bool,
    args: &[String],
) -> bool {
    if !is_standalone_console || is_modern_terminal || no_relaunch_guard {
        return false;
    }

    const NON_INTERACTIVE_TOKENS: [&str; 9] = ["-h", "--help", "-V", "--version", "lang", "anilist", "mal", "update", "cloud"];
    !args.iter().skip(1).any(|arg| NON_INTERACTIVE_TOKENS.iter().any(|token| arg == token))
}

#[cfg(target_os = "windows")]
fn configure_windows_console() {
    use windows_sys::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};
    const CP_UTF8: u32 = 65001;
    unsafe {
        let _ = SetConsoleCP(CP_UTF8);
        let _ = SetConsoleOutputCP(CP_UTF8);
    }
}

#[cfg(target_os = "windows")]
fn is_standalone_console_launch() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;
    let mut pids = [0u32; 2];
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
    count == 1
}

#[cfg(target_os = "windows")]
fn is_existing_executable_or_alias(path: &std::path::Path) -> bool {
    path.is_file() || std::fs::symlink_metadata(path).is_ok()
}

#[cfg(target_os = "windows")]
fn find_windows_terminal_binary_in(local_app_data: Option<&str>, path_env: Option<&std::ffi::OsStr>) -> Option<std::path::PathBuf> {
    if let Some(local) = local_app_data.filter(|s| !s.trim().is_empty()) {
        let wt_path = std::path::PathBuf::from(local).join("Microsoft").join("WindowsApps").join("wt.exe");
        if is_existing_executable_or_alias(&wt_path) {
            return Some(wt_path);
        }
    }

    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let candidate = dir.join("wt.exe");
            if is_existing_executable_or_alias(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn find_windows_terminal_binary() -> Option<std::path::PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok();
    let path_env = std::env::var_os("PATH");
    find_windows_terminal_binary_in(local_app_data.as_deref(), path_env.as_deref())
}

#[cfg(target_os = "windows")]
fn try_relaunch_in_windows_terminal() {
    let args: Vec<String> = std::env::args().collect();
    let is_standalone = is_standalone_console_launch();
    let is_modern = backend::tui::is_running_in_modern_windows_terminal();
    let no_relaunch = std::env::var_os(NO_WT_RELAUNCH_ENV).is_some();

    if !should_relaunch_in_windows_terminal(is_standalone, is_modern, no_relaunch, &args) {
        return;
    }

    let Some(wt_path) = find_windows_terminal_binary() else {
        return;
    };

    let Ok(current_exe) = std::env::current_exe() else {
        return;
    };

    let mut cmd = std::process::Command::new(wt_path);
    cmd.args(["-M", "new-tab", "--title", "Manga TUI"]);
    if let Ok(current_dir) = std::env::current_dir() {
        cmd.arg("-d").arg(current_dir);
    }
    cmd.arg("--").arg(&current_exe);
    for arg in args.iter().skip(1) {
        cmd.arg(arg);
    }
    cmd.env(NO_WT_RELAUNCH_ENV, "1");

    if cmd.spawn().is_ok() {
        exit(0);
    }
}

fn pause_before_exit_if_standalone() {
    #[cfg(target_os = "windows")]
    if is_standalone_console_launch() {
        eprintln!("\nPresiona Enter para cerrar esta ventana... / Press Enter to close this window...");
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    {
        configure_windows_console();
        try_relaunch_in_windows_terminal();
    }

    let exit_code = match tokio::runtime::Builder::new_multi_thread().worker_threads(7).enable_all().build() {
        Ok(runtime) => match runtime.block_on(async_main()) {
            Ok(()) => 0,
            Err(err) => {
                restore_terminal_safely();
                eprintln!("Error: {err}");
                pause_before_exit_if_standalone();
                1
            },
        },
        Err(err) => {
            eprintln!("Failed to initialize async runtime: {err}");
            pause_before_exit_if_standalone();
            1
        },
    };

    if exit_code != 0 {
        exit(exit_code);
    }
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    crate::backend::updater::cleanup_stale_old_binary();

    let logger = Logger;

    pretty_env_logger::formatted_builder()
        .format_module_path(false)
        .filter_level(LevelFilter::Info)
        .init();

    match build_data_dir(&logger) {
        Ok(_) => {},
        Err(e) => {
            logger.error(
                format!(
                    "No se pudo crear la carpeta de datos donde se guardan tu historial, configuración y descargas.\n\
                     Esto suele deberse a falta de permisos de escritura en esta ubicación.\n\
                     Solución: mueve manga-tui a otra carpeta o define la variable de entorno `MANGA_TUI_DATA_DIR`.\n\
                     ---\n\
                     Could not create the data directory where your history, config, and downloads are stored.\n\
                     This is usually caused by insufficient write permissions here.\n\
                     Fix: move manga-tui to a writable folder or set `MANGA_TUI_DATA_DIR`.\n\
                     Detalle / Details: {e}"
                )
                .into(),
            );
            pause_before_exit_if_standalone();
            exit(1)
        },
    }

    let cli_args = CliArgs::parse();

    let manga_provider_cli = cli_args.manga_provider;

    cli_args.proccess_args().await?;

    let config = MangaTuiConfig::get();

    let anilist_storage = KeyringStorage::new();

    let init_anilist = |credentials: Credentials| {
        Anilist::new(BASE_ANILIST_API_URL.parse().unwrap())
            .with_token(credentials.access_token)
            .with_client_id(credentials.client_id)
    };

    let anilist_client = match config.check_anilist_credentials() {
        Some(credentials) => Some(init_anilist(credentials)),
        None => check_anilist_credentials_are_stored(anilist_storage)
            .inspect_err(|e| {
                logger.warn(format!(
                    "There is an issue when trying to check anilist credentials, more details about the error : {e}"
                ));
            })
            .ok()
            .flatten()
            .map(init_anilist),
    }
    .filter(|_| config.track_reading_history);

    let mal_client = MyAnimeListClient::from_saved_state().filter(|_| config.track_reading_history);

    if mal_client.is_some() {
        logger.inform(tr(
            "MyAnimeList configurado (OAuth 2.0 PKCE), se sincronizará el progreso de lectura",
            "MyAnimeList is setup (OAuth 2.0 PKCE), tracking reading progress",
        ));
    }

    let unified_tracker: Option<UnifiedTracker> = match (anilist_client, mal_client) {
        (Some(a), Some(m)) => Some(UnifiedTracker::Both(a, m)),
        (Some(a), None) => Some(UnifiedTracker::Anilist(a)),
        (None, Some(m)) => Some(UnifiedTracker::MyAnimeList(m)),
        (None, None) => None,
    };

    let mut connection = Database::get_connection()?;
    let database = Database::new(&connection);

    database.setup()?;
    update_database_with_migrations(&mut connection, &logger)?;

    drop(connection);

    color_eyre::install()?;
    let prev_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        restore_terminal_safely();
        prev_panic_hook(panic_info);
        pause_before_exit_if_standalone();
    }));

    let cache_provider: Arc<dyn Cacher> = InMemoryCache::init(8);

    let provider = if let Some(pro) = manga_provider_cli { pro } else { config.default_manga_provider };

    let pending_update = match provider {
        MangaProviders::Mangadex => {
            let mangadex_client =
                MangadexClient::new(API_URL_BASE.parse().unwrap(), COVER_IMG_URL_BASE.parse().unwrap(), cache_provider)
                    .with_image_quality(config.image_quality);

            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                mangadex_client,
                unified_tracker,
                MangadexFilterProvider::from(get_cached_filters()),
                MangadexFilterWidget::new(),
            )
            .await?
        },
        MangaProviders::Weebcentral => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                WeebcentralProvider::new(WEEBCENTRAL_BASE_URL.parse().unwrap(), cache_provider),
                unified_tracker,
                WeebcentralFiltersProvider::new(WeebcentralFilterState::default()),
                WeebcentralFilterWidget::new(),
            )
            .await?
        },
        MangaProviders::Mangapill => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                MangaPillProvider::init(cache_provider),
                unified_tracker,
                MangaPillFiltersProvider::new(backend::manga_provider::mangapill::filter_state::MangaPillFilterState::default()),
                MangaPillFilterWidget::new(),
            )
            .await?
        },
        MangaProviders::Inmanga => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                InmangaProvider::new(cache_provider),
                unified_tracker,
                InmangaFiltersProvider::new(InmangaFilterState::default()),
                InmangaFilterWidget::new(),
            )
            .await?
        },
        MangaProviders::Mangaoni => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                MangaoniProvider::new(cache_provider),
                unified_tracker,
                MangaoniFiltersProvider::new(MangaoniFilterState::default()),
                MangaoniFilterWidget::new(),
            )
            .await?
        },
        MangaProviders::Leercapitulo => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                LeercapituloProvider::new(cache_provider),
                unified_tracker,
                LeercapituloFiltersProvider::new(LeercapituloFilterState::default()),
                LeercapituloFilterWidget::new(),
            )
            .await?
        },
        MangaProviders::Comizy => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                ComizyProvider::new(cache_provider),
                unified_tracker,
                ComizyFiltersProvider::new(ComizyFilterState::default()),
                ComizyFilterWidget::new(),
            )
            .await?
        },
    };

    if let Some(prepared) = pending_update {
        println!("{} {}...", tr("Aplicando actualización", "Applying update"), prepared.available.display_version);
        crate::backend::updater::apply_prepared_update_and_relaunch(&prepared)?;
    }

    Ok(())
}
