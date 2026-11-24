use crate::extractor::{VideoExtractor, origin_with_trailing_slash};
use crate::model::{ResolvedStream, StreamKind, VideoMirror};
use crate::{AnimeError, DEFAULT_USER_AGENT};

#[derive(Debug, Default, Clone, Copy)]
pub struct ZillaHlsExtractor;

impl ZillaHlsExtractor {
    fn extract_origin_and_hash(embed_url: &str) -> Option<(String, String)> {
        let parsed = reqwest::Url::parse(embed_url).ok()?;
        let origin = origin_with_trailing_slash(embed_url)?;
        let origin = origin.trim_end_matches('/').to_string();

        let segments: Vec<&str> = parsed.path_segments()?.collect();
        let play_idx = segments.iter().position(|seg| *seg == "play")?;
        let hash = segments.get(play_idx + 1)?.trim();
        if hash.len() < 16 || !hash.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        Some((origin, hash.to_string()))
    }
}

impl VideoExtractor for ZillaHlsExtractor {
    fn name(&self) -> &'static str {
        "ZillaHLS"
    }

    fn can_handle(&self, embed_url: &str, server_hint: &str) -> bool {
        let lower = embed_url.to_ascii_lowercase();
        if lower.contains("zilla-networks.com/play/") {
            return true;
        }
        server_hint.eq_ignore_ascii_case("hls")
            && Self::extract_origin_and_hash(embed_url)
                .is_some_and(|(_, hash)| hash.len() == 32 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    }

    async fn extract(&self, client: &reqwest::Client, mirror: &VideoMirror) -> Result<ResolvedStream, AnimeError> {
        let Some((origin, hash)) = Self::extract_origin_and_hash(&mirror.embed_url) else {
            return Err(AnimeError::ExtractionFailed(format!("Invalid ZillaHLS embed URL: {}", mirror.embed_url)));
        };

        let m3u8_url = format!("{origin}/m3u8/{hash}");
        let referer = format!("{origin}/");

        let response = client
            .get(&m3u8_url)
            .header(reqwest::header::REFERER, &referer)
            .header(reqwest::header::USER_AGENT, DEFAULT_USER_AGENT)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(AnimeError::Http {
                status: Some(status.as_u16()),
                message: format!("ZillaHLS returned HTTP {status} for {m3u8_url}"),
            });
        }

        let playlist = response.text().await?;
        if !playlist.contains("#EXTM3U") {
            return Err(AnimeError::ExtractionFailed(format!("ZillaHLS response from {m3u8_url} is not a valid M3U8 playlist")));
        }

        Ok(ResolvedStream {
            stream_url: m3u8_url,
            headers: vec![("Referer".to_string(), referer), ("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string())],
            kind: StreamKind::Hls,
            server_name: "HLS (Zilla)".to_string(),
            quality: Some("HD".to_string()),
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
    async fn extracts_zilla_hls_playlist() {
        let server = MockServer::start();
        let hash = "7601658844858539e438dceee32e7924";

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/m3u8/{hash}"))
                .header("referer", format!("{}/", server.base_url()));
            then.status(200)
                .header("content-type", "application/x-mpegURL")
                .body("#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-MAP:URI=\"https://player.zilla-networks.com/segs/x/init.html\"\n");
        });

        let client = build_default_http_client().unwrap();
        let mirror = VideoMirror {
            server_name: "HLS".to_string(),
            embed_url: format!("{}/play/{hash}", server.base_url()),
            variant: AnimeVariant::Sub,
            referer: "https://animeav1.com/".to_string(),
            size: None,
        };

        let extractor = ZillaHlsExtractor;
        assert!(extractor.can_handle(&mirror.embed_url, &mirror.server_name));

        let stream = extractor.extract(&client, &mirror).await.unwrap();
        mock.assert();
        assert_eq!(stream.kind, StreamKind::Hls);
        assert_eq!(stream.stream_url, format!("{}/m3u8/{hash}", server.base_url()));
        assert_eq!(stream.server_name, "HLS (Zilla)");
        assert_eq!(stream.quality.as_deref(), Some("HD"));
    }

    #[tokio::test]
    async fn fails_when_zilla_playlist_is_not_m3u8() {
        let server = MockServer::start();
        let hash = "7601658844858539e438dceee32e7924";

        server.mock(|when, then| {
            when.method(GET).path(format!("/m3u8/{hash}"));
            then.status(200).body("<html>not a playlist</html>");
        });

        let client = build_default_http_client().unwrap();
        let mirror = VideoMirror {
            server_name: "HLS".to_string(),
            embed_url: format!("{}/play/{hash}", server.base_url()),
            variant: AnimeVariant::Sub,
            referer: "https://animeav1.com/".to_string(),
            size: None,
        };

        let err = ZillaHlsExtractor.extract(&client, &mirror).await.unwrap_err();
        assert!(matches!(err, AnimeError::ExtractionFailed(_)));
    }
}
