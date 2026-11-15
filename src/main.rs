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
use backend::manga_provider::comizy::filter_state::{ComizyFilterState, ComizyFiltersProvider};
use backend::manga_provider::comizy::filter_widget::ComizyFilterWidget;
use backend::manga_provider::inmanga::InmangaProvider;
use backend::manga_provider::inmanga::filter_state::{InmangaFilterState, InmangaFiltersProvider};
use backend::manga_provider::inmanga::filter_widget::InmangaFilterWidget;
use backend::manga_provider::leercapitulo::LeercapituloProvider;
use backend::manga_provider::leercapitulo::filter_state::{LeercapituloFilterState, LeercapituloFiltersProvider};
use backend::manga_provider::leercapitulo::filter_widget::LeercapituloFilterWidget;
use backend::manga_provider::mangadex::filter_widget::MangadexFilterWidget;
use backend::manga_provider::mangadex::filters::filter_provider::MangadexFilterProvider;
use backend::manga_provider::mangadex::{API_URL_BASE, COVER_IMG_URL_BASE, MangadexClient};
use backend::manga_provider::mangaoni::MangaoniProvider;
use backend::manga_provider::mangaoni::filter_state::{MangaoniFilterState, MangaoniFiltersProvider};
use backend::manga_provider::mangaoni::filter_widget::MangaoniFilterWidget;
use backend::manga_provider::weebcentral::filter_state::{WeebcentralFilterState, WeebcentralFiltersProvider};
use backend::manga_provider::weebcentral::filter_widget::WeebcentralFilterWidget;
use backend::manga_provider::weebcentral::{WEEBCENTRAL_BASE_URL, WeebcentralProvider};
use backend::manga_provider::{ChapterFilters, GetChapterPages, MangaPageProvider, MangaProviders, Pagination, SearchPageProvider};
use backend::release_notifier::{GITHUB_URL, ReleaseNotifier};
use backend::secrets::keyring::KeyringStorage;
use backend::tracker::anilist::{Anilist, BASE_ANILIST_API_URL};
use clap::Parser;
use cli::{Credentials, check_anilist_credentials_are_stored};
use crossterm::ExecutableCommand;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
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
use crate::backend::manga_provider::mangapill::filter_state::MangaPillFiltersProvider;
use crate::backend::manga_provider::mangapill::filter_widget::MangaPillFilterWidget;
use crate::backend::multi_provider::spanish_provider_candidates;
use crate::config::ImageQuality;

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
}

mod backend;
mod cli;
mod common;
mod config;
mod global;
mod i18n;
mod logger;
mod theme;
mod utils;
mod view;

/// Restores the terminal (raw mode, alternate screen and mouse capture) when dropped, so the user's
/// terminal is left in a usable state even if the app returns early with an error
struct TerminalGuard;

impl TerminalGuard {
    fn init() -> Result<(ratatui::DefaultTerminal, Self), Box<dyn std::error::Error>> {
        let terminal = ratatui::init();
        stdout().execute(EnableMouseCapture)?;
        Ok((terminal, Self))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = stdout().execute(DisableMouseCapture);
        ratatui::restore();
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 7)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
            "Data directory could not be created, this is where your manga history and manga downloads is stored
             \n this could be for many reasons such as the application not having enough permissions
            \n Try setting the environment variable `MANGA_TUI_DATA_DIR` to some path pointing to a directory, example: /home/user/somedirectory 
            \n Error details : {e}"
        ).into()
            );
            exit(1)
        },
    }

    let cli_args = CliArgs::parse();

    let manga_provider_cli = cli_args.manga_provider;

    cli_args.proccess_args().await?;

    let config = MangaTuiConfig::get();

    if config.check_new_updates
        && let Ok(github_url) = GITHUB_URL.parse()
    {
        tokio::spawn(async move {
            struct SilentLogger;
            impl ILogger for SilentLogger {
                fn inform(&self, _message: impl AsRef<str>) {}

                fn warn(&self, _warning: impl AsRef<str>) {}

                fn error(&self, _error: Box<dyn std::error::Error>) {}
            }
            let notifier = ReleaseNotifier::new(github_url);
            let _ = notifier.check_new_releases(&SilentLogger).await;
        });
    }

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

    if anilist_client.is_some() {
        logger.inform(tr(
            "Anilist configurado, se sincronizará el historial de lectura",
            "Anilist is setup, tracking reading history",
        ));
    }

    let mut connection = Database::get_connection()?;
    let database = Database::new(&connection);

    database.setup()?;
    update_database_with_migrations(&mut connection, &logger)?;

    drop(connection);

    color_eyre::install()?;

    let cache_provider: Arc<dyn Cacher> = InMemoryCache::init(8);

    let provider = if let Some(pro) = manga_provider_cli { pro } else { config.default_manga_provider };

    match provider {
        MangaProviders::Mangadex => {
            let mangadex_client =
                MangadexClient::new(API_URL_BASE.parse().unwrap(), COVER_IMG_URL_BASE.parse().unwrap(), cache_provider)
                    .with_image_quality(config.image_quality);

            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                mangadex_client,
                anilist_client,
                MangadexFilterProvider::from(get_cached_filters()),
                MangadexFilterWidget::new(),
            )
            .await?;
        },
        MangaProviders::Weebcentral => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                WeebcentralProvider::new(WEEBCENTRAL_BASE_URL.parse().unwrap(), cache_provider),
                anilist_client,
                WeebcentralFiltersProvider::new(WeebcentralFilterState::default()),
                WeebcentralFilterWidget::new(),
            )
            .await?;
        },
        MangaProviders::Mangapill => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                MangaPillProvider::init(cache_provider),
                anilist_client,
                MangaPillFiltersProvider::new(backend::manga_provider::mangapill::filter_state::MangaPillFilterState::default()),
                MangaPillFilterWidget::new(),
            )
            .await?;
        },
        MangaProviders::Inmanga => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                InmangaProvider::new(cache_provider),
                anilist_client,
                InmangaFiltersProvider::new(InmangaFilterState::default()),
                InmangaFilterWidget::new(),
            )
            .await?;
        },
        MangaProviders::Mangaoni => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                MangaoniProvider::new(cache_provider),
                anilist_client,
                MangaoniFiltersProvider::new(MangaoniFilterState::default()),
                MangaoniFilterWidget::new(),
            )
            .await?;
        },
        MangaProviders::Leercapitulo => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                LeercapituloProvider::new(cache_provider),
                anilist_client,
                LeercapituloFiltersProvider::new(LeercapituloFilterState::default()),
                LeercapituloFilterWidget::new(),
            )
            .await?;
        },
        MangaProviders::Comizy => {
            let (terminal, _guard) = TerminalGuard::init()?;
            run_app(
                terminal,
                ComizyProvider::new(cache_provider),
                anilist_client,
                ComizyFiltersProvider::new(ComizyFilterState::default()),
                ComizyFilterWidget::new(),
            )
            .await?;
        },
    }

    Ok(())
}
