use std::fs::create_dir_all;
use std::path::PathBuf;

use rusqlite::Connection;

use crate::backend::database::Database;
use crate::backend::error_log::create_error_logs_files;
use crate::backend::migration::update_database_with_migrations;
use crate::backend::{AppDirectories, set_custom_data_dir};
use crate::config::{build_config_file_in_dir, set_custom_config_dir};
use crate::error::{CoreError, CoreResult};
use crate::logger::Logger;

/// Platform-supplied filesystem paths used by a host (`manga-tui`, Mangabrary Desktop, or Android)
/// to initialize `manga-core` without relying on `current_executable_dir()` or global OS guesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub downloads_dir: Option<PathBuf>,
}

/// Initialized core runtime handle providing access to resolved directories and SQLite connections.
#[derive(Debug, Clone)]
pub struct CoreContext {
    paths: CorePaths,
}

impl CoreContext {
    /// Initializes directories, registers host path overrides, loads/creates `config.toml`,
    /// sets up the SQLite database schema, and applies pending migrations.
    pub fn init(paths: CorePaths) -> CoreResult<Self> {
        Self::init_internal(paths, true)
    }

    /// Same setup as [`CoreContext::init`] without registering process-wide custom directory globals.
    /// Useful for isolated integration tests.
    pub fn init_isolated(paths: CorePaths) -> CoreResult<Self> {
        Self::init_internal(paths, false)
    }

    fn init_internal(paths: CorePaths, register_globals: bool) -> CoreResult<Self> {
        create_dir_all(&paths.data_dir)?;
        create_dir_all(&paths.config_dir)?;
        AppDirectories::build_if_not_exists(&paths.data_dir)?;
        create_error_logs_files(&paths.data_dir)?;

        if let Some(downloads_dir) = &paths.downloads_dir {
            create_dir_all(downloads_dir)?;
        }

        if register_globals {
            let _ = set_custom_data_dir(paths.data_dir.clone());
            let _ = set_custom_config_dir(paths.config_dir.clone());
        }

        build_config_file_in_dir(&paths.config_dir).map_err(|e| CoreError::Config(e.to_string()))?;

        let ctx = Self { paths };
        let mut conn = ctx.open_database()?;
        Database::new(&conn).setup()?;
        update_database_with_migrations(&mut conn, &Logger)?;

        Ok(ctx)
    }

    /// Returns the paths configured for this context.
    pub fn paths(&self) -> &CorePaths {
        &self.paths
    }

    /// Opens a SQLite connection pointing at `<data_dir>/history/manga-tui-history.db`
    /// with recommended concurrency and integrity PRAGMAs enabled.
    pub fn open_database(&self) -> CoreResult<Connection> {
        let db_path = self.paths.data_dir.join(AppDirectories::History.get_path());
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA foreign_keys = ON;
            PRAGMA synchronous = NORMAL;
            ",
        )?;
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_context_init_creates_directories_config_and_database() {
        let base = std::env::temp_dir().join(format!("mangabrary_core_context_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        let paths = CorePaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            downloads_dir: Some(base.join("downloads")),
        };

        let ctx = CoreContext::init_isolated(paths.clone()).expect("CoreContext::init_isolated should succeed");
        assert_eq!(ctx.paths(), &paths);
        assert!(paths.config_dir.join("config.toml").is_file());
        assert!(paths.data_dir.join(AppDirectories::MangaDownloads.get_path()).is_dir());
        assert!(paths.data_dir.join(AppDirectories::ErrorLogs.get_path()).is_file());
        assert!(paths.data_dir.join(AppDirectories::History.get_path()).is_file());
        assert!(base.join("downloads").is_dir());

        let conn = ctx.open_database().expect("open_database should succeed");
        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('mangas', 'chapters', 'migrations')",
                [],
                |row| row.get(0),
            )
            .expect("querying sqlite_master should succeed");
        assert_eq!(table_count, 3);
        drop(conn);

        let _ = std::fs::remove_dir_all(&base);
    }
}
