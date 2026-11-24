use std::sync::OnceLock;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use regex::Regex;

use crate::extractor::{VideoExtractor, origin_with_trailing_slash};
use crate::model::{ResolvedStream, StreamKind, VideoMirror};
use crate::{AnimeError, DEFAULT_USER_AGENT};

#[derive(Debug, Default, Clone, Copy)]
pub struct JkPlayerExtractor;

fn source_m3u8_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)<source[^>]+src=['"](https?://[^'"]+\.m3u8[^'"]*)['"]"#).expect("valid regex"))
}

fn hls_load_source_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"hls\.loadSource\(\s*['"](https?://[^'"]+\.m3u8[^'"]*)['"]"#).expect("valid regex"))
}

fn atob_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"atob\(\s*['"]([A-Za-z0-9+/=]+)['"]\s*\)"#).expect("valid regex"))
}

fn source_mp4_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)<source[^>]+src=['"](https?://[^'"]+\.mp4[^'"]*)['"]"#).expect("valid regex"))
}

fn file_prop_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"file\s*:\s*['"](https?://[^'"]+\.(?:m3u8|mp4)[^'"]*)['"]"#).expect("valid regex"))
}

fn decode_base64_str(raw: &str) -> Option<String> {
    let bytes = STANDARD.decode(raw.trim()).or_else(|_| STANDARD_NO_PAD.decode(raw.trim())).ok()?;
    String::from_utf8(bytes).ok()
}

fn infer_stream_kind(url: &str) -> StreamKind {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".mp4") && !lower.contains(".m3u8") { StreamKind::Mp4 } else { StreamKind::Hls }
}

fn extract_stream_url_from_html(html: &str) -> Option<(String, StreamKind)> {
    if let Some(cap) = source_m3u8_re().captures(html) {
        return Some((cap[1].to_string(), StreamKind::Hls));
    }
    if let Some(cap) = hls_load_source_re().captures(html) {
        return Some((cap[1].to_string(), StreamKind::Hls));
    }
    for cap in atob_re().captures_iter(html) {
        if let Some(decoded) = decode_base64_str(&cap[1]) {
            let trimmed = decoded.trim();
            if (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
                && (trimmed.contains(".m3u8") || trimmed.contains(".mp4"))
            {
                return Some((trimmed.to_string(), infer_stream_kind(trimmed)));
            }
        }
    }
    if let Some(cap) = source_mp4_re().captures(html) {
        return Some((cap[1].to_string(), StreamKind::Mp4));
    }
    if let Some(cap) = file_prop_re().captures(html) {
        let url = cap[1].to_string();
        let kind = infer_stream_kind(&url);
        return Some((url, kind));
    }
    None
}

impl VideoExtractor for JkPlayerExtractor {
    fn name(&self) -> &'static str {
        "JKPlayer"
    }

    fn can_handle(&self, embed_url: &str, _server_hint: &str) -> bool {
        let lower = embed_url.to_ascii_lowercase();
        lower.contains("/jkplayer/umv") || lower.contains("/jkplayer/um") || lower.contains("/jkplayer/jk")
    }

    async fn extract(&self, client: &reqwest::Client, mirror: &VideoMirror) -> Result<ResolvedStream, AnimeError> {
        let response = client
            .get(&mirror.embed_url)
            .header(reqwest::header::REFERER, &mirror.referer)
            .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(AnimeError::Http {
                status: Some(status.as_u16()),
                message: format!("JKPlayer returned HTTP {status} for {}", mirror.embed_url),
            });
        }

        let html = response.text().await?;
        let Some((stream_url, kind)) = extract_stream_url_from_html(&html) else {
            return Err(AnimeError::ExtractionFailed(format!(
                "Could not find stream URL inside JKPlayer embed {}",
                mirror.embed_url
            )));
        };

        let referer = origin_with_trailing_slash(&mirror.embed_url).unwrap_or_else(|| mirror.referer.clone());

        Ok(ResolvedStream {
            stream_url,
            headers: vec![("Referer".to_string(), referer), ("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string())],
            kind,
            server_name: mirror.server_name.clone(),
            quality: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::build_default_http_client;
    use crate::model::AnimeVariant;

    #[tokio::test]
    async fn extracts_magi_umv_m3u8_source() {
        let server = MockServer::start();
        let stream_url = "https://nika.playmudos.com/onupiesz/sample.m3u8?st=abc&e=123";

        server.mock(|when, then| {
            when.method(GET)
                .path("/jkplayer/umv")
                .header("referer", "https://jkanime.net/one-piece/1/");
            then.status(200)
                .body(format!(r#"<video id="video"><source src='{stream_url}' type='application/x-mpegURL'></video>"#));
        });

        let client = build_default_http_client().unwrap();
        let mirror = VideoMirror {
            server_name: "Magi".to_string(),
            embed_url: format!("{}/jkplayer/umv?e=test", server.base_url()),
            variant: AnimeVariant::Sub,
            referer: "https://jkanime.net/one-piece/1/".to_string(),
            size: None,
        };

        let resolved = JkPlayerExtractor.extract(&client, &mirror).await.unwrap();
        assert_eq!(resolved.kind, StreamKind::Hls);
        assert_eq!(resolved.stream_url, stream_url);
        assert_eq!(resolved.server_name, "Magi");
    }

    #[tokio::test]
    async fn extracts_desu_um_atob_m3u8_source() {
        let server = MockServer::start();
        let stream_url = "https://nika.playmudos.com/onupiesz/desu.m3u8?st=xyz&e=456";
        let encoded = STANDARD.encode(stream_url);

        server.mock(|when, then| {
            when.method(GET).path("/jkplayer/um");
            then.status(200).body(format!(
                r#"<script>window.dp = new DPlayer({{ video: {{ url: atob('{encoded}'), type: 'customHls' }} }});</script>"#
            ));
        });

        let client = build_default_http_client().unwrap();
        let mirror = VideoMirror {
            server_name: "Desu".to_string(),
            embed_url: format!("{}/jkplayer/um?e=test", server.base_url()),
            variant: AnimeVariant::Sub,
            referer: "https://jkanime.net/one-piece/1/".to_string(),
            size: None,
        };

        let resolved = JkPlayerExtractor.extract(&client, &mirror).await.unwrap();
        assert_eq!(resolved.kind, StreamKind::Hls);
        assert_eq!(resolved.stream_url, stream_url);
        assert_eq!(resolved.server_name, "Desu");
    }
}
