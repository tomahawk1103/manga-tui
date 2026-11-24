use serde_json::Value;

use crate::model::{
    AnimeDetails, AnimeEpisode, AnimeEpisodesPage, AnimeProviders, AnimeSearchResult, AnimeStatus, AnimeSummary, AnimeType,
    AnimeVariant, VideoMirror,
};
use crate::provider::AnimeProvider;
use crate::{AnimeError, DEFAULT_USER_AGENT};

const MAX_DEVALUE_DEPTH: usize = 32;

#[derive(Debug, Clone)]
pub struct AnimeAv1Provider {
    client: reqwest::Client,
    base_url: String,
    cdn_url: String,
}

impl AnimeAv1Provider {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: AnimeProviders::AnimeAv1.base_url().to_string(),
            cdn_url: "https://cdn.animeav1.com".to_string(),
        }
    }

    pub fn with_base_url(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            client,
            cdn_url: format!("{base_url}/cdn"),
            base_url,
        }
    }

    fn cover_url_for_id(&self, id: &str) -> Option<String> {
        let trimmed = id.trim();
        if trimmed.is_empty() { None } else { Some(format!("{}/covers/{trimmed}.jpg", self.cdn_url)) }
    }

    async fn fetch_hydrated_json(&self, url: &str) -> Result<Value, AnimeError> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
            .header(reqwest::header::REFERER, format!("{}/", self.base_url))
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 404 {
            return Err(AnimeError::NotFound(format!("AnimeAV1 resource not found: {url}")));
        }
        if status.as_u16() == 429 {
            return Err(AnimeError::RateLimited(format!("AnimeAV1 rate limited request: {url}")));
        }
        if !status.is_success() {
            return Err(AnimeError::Http {
                status: Some(status.as_u16()),
                message: format!("AnimeAV1 returned HTTP {status} for {url}"),
            });
        }

        let root: Value = response
            .json()
            .await
            .map_err(|e| AnimeError::Parse(format!("Invalid JSON from AnimeAV1 ({url}): {e}")))?;
        hydrate_sveltekit_data(&root)
    }
}

/// Decodes SvelteKit's flat `__data.json` devalue array into a standard nested JSON `Value`.
pub(crate) fn hydrate_sveltekit_data(root: &Value) -> Result<Value, AnimeError> {
    let nodes = root
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| AnimeError::Parse("Missing 'nodes' array in SvelteKit __data.json".to_string()))?;

    let data_array = nodes
        .iter()
        .rev()
        .filter_map(|node| {
            let is_data = node.get("type").and_then(Value::as_str) == Some("data");
            if !is_data {
                return None;
            }
            node.get("data").and_then(Value::as_array).filter(|arr| !arr.is_empty())
        })
        .next()
        .ok_or_else(|| AnimeError::Parse("No populated 'data' node found in SvelteKit __data.json".to_string()))?;

    resolve_index(data_array, 0, 0)
}

fn resolve_index(data: &[Value], idx: i64, depth: usize) -> Result<Value, AnimeError> {
    if depth > MAX_DEVALUE_DEPTH {
        return Err(AnimeError::Parse("SvelteKit devalue nesting too deep".to_string()));
    }
    if idx < 0 {
        return Ok(match idx {
            -6 => Value::from(0),
            _ => Value::Null,
        });
    }

    let usize_idx = idx as usize;
    let raw = data
        .get(usize_idx)
        .ok_or_else(|| AnimeError::Parse(format!("SvelteKit devalue index {idx} out of bounds (len {})", data.len())))?;

    match raw {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, val) in map {
                let resolved =
                    if let Some(child_idx) = val.as_i64() { resolve_index(data, child_idx, depth + 1)? } else { val.clone() };
                out.insert(key.clone(), resolved);
            }
            Ok(Value::Object(out))
        },
        Value::Array(arr) => {
            if let Some(first) = arr.first()
                && first.is_string()
            {
                if let Some(inner_idx) = arr.get(1).and_then(Value::as_i64) {
                    return resolve_index(data, inner_idx, depth + 1);
                }
                return Ok(arr.get(1).cloned().unwrap_or(Value::Null));
            }

            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                if let Some(child_idx) = item.as_i64() {
                    out.push(resolve_index(data, child_idx, depth + 1)?);
                } else {
                    out.push(item.clone());
                }
            }
            Ok(Value::Array(out))
        },
        primitive => Ok(primitive.clone()),
    }
}

fn value_to_id_string(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        },
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_to_episode_number(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        },
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_to_u32(val: &Value) -> Option<u32> {
    match val {
        Value::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
        Value::String(s) => s.trim().parse::<u32>().ok(),
        _ => None,
    }
}

fn parse_status(val: Option<&Value>) -> AnimeStatus {
    match val.and_then(Value::as_i64) {
        Some(1) => AnimeStatus::Ongoing,
        Some(0) => AnimeStatus::Completed,
        Some(2) => AnimeStatus::Upcoming,
        _ => match val.and_then(Value::as_str).map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("emision" | "emisión" | "ongoing" | "en emision") => AnimeStatus::Ongoing,
            Some("finalizado" | "concluido" | "completed") => AnimeStatus::Completed,
            Some("proximamente" | "próximamente" | "upcoming") => AnimeStatus::Upcoming,
            _ => AnimeStatus::Unknown,
        },
    }
}

fn parse_type(name: Option<&str>) -> AnimeType {
    let Some(raw) = name else {
        return AnimeType::Unknown;
    };
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains("tv") || lower == "serie" {
        AnimeType::Tv
    } else if lower.contains("pelicula") || lower.contains("película") || lower.contains("movie") {
        AnimeType::Movie
    } else if lower == "ova" {
        AnimeType::Ova
    } else if lower.contains("especial") || lower.contains("special") {
        AnimeType::Special
    } else if lower == "ona" {
        AnimeType::Ona
    } else {
        AnimeType::Unknown
    }
}

fn mirror_priority(server_name: &str) -> u8 {
    let lower = server_name.trim().to_ascii_lowercase();
    if lower == "hls" {
        0
    } else if lower == "mp4upload" {
        1
    } else {
        2
    }
}

impl AnimeProvider for AnimeAv1Provider {
    fn kind(&self) -> AnimeProviders {
        AnimeProviders::AnimeAv1
    }

    async fn search(&self, query: &str, page: u32) -> Result<AnimeSearchResult, AnimeError> {
        let mut url = reqwest::Url::parse(&format!("{}/catalogo/__data.json", self.base_url))
            .map_err(|e| AnimeError::InvalidInput(format!("Invalid base URL: {e}")))?;

        let page = page.max(1);
        {
            let mut pairs = url.query_pairs_mut();
            let trimmed = query.trim();
            if !trimmed.is_empty() {
                pairs.append_pair("search", trimmed);
            }
            pairs.append_pair("page", &page.to_string());
        }

        let data = self.fetch_hydrated_json(url.as_str()).await?;
        let results = data
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| AnimeError::Parse("Missing 'results' array in AnimeAV1 search response".to_string()))?;

        let mut items = Vec::with_capacity(results.len());
        for entry in results {
            let Some(slug) = entry.get("slug").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else {
                continue;
            };
            let id = entry.get("id").and_then(value_to_id_string).unwrap_or_else(|| slug.to_string());
            let title = entry.get("title").and_then(Value::as_str).map(str::trim).unwrap_or(slug).to_string();
            let synopsis = entry
                .get("synopsis")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            let category_name = entry.get("category").and_then(|c| c.get("name")).and_then(Value::as_str);

            items.push(AnimeSummary {
                cover_url: self.cover_url_for_id(&id),
                id,
                slug: slug.to_string(),
                title,
                synopsis,
                anime_type: parse_type(category_name),
                status: parse_status(entry.get("status")),
                provider: AnimeProviders::AnimeAv1,
            });
        }

        let pagination = data.get("pagination");
        let current_page = pagination.and_then(|p| p.get("currentPage")).and_then(value_to_u32).unwrap_or(page);
        let total_pages = pagination.and_then(|p| p.get("totalPages")).and_then(value_to_u32).unwrap_or(1).max(1);
        let total_records = pagination.and_then(|p| p.get("totalRecords")).and_then(value_to_u32);

        Ok(AnimeSearchResult {
            items,
            current_page,
            total_pages,
            total_records,
        })
    }

    async fn get_details(&self, slug: &str) -> Result<AnimeDetails, AnimeError> {
        let slug = slug.trim().trim_matches('/');
        if slug.is_empty() {
            return Err(AnimeError::InvalidInput("Anime slug cannot be empty".to_string()));
        }

        let url = format!("{}/media/{slug}/__data.json", self.base_url);
        let data = self.fetch_hydrated_json(&url).await?;
        let media = data
            .get("media")
            .ok_or_else(|| AnimeError::Parse("Missing 'media' object in AnimeAV1 details response".to_string()))?;

        let id = media.get("id").and_then(value_to_id_string).unwrap_or_else(|| slug.to_string());
        let resolved_slug = media
            .get("slug")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(slug)
            .to_string();
        let title = media
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or(&resolved_slug)
            .to_string();
        let synopsis = media
            .get("synopsis")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        let genres = media
            .get("genres")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|g| g.get("name").and_then(Value::as_str).map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let category_name = media.get("category").and_then(|c| c.get("name")).and_then(Value::as_str);
        let score = media.get("score").and_then(Value::as_f64).map(|s| s as f32);
        let mal_id = media.get("malId").and_then(Value::as_u64);
        let episodes_count = media.get("episodesCount").and_then(value_to_u32);

        let episodes = media
            .get("episodes")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|ep| {
                        let number = ep.get("number").and_then(value_to_episode_number)?;
                        let ep_id = ep.get("id").and_then(value_to_id_string);
                        Some(AnimeEpisode {
                            id: ep_id,
                            number,
                            title: None,
                            thumbnail_url: None,
                            aired_at: None,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(AnimeDetails {
            cover_url: self.cover_url_for_id(&id),
            id,
            slug: resolved_slug,
            title,
            synopsis,
            genres,
            status: parse_status(media.get("status")),
            anime_type: parse_type(category_name),
            score,
            mal_id,
            episodes_count: episodes_count.or(Some(episodes.len() as u32)),
            provider: AnimeProviders::AnimeAv1,
            episodes,
            csrf_token: None,
        })
    }

    async fn get_episodes(&self, details: &AnimeDetails, _page: u32) -> Result<AnimeEpisodesPage, AnimeError> {
        let episodes =
            if !details.episodes.is_empty() { details.episodes.clone() } else { self.get_details(&details.slug).await?.episodes };

        let total = u32::try_from(episodes.len()).ok();
        Ok(AnimeEpisodesPage {
            episodes,
            current_page: 1,
            total_pages: 1,
            total_episodes: total,
        })
    }

    async fn get_episode_mirrors(&self, slug: &str, episode_number: &str) -> Result<Vec<VideoMirror>, AnimeError> {
        let slug = slug.trim().trim_matches('/');
        let episode_number = episode_number.trim().trim_matches('/');
        if slug.is_empty() || episode_number.is_empty() {
            return Err(AnimeError::InvalidInput("Slug and episode number cannot be empty".to_string()));
        }

        let page_url = format!("{}/media/{slug}/{episode_number}", self.base_url);
        let data_url = format!("{page_url}/__data.json");
        let data = self.fetch_hydrated_json(&data_url).await?;

        let embeds = data
            .get("embeds")
            .and_then(Value::as_object)
            .ok_or_else(|| AnimeError::Parse("Missing 'embeds' object in AnimeAV1 episode response".to_string()))?;

        let mut mirrors = Vec::new();
        for (variant_key, variant) in [("SUB", AnimeVariant::Sub), ("DUB", AnimeVariant::Dub)] {
            let Some(list) = embeds.get(variant_key).and_then(Value::as_array) else {
                continue;
            };
            for item in list {
                let Some(server_name) = item.get("server").and_then(Value::as_str).map(str::trim) else {
                    continue;
                };
                let Some(embed_url) = item.get("url").and_then(Value::as_str).map(str::trim) else {
                    continue;
                };
                if server_name.is_empty() || embed_url.is_empty() {
                    continue;
                }
                mirrors.push(VideoMirror {
                    server_name: server_name.to_string(),
                    embed_url: embed_url.to_string(),
                    variant,
                    referer: format!("{page_url}/"),
                    size: None,
                });
            }
        }

        mirrors.sort_by_key(|m| mirror_priority(&m.server_name));
        Ok(mirrors)
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::build_default_http_client;

    #[test]
    fn hydrate_handles_sentinels_shared_refs_and_picks_last_data_node() {
        let root = json!({
            "type": "data",
            "nodes": [
                null,
                {"type": "data", "data": [{"user": 1}, null]},
                {"type": "data", "data": [
                    {"results": 1, "zero": -6, "empty": -1, "count": 4},
                    [2, 5],
                    {"id": 3, "title": 4},
                    "159",
                    "Sousou no Frieren",
                    {"id": 6, "title": 7},
                    3560,
                    "Frieren 2nd"
                ]}
            ]
        });

        let hydrated = hydrate_sveltekit_data(&root).unwrap();
        assert_eq!(hydrated["zero"], 0);
        assert_eq!(hydrated["empty"], Value::Null);
        assert_eq!(hydrated["count"], "Sousou no Frieren");
        assert_eq!(hydrated["results"][0]["id"], "159");
        assert_eq!(hydrated["results"][1]["id"], 3560);
    }

    #[test]
    fn hydrate_rejects_excessive_recursion() {
        let mut arr = Vec::new();
        for i in 0..=40 {
            arr.push(json!({"next": i + 1}));
        }
        arr.push(json!("leaf"));
        let root = json!({
            "nodes": [{"type": "data", "data": arr}]
        });

        let err = hydrate_sveltekit_data(&root).unwrap_err();
        assert!(matches!(err, AnimeError::Parse(_)));
    }

    #[tokio::test]
    async fn search_details_episodes_and_mirrors_work_with_devalue_payloads() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET)
                .path("/catalogo/__data.json")
                .query_param("search", "frieren")
                .query_param("page", "1");
            then.status(200).json_body(json!({
                "type": "data",
                "nodes": [
                    null,
                    {"type": "data", "data": [{"user": 1}, null]},
                    {"type": "data", "data": [
                        {"results": 1, "pagination": 9},
                        [2],
                        {"id": 3, "title": 4, "synopsis": 5, "slug": 6, "category": 7},
                        "159",
                        "Sousou no Frieren",
                        "La historia de Frieren.",
                        "sousou-no-frieren",
                        {"name": 8},
                        "TV Anime",
                        {"currentPage": 10, "totalPages": 11, "totalRecords": 10},
                        1,
                        2
                    ]}
                ]
            }));
        });

        server.mock(|when, then| {
            when.method(GET).path("/media/sousou-no-frieren/__data.json");
            then.status(200).json_body(json!({
                "type": "data",
                "nodes": [
                    null,
                    {"type": "data", "data": [
                        {"media": 1},
                        {
                            "id": 2,
                            "slug": 3,
                            "title": 4,
                            "synopsis": 5,
                            "genres": 6,
                            "status": 9,
                            "category": 10,
                            "score": 12,
                            "malId": 13,
                            "episodesCount": 14,
                            "episodes": 15
                        },
                        159,
                        "sousou-no-frieren",
                        "Sousou no Frieren",
                        "Sinopsis completa.",
                        [7],
                        {"name": 8},
                        "Fantasía",
                        0,
                        {"name": 11},
                        "TV Anime",
                        9.25,
                        52991,
                        2,
                        [16, 19],
                        {"id": 17, "number": 18},
                        2047,
                        1,
                        {"id": 20, "number": 21},
                        2048,
                        "12.5"
                    ]}
                ]
            }));
        });

        server.mock(|when, then| {
            when.method(GET).path("/media/sousou-no-frieren/1/__data.json");
            then.status(200).json_body(json!({
                "type": "data",
                "nodes": [
                    null,
                    {"type": "data", "data": [
                        {"embeds": 1},
                        {"SUB": 2, "DUB": 7},
                        [3, 5],
                        {"server": 4, "url": 9},
                        "MP4Upload",
                        {"server": 6, "url": 10},
                        "HLS",
                        [8],
                        {"server": 11, "url": 12},
                        "https://www.mp4upload.com/embed-abc.html",
                        "https://player.zilla-networks.com/play/0123456789abcdef0123456789abcdef",
                        "Voe",
                        "https://voe.sx/e/xyz"
                    ]}
                ]
            }));
        });

        let client = build_default_http_client().unwrap();
        let provider = AnimeAv1Provider::with_base_url(client, server.base_url());

        let search = provider.search("frieren", 1).await.unwrap();
        assert_eq!(search.items.len(), 1);
        assert_eq!(search.items[0].id, "159");
        assert_eq!(search.items[0].anime_type, AnimeType::Tv);
        assert_eq!(search.items[0].cover_url.as_deref(), Some(format!("{}/cdn/covers/159.jpg", server.base_url()).as_str()));
        assert_eq!(search.total_pages, 2);

        let details = provider.get_details("sousou-no-frieren").await.unwrap();
        assert_eq!(details.mal_id, Some(52991));
        assert_eq!(details.status, AnimeStatus::Completed);
        assert_eq!(details.genres, vec!["Fantasía"]);
        assert_eq!(details.episodes.len(), 2);
        assert_eq!(details.episodes[1].number, "12.5");

        let eps_page = provider.get_episodes(&details, 1).await.unwrap();
        assert_eq!(eps_page.episodes.len(), 2);

        let mirrors = provider.get_episode_mirrors("sousou-no-frieren", "1").await.unwrap();
        assert_eq!(mirrors.len(), 3);
        assert_eq!(mirrors[0].server_name, "HLS");
        assert_eq!(mirrors[1].server_name, "MP4Upload");
        assert_eq!(mirrors[2].server_name, "Voe");
        assert_eq!(mirrors[2].variant, AnimeVariant::Dub);
    }
}
