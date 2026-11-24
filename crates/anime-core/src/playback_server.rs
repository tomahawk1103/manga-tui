mod proxy;

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tokio::task::{JoinHandle, JoinSet};

use crate::{AnimeEpisode, AnimeError, AnimeProvider, AnimeVariant, ExtractorRegistry, StreamKind, VideoMirror};

type Body = UnsyncBoxBody<Bytes, reqwest::Error>;
type Reply = Response<Body>;
type Mirrors = Pin<Box<dyn Future<Output = Result<Vec<VideoMirror>, AnimeError>> + Send>>;
type LoadMirrors = dyn Fn(String) -> Mirrors + Send + Sync;

const WEB_PLAYER: &str = include_str!("player/web_player.html");
const WEB_SCRIPT: &str = include_str!("player/web_player.js");
// hls.js 1.6.13, distributed under the Apache 2.0 license in player/LICENSE.
const HLS_SCRIPT: &str = include_str!("player/hls.min.js");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybackProgress {
    pub position_seconds: f64,
    pub duration_seconds: f64,
}

/// Optional durable history for a single anime. Calls run on Tokio's blocking pool.
pub trait PlaybackHistory: Send + Sync + 'static {
    fn load(&self) -> Result<HashMap<String, PlaybackProgress>, AnimeError>;
    fn save(&self, episode: &AnimeEpisode, progress: &PlaybackProgress) -> Result<(), AnimeError>;
}

struct Active {
    episode: usize,
    mirrors: Vec<VideoMirror>,
    mirror: usize,
    stream: Arc<proxy::Source>,
    generation: u64,
    progress: HashMap<usize, PlaybackProgress>,
}

struct Session {
    address: SocketAddr,
    token: String,
    title: String,
    cover_url: Option<String>,
    episodes: Vec<AnimeEpisode>,
    preferred_variant: AnimeVariant,
    load_mirrors: Box<LoadMirrors>,
    extractor: ExtractorRegistry,
    client: reqwest::Client,
    active: RwLock<Active>,
    switch: Mutex<()>,
    history: Option<Arc<dyn PlaybackHistory>>,
}

#[derive(Deserialize)]
struct EpisodeChoice {
    episode: usize,
}

#[derive(Deserialize)]
struct MirrorChoice {
    generation: u64,
    mirror: usize,
}

#[derive(Deserialize)]
struct ProgressUpdate {
    generation: u64,
    episode_number: String,
    #[serde(flatten)]
    progress: PlaybackProgress,
}

pub struct AnimePlaybackServer {
    session: Arc<Session>,
    task: JoinHandle<()>,
}

impl AnimePlaybackServer {
    /// Starts a playback session for one anime. Keep this handle alive while a player is connected.
    pub async fn start(
        provider: Arc<impl AnimeProvider + 'static>,
        details: &crate::AnimeDetails,
        episodes: Vec<AnimeEpisode>,
        episode: usize,
        preferred_variant: AnimeVariant,
        preferred_mirror: Option<&VideoMirror>,
    ) -> Result<Self, AnimeError> {
        Self::start_with_history(provider, details, episodes, episode, preferred_variant, preferred_mirror, None).await
    }

    pub async fn start_with_history(
        provider: Arc<impl AnimeProvider + 'static>,
        details: &crate::AnimeDetails,
        episodes: Vec<AnimeEpisode>,
        episode: usize,
        preferred_variant: AnimeVariant,
        preferred_mirror: Option<&VideoMirror>,
        history: Option<Arc<dyn PlaybackHistory>>,
    ) -> Result<Self, AnimeError> {
        let slug = details.slug.clone();
        let load_mirrors: Box<LoadMirrors> = Box::new(move |number| {
            let provider = provider.clone();
            let slug = slug.clone();
            Box::pin(async move { provider.get_episode_mirrors(&slug, &number).await })
        });
        let first = episodes.get(episode).ok_or_else(|| AnimeError::InvalidInput("Invalid episode".into()))?;
        let extractor = ExtractorRegistry::default();
        let client = proxy::client()?;
        let mut mirrors = load_mirrors(first.number.clone()).await?;
        mirrors.retain(|mirror| extractor.supports_mirror(mirror));
        mirrors.sort_by_key(|mirror| (Some(mirror) != preferred_mirror, mirror.variant != preferred_variant));
        let (mirror, stream) = select_source(&extractor, &client, &mirrors).await?;
        let progress = if let Some(store) = history.clone() {
            let saved = tokio::task::spawn_blocking(move || store.load())
                .await
                .map_err(|error| AnimeError::Persistence(error.to_string()))??;
            episodes
                .iter()
                .enumerate()
                .filter_map(|(index, episode)| saved.get(&episode.number).cloned().map(|progress| (index, progress)))
                .collect()
        } else {
            HashMap::new()
        };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.map_err(server_error)?;
        let session = Arc::new(Session {
            address: listener.local_addr().map_err(server_error)?,
            token: format!("{:032x}", rand::random::<u128>()),
            title: details.title.clone(),
            cover_url: details.cover_url.clone(),
            episodes,
            preferred_variant,
            load_mirrors,
            extractor,
            client,
            active: RwLock::new(Active {
                episode,
                mirrors,
                mirror,
                stream,
                generation: 0,
                progress,
            }),
            switch: Mutex::new(()),
            history,
        });
        Ok(Self::serve(listener, session))
    }

    fn serve(listener: TcpListener, session: Arc<Session>) -> Self {
        let state = session.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    Some(_) = connections.join_next() => {}
                    result = listener.accept(), if connections.len() < 128 => {
                        let Ok((socket, _)) = result else { break };
                        let state = state.clone();
                        connections.spawn(async move {
                            let service = service_fn(move |request| {
                                let state = state.clone();
                                async move { Ok::<_, std::convert::Infallible>(state.respond(request).await) }
                            });
                            let _ = http1::Builder::new()
                                .timer(TokioTimer::new())
                                .header_read_timeout(Duration::from_secs(10))
                                .serve_connection(TokioIo::new(socket), service)
                                .await;
                        });
                    }
                }
            }
        });
        Self { session, task }
    }

    pub fn watch_url(&self) -> String {
        format!("http://{}/watch/{}", self.session.address, self.session.token)
    }

    pub fn open_web(&self) -> io::Result<()> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            return open::that_detached(self.watch_url());
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            Err(io::Error::new(io::ErrorKind::Unsupported, "opening external browser is not supported on mobile targets"))
        }
    }

    pub async fn stream_url(&self) -> String {
        self.external_media().await.0
    }

    pub(crate) async fn external_media(&self) -> (String, String) {
        let active = self.session.active.read().await;
        (
            format!("http://{}{}", self.session.address, self.session.stream_path(&active)),
            format!("{} - Ep. {}", self.session.title, self.session.episodes[active.episode].number),
        )
    }

    pub async fn progress(&self, episode: usize) -> Option<PlaybackProgress> {
        self.session.active.read().await.progress.get(&episode).cloned()
    }
}

impl Drop for AnimePlaybackServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn select_source(
    extractor: &ExtractorRegistry,
    client: &reqwest::Client,
    mirrors: &[VideoMirror],
) -> Result<(usize, Arc<proxy::Source>), AnimeError> {
    let mut last_error = AnimeError::UnsupportedMirror("No supported mirrors for this episode".into());
    for (index, mirror) in mirrors.iter().enumerate() {
        let result = resolve_source(extractor, client, mirror).await;
        match result {
            Ok(stream) => return Ok((index, Arc::new(stream))),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

async fn resolve_source(
    extractor: &ExtractorRegistry,
    client: &reqwest::Client,
    mirror: &VideoMirror,
) -> Result<proxy::Source, AnimeError> {
    if !reqwest::Url::parse(&mirror.embed_url).is_ok_and(|url| proxy::valid_url(&url)) {
        return Err(AnimeError::InvalidInput("Invalid upstream embed URL".into()));
    }
    tokio::time::timeout(Duration::from_secs(25), async {
        let source = proxy::Source::new(extractor.extract_mirror(client, mirror).await?)?;
        source.probe(client).await?;
        Ok(source)
    })
    .await
    .map_err(|_| AnimeError::ExtractionFailed("Server took too long to resolve".into()))?
}

impl Session {
    fn stream_path(&self, active: &Active) -> String {
        let file = match active.stream.kind {
            StreamKind::Hls => "master.m3u8",
            StreamKind::Mp4 => "video.mp4",
        };
        format!("/stream/{}/{file}?g={}", self.token, active.generation)
    }

    async fn snapshot(&self) -> Reply {
        let active = self.active.read().await;
        let mirrors: Vec<_> = active
            .mirrors
            .iter()
            .map(|mirror| serde_json::json!({ "name": mirror.server_name, "variant": mirror.variant.to_string() }))
            .collect();
        json_reply(&serde_json::json!({
            "title": self.title,
            "cover_url": self.cover_url,
            "episodes": self.episodes,
            "episode": active.episode,
            "mirrors": mirrors,
            "mirror": active.mirror,
            "generation": active.generation,
            "kind": active.stream.kind,
            "stream": self.stream_path(&active),
            "progress": active.progress.get(&active.episode).cloned().unwrap_or_default(),
        }))
    }

    async fn respond(&self, request: Request<Incoming>) -> Reply {
        let authority = self.address.to_string();
        if request.headers().get(header::HOST).and_then(|host| host.to_str().ok()) != Some(&authority) {
            return empty(StatusCode::MISDIRECTED_REQUEST);
        }

        let mut response = self.route(request).await;
        let headers = response.headers_mut();
        headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
        headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; media-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'".parse().unwrap(),
        );
        response
    }

    async fn route(&self, request: Request<Incoming>) -> Reply {
        let path = request.uri().path();
        let mut parts = path.strip_prefix('/').unwrap_or_default().split('/');
        let route = parts.next().unwrap_or_default();
        if parts.next() != Some(self.token.as_str()) {
            return empty(StatusCode::NOT_FOUND);
        }
        let file = parts.next();
        if parts.next().is_some() {
            return empty(StatusCode::NOT_FOUND);
        }

        match (request.method(), route, file) {
            (&Method::GET, "watch", None) => content("text/html; charset=utf-8", WEB_PLAYER.replace("__TOKEN__", &self.token)),
            (&Method::GET, "assets", Some("hls.min.js")) => content("text/javascript", HLS_SCRIPT),
            (&Method::GET, "assets", Some("player.js")) => content("text/javascript", WEB_SCRIPT),
            (&Method::GET, "api", Some("state")) => self.snapshot().await,
            (&Method::POST, "api", Some(action)) => self.update(action.to_owned(), request).await,
            (&Method::GET | &Method::HEAD, "stream" | "proxy", Some(file)) => {
                let query = request.uri().query().unwrap_or_default();
                let active = self.active.read().await;
                let generation = active.generation.to_string();
                if !query.split('&').any(|part| part.strip_prefix("g=") == Some(&generation)) {
                    return empty(StatusCode::NOT_FOUND);
                }
                let stream = Arc::clone(&active.stream);
                drop(active);
                let resource = match (route, file, stream.kind) {
                    ("stream", "master.m3u8", StreamKind::Hls) | ("stream", "video.mp4", StreamKind::Mp4) => Some(stream.root()),
                    ("proxy", "seg", StreamKind::Hls) => query
                        .split('&')
                        .find_map(|part| part.strip_prefix("u="))
                        .and_then(|id| id.parse().ok())
                        .and_then(|id| stream.resource(id)),
                    _ => None,
                };
                match resource {
                    Some(resource) => {
                        stream
                            .forward(&self.client, request.method(), request.headers(), resource, &self.token, &generation)
                            .await
                    },
                    None => empty(StatusCode::NOT_FOUND),
                }
            },
            _ => empty(StatusCode::NOT_FOUND),
        }
    }

    async fn update(&self, action: String, request: Request<Incoming>) -> Reply {
        let origin = format!("http://{}", self.address);
        let headers = request.headers();
        if headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value != origin)
            || headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()) != Some("application/json")
        {
            return empty(StatusCode::FORBIDDEN);
        }

        let body = match tokio::time::timeout(Duration::from_secs(5), Limited::new(request.into_body(), 4096).collect()).await {
            Ok(Ok(body)) => body,
            Ok(Err(_)) => return empty(StatusCode::PAYLOAD_TOO_LARGE),
            Err(_) => return empty(StatusCode::REQUEST_TIMEOUT),
        };
        let bytes = body.to_bytes();

        match action.as_str() {
            "episode" => {
                let Ok(EpisodeChoice { episode }) = serde_json::from_slice(&bytes) else {
                    return empty(StatusCode::BAD_REQUEST);
                };
                let Some(selected) = self.episodes.get(episode) else {
                    return empty(StatusCode::BAD_REQUEST);
                };
                let _switch = self.switch.lock().await;
                if self.active.read().await.episode == episode {
                    return self.snapshot().await;
                }

                let Ok(mut mirrors) = (self.load_mirrors)(selected.number.clone()).await else {
                    return empty(StatusCode::BAD_GATEWAY);
                };
                mirrors.retain(|mirror| self.extractor.supports_mirror(mirror));
                mirrors.sort_by_key(|mirror| mirror.variant != self.preferred_variant);
                let Ok((mirror, stream)) = select_source(&self.extractor, &self.client, &mirrors).await else {
                    return empty(StatusCode::BAD_GATEWAY);
                };
                let mut active = self.active.write().await;
                active.episode = episode;
                active.mirrors = mirrors;
                active.mirror = mirror;
                active.stream = stream;
                active.generation += 1;
            },
            "mirror" => {
                let Ok(MirrorChoice { generation, mirror }) = serde_json::from_slice(&bytes) else {
                    return empty(StatusCode::BAD_REQUEST);
                };
                let _switch = self.switch.lock().await;
                let active = self.active.read().await;
                if generation != active.generation {
                    return empty(StatusCode::CONFLICT);
                }
                let Some(selected) = active.mirrors.get(mirror).cloned() else {
                    return empty(StatusCode::BAD_REQUEST);
                };
                drop(active);
                let result = resolve_source(&self.extractor, &self.client, &selected).await;
                let Ok(stream) = result else {
                    return empty(StatusCode::BAD_GATEWAY);
                };
                let mut active = self.active.write().await;
                active.mirror = mirror;
                active.stream = Arc::new(stream);
                active.generation += 1;
            },
            "progress" => {
                let Ok(update) = serde_json::from_slice::<ProgressUpdate>(&bytes) else {
                    return empty(StatusCode::BAD_REQUEST);
                };
                let mut active = self.active.write().await;
                let episode = active.episode;
                if update.generation != active.generation || update.episode_number != self.episodes[episode].number {
                    return empty(StatusCode::CONFLICT);
                }
                let progress = update.progress;
                if progress.duration_seconds <= 0.0
                    || progress.position_seconds < 0.0
                    || progress.position_seconds > progress.duration_seconds
                {
                    return empty(StatusCode::BAD_REQUEST);
                }
                active.progress.insert(episode, progress.clone());
                if let Some(history) = self.history.clone() {
                    let episode = self.episodes[episode].clone();
                    let persisted = tokio::task::spawn_blocking(move || history.save(&episode, &progress)).await;
                    if !matches!(persisted, Ok(Ok(()))) {
                        log::warn!("Could not save anime playback progress: {persisted:?}");
                        return empty(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
                return empty(StatusCode::NO_CONTENT);
            },
            _ => return empty(StatusCode::NOT_FOUND),
        }
        self.snapshot().await
    }
}

fn server_error(error: io::Error) -> AnimeError {
    AnimeError::Http {
        status: None,
        message: error.to_string(),
    }
}

fn json_reply(value: &serde_json::Value) -> Reply {
    content("application/json", value.to_string())
}

fn body(data: impl Into<Bytes>) -> Body {
    Full::new(data.into()).map_err(|error| match error {}).boxed_unsync()
}

fn empty(status: StatusCode) -> Reply {
    Response::builder().status(status).body(body(Bytes::new())).unwrap()
}

fn content(content_type: &'static str, data: impl Into<Bytes>) -> Reply {
    Response::builder().header(header::CONTENT_TYPE, content_type).body(body(data)).unwrap()
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;

    use super::*;

    #[derive(Default)]
    struct TestHistory(std::sync::Mutex<HashMap<String, PlaybackProgress>>);

    impl PlaybackHistory for TestHistory {
        fn load(&self) -> Result<HashMap<String, PlaybackProgress>, AnimeError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, episode: &AnimeEpisode, progress: &PlaybackProgress) -> Result<(), AnimeError> {
            self.0.lock().unwrap().insert(episode.number.clone(), progress.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn serves_local_player_and_proxies_only_resources_from_the_active_playlist() {
        let upstream = MockServer::start_async().await;
        let host = "media.example.test";
        let manifest = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/catalog/index").header("referer", "https://example.test/");
                then.status(200).body("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=10000\nchild\n");
            })
            .await;
        let child = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/catalog/child");
                then.status(200).body(concat!(
                    "#EXTM3U\n#EXT-X-MAP:URI=\"init.html\"\n",
                    "#EXT-X-KEY:METHOD=AES-128,URI=\"/key\"\n",
                    "#EXTINF:4,\n/disguised.html\n#EXT-X-ENDLIST\n",
                ));
            })
            .await;
        let denied = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/catalog/init.html");
                then.status(403).body("upstream denial page");
            })
            .await;
        let segment = upstream
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/disguised.html")
                    .header("range", "bytes=2-4")
                    .header("referer", "https://example.test/");
                then.status(206)
                    .header("content-range", "bytes 2-4/10")
                    .header("accept-ranges", "bytes")
                    .header("content-type", "text/html")
                    .body("abc");
            })
            .await;
        let embed = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/embed-test.html");
                then.status(200)
                    .body(format!(r#"<script>player.src({{src:"http://{host}/media.mp4"}})</script>"#));
            })
            .await;
        let mp4 = upstream
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/media.mp4")
                    .header("range", "bytes=1-2")
                    .header("referer", format!("http://{host}/"));
                then.status(206).header("content-range", "bytes 1-2/10").body("bc");
            })
            .await;
        let probe = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/media.mp4").header("range", "bytes=0-511");
                then.status(206).body("media");
            })
            .await;
        let proxy_client = reqwest::Client::builder().no_proxy().resolve(host, *upstream.address()).build().unwrap();
        let stream = proxy::Source::new(crate::ResolvedStream {
            stream_url: format!("http://{host}/catalog/index"),
            headers: vec![("referer".into(), "https://example.test/".into())],
            kind: StreamKind::Hls,
            server_name: "Test".into(),
            quality: None,
        })
        .unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let history = Arc::new(TestHistory::default());
        let server = AnimePlaybackServer::serve(
            listener,
            Arc::new(Session {
                address,
                token: "test-token".into(),
                title: "Test".into(),
                cover_url: None,
                episodes: ["1.5", "2"]
                    .into_iter()
                    .map(|number| AnimeEpisode {
                        id: None,
                        number: number.into(),
                        title: None,
                        thumbnail_url: None,
                        aired_at: None,
                    })
                    .collect(),
                preferred_variant: AnimeVariant::Sub,
                load_mirrors: Box::new(move |_| {
                    Box::pin(async move {
                        Ok(vec![VideoMirror {
                            server_name: "MP4Upload".into(),
                            embed_url: format!("http://{host}/embed-test.html"),
                            variant: AnimeVariant::Sub,
                            referer: format!("http://{host}/"),
                            size: None,
                        }])
                    })
                }),
                extractor: ExtractorRegistry::default(),
                client: proxy_client,
                active: RwLock::new(Active {
                    episode: 0,
                    mirrors: vec![],
                    mirror: 0,
                    stream: Arc::new(stream),
                    generation: 0,
                    progress: HashMap::new(),
                }),
                switch: Mutex::new(()),
                history: Some(history.clone()),
            }),
        );
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let origin = format!("http://{address}");
        let player = client.get(server.watch_url()).send().await.unwrap().text().await.unwrap();
        assert!(player.contains("/assets/test-token/hls.min.js"));
        let root = client.get(server.stream_url().await).send().await.unwrap().text().await.unwrap();
        let playlist_path = root.lines().find(|line| line.starts_with("/proxy/")).unwrap();
        let playlist = client.get(format!("{origin}{playlist_path}")).send().await.unwrap().text().await.unwrap();
        assert!(playlist.contains("#EXT-X-MAP:URI=\"/proxy/test-token/seg?g=0&u=1\""));
        assert!(playlist.contains("#EXT-X-KEY:METHOD=AES-128,URI=\"/proxy/test-token/seg?g=0&u=2\""));
        let denied_path = format!("{origin}/proxy/test-token/seg?g=0&u=1");
        let mut denial = client.get(denied_path).send().await.unwrap();
        assert_eq!(denial.status(), StatusCode::FORBIDDEN);
        assert!(denial.chunk().await.unwrap().is_none());
        let segment_path = playlist.lines().find(|line| line.starts_with("/proxy/")).unwrap();
        let reply = client
            .get(format!("{origin}{segment_path}"))
            .header(header::RANGE, "bytes=2-4")
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(reply.headers()[header::CONTENT_RANGE], "bytes 2-4/10");
        assert_eq!(reply.headers()[header::CONTENT_LENGTH], "3");
        assert_eq!(reply.headers()[header::ACCEPT_RANGES], "bytes");
        assert_eq!(reply.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(reply.text().await.unwrap(), "abc");

        for path in ["/proxy/test-token/seg?g=0&u=999", "/proxy/test-token/seg?g=1&u=3", "/proxy/wrong/seg?g=0&u=3"] {
            let reply = client.get(format!("{origin}{path}")).send().await.unwrap();
            assert_eq!(reply.status(), StatusCode::NOT_FOUND);
        }
        let progress = serde_json::json!({
            "generation": 0, "episode_number": "1.5", "position_seconds": 12.5, "duration_seconds": 100
        });
        let endpoint = format!("{origin}/api/test-token/progress");
        let reply = client
            .post(&endpoint)
            .header(header::ORIGIN, "https://unrelated.test")
            .json(&progress)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::FORBIDDEN);
        let reply = client
            .post(&endpoint)
            .header(header::ORIGIN, &origin)
            .json(&progress)
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::NO_CONTENT);
        assert_eq!(server.progress(0).await.unwrap().position_seconds, 12.5);
        assert_eq!(history.load().unwrap()["1.5"].position_seconds, 12.5);

        let changed: serde_json::Value = client
            .post(format!("{origin}/api/test-token/episode"))
            .json(&serde_json::json!({ "episode": 1 }))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(changed["kind"], "Mp4");
        assert_eq!(changed["episode"], 1);
        assert_eq!(client.get(format!("{origin}{segment_path}")).send().await.unwrap().status(), StatusCode::NOT_FOUND);
        let reply = client
            .get(server.stream_url().await)
            .header(header::RANGE, "bytes=1-2")
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(reply.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(reply.text().await.unwrap(), "bc");
        assert_eq!(client.post(&endpoint).json(&progress).send().await.unwrap().status(), StatusCode::CONFLICT);

        manifest.assert_async().await;
        child.assert_async().await;
        denied.assert_async().await;
        segment.assert_async().await;
        embed.assert_async().await;
        mp4.assert_async().await;
        probe.assert_async().await;
    }

    #[tokio::test]
    async fn skips_hls_mirror_when_manifest_works_but_initialization_is_forbidden() {
        let upstream = MockServer::start_async().await;
        let host = "media.example.test";
        let hash = "abcdefabcdefabcdefabcdefabcdefab";
        let playlist = upstream
            .mock_async(|when, then| {
                when.method(GET).path(format!("/m3u8/{hash}"));
                then.status(200).body("#EXTM3U\n#EXT-X-MAP:URI=\"/init.html\"\n#EXTINF:4,\n/video.html\n");
            })
            .await;
        let denied = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/init.html");
                then.status(403);
            })
            .await;
        let embed = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/embed-good.html");
                then.status(200).body(format!(r#"player.src({{src:"http://{host}/good.mp4"}})"#));
            })
            .await;
        let media = upstream
            .mock_async(|when, then| {
                when.method(GET).path("/good.mp4").header("range", "bytes=0-511");
                then.status(206).body("media");
            })
            .await;
        let mirrors = [("HLS", format!("play/{hash}")), ("MP4Upload", "embed-good.html".into())].map(|(name, path)| VideoMirror {
            server_name: name.into(),
            embed_url: format!("http://{host}/{path}"),
            variant: AnimeVariant::Sub,
            referer: format!("http://{host}/"),
            size: None,
        });
        let client = reqwest::Client::builder().no_proxy().resolve(host, *upstream.address()).build().unwrap();
        let (index, source) = select_source(&ExtractorRegistry::default(), &client, &mirrors).await.unwrap();
        assert_eq!(index, 1);
        assert_eq!(source.kind, StreamKind::Mp4);
        assert_eq!(playlist.hits_async().await, 2);
        denied.assert_async().await;
        embed.assert_async().await;
        media.assert_async().await;
    }
}
