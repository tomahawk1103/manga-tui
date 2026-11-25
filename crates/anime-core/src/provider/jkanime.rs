use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{
    AnimeDetails, AnimeEpisode, AnimeEpisodesPage, AnimeHomeFeed, AnimeProviders, AnimeSearchResult, AnimeStatus, AnimeSummary,
    AnimeType, AnimeVariant, RecentAnimeEpisode, VideoMirror,
};
use crate::provider::AnimeProvider;
use crate::{AnimeError, DEFAULT_USER_AGENT};

const JKANIME_THUMB_CDN: &str = "https://cdn.jkdesa.com/assets/images/animes/video/image_thumb/";
const DEFAULT_EPISODES_PER_PAGE: u32 = 16;

#[derive(Debug, Clone)]
pub struct JkAnimeProvider {
    client: reqwest::Client,
    base_url: String,
    retry_delay_ms: u64,
}

impl JkAnimeProvider {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: AnimeProviders::JkAnime.base_url().to_string(),
            retry_delay_ms: 800,
        }
    }

    pub fn with_base_url(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            retry_delay_ms: 20,
        }
    }

    async fn get_html(&self, url: &str) -> Result<String, AnimeError> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
            .header(reqwest::header::REFERER, format!("{}/", self.base_url))
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 404 {
            return Err(AnimeError::NotFound(format!("JKAnime resource not found: {url}")));
        }
        if status.as_u16() == 429 {
            return Err(AnimeError::RateLimited(format!("JKAnime rate limited request: {url}")));
        }
        if !status.is_success() {
            return Err(AnimeError::Http {
                status: Some(status.as_u16()),
                message: format!("JKAnime returned HTTP {status} for {url}"),
            });
        }

        response.text().await.map_err(AnimeError::from)
    }

    fn resolve_url(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else if trimmed.starts_with('/') {
            format!("{}{trimmed}", self.base_url)
        } else {
            format!("{}/{trimmed}", self.base_url)
        }
    }
}

fn decode_base64_utf8(raw: &str) -> Option<String> {
    let bytes = STANDARD.decode(raw.trim()).or_else(|_| STANDARD_NO_PAD.decode(raw.trim())).ok()?;
    String::from_utf8(bytes).ok()
}

fn slug_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.split('/').rev().find(|seg| !seg.trim().is_empty()).map(ToString::to_string)
}

fn encode_search_segment(query: &str) -> String {
    let normalized = query.split_whitespace().collect::<Vec<_>>().join("_");
    let mut out = String::with_capacity(normalized.len());
    for byte in normalized.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn parse_jkanime_status(raw: &str) -> AnimeStatus {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains("emision") || lower.contains("emisión") {
        AnimeStatus::Ongoing
    } else if lower.contains("concluido") || lower.contains("finalizado") {
        AnimeStatus::Completed
    } else if lower.contains("estreno") || lower.contains("proximamente") || lower.contains("próximamente") {
        AnimeStatus::Upcoming
    } else {
        AnimeStatus::Unknown
    }
}

fn parse_jkanime_type(raw: &str) -> AnimeType {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains("serie") || lower == "tv" {
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

fn video_iframe_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)video\[(\d+)\]\s*=\s*['"]\s*<iframe[^>]+src=['"]([^'"]+)['"]"#).expect("valid regex"))
}

fn extract_json_array_after(haystack: &str, marker: &str) -> Option<String> {
    let idx = haystack.find(marker)?;
    let rest = &haystack[idx + marker.len()..];
    let start = rest.find('[')?;
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;

    for (offset, &b) in bytes[start..].iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(rest[start..=start + offset].to_string());
                }
            },
            _ => {},
        }
    }
    None
}

fn infer_jkplayer_name(embed_url: &str) -> &'static str {
    let lower = embed_url.to_ascii_lowercase();
    if lower.contains("/jkplayer/umv") {
        "Magi"
    } else if lower.contains("/jkplayer/um") {
        "Desu"
    } else if lower.contains("/jkplayer/jk") {
        "Desuka"
    } else {
        "JKPlayer"
    }
}

fn mirror_priority(mirror: &VideoMirror) -> u8 {
    let url = mirror.embed_url.to_ascii_lowercase();
    let name = mirror.server_name.to_ascii_lowercase();
    if url.contains("/jkplayer/umv") || name == "magi" {
        0
    } else if url.contains("/jkplayer/um") || name == "desu" {
        1
    } else if name == "mp4upload" || url.contains("mp4upload.com") {
        2
    } else if url.contains("/jkplayer/jk") || name == "desuka" {
        3
    } else {
        4
    }
}

#[derive(Debug, Deserialize)]
struct JkEpisodesResponse {
    total: Option<u32>,
    last_page: Option<u32>,
    current_page: Option<u32>,
    per_page: Option<u32>,
    #[serde(default)]
    data: Vec<JkEpisodeItem>,
}

#[derive(Debug, Deserialize)]
struct JkEpisodeItem {
    id: Option<Value>,
    number: Value,
    title: Option<String>,
    image: Option<String>,
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JkServerItem {
    remote: String,
    server: String,
    lang: Option<u8>,
    size: Option<String>,
}

fn value_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        },
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn parse_search_html(html: &str, current_page: u32) -> AnimeSearchResult {
    let doc = Html::parse_document(html);
    let card_sel = Selector::parse(".row.page_directorio > div").unwrap();
    let title_link_sel = Selector::parse(".anime__item h5 a").unwrap();
    let pic_sel = Selector::parse(".anime__item__pic").unwrap();
    let img_sel = Selector::parse(".anime__item__pic img").unwrap();
    let badge_sel = Selector::parse(".anime__item__text ul li").unwrap();
    let nav_links_sel = Selector::parse(".navigation a, a.nav-next").unwrap();

    let mut items = Vec::new();
    for card in doc.select(&card_sel) {
        let Some(link_el) = card.select(&title_link_sel).next() else {
            continue;
        };
        let title = link_el.text().collect::<Vec<_>>().join(" ").trim().to_string();
        let Some(href) = link_el.value().attr("href") else {
            continue;
        };
        let Some(slug) = slug_from_url(href) else {
            continue;
        };

        let id = card
            .value()
            .attr("data-g")
            .and_then(decode_base64_utf8)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| slug.clone());

        let cover_url = card
            .select(&pic_sel)
            .next()
            .and_then(|el| el.value().attr("data-setbg"))
            .or_else(|| card.select(&img_sel).next().and_then(|el| el.value().attr("src")))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut status = AnimeStatus::Unknown;
        let mut anime_type = AnimeType::Unknown;
        for badge in card.select(&badge_sel) {
            let text = badge.text().collect::<Vec<_>>().join(" ");
            let parsed_status = parse_jkanime_status(&text);
            if parsed_status != AnimeStatus::Unknown {
                status = parsed_status;
            }
            let parsed_type = parse_jkanime_type(&text);
            if parsed_type != AnimeType::Unknown {
                anime_type = parsed_type;
            }
        }

        items.push(AnimeSummary {
            id,
            slug,
            title,
            synopsis: None,
            cover_url,
            anime_type,
            status,
            provider: AnimeProviders::JkAnime,
        });
    }

    let next_marker = format!("/{}/", current_page + 1);
    let has_next = doc.select(&nav_links_sel).any(|el| {
        el.value().attr("href").is_some_and(|h| h.contains(&next_marker))
            || el.value().attr("class").is_some_and(|c| c.contains("nav-next"))
    });
    let total_pages = if has_next { current_page + 1 } else { current_page.max(1) };

    AnimeSearchResult {
        items,
        current_page: current_page.max(1),
        total_pages,
        total_records: None,
    }
}

fn parse_details_html(html: &str, slug: &str) -> Result<AnimeDetails, AnimeError> {
    let doc = Html::parse_document(html);
    let title_sel = Selector::parse("div.anime_info h3").unwrap();
    let synopsis_sel = Selector::parse("div.anime_info p.scroll").unwrap();
    let pic_sel = Selector::parse("div.anime_pic img").unwrap();
    let save_btn_sel = Selector::parse("#guardar-anime").unwrap();
    let csrf_sel = Selector::parse(r#"meta[name="csrf-token"]"#).unwrap();
    let info_li_sel = Selector::parse("div.anime_data li").unwrap();
    let span_sel = Selector::parse("span").unwrap();
    let anchor_sel = Selector::parse("a").unwrap();
    let uep_sel = Selector::parse("#uep").unwrap();

    let title = doc
        .select(&title_sel)
        .next()
        .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AnimeError::Parse(format!("Missing anime title in JKAnime page for '{slug}'")))?;

    let synopsis = doc
        .select(&synopsis_sel)
        .next()
        .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
        .unwrap_or_default();

    let cover_url = doc
        .select(&pic_sel)
        .next()
        .and_then(|el| el.value().attr("src"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let id = doc
        .select(&save_btn_sel)
        .next()
        .and_then(|el| el.value().attr("data-anime"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slug.to_string());

    let csrf_token = doc
        .select(&csrf_sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let mut genres = Vec::new();
    let mut anime_type = AnimeType::Unknown;
    let mut status = AnimeStatus::Unknown;
    let mut score = None;
    let mut episodes_count = None;

    for li in doc.select(&info_li_sel) {
        let label = li
            .select(&span_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_ascii_lowercase())
            .unwrap_or_default();
        let full_text = li.text().collect::<Vec<_>>().join(" ");
        let value_text = full_text.split_once(':').map(|(_, v)| v.trim()).unwrap_or("").trim();

        if label.starts_with("tipo") {
            anime_type = parse_jkanime_type(value_text);
        } else if label.starts_with("generos") || label.starts_with("géneros") {
            for a in li.select(&anchor_sel) {
                let genre = a.text().collect::<Vec<_>>().join(" ").trim().to_string();
                if !genre.is_empty() && !genres.contains(&genre) {
                    genres.push(genre);
                }
            }
        } else if label.starts_with("estado") {
            status = parse_jkanime_status(value_text);
        } else if label.starts_with("puntuación") || label.starts_with("puntuacion") {
            score = value_text.parse::<f32>().ok();
        } else if label.starts_with("episodios") {
            episodes_count = value_text.parse::<u32>().ok().filter(|n| *n > 0);
        }
    }

    if episodes_count.is_none()
        && let Some(uep_href) = doc.select(&uep_sel).next().and_then(|el| el.value().attr("href"))
        && let Some(last_num) = slug_from_url(uep_href)
        && let Ok(parsed) = last_num.parse::<u32>()
    {
        episodes_count = Some(parsed);
    }

    Ok(AnimeDetails {
        id,
        slug: slug.to_string(),
        title,
        synopsis,
        cover_url,
        genres,
        status,
        anime_type,
        score,
        mal_id: None,
        episodes_count,
        provider: AnimeProviders::JkAnime,
        episodes: Vec::new(),
        csrf_token,
    })
}

fn parse_episode_mirrors_html(provider: &JkAnimeProvider, html: &str, referer: &str) -> Result<Vec<VideoMirror>, AnimeError> {
    let doc = Html::parse_document(html);
    let btn_sel = Selector::parse(".bg-servers a[data-id]").unwrap();

    let mut button_map: HashMap<String, (String, AnimeVariant)> = HashMap::new();
    for btn in doc.select(&btn_sel) {
        let Some(data_id) = btn.value().attr("data-id") else {
            continue;
        };
        let label = btn.text().collect::<Vec<_>>().join(" ").trim().to_string();
        let classes = btn.value().attr("class").unwrap_or("");
        let variant = if classes.contains("lg_2") { AnimeVariant::Dub } else { AnimeVariant::Sub };
        button_map.insert(data_id.trim().to_string(), (label, variant));
    }

    let mut mirrors = Vec::new();
    let mut seen_urls = HashSet::new();

    for cap in video_iframe_re().captures_iter(html) {
        let idx = &cap[1];
        let embed_url = provider.resolve_url(&cap[2]);
        if !seen_urls.insert(embed_url.clone()) {
            continue;
        }
        let (server_name, variant) = button_map
            .get(idx)
            .cloned()
            .filter(|(name, _)| !name.is_empty())
            .unwrap_or_else(|| (infer_jkplayer_name(&embed_url).to_string(), AnimeVariant::Sub));

        mirrors.push(VideoMirror {
            server_name,
            embed_url,
            variant,
            referer: referer.to_string(),
            size: None,
        });
    }

    if let Some(json_str) = extract_json_array_after(html, "var servers")
        && let Ok(items) = serde_json::from_str::<Vec<JkServerItem>>(&json_str)
    {
        for item in items {
            let Some(decoded_url) = decode_base64_utf8(&item.remote) else {
                continue;
            };
            let decoded_url = decoded_url.trim().to_string();
            if !(decoded_url.starts_with("http://") || decoded_url.starts_with("https://")) {
                continue;
            }
            if !seen_urls.insert(decoded_url.clone()) {
                continue;
            }
            let variant = if item.lang == Some(2) { AnimeVariant::Dub } else { AnimeVariant::Sub };
            mirrors.push(VideoMirror {
                server_name: item.server.trim().to_string(),
                embed_url: decoded_url,
                variant,
                referer: referer.to_string(),
                size: item.size.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            });
        }
    }

    mirrors.sort_by_key(mirror_priority);
    Ok(mirrors)
}

fn slug_and_episode_from_url(url: &str) -> Option<(String, String)> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let mut segs = path.split('/').filter(|seg| !seg.trim().is_empty()).collect::<Vec<_>>();
    let last = segs.pop()?.trim().to_string();
    if !last.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let slug = segs.pop()?.trim().to_string();
    (!slug.is_empty()).then_some((slug, last))
}

fn parse_home_html(html: &str) -> AnimeHomeFeed {
    let doc = Html::parse_document(html);
    let ep_card_sel = Selector::parse("#animes .dir1 .card, #ovas .dir1 .card").unwrap();
    let link_sel = Selector::parse("a[href]").unwrap();
    let img_sel = Selector::parse("img").unwrap();
    let title_sel = Selector::parse(".card-title").unwrap();
    let ep_badge_sel = Selector::parse(".badges .badge-primary").unwrap();
    let time_badge_sel = Selector::parse(".badges .badge-secondary").unwrap();
    let hero_sel = Selector::parse(".hero__items").unwrap();
    let hero_title_sel = Selector::parse(".hero__text h2").unwrap();
    let hero_info_sel = Selector::parse(".hero__text .ainfo span").unwrap();
    let hero_link_sel = Selector::parse("a.slider-show[href]").unwrap();
    let top_card_sel = Selector::parse(".toplist .card").unwrap();
    let top_synopsis_sel = Selector::parse(".card-synopsis").unwrap();

    let mut recent_episodes = Vec::new();
    let mut seen_episodes = HashSet::new();
    for card in doc.select(&ep_card_sel) {
        let Some(anchor) = card.select(&link_sel).next() else {
            continue;
        };
        if anchor.value().attr("class").is_some_and(|c| c.contains("mayor-edad")) {
            continue;
        }
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        if href.contains("hentaijk") {
            continue;
        }

        let (slug, episode_number) = if let Some(pair) = slug_and_episode_from_url(href) {
            pair
        } else {
            let Some(slug) = slug_from_url(href) else {
                continue;
            };
            let ep_num = card
                .select(&ep_badge_sel)
                .next()
                .map(|el| el.text().collect::<Vec<_>>().join(" "))
                .map(|text| text.trim().trim_start_matches("Ep.").trim_start_matches("Ep").trim().to_string())
                .filter(|s| s.chars().next().is_some_and(|ch| ch.is_ascii_digit()));
            let Some(ep_num) = ep_num else {
                continue;
            };
            (slug, ep_num)
        };

        if !seen_episodes.insert((slug.clone(), episode_number.clone())) {
            continue;
        }

        let raw_title = card
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| slug.clone());
        let suffix = format!(" - {episode_number}");
        let anime_title = raw_title.strip_suffix(&suffix).unwrap_or(&raw_title).trim().to_string();

        let aired_label = card
            .select(&time_badge_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty());

        let cover_url = card
            .select(&img_sel)
            .next()
            .and_then(|el| el.value().attr("data-animepic").or_else(|| el.value().attr("src")))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);

        recent_episodes.push(RecentAnimeEpisode {
            anime_id: slug.clone(),
            slug,
            anime_title,
            episode_number,
            aired_label,
            cover_url,
            provider: AnimeProviders::JkAnime,
        });
    }

    let mut recommendations = Vec::new();
    let mut seen_slugs = HashSet::new();

    for hero in doc.select(&hero_sel) {
        let Some(href) = hero.select(&hero_link_sel).next().and_then(|el| el.value().attr("href")) else {
            continue;
        };
        if href.contains("hentaijk") {
            continue;
        }
        let Some(slug) = slug_from_url(href) else {
            continue;
        };
        if !seen_slugs.insert(slug.clone()) {
            continue;
        }
        let title = hero
            .select(&hero_title_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| slug.clone());
        let cover_url = hero
            .value()
            .attr("data-setbg")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
        let mut status = AnimeStatus::Unknown;
        let mut anime_type = AnimeType::Unknown;
        for badge in hero.select(&hero_info_sel) {
            let text = badge.text().collect::<Vec<_>>().join(" ");
            let st = parse_jkanime_status(&text);
            if st != AnimeStatus::Unknown {
                status = st;
            }
            let ty = parse_jkanime_type(&text);
            if ty != AnimeType::Unknown {
                anime_type = ty;
            }
        }
        recommendations.push(AnimeSummary {
            id: slug.clone(),
            slug,
            title,
            synopsis: None,
            cover_url,
            anime_type,
            status,
            provider: AnimeProviders::JkAnime,
        });
    }

    for card in doc.select(&top_card_sel) {
        let Some(anchor) = card.select(&link_sel).next() else {
            continue;
        };
        if anchor.value().attr("class").is_some_and(|c| c.contains("mayor-edad")) {
            continue;
        }
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        if href.contains("hentaijk") {
            continue;
        }
        let Some(slug) = slug_from_url(href) else {
            continue;
        };
        if !seen_slugs.insert(slug.clone()) {
            continue;
        }
        let title = card
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| slug.clone());
        let synopsis = card
            .select(&top_synopsis_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .filter(|s| !s.is_empty());
        let cover_url = card
            .select(&img_sel)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
        recommendations.push(AnimeSummary {
            id: slug.clone(),
            slug,
            title,
            synopsis,
            cover_url,
            anime_type: AnimeType::Unknown,
            status: AnimeStatus::Unknown,
            provider: AnimeProviders::JkAnime,
        });
    }

    AnimeHomeFeed {
        recent_episodes,
        recommendations,
    }
}

impl AnimeProvider for JkAnimeProvider {
    fn kind(&self) -> AnimeProviders {
        AnimeProviders::JkAnime
    }

    async fn get_home_feed(&self) -> Result<AnimeHomeFeed, AnimeError> {
        let html = self.get_html(&format!("{}/", self.base_url)).await?;
        Ok(parse_home_html(&html))
    }

    async fn search(&self, query: &str, page: u32) -> Result<AnimeSearchResult, AnimeError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Err(AnimeError::InvalidInput("Search query cannot be empty".to_string()));
        }

        let segment = encode_search_segment(trimmed);
        let page = page.max(1);
        let url = if page <= 1 {
            format!("{}/buscar/{segment}/", self.base_url)
        } else {
            format!("{}/buscar/{segment}/{page}/", self.base_url)
        };

        let html = self.get_html(&url).await?;
        Ok(parse_search_html(&html, page))
    }

    async fn get_details(&self, slug: &str) -> Result<AnimeDetails, AnimeError> {
        let slug = slug.trim().trim_matches('/');
        if slug.is_empty() {
            return Err(AnimeError::InvalidInput("Anime slug cannot be empty".to_string()));
        }
        let url = format!("{}/{slug}/", self.base_url);
        let html = self.get_html(&url).await?;
        parse_details_html(&html, slug)
    }

    async fn get_episodes(&self, details: &AnimeDetails, page: u32) -> Result<AnimeEpisodesPage, AnimeError> {
        let needs_session =
            details.csrf_token.as_ref().is_none_or(|t| t.is_empty()) || !details.id.chars().all(|c| c.is_ascii_digit());
        let owned_details;
        let active_details = if needs_session {
            owned_details = self.get_details(&details.slug).await?;
            &owned_details
        } else {
            details
        };

        let token = active_details
            .csrf_token
            .as_deref()
            .ok_or_else(|| AnimeError::Parse("Missing JKAnime CSRF token for episodes AJAX request".to_string()))?;
        let page = page.max(1);
        let ajax_url = format!("{}/ajax/episodes/{}/{page}", self.base_url, active_details.id);
        let referer = format!("{}/{}/", self.base_url, active_details.slug);

        let mut last_rate_limit = None;
        let mut response_opt = None;
        for attempt in 0..3u64 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(self.retry_delay_ms * attempt)).await;
            }
            let response = self
                .client
                .post(&ajax_url)
                .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
                .header(reqwest::header::REFERER, &referer)
                .header("X-Requested-With", "XMLHttpRequest")
                .form(&[("_token", token)])
                .send()
                .await?;

            if response.status().as_u16() == 429 {
                last_rate_limit = Some(AnimeError::RateLimited(format!("JKAnime HTTP 429 on {ajax_url}")));
                continue;
            }
            response_opt = Some(response);
            break;
        }

        let response = match response_opt {
            Some(resp) => resp,
            None => return Err(last_rate_limit.unwrap_or_else(|| AnimeError::RateLimited(ajax_url.clone()))),
        };

        let status = response.status();
        if !status.is_success() {
            return Err(AnimeError::Http {
                status: Some(status.as_u16()),
                message: format!("JKAnime episodes AJAX returned HTTP {status}"),
            });
        }

        let payload: JkEpisodesResponse = response
            .json()
            .await
            .map_err(|e| AnimeError::Parse(format!("Invalid JKAnime episodes JSON ({ajax_url}): {e}")))?;

        let episodes = payload
            .data
            .into_iter()
            .filter_map(|item| {
                let number = value_to_string(&item.number)?;
                let thumb = item.image.and_then(|img| {
                    let trimmed = img.trim();
                    if trimmed.len() <= 5 {
                        None
                    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                        Some(trimmed.to_string())
                    } else {
                        Some(format!("{JKANIME_THUMB_CDN}{trimmed}"))
                    }
                });
                Some(AnimeEpisode {
                    id: item.id.as_ref().and_then(value_to_string),
                    number,
                    title: item.title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
                    thumbnail_url: thumb,
                    aired_at: item.timestamp.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
                })
            })
            .collect();

        let per_page = payload.per_page.unwrap_or(DEFAULT_EPISODES_PER_PAGE).max(1);
        let total_pages = payload.last_page.unwrap_or_else(|| payload.total.unwrap_or(0).div_ceil(per_page)).max(1);

        Ok(AnimeEpisodesPage {
            episodes,
            current_page: payload.current_page.unwrap_or(page),
            total_pages,
            total_episodes: payload.total,
        })
    }

    async fn get_episode_mirrors(&self, slug: &str, episode_number: &str) -> Result<Vec<VideoMirror>, AnimeError> {
        let slug = slug.trim().trim_matches('/');
        let episode_number = episode_number.trim().trim_matches('/');
        if slug.is_empty() || episode_number.is_empty() {
            return Err(AnimeError::InvalidInput("Slug and episode number cannot be empty".to_string()));
        }

        let url = format!("{}/{slug}/{episode_number}/", self.base_url);
        let html = self.get_html(&url).await?;
        parse_episode_mirrors_html(self, &html, &url)
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::build_default_http_client;

    #[tokio::test]
    async fn search_parses_base64_ids_slugs_covers_and_badges() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/buscar/one_piece/");
            then.status(200).body(
                r#"
                <div class="row page_directorio">
                  <div class="col-lg-2" data-g="MjAx">
                    <div class="anime__item">
                      <a href="https://jkanime.net/one-piece/">
                        <div class="anime__item__pic" data-setbg="https://cdn.jkdesa.com/assets/images/animes/image/one-piece.jpg"></div>
                      </a>
                      <div class="anime__item__text">
                        <ul><li>En emision</li><li>Serie</li></ul>
                        <h5><a href="https://jkanime.net/one-piece/">One Piece</a></h5>
                      </div>
                    </div>
                  </div>
                </div>
                <div class="navigation"><a class="nav-next" href="/buscar/one_piece/2/">Siguiente</a></div>
                "#,
            );
        });

        let client = build_default_http_client().unwrap();
        let provider = JkAnimeProvider::with_base_url(client, server.base_url());

        let result = provider.search("one piece", 1).await.unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].id, "201");
        assert_eq!(result.items[0].slug, "one-piece");
        assert_eq!(result.items[0].title, "One Piece");
        assert_eq!(result.items[0].status, AnimeStatus::Ongoing);
        assert_eq!(result.items[0].anime_type, AnimeType::Tv);
        assert_eq!(result.total_pages, 2);
    }

    #[tokio::test]
    async fn details_and_episodes_handle_csrf_uep_fallback_and_429_retry() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/one-piece/");
            then.status(200)
                .header("set-cookie", "jkanime-session=abc123; Path=/")
                .body(
                    r#"
                    <html><head><meta name="csrf-token" content="tok_xyz"></head><body>
                      <div class="anime_info">
                        <h3>One Piece</h3>
                        <p class="scroll">Historia pirata.</p>
                      </div>
                      <div class="anime_pic"><img src="https://cdn.jkdesa.com/assets/images/animes/image/one-piece.jpg"></div>
                      <div id="guardar-anime" data-anime="201"></div>
                      <div class="anime_data"><ul>
                        <li><span>Tipo:</span> Serie</li>
                        <li><span>Generos:</span> <a href="/genero/accion/">Accion</a>, <a href="/genero/aventura/">Aventura</a></li>
                        <li><span>Episodios:</span> 0</li>
                        <li><span>Estado:</span> <div class="enemision">En emision</div></li>
                        <li><span>Puntuación:</span> 8.73</li>
                      </ul></div>
                      <a id="uep" href="https://jkanime.net/one-piece/1180/">1180</a>
                    </body></html>
                    "#,
                );
        });

        // First call returns 429, second succeeds
        let rate_limited = server.mock(|when, then| {
            when.method(POST).path("/ajax/episodes/201/1").body("_token=tok_xyz");
            then.status(429).body("slow down");
        });

        let client = build_default_http_client().unwrap();
        let provider = JkAnimeProvider::with_base_url(client, server.base_url());

        let details = provider.get_details("one-piece").await.unwrap();
        assert_eq!(details.id, "201");
        assert_eq!(details.csrf_token.as_deref(), Some("tok_xyz"));
        assert_eq!(details.episodes_count, Some(1180));
        assert_eq!(details.genres, vec!["Accion", "Aventura"]);

        // First confirm 429 retries exhaust cleanly when persistent
        let err = provider.get_episodes(&details, 1).await.unwrap_err();
        assert!(matches!(err, AnimeError::RateLimited(_)));
        assert_eq!(rate_limited.hits(), 3);

        // Now test successful episodes AJAX on page 2
        server.mock(|when, then| {
            when.method(POST)
                .path("/ajax/episodes/201/2")
                .header("cookie", "jkanime-session=abc123")
                .body("_token=tok_xyz");
            then.status(200).json_body(json!({
                "total": 32,
                "last_page": 2,
                "current_page": 2,
                "per_page": 16,
                "data": [
                    { "id": 500, "number": "12.5", "title": "Recap", "image": "thumb_12_5.jpg", "timestamp": "2025-01-01" }
                ]
            }));
        });

        let eps = provider.get_episodes(&details, 2).await.unwrap();
        assert_eq!(eps.current_page, 2);
        assert_eq!(eps.total_pages, 2);
        assert_eq!(eps.episodes.len(), 1);
        assert_eq!(eps.episodes[0].number, "12.5");
        assert_eq!(
            eps.episodes[0].thumbnail_url.as_deref(),
            Some("https://cdn.jkdesa.com/assets/images/animes/video/image_thumb/thumb_12_5.jpg")
        );
    }

    #[tokio::test]
    async fn episode_mirrors_extract_native_and_base64_servers_sorted_by_priority() {
        let server = MockServer::start();
        let mp4_b64 = STANDARD.encode("https://www.mp4upload.com/embed-te2zczstfq98.html");
        let voe_b64 = STANDARD.encode("https://voe.sx/e/2fsei9f0kajt");

        server.mock(|when, then| {
            when.method(GET).path("/one-piece/1/");
            then.status(200).body(format!(
                r#"
                <div class="bg-servers">
                  <a data-id="0" class="servers btn-show lg_1">Desu</a>
                  <a data-id="1" class="servers btn-show lg_1">Magi</a>
                </div>
                <script>
                  var video = [];
                  video[0] = '<iframe class="player_conte" src="https://jkanime.net/jkplayer/um?e=desu123"></iframe>';
                  video[1] = '<iframe class="player_conte" src="https://jkanime.net/jkplayer/umv?e=magi123"></iframe>';
                  var servers = [
                    {{"remote":"{voe_b64}","server":"VOE","lang":1,"size":"52 MB"}},
                    {{"remote":"{mp4_b64}","server":"Mp4upload","lang":1,"size":"52 MB"}}
                  ];
                </script>
                "#
            ));
        });

        let client = build_default_http_client().unwrap();
        let provider = JkAnimeProvider::with_base_url(client, server.base_url());

        let mirrors = provider.get_episode_mirrors("one-piece", "1").await.unwrap();
        assert_eq!(mirrors.len(), 4);
        assert_eq!(mirrors[0].server_name, "Magi");
        assert_eq!(mirrors[1].server_name, "Desu");
        assert_eq!(mirrors[2].server_name, "Mp4upload");
        assert_eq!(mirrors[2].embed_url, "https://www.mp4upload.com/embed-te2zczstfq98.html");
        assert_eq!(mirrors[3].server_name, "VOE");
    }

    #[tokio::test]
    async fn home_feed_parses_airing_episodes_excludes_hentai_and_parses_recommendations() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200).body(
                r#"
                <div class="hero__items set-bg" data-setbg="https://cdn.jkdesa.com/hero.jpg">
                  <div class="hero__text">
                    <div class="ainfo"><span>Serie</span><span>En emision</span></div>
                    <h2>Dragon Ball Super: Beerus</h2>
                  </div>
                  <a class="slider-show" href="https://jkanime.net/dragon-ball-super-beerus/">Ver</a>
                </div>
                <div id="animes">
                  <div class="dir1"><div class="card">
                    <a href="https://jkanime.net/dark-machine-the-animation/7/">
                      <div class="d-thumb">
                        <img src="thumb.jpg" data-animepic="https://cdn.jkdesa.com/dark.jpg">
                        <div class="badges"><span class="badge-primary">Ep 7</span><span class="badge-secondary"> Ayer</span></div>
                      </div>
                      <h5 class="card-title">Dark Machine: The Animation</h5>
                    </a>
                  </div></div>
                </div>
                <div id="hentai">
                  <div class="dir1"><div class="card">
                    <a href="https://hentaijk.com/adult/1/" class="mayor-edad">
                      <h5 class="card-title">Adult - 1</h5>
                    </a>
                  </div></div>
                </div>
                <div class="toplist"><div class="card">
                  <a href="https://jkanime.net/one-piece/">
                    <div class="card-img"><img class="card-img-top" src="https://cdn.jkdesa.com/op.jpg"></div>
                    <h5 class="card-title">One Piece</h5>
                    <div class="card-synopsis">Historia pirata.</div>
                  </a>
                </div></div>
                "#,
            );
        });

        let client = build_default_http_client().unwrap();
        let provider = JkAnimeProvider::with_base_url(client, server.base_url());
        let feed = provider.get_home_feed().await.unwrap();
        assert_eq!(feed.recent_episodes.len(), 1);
        assert_eq!(feed.recent_episodes[0].slug, "dark-machine-the-animation");
        assert_eq!(feed.recent_episodes[0].episode_number, "7");
        assert_eq!(feed.recent_episodes[0].aired_label.as_deref(), Some("Ayer"));
        assert_eq!(feed.recent_episodes[0].cover_url.as_deref(), Some("https://cdn.jkdesa.com/dark.jpg"));

        assert_eq!(feed.recommendations.len(), 2);
        assert_eq!(feed.recommendations[0].slug, "dragon-ball-super-beerus");
        assert_eq!(feed.recommendations[0].status, AnimeStatus::Ongoing);
        assert_eq!(feed.recommendations[1].slug, "one-piece");
        assert_eq!(feed.recommendations[1].synopsis.as_deref(), Some("Historia pirata."));
    }
}
