use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use bytes::Bytes;
use futures_util::TryStreamExt;
use http::{HeaderMap, Method, Response, StatusCode, header};
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use regex::Regex;
use reqwest::Url;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::{Reply, body, content, empty};
use crate::{AnimeError, ResolvedStream, StreamKind};

const HLS_TYPE: &str = "application/vnd.apple.mpegurl";
const MAX_PLAYLIST_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESOURCES: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Playlist,
    Rendition,
    Init,
    Segment,
    Key,
    Mp4,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Resource {
    url: Url,
    kind: Kind,
}

#[derive(Default)]
struct Resources {
    by_id: Vec<Resource>,
    ids: HashMap<Resource, usize>,
}

pub(super) struct Source {
    pub kind: StreamKind,
    root: Resource,
    headers: HeaderMap,
    resources: Mutex<Resources>,
}

impl Source {
    pub(super) fn new(stream: ResolvedStream) -> Result<Self, AnimeError> {
        let invalid = || AnimeError::InvalidInput("Invalid upstream media URL or headers".into());
        let url = Url::parse(&stream.stream_url).map_err(|_| invalid())?;
        if !valid_url(&url) {
            return Err(invalid());
        }
        let mut headers = HeaderMap::new();
        for (name, value) in stream.headers {
            let normalized = name.to_ascii_lowercase();
            if matches!(normalized.as_str(), "referer" | "origin" | "user-agent") {
                headers.insert(
                    header::HeaderName::from_bytes(normalized.as_bytes()).map_err(|_| invalid())?,
                    value.parse().map_err(|_| invalid())?,
                );
            }
        }

        Ok(Self {
            kind: stream.kind,
            root: Resource {
                url,
                kind: match stream.kind {
                    StreamKind::Hls => Kind::Playlist,
                    StreamKind::Mp4 => Kind::Mp4,
                },
            },
            headers,
            resources: Mutex::default(),
        })
    }

    pub(super) fn root(&self) -> Resource {
        self.root.clone()
    }

    pub(super) fn resource(&self, id: usize) -> Option<Resource> {
        self.resources.lock().unwrap().by_id.get(id).cloned()
    }

    fn request(&self, client: &reqwest::Client, method: &Method, resource: &Resource) -> reqwest::RequestBuilder {
        client
            .request(method.clone(), resource.url.clone())
            .headers(self.headers.clone())
            .header(header::ACCEPT_ENCODING, "identity")
    }

    /// Check that the server delivers media, including the initialization segment for fMP4.
    pub(super) async fn probe(&self, client: &reqwest::Client) -> Result<(), AnimeError> {
        let mut current = self.root();
        for _ in 0..4 {
            if current.kind == Kind::Mp4 {
                return self.probe_media(client, &current).await;
            }
            let upstream = self.request(client, &Method::GET, &current).send().await?.error_for_status()?;
            let (base, text) = read_playlist(upstream).await?;
            let mut resources = Vec::new();
            rewrite_playlist(&text, &base, |resource| {
                resources.push(resource);
                Some(String::new())
            })
            .ok_or_else(|| AnimeError::Parse("Invalid HLS playlist".into()))?;
            let mut stream = None;
            for resource in resources {
                match resource.kind {
                    Kind::Playlist | Kind::Segment => {
                        stream = Some(resource);
                        break;
                    },
                    Kind::Init | Kind::Key => self.probe_media(client, &resource).await?,
                    Kind::Rendition | Kind::Mp4 => {},
                }
            }
            current = stream.ok_or_else(|| AnimeError::Parse("HLS playlist contains no media".into()))?;
            if current.kind == Kind::Segment {
                return self.probe_media(client, &current).await;
            }
        }
        Err(AnimeError::Parse("Too many nested HLS playlists".into()))
    }

    async fn probe_media(&self, client: &reqwest::Client, resource: &Resource) -> Result<(), AnimeError> {
        let mut upstream = self
            .request(client, &Method::GET, resource)
            .header(header::RANGE, "bytes=0-511")
            .send()
            .await?
            .error_for_status()?;
        let chunk = upstream
            .chunk()
            .await?
            .filter(|data| !data.is_empty())
            .ok_or_else(|| AnimeError::Parse("Empty upstream media".into()))?;
        let prefix = String::from_utf8_lossy(&chunk[..chunk.len().min(512)]).to_ascii_lowercase();
        if ["<!doctype html", "<html", "<head", "<body"]
            .iter()
            .any(|tag| prefix.trim_start().starts_with(tag))
        {
            return Err(AnimeError::Parse("Expected media but received HTML".into()));
        }
        Ok(())
    }

    pub(super) async fn forward(
        &self,
        client: &reqwest::Client,
        method: &Method,
        headers: &HeaderMap,
        resource: Resource,
        token: &str,
        generation: &str,
    ) -> Reply {
        let playlist = matches!(resource.kind, Kind::Playlist | Kind::Rendition);
        let mut request = self.request(client, method, &resource);
        if !playlist {
            for name in [header::RANGE, header::IF_RANGE] {
                if let Some(value) = headers.get(&name) {
                    request = request.header(name, value);
                }
            }
        }
        let Ok(upstream) = request.send().await else {
            return empty(StatusCode::BAD_GATEWAY);
        };

        let status = upstream.status();
        if status.is_client_error() && status != StatusCode::RANGE_NOT_SATISFIABLE {
            // Keep the upstream status so playback clients can distinguish denials from transient failures.
            return empty(status);
        }
        if !matches!(status, StatusCode::OK | StatusCode::PARTIAL_CONTENT | StatusCode::RANGE_NOT_SATISFIABLE) {
            return empty(StatusCode::BAD_GATEWAY);
        }
        let media_type = upstream
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let playlist = playlist || (resource.kind == Kind::Segment && media_type.to_ascii_lowercase().contains("mpegurl"));
        if playlist && *method == Method::HEAD {
            return content(HLS_TYPE, Bytes::new());
        }
        if playlist {
            if status != StatusCode::OK {
                return empty(StatusCode::BAD_GATEWAY);
            }
            return match read_playlist(upstream)
                .await
                .ok()
                .and_then(|(base, text)| self.rewrite(&text, &base, token, generation))
            {
                Some(text) => content(HLS_TYPE, text),
                None => empty(StatusCode::BAD_GATEWAY),
            };
        }

        let mut response = Response::builder().status(status);
        for name in [header::ACCEPT_RANGES, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ETAG, header::LAST_MODIFIED] {
            if let Some(value) = upstream.headers().get(&name) {
                response = response.header(name, value);
            }
        }
        let media_type = match resource.kind {
            Kind::Mp4 => "video/mp4",
            Kind::Key => "application/octet-stream",
            Kind::Segment | Kind::Init
                if resource.url.path().to_ascii_lowercase().ends_with(".html")
                    || upstream.url().path().to_ascii_lowercase().ends_with(".html") =>
            {
                "video/mp4"
            },
            _ if media_type.starts_with("video/") || media_type.starts_with("audio/") || media_type.starts_with("text/vtt") => {
                media_type
            },
            _ => "application/octet-stream",
        };
        response = response.header(header::CONTENT_TYPE, media_type);
        let payload = if *method == Method::HEAD {
            body(Bytes::new())
        } else {
            StreamBody::new(upstream.bytes_stream().map_ok(Frame::data)).boxed_unsync()
        };
        response.body(payload).unwrap()
    }

    fn rewrite(&self, playlist: &str, base: &Url, token: &str, generation: &str) -> Option<String> {
        rewrite_playlist(playlist, base, |resource| {
            let mut resources = self.resources.lock().unwrap();
            let id = match resources.ids.get(&resource) {
                Some(id) => *id,
                None if resources.by_id.len() < MAX_RESOURCES => {
                    let id = resources.by_id.len();
                    resources.by_id.push(resource.clone());
                    resources.ids.insert(resource, id);
                    id
                },
                None => return None,
            };
            Some(format!("/proxy/{token}/seg?g={generation}&u={id}"))
        })
    }
}

async fn read_playlist(mut response: reqwest::Response) -> Result<(Url, String), AnimeError> {
    let base = response.url().clone();
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if data.len() + chunk.len() > MAX_PLAYLIST_BYTES {
            return Err(AnimeError::Parse("HLS playlist exceeds size limit".into()));
        }
        data.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(data).map_err(|_| AnimeError::Parse("HLS playlist is not UTF-8".into()))?;
    Ok((base, text))
}

fn rewrite_playlist(playlist: &str, base: &Url, mut register: impl FnMut(Resource) -> Option<String>) -> Option<String> {
    // Match whole attribute values so a quoted value containing ",URI=" cannot be mistaken for an attribute.
    static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?:^|,)\s*([A-Z0-9-]+)\s*=\s*("[^"]*"|[^,]*)"#).unwrap());
    let playlist = playlist.strip_prefix('\u{feff}').unwrap_or(playlist);
    if playlist.lines().next() != Some("#EXTM3U") {
        return None;
    }

    let mut local_url = |reference: &str, mut kind: Kind| {
        let mut url = base.join(reference).ok()?;
        if !valid_url(&url) || reference.contains("{$") {
            return None;
        }
        url.set_fragment(None);
        if kind == Kind::Segment && url.path().to_ascii_lowercase().ends_with(".m3u8") {
            kind = Kind::Playlist;
        }
        register(Resource { url, kind })
    };

    let mut output = String::new();
    let mut next_variant = false;
    for line in playlist.lines().map(str::trim) {
        if !line.is_empty() && !line.starts_with('#') {
            let kind = if next_variant { Kind::Playlist } else { Kind::Segment };
            output.push_str(&local_url(line, kind)?);
            next_variant = false;
        } else if let Some((tag, attributes)) = line.split_once(':').filter(|(tag, _)| tag.starts_with("#EXT-X-")) {
            if tag == "#EXT-X-CONTENT-STEERING" {
                continue;
            }
            next_variant |= tag == "#EXT-X-STREAM-INF";
            output.push_str(tag);
            output.push(':');
            let mut end = 0;
            for captures in ATTRIBUTE.captures_iter(attributes) {
                if &captures[1] != "URI" {
                    continue;
                }
                let value = captures.get(2)?;
                let uri = value.as_str().strip_prefix('"')?.strip_suffix('"')?;
                let kind = match tag {
                    "#EXT-X-KEY" | "#EXT-X-SESSION-KEY" => Kind::Key,
                    "#EXT-X-MEDIA" | "#EXT-X-I-FRAME-STREAM-INF" | "#EXT-X-RENDITION-REPORT" => Kind::Rendition,
                    "#EXT-X-MAP" => Kind::Init,
                    _ => Kind::Segment,
                };
                output.push_str(&attributes[end..value.start()]);
                output.push('"');
                output.push_str(&local_url(uri, kind)?);
                output.push('"');
                end = value.end();
            }
            output.push_str(&attributes[end..]);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    Some(output)
}

pub(super) fn client() -> Result<reqwest::Client, AnimeError> {
    reqwest::Client::builder()
        .no_proxy()
        .referer(false)
        .user_agent(crate::DEFAULT_USER_AGENT)
        .no_gzip()
        .no_deflate()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(20))
        .dns_resolver(Arc::new(PublicDns))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 10 && valid_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("Untrusted upstream redirect")
            }
        }))
        .build()
        .map_err(Into::into)
}

struct PublicDns;

impl Resolve for PublicDns {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|address| public_ip(address.ip()))
                .collect();
            if addresses.is_empty() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "No public upstream address").into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

pub(super) fn valid_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && matches!(url.port_or_known_default(), Some(80 | 443))
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            let host = host.trim_matches(['[', ']']);
            host.parse::<IpAddr>()
                .map_or_else(|_| host.contains('.') && !host.trim_end_matches('.').ends_with(".localhost"), public_ip)
        })
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => !matches!(
            ip.octets(),
            [0 | 10 | 127 | 224..=255, ..]
                | [100, 64..=127, ..]
                | [169, 254, ..]
                | [172, 16..=31, ..]
                | [192, 0, 0 | 2, _]
                | [192, 88, 99, _]
                | [192, 168, ..]
                | [198, 18 | 19, ..]
                | [198, 51, 100, _]
                | [203, 0, 113, _]
        ),
        IpAddr::V6(ip) => {
            matches!(ip.segments(), [0x2000..=0x3fff, ..])
                && !matches!(ip.segments(), [0x2001, 0..=0x01ff | 0x0db8, ..] | [0x2002 | 0x3fff, ..])
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blocks_private_destinations_and_dns_answers() {
        for target in [
            "http://127.1/x",
            "http://169.254.169.254/x",
            "http://192.168.1.1/x",
            "http://[::ffff:127.0.0.1]/x",
            "http://[fd00::1]/x",
            "file:///secret",
            "https://cdn.example.test:8888/x",
            "https://user:password@cdn.example.test/x",
        ] {
            assert!(!valid_url(&Url::parse(target).unwrap()), "{target}");
        }
        assert!(valid_url(&Url::parse("https://cdn.example.test/stream").unwrap()));
        assert!(PublicDns.resolve("localhost".parse().unwrap()).await.is_err());
    }
}
