use std::sync::OnceLock;

use regex::Regex;

use crate::extractor::{VideoExtractor, origin_with_trailing_slash};
use crate::model::{ResolvedStream, StreamKind, VideoMirror};
use crate::{AnimeError, DEFAULT_USER_AGENT};

#[derive(Debug, Default, Clone, Copy)]
pub struct Mp4UploadExtractor;

fn player_src_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)player\.src\(\s*\{.*?src\s*:\s*["'](https?://[^"']+\.mp4(?:\?[^"']*)?)["']"#).expect("valid regex")
    })
}

fn fallback_src_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"src\s*:\s*["'](https?://[^"']+\.mp4(?:\?[^"']*)?)["']"#).expect("valid regex"))
}

impl VideoExtractor for Mp4UploadExtractor {
    fn name(&self) -> &'static str {
        "MP4Upload"
    }

    fn can_handle(&self, embed_url: &str, server_hint: &str) -> bool {
        let lower = embed_url.to_ascii_lowercase();
        lower.contains("mp4upload.com/embed-") || (server_hint.eq_ignore_ascii_case("mp4upload") && lower.contains("embed-"))
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
                message: format!("MP4Upload returned HTTP {status} for {}", mirror.embed_url),
            });
        }

        let html = response.text().await?;
        let stream_url = player_src_re()
            .captures(&html)
            .or_else(|| fallback_src_re().captures(&html))
            .map(|cap| cap[1].to_string())
            .ok_or_else(|| AnimeError::ExtractionFailed(format!("Could not find MP4 source in {}", mirror.embed_url)))?;

        let referer = origin_with_trailing_slash(&mirror.embed_url).unwrap_or_else(|| "https://www.mp4upload.com/".to_string());

        Ok(ResolvedStream {
            stream_url,
            headers: vec![("Referer".to_string(), referer), ("User-Agent".to_string(), DEFAULT_USER_AGENT.to_string())],
            kind: StreamKind::Mp4,
            server_name: "MP4Upload".to_string(),
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
    async fn extracts_mp4upload_direct_video_url() {
        let server = MockServer::start();
        let video_url = "https://a4.mp4upload.com:183/d/token123/video.mp4";

        server.mock(|when, then| {
            when.method(GET)
                .path("/embed-iwh1k72esxo1.html")
                .header("referer", "https://animeav1.com/");
            then.status(200).body(format!(
                r#"
                <script>
                var player = videojs("player", {{}}, function(){{
                    player.src({{
                        type: "video/mp4",
                        src: "{video_url}"
                    }});
                }});
                </script>
                "#
            ));
        });

        let client = build_default_http_client().unwrap();
        let mirror = VideoMirror {
            server_name: "MP4Upload".to_string(),
            embed_url: format!("{}/embed-iwh1k72esxo1.html", server.base_url()),
            variant: AnimeVariant::Sub,
            referer: "https://animeav1.com/".to_string(),
            size: None,
        };

        assert!(Mp4UploadExtractor.can_handle(&mirror.embed_url, &mirror.server_name));
        let resolved = Mp4UploadExtractor.extract(&client, &mirror).await.unwrap();
        assert_eq!(resolved.kind, StreamKind::Mp4);
        assert_eq!(resolved.stream_url, video_url);
        assert_eq!(resolved.server_name, "MP4Upload");
    }
}
