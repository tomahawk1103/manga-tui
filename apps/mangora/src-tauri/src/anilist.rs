use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const ANILIST_GRAPHQL_URL: &str = "https://graphql.anilist.co";

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

#[cfg(test)]
mod tests {
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use pretty_assertions::assert_eq;

    use super::*;

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
