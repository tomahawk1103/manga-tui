use anime_core::provider::animeav1::AnimeAv1Provider;
use anime_core::provider::jkanime::JkAnimeProvider;
use anime_core::{AnimeProvider, ExtractorRegistry, build_default_http_client};

#[tokio::test]
#[ignore = "hits live AnimeAV1 servers; run manually with --ignored"]
async fn live_animeav1_search_details_and_extract() {
    let client = build_default_http_client().expect("http client");
    let provider = AnimeAv1Provider::new(client.clone());
    let registry = ExtractorRegistry::default_v1();

    let search = provider.search("frieren", 1).await.expect("AnimeAV1 search");
    assert!(!search.items.is_empty(), "expected AnimeAV1 search results for frieren");

    let first = &search.items[0];
    let details = provider.get_details(&first.slug).await.expect("AnimeAV1 details");
    assert!(!details.episodes.is_empty(), "expected episodes on AnimeAV1 details");

    let ep_num = &details.episodes[0].number;
    let mirrors = provider.get_episode_mirrors(&details.slug, ep_num).await.expect("AnimeAV1 mirrors");
    assert!(!mirrors.is_empty(), "expected mirrors for AnimeAV1 episode");

    let stream = registry.resolve_best_stream(&client, &mirrors).await.expect("AnimeAV1 stream extraction");
    assert!(stream.stream_url.starts_with("https://"), "expected https stream url");
    println!("AnimeAV1 resolved [{}] -> {}", stream.server_name, stream.stream_url);
}

#[tokio::test]
#[ignore = "hits live JKAnime servers; run manually with --ignored"]
async fn live_jkanime_search_details_episodes_and_extract() {
    let client = build_default_http_client().expect("http client");
    let provider = JkAnimeProvider::new(client.clone());
    let registry = ExtractorRegistry::default_v1();

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

    let stream = registry.resolve_best_stream(&client, &mirrors).await.expect("JKAnime stream extraction");
    assert!(stream.stream_url.starts_with("https://"), "expected https stream url");
    println!("JKAnime resolved [{}] -> {}", stream.server_name, stream.stream_url);
}
