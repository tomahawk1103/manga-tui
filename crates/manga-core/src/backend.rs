use std::fs::{create_dir, create_dir_all};
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

use self::error_log::create_error_logs_files;
use crate::config::build_config_file;
use crate::exists;
use crate::logger::ILogger;

pub mod anime_history;
pub mod cache;
pub mod cloud;
pub mod database;
pub mod download_queue;
pub mod error_log;
pub mod html_parser;
pub mod manga_downloader;
pub mod manga_provider;
pub mod migration;
pub mod multi_provider;
pub mod offline;
pub mod release_notifier;
pub mod secrets;
pub mod tracker;
pub mod updater;
pub mod web_viewer;

#[derive(Display, EnumIter, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppDirectories {
    #[strum(to_string = "mangaDownloads")]
    MangaDownloads,
    #[strum(to_string = "errorLogs")]
    ErrorLogs,
    #[strum(to_string = "history")]
    History,
}

static ERROR_LOGS_FILE: &str = "manga-tui-error-logs.txt";

static DATABASE_FILE: &str = "manga-tui-history.db";

impl AppDirectories {
    pub fn get_full_path(self) -> PathBuf {
        if matches!(self, Self::MangaDownloads) {
            let config = crate::config::MangaTuiConfig::get();
            if let Some(custom_dir) = config.get_custom_download_dir() {
                return custom_dir;
            }
        }
        Self::get_app_directory().join(self.get_path())
    }

    pub fn build_if_not_exists(app_directory: &Path) -> Result<(), std::io::Error> {
        for dir in AppDirectories::iter() {
            let directory_path = app_directory.join(dir.to_string());
            if !exists!(&directory_path) {
                create_dir(directory_path)?;
            }
        }
        Ok(())
    }

    pub fn get_app_directory() -> &'static Path {
        APP_DATA_DIR.as_ref().unwrap()
    }

    pub fn get_base_directory(self) -> PathBuf {
        Self::get_app_directory().join(self.to_string())
    }

    pub fn get_path(self) -> PathBuf {
        let base_directory = self.to_string();
        match self {
            Self::History => PathBuf::from(base_directory).join(DATABASE_FILE),
            Self::ErrorLogs => PathBuf::from(base_directory).join(ERROR_LOGS_FILE),
            Self::MangaDownloads => PathBuf::from(base_directory),
        }
    }
}

pub const PORTABLE_MARKER_FILE: &str = "portable.txt";
pub const PORTABLE_DATA_DIR_NAME: &str = "data";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    EnvOverride,
    Portable,
    System,
    AppSandbox,
}

impl StorageMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::EnvOverride => "MANGA_TUI_DATA_DIR",
            Self::Portable => "Portable (data/)",
            Self::System => "Sistema / System",
            Self::AppSandbox => "App Sandbox",
        }
    }
}

/// Returns `true` if `exe_dir` contains a `portable.txt` marker file or an existing `data/` directory.
pub fn is_portable_next_to_exe(exe_dir: &Path) -> bool {
    exe_dir.join(PORTABLE_MARKER_FILE).is_file() || exe_dir.join(PORTABLE_DATA_DIR_NAME).is_dir()
}

/// Resolves the active data/config directory following strict precedence:
/// 1. `MANGA_TUI_DATA_DIR` environment variable (if non-empty)
/// 2. `<exe_dir>/data` if `portable.txt` or `data/` exists next to the executable
/// 3. OS system directory (`directories::ProjectDirs`)
pub fn resolve_data_dir_with_precedence(
    env_data_dir: Option<&str>,
    exe_dir: Option<&Path>,
    system_dir: Option<PathBuf>,
) -> (Option<PathBuf>, StorageMode) {
    if let Some(raw_env) = env_data_dir.map(str::trim).filter(|s| !s.is_empty()) {
        return (Some(PathBuf::from(raw_env)), StorageMode::EnvOverride);
    }

    if let Some(dir) = exe_dir
        && is_portable_next_to_exe(dir)
    {
        return (Some(dir.join(PORTABLE_DATA_DIR_NAME)), StorageMode::Portable);
    }

    (system_dir, StorageMode::System)
}

pub fn current_executable_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
}

pub fn current_storage_mode() -> StorageMode {
    if CUSTOM_DATA_DIR.get().is_some() {
        return StorageMode::AppSandbox;
    }
    let env_dir = std::env::var("MANGA_TUI_DATA_DIR").ok();
    let exe_dir = current_executable_dir();
    let (_, mode) = resolve_data_dir_with_precedence(env_dir.as_deref(), exe_dir.as_deref(), None);
    mode
}

static CUSTOM_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static CUSTOM_CACHE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Sets an explicit application data directory provided by the host (e.g. Tauri on desktop or Android/iOS).
/// Must be called before `APP_DATA_DIR` is initialized.
pub fn set_custom_data_dir(dir: PathBuf) -> Result<(), PathBuf> {
    CUSTOM_DATA_DIR.set(dir)
}

/// Sets an explicit application cache directory provided by the host (e.g. Tauri on Android/iOS).
/// Must be called before cache directories are resolved.
pub fn set_custom_cache_dir(dir: PathBuf) -> Result<(), PathBuf> {
    CUSTOM_CACHE_DIR.set(dir)
}

/// Returns the explicit host-provided cache directory, if configured.
pub fn custom_cache_dir() -> Option<&'static PathBuf> {
    CUSTOM_CACHE_DIR.get()
}

#[cfg(not(any(test, feature = "test-utils")))]
pub static APP_DATA_DIR: Lazy<Option<PathBuf>> = Lazy::new(|| {
    if let Some(custom) = CUSTOM_DATA_DIR.get() {
        return Some(custom.clone());
    }
    let env_dir = std::env::var("MANGA_TUI_DATA_DIR").ok();
    let exe_dir = current_executable_dir();
    let system_dir = directories::ProjectDirs::from("", "", "manga-tui").map(|dirs| dirs.data_dir().to_path_buf());
    resolve_data_dir_with_precedence(env_dir.as_deref(), exe_dir.as_deref(), system_dir).0
});

#[cfg(any(test, feature = "test-utils"))]
pub static APP_DATA_DIR: Lazy<Option<PathBuf>> = Lazy::new(|| {
    CUSTOM_DATA_DIR
        .get()
        .cloned()
        .or_else(|| Some(PathBuf::from("./test_results/data-directory")))
});

pub fn build_data_dir(logger: &impl ILogger) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let data_dir = APP_DATA_DIR.as_ref();
    match data_dir {
        Some(dir) => {
            if !exists!(dir) {
                create_dir_all(dir)?;
                logger.inform(format!("Creating directory: {}", dir.display()));
            }

            AppDirectories::build_if_not_exists(dir)?;

            create_error_logs_files(dir)?;

            build_config_file()?;

            let config = crate::config::MangaTuiConfig::get();
            if let Some(custom_dir) = config.get_custom_download_dir()
                && !exists!(&custom_dir)
                && let Err(e) = create_dir_all(&custom_dir)
            {
                logger.error(format!("Could not create custom download directory {}: {e}", custom_dir.display()).into());
            }

            Ok(dir.to_path_buf())
        },
        None => Err("data dir could not be found".into()),
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;
    use std::fs;
    use std::thread::sleep;
    use std::time::Duration;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::logger::DefaultLogger;

    #[test]
    #[ignore]
    fn data_directory_is_built() -> Result<(), Box<dyn Error>> {
        sleep(Duration::from_millis(1000));
        dbg!(build_data_dir(&DefaultLogger).expect("Could not build data directory"));

        let mut amount_directories = 0;

        let directory_built = fs::read_dir(AppDirectories::get_app_directory())?;

        for dir in directory_built {
            let dir = dir?;

            let directory_name = dir.file_name();

            let directory_was_created =
                AppDirectories::iter().any(|app_dir| app_dir.to_string() == directory_name.to_string_lossy());

            assert!(directory_was_created);

            amount_directories += 1;
        }

        assert_eq!(3, amount_directories);

        let error_logs_path = dbg!(AppDirectories::ErrorLogs.get_full_path());

        fs::File::open(error_logs_path).expect("Could not open error logs file");

        Ok(())
    }

    #[test]
    fn portable_precedence_respects_env_then_portable_then_system() {
        let base = PathBuf::from("./test_results/portable_precedence_test");
        let _ = fs::remove_dir_all(&base);
        let exe_dir_with_marker = base.join("with_marker");
        let exe_dir_with_data_folder = base.join("with_data_folder");
        let exe_dir_plain = base.join("plain");
        fs::create_dir_all(&exe_dir_with_marker).unwrap();
        fs::create_dir_all(exe_dir_with_data_folder.join("data")).unwrap();
        fs::create_dir_all(&exe_dir_plain).unwrap();
        fs::write(exe_dir_with_marker.join(PORTABLE_MARKER_FILE), "portable").unwrap();

        let system_dir = Some(PathBuf::from("C:/System/AppData/manga-tui"));

        // 1. MANGA_TUI_DATA_DIR wins even when portable.txt exists
        let (resolved, mode) =
            resolve_data_dir_with_precedence(Some("E:/Custom/MangaData"), Some(&exe_dir_with_marker), system_dir.clone());
        assert_eq!(mode, StorageMode::EnvOverride);
        assert_eq!(resolved, Some(PathBuf::from("E:/Custom/MangaData")));

        // 2. Empty/whitespace env var falls back to portable.txt -> <exe_dir>/data
        let (resolved, mode) = resolve_data_dir_with_precedence(Some("   "), Some(&exe_dir_with_marker), system_dir.clone());
        assert_eq!(mode, StorageMode::Portable);
        assert_eq!(resolved, Some(exe_dir_with_marker.join("data")));

        // 3. Existing data/ directory activates portable mode even without portable.txt
        let (resolved, mode) = resolve_data_dir_with_precedence(None, Some(&exe_dir_with_data_folder), system_dir.clone());
        assert_eq!(mode, StorageMode::Portable);
        assert_eq!(resolved, Some(exe_dir_with_data_folder.join("data")));

        // 4. Without env override or portable markers, falls back to system dir
        let (resolved, mode) = resolve_data_dir_with_precedence(None, Some(&exe_dir_plain), system_dir.clone());
        assert_eq!(mode, StorageMode::System);
        assert_eq!(resolved, system_dir);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn portable_mode_handles_paths_with_spaces_and_accents_and_survives_exe_upgrade() {
        let base = PathBuf::from("./test_results/Álvaro Mis Mangas Portable/Versión 1");
        let _ = fs::remove_dir_all("./test_results/Álvaro Mis Mangas Portable");
        fs::create_dir_all(&base).unwrap();

        let marker = base.join(PORTABLE_MARKER_FILE);
        let exe_path = base.join("manga-tui.exe");
        fs::write(&marker, "portable mode enabled\n").unwrap();
        fs::write(&exe_path, b"binary-v0.10.0").unwrap();

        let (resolved, mode) =
            resolve_data_dir_with_precedence(None, Some(&base), Some(PathBuf::from("./test_results/unused_system_dir")));
        assert_eq!(mode, StorageMode::Portable);
        let data_dir = resolved.expect("portable data dir should resolve");
        assert_eq!(data_dir, base.join("data"));

        fs::create_dir_all(&data_dir).unwrap();
        AppDirectories::build_if_not_exists(&data_dir).unwrap();

        let config_path = data_dir.join("config.toml");
        let cbz_path = data_dir.join("mangaDownloads").join("Mi Manga Cap 1.cbz");
        let db_path = data_dir.join("history").join(DATABASE_FILE);

        fs::write(&config_path, "ui_language = \"es\"\n").unwrap();
        fs::write(&cbz_path, b"fake-cbz-archive").unwrap();
        fs::write(&db_path, b"sqlite-history-marker").unwrap();

        // Simulate updating the executable in place
        fs::write(&exe_path, b"binary-v0.10.1").unwrap();

        let (resolved_after, mode_after) = resolve_data_dir_with_precedence(None, Some(&base), None);
        assert_eq!(mode_after, StorageMode::Portable);
        assert_eq!(resolved_after, Some(data_dir.clone()));
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "ui_language = \"es\"\n");
        assert_eq!(fs::read(&cbz_path).unwrap(), b"fake-cbz-archive");
        assert_eq!(fs::read(&db_path).unwrap(), b"sqlite-history-marker");

        let _ = fs::remove_dir_all("./test_results/Álvaro Mis Mangas Portable");
    }

    #[test]
    fn installed_mode_preserves_system_user_data_on_upgrade_and_uninstall() {
        let base = PathBuf::from("./test_results/installer_preservation_test");
        let _ = fs::remove_dir_all(&base);
        let install_dir = base.join("Programs").join("Manga TUI");
        let system_data_dir = base.join("AppData").join("manga-tui").join("data");
        let system_config_dir = base.join("AppData").join("manga-tui").join("config");

        fs::create_dir_all(&install_dir).unwrap();
        fs::create_dir_all(&system_data_dir).unwrap();
        fs::create_dir_all(&system_config_dir).unwrap();

        let exe_path = install_dir.join("manga-tui.exe");
        fs::write(&exe_path, b"installed-binary-v0.10.0").unwrap();
        // Installed mode must not have portable.txt next to the executable
        assert!(!is_portable_next_to_exe(&install_dir));

        let (resolved_data, mode) = resolve_data_dir_with_precedence(None, Some(&install_dir), Some(system_data_dir.clone()));
        assert_eq!(mode, StorageMode::System);
        assert_eq!(resolved_data, Some(system_data_dir.clone()));

        AppDirectories::build_if_not_exists(&system_data_dir).unwrap();
        let config_path = system_config_dir.join("config.toml");
        let db_path = system_data_dir.join("history").join(DATABASE_FILE);
        let cbz_path = system_data_dir.join("mangaDownloads").join("Chapter 1.cbz");

        fs::write(&config_path, "download_type = \"cbz\"\n").unwrap();
        fs::write(&db_path, b"sqlite-user-history").unwrap();
        fs::write(&cbz_path, b"downloaded-cbz-bytes").unwrap();

        // Simulate in-place installer upgrade
        fs::write(&exe_path, b"installed-binary-v0.10.1").unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "download_type = \"cbz\"\n");
        assert_eq!(fs::read(&db_path).unwrap(), b"sqlite-user-history");
        assert_eq!(fs::read(&cbz_path).unwrap(), b"downloaded-cbz-bytes");

        // Simulate uninstaller deleting the entire installation directory
        fs::remove_dir_all(&install_dir).unwrap();
        assert!(!exe_path.exists());
        assert_eq!(fs::read_to_string(&config_path).unwrap(), "download_type = \"cbz\"\n");
        assert_eq!(fs::read(&db_path).unwrap(), b"sqlite-user-history");
        assert_eq!(fs::read(&cbz_path).unwrap(), b"downloaded-cbz-bytes");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn app_sandbox_storage_mode_has_expected_label() {
        assert_eq!(StorageMode::AppSandbox.label(), "App Sandbox");
    }
}
