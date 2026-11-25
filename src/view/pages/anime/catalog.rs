use std::collections::HashSet;
use std::sync::Arc;

use anime_core::provider::animeav1::AnimeAv1Provider;
use anime_core::provider::jkanime::JkAnimeProvider;
use anime_core::{
    AnimeDetails, AnimeEpisode, AnimeEpisodesPage, AnimeError, AnimeHomeFeed, AnimePlaybackServer, AnimeProvider, AnimeProviders,
    AnimeSearchResult, AnimeSummary, ExtractorRegistry, VideoMirror, build_default_http_client,
};
use manga_core::backend::anime_history::{AnimeEpisodeHistory, start_anime_playback, start_anime_playback_with_owner};

use crate::backend::database::Database;
use crate::config::AnimeConfig;

enum Provider {
    AnimeAv1(AnimeAv1Provider),
    JkAnime(JkAnimeProvider),
}

macro_rules! dispatch {
    ($self:expr, $method:ident($($arg:expr),*)) => {
        match $self {
            Provider::AnimeAv1(provider) => provider.$method($($arg),*).await,
            Provider::JkAnime(provider) => provider.$method($($arg),*).await,
        }
    };
}

impl Provider {
    fn new(kind: AnimeProviders) -> Result<Self, AnimeError> {
        let client = build_default_http_client()?;
        Ok(match kind {
            AnimeProviders::AnimeAv1 => Self::AnimeAv1(AnimeAv1Provider::new(client)),
            AnimeProviders::JkAnime => Self::JkAnime(JkAnimeProvider::new(client)),
        })
    }
}

impl AnimeProvider for Provider {
    fn kind(&self) -> AnimeProviders {
        match self {
            Self::AnimeAv1(_) => AnimeProviders::AnimeAv1,
            Self::JkAnime(_) => AnimeProviders::JkAnime,
        }
    }

    async fn get_home_feed(&self) -> Result<AnimeHomeFeed, AnimeError> {
        dispatch!(self, get_home_feed())
    }

    async fn search(&self, query: &str, page: u32) -> Result<AnimeSearchResult, AnimeError> {
        dispatch!(self, search(query, page))
    }

    async fn get_details(&self, slug: &str) -> Result<AnimeDetails, AnimeError> {
        dispatch!(self, get_details(slug))
    }

    async fn get_episodes(&self, details: &AnimeDetails, page: u32) -> Result<AnimeEpisodesPage, AnimeError> {
        dispatch!(self, get_episodes(details, page))
    }

    async fn get_episode_mirrors(&self, slug: &str, number: &str) -> Result<Vec<VideoMirror>, AnimeError> {
        dispatch!(self, get_episode_mirrors(slug, number))
    }
}

pub(crate) struct Details {
    pub anime: AnimeDetails,
    pub episodes: Vec<AnimeEpisode>,
    pub history: Vec<AnimeEpisodeHistory>,
    pub favorite: bool,
}

pub(super) async fn home_feed(provider: AnimeProviders) -> Result<(AnimeHomeFeed, HashSet<String>), AnimeError> {
    let feed = Provider::new(provider)?.get_home_feed().await?;
    let favorites = db(move |db| db.anime_history(provider, true))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.slug)
        .collect();
    Ok((feed, favorites))
}

pub(super) async fn search(provider: AnimeProviders, query: String, page: u32) -> Result<AnimeSearchResult, AnimeError> {
    Provider::new(provider)?.search(&query, page).await
}

pub(super) async fn details(kind: AnimeProviders, slug: String) -> Result<Details, AnimeError> {
    let provider = Provider::new(kind)?;
    let anime = provider.get_details(&slug).await?;
    let first = provider.get_episodes(&anime, 1).await?;
    let mut episodes = first.episodes;
    for page in 2..=first.total_pages {
        episodes.extend(provider.get_episodes(&anime, page).await?.episodes);
    }
    episodes.sort_by(|left, right| {
        match (left.number.parse::<f64>(), right.number.parse::<f64>()) {
            (Ok(left), Ok(right)) => left.total_cmp(&right),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            _ => left.number.cmp(&right.number),
        }
        .then_with(|| left.number.cmp(&right.number))
    });
    episodes.dedup_by(|left, right| left.number == right.number);
    let (history, favorite) =
        db(move |db| Ok((db.anime_episode_history(kind, &slug)?, db.is_anime_favorite(kind, &slug)?))).await?;
    Ok(Details {
        anime,
        episodes,
        history,
        favorite,
    })
}

pub(super) async fn mirrors(anime: AnimeDetails, number: String) -> Result<Vec<VideoMirror>, AnimeError> {
    let registry = ExtractorRegistry::default();
    let mut mirrors = Provider::new(anime.provider)?.get_episode_mirrors(&anime.slug, &number).await?;
    mirrors.retain(|mirror| registry.supports_mirror(mirror));
    Ok(mirrors)
}

pub(super) async fn playback(
    anime: AnimeDetails,
    episodes: Vec<AnimeEpisode>,
    index: usize,
    config: &AnimeConfig,
    mirror: Option<VideoMirror>,
) -> Result<(AnimePlaybackServer, AnimeProviders), AnimeError> {
    let provider = Arc::new(Provider::new(anime.provider)?);
    let original = match start_anime_playback(provider, &anime, episodes.clone(), index, config, mirror.as_ref()).await {
        Ok(server) => return Ok((server, anime.provider)),
        Err(error @ AnimeError::Persistence(_)) => return Err(error),
        Err(error) => error,
    };
    log::debug!("Anime playback on {} failed: {original}", anime.provider);
    let alternate = match anime.provider {
        AnimeProviders::AnimeAv1 => AnimeProviders::JkAnime,
        AnimeProviders::JkAnime => AnimeProviders::AnimeAv1,
    };
    let fallback = async {
        let provider = Arc::new(Provider::new(alternate)?);
        let candidates = provider.search(&anime.title, 1).await?;
        let Some(candidate) = unique_title(&anime.title, &candidates.items) else { return Ok(None) };
        let source = provider.get_details(&candidate.slug).await?;
        if title_key(&source.title) != title_key(&anime.title)
            || matches!((anime.mal_id, source.mal_id), (Some(a), Some(b)) if a != b)
            || (anime.anime_type != anime_core::AnimeType::Unknown
                && source.anime_type != anime_core::AnimeType::Unknown
                && anime.anime_type != source.anime_type)
        {
            return Ok(None);
        }
        start_anime_playback_with_owner(provider, &source, &anime, episodes, index, config, None)
            .await
            .map(Some)
    }
    .await;
    match (fallback, original) {
        (Ok(Some(server)), _) => Ok((server, alternate)),
        (Err(AnimeError::MirrorsFailed(other)), AnimeError::MirrorsFailed(mut failures)) => {
            failures.extend(other);
            Err(AnimeError::MirrorsFailed(failures))
        },
        (Err(error), original @ AnimeError::MirrorsFailed(_)) => {
            log::debug!("Anime playback fallback on {alternate} failed: {error}");
            Err(original)
        },
        (Err(error), _) => Err(error),
        (Ok(None), original) => Err(original),
    }
}

fn title_key(title: &str) -> String {
    title.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ")
}

fn unique_title<'a>(title: &str, candidates: &'a [AnimeSummary]) -> Option<&'a AnimeSummary> {
    let key = title_key(title);
    let mut matches = candidates.iter().filter(|candidate| title_key(&candidate.title) == key);
    let first = matches.next()?;
    matches.all(|candidate| candidate.slug == first.slug).then_some(first)
}

pub(super) async fn db<T: Send + 'static>(
    operation: impl FnOnce(Database<'_>) -> rusqlite::Result<T> + Send + 'static,
) -> Result<T, AnimeError> {
    tokio::task::spawn_blocking(move || {
        let connection = Database::get_connection()?;
        #[cfg(test)]
        Database::new(&connection).setup()?;
        operation(Database::new(&connection))
    })
    .await
    .map_err(|error| AnimeError::Persistence(error.to_string()))?
    .map_err(|error| AnimeError::Persistence(error.to_string()))
}

pub(super) async fn cover(url: String) -> Option<image::DynamicImage> {
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    let mut response = build_default_http_client().ok()?.get(url).send().await.ok()?.error_for_status().ok()?;
    if response.content_length().is_some_and(|length| length > MAX_BYTES as u64) {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    tokio::task::spawn_blocking(move || {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().ok()
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(slug: &str, title: &str) -> AnimeSummary {
        AnimeSummary {
            id: slug.into(),
            slug: slug.into(),
            title: title.into(),
            cover_url: None,
            synopsis: None,
            anime_type: anime_core::AnimeType::Tv,
            status: anime_core::AnimeStatus::Completed,
            provider: AnimeProviders::JkAnime,
        }
    }

    #[test]
    fn alternate_catalog_requires_one_exact_series_match() {
        let mut results = vec![
            candidate("frieren-2", "Sousou no Frieren 2nd Season"),
            candidate("frieren", "Sousou no Frieren"),
            candidate("frieren-special", "Sousou no Frieren: Special"),
        ];
        assert_eq!(unique_title(" SOUSOU no   Frieren ", &results).unwrap().slug, "frieren");
        assert!(unique_title("Sousou no Frieren 3rd Season", &results).is_none());
        results.push(candidate("duplicate-title", "Sousou no Frieren"));
        assert!(unique_title("Sousou no Frieren", &results).is_none());
    }
}
