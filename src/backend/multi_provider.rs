use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use image::DynamicImage;
use manga_tui::SearchTerm;
use reqwest::Url;

use crate::backend::cache::Cacher;
use crate::backend::cache::in_memory::InMemoryCache;
use crate::backend::database::ChapterBookmarked;
use crate::backend::manga_provider::comizy::ComizyProvider;
use crate::backend::manga_provider::comizy::filter_state::ComizyFilterState;
use crate::backend::manga_provider::inmanga::InmangaProvider;
use crate::backend::manga_provider::inmanga::filter_state::InmangaFilterState;
use crate::backend::manga_provider::leercapitulo::LeercapituloProvider;
use crate::backend::manga_provider::leercapitulo::filter_state::LeercapituloFilterState;
use crate::backend::manga_provider::mangadex::filters::api_parameter::{ContentRating, Filters, SortBy};
use crate::backend::manga_provider::mangadex::{API_URL_BASE, COVER_IMG_URL_BASE, MangadexClient};
use crate::backend::manga_provider::mangaoni::MangaoniProvider;
use crate::backend::manga_provider::mangaoni::filter_state::MangaoniFilterState;
use crate::backend::manga_provider::mangapill::MangaPillProvider;
use crate::backend::manga_provider::mangapill::filter_state::MangaPillFilterState;
use crate::backend::manga_provider::weebcentral::filter_state::WeebcentralFilterState;
use crate::backend::manga_provider::weebcentral::{WEEBCENTRAL_BASE_URL, WeebcentralProvider};
use crate::backend::manga_provider::{
    Chapter, ChapterFilters, ChapterPage, ChapterToRead, DecodeBytesToImage, FeedPageProvider, FetchChapterBookmarked,
    GetChapterPages, GetChaptersResponse, GetMangasResponse, GetRawImage, GoToReadChapter, HomePageMangaProvider, Languages,
    LatestChapter, ListOfChapters, Manga, MangaPageProvider, MangaPanel, MangaProviders, Pagination, PopularManga, ProviderError,
    RecentlyAddedManga, SearchChapterById, SearchManga, SearchMangaById, SearchMangaPanel, SearchPageProvider,
};
use crate::config::{ImageQuality, MangaTuiConfig};

const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderHealthStatus {
    #[default]
    Healthy,
    Degraded,
    CircuitOpen,
}

impl ProviderHealthStatus {
    pub fn indicator(&self) -> &'static str {
        match self {
            Self::Healthy => "🟢",
            Self::Degraded => "🟡",
            Self::CircuitOpen => "🔴",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProviderHealthRecord {
    pub consecutive_failures: u32,
    pub total_successes: u64,
    pub total_failures: u64,
    pub circuit_opened_at: Option<Instant>,
    pub last_error: Option<ProviderError>,
}

#[derive(Debug, Default)]
pub struct ProviderHealthTracker {
    records: Mutex<HashMap<MangaProviders, ProviderHealthRecord>>,
    cooldown: Duration,
}

impl ProviderHealthTracker {
    pub fn new() -> Self {
        Self::with_cooldown(CIRCUIT_BREAKER_COOLDOWN)
    }

    pub fn with_cooldown(cooldown: Duration) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            cooldown,
        }
    }

    pub fn record_success(&self, provider: MangaProviders) {
        if let Ok(mut map) = self.records.lock() {
            let rec = map.entry(provider).or_default();
            rec.consecutive_failures = 0;
            rec.total_successes += 1;
            rec.circuit_opened_at = None;
            rec.last_error = None;
        }
    }

    pub fn record_failure(&self, provider: MangaProviders, error: ProviderError) {
        if !error.should_trip_circuit() {
            return;
        }
        if let Ok(mut map) = self.records.lock() {
            let rec = map.entry(provider).or_default();
            rec.total_failures += 1;
            rec.consecutive_failures += 1;
            // Immediate circuit trip on Cloudflare block or rate-limit
            if matches!(error, ProviderError::CloudflareBlocked { .. } | ProviderError::RateLimited { .. }) {
                rec.consecutive_failures = rec.consecutive_failures.max(CIRCUIT_BREAKER_THRESHOLD);
            }
            if rec.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
                rec.circuit_opened_at = Some(Instant::now());
            }
            rec.last_error = Some(error);
        }
    }

    pub fn status(&self, provider: MangaProviders) -> ProviderHealthStatus {
        let Ok(map) = self.records.lock() else {
            return ProviderHealthStatus::Healthy;
        };
        let Some(rec) = map.get(&provider) else {
            return ProviderHealthStatus::Healthy;
        };
        if rec.consecutive_failures == 0 {
            ProviderHealthStatus::Healthy
        } else if rec.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
            if let Some(opened_at) = rec.circuit_opened_at
                && opened_at.elapsed() < self.cooldown
            {
                return ProviderHealthStatus::CircuitOpen;
            }
            // Cooldown elapsed -> half-open (degraded until next probe succeeds)
            ProviderHealthStatus::Degraded
        } else {
            ProviderHealthStatus::Degraded
        }
    }

    pub fn is_available(&self, provider: MangaProviders) -> bool {
        self.status(provider) != ProviderHealthStatus::CircuitOpen
    }

    pub fn last_error(&self, provider: MangaProviders) -> Option<ProviderError> {
        self.records.lock().ok()?.get(&provider).and_then(|r| r.last_error.clone())
    }

    pub fn reset(&self, provider: MangaProviders) {
        if let Ok(mut map) = self.records.lock() {
            map.remove(&provider);
        }
    }

    pub fn record_startup_check_failure(&self, provider: MangaProviders, reason: &str) {
        let error = ProviderError::from_error_str(Some(provider), reason);
        if let Ok(mut map) = self.records.lock() {
            let rec = map.entry(provider).or_default();
            rec.total_failures += 1;
            rec.consecutive_failures = rec.consecutive_failures.max(CIRCUIT_BREAKER_THRESHOLD);
            rec.circuit_opened_at = Some(Instant::now());
            rec.last_error = Some(error);
        }
    }
}

pub fn spanish_provider_candidates(provider: MangaProviders) -> Option<[MangaProviders; 3]> {
    match provider {
        MangaProviders::Inmanga => Some([MangaProviders::Inmanga, MangaProviders::Mangaoni, MangaProviders::Leercapitulo]),
        MangaProviders::Mangaoni => Some([MangaProviders::Mangaoni, MangaProviders::Inmanga, MangaProviders::Leercapitulo]),
        MangaProviders::Leercapitulo => Some([MangaProviders::Leercapitulo, MangaProviders::Inmanga, MangaProviders::Mangaoni]),
        _ => None,
    }
}

pub async fn check_provider_capabilities<P>(provider: &P, query: &str, image_quality: ImageQuality) -> Result<(), String>
where
    P: SearchPageProvider + MangaPageProvider + GetChapterPages,
    <P as SearchPageProvider>::InnerState: Default,
{
    let search_results = provider
        .search_mangas(SearchTerm::trimmed(query), <P as SearchPageProvider>::InnerState::default(), Pagination::default())
        .await
        .map_err(|error| format!("search failed: {error}"))?;
    let manga = search_results
        .mangas
        .first()
        .ok_or_else(|| format!("search for '{query}' returned no results"))?;
    let chapters = provider
        .get_chapters(&manga.id, ChapterFilters::default(), Pagination::default())
        .await
        .map_err(|error| format!("chapter loading failed: {error}"))?;

    let chapter = chapters
        .chapters
        .first()
        .ok_or_else(|| format!("chapter loading returned no chapters for '{}'", manga.title))?;
    let pages = provider
        .get_chapter_pages_url_with_extension(&chapter.id, &manga.id, image_quality)
        .await
        .map_err(|error| format!("reader page loading failed: {error}"))?;
    let first_page = pages.first().ok_or_else(|| format!("reader returned no pages for '{}'", manga.title))?;
    provider
        .get_image(first_page.url.as_str())
        .await
        .map_err(|error| format!("reader image failed to load or decode: {error}"))?;

    Ok(())
}

pub async fn check_spanish_provider(provider: MangaProviders, image_quality: ImageQuality) -> Result<(), String> {
    let pool = &*POOL;
    match provider {
        MangaProviders::Inmanga => check_provider_capabilities(&pool.inmanga, "frieren", image_quality).await,
        MangaProviders::Mangaoni => check_provider_capabilities(&pool.mangaoni, "one piece", image_quality).await,
        MangaProviders::Leercapitulo => check_provider_capabilities(&pool.leercapitulo, "one piece", image_quality).await,
        _ => Err(format!("{provider} is not a Spanish provider")),
    }
}

pub struct MultiProviderPool {
    pub cache: Arc<dyn Cacher>,
    pub health: ProviderHealthTracker,
    pub mangadex: MangadexClient,
    pub inmanga: InmangaProvider,
    pub leercapitulo: LeercapituloProvider,
    pub mangaoni: MangaoniProvider,
    pub weebcentral: WeebcentralProvider,
    pub mangapill: MangaPillProvider,
    pub comizy: ComizyProvider,
}

impl MultiProviderPool {
    pub fn new() -> Self {
        let cache: Arc<dyn Cacher> = InMemoryCache::init(16);
        let config = MangaTuiConfig::get();
        Self {
            mangadex: MangadexClient::new(API_URL_BASE.parse().unwrap(), COVER_IMG_URL_BASE.parse().unwrap(), Arc::clone(&cache))
                .with_image_quality(config.image_quality),
            inmanga: InmangaProvider::new(Arc::clone(&cache)),
            leercapitulo: LeercapituloProvider::new(Arc::clone(&cache)),
            mangaoni: MangaoniProvider::new(Arc::clone(&cache)),
            weebcentral: WeebcentralProvider::new(WEEBCENTRAL_BASE_URL.parse().unwrap(), Arc::clone(&cache)),
            mangapill: MangaPillProvider::init(Arc::clone(&cache)),
            comizy: ComizyProvider::new(Arc::clone(&cache)),
            health: ProviderHealthTracker::new(),
            cache,
        }
    }
}

pub static POOL: LazyLock<MultiProviderPool> = LazyLock::new(MultiProviderPool::new);

pub fn get_provider_health_status(provider: MangaProviders) -> ProviderHealthStatus {
    POOL.health.status(provider)
}

pub fn is_provider_available(provider: MangaProviders) -> bool {
    POOL.health.is_available(provider)
}

pub fn get_provider_last_error(provider: MangaProviders) -> Option<ProviderError> {
    POOL.health.last_error(provider)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultiSearchLanguage {
    #[default]
    All,
    Spanish,
    English,
}

/// Represents a deduplicated manga card across multiple providers.
/// `variants` holds each provider's `SearchManga` sorted from best to fallback,
/// and `selected_idx` points to the active provider (defaults to 0 = best provider).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupedSearchManga {
    pub variants: Vec<SearchManga>,
    pub selected_idx: usize,
}

impl GroupedSearchManga {
    pub fn new(variants: Vec<SearchManga>) -> Self {
        Self {
            variants,
            selected_idx: 0,
        }
    }

    pub fn selected_manga(&self) -> &SearchManga {
        &self.variants[self.selected_idx.min(self.variants.len().saturating_sub(1))]
    }

    pub fn cycle_provider(&mut self) -> bool {
        if self.variants.len() <= 1 {
            return false;
        }
        self.selected_idx = (self.selected_idx + 1) % self.variants.len();
        true
    }

    pub fn available_providers(&self) -> Vec<MangaProviders> {
        self.variants.iter().filter_map(|v| v.provider).collect()
    }
}

fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

pub fn normalize_title_for_grouping(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_space = true;
    for ch in title.chars() {
        for lower in ch.to_lowercase() {
            let folded = fold_accent(lower);
            if folded.is_alphanumeric() {
                out.push(folded);
                prev_space = false;
            } else if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() { title.trim().to_lowercase() } else { trimmed.to_string() }
}

pub fn provider_priority_rank(provider: Option<MangaProviders>, lang: MultiSearchLanguage) -> u8 {
    match lang {
        MultiSearchLanguage::English => match provider {
            Some(MangaProviders::Weebcentral) => 0,
            Some(MangaProviders::Mangapill) => 1,
            Some(MangaProviders::Mangadex) => 2,
            Some(MangaProviders::Comizy) => 3,
            Some(MangaProviders::Inmanga) => 4,
            Some(MangaProviders::Mangaoni) => 5,
            Some(MangaProviders::Leercapitulo) => 6,
            None => 7,
        },
        MultiSearchLanguage::All | MultiSearchLanguage::Spanish => match provider {
            Some(MangaProviders::Inmanga) => 0,
            Some(MangaProviders::Mangaoni) => 1,
            Some(MangaProviders::Leercapitulo) => 2,
            Some(MangaProviders::Mangadex) => 3,
            Some(MangaProviders::Weebcentral) => 4,
            Some(MangaProviders::Mangapill) => 5,
            Some(MangaProviders::Comizy) => 6,
            None => 7,
        },
    }
}

fn provider_health_rank(provider: Option<MangaProviders>) -> u8 {
    match provider.map(get_provider_health_status).unwrap_or_default() {
        ProviderHealthStatus::Healthy => 0,
        ProviderHealthStatus::Degraded => 1,
        ProviderHealthStatus::CircuitOpen => 2,
    }
}

fn metadata_richness(manga: &SearchManga) -> u8 {
    let mut score = 0;
    if !manga.cover_img_url.trim().is_empty() {
        score += 1;
    }
    if manga
        .description
        .as_deref()
        .is_some_and(|d| !d.trim().is_empty() && d != "No description")
    {
        score += 2;
    }
    if !manga.genres.is_empty() {
        score += 1;
    }
    if manga.status.is_some() {
        score += 1;
    }
    score
}

fn enrich_group_variants(variants: &mut [SearchManga]) {
    if variants.len() <= 1 {
        return;
    }
    let fallback_desc = variants.iter().find_map(|v| {
        v.description
            .as_ref()
            .filter(|d| !d.trim().is_empty() && d.as_str() != "No description")
            .cloned()
    });
    let fallback_genres = variants.iter().find(|v| !v.genres.is_empty()).map(|v| v.genres.clone());
    let fallback_status = variants.iter().find_map(|v| v.status);
    let fallback_cover = variants
        .iter()
        .find(|v| !v.cover_img_url.trim().is_empty())
        .map(|v| v.cover_img_url.clone());
    let fallback_author = variants.iter().find_map(|v| v.author.clone());
    let fallback_artist = variants.iter().find_map(|v| v.artist.clone());

    for v in variants.iter_mut() {
        let needs_desc = v.description.as_deref().is_none_or(|d| d.trim().is_empty() || d == "No description");
        if needs_desc && let Some(desc) = &fallback_desc {
            v.description = Some(desc.clone());
        }
        if v.genres.is_empty()
            && let Some(genres) = &fallback_genres
        {
            v.genres = genres.clone();
        }
        if v.status.is_none() {
            v.status = fallback_status;
        }
        if v.cover_img_url.trim().is_empty()
            && let Some(cover) = &fallback_cover
        {
            v.cover_img_url = cover.clone();
        }
        if v.author.is_none() {
            v.author = fallback_author.clone();
        }
        if v.artist.is_none() {
            v.artist = fallback_artist.clone();
        }
    }
}

/// Groups duplicate `SearchManga` results across providers by normalized title,
/// enriching metadata across variants and automatically selecting the best healthy provider.
pub fn group_search_results(results: Vec<SearchManga>, query: &str, lang: MultiSearchLanguage) -> Vec<GroupedSearchManga> {
    if results.is_empty() {
        return Vec::new();
    }
    let query_norm = normalize_title_for_grouping(query);
    let mut group_indices: HashMap<String, usize> = HashMap::new();
    let mut buckets: Vec<Vec<SearchManga>> = Vec::new();

    for manga in results {
        let key = normalize_title_for_grouping(&manga.title);
        if let Some(&bucket_idx) = group_indices.get(&key) {
            let bucket = &mut buckets[bucket_idx];
            if let Some(prov) = manga.provider {
                if let Some(existing) = bucket.iter_mut().find(|v| v.provider == Some(prov)) {
                    if existing
                        .description
                        .as_deref()
                        .is_none_or(|d| d.trim().is_empty() || d == "No description")
                        && manga.description.is_some()
                    {
                        existing.description = manga.description;
                    }
                    if existing.genres.is_empty() && !manga.genres.is_empty() {
                        existing.genres = manga.genres;
                    }
                    if existing.status.is_none() && manga.status.is_some() {
                        existing.status = manga.status;
                    }
                    if existing.cover_img_url.trim().is_empty() && !manga.cover_img_url.trim().is_empty() {
                        existing.cover_img_url = manga.cover_img_url;
                    }
                } else {
                    bucket.push(manga);
                }
            } else if !bucket.iter().any(|v| v.id == manga.id) {
                bucket.push(manga);
            }
        } else {
            let idx = buckets.len();
            group_indices.insert(key, idx);
            buckets.push(vec![manga]);
        }
    }

    let mut groups: Vec<GroupedSearchManga> = buckets
        .into_iter()
        .map(|mut variants| {
            variants.sort_by(|a, b| {
                let health_a = provider_health_rank(a.provider);
                let health_b = provider_health_rank(b.provider);
                let prio_a = provider_priority_rank(a.provider, lang);
                let prio_b = provider_priority_rank(b.provider, lang);
                let rich_a = metadata_richness(a);
                let rich_b = metadata_richness(b);

                health_a
                    .cmp(&health_b)
                    .then_with(|| prio_a.cmp(&prio_b))
                    .then_with(|| rich_b.cmp(&rich_a))
            });
            enrich_group_variants(&mut variants);
            GroupedSearchManga::new(variants)
        })
        .collect();

    if !query_norm.is_empty() {
        groups.sort_by(|a, b| {
            let norm_a = normalize_title_for_grouping(&a.selected_manga().title);
            let norm_b = normalize_title_for_grouping(&b.selected_manga().title);
            let rel_a = calculate_relevance_score(&norm_a, &query_norm);
            let rel_b = calculate_relevance_score(&norm_b, &query_norm);
            rel_a.cmp(&rel_b).then_with(|| b.variants.len().cmp(&a.variants.len()))
        });
    }

    groups
}

pub fn calculate_relevance_score(title: &str, query_lower: &str) -> u8 {
    let t = title.to_lowercase();
    if t == query_lower {
        0
    } else if t.starts_with(query_lower) {
        1
    } else if t.split_whitespace().any(|word| word.starts_with(query_lower)) {
        2
    } else if t.contains(query_lower) {
        3
    } else {
        4
    }
}

fn handle_search_outcome<T>(
    pool: &MultiProviderPool,
    provider: MangaProviders,
    res: Result<Result<T, Box<dyn Error>>, tokio::time::error::Elapsed>,
) -> Option<T> {
    match res {
        Ok(Ok(val)) => {
            pool.health.record_success(provider);
            Some(val)
        },
        Ok(Err(e)) => {
            pool.health
                .record_failure(provider, ProviderError::from_error(Some(provider), e.as_ref()));
            None
        },
        Err(_) => {
            pool.health.record_failure(provider, ProviderError::NetworkTimeout {
                provider: Some(provider),
            });
            None
        },
    }
}

/// Search concurrently across manga providers matching the specified language filter.
/// Providers whose circuit breaker is open are automatically skipped during their cooldown window.
pub async fn search_all_providers_with_lang(query: &str, lang: MultiSearchLanguage) -> Vec<SearchManga> {
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        return Vec::new();
    }
    let pool = &*POOL;
    let timeout_duration = Duration::from_secs(8);

    let query_spanish = lang == MultiSearchLanguage::All || lang == MultiSearchLanguage::Spanish;
    let query_english = lang == MultiSearchLanguage::All || lang == MultiSearchLanguage::English;

    let (inmanga_res, mangaoni_res, leercapitulo_res, mangadex_res, weebcentral_res, mangapill_res, comizy_res) = tokio::join!(
        // InManga
        async {
            if !query_spanish || !pool.health.is_available(MangaProviders::Inmanga) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let res = tokio::time::timeout(
                timeout_duration,
                pool.inmanga.search_mangas(term, InmangaFilterState::default(), Pagination::default()),
            )
            .await;
            handle_search_outcome(pool, MangaProviders::Inmanga, res)
        },
        // MangaOni
        async {
            if !query_spanish || !pool.health.is_available(MangaProviders::Mangaoni) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let res = tokio::time::timeout(
                timeout_duration,
                pool.mangaoni.search_mangas(term, MangaoniFilterState::default(), Pagination::default()),
            )
            .await;
            handle_search_outcome(pool, MangaProviders::Mangaoni, res)
        },
        // LeerCapítulo
        async {
            if !query_spanish || !pool.health.is_available(MangaProviders::Leercapitulo) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let res = tokio::time::timeout(
                timeout_duration,
                pool.leercapitulo
                    .search_mangas(term, LeercapituloFilterState::default(), Pagination::default()),
            )
            .await;
            handle_search_outcome(pool, MangaProviders::Leercapitulo, res)
        },
        // MangaDex (included in All, Spanish, and English)
        async {
            if !pool.health.is_available(MangaProviders::Mangadex) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let mut filters = Filters::default();
            filters.set_sort_by(SortBy::BestMatch);
            filters.set_content_rating(vec![ContentRating::Safe, ContentRating::Suggestive]);
            let res =
                tokio::time::timeout(timeout_duration, pool.mangadex.search_mangas(term, filters, Pagination::default())).await;
            handle_search_outcome(pool, MangaProviders::Mangadex, res)
        },
        // WeebCentral
        async {
            if !query_english || !pool.health.is_available(MangaProviders::Weebcentral) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let res = tokio::time::timeout(
                timeout_duration,
                pool.weebcentral
                    .search_mangas(term, WeebcentralFilterState::default(), Pagination::default()),
            )
            .await;
            handle_search_outcome(pool, MangaProviders::Weebcentral, res)
        },
        // MangaPill
        async {
            if !query_english || !pool.health.is_available(MangaProviders::Mangapill) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let res = tokio::time::timeout(
                timeout_duration,
                pool.mangapill.search_mangas(term, MangaPillFilterState::default(), Pagination::default()),
            )
            .await;
            handle_search_outcome(pool, MangaProviders::Mangapill, res)
        },
        // Comizy
        async {
            if !query_english || !pool.health.is_available(MangaProviders::Comizy) {
                return None;
            }
            let term = SearchTerm::trimmed_lowercased(query);
            let res = tokio::time::timeout(
                timeout_duration,
                pool.comizy.search_mangas(term, ComizyFilterState::default(), Pagination::default()),
            )
            .await;
            handle_search_outcome(pool, MangaProviders::Comizy, res)
        },
    );

    let mut combined = Vec::new();

    // Spanish providers first for Spanish user priority, then international
    if let Some(res) = inmanga_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Inmanga);
            combined.push(manga);
        }
    }
    if let Some(res) = mangaoni_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Mangaoni);
            combined.push(manga);
        }
    }
    if let Some(res) = leercapitulo_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Leercapitulo);
            combined.push(manga);
        }
    }
    if let Some(res) = mangadex_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Mangadex);
            combined.push(manga);
        }
    }
    if let Some(res) = weebcentral_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Weebcentral);
            combined.push(manga);
        }
    }
    if let Some(res) = mangapill_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Mangapill);
            combined.push(manga);
        }
    }
    if let Some(res) = comizy_res {
        for mut manga in res.mangas {
            manga.provider = Some(MangaProviders::Comizy);
            combined.push(manga);
        }
    }

    // Sort by query relevance (exact match first -> prefix -> word match -> substring)
    let query_lower = query_trimmed.to_lowercase();
    combined.sort_by(|a, b| {
        let score_a = calculate_relevance_score(&a.title, &query_lower);
        let score_b = calculate_relevance_score(&b.title, &query_lower);
        score_a.cmp(&score_b)
    });

    combined
}

/// Search concurrently across all available manga providers
pub async fn search_all_providers(query: &str) -> Vec<SearchManga> {
    search_all_providers_with_lang(query, MultiSearchLanguage::All).await
}

/// Find alternative providers that carry a manga matching `manga_title`, excluding `exclude_provider`.
/// Prioritizes the best match per provider first so the user can quickly migrate between sources.
pub async fn find_alternative_providers(manga_title: &str, exclude_provider: Option<MangaProviders>) -> Vec<SearchManga> {
    let query_trimmed = manga_title.trim();
    if query_trimmed.is_empty() {
        return Vec::new();
    }
    let query_lower = query_trimmed.to_lowercase();
    let all_results = search_all_providers(query_trimmed).await;

    let mut primary_per_provider = Vec::new();
    let mut secondary_matches = Vec::new();
    let mut seen_providers = HashSet::new();

    for manga in all_results {
        if manga.provider == exclude_provider {
            continue;
        }
        let score = calculate_relevance_score(&manga.title, &query_lower);
        if score <= 3 {
            if let Some(prov) = manga.provider
                && seen_providers.insert(prov)
            {
                primary_per_provider.push(manga);
            } else if score <= 1 {
                secondary_matches.push(manga);
            }
        }
    }

    primary_per_provider.extend(secondary_matches);
    primary_per_provider
}

fn record_result<T>(provider: MangaProviders, res: Result<T, Box<dyn Error>>) -> Result<T, Box<dyn Error>> {
    let pool = &*POOL;
    match res {
        Ok(val) => {
            pool.health.record_success(provider);
            Ok(val)
        },
        Err(e) => {
            let typed_err = ProviderError::from_error(Some(provider), e.as_ref());
            pool.health.record_failure(provider, typed_err.clone());
            Err(Box::new(typed_err))
        },
    }
}

pub async fn get_manga_by_id_for_provider(provider: MangaProviders, manga_id: &str) -> Result<Manga, Box<dyn Error + Send + Sync>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_manga_by_id(manga_id).await,
        MangaProviders::Inmanga => pool.inmanga.get_manga_by_id(manga_id).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_manga_by_id(manga_id).await,
        MangaProviders::Mangaoni => pool.mangaoni.get_manga_by_id(manga_id).await,
        MangaProviders::Weebcentral => pool.weebcentral.get_manga_by_id(manga_id).await,
        MangaProviders::Mangapill => pool.mangapill.get_manga_by_id(manga_id).await,
        MangaProviders::Comizy => pool.comizy.get_manga_by_id(manga_id).await,
    };
    match res {
        Ok(mut manga) => {
            pool.health.record_success(provider);
            manga.provider = Some(provider);
            Ok(manga)
        },
        Err(e) => {
            let typed_err = ProviderError::from_error(Some(provider), e.as_ref());
            pool.health.record_failure(provider, typed_err.clone());
            Err(Box::new(typed_err))
        },
    }
}

pub async fn get_chapters_for_provider(
    provider: MangaProviders,
    manga_id: &str,
    chapter_filters: ChapterFilters,
    pagination: Pagination,
) -> Result<GetChaptersResponse, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_chapters(manga_id, chapter_filters, pagination).await,
        MangaProviders::Inmanga => pool.inmanga.get_chapters(manga_id, chapter_filters, pagination).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_chapters(manga_id, chapter_filters, pagination).await,
        MangaProviders::Mangaoni => pool.mangaoni.get_chapters(manga_id, chapter_filters, pagination).await,
        MangaProviders::Weebcentral => pool.weebcentral.get_chapters(manga_id, chapter_filters, pagination).await,
        MangaProviders::Mangapill => pool.mangapill.get_chapters(manga_id, chapter_filters, pagination).await,
        MangaProviders::Comizy => pool.comizy.get_chapters(manga_id, chapter_filters, pagination).await,
    };
    record_result(provider, res)
}

pub async fn get_all_chapters_for_provider(
    provider: MangaProviders,
    manga_id: &str,
    lang: Languages,
) -> Result<Vec<Chapter>, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_all_chapters(manga_id, lang).await,
        MangaProviders::Inmanga => pool.inmanga.get_all_chapters(manga_id, lang).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_all_chapters(manga_id, lang).await,
        MangaProviders::Mangaoni => pool.mangaoni.get_all_chapters(manga_id, lang).await,
        MangaProviders::Weebcentral => pool.weebcentral.get_all_chapters(manga_id, lang).await,
        MangaProviders::Mangapill => pool.mangapill.get_all_chapters(manga_id, lang).await,
        MangaProviders::Comizy => pool.comizy.get_all_chapters(manga_id, lang).await,
    };
    record_result(provider, res)
}

pub async fn get_latest_chapters_for_provider(
    provider: MangaProviders,
    manga_id: &str,
) -> Result<Vec<LatestChapter>, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_latest_chapters(manga_id).await,
        MangaProviders::Inmanga => pool.inmanga.get_latest_chapters(manga_id).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_latest_chapters(manga_id).await,
        MangaProviders::Mangaoni => pool.mangaoni.get_latest_chapters(manga_id).await,
        MangaProviders::Weebcentral => pool.weebcentral.get_latest_chapters(manga_id).await,
        MangaProviders::Mangapill => pool.mangapill.get_latest_chapters(manga_id).await,
        MangaProviders::Comizy => pool.comizy.get_latest_chapters(manga_id).await,
    };
    record_result(provider, res)
}

pub async fn read_chapter_for_provider(
    provider: MangaProviders,
    chapter_id: &str,
    manga_id: &str,
) -> Result<(ChapterToRead, ListOfChapters), Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.read_chapter(chapter_id, manga_id).await,
        MangaProviders::Inmanga => pool.inmanga.read_chapter(chapter_id, manga_id).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.read_chapter(chapter_id, manga_id).await,
        MangaProviders::Mangaoni => pool.mangaoni.read_chapter(chapter_id, manga_id).await,
        MangaProviders::Weebcentral => pool.weebcentral.read_chapter(chapter_id, manga_id).await,
        MangaProviders::Mangapill => pool.mangapill.read_chapter(chapter_id, manga_id).await,
        MangaProviders::Comizy => pool.comizy.read_chapter(chapter_id, manga_id).await,
    };
    record_result(provider, res)
}

pub async fn search_chapter_for_provider(
    provider: MangaProviders,
    chapter_id: &str,
    manga_id: &str,
) -> Result<ChapterToRead, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.search_chapter(chapter_id, manga_id).await,
        MangaProviders::Inmanga => pool.inmanga.search_chapter(chapter_id, manga_id).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.search_chapter(chapter_id, manga_id).await,
        MangaProviders::Mangaoni => pool.mangaoni.search_chapter(chapter_id, manga_id).await,
        MangaProviders::Weebcentral => pool.weebcentral.search_chapter(chapter_id, manga_id).await,
        MangaProviders::Mangapill => pool.mangapill.search_chapter(chapter_id, manga_id).await,
        MangaProviders::Comizy => pool.comizy.search_chapter(chapter_id, manga_id).await,
    };
    record_result(provider, res)
}

pub async fn get_chapter_pages_for_provider<F: Fn(f64, &str) + 'static + Send>(
    provider: MangaProviders,
    chapter_id: &str,
    manga_id: &str,
    quality: ImageQuality,
    progress: F,
) -> Result<Vec<ChapterPage>, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
        MangaProviders::Inmanga => pool.inmanga.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
        MangaProviders::Mangaoni => pool.mangaoni.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
        MangaProviders::Weebcentral => pool.weebcentral.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
        MangaProviders::Mangapill => pool.mangapill.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
        MangaProviders::Comizy => pool.comizy.get_chapter_pages(chapter_id, manga_id, quality, progress).await,
    };
    record_result(provider, res)
}

pub async fn search_manga_panel_for_provider(provider: MangaProviders, endpoint: Url) -> Result<MangaPanel, Box<dyn Error>> {
    let pool = &*POOL;
    match provider {
        MangaProviders::Mangadex => pool.mangadex.search_manga_panel(endpoint).await,
        MangaProviders::Inmanga => pool.inmanga.search_manga_panel(endpoint).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.search_manga_panel(endpoint).await,
        MangaProviders::Mangaoni => pool.mangaoni.search_manga_panel(endpoint).await,
        MangaProviders::Weebcentral => pool.weebcentral.search_manga_panel(endpoint).await,
        MangaProviders::Mangapill => pool.mangapill.search_manga_panel(endpoint).await,
        MangaProviders::Comizy => pool.comizy.search_manga_panel(endpoint).await,
    }
}

pub async fn get_raw_image_for_provider(provider: MangaProviders, url: &str) -> Result<Bytes, Box<dyn Error>> {
    let pool = &*POOL;
    match provider {
        MangaProviders::Mangadex => pool.mangadex.get_raw_image(url).await,
        MangaProviders::Inmanga => pool.inmanga.get_raw_image(url).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_raw_image(url).await,
        MangaProviders::Mangaoni => pool.mangaoni.get_raw_image(url).await,
        MangaProviders::Weebcentral => pool.weebcentral.get_raw_image(url).await,
        MangaProviders::Mangapill => pool.mangapill.get_raw_image(url).await,
        MangaProviders::Comizy => pool.comizy.get_raw_image(url).await,
    }
}

pub async fn get_image_for_provider(provider: MangaProviders, url: &str) -> Result<DynamicImage, Box<dyn Error + Send + Sync>> {
    let pool = &*POOL;
    match provider {
        MangaProviders::Mangadex => pool.mangadex.get_image(url).await.map_err(|e| e.to_string().into()),
        MangaProviders::Inmanga => pool.inmanga.get_image(url).await.map_err(|e| e.to_string().into()),
        MangaProviders::Leercapitulo => pool.leercapitulo.get_image(url).await.map_err(|e| e.to_string().into()),
        MangaProviders::Mangaoni => pool.mangaoni.get_image(url).await.map_err(|e| e.to_string().into()),
        MangaProviders::Weebcentral => pool.weebcentral.get_image(url).await.map_err(|e| e.to_string().into()),
        MangaProviders::Mangapill => pool.mangapill.get_image(url).await.map_err(|e| e.to_string().into()),
        MangaProviders::Comizy => pool.comizy.get_image(url).await.map_err(|e| e.to_string().into()),
    }
}

pub async fn fetch_chapter_bookmarked_for_provider(
    provider: MangaProviders,
    bookmark: ChapterBookmarked,
) -> Result<(ChapterToRead, ListOfChapters), Box<dyn Error>> {
    let pool = &*POOL;
    match provider {
        MangaProviders::Mangadex => pool.mangadex.fetch_chapter_bookmarked(bookmark).await,
        MangaProviders::Inmanga => pool.inmanga.fetch_chapter_bookmarked(bookmark).await,
        MangaProviders::Leercapitulo => pool.leercapitulo.fetch_chapter_bookmarked(bookmark).await,
        MangaProviders::Mangaoni => pool.mangaoni.fetch_chapter_bookmarked(bookmark).await,
        MangaProviders::Weebcentral => pool.weebcentral.fetch_chapter_bookmarked(bookmark).await,
        MangaProviders::Mangapill => pool.mangapill.fetch_chapter_bookmarked(bookmark).await,
        MangaProviders::Comizy => pool.comizy.fetch_chapter_bookmarked(bookmark).await,
    }
}

pub async fn get_popular_mangas_for_provider(provider: MangaProviders) -> Result<Vec<PopularManga>, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_popular_mangas().await,
        MangaProviders::Inmanga => pool.inmanga.get_popular_mangas().await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_popular_mangas().await,
        MangaProviders::Mangaoni => pool.mangaoni.get_popular_mangas().await,
        MangaProviders::Weebcentral => pool.weebcentral.get_popular_mangas().await,
        MangaProviders::Mangapill => pool.mangapill.get_popular_mangas().await,
        MangaProviders::Comizy => pool.comizy.get_popular_mangas().await,
    };
    record_result(provider, res)
}

pub async fn get_recently_added_mangas_for_provider(provider: MangaProviders) -> Result<Vec<RecentlyAddedManga>, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => pool.mangadex.get_recently_added_mangas().await,
        MangaProviders::Inmanga => pool.inmanga.get_recently_added_mangas().await,
        MangaProviders::Leercapitulo => pool.leercapitulo.get_recently_added_mangas().await,
        MangaProviders::Mangaoni => pool.mangaoni.get_recently_added_mangas().await,
        MangaProviders::Weebcentral => pool.weebcentral.get_recently_added_mangas().await,
        MangaProviders::Mangapill => pool.mangapill.get_recently_added_mangas().await,
        MangaProviders::Comizy => pool.comizy.get_recently_added_mangas().await,
    };
    record_result(provider, res)
}

pub async fn search_mangas_for_provider(
    provider: MangaProviders,
    search_term: Option<SearchTerm>,
    pagination: Pagination,
) -> Result<GetMangasResponse, Box<dyn Error>> {
    let pool = &*POOL;
    let res = match provider {
        MangaProviders::Mangadex => {
            let mut filters = Filters::default();
            filters.set_sort_by(SortBy::BestMatch);
            filters.set_content_rating(vec![ContentRating::Safe, ContentRating::Suggestive]);
            pool.mangadex.search_mangas(search_term, filters, pagination).await
        },
        MangaProviders::Inmanga => pool.inmanga.search_mangas(search_term, InmangaFilterState::default(), pagination).await,
        MangaProviders::Leercapitulo => {
            pool.leercapitulo
                .search_mangas(search_term, LeercapituloFilterState::default(), pagination)
                .await
        },
        MangaProviders::Mangaoni => pool.mangaoni.search_mangas(search_term, MangaoniFilterState::default(), pagination).await,
        MangaProviders::Weebcentral => {
            pool.weebcentral
                .search_mangas(search_term, WeebcentralFilterState::default(), pagination)
                .await
        },
        MangaProviders::Mangapill => {
            pool.mangapill
                .search_mangas(search_term, MangaPillFilterState::default(), pagination)
                .await
        },
        MangaProviders::Comizy => pool.comizy.search_mangas(search_term, ComizyFilterState::default(), pagination).await,
    };
    record_result(provider, res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::manga_provider::ProviderIdentity;

    #[tokio::test]
    async fn test_pool_initializes_and_empty_query_returns_empty() {
        let results = search_all_providers("   ").await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_pool_singleton() {
        assert_eq!(POOL.mangadex.name(), MangaProviders::Mangadex);
        assert_eq!(POOL.inmanga.name(), MangaProviders::Inmanga);
        assert_eq!(POOL.mangaoni.name(), MangaProviders::Mangaoni);
        assert_eq!(POOL.leercapitulo.name(), MangaProviders::Leercapitulo);
        assert_eq!(POOL.weebcentral.name(), MangaProviders::Weebcentral);
        assert_eq!(POOL.mangapill.name(), MangaProviders::Mangapill);
        assert_eq!(POOL.comizy.name(), MangaProviders::Comizy);
    }

    #[test]
    fn test_calculate_relevance_score() {
        assert_eq!(calculate_relevance_score("One Piece", "one piece"), 0);
        assert_eq!(calculate_relevance_score("One Piece: Ace's Story", "one piece"), 1);
        assert_eq!(calculate_relevance_score("That One Piece Fan", "piece"), 2);
        assert_eq!(calculate_relevance_score("Clone Piece", "one"), 3);
        assert_eq!(calculate_relevance_score("Naruto", "one piece"), 4);
    }

    #[test]
    fn test_provider_error_classification() {
        let rate_err = ProviderError::from_error_str(Some(MangaProviders::Mangadex), "HTTP 429 Too Many Requests");
        assert!(rate_err.is_rate_limited());
        assert!(rate_err.is_transient());
        assert!(rate_err.should_trip_circuit());

        let cf_err = ProviderError::from_error_str(Some(MangaProviders::Inmanga), "403 Forbidden: Cloudflare challenge");
        assert_eq!(cf_err, ProviderError::CloudflareBlocked {
            provider: Some(MangaProviders::Inmanga),
        });
        assert!(cf_err.should_trip_circuit());

        let not_found = ProviderError::from_http_status(Some(MangaProviders::Mangaoni), reqwest::StatusCode::NOT_FOUND);
        assert!(!not_found.should_trip_circuit());

        let timeout_err = ProviderError::from_error_str(Some(MangaProviders::Comizy), "operation timed out");
        assert_eq!(timeout_err, ProviderError::NetworkTimeout {
            provider: Some(MangaProviders::Comizy),
        });
    }

    #[test]
    fn test_circuit_breaker_health_tracker() {
        let tracker = ProviderHealthTracker::with_cooldown(Duration::from_millis(50));
        let p = MangaProviders::Inmanga;

        assert_eq!(tracker.status(p), ProviderHealthStatus::Healthy);
        assert!(tracker.is_available(p));

        // 1st transient failure -> Degraded
        tracker.record_failure(p, ProviderError::NetworkTimeout { provider: Some(p) });
        assert_eq!(tracker.status(p), ProviderHealthStatus::Degraded);
        assert!(tracker.is_available(p));

        // NotFound should not trip circuit
        tracker.record_failure(p, ProviderError::NotFound {
            resource: "ch1".to_string(),
        });
        assert_eq!(tracker.status(p), ProviderHealthStatus::Degraded);

        // 2nd & 3rd failures -> CircuitOpen
        tracker.record_failure(p, ProviderError::NetworkTimeout { provider: Some(p) });
        tracker.record_failure(p, ProviderError::NetworkTimeout { provider: Some(p) });
        assert_eq!(tracker.status(p), ProviderHealthStatus::CircuitOpen);
        assert!(!tracker.is_available(p));

        // Wait for cooldown -> transitions to half-open (Degraded) and available again
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(tracker.status(p), ProviderHealthStatus::Degraded);
        assert!(tracker.is_available(p));

        // Success resets to Healthy
        tracker.record_success(p);
        assert_eq!(tracker.status(p), ProviderHealthStatus::Healthy);
        assert!(tracker.last_error(p).is_none());

        // Immediate circuit trip on Cloudflare block
        tracker.record_failure(p, ProviderError::CloudflareBlocked { provider: Some(p) });
        assert_eq!(tracker.status(p), ProviderHealthStatus::CircuitOpen);
    }

    #[test]
    fn test_normalize_title_for_grouping() {
        assert_eq!(normalize_title_for_grouping("One-Punch Man"), "one punch man");
        assert_eq!(normalize_title_for_grouping("One Punch Man!"), "one punch man");
        assert_eq!(normalize_title_for_grouping("Kaguya-sama: Love is War"), "kaguya sama love is war");
        assert_eq!(normalize_title_for_grouping("Kaguya-Sama Love Is War"), "kaguya sama love is war");
        assert_eq!(normalize_title_for_grouping("  BERSERK  "), "berserk");
        assert_ne!(normalize_title_for_grouping("Berserk"), normalize_title_for_grouping("Berserk: The Prototype"));
    }

    #[test]
    fn test_group_search_results_deduplicates_and_enriches() {
        let raw = vec![
            SearchManga {
                id: "md_1".to_string(),
                title: "One-Punch Man".to_string(),
                description: Some("Saitama is a hero for fun.".to_string()),
                cover_img_url: "https://mangadex.org/cover.jpg".to_string(),
                provider: Some(MangaProviders::Mangadex),
                ..Default::default()
            },
            SearchManga {
                id: "oni_1".to_string(),
                title: "One Punch Man".to_string(),
                description: None,
                cover_img_url: String::new(),
                provider: Some(MangaProviders::Mangaoni),
                ..Default::default()
            },
            SearchManga {
                id: "inm_1".to_string(),
                title: "ONE-PUNCH MAN!".to_string(),
                description: None,
                cover_img_url: "https://inmanga.com/opm.jpg".to_string(),
                provider: Some(MangaProviders::Inmanga),
                ..Default::default()
            },
            SearchManga {
                id: "inm_other".to_string(),
                title: "One-Punch Man: Hero Encyclopedia".to_string(),
                provider: Some(MangaProviders::Inmanga),
                ..Default::default()
            },
        ];

        let mut grouped = group_search_results(raw, "One-Punch Man", MultiSearchLanguage::All);
        assert_eq!(grouped.len(), 2);

        let main_group = &mut grouped[0];
        assert_eq!(main_group.variants.len(), 3);
        assert_eq!(main_group.available_providers(), vec![
            MangaProviders::Inmanga,
            MangaProviders::Mangaoni,
            MangaProviders::Mangadex,
        ]);

        // Automatically selects InManga first (best priority) and enriches its missing description from MangaDex
        assert_eq!(main_group.selected_manga().provider, Some(MangaProviders::Inmanga));
        assert_eq!(main_group.selected_manga().id, "inm_1");
        assert_eq!(main_group.selected_manga().description.as_deref(), Some("Saitama is a hero for fun."));

        // Cycling switches to MangaOni, then MangaDex, then wraps around to InManga
        assert!(main_group.cycle_provider());
        assert_eq!(main_group.selected_manga().provider, Some(MangaProviders::Mangaoni));
        assert_eq!(main_group.selected_manga().cover_img_url, "https://inmanga.com/opm.jpg");

        assert!(main_group.cycle_provider());
        assert_eq!(main_group.selected_manga().provider, Some(MangaProviders::Mangadex));

        assert!(main_group.cycle_provider());
        assert_eq!(main_group.selected_manga().provider, Some(MangaProviders::Inmanga));
    }

    #[tokio::test]
    #[ignore]
    async fn test_search_all_providers_one_piece() {
        let results = search_all_providers("one piece").await;
        println!("Total results: {}", results.len());
        for r in &results {
            println!("Provider: {:?} | Title: {}", r.provider, r.title);
        }
        let mangadex_count = results.iter().filter(|r| r.provider == Some(MangaProviders::Mangadex)).count();
        println!("Mangadex results count: {}", mangadex_count);
        assert!(mangadex_count > 0, "No mangadex results returned in search_all_providers!");
    }

    #[tokio::test]
    #[ignore]
    async fn test_search_all_providers_made_in_abyss() {
        let results = search_all_providers("made in abyss").await;
        println!("Total results: {}", results.len());
        for r in &results {
            println!("Provider: {:?} | Title: {}", r.provider, r.title);
        }
        let mangaoni_count = results.iter().filter(|r| r.provider == Some(MangaProviders::Mangaoni)).count();
        println!("MangaOni results count: {}", mangaoni_count);
        assert!(mangaoni_count > 0, "No mangaoni results returned in search_all_providers for made in abyss!");
    }
}
