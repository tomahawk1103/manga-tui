pub mod extractor;
pub mod model;
pub mod playback_server;
pub mod player;
pub mod provider;

use std::fmt::{Display, Formatter};
use std::time::Duration;

pub use extractor::{BuiltinExtractor, ExtractorRegistry, VideoExtractor};
pub use model::{
    AnimeDetails, AnimeEpisode, AnimeEpisodesPage, AnimeHomeFeed, AnimeProviders, AnimeSearchResult, AnimeStatus, AnimeSummary,
    AnimeType, AnimeVariant, RecentAnimeEpisode, ResolvedStream, StreamKind, VideoMirror,
};
pub use playback_server::{AnimePlaybackServer, PlaybackHistory, PlaybackProgress};
pub use player::{ExternalPlayer, ExternalPlayerLauncher};
pub use provider::AnimeProvider;

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimeError {
    Http {
        status: Option<u16>,
        message: String,
    },
    Parse(String),
    NotFound(String),
    RateLimited(String),
    UnsupportedMirror(String),
    ExtractionFailed(String),
    InvalidInput(String),
    Persistence(String),
}

impl Display for AnimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http {
                status: Some(code),
                message,
            } => write!(f, "HTTP {code}: {message}"),
            Self::Http {
                status: None,
                message,
            } => write!(f, "HTTP error: {message}"),
            Self::Parse(msg) => write!(f, "Parse error: {msg}"),
            Self::NotFound(msg) => write!(f, "Not found: {msg}"),
            Self::RateLimited(msg) => write!(f, "Rate limited: {msg}"),
            Self::UnsupportedMirror(msg) => write!(f, "Unsupported mirror: {msg}"),
            Self::ExtractionFailed(msg) => write!(f, "Extraction failed: {msg}"),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {msg}"),
            Self::Persistence(msg) => write!(f, "Playback history error: {msg}"),
        }
    }
}

impl std::error::Error for AnimeError {}

impl From<reqwest::Error> for AnimeError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http {
            status: err.status().map(|s| s.as_u16()),
            message: err.to_string(),
        }
    }
}

/// Builds a shared HTTP client with cookie jar, browser User-Agent, and sane timeouts.
pub fn build_default_http_client() -> Result<reqwest::Client, AnimeError> {
    let mut builder = reqwest::Client::builder()
        .cookie_store(true)
        .user_agent(DEFAULT_USER_AGENT)
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10));
    if cfg!(test) {
        builder = builder.no_proxy();
    }
    builder.build().map_err(AnimeError::from)
}
