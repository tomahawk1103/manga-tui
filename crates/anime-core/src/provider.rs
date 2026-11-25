use std::future::Future;

use crate::AnimeError;
use crate::model::{AnimeDetails, AnimeEpisodesPage, AnimeHomeFeed, AnimeProviders, AnimeSearchResult, VideoMirror};

pub mod animeav1;
pub mod jkanime;

pub trait AnimeProvider: Send + Sync {
    fn kind(&self) -> AnimeProviders;

    fn get_home_feed(&self) -> impl Future<Output = Result<AnimeHomeFeed, AnimeError>> + Send {
        async { Ok(AnimeHomeFeed::default()) }
    }

    fn search(&self, query: &str, page: u32) -> impl Future<Output = Result<AnimeSearchResult, AnimeError>> + Send;

    fn get_details(&self, slug: &str) -> impl Future<Output = Result<AnimeDetails, AnimeError>> + Send;

    fn get_episodes(&self, details: &AnimeDetails, page: u32)
    -> impl Future<Output = Result<AnimeEpisodesPage, AnimeError>> + Send;

    fn get_episode_mirrors(
        &self,
        slug: &str,
        episode_number: &str,
    ) -> impl Future<Output = Result<Vec<VideoMirror>, AnimeError>> + Send;
}
