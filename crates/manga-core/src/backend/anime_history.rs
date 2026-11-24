use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anime_core::{
    AnimeDetails, AnimeEpisode, AnimeError, AnimePlaybackServer, AnimeProvider, AnimeProviders, PlaybackHistory, PlaybackProgress,
    VideoMirror,
};
use rusqlite::{Connection, OptionalExtension, params};

use super::database::Database;
use crate::config::AnimeConfig;

/// Starts playback with the same history database used by the TUI.
pub async fn start_anime_playback(
    provider: Arc<impl AnimeProvider + 'static>,
    anime: &AnimeDetails,
    episodes: Vec<AnimeEpisode>,
    episode: usize,
    settings: &AnimeConfig,
    preferred_mirror: Option<&VideoMirror>,
) -> Result<AnimePlaybackServer, AnimeError> {
    start_anime_playback_with_owner(provider, anime, anime, episodes, episode, settings, preferred_mirror).await
}

/// Opens a stream from another catalog while keeping progress under the selected anime.
pub async fn start_anime_playback_with_owner(
    provider: Arc<impl AnimeProvider + 'static>,
    source: &AnimeDetails,
    owner: &AnimeDetails,
    episodes: Vec<AnimeEpisode>,
    episode: usize,
    settings: &AnimeConfig,
    preferred_mirror: Option<&VideoMirror>,
) -> Result<AnimePlaybackServer, AnimeError> {
    let connection = tokio::task::spawn_blocking(Database::get_connection)
        .await
        .map_err(persistence_error)?
        .map_err(persistence_error)?;
    let history = Arc::new(SqlitePlaybackHistory {
        connection: Mutex::new(connection),
        anime: owner.clone(),
        watched_percent: settings.auto_mark_watched_percent,
    });
    AnimePlaybackServer::start_with_history(
        provider,
        source,
        episodes,
        episode,
        settings.preferred_variant,
        preferred_mirror,
        Some(history),
    )
    .await
}

struct SqlitePlaybackHistory {
    connection: Mutex<Connection>,
    anime: AnimeDetails,
    watched_percent: u8,
}

impl PlaybackHistory for SqlitePlaybackHistory {
    fn load(&self) -> Result<HashMap<String, PlaybackProgress>, AnimeError> {
        let connection = self.connection.lock().map_err(persistence_error)?;
        let episodes = Database::new(&connection)
            .anime_episode_history(self.anime.provider, &self.anime.slug)
            .map_err(persistence_error)?;
        Ok(episodes
            .into_iter()
            .map(|episode| {
                (episode.episode_number, PlaybackProgress {
                    position_seconds: f64::from(episode.progress_seconds),
                    duration_seconds: f64::from(episode.duration_seconds),
                })
            })
            .collect())
    }

    fn save(&self, episode: &AnimeEpisode, progress: &PlaybackProgress) -> Result<(), AnimeError> {
        let connection = self.connection.lock().map_err(persistence_error)?;
        Database::new(&connection)
            .save_anime_progress(&self.anime, episode, progress, self.watched_percent)
            .map_err(persistence_error)
    }
}

fn persistence_error(error: impl std::fmt::Display) -> AnimeError {
    AnimeError::Persistence(error.to_string())
}

pub(super) const SCHEMA: [&str; 4] = [
    "CREATE TABLE IF NOT EXISTS animes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        provider TEXT NOT NULL,
        slug TEXT NOT NULL,
        title TEXT NOT NULL,
        cover_url TEXT,
        status TEXT,
        is_favorite BOOLEAN NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT (datetime('now')),
        last_watched_at DATETIME,
        UNIQUE (provider, slug)
    )",
    "CREATE TABLE IF NOT EXISTS anime_episodes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        anime_id INTEGER NOT NULL,
        episode_number TEXT NOT NULL,
        episode_slug TEXT NOT NULL,
        is_watched BOOLEAN NOT NULL DEFAULT 0,
        progress_seconds INTEGER NOT NULL DEFAULT 0,
        duration_seconds INTEGER NOT NULL DEFAULT 0,
        last_watched_at DATETIME,
        FOREIGN KEY (anime_id) REFERENCES animes(id) ON DELETE CASCADE,
        UNIQUE (anime_id, episode_number)
    )",
    "CREATE INDEX IF NOT EXISTS idx_animes_provider_last_watched ON animes(provider, last_watched_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_anime_episodes_anime_number ON anime_episodes(anime_id, episode_number)",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeHistoryEntry {
    pub slug: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub status: Option<String>,
    pub is_favorite: bool,
    pub last_watched_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimeEpisodeHistory {
    pub episode_number: String,
    pub is_watched: bool,
    pub progress_seconds: u32,
    pub duration_seconds: u32,
}

impl Database<'_> {
    pub fn anime_history(&self, provider: AnimeProviders, favorites_only: bool) -> rusqlite::Result<Vec<AnimeHistoryEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT slug, title, cover_url, status, is_favorite, last_watched_at FROM animes
             WHERE provider = ?1 AND ((?2 AND is_favorite) OR (NOT ?2 AND last_watched_at IS NOT NULL))
             ORDER BY last_watched_at DESC, created_at DESC, id DESC",
        )?;
        statement
            .query_map(params![provider.as_id(), favorites_only], |row| {
                Ok(AnimeHistoryEntry {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    cover_url: row.get(2)?,
                    status: row.get(3)?,
                    is_favorite: row.get(4)?,
                    last_watched_at: row.get(5)?,
                })
            })?
            .collect()
    }

    pub fn is_anime_favorite(&self, provider: AnimeProviders, slug: &str) -> rusqlite::Result<bool> {
        self.connection
            .query_row("SELECT is_favorite FROM animes WHERE provider = ?1 AND slug = ?2", params![provider.as_id(), slug], |row| {
                row.get(0)
            })
            .optional()
            .map(|favorite| favorite.unwrap_or(false))
    }

    pub fn set_anime_favorite(&self, anime: &AnimeDetails, favorite: bool) -> rusqlite::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let id = upsert_anime(&transaction, anime)?;
        transaction.execute("UPDATE animes SET is_favorite = ?2 WHERE id = ?1", params![id, favorite])?;
        transaction.commit()
    }

    pub fn anime_episode_history(&self, provider: AnimeProviders, slug: &str) -> rusqlite::Result<Vec<AnimeEpisodeHistory>> {
        let mut statement = self.connection.prepare(
            "SELECT episode_number, is_watched, progress_seconds, duration_seconds
             FROM anime_episodes JOIN animes ON animes.id = anime_episodes.anime_id
             WHERE provider = ?1 AND slug = ?2",
        )?;
        statement
            .query_map(params![provider.as_id(), slug], |row| {
                Ok(AnimeEpisodeHistory {
                    episode_number: row.get(0)?,
                    is_watched: row.get(1)?,
                    progress_seconds: row.get(2)?,
                    duration_seconds: row.get(3)?,
                })
            })?
            .collect()
    }

    pub fn set_anime_episode_watched(&self, anime: &AnimeDetails, episode: &AnimeEpisode, watched: bool) -> rusqlite::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let id = upsert_anime(&transaction, anime)?;
        transaction.execute(
            "INSERT INTO anime_episodes(anime_id, episode_number, episode_slug, is_watched, last_watched_at)
             VALUES (?1, ?2, ?3, ?4, CASE WHEN ?4 THEN datetime('now') END)
             ON CONFLICT(anime_id, episode_number) DO UPDATE SET episode_slug = excluded.episode_slug,
               is_watched = excluded.is_watched,
               last_watched_at = COALESCE(excluded.last_watched_at, anime_episodes.last_watched_at)",
            params![id, episode.number, episode_slug(episode), watched],
        )?;
        if watched {
            touch_anime(&transaction, id)?;
        }
        transaction.commit()
    }

    /// Saves the latest playback position. Watched status survives subsequent rewinds.
    pub fn save_anime_progress(
        &self,
        anime: &AnimeDetails,
        episode: &AnimeEpisode,
        progress: &PlaybackProgress,
        watched_percent: u8,
    ) -> rusqlite::Result<()> {
        let position = progress.position_seconds;
        let duration = progress.duration_seconds;
        if !position.is_finite()
            || !duration.is_finite()
            || position < 0.0
            || duration <= 0.0
            || position > duration
            || duration > f64::from(u32::MAX)
        {
            return Err(rusqlite::Error::InvalidParameterName("anime playback position/duration".into()));
        }
        let watched = position / duration * 100.0 >= f64::from(watched_percent.clamp(50, 100));
        let transaction = self.connection.unchecked_transaction()?;
        let id = upsert_anime(&transaction, anime)?;
        transaction.execute(
            "INSERT INTO anime_episodes(anime_id, episode_number, episode_slug, is_watched,
                progress_seconds, duration_seconds, last_watched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(anime_id, episode_number) DO UPDATE SET
                episode_slug = excluded.episode_slug,
                is_watched = (anime_episodes.is_watched OR excluded.is_watched),
                progress_seconds = excluded.progress_seconds, duration_seconds = excluded.duration_seconds,
                last_watched_at = excluded.last_watched_at",
            params![id, episode.number, episode_slug(episode), watched, position as u32, duration as u32],
        )?;
        touch_anime(&transaction, id)?;
        transaction.commit()
    }
}

fn episode_slug(episode: &AnimeEpisode) -> &str {
    episode.id.as_deref().unwrap_or(&episode.number)
}

fn upsert_anime(connection: &Connection, anime: &AnimeDetails) -> rusqlite::Result<i64> {
    connection.query_row(
        "INSERT INTO animes(provider, slug, title, cover_url, status) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(provider, slug) DO UPDATE SET title = excluded.title, cover_url = excluded.cover_url, status = excluded.status
         RETURNING id",
        params![anime.provider.as_id(), anime.slug, anime.title, anime.cover_url, anime.status.to_string()],
        |row| row.get(0),
    )
}

fn touch_anime(connection: &Connection, id: i64) -> rusqlite::Result<()> {
    connection.execute("UPDATE animes SET last_watched_at = datetime('now') WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_resumes_fractional_episodes_and_preserves_watched_status_on_rewind() -> rusqlite::Result<()> {
        let connection = Database::get_connection()?;
        let db = Database::new(&connection);
        db.setup()?;
        let anime = AnimeDetails {
            id: "test".into(),
            slug: "test".into(),
            title: "Test anime".into(),
            synopsis: String::new(),
            cover_url: None,
            genres: Vec::new(),
            status: anime_core::AnimeStatus::Completed,
            anime_type: anime_core::AnimeType::Tv,
            score: None,
            mal_id: None,
            episodes_count: None,
            provider: AnimeProviders::AnimeAv1,
            episodes: Vec::new(),
            csrf_token: None,
        };
        let episode = AnimeEpisode {
            id: Some("episode-id".into()),
            number: "12.5".into(),
            title: None,
            thumbnail_url: None,
            aired_at: None,
        };

        assert!(!db.is_anime_favorite(anime.provider, &anime.slug)?);
        db.set_anime_favorite(&anime, true)?;
        assert_eq!(db.anime_history(anime.provider, true)?.len(), 1);
        assert!(db.anime_history(anime.provider, false)?.is_empty());

        let progress = |position_seconds| PlaybackProgress {
            position_seconds,
            duration_seconds: 100.0,
        };
        db.save_anime_progress(&anime, &episode, &progress(84.9), 85)?;
        assert!(!db.anime_episode_history(anime.provider, &anime.slug)?[0].is_watched);
        db.save_anime_progress(&anime, &episode, &progress(85.0), 85)?;
        db.save_anime_progress(&anime, &episode, &progress(14.5), 85)?;
        assert_eq!(db.anime_episode_history(anime.provider, &anime.slug)?, vec![AnimeEpisodeHistory {
            episode_number: "12.5".into(),
            is_watched: true,
            progress_seconds: 14,
            duration_seconds: 100
        }]);
        assert_eq!(db.anime_history(anime.provider, false)?.len(), 1);
        assert!(db.anime_history(AnimeProviders::JkAnime, false)?.is_empty());
        db.set_anime_episode_watched(&anime, &episode, false)?;
        assert!(!db.anime_episode_history(anime.provider, &anime.slug)?[0].is_watched);
        assert!(db.is_anime_favorite(anime.provider, &anime.slug)?);

        let store = SqlitePlaybackHistory {
            connection: Mutex::new(connection),
            anime,
            watched_percent: 85,
        };
        assert_eq!(store.load().unwrap()["12.5"].position_seconds, 14.0);
        store.save(&episode, &progress(32.0)).unwrap();
        assert_eq!(store.load().unwrap()["12.5"].position_seconds, 32.0);
        Ok(())
    }
}
