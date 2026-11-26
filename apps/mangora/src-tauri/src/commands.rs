use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;

use manga_core::SearchTerm;
use manga_core::backend::database::{Database, MangaReadingHistoryRetrieve, RetrieveBookmark, extract_normalized_chapter_number};
use manga_core::backend::download_queue::ManagedDownloadQueue;
use manga_core::backend::manga_provider::{
    Chapter, ChapterFilters, ChapterOrderBy, Languages, Manga, MangaProviders, MangaStatus, Pagination,
};
use manga_core::backend::multi_provider::{
    get_chapters_for_provider, get_manga_by_id_for_provider, get_popular_mangas_for_provider, search_mangas_with_tags_for_provider,
};
use manga_core::backend::offline::{OfflineChapter, OfflineManga, find_offline_manga_in_dir};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::anilist::{AniListCommunityInfo, fetch_anilist_community};
use crate::downloads::{
    DownloadQueueControlRequest, DownloadQueueSnapshotDto, EnqueueChaptersRequest, OfflineSeriesItemDto, SetDownloadFormatResult,
    apply_queue_control, enqueue_chapters, map_queue_snapshot, parse_download_type, scan_offline_series,
};
use crate::reader::{
    OpenReaderChapterRequest, ReaderSessionResponse, ReaderSessionStore, SaveReaderProgressPayload, SaveReaderProgressResult,
    resolve_and_open_chapter, save_progress_in_db,
};
use crate::state::{
    AppState, InitialAppState, LibraryOverview, OnboardingPreferencesPayload, ReaderPrefs, ToggleChapterReadPayload,
    ToggleChapterReadResult, ToggleFavoritePayload, ToggleFavoriteResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub provider: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub cover_url: String,
    pub genres: Vec<String>,
    pub languages: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    items: Vec<CatalogItem>,
    has_next: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDetailRequest {
    pub manga_id: String,
    pub provider: String,
    pub fallback_title: Option<String>,
    pub fallback_cover_url: Option<String>,
    pub fallback_description: Option<String>,
    pub language: Option<String>,
    pub order: Option<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesChapterItem {
    pub id: String,
    pub title: String,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub language: String,
    pub scanlator: Option<String>,
    pub publication_date: Option<String>,
    pub is_read: bool,
    pub is_downloaded: bool,
    pub is_available_online: bool,
    pub is_bookmarked: bool,
    pub bookmarked_page: Option<u32>,
    pub offline_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDetailResponse {
    pub id: String,
    pub provider: String,
    pub provider_label: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub status: String,
    pub genres: Vec<String>,
    pub available_languages: Vec<String>,
    pub active_language: String,
    pub order: String,
    pub page: u32,
    pub total_chapters: u32,
    pub has_next_page: bool,
    pub is_favorite: bool,
    pub is_offline_fallback: bool,
    pub offline_chapter_count: u32,
    pub read_chapter_count: u32,
    pub next_unread_chapter: Option<SeriesChapterItem>,
    pub chapters: Vec<SeriesChapterItem>,
    pub online_warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForYouResponse {
    pub provider: String,
    pub provider_label: String,
    pub matched_genres: Vec<String>,
    pub items: Vec<CatalogItem>,
    pub fallback_popular: bool,
}

#[tauri::command]
pub async fn search_catalog(provider: String, query: String, page: u32, tags: Option<Vec<String>>) -> Result<CatalogPage, String> {
    let provider = MangaProviders::from_str(&provider).map_err(|_| "Unknown catalog source")?;
    let cleaned_tags: Vec<String> = tags
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let term = SearchTerm::trimmed(&query);
    if term.is_none() && cleaned_tags.is_empty() {
        return Err("Missing search term or category tag".into());
    }
    let mut pagination = Pagination::default();
    if page == 0 || page > u32::MAX / pagination.items_per_page {
        return Err("Invalid catalog page".into());
    }
    pagination.current_page = page;
    let end_offset = page * pagination.items_per_page;
    let (response, effective_provider) = tokio::time::timeout(
        Duration::from_secs(20),
        search_mangas_with_tags_for_provider(provider, term, &cleaned_tags, pagination),
    )
    .await
    .map_err(|_| "Catalog request timed out")?
    .map_err(|err| err.to_string())?;
    let effective_provider_id = effective_provider.to_string();
    Ok(CatalogPage {
        has_next: response.next_page
            && !response.mangas.is_empty()
            && (effective_provider != MangaProviders::Mangadex || end_offset < response.total_mangas),
        items: response
            .mangas
            .into_iter()
            .map(|item| CatalogItem {
                id: item.id,
                provider: Some(item.provider.map(|p| p.to_string()).unwrap_or_else(|| effective_provider_id.clone())),
                title: item.title,
                description: item.description,
                cover_url: item.cover_img_url,
                genres: item.genres.into_iter().map(|genre| genre.title).collect(),
                languages: item
                    .languages
                    .into_iter()
                    .map(|lang| lang.as_iso_code())
                    .filter(|code| !code.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn get_series_detail(state: State<'_, AppState>, request: SeriesDetailRequest) -> Result<SeriesDetailResponse, String> {
    let manga_id = request.manga_id.trim().to_string();
    if manga_id.is_empty() {
        return Err("Missing manga identifier".into());
    }
    let is_offline_only = manga_id.starts_with("offline::");
    let provider = match MangaProviders::from_str(request.provider.trim()) {
        Ok(p) => p,
        Err(_) if is_offline_only => MangaProviders::default(),
        Err(_) => return Err("Unknown catalog source".into()),
    };

    let page = request.page.unwrap_or(1);
    const CHAPTERS_PER_PAGE: u32 = 50;
    if page == 0 || page > u32::MAX / CHAPTERS_PER_PAGE {
        return Err("Invalid chapters page".into());
    }

    let order = parse_chapter_order(request.order.as_deref());
    let default_chapter_lang = state.default_chapter_language()?;
    let requested_lang = request
        .language
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(Languages::from_str_lenient);
    let mut active_lang = requested_lang.unwrap_or(default_chapter_lang);

    let (manga_result, mut chapters_result) = if is_offline_only {
        (Err("Offline-only archive".to_string()), Err("Offline-only archive".to_string()))
    } else {
        let meta = tokio::time::timeout(Duration::from_secs(15), get_manga_by_id_for_provider(provider, &manga_id))
            .await
            .map_err(|_| "Series metadata request timed out".to_string())
            .and_then(|res| res.map_err(|e| e.to_string()));
        let chs = fetch_chapters_page(provider, &manga_id, active_lang, order, page, CHAPTERS_PER_PAGE).await;
        (meta, chs)
    };

    if !is_offline_only
        && request.language.is_none()
        && provider == MangaProviders::Mangadex
        && chapters_result.as_ref().is_ok_and(|res| res.chapters.is_empty())
    {
        for fallback_lang in mangadex_fallback_languages(active_lang) {
            if let Ok(candidate) = fetch_chapters_page(provider, &manga_id, fallback_lang, order, page, CHAPTERS_PER_PAGE).await
                && !candidate.chapters.is_empty()
            {
                active_lang = fallback_lang;
                chapters_result = Ok(candidate);
                break;
            }
        }
    }

    let online_warning = if is_offline_only {
        None
    } else {
        match (&manga_result, &chapters_result) {
            (Err(e1), Err(e2)) => Some(format!("{e1}; {e2}")),
            (Ok(_), Err(e)) => Some(e.clone()),
            (Err(e), Ok(_)) if request.fallback_title.as_deref().map(str::trim).is_some_and(|t| !t.is_empty()) => Some(e.clone()),
            _ => None,
        }
    };

    let manga_meta: Option<Manga> = manga_result.ok();
    let (online_chapters, online_total) = match chapters_result {
        Ok(res) => (res.chapters, res.total_chapters),
        Err(_) => (Vec::new(), 0),
    };

    let mut title = manga_meta
        .as_ref()
        .map(|m| m.title.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| {
            request
                .fallback_title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| manga_id.clone());

    let downloads_dir = state.downloads_dir()?;
    let (is_favorite, db_statuses, read_numbers_by_title, downloaded_db_ids, bookmark, offline_manga, db_cover) = {
        let conn = state.open_db_connection()?;
        let db = Database::new(&conn);
        let offline = find_offline_manga_in_dir(&downloads_dir, &manga_id, &title, Some(provider), Some(&db));
        if title == manga_id
            && let Some(off) = offline.as_ref()
            && !off.title.trim().is_empty()
        {
            title = off.title.clone();
        }

        if let Some(meta) = manga_meta.as_ref() {
            let genre_titles: Vec<String> =
                meta.genres.iter().map(|g| g.title.trim().to_string()).filter(|g| !g.is_empty()).collect();
            if !genre_titles.is_empty() {
                let _ = db.save_manga_genres(&manga_id, provider, &genre_titles);
            }
        }

        let is_fav = db.is_favorite(&manga_id, provider).unwrap_or(false);
        let statuses = db.get_chapters_history_status(&manga_id).unwrap_or_default();
        let read_nums = db.get_read_chapter_numbers_by_manga_title(&title).unwrap_or_default();
        let dl_ids: HashSet<String> = db
            .get_downloaded_chapter_ids(&manga_id, provider)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let bookmark = db.get_bookmarked(&manga_id).ok().flatten();
        let stored_cover = conn
            .query_row(
                "SELECT img_url FROM mangas WHERE id = ?1 AND img_url IS NOT NULL AND TRIM(img_url) != '' LIMIT 1",
                rusqlite::params![&manga_id],
                |row| row.get::<_, String>(0),
            )
            .ok();

        (is_fav, statuses, read_nums, dl_ids, bookmark, offline, stored_cover)
    };

    if online_chapters.is_empty()
        && manga_meta.is_none()
        && offline_manga.is_none()
        && request.fallback_title.as_deref().map(str::trim).unwrap_or("").is_empty()
        && let Some(err) = online_warning
    {
        return Err(err);
    }

    let is_offline_fallback = online_chapters.is_empty() && offline_manga.as_ref().is_some_and(|off| !off.chapters.is_empty());
    let (chapters, next_unread_chapter, read_chapter_count, reconciled_offline_count) = reconcile_chapters(
        online_chapters,
        &db_statuses,
        &read_numbers_by_title,
        &downloaded_db_ids,
        bookmark.as_ref(),
        if page == 1 || !is_offline_fallback { offline_manga.as_ref() } else { None },
        order,
    );

    let offline_chapter_count = offline_manga
        .as_ref()
        .map(|m| m.chapters.len() as u32)
        .unwrap_or(reconciled_offline_count);

    let total_chapters = if online_total > 0 { online_total } else { chapters.len() as u32 };
    let has_next_page = !is_offline_fallback && !chapters.is_empty() && page.saturating_mul(CHAPTERS_PER_PAGE) < total_chapters;

    let cover_url = manga_meta
        .as_ref()
        .map(|m| m.cover_img_url.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            request
                .fallback_cover_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
        .or(db_cover);

    let description = manga_meta
        .as_ref()
        .map(|m| m.description.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            request
                .fallback_description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        });

    let mut available_languages: Vec<String> = manga_meta
        .as_ref()
        .map(|m| {
            m.languages
                .iter()
                .map(|l| l.as_iso_code())
                .filter(|c| !c.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let active_code = active_lang.as_iso_code();
    if !active_code.is_empty() && !available_languages.iter().any(|c| c == active_code) {
        available_languages.insert(0, active_code.to_string());
    }

    Ok(SeriesDetailResponse {
        id: manga_id,
        provider: provider.to_string(),
        provider_label: provider.badge_label().to_string(),
        title,
        cover_url,
        description,
        author: manga_meta
            .as_ref()
            .and_then(|m| m.author.as_ref())
            .map(|a| a.name.clone())
            .filter(|s| !s.trim().is_empty()),
        artist: manga_meta
            .as_ref()
            .and_then(|m| m.artist.as_ref())
            .map(|a| a.name.clone())
            .filter(|s| !s.trim().is_empty()),
        status: manga_meta
            .as_ref()
            .map(|m| status_id(m.status).to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        genres: manga_meta
            .as_ref()
            .map(|m| m.genres.iter().map(|g| g.title.clone()).filter(|s| !s.trim().is_empty()).collect())
            .unwrap_or_default(),
        available_languages,
        active_language: active_code.to_string(),
        order: match order {
            ChapterOrderBy::Ascending => "asc".to_string(),
            ChapterOrderBy::Descending => "desc".to_string(),
        },
        page,
        total_chapters,
        has_next_page,
        is_favorite,
        is_offline_fallback,
        offline_chapter_count,
        read_chapter_count,
        next_unread_chapter,
        chapters,
        online_warning,
    })
}

#[tauri::command]
pub async fn get_anilist_community(title: String) -> Result<AniListCommunityInfo, String> {
    Ok(fetch_anilist_community(&title).await)
}

#[tauri::command]
pub async fn get_for_you_recommendations(state: State<'_, AppState>, provider: Option<String>) -> Result<ForYouResponse, String> {
    let snapshot = state.snapshot()?;
    let provider_id = provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(snapshot.active_provider.as_str());
    let parsed_provider = MangaProviders::from_str(provider_id).map_err(|_| "Unknown catalog source")?;

    let popular_res = tokio::time::timeout(Duration::from_secs(15), get_popular_mangas_for_provider(parsed_provider))
        .await
        .map_err(|_| "Recommendations request timed out".to_string())
        .and_then(|res| res.map_err(|e| e.to_string()));

    let mut raw_items: Vec<CatalogItem> = match popular_res {
        Ok(list) => list
            .into_iter()
            .map(|item| CatalogItem {
                id: item.id,
                provider: Some(parsed_provider.to_string()),
                title: item.title,
                description: if item.description.trim().is_empty() { None } else { Some(item.description) },
                cover_url: item.cover_img_url,
                genres: item.genres.into_iter().map(|g| g.title).collect(),
                languages: Vec::new(),
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    if raw_items.is_empty() {
        let (search_fallback, fallback_provider) = tokio::time::timeout(
            Duration::from_secs(15),
            search_mangas_with_tags_for_provider(parsed_provider, None, &[], Pagination::default()),
        )
        .await
        .map_err(|_| "Recommendations request timed out".to_string())
        .and_then(|res| res.map_err(|e| e.to_string()))?;

        raw_items = search_fallback
            .mangas
            .into_iter()
            .map(|item| CatalogItem {
                id: item.id,
                provider: Some(fallback_provider.to_string()),
                title: item.title,
                description: item.description,
                cover_url: item.cover_img_url,
                genres: item.genres.into_iter().map(|g| g.title).collect(),
                languages: item
                    .languages
                    .into_iter()
                    .map(|l| l.as_iso_code())
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
            .collect();
    }

    let (items, matched_genres, fallback_popular) = rank_for_you_items(raw_items, &snapshot.favorite_genres);

    Ok(ForYouResponse {
        provider: parsed_provider.to_string(),
        provider_label: parsed_provider.badge_label().to_string(),
        matched_genres,
        items,
        fallback_popular,
    })
}

async fn fetch_chapters_page(
    provider: MangaProviders,
    manga_id: &str,
    language: Languages,
    order: ChapterOrderBy,
    page: u32,
    items_per_page: u32,
) -> Result<manga_core::backend::manga_provider::GetChaptersResponse, String> {
    tokio::time::timeout(
        Duration::from_secs(20),
        get_chapters_for_provider(
            provider,
            manga_id,
            ChapterFilters { order, language },
            Pagination::new(page, 300, items_per_page),
        ),
    )
    .await
    .map_err(|_| "Chapters request timed out".to_string())?
    .map_err(|e| e.to_string())
}

fn parse_chapter_order(raw: Option<&str>) -> ChapterOrderBy {
    match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("asc" | "ascending") => ChapterOrderBy::Ascending,
        _ => ChapterOrderBy::Descending,
    }
}

fn mangadex_fallback_languages(primary: Languages) -> Vec<Languages> {
    match primary {
        Languages::Spanish => vec![Languages::SpanishLa, Languages::English],
        Languages::English => vec![Languages::Spanish, Languages::SpanishLa],
        _ => vec![Languages::English, Languages::Spanish],
    }
}

fn status_id(status: MangaStatus) -> &'static str {
    match status {
        MangaStatus::Ongoing => "ongoing",
        MangaStatus::Completed => "completed",
        MangaStatus::Hiatus => "hiatus",
        MangaStatus::Cancelled => "cancelled",
    }
}

pub fn reconcile_chapters(
    online_chapters: Vec<Chapter>,
    db_statuses: &[MangaReadingHistoryRetrieve],
    read_numbers_by_title: &HashSet<String>,
    downloaded_db_ids: &HashSet<String>,
    bookmark: Option<&manga_core::backend::database::ChapterBookmarked>,
    offline_manga: Option<&OfflineManga>,
    order: ChapterOrderBy,
) -> (Vec<SeriesChapterItem>, Option<SeriesChapterItem>, u32, u32) {
    let status_by_id: HashMap<&str, &MangaReadingHistoryRetrieve> =
        db_statuses.iter().map(|entry| (entry.id.as_str(), entry)).collect();

    let mut offline_by_id: HashMap<&str, &OfflineChapter> = HashMap::new();
    let mut offline_by_num: HashMap<String, &OfflineChapter> = HashMap::new();
    if let Some(off) = offline_manga {
        for ch in &off.chapters {
            offline_by_id.insert(ch.chapter.id.as_str(), ch);
            if let Some(num) = extract_normalized_chapter_number(&ch.chapter.chapter_number)
                .or_else(|| extract_normalized_chapter_number(&ch.chapter.title))
            {
                offline_by_num.entry(num).or_insert(ch);
            }
        }
    }

    let mut items: Vec<SeriesChapterItem> = if !online_chapters.is_empty() {
        online_chapters
            .into_iter()
            .map(|ch| {
                let norm_num =
                    extract_normalized_chapter_number(&ch.chapter_number).or_else(|| extract_normalized_chapter_number(&ch.title));
                let db_hit = status_by_id.get(ch.id.as_str()).copied();
                let off_hit = offline_by_id
                    .get(ch.id.as_str())
                    .copied()
                    .or_else(|| norm_num.as_deref().and_then(|n| offline_by_num.get(n).copied()));

                let is_read = db_hit.is_some_and(|s| s.is_read)
                    || off_hit.is_some_and(|o| o.is_read)
                    || norm_num.as_deref().is_some_and(|n| read_numbers_by_title.contains(n));
                let is_downloaded =
                    db_hit.is_some_and(|s| s.is_downloaded) || downloaded_db_ids.contains(&ch.id) || off_hit.is_some();
                let is_bookmarked = bookmark.is_some_and(|b| b.id == ch.id) || off_hit.is_some_and(|o| o.is_bookmarked);
                let bookmarked_page = bookmark
                    .filter(|b| b.id == ch.id)
                    .and_then(|b| b.number_page_bookmarked)
                    .or_else(|| off_hit.and_then(|o| o.bookmarked_page));

                SeriesChapterItem {
                    id: ch.id,
                    title: ch.title,
                    chapter_number: ch.chapter_number,
                    volume_number: ch.volume_number,
                    language: ch.language.as_iso_code().to_string(),
                    scanlator: ch.scanlator,
                    publication_date: ch.publication_date.map(|d| d.to_string()),
                    is_read,
                    is_downloaded,
                    is_available_online: true,
                    is_bookmarked,
                    bookmarked_page,
                    offline_format: off_hit.map(|o| o.format_label.to_string()),
                }
            })
            .collect()
    } else if let Some(off) = offline_manga {
        let mut offline_items: Vec<SeriesChapterItem> = off
            .chapters
            .iter()
            .map(|off_ch| {
                let norm_num = extract_normalized_chapter_number(&off_ch.chapter.chapter_number)
                    .or_else(|| extract_normalized_chapter_number(&off_ch.chapter.title));
                let db_hit = status_by_id.get(off_ch.chapter.id.as_str()).copied();
                let is_read = off_ch.is_read
                    || db_hit.is_some_and(|s| s.is_read)
                    || norm_num.as_deref().is_some_and(|n| read_numbers_by_title.contains(n));
                let is_bookmarked = off_ch.is_bookmarked || bookmark.is_some_and(|b| b.id == off_ch.chapter.id);
                let bookmarked_page = bookmark
                    .filter(|b| b.id == off_ch.chapter.id)
                    .and_then(|b| b.number_page_bookmarked)
                    .or(off_ch.bookmarked_page);

                SeriesChapterItem {
                    id: off_ch.chapter.id.clone(),
                    title: off_ch.chapter.title.clone(),
                    chapter_number: off_ch.chapter.chapter_number.clone(),
                    volume_number: off_ch.chapter.volume_number.clone(),
                    language: off_ch.chapter.language.as_iso_code().to_string(),
                    scanlator: off_ch.chapter.scanlator.clone(),
                    publication_date: off_ch.chapter.publication_date.map(|d| d.to_string()),
                    is_read,
                    is_downloaded: true,
                    is_available_online: false,
                    is_bookmarked,
                    bookmarked_page,
                    offline_format: Some(off_ch.format_label.to_string()),
                }
            })
            .collect();

        offline_items.sort_by(|a, b| {
            let na = a.chapter_number.trim().parse::<f64>().unwrap_or(0.0);
            let nb = b.chapter_number.trim().parse::<f64>().unwrap_or(0.0);
            match order {
                ChapterOrderBy::Ascending => na.total_cmp(&nb).then_with(|| a.title.cmp(&b.title)),
                ChapterOrderBy::Descending => nb.total_cmp(&na).then_with(|| b.title.cmp(&a.title)),
            }
        });
        offline_items
    } else {
        Vec::new()
    };

    if matches!(order, ChapterOrderBy::Ascending) && items.len() > 1 {
        // Keep ascending order consistent if offline-only or mixed sources produced items
        let first_num = items.first().and_then(|c| c.chapter_number.trim().parse::<f64>().ok());
        let last_num = items.last().and_then(|c| c.chapter_number.trim().parse::<f64>().ok());
        if let (Some(f), Some(l)) = (first_num, last_num)
            && f > l
        {
            items.reverse();
        }
    }

    let read_count = items.iter().filter(|c| c.is_read).count() as u32;
    let offline_count = items.iter().filter(|c| c.is_downloaded).count() as u32;

    let next_unread_chapter = items
        .iter()
        .filter(|c| !c.is_read)
        .min_by(|a, b| match (a.chapter_number.trim().parse::<f64>(), b.chapter_number.trim().parse::<f64>()) {
            (Ok(na), Ok(nb)) => na.total_cmp(&nb),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => Ordering::Equal,
        })
        .cloned();

    (items, next_unread_chapter, read_count, offline_count)
}

fn genre_keywords(genre_id: &str) -> &'static [&'static str] {
    match genre_id {
        "shonen" => &["shonen", "shounen", "action", "acción", "accion", "adventure", "aventura", "martial arts"],
        "seinen" => &["seinen", "psychological", "psicológico", "psicologico", "drama", "thriller", "mature"],
        "romance" => &["romance", "romantic", "romántica", "romantica", "love", "shoujo", "josei"],
        "fantasy" => &["fantasy", "fantasía", "fantasia", "isekai", "magic", "magia", "supernatural"],
        "slice_of_life" => &["slice of life", "iyashikei", "school life", "vida cotidiana", "costumbrista"],
        "comedy" => &["comedy", "comedia", "parody", "gag"],
        "mystery" => &["mystery", "misterio", "detective", "suspense", "crime", "thriller"],
        "scifi" => &["sci-fi", "science fiction", "ciencia ficción", "ciencia ficcion", "mecha", "cyberpunk", "space"],
        "horror" => &["horror", "terror", "gore", "supernatural", "occult"],
        "sports" => &["sports", "deportes", "spokon", "football", "basketball", "baseball"],
        _ => &[],
    }
}

pub fn rank_for_you_items(items: Vec<CatalogItem>, favorite_genres: &[String]) -> (Vec<CatalogItem>, Vec<String>, bool) {
    let cleaned_genres: Vec<String> = favorite_genres
        .iter()
        .map(|g| g.trim().to_ascii_lowercase())
        .filter(|g| !g.is_empty())
        .collect();

    if cleaned_genres.is_empty() {
        return (items.into_iter().take(8).collect(), Vec::new(), true);
    }

    let mut matched_genres: Vec<String> = Vec::new();
    let mut scored: Vec<(usize, usize, CatalogItem)> = items
        .into_iter()
        .enumerate()
        .map(|(idx, item)| {
            let genres_text = item.genres.join(" ").to_lowercase();
            let extra_text = format!("{} {}", item.title, item.description.as_deref().unwrap_or("")).to_lowercase();
            let mut score = 0usize;

            for genre_id in &cleaned_genres {
                let keywords = genre_keywords(genre_id);
                let genre_hit = keywords.iter().any(|kw| genres_text.contains(kw)) || genres_text.contains(genre_id);
                let text_hit = keywords.iter().any(|kw| extra_text.contains(kw));
                if genre_hit {
                    score += 3;
                    if !matched_genres.contains(genre_id) {
                        matched_genres.push(genre_id.clone());
                    }
                } else if text_hit {
                    score += 1;
                    if !matched_genres.contains(genre_id) {
                        matched_genres.push(genre_id.clone());
                    }
                }
            }

            (score, idx, item)
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let any_match = scored.iter().any(|(score, _, _)| *score > 0);
    let top_items = scored.into_iter().take(8).map(|(_, _, item)| item).collect();
    (top_items, matched_genres, !any_match)
}

#[tauri::command]
pub fn get_initial_state(state: State<'_, AppState>) -> Result<InitialAppState, String> {
    state.snapshot()
}

#[tauri::command]
pub fn get_library_overview(state: State<'_, AppState>, search: Option<String>) -> Result<LibraryOverview, String> {
    state.get_library_overview(search.as_deref())
}

#[tauri::command]
pub fn toggle_library_favorite(state: State<'_, AppState>, payload: ToggleFavoritePayload) -> Result<ToggleFavoriteResult, String> {
    state.toggle_library_favorite(payload)
}

#[tauri::command]
pub fn toggle_chapter_read(
    state: State<'_, AppState>,
    payload: ToggleChapterReadPayload,
) -> Result<ToggleChapterReadResult, String> {
    state.toggle_chapter_read(payload)
}

#[tauri::command]
pub fn save_onboarding_preferences(
    state: State<'_, AppState>,
    payload: OnboardingPreferencesPayload,
) -> Result<InitialAppState, String> {
    state.save_onboarding_preferences(payload)
}

#[tauri::command]
pub fn set_ui_language(state: State<'_, AppState>, language: String) -> Result<InitialAppState, String> {
    state.set_ui_language(&language)
}

#[tauri::command]
pub fn set_theme_mode(state: State<'_, AppState>, theme_mode: String) -> Result<InitialAppState, String> {
    state.set_theme_mode(&theme_mode)
}

#[tauri::command]
pub fn set_default_provider(state: State<'_, AppState>, provider: String) -> Result<InitialAppState, String> {
    state.set_default_provider(&provider)
}

#[tauri::command]
pub fn set_favorite_genres(state: State<'_, AppState>, genres: Vec<String>) -> Result<InitialAppState, String> {
    state.set_favorite_genres(&genres)
}

#[tauri::command]
pub fn set_onboarding_completed(state: State<'_, AppState>, completed: bool) -> Result<InitialAppState, String> {
    state.set_onboarding_completed(completed)
}

#[tauri::command]
pub async fn open_reader_chapter(
    state: State<'_, AppState>,
    reader_store: State<'_, ReaderSessionStore>,
    request: OpenReaderChapterRequest,
) -> Result<ReaderSessionResponse, String> {
    resolve_and_open_chapter(&state, &reader_store, request).await
}

#[tauri::command]
pub fn save_reader_progress(
    state: State<'_, AppState>,
    payload: SaveReaderProgressPayload,
) -> Result<SaveReaderProgressResult, String> {
    save_progress_in_db(&state, payload)
}

#[tauri::command]
pub fn save_reader_prefs(state: State<'_, AppState>, prefs: ReaderPrefs) -> Result<InitialAppState, String> {
    state.save_reader_prefs(prefs)
}

#[tauri::command]
pub fn get_download_queue_snapshot(queue: State<'_, ManagedDownloadQueue>) -> Result<DownloadQueueSnapshotDto, String> {
    Ok(map_queue_snapshot(queue.snapshot()))
}

#[tauri::command]
pub fn enqueue_chapters_download(
    queue: State<'_, ManagedDownloadQueue>,
    payload: EnqueueChaptersRequest,
) -> Result<DownloadQueueSnapshotDto, String> {
    enqueue_chapters(&queue, payload)
}

#[tauri::command]
pub fn control_download_queue(
    queue: State<'_, ManagedDownloadQueue>,
    request: DownloadQueueControlRequest,
) -> Result<DownloadQueueSnapshotDto, String> {
    apply_queue_control(&queue, request)
}

#[tauri::command]
pub fn set_download_format(
    state: State<'_, AppState>,
    queue: State<'_, ManagedDownloadQueue>,
    format: String,
) -> Result<SetDownloadFormatResult, String> {
    let parsed = parse_download_type(&format)?;
    let updated_queue = queue.set_active_format(parsed)?;
    Ok(SetDownloadFormatResult {
        queue: map_queue_snapshot(updated_queue),
        app_state: state.snapshot()?,
    })
}

#[tauri::command]
pub fn list_offline_library(state: State<'_, AppState>) -> Result<Vec<OfflineSeriesItemDto>, String> {
    scan_offline_series(&state)
}

fn validate_external_https_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || !trimmed.starts_with("https://") {
        return Err("Only HTTPS URLs can be opened externally".to_string());
    }
    Ok(trimmed)
}

fn open_folder_in_file_manager(
    app: &tauri::AppHandle,
    dir: &std::path::Path,
    label: &str,
    create_if_missing: bool,
) -> Result<(), String> {
    #[cfg(desktop)]
    {
        if create_if_missing {
            std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {label} folder {}: {e}", dir.display()))?;
        }
        app.opener()
            .open_path(dir.to_string_lossy().to_string(), None::<&str>)
            .map_err(|e| format!("Could not open {label} folder {}: {e}", dir.display()))
    }
    #[cfg(mobile)]
    {
        let _ = (app, dir, label, create_if_missing);
        Err("Opening the file manager is not supported on mobile devices".to_string())
    }
}

#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let trimmed = validate_external_https_url(&url)?;
    app.opener()
        .open_url(trimmed, None::<&str>)
        .map_err(|e| format!("Could not open URL: {e}"))
}

#[tauri::command]
pub fn open_data_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.data_dir()?;
    open_folder_in_file_manager(&app, &dir, "data", false)
}

#[tauri::command]
pub fn open_downloads_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.downloads_dir()?;
    open_folder_in_file_manager(&app, &dir, "downloads", true)
}

#[tauri::command]
pub fn get_integrations_overview(state: State<'_, AppState>) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::build_integrations_overview(&state)
}

#[tauri::command]
pub async fn connect_google_drive(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    pending_oauth: State<'_, crate::integrations::PendingOAuthStore>,
    client_id: Option<String>,
    client_secret: Option<String>,
    use_deeplink: Option<bool>,
) -> Result<crate::integrations::OAuthStartResultDto, String> {
    crate::integrations::connect_google_drive(&app, &state, &pending_oauth, client_id, client_secret, use_deeplink.unwrap_or(false))
        .await
}

#[tauri::command]
pub fn connect_local_cloud_folder(
    state: State<'_, AppState>,
    folder_path: String,
    provider_label: Option<String>,
) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::connect_local_cloud_folder(&state, folder_path, provider_label)
}

#[tauri::command]
pub async fn run_cloud_sync_now(
    state: State<'_, AppState>,
    mode: String,
) -> Result<crate::integrations::CloudSyncActionResultDto, String> {
    crate::integrations::run_cloud_sync_now(&state, &mode).await
}

#[tauri::command]
pub fn disconnect_cloud(state: State<'_, AppState>) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::disconnect_cloud(&state)
}

#[tauri::command]
pub async fn save_anilist_credentials(
    state: State<'_, AppState>,
    client_id: String,
    access_token: String,
    verify: Option<bool>,
) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::save_anilist_credentials(&state, client_id, access_token, verify.unwrap_or(true)).await
}

#[tauri::command]
pub fn disconnect_anilist(state: State<'_, AppState>) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::disconnect_anilist(&state)
}

#[tauri::command]
pub async fn connect_myanimelist(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    pending_oauth: State<'_, crate::integrations::PendingOAuthStore>,
    client_id: Option<String>,
    client_secret: Option<String>,
    use_deeplink: Option<bool>,
) -> Result<crate::integrations::OAuthStartResultDto, String> {
    crate::integrations::connect_myanimelist(&app, &state, &pending_oauth, client_id, client_secret, use_deeplink.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn sync_myanimelist_now(state: State<'_, AppState>) -> Result<crate::integrations::MalSyncActionResultDto, String> {
    crate::integrations::sync_myanimelist_now(&state).await
}

#[tauri::command]
pub fn disconnect_myanimelist(state: State<'_, AppState>) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::disconnect_myanimelist(&state)
}

#[tauri::command]
pub async fn complete_oauth_callback(
    state: State<'_, AppState>,
    pending_oauth: State<'_, crate::integrations::PendingOAuthStore>,
    provider: String,
    callback_input: String,
) -> Result<crate::integrations::IntegrationsOverviewDto, String> {
    crate::integrations::complete_oauth_callback(&state, &pending_oauth, &provider, &callback_input).await
}

#[tauri::command]
pub async fn check_mangora_updates(state: State<'_, AppState>) -> Result<crate::integrations::UpdateCheckResultDto, String> {
    let snap = state.snapshot()?;
    crate::integrations::check_mangora_updates(&snap.app_version).await
}

#[tauri::command]
pub async fn submit_feedback(
    state: State<'_, AppState>,
    request: crate::feedback::SubmitFeedbackRequest,
) -> Result<crate::feedback::SubmitFeedbackResponse, String> {
    crate::feedback::submit_feedback_action(&state, request).await
}

#[tauri::command]
pub fn dismiss_support_banner(state: State<'_, AppState>) -> Result<InitialAppState, String> {
    state.dismiss_support_banner()
}

#[tauri::command]
pub fn open_manga_tui_desktop() -> Result<(), String> {
    crate::integrations::launch_manga_tui_desktop()
}

#[tauri::command]
pub fn export_library_backup_json(state: State<'_, AppState>) -> Result<String, String> {
    crate::integrations::export_library_backup_json(&state)
}

#[tauri::command]
pub fn export_library_backup_file(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    use tauri::Manager;
    let target_dir = app
        .path()
        .download_dir()
        .ok()
        .or_else(|| state.data_dir().ok())
        .ok_or_else(|| "Could not resolve Downloads directory".to_string())?;
    crate::integrations::export_library_backup_to_path(&state, &target_dir)
}

#[tauri::command]
pub fn import_library_backup_json(
    state: State<'_, AppState>,
    json_payload: String,
) -> Result<crate::integrations::PortableBackupImportResultDto, String> {
    crate::integrations::import_library_backup_json(&state, &json_payload)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use manga_core::backend::database::ChapterBookmarked;
    use manga_core::backend::offline::LocalChapterSource;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn validate_external_https_url_allows_only_https() {
        assert_eq!(
            validate_external_https_url("  https://github.com/tomahawk1103/manga-tui  ").unwrap(),
            "https://github.com/tomahawk1103/manga-tui"
        );
        for invalid in ["", "   ", "http://example.com", "javascript:alert(1)", "file:///etc/passwd"] {
            assert!(validate_external_https_url(invalid).is_err());
        }
    }

    #[test]
    fn reconcile_chapters_merges_read_offline_and_picks_next_unread() {
        let online = vec![
            Chapter {
                id: "ch-3".into(),
                manga_id: "m1".into(),
                title: "Capítulo 3".into(),
                language: Languages::Spanish,
                chapter_number: "3".into(),
                ..Default::default()
            },
            Chapter {
                id: "ch-2".into(),
                manga_id: "m1".into(),
                title: "Capítulo 2".into(),
                language: Languages::Spanish,
                chapter_number: "2".into(),
                ..Default::default()
            },
            Chapter {
                id: "ch-1".into(),
                manga_id: "m1".into(),
                title: "Capítulo 1".into(),
                language: Languages::Spanish,
                chapter_number: "1".into(),
                ..Default::default()
            },
        ];

        let db_statuses = vec![MangaReadingHistoryRetrieve {
            id: "ch-1".into(),
            is_downloaded: false,
            is_read: true,
        }];
        let read_by_title = HashSet::new();
        let downloaded_ids = HashSet::from(["ch-2".to_string()]);
        let bookmark = ChapterBookmarked {
            id: "ch-2".into(),
            number_page_bookmarked: Some(7),
            ..Default::default()
        };
        let offline = OfflineManga {
            manga_id: "m1".into(),
            title: "Serie".into(),
            folder_name: "Serie".into(),
            provider: Some(MangaProviders::Inmanga),
            is_favorite: false,
            chapters: vec![OfflineChapter {
                chapter: Chapter {
                    id: "ch-2".into(),
                    title: "Capítulo 2".into(),
                    chapter_number: "2".into(),
                    ..Default::default()
                },
                source: LocalChapterSource::Cbz(PathBuf::from("ch2.cbz")),
                format_label: "CBZ",
                size_bytes: 1024,
                is_read: false,
                is_bookmarked: false,
                bookmarked_page: None,
            }],
            total_size_bytes: 1024,
            languages: vec![Languages::Spanish],
        };

        let (items, next_unread, read_count, offline_count) = reconcile_chapters(
            online,
            &db_statuses,
            &read_by_title,
            &downloaded_ids,
            Some(&bookmark),
            Some(&offline),
            ChapterOrderBy::Descending,
        );

        assert_eq!(items.len(), 3);
        assert_eq!(read_count, 1);
        assert_eq!(offline_count, 1);
        assert_eq!(next_unread.as_ref().map(|c| c.id.as_str()), Some("ch-2"));
        assert!(items[1].is_downloaded);
        assert_eq!(items[1].offline_format.as_deref(), Some("CBZ"));
        assert!(items[1].is_bookmarked);
        assert_eq!(items[1].bookmarked_page, Some(7));
        assert!(items[2].is_read);
    }

    #[test]
    fn rank_for_you_items_prioritizes_favorite_genre_matches() {
        let items = vec![
            CatalogItem {
                id: "1".into(),
                provider: None,
                title: "Pure Sports".into(),
                description: None,
                cover_url: String::new(),
                genres: vec!["Sports".into()],
                languages: vec![],
            },
            CatalogItem {
                id: "2".into(),
                provider: None,
                title: "Dark Mystery".into(),
                description: Some("A psychological thriller".into()),
                cover_url: String::new(),
                genres: vec!["Seinen".into(), "Mystery".into()],
                languages: vec![],
            },
        ];

        let (ranked, matched, fallback) = rank_for_you_items(items, &["seinen".into(), "mystery".into()]);
        assert!(!fallback);
        assert_eq!(ranked[0].id, "2");
        assert_eq!(matched, vec!["seinen".to_string(), "mystery".to_string()]);
    }

    #[tokio::test]
    async fn catalog_rejects_invalid_input_before_querying_the_provider() {
        for (provider, query, page) in [("missing", "Monster", 1), ("mangadex", "  ", 1), ("mangadex", "Monster", 0)] {
            assert!(search_catalog(provider.into(), query.into(), page, None).await.is_err());
        }
        assert!(search_catalog("mangadex".into(), "  ".into(), 1, Some(vec!["  ".into()])).await.is_err());
        assert!(
            search_catalog("mangadex".into(), "Monster".into(), 0, Some(vec!["shonen".into()]))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "requires a live MangaDex connection"]
    async fn mangadex_catalog_returns_titles_and_covers() {
        let page = search_catalog("mangadex".into(), "Monster".into(), 1, None).await.unwrap();
        assert!(
            page.items
                .iter()
                .any(|item| !item.title.is_empty() && item.cover_url.starts_with("https://"))
        );
    }
}
