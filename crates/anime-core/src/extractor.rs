use std::future::Future;

use crate::AnimeError;
use crate::model::{ResolvedStream, VideoMirror};

pub mod jkplayer;
pub mod mp4upload;
pub mod zilla_hls;

pub use jkplayer::JkPlayerExtractor;
pub use mp4upload::Mp4UploadExtractor;
pub use zilla_hls::ZillaHlsExtractor;

pub trait VideoExtractor: Send + Sync {
    fn name(&self) -> &'static str;

    fn can_handle(&self, embed_url: &str, server_hint: &str) -> bool;

    fn extract(
        &self,
        client: &reqwest::Client,
        mirror: &VideoMirror,
    ) -> impl Future<Output = Result<ResolvedStream, AnimeError>> + Send;
}

#[derive(Debug, Clone)]
pub enum BuiltinExtractor {
    ZillaHls(ZillaHlsExtractor),
    JkPlayer(JkPlayerExtractor),
    Mp4Upload(Mp4UploadExtractor),
}

impl BuiltinExtractor {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ZillaHls(ext) => ext.name(),
            Self::JkPlayer(ext) => ext.name(),
            Self::Mp4Upload(ext) => ext.name(),
        }
    }

    pub fn can_handle(&self, embed_url: &str, server_hint: &str) -> bool {
        match self {
            Self::ZillaHls(ext) => ext.can_handle(embed_url, server_hint),
            Self::JkPlayer(ext) => ext.can_handle(embed_url, server_hint),
            Self::Mp4Upload(ext) => ext.can_handle(embed_url, server_hint),
        }
    }

    pub async fn extract(&self, client: &reqwest::Client, mirror: &VideoMirror) -> Result<ResolvedStream, AnimeError> {
        match self {
            Self::ZillaHls(ext) => ext.extract(client, mirror).await,
            Self::JkPlayer(ext) => ext.extract(client, mirror).await,
            Self::Mp4Upload(ext) => ext.extract(client, mirror).await,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractorRegistry {
    extractors: Vec<BuiltinExtractor>,
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::default_v1()
    }
}

impl ExtractorRegistry {
    pub fn default_v1() -> Self {
        Self {
            extractors: vec![
                BuiltinExtractor::ZillaHls(ZillaHlsExtractor),
                BuiltinExtractor::JkPlayer(JkPlayerExtractor),
                BuiltinExtractor::Mp4Upload(Mp4UploadExtractor),
            ],
        }
    }

    pub fn supports_mirror(&self, mirror: &VideoMirror) -> bool {
        self.extractors.iter().any(|ext| ext.can_handle(&mirror.embed_url, &mirror.server_name))
    }

    pub async fn extract_mirror(&self, client: &reqwest::Client, mirror: &VideoMirror) -> Result<ResolvedStream, AnimeError> {
        let Some(extractor) = self.extractors.iter().find(|ext| ext.can_handle(&mirror.embed_url, &mirror.server_name)) else {
            return Err(AnimeError::UnsupportedMirror(format!(
                "No extractor available for server '{}' ({})",
                mirror.server_name, mirror.embed_url
            )));
        };

        extractor.extract(client, mirror).await
    }

    pub async fn resolve_best_stream(
        &self,
        client: &reqwest::Client,
        mirrors: &[VideoMirror],
    ) -> Result<ResolvedStream, AnimeError> {
        let mut errors = Vec::new();

        for mirror in mirrors {
            if !self.supports_mirror(mirror) {
                continue;
            }
            match self.extract_mirror(client, mirror).await {
                Ok(stream) => return Ok(stream),
                Err(err) => errors.push(format!("{} ({}): {}", mirror.server_name, mirror.embed_url, err)),
            }
        }

        if errors.is_empty() {
            return Err(AnimeError::UnsupportedMirror("No supported mirrors found for this episode".to_string()));
        }

        Err(AnimeError::ExtractionFailed(format!("All supported mirrors failed: {}", errors.join("; "))))
    }
}

pub(crate) fn origin_with_trailing_slash(url_str: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url_str).ok()?;
    let scheme = parsed.scheme();
    let host = parsed.host_str()?;
    match parsed.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}/")),
        None => Some(format!("{scheme}://{host}/")),
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::build_default_http_client;
    use crate::model::{AnimeVariant, StreamKind};

    #[tokio::test]
    async fn resolve_best_stream_falls_back_on_error() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/m3u8/0123456789abcdef0123456789abcdef");
            then.status(500).body("upstream error");
        });

        let mp4_url = format!("{}/d/token/video.mp4", server.base_url());
        server.mock(|when, then| {
            when.method(GET).path("/embed-abc123.html");
            then.status(200)
                .header("content-type", "text/html")
                .body(format!(r#"<script>player.src({{ type: "video/mp4", src: "{mp4_url}" }});</script>"#));
        });

        let client = build_default_http_client().unwrap();
        let registry = ExtractorRegistry::default_v1();

        let mirrors = vec![
            VideoMirror {
                server_name: "HLS".to_string(),
                embed_url: format!("{}/play/0123456789abcdef0123456789abcdef", server.base_url()),
                variant: AnimeVariant::Sub,
                referer: "https://animeav1.com/".to_string(),
                size: None,
            },
            VideoMirror {
                server_name: "MP4Upload".to_string(),
                embed_url: format!("{}/embed-abc123.html", server.base_url()),
                variant: AnimeVariant::Sub,
                referer: "https://animeav1.com/".to_string(),
                size: None,
            },
            VideoMirror {
                server_name: "UnsupportedHost".to_string(),
                embed_url: "https://example.invalid/embed/123".to_string(),
                variant: AnimeVariant::Sub,
                referer: "https://animeav1.com/".to_string(),
                size: None,
            },
        ];

        assert!(registry.supports_mirror(&mirrors[0]));
        assert!(registry.supports_mirror(&mirrors[1]));
        assert!(!registry.supports_mirror(&mirrors[2]));

        let resolved = registry.resolve_best_stream(&client, &mirrors).await.unwrap();
        assert_eq!(resolved.kind, StreamKind::Mp4);
        assert_eq!(resolved.stream_url, mp4_url);
        assert_eq!(resolved.server_name, "MP4Upload");
    }

    #[tokio::test]
    async fn resolve_best_stream_returns_unsupported_when_no_mirrors_match() {
        let client = build_default_http_client().unwrap();
        let registry = ExtractorRegistry::default_v1();

        let mirrors = vec![VideoMirror {
            server_name: "Mega".to_string(),
            embed_url: "https://mega.nz/file/123".to_string(),
            variant: AnimeVariant::Sub,
            referer: "https://animeav1.com/".to_string(),
            size: None,
        }];

        let err = registry.resolve_best_stream(&client, &mirrors).await.unwrap_err();
        assert!(matches!(err, AnimeError::UnsupportedMirror(_)));
    }
}
