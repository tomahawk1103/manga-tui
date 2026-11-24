use std::sync::Arc;

use anime_core::provider::animeav1::AnimeAv1Provider;
use anime_core::provider::jkanime::JkAnimeProvider;
use anime_core::{AnimeDetails, AnimeEpisode, AnimePlaybackServer, AnimeProvider, AnimeVariant, build_default_http_client};

#[tokio::test]
#[ignore = "hits live AnimeAV1 servers; run manually with --ignored"]
async fn live_animeav1_search_details_and_extract() {
    let client = build_default_http_client().expect("http client");
    let provider = Arc::new(AnimeAv1Provider::new(client));

    let search = provider.search("frieren", 1).await.expect("AnimeAV1 search");
    assert!(!search.items.is_empty(), "expected AnimeAV1 search results for frieren");

    let first = &search.items[0];
    let details = provider.get_details(&first.slug).await.expect("AnimeAV1 details");
    assert!(!details.episodes.is_empty(), "expected episodes on AnimeAV1 details");

    let ep_num = &details.episodes[0].number;
    let mirrors = provider.get_episode_mirrors(&details.slug, ep_num).await.expect("AnimeAV1 mirrors");
    assert!(!mirrors.is_empty(), "expected mirrors for AnimeAV1 episode");

    assert!(!mirrors.is_empty(), "AnimeAV1 has no mirrors");
    check_playback(provider, &details, details.episodes.clone()).await;
}

#[tokio::test]
#[ignore = "hits live JKAnime servers; run manually with --ignored"]
async fn live_jkanime_search_details_episodes_and_extract() {
    let client = build_default_http_client().expect("http client");
    let provider = Arc::new(JkAnimeProvider::new(client));

    let search = provider.search("one piece", 1).await.expect("JKAnime search");
    assert!(!search.items.is_empty(), "expected JKAnime search results for one piece");

    let slug = &search.items[0].slug;
    let details = provider.get_details(slug).await.expect("JKAnime details");
    assert!(!details.id.is_empty(), "expected anime id");

    let episodes = provider.get_episodes(&details, 1).await.expect("JKAnime episodes page 1");
    assert!(!episodes.episodes.is_empty(), "expected JKAnime episodes on page 1");

    let ep_num = &episodes.episodes[0].number;
    let mirrors = provider.get_episode_mirrors(&details.slug, ep_num).await.expect("JKAnime mirrors");
    assert!(!mirrors.is_empty(), "expected mirrors for JKAnime episode");

    assert!(!mirrors.is_empty(), "JKAnime has no mirrors");
    check_playback(provider, &details, episodes.episodes).await;
}

async fn check_playback(provider: Arc<impl AnimeProvider + 'static>, details: &AnimeDetails, mut episodes: Vec<AnimeEpisode>) {
    episodes.sort_by(|a, b| {
        a.number
            .parse::<f64>()
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b.number.parse::<f64>().unwrap_or(f64::INFINITY))
    });
    let index = episodes.iter().position(|episode| episode.number == "1").unwrap_or(0);
    let server = AnimePlaybackServer::start(provider, details, episodes, index, AnimeVariant::Sub, None)
        .await
        .unwrap_or_else(|error| {
            let status = match error {
                anime_core::AnimeError::Http { status, .. } => status,
                _ => None,
            };
            panic!("{}: no mirror passed the media probe (last HTTP status: {status:?})", details.provider)
        });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let stream = server.stream_url().await;
    let mut response = client.get(&stream).header(http::header::RANGE, "bytes=0-511").send().await.unwrap();
    assert!(response.status().is_success(), "{} proxy status: {}", details.provider, response.status());
    if stream.contains("/master.m3u8?") {
        let playlist = response.text().await.unwrap();
        assert!(playlist.starts_with("#EXTM3U"));
        assert!(playlist.contains("/proxy/"));
    } else {
        assert!(!response.chunk().await.unwrap().unwrap().is_empty());
    }
    println!("{} played through the local proxy: {}", details.provider, details.title);
}
