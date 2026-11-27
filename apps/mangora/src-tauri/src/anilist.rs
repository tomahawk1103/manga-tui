use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const ANILIST_GRAPHQL_URL: &str = "https://graphql.anilist.co";

const TAG_SEARCH_QUERY: &str = r#"
query ($page: Int, $perPage: Int, $search: String, $genres: [String], $tags: [String], $sort: [MediaSort]) {
  Page(page: $page, perPage: $perPage) {
    pageInfo {
      hasNextPage
    }
    media(type: MANGA, sort: $sort, search: $search, genre_in: $genres, tag_in: $tags, isAdult: false) {
      id
      title {
        romaji
        english
        native
      }
      synonyms
      description(asHtml: false)
      status
      genres
      coverImage {
        large
        extraLarge
      }
    }
  }
}
"#;

const MEDIA_BY_ID_QUERY: &str = r#"
query ($id: Int) {
  Media(id: $id, type: MANGA) {
    id
    title {
      romaji
      english
      native
    }
    synonyms
    description(asHtml: false)
    status
    genres
    coverImage {
      large
      extraLarge
    }
  }
}
"#;

const COMMUNITY_QUERY: &str = r#"
query ($search: String) {
  Media(search: $search, type: MANGA, sort: SEARCH_MATCH) {
    id
    siteUrl
    title {
      romaji
      english
      native
    }
    averageScore
    meanScore
    popularity
    favourites
    status
    genres
    tags {
      name
      rank
      isMediaSpoiler
      isGeneralSpoiler
    }
    reviews(sort: RATING_DESC, perPage: 5) {
      nodes {
        id
        summary
        rating
        ratingAmount
        score
        siteUrl
        user {
          name
        }
      }
    }
  }
}
"#;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListTag {
    pub name: String,
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListReview {
    pub id: u64,
    pub summary: String,
    pub rating: Option<i32>,
    pub rating_amount: Option<i32>,
    pub score: Option<i32>,
    pub user_name: String,
    pub site_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListCommunityInfo {
    pub found: bool,
    pub anilist_id: Option<u64>,
    pub site_url: Option<String>,
    pub title: Option<String>,
    pub average_score: Option<u32>,
    pub mean_score: Option<u32>,
    pub popularity: Option<u32>,
    pub favourites: Option<u32>,
    pub status: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<AniListTag>,
    pub reviews: Vec<AniListReview>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AniListMediaSummary {
    pub id: u64,
    pub title: String,
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub synonyms: Vec<String>,
    pub cover_url: String,
    pub description: Option<String>,
    pub status: String,
    pub genres: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AniListSearchPage {
    pub items: Vec<AniListMediaSummary>,
    pub has_next: bool,
}

static MEDIA_CACHE: OnceLock<Mutex<HashMap<u64, AniListMediaSummary>>> = OnceLock::new();
static RESOLVED_SERIES_CACHE: OnceLock<Mutex<HashMap<(u64, String), (String, String)>>> = OnceLock::new();

fn media_cache() -> &'static Mutex<HashMap<u64, AniListMediaSummary>> {
    MEDIA_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn resolved_series_cache() -> &'static Mutex<HashMap<(u64, String), (String, String)>> {
    RESOLVED_SERIES_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_cached_anilist_media(id: u64) -> Option<AniListMediaSummary> {
    media_cache().lock().ok().and_then(|guard| guard.get(&id).cloned())
}

fn store_cached_anilist_media(summary: &AniListMediaSummary) {
    if let Ok(mut guard) = media_cache().lock() {
        if guard.len() > 512 {
            guard.clear();
        }
        guard.insert(summary.id, summary.clone());
    }
}

pub fn get_cached_resolved_series(anilist_id: u64, active_provider: &str) -> Option<(String, String)> {
    let key = (anilist_id, active_provider.trim().to_ascii_lowercase());
    resolved_series_cache().lock().ok().and_then(|guard| guard.get(&key).cloned())
}

pub fn cache_resolved_series(anilist_id: u64, active_provider: &str, resolved_provider: &str, resolved_manga_id: &str) {
    if let Ok(mut guard) = resolved_series_cache().lock() {
        if guard.len() > 512 {
            guard.clear();
        }
        guard.insert(
            (anilist_id, active_provider.trim().to_ascii_lowercase()),
            (resolved_provider.trim().to_ascii_lowercase(), resolved_manga_id.to_string()),
        );
    }
}

pub fn split_anilist_genres_and_tags(raw_tags: &[String]) -> (Vec<String>, Vec<String>) {
    let mut genres: Vec<String> = Vec::new();
    let mut tags: Vec<String> = Vec::new();

    let push_unique = |target: &mut Vec<String>, value: String| {
        if !target.iter().any(|existing| existing.eq_ignore_ascii_case(&value)) {
            target.push(value);
        }
    };

    for raw in raw_tags {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        let mapped_genre = match lower.as_str() {
            "action" | "accion" | "acción" => Some("Action"),
            "adventure" | "aventura" | "aventuras" => Some("Adventure"),
            "comedy" | "comedia" => Some("Comedy"),
            "drama" => Some("Drama"),
            "ecchi" => Some("Ecchi"),
            "fantasy" | "fantasia" | "fantasía" => Some("Fantasy"),
            "horror" | "terror" => Some("Horror"),
            "mahou shoujo" | "mahou_shoujo" => Some("Mahou Shoujo"),
            "mecha" => Some("Mecha"),
            "music" => Some("Music"),
            "mystery" | "misterio" => Some("Mystery"),
            "psychological" | "psicologico" | "psicológico" => Some("Psychological"),
            "romance" | "romantica" | "romántica" => Some("Romance"),
            "scifi" | "sci-fi" | "ciencia ficcion" | "ciencia ficción" => Some("Sci-Fi"),
            "slice_of_life" | "slice of life" => Some("Slice of Life"),
            "sports" | "deportes" => Some("Sports"),
            "supernatural" | "sobrenatural" => Some("Supernatural"),
            "thriller" => Some("Thriller"),
            _ => None,
        };
        if let Some(genre) = mapped_genre {
            push_unique(&mut genres, genre.to_string());
            continue;
        }

        let mapped_tag = match lower.as_str() {
            "shonen" | "shounen" => "Shounen".to_string(),
            "seinen" => "Seinen".to_string(),
            "shojo" | "shoujo" => "Shoujo".to_string(),
            "josei" => "Josei".to_string(),
            "isekai" => "Isekai".to_string(),
            "martial_arts" | "martial arts" | "artes marciales" => "Martial Arts".to_string(),
            "historical" | "historico" | "histórico" => "Historical".to_string(),
            "harem" => "Female Harem".to_string(),
            _ => trimmed.to_string(),
        };
        push_unique(&mut tags, mapped_tag);
    }

    (genres, tags)
}

#[derive(Debug, Deserialize)]
struct GraphQlEnvelope {
    data: Option<GraphQlData>,
}

#[derive(Debug, Deserialize)]
struct GraphQlData {
    #[serde(rename = "Media")]
    media: Option<RawMedia>,
}

#[derive(Debug, Deserialize)]
struct TagSearchEnvelope {
    data: Option<TagSearchData>,
}

#[derive(Debug, Deserialize)]
struct TagSearchData {
    #[serde(rename = "Page")]
    page: Option<RawTagSearchPage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTagSearchPage {
    page_info: Option<RawPageInfo>,
    #[serde(default)]
    media: Vec<RawSearchMedia>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPageInfo {
    #[serde(default)]
    has_next_page: bool,
}

#[derive(Debug, Deserialize)]
struct MediaByIdEnvelope {
    data: Option<MediaByIdData>,
}

#[derive(Debug, Deserialize)]
struct MediaByIdData {
    #[serde(rename = "Media")]
    media: Option<RawSearchMedia>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSearchMedia {
    id: u64,
    title: Option<RawTitle>,
    #[serde(default)]
    synonyms: Vec<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    cover_image: Option<RawCoverImage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCoverImage {
    large: Option<String>,
    extra_large: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMedia {
    id: u64,
    site_url: Option<String>,
    title: Option<RawTitle>,
    average_score: Option<u32>,
    mean_score: Option<u32>,
    popularity: Option<u32>,
    favourites: Option<u32>,
    status: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    tags: Vec<RawTag>,
    reviews: Option<RawReviewConnection>,
}

#[derive(Debug, Deserialize)]
struct RawTitle {
    english: Option<String>,
    romaji: Option<String>,
    native: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTag {
    name: String,
    rank: Option<u32>,
    #[serde(default)]
    is_media_spoiler: bool,
    #[serde(default)]
    is_general_spoiler: bool,
}

#[derive(Debug, Deserialize)]
struct RawReviewConnection {
    #[serde(default)]
    nodes: Vec<RawReview>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReview {
    id: u64,
    summary: Option<String>,
    rating: Option<i32>,
    rating_amount: Option<i32>,
    score: Option<i32>,
    site_url: Option<String>,
    user: Option<RawReviewUser>,
}

#[derive(Debug, Deserialize)]
struct RawReviewUser {
    name: Option<String>,
}

fn fallback_info(note: impl Into<String>) -> AniListCommunityInfo {
    AniListCommunityInfo {
        found: false,
        note: Some(note.into()),
        ..Default::default()
    }
}

fn clean_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fetches public, read-only community scores, tags, and reviews from AniList without requiring login.
pub async fn fetch_anilist_community(title: &str) -> AniListCommunityInfo {
    fetch_anilist_community_from(ANILIST_GRAPHQL_URL, title).await
}

pub async fn fetch_anilist_community_from(endpoint: &str, title: &str) -> AniListCommunityInfo {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return fallback_info("missing_title");
    }

    let mut builder = Client::builder().timeout(Duration::from_secs(10)).user_agent("Mangora/0.1.0");
    if cfg!(test) {
        builder = builder.no_proxy();
    }
    let Ok(client) = builder.build() else {
        return fallback_info("unavailable");
    };

    let response = match client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&json!({
            "query": COMMUNITY_QUERY,
            "variables": { "search": trimmed }
        }))
        .send()
        .await
    {
        Ok(res) => res,
        Err(_) => return fallback_info("unavailable"),
    };

    if !response.status().is_success() {
        return fallback_info("unavailable");
    }

    let Ok(parsed) = response.json::<GraphQlEnvelope>().await else {
        return fallback_info("unavailable");
    };

    let Some(media) = parsed.data.and_then(|d| d.media) else {
        return fallback_info("not_found");
    };

    let resolved_title = media.title.and_then(|t| {
        [t.english, t.romaji, t.native]
            .into_iter()
            .flatten()
            .map(|s| s.trim().to_string())
            .find(|s| !s.is_empty())
    });

    let tags = media
        .tags
        .into_iter()
        .filter(|tag| !tag.is_media_spoiler && !tag.is_general_spoiler && !tag.name.trim().is_empty())
        .take(8)
        .map(|tag| AniListTag {
            name: tag.name.trim().to_string(),
            rank: tag.rank,
        })
        .collect();

    let reviews = media
        .reviews
        .map(|conn| conn.nodes)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|review| {
            let summary = review.summary.map(|s| clean_text(&s)).unwrap_or_default();
            if summary.is_empty() {
                return None;
            }
            let user_name = review
                .user
                .and_then(|u| u.name)
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "AniList Reader".to_string());
            Some(AniListReview {
                id: review.id,
                summary,
                rating: review.rating,
                rating_amount: review.rating_amount,
                score: review.score,
                user_name,
                site_url: review.site_url.filter(|u| u.starts_with("https://")),
            })
        })
        .take(5)
        .collect();

    AniListCommunityInfo {
        found: true,
        anilist_id: Some(media.id),
        site_url: media.site_url.filter(|u| u.starts_with("https://")),
        title: resolved_title,
        average_score: media.average_score,
        mean_score: media.mean_score,
        popularity: media.popularity,
        favourites: media.favourites,
        status: media.status,
        genres: media.genres.into_iter().map(|g| g.trim().to_string()).filter(|g| !g.is_empty()).collect(),
        tags,
        reviews,
        note: None,
    }
}

fn build_anilist_client() -> Result<Client, String> {
    let mut builder = Client::builder().timeout(Duration::from_secs(12)).user_agent("Mangora/0.1.0");
    if cfg!(test) {
        builder = builder.no_proxy();
    }
    builder.build().map_err(|err| format!("Failed to initialize HTTP client: {err}"))
}

fn strip_anilist_html(raw: Option<&str>) -> Option<String> {
    let input = raw?;
    let replaced_br = input
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<BR>", "\n");
    let mut out = String::with_capacity(replaced_br.len());
    let mut in_tag = false;
    for ch in replaced_br.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {},
        }
    }
    let decoded = out
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    let cleaned = decoded.trim().to_string();
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

fn map_anilist_status(raw: Option<&str>) -> String {
    match raw.unwrap_or("").trim().to_ascii_uppercase().as_str() {
        "FINISHED" => "completed".to_string(),
        "RELEASING" => "ongoing".to_string(),
        "HIATUS" => "hiatus".to_string(),
        "CANCELLED" => "cancelled".to_string(),
        _ => "ongoing".to_string(),
    }
}

fn map_raw_search_media(media: RawSearchMedia) -> AniListMediaSummary {
    let romaji = media
        .title
        .as_ref()
        .and_then(|t| t.romaji.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let english = media
        .title
        .as_ref()
        .and_then(|t| t.english.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let native = media
        .title
        .as_ref()
        .and_then(|t| t.native.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let title = english
        .clone()
        .or_else(|| romaji.clone())
        .or(native)
        .unwrap_or_else(|| format!("AniList #{}", media.id));

    let cover_url = media
        .cover_image
        .and_then(|c| c.extra_large.or(c.large))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let summary = AniListMediaSummary {
        id: media.id,
        title,
        romaji,
        english,
        synonyms: media
            .synonyms
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        cover_url,
        description: strip_anilist_html(media.description.as_deref()),
        status: map_anilist_status(media.status.as_deref()),
        genres: media.genres.into_iter().map(|g| g.trim().to_string()).filter(|g| !g.is_empty()).collect(),
    };
    store_cached_anilist_media(&summary);
    summary
}

pub async fn search_anilist_by_tags(query: Option<&str>, tags: &[String], page: u32) -> Result<AniListSearchPage, String> {
    search_anilist_by_tags_from(ANILIST_GRAPHQL_URL, query, tags, page).await
}

pub async fn search_anilist_by_tags_from(
    endpoint: &str,
    query: Option<&str>,
    tags: &[String],
    page: u32,
) -> Result<AniListSearchPage, String> {
    let (genres, tag_names) = split_anilist_genres_and_tags(tags);
    let trimmed_query = query.map(str::trim).filter(|s| !s.is_empty());
    if trimmed_query.is_none() && genres.is_empty() && tag_names.is_empty() {
        return Ok(AniListSearchPage::default());
    }

    let mut variables = serde_json::Map::new();
    variables.insert("page".to_string(), json!(page.max(1)));
    variables.insert("perPage".to_string(), json!(20));
    variables
        .insert("sort".to_string(), if trimmed_query.is_some() { json!(["SEARCH_MATCH"]) } else { json!(["POPULARITY_DESC"]) });
    if let Some(q) = trimmed_query {
        variables.insert("search".to_string(), json!(q));
    }
    if !genres.is_empty() {
        variables.insert("genres".to_string(), json!(genres));
    }
    if !tag_names.is_empty() {
        variables.insert("tags".to_string(), json!(tag_names));
    }

    let client = build_anilist_client()?;
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&json!({
            "query": TAG_SEARCH_QUERY,
            "variables": serde_json::Value::Object(variables),
        }))
        .send()
        .await
        .map_err(|err| format!("AniList request failed: {err}"))?;

    if !response.status().is_success() {
        return Err(format!("AniList returned HTTP {}", response.status()));
    }

    let parsed: TagSearchEnvelope = response.json().await.map_err(|err| format!("Failed to parse AniList response: {err}"))?;

    let page_data = parsed.data.and_then(|d| d.page);
    let has_next = page_data
        .as_ref()
        .and_then(|p| p.page_info.as_ref())
        .is_some_and(|info| info.has_next_page);
    let items: Vec<AniListMediaSummary> = page_data
        .map(|p| p.media.into_iter().map(map_raw_search_media).collect())
        .unwrap_or_default();

    Ok(AniListSearchPage {
        has_next: has_next && !items.is_empty(),
        items,
    })
}

pub async fn fetch_anilist_media_by_id(id: u64) -> Option<AniListMediaSummary> {
    fetch_anilist_media_by_id_from(ANILIST_GRAPHQL_URL, id).await
}

pub async fn fetch_anilist_media_by_id_from(endpoint: &str, id: u64) -> Option<AniListMediaSummary> {
    if let Some(cached) = get_cached_anilist_media(id) {
        return Some(cached);
    }
    let client = build_anilist_client().ok()?;
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&json!({
            "query": MEDIA_BY_ID_QUERY,
            "variables": { "id": id },
        }))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let parsed: MediaByIdEnvelope = response.json().await.ok()?;
    parsed.data.and_then(|d| d.media).map(map_raw_search_media)
}

#[cfg(test)]
mod tests {
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn splits_anilist_genres_and_preserves_custom_tag_casing() {
        let (genres, tags) = split_anilist_genres_and_tags(&[
            "Action".to_string(),
            "ciencia ficción".to_string(),
            "Philosophy".to_string(),
            "Coming of Age".to_string(),
            "Post-Apocalyptic".to_string(),
            "shonen".to_string(),
            "philosophy".to_string(),
        ]);

        assert_eq!(genres, vec!["Action".to_string(), "Sci-Fi".to_string()]);
        assert_eq!(tags, vec![
            "Philosophy".to_string(),
            "Coming of Age".to_string(),
            "Post-Apocalyptic".to_string(),
            "Shounen".to_string(),
        ]);
    }

    #[tokio::test]
    async fn searches_anilist_by_custom_tags_and_caches_media_by_id() {
        let server = MockServer::start_async().await;
        let search_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/search");
                then.status(200).json_body(json!({
                    "data": {
                        "Page": {
                            "pageInfo": { "hasNextPage": true },
                            "media": [{
                                "id": 999001,
                                "title": { "romaji": "Shin Seiki Evangelion", "english": "Neon Genesis Evangelion", "native": "新世紀エヴァンゲリオン" },
                                "synonyms": ["Eva"],
                                "description": "Angels &amp; <i>Eva</i><br>Mecha classic.",
                                "status": "FINISHED",
                                "genres": ["Action", "Drama", "Mecha"],
                                "coverImage": {
                                    "large": "https://cdn.anilist.co/eva-l.jpg",
                                    "extraLarge": "https://cdn.anilist.co/eva-xl.jpg"
                                }
                            }]
                        }
                    }
                }));
            })
            .await;

        let page = search_anilist_by_tags_from(
            &format!("{}/search", server.base_url()),
            None,
            &["Philosophy".to_string(), "Mecha".to_string()],
            1,
        )
        .await
        .unwrap();
        search_mock.assert_async().await;

        assert!(page.has_next);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, 999001);
        assert_eq!(page.items[0].title, "Neon Genesis Evangelion");
        assert_eq!(page.items[0].description.as_deref(), Some("Angels & Eva\nMecha classic."));
        assert_eq!(page.items[0].cover_url, "https://cdn.anilist.co/eva-xl.jpg");

        // Cached by ID without hitting network again
        let cached = fetch_anilist_media_by_id_from(&format!("{}/unused", server.base_url()), 999001)
            .await
            .unwrap();
        assert_eq!(cached.title, "Neon Genesis Evangelion");

        let by_id_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/by-id");
                then.status(200).json_body(json!({
                    "data": {
                        "Media": {
                            "id": 999002,
                            "title": { "romaji": "Kaiju No. 8", "english": "Kaiju No. 8", "native": "怪獣8号" },
                            "synonyms": [],
                            "description": null,
                            "status": "RELEASING",
                            "genres": ["Action", "Sci-Fi"],
                            "coverImage": { "large": "https://cdn.anilist.co/kaiju.jpg", "extraLarge": null }
                        }
                    }
                }));
            })
            .await;

        let fetched = fetch_anilist_media_by_id_from(&format!("{}/by-id", server.base_url()), 999002)
            .await
            .unwrap();
        by_id_mock.assert_async().await;
        assert_eq!(fetched.status, "ongoing");
        assert_eq!(fetched.cover_url, "https://cdn.anilist.co/kaiju.jpg");
    }

    #[tokio::test]
    async fn fetches_and_parses_public_anilist_scores_tags_and_reviews() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/");
                then.status(200).json_body(json!({
                    "data": {
                        "Media": {
                            "id": 30001,
                            "siteUrl": "https://anilist.co/manga/30001",
                            "title": { "romaji": "MONSTER", "english": "Monster", "native": "MONSTER" },
                            "averageScore": 91,
                            "meanScore": 90,
                            "popularity": 124500,
                            "favourites": 11200,
                            "status": "FINISHED",
                            "genres": ["Drama", "Mystery", "Psychological"],
                            "tags": [
                                { "name": "Crime", "rank": 95, "isMediaSpoiler": false, "isGeneralSpoiler": false },
                                { "name": "Secret Twist", "rank": 90, "isMediaSpoiler": true, "isGeneralSpoiler": false }
                            ],
                            "reviews": {
                                "nodes": [
                                    {
                                        "id": 77,
                                        "summary": "  A masterful   thriller about ethics and memory.  ",
                                        "rating": 84,
                                        "ratingAmount": 90,
                                        "score": 95,
                                        "siteUrl": "https://anilist.co/review/77",
                                        "user": { "name": "JohanReader" }
                                    },
                                    {
                                        "id": 78,
                                        "summary": "   ",
                                        "rating": 1,
                                        "ratingAmount": 2,
                                        "score": 50,
                                        "siteUrl": "https://anilist.co/review/78",
                                        "user": { "name": "Empty" }
                                    }
                                ]
                            }
                        }
                    }
                }));
            })
            .await;

        let info = fetch_anilist_community_from(&server.base_url(), "Monster").await;
        mock.assert_async().await;

        assert!(info.found);
        assert_eq!(info.anilist_id, Some(30001));
        assert_eq!(info.title.as_deref(), Some("Monster"));
        assert_eq!(info.average_score, Some(91));
        assert_eq!(info.tags, vec![AniListTag {
            name: "Crime".into(),
            rank: Some(95)
        }]);
        assert_eq!(info.reviews.len(), 1);
        assert_eq!(info.reviews[0].summary, "A masterful thriller about ethics and memory.");
        assert_eq!(info.reviews[0].user_name, "JohanReader");
    }

    #[tokio::test]
    async fn returns_clean_fallback_when_anilist_has_no_match_or_errors() {
        let server = MockServer::start_async().await;
        let not_found_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/not-found");
                then.status(200).json_body(json!({ "data": { "Media": null } }));
            })
            .await;

        let missing = fetch_anilist_community_from(&format!("{}/not-found", server.base_url()), "UnknownManga").await;
        not_found_mock.assert_async().await;
        assert!(!missing.found);
        assert_eq!(missing.note.as_deref(), Some("not_found"));

        let err_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/down");
                then.status(503).body("Service Unavailable");
            })
            .await;

        let down = fetch_anilist_community_from(&format!("{}/down", server.base_url()), "Monster").await;
        err_mock.assert_async().await;
        assert!(!down.found);
        assert_eq!(down.note.as_deref(), Some("unavailable"));
    }
}
