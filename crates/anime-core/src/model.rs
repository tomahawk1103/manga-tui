use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumIter, EnumString)]
pub enum AnimeProviders {
    #[default]
    #[strum(to_string = "AnimeAV1", serialize = "animeav1")]
    AnimeAv1,
    #[strum(to_string = "JKAnime", serialize = "jkanime")]
    JkAnime,
}

impl AnimeProviders {
    pub fn as_id(self) -> &'static str {
        match self {
            Self::AnimeAv1 => "animeav1",
            Self::JkAnime => "jkanime",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Self::AnimeAv1 => "https://animeav1.com",
            Self::JkAnime => "https://jkanime.net",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumIter, EnumString)]
pub enum AnimeVariant {
    #[default]
    #[strum(to_string = "SUB", serialize = "sub")]
    Sub,
    #[strum(to_string = "DUB", serialize = "dub")]
    Dub,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum AnimeStatus {
    Ongoing,
    Completed,
    Upcoming,
    #[default]
    Unknown,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum AnimeType {
    Tv,
    Movie,
    Ova,
    Special,
    Ona,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeSummary {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub synopsis: Option<String>,
    pub cover_url: Option<String>,
    pub anime_type: AnimeType,
    pub status: AnimeStatus,
    pub provider: AnimeProviders,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeSearchResult {
    pub items: Vec<AnimeSummary>,
    pub current_page: u32,
    pub total_pages: u32,
    pub total_records: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentAnimeEpisode {
    pub anime_id: String,
    pub slug: String,
    pub anime_title: String,
    pub episode_number: String,
    pub aired_label: Option<String>,
    pub cover_url: Option<String>,
    pub provider: AnimeProviders,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeHomeFeed {
    pub recent_episodes: Vec<RecentAnimeEpisode>,
    pub recommendations: Vec<AnimeSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeEpisode {
    pub id: Option<String>,
    pub number: String,
    pub title: Option<String>,
    pub thumbnail_url: Option<String>,
    pub aired_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimeEpisodesPage {
    pub episodes: Vec<AnimeEpisode>,
    pub current_page: u32,
    pub total_pages: u32,
    pub total_episodes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimeDetails {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub synopsis: String,
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub status: AnimeStatus,
    pub anime_type: AnimeType,
    pub score: Option<f32>,
    pub mal_id: Option<u64>,
    pub episodes_count: Option<u32>,
    pub provider: AnimeProviders,
    pub episodes: Vec<AnimeEpisode>,
    #[serde(skip)]
    pub csrf_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoMirror {
    pub server_name: String,
    pub embed_url: String,
    pub variant: AnimeVariant,
    pub referer: String,
    pub size: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamKind {
    Hls,
    Mp4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedStream {
    pub stream_url: String,
    pub headers: Vec<(String, String)>,
    pub kind: StreamKind,
    pub server_name: String,
    pub quality: Option<String>,
}
