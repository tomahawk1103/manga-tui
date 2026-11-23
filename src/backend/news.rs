//! Manga & anime news aggregator based on public RSS feeds.
//!
//! Only public, freely distributed RSS feeds are used. We show the headline, a short summary and a
//! link; the full article is always opened on the original website.

use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NewsSource {
    RamenParaDos,
    AnimeNewsNetwork,
    MyAnimeList,
}

impl NewsSource {
    pub const ALL: [NewsSource; 3] = [NewsSource::RamenParaDos, NewsSource::AnimeNewsNetwork, NewsSource::MyAnimeList];

    pub fn name(self) -> &'static str {
        match self {
            NewsSource::RamenParaDos => "Ramen Para Dos",
            NewsSource::AnimeNewsNetwork => "Anime News Network",
            NewsSource::MyAnimeList => "MyAnimeList",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            NewsSource::RamenParaDos => "R4D",
            NewsSource::AnimeNewsNetwork => "ANN",
            NewsSource::MyAnimeList => "MAL",
        }
    }

    pub fn language(self) -> &'static str {
        match self {
            NewsSource::RamenParaDos => "ES",
            _ => "EN",
        }
    }

    pub fn feed_url(self) -> &'static str {
        match self {
            NewsSource::RamenParaDos => "https://ramenparados.com/feed/",
            NewsSource::AnimeNewsNetwork => "https://www.animenewsnetwork.com/all/rss.xml?ann-edition=w",
            NewsSource::MyAnimeList => "https://myanimelist.net/rss/news.xml",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewsItem {
    pub source: NewsSource,
    pub title: String,
    pub link: String,
    pub summary: String,
    pub published: Option<DateTime<FixedOffset>>,
    /// Thumbnail / hero image of the article, when the feed provides one.
    /// If `None`, the article page's `og:image` is looked up lazily.
    pub image_url: Option<String>,
}

static MEDIA_URL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<(?:media:content|media:thumbnail|enclosure|itunes:image)\b[^>]*?\b(?:url|href)=["']([^"']+)["']"#).unwrap()
});
static IMG_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?is)<img\b[^>]*?\bsrc=["']([^"']+)["']"#).unwrap());
static OG_IMAGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<meta\b[^>]*?(?:property|name)=["'](?:og:image(?::secure_url)?|twitter:image(?::src)?)["'][^>]*?content=["']([^"']+)["']"#,
    )
    .unwrap()
});
static OG_IMAGE_REV_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<meta\b[^>]*?content=["']([^"']+)["'][^>]*?(?:property|name)=["'](?:og:image(?::secure_url)?|twitter:image(?::src)?)["']"#,
    )
    .unwrap()
});

/// Turns protocol-relative or root-relative URLs into absolute ones using `base` (the article link).
pub fn absolutize_url(url: &str, base: &str) -> String {
    let url = url.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    if let Some(rest) = url.strip_prefix("//") {
        return format!("https://{rest}");
    }
    if url.starts_with('/')
        && let Some(scheme_end) = base.find("://")
    {
        let host_end = base[scheme_end + 3..].find('/').map(|i| i + scheme_end + 3).unwrap_or(base.len());
        return format!("{}{url}", &base[..host_end]);
    }
    url.to_string()
}

fn is_probably_image(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    // Skip tracking pixels, emojis and feed icons
    !(lower.contains("feedburner") || lower.contains("/emoji/") || lower.contains("gravatar") || lower.contains("pixel"))
}

/// Finds the best image in an RSS/Atom item block (media tags first, then the first `<img>` in the HTML body).
fn extract_item_image(block: &str, link: &str) -> Option<String> {
    let from_media = MEDIA_URL_RE
        .captures_iter(block)
        .map(|c| decode_entities(&c[1]))
        .find(|u| is_probably_image(u));
    let candidate = from_media.or_else(|| {
        let html = decode_entities(&CDATA_RE.replace_all(block, "$1"));
        IMG_TAG_RE
            .captures_iter(&html)
            .map(|c| decode_entities(&c[1]))
            .find(|u| is_probably_image(u))
    })?;
    Some(absolutize_url(&candidate, link))
}

/// Extracts the `og:image` / `twitter:image` of an article page.
pub fn extract_og_image(html: &str, page_url: &str) -> Option<String> {
    OG_IMAGE_RE
        .captures(html)
        .or_else(|| OG_IMAGE_REV_RE.captures(html))
        .map(|c| absolutize_url(&decode_entities(&c[1]), page_url))
}

static ITEM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<(item|entry)[\s>].*?</(item|entry)>").unwrap());
static CDATA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<!\[CDATA\[(.*?)\]\]>").unwrap());
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static WS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static ATOM_LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?is)<link[^>]*href=["']([^"']+)["']"#).unwrap());

fn tag_content(block: &str, tag: &str) -> Option<String> {
    let re = Regex::new(&format!(r"(?is)<{tag}(?:\s[^>]*)?>(.*?)</{tag}>")).ok()?;
    re.captures(block).map(|c| c[1].to_string())
}

fn decode_entities(input: &str) -> String {
    let mut out = input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&hellip;", "…")
        .replace("&#8230;", "…")
        .replace("&#8217;", "’")
        .replace("&#8216;", "‘")
        .replace("&#8220;", "“")
        .replace("&#8221;", "”")
        .replace("&#8211;", "–")
        .replace("&#8212;", "—");
    // Generic numeric entities
    static NUM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#(x?[0-9a-fA-F]+);").unwrap());
    out = NUM_RE
        .replace_all(&out, |c: &regex::Captures<'_>| {
            let raw = &c[1];
            let code = if let Some(hex) = raw.strip_prefix('x') { u32::from_str_radix(hex, 16).ok() } else { raw.parse().ok() };
            code.and_then(char::from_u32).map(|ch| ch.to_string()).unwrap_or_default()
        })
        .into_owned();
    out.replace("&amp;", "&")
}

/// Turns raw RSS field content (possibly CDATA + escaped HTML) into clean plain text.
pub fn clean_text(raw: &str) -> String {
    let unwrapped = CDATA_RE.replace_all(raw, "$1");
    // Entities may hide HTML tags (`&lt;p&gt;`), so decode, strip tags, then decode again.
    let decoded = decode_entities(&unwrapped);
    let stripped = TAG_RE.replace_all(&decoded, " ");
    let decoded_again = decode_entities(&stripped);
    WS_RE.replace_all(decoded_again.trim(), " ").into_owned()
}

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    let raw = raw.trim();
    DateTime::parse_from_rfc2822(raw).or_else(|_| DateTime::parse_from_rfc3339(raw)).ok()
}

/// Parses an RSS 2.0 or Atom document into news items.
pub fn parse_feed(xml: &str, source: NewsSource) -> Vec<NewsItem> {
    ITEM_RE
        .find_iter(xml)
        .filter_map(|m| {
            let block = m.as_str();
            let title = clean_text(&tag_content(block, "title")?);
            if title.is_empty() {
                return None;
            }
            let link = tag_content(block, "link")
                .map(|l| clean_text(&l))
                .filter(|l| !l.is_empty())
                .or_else(|| ATOM_LINK_RE.captures(block).map(|c| decode_entities(&c[1])))
                .unwrap_or_default();
            let summary = tag_content(block, "description")
                .or_else(|| tag_content(block, "summary"))
                .or_else(|| tag_content(block, "content"))
                .or_else(|| tag_content(block, "content:encoded"))
                .map(|s| clean_text(&s))
                .unwrap_or_default();
            let image_url = extract_item_image(block, &link);
            let published = tag_content(block, "pubDate")
                .or_else(|| tag_content(block, "published"))
                .or_else(|| tag_content(block, "updated"))
                .or_else(|| tag_content(block, "dc:date"))
                .and_then(|d| parse_date(&clean_text(&d)));
            Some(NewsItem {
                source,
                title,
                link,
                summary,
                published,
                image_url,
            })
        })
        .collect()
}

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Mozilla/5.0 (compatible; manga-tui/", env!("CARGO_PKG_VERSION"), "; RSS reader)"))
        .build()
        .unwrap_or_default()
});

/// Max side (px) of decoded news images; keeps memory and Sixel encoding cheap.
const MAX_IMAGE_SIDE: u32 = 960;

async fn download_image(url: &str) -> Result<image::DynamicImage, String> {
    let response = HTTP_CLIENT.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        Ok(if img.width() > MAX_IMAGE_SIDE || img.height() > MAX_IMAGE_SIDE {
            img.thumbnail(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE)
        } else {
            img
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn find_og_image(link: &str) -> Option<String> {
    let response = HTTP_CLIENT.get(link).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let html = response.text().await.ok()?;
    extract_og_image(&html, link)
}

/// Downloads the image of a news item: uses the feed image, or falls back to the article's `og:image`.
pub async fn fetch_news_image(image_url: Option<String>, link: String) -> Result<image::DynamicImage, String> {
    if let Some(url) = image_url
        && let Ok(img) = download_image(&url).await
    {
        return Ok(img);
    }
    let og = find_og_image(&link).await.ok_or_else(|| "no image".to_string())?;
    download_image(&og).await
}

pub async fn fetch_source(source: NewsSource) -> Result<Vec<NewsItem>, String> {
    let response = HTTP_CLIENT.get(source.feed_url()).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let body = response.text().await.map_err(|e| e.to_string())?;
    let items = parse_feed(&body, source);
    if items.is_empty() {
        return Err("feed vacío / empty feed".to_string());
    }
    Ok(items)
}

/// Sorts newest first; undated items go last.
pub fn sort_newest_first(items: &mut [NewsItem]) {
    items.sort_by(|a, b| b.published.cmp(&a.published));
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?><rss><channel><title>Feed</title>
        <item><title><![CDATA[Frieren &#8211; temporada 2]]></title><link>https://example.com/a</link>
        <description><![CDATA[<p>Nueva <b>temporada</b> &amp; más</p>]]></description>
        <pubDate>Mon, 23 Nov 2026 10:00:00 +0000</pubDate></item>
        <item><title>One Piece &amp; Co</title><link>https://example.com/b</link>
        <description>&lt;p&gt;Hola&lt;/p&gt;</description><pubDate>Sun, 22 Nov 2026 10:00:00 +0000</pubDate></item>
        </channel></rss>"#;

    #[test]
    fn parses_rss_items_and_cleans_html() {
        let items = parse_feed(RSS, NewsSource::RamenParaDos);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Frieren – temporada 2");
        assert_eq!(items[0].link, "https://example.com/a");
        assert_eq!(items[0].summary, "Nueva temporada & más");
        assert_eq!(items[1].title, "One Piece & Co");
        assert_eq!(items[1].summary, "Hola");
        assert!(items[0].published > items[1].published);
    }

    #[test]
    fn parses_atom_entries() {
        let atom = r#"<feed><entry><title>Atom news</title><link rel="alternate" href="https://x.y/z"/>
            <summary>Sum</summary><updated>2026-11-23T10:00:00Z</updated></entry></feed>"#;
        let items = parse_feed(atom, NewsSource::MyAnimeList);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].link, "https://x.y/z");
        assert!(items[0].published.is_some());
    }

    #[test]
    fn sorts_newest_first() {
        let mut items = parse_feed(RSS, NewsSource::RamenParaDos);
        items.reverse();
        sort_newest_first(&mut items);
        assert_eq!(items[0].link, "https://example.com/a");
    }

    #[test]
    fn extracts_images_from_media_tags_and_html_body() {
        let feed = r#"<rss><channel>
            <item><title>A</title><link>https://site.com/a</link>
              <media:thumbnail url="https://cdn.site.com/a.jpg" width="100"/></item>
            <item><title>B</title><link>https://site.com/b</link>
              <description><![CDATA[<p><img class="x" src="/wp/b.png" /> text</p>]]></description></item>
            <item><title>C</title><link>https://site.com/c</link>
              <description>&lt;img src=&quot;//cdn.site.com/c.webp&quot;&gt; hi</description></item>
            <item><title>D</title><link>https://site.com/d</link><description>no image</description></item>
            </channel></rss>"#;
        let items = parse_feed(feed, NewsSource::AnimeNewsNetwork);
        assert_eq!(items[0].image_url.as_deref(), Some("https://cdn.site.com/a.jpg"));
        assert_eq!(items[1].image_url.as_deref(), Some("https://site.com/wp/b.png"));
        assert_eq!(items[2].image_url.as_deref(), Some("https://cdn.site.com/c.webp"));
        assert_eq!(items[3].image_url, None);
    }

    #[test]
    fn extracts_og_image_in_any_attribute_order() {
        let a = r#"<head><meta property="og:image" content="https://x.com/i.jpg"></head>"#;
        let b = r#"<head><meta content="/img/j.jpg" name="twitter:image"></head>"#;
        assert_eq!(extract_og_image(a, "https://x.com/p").as_deref(), Some("https://x.com/i.jpg"));
        assert_eq!(extract_og_image(b, "https://x.com/p/q").as_deref(), Some("https://x.com/img/j.jpg"));
        assert_eq!(extract_og_image("<head></head>", "https://x.com"), None);
    }
}
