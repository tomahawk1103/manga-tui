/// <reference types="vite/client" />
import type { CatalogPage } from "../lib/tauri";
import type {
  AniListCommunityInfo,
  ForYouResponse,
  OpenReaderChapterRequest,
  ProviderOption,
  ReaderChapterNeighbor,
  ReaderSessionResponse,
  SeriesChapterItem,
  SeriesDetailResponse,
  SeriesSelection,
  UpdateCheckResultDto,
} from "../types/mangora";
import { getWebMangaChapterState, recordWebChapterOpen } from "./db";

export interface ChapterUpdate {
  id: string;
  provider?: string;
  title: string;
  cover?: string;
  published?: string;
  chapterId: string;
  chapterTitle: string;
}

interface LegacyChapter {
  id: string;
  title: string;
  number: string;
  manifest: string;
}

interface LegacySeries {
  id: string;
  title: string;
  cover?: string;
  description?: string | null;
  status?: string;
  genres?: string[];
  language: string;
  chapters: LegacyChapter[];
}

interface WorkerV1SeriesResponse {
  id: string;
  provider: string;
  title: string;
  coverUrl: string | null;
  description: string | null;
  author: string | null;
  artist: string | null;
  status: string;
  genres: string[];
  availableLanguages: string[];
  activeLanguage: string;
  order: "asc" | "desc";
  page: number;
  totalChapters: number;
  hasNextPage: boolean;
  chapters: Array<{
    id: string;
    title: string;
    chapterNumber: string;
    volumeNumber: string | null;
    language: string;
    scanlator: string | null;
    publicationDate: string | null;
  }>;
}

interface WorkerCapabilities {
  v1Supported: boolean;
  providers: string[];
}

const DEFAULT_PUBLIC_API = "https://mangora-lector-publico.chirpybroth8.chatgpt.site/";
const CHAPTERS_PER_PAGE = 50;
const MAX_WEB_SESSIONS = 12;

const apiBase =
  import.meta.env.VITE_MANGORA_API_URL ||
  import.meta.env.VITE_MANGAONI_API_URL ||
  (import.meta.env.MODE === "mangaoni" ? "./api/mangaoni/" : DEFAULT_PUBLIC_API);

const activeWebSessions = new Map<string, string[]>();
const sessionOrder: string[] = [];
let sessionSeq = 0;

const anilistCache = new Map<string, AniListCommunityInfo>();
let capabilitiesPromise: Promise<WorkerCapabilities> | null = null;
let capabilitiesCache: WorkerCapabilities | null = null;

function baseHref(): string {
  return typeof document !== "undefined" ? document.baseURI : "https://localhost/";
}

function apiUrl(endpoint: string): URL {
  return new URL(endpoint, new URL(apiBase.replace(/\/?$/, "/"), baseHref()));
}

function catalogJsonUrl(): URL {
  return new URL(import.meta.env.VITE_MANGORA_CATALOG_URL || "./api/catalog.json", baseHref());
}

async function fetchJson<T>(url: URL | string, init?: RequestInit, timeoutMs = 15000): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const externalSignal = init?.signal;
  const onAbort = () => controller.abort();
  if (externalSignal) {
    if (externalSignal.aborted) controller.abort();
    else externalSignal.addEventListener("abort", onAbort, { once: true });
  }
  try {
    const response = await fetch(url, {
      cache: "no-cache",
      ...init,
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(`El servidor respondió con el código ${response.status}.`);
    }
    return (await response.json()) as T;
  } finally {
    clearTimeout(timer);
    externalSignal?.removeEventListener("abort", onAbort);
  }
}

export async function detectWorkerCapabilities(): Promise<WorkerCapabilities> {
  if (capabilitiesCache) return capabilitiesCache;
  if (capabilitiesPromise) return capabilitiesPromise;
  capabilitiesPromise = (async () => {
    try {
      const data = await fetchJson<{ providers?: string[] }>(apiUrl("v1/providers"), undefined, 3500);
      if (Array.isArray(data.providers) && data.providers.length > 0) {
        capabilitiesCache = {
          v1Supported: true,
          providers: data.providers.map((p) => p.toLowerCase()),
        };
        return capabilitiesCache;
      }
    } catch {
      // Legacy worker without v1 routes or offline
    }
    capabilitiesCache = { v1Supported: false, providers: ["mangaoni"] };
    return capabilitiesCache;
  })();
  return capabilitiesPromise;
}

export function getCachedWebProviders(allProviders: ProviderOption[]): ProviderOption[] {
  if (!capabilitiesCache) {
    void detectWorkerCapabilities();
    const initialIds = import.meta.env.MODE === "mangaoni" ? new Set(["mangaoni", "mangadex"]) : new Set(["mangaoni"]);
    return allProviders.filter((p) => initialIds.has(p.id));
  }
  const supported = new Set(capabilitiesCache.providers);
  const filtered = allProviders.filter((p) => supported.has(p.id));
  return filtered.length > 0 ? filtered : allProviders.filter((p) => p.id === "mangaoni");
}

export async function resolveWebProviders(allProviders: ProviderOption[]): Promise<ProviderOption[]> {
  const caps = await detectWorkerCapabilities();
  const supported = new Set(caps.providers);
  const filtered = allProviders.filter((p) => supported.has(p.id));
  if (filtered.length > 0) return filtered;
  return allProviders.filter((p) => p.id === "mangaoni");
}

const MANGADEX_PUBLIC_API = "https://api.mangadex.org";
const ANILIST_GRAPHQL_URL = "https://graphql.anilist.co";

interface AniListSearchMeta {
  id: number;
  title: string;
  romaji: string | null;
  english: string | null;
  synonyms: string[];
  coverUrl: string;
  description: string | null;
  status: string;
  genres: string[];
}

const anilistSearchMetaCache = new Map<string, AniListSearchMeta>();

const ANILIST_GENRE_MAP: Record<string, string> = {
  action: "Action",
  adventure: "Adventure",
  comedy: "Comedy",
  drama: "Drama",
  ecchi: "Ecchi",
  fantasy: "Fantasy",
  horror: "Horror",
  mecha: "Mecha",
  mystery: "Mystery",
  psychological: "Psychological",
  romance: "Romance",
  scifi: "Sci-Fi",
  "sci-fi": "Sci-Fi",
  slice_of_life: "Slice of Life",
  "slice of life": "Slice of Life",
  sports: "Sports",
  supernatural: "Supernatural",
};

const ANILIST_TAG_MAP: Record<string, string> = {
  shonen: "Shounen",
  shounen: "Shounen",
  seinen: "Seinen",
  shojo: "Shoujo",
  shoujo: "Shoujo",
  josei: "Josei",
  isekai: "Isekai",
  martial_arts: "Martial Arts",
  "martial arts": "Martial Arts",
  historical: "Historical",
  harem: "Female Harem",
};

function stripAniListHtml(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const cleaned = raw
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<[^>]*>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return cleaned || null;
}

function mapAniListStatus(raw: string | null | undefined): string {
  const key = (raw ?? "").toUpperCase();
  if (key === "FINISHED") return "completed";
  if (key === "RELEASING") return "ongoing";
  if (key === "HIATUS") return "hiatus";
  if (key === "CANCELLED") return "cancelled";
  return "ongoing";
}

function resolveDirectMdCover(mangaId: string, fileName: string | undefined, size: 256 | 512, useProxy: boolean): string {
  if (!fileName) return "";
  const raw = `https://uploads.mangadex.org/covers/${mangaId}/${fileName}.${size}.jpg`;
  if (!useProxy) return raw;
  const proxy = apiUrl("v1/image");
  proxy.searchParams.set("url", raw);
  return proxy.href;
}

function pickMdText(map: Record<string, string> | undefined, preferred: string[]): string {
  if (!map) return "";
  for (const key of preferred) {
    const val = map[key]?.trim();
    if (val) return val;
  }
  const first = Object.values(map).find((v) => typeof v === "string" && v.trim());
  return first?.trim() ?? "";
}

const ANILIST_TAG_SEARCH_QUERY = `
query ($page: Int, $perPage: Int, $search: String, $genres: [String], $tags: [String], $sort: [MediaSort]) {
  Page(page: $page, perPage: $perPage) {
    pageInfo {
      hasNextPage
    }
    media(type: MANGA, sort: $sort, search: $search, genre_in: $genres, tag_in: $tags, isAdult: false) {
      id
      title {
        romaji
        english
        native
      }
      synonyms
      description(asHtml: false)
      status
      genres
      coverImage {
        large
        extraLarge
      }
    }
  }
}
`;

const ANILIST_MEDIA_BY_ID_QUERY = `
query ($id: Int) {
  Media(id: $id, type: MANGA) {
    id
    title {
      romaji
      english
      native
    }
    synonyms
    description(asHtml: false)
    status
    genres
    coverImage {
      large
      extraLarge
    }
  }
}
`;

interface RawAniListMedia {
  id: number;
  title?: { romaji?: string | null; english?: string | null; native?: string | null };
  synonyms?: string[] | null;
  description?: string | null;
  status?: string | null;
  genres?: string[] | null;
  coverImage?: { large?: string | null; extraLarge?: string | null };
}

function cacheAniListMedia(media: RawAniListMedia): AniListSearchMeta {
  const romaji = media.title?.romaji?.trim() || null;
  const english = media.title?.english?.trim() || null;
  const native = media.title?.native?.trim() || null;
  const title = english || romaji || native || `AniList #${media.id}`;
  const meta: AniListSearchMeta = {
    id: media.id,
    title,
    romaji,
    english,
    synonyms: (media.synonyms ?? []).map((s) => s.trim()).filter(Boolean),
    coverUrl: media.coverImage?.extraLarge || media.coverImage?.large || "",
    description: stripAniListHtml(media.description),
    status: mapAniListStatus(media.status),
    genres: (media.genres ?? []).map((g) => g.trim()).filter(Boolean),
  };
  anilistSearchMetaCache.set(`anilist:${media.id}`, meta);
  return meta;
}

async function fetchAniListMediaById(id: number): Promise<AniListSearchMeta | null> {
  const cacheKey = `anilist:${id}`;
  const cached = anilistSearchMetaCache.get(cacheKey);
  if (cached) return cached;
  try {
    const payload = await fetchJson<{ data?: { Media?: RawAniListMedia | null } }>(
      ANILIST_GRAPHQL_URL,
      {
        method: "POST",
        headers: { "Content-Type": "application/json", Accept: "application/json" },
        body: JSON.stringify({ query: ANILIST_MEDIA_BY_ID_QUERY, variables: { id } }),
      },
      10000,
    );
    if (!payload.data?.Media) return null;
    return cacheAniListMedia(payload.data.Media);
  } catch {
    return null;
  }
}

async function aniListTagSearch(
  query: string,
  page: number,
  tags: string[],
  uiLanguage: "es" | "en",
): Promise<CatalogPage> {
  const trimmed = query.trim();
  const genres: string[] = [];
  const tagNames: string[] = [];
  for (const raw of tags) {
    const norm = raw.trim().toLowerCase();
    if (!norm) continue;
    const mappedGenre = ANILIST_GENRE_MAP[norm];
    if (mappedGenre && !genres.includes(mappedGenre)) {
      genres.push(mappedGenre);
      continue;
    }
    const mappedTag = ANILIST_TAG_MAP[norm];
    if (mappedTag && !tagNames.includes(mappedTag)) {
      tagNames.push(mappedTag);
    }
  }

  const variables: Record<string, unknown> = {
    page: Math.max(1, page),
    perPage: 20,
    sort: trimmed ? ["SEARCH_MATCH"] : ["POPULARITY_DESC"],
  };
  if (trimmed) variables.search = trimmed;
  if (genres.length > 0) variables.genres = genres;
  if (tagNames.length > 0) variables.tags = tagNames;

  const payload = await fetchJson<{
    data?: {
      Page?: {
        pageInfo?: { hasNextPage?: boolean };
        media?: RawAniListMedia[];
      };
    };
  }>(
    ANILIST_GRAPHQL_URL,
    {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({ query: ANILIST_TAG_SEARCH_QUERY, variables }),
    },
    12000,
  );

  const pageData = payload.data?.Page;
  const items = (pageData?.media ?? []).map((raw) => {
    const meta = cacheAniListMedia(raw);
    return {
      id: `anilist:${meta.id}`,
      provider: "anilist",
      title: meta.title,
      description: meta.description,
      coverUrl: meta.coverUrl,
      genres: meta.genres,
      languages: [uiLanguage],
    };
  });
  return {
    items,
    hasNext: Boolean(pageData?.pageInfo?.hasNextPage) && items.length > 0,
  };
}

async function aniListRecentUpdates(
  page: number,
  language: "es" | "en",
  signal: AbortSignal,
): Promise<{ items: ChapterUpdate[]; hasMore: boolean }> {
  const payload = await fetchJson<{
    data?: {
      Page?: {
        pageInfo?: { hasNextPage?: boolean };
        media?: RawAniListMedia[];
      };
    };
  }>(
    ANILIST_GRAPHQL_URL,
    {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({
        query: ANILIST_TAG_SEARCH_QUERY,
        variables: { page: Math.max(1, page), perPage: 16, sort: ["TRENDING_DESC"] },
      }),
      signal,
    },
    12000,
  );
  const pageData = payload.data?.Page;
  const items: ChapterUpdate[] = (pageData?.media ?? []).map((raw) => {
    const meta = cacheAniListMedia(raw);
    return {
      id: `anilist:${meta.id}`,
      provider: "anilist",
      title: meta.title,
      cover: meta.coverUrl || undefined,
      chapterId: "open-series",
      chapterTitle: language === "en" ? "View chapters" : "Ver capítulos",
    };
  });
  return {
    items,
    hasMore: Boolean(pageData?.pageInfo?.hasNextPage) && items.length > 0,
  };
}

export async function loadUpdates(signal: AbortSignal): Promise<ChapterUpdate[]> {
  const response = await fetch(apiUrl("updates"), { signal });
  if (!response.ok) throw new Error(String(response.status));
  return (await response.json()) as ChapterUpdate[];
}

export async function loadUpdatesPage(
  page: number,
  language: "es" | "en",
  signal: AbortSignal,
): Promise<{ items: ChapterUpdate[]; hasMore: boolean }> {
  const caps = await detectWorkerCapabilities();
  if (page === 1) {
    const items = await loadUpdates(signal);
    return {
      items: items.map((entry) => ({ ...entry, provider: entry.provider ?? "mangaoni" })),
      hasMore: true,
    };
  }

  if (caps.v1Supported) {
    try {
      const url = apiUrl("v1/updates");
      url.searchParams.set("provider", "mangaoni");
      url.searchParams.set("page", String(page));
      url.searchParams.set("lang", language);
      const data = await fetchJson<{ items?: ChapterUpdate[]; hasMore?: boolean } | ChapterUpdate[]>(url, { signal }, 12000);
      if (!Array.isArray(data) && Array.isArray(data.items)) {
        return { items: data.items, hasMore: Boolean(data.hasMore) };
      }
    } catch {
      // Fallback to CORS-friendly AniList trending feed if deployed worker predates paginated v1/updates
    }
  }

  return aniListRecentUpdates(Math.max(1, page - 1), language, signal);
}

function normalizeLegacySeriesId(rawId: string): string {
  return rawId === "mangaoni-dandadan" ? "manga/dandadan" : rawId;
}

function normalizeChapterCompoundId(seriesId: string, rawChapterId: string): string {
  return rawChapterId.includes("|") ? rawChapterId : `${seriesId}|${rawChapterId}`;
}

function splitChapterCompoundId(seriesId: string, chapterId: string): { mangaId: string; chapterId: string } {
  if (chapterId.includes("|")) {
    const [left, right] = chapterId.split("|", 2);
    return { mangaId: left || seriesId, chapterId: right };
  }
  return { mangaId: seriesId, chapterId };
}

function compareChapterNumber(a: string, b: string, order: "asc" | "desc"): number {
  const na = Number.parseFloat(a);
  const nb = Number.parseFloat(b);
  const fa = Number.isFinite(na);
  const fb = Number.isFinite(nb);
  if (fa && fb) return order === "asc" ? na - nb : nb - na;
  if (fa) return -1;
  if (fb) return 1;
  return order === "asc" ? a.localeCompare(b) : b.localeCompare(a);
}

function pickNextUnread(chapters: SeriesChapterItem[]): SeriesChapterItem | null {
  const unread = chapters.filter((ch) => !ch.isRead);
  if (unread.length === 0) return null;
  let best = unread[0];
  let bestNum = Number.parseFloat(best.chapterNumber);
  for (let idx = 1; idx < unread.length; idx += 1) {
    const cand = unread[idx];
    const candNum = Number.parseFloat(cand.chapterNumber);
    if (Number.isFinite(candNum) && (!Number.isFinite(bestNum) || candNum < bestNum)) {
      best = cand;
      bestNum = candNum;
    }
  }
  return best;
}

function providerBadge(provider: string): string {
  if (provider === "mangadex") return "MangaDex 🌐";
  if (provider === "mangaoni") return "MangaOni 🇪🇸";
  return provider;
}

export async function webSearchCatalog(
  provider: string,
  query: string,
  page: number,
  tags: string[] = [],
  uiLanguage: "es" | "en" = "es",
): Promise<CatalogPage> {
  const trimmed = query.trim();
  const cleanedTags = tags.map((t) => t.trim()).filter(Boolean);
  if (!trimmed && cleanedTags.length === 0) return { items: [], hasNext: false };

  const caps = await detectWorkerCapabilities();
  const normProvider = provider.trim().toLowerCase() || "mangaoni";

  if (caps.v1Supported && (caps.providers.includes(normProvider) || cleanedTags.length > 0)) {
    try {
      const url = apiUrl("v1/search");
      url.searchParams.set("provider", normProvider);
      if (trimmed) url.searchParams.set("q", trimmed);
      if (cleanedTags.length > 0) {
        url.searchParams.set("tags", cleanedTags.join(","));
        url.searchParams.set("lang", uiLanguage);
      }
      url.searchParams.set("page", String(page));
      return await fetchJson<CatalogPage>(url);
    } catch (err) {
      if (cleanedTags.length === 0 && normProvider !== "mangadex") {
        throw err;
      }
    }
  }

  if (cleanedTags.length > 0 || normProvider === "mangadex") {
    return aniListTagSearch(trimmed, page, cleanedTags, uiLanguage);
  }

  try {
    const url = apiUrl("search");
    url.searchParams.set("q", trimmed);
    const items = await fetchJson<LegacySeries[]>(url);
    return {
      items: items.map((item) => ({
        id: item.id,
        provider: "mangaoni",
        title: item.title,
        description: null,
        coverUrl: item.cover ?? "",
        genres: [],
        languages: [item.language || "es"],
      })),
      hasNext: false,
    };
  } catch (err) {
    // Fallback to static sample catalog if network/worker is unreachable
    const sampleItems = await fetchJson<LegacySeries[]>(catalogJsonUrl()).catch(() => []);
    const lower = trimmed.toLocaleLowerCase("es");
    const matched = sampleItems.filter((item) => item.title.toLocaleLowerCase("es").includes(lower));
    if (matched.length > 0) {
      return {
        items: matched.map((item) => ({
          id: item.id,
          provider: "mangaoni",
          title: item.title,
          description: null,
          coverUrl: item.cover ? new URL(item.cover, catalogJsonUrl()).href : "",
          genres: [],
          languages: [item.language || "es"],
        })),
        hasNext: false,
      };
    }
    throw err;
  }
}

export async function webGetForYouRecommendations(
  provider: string,
  favoriteGenres: string[],
  uiLanguage: "es" | "en",
): Promise<ForYouResponse> {
  const normProvider = provider.trim().toLowerCase() || "mangaoni";
  const caps = await detectWorkerCapabilities();
  const label = providerBadge(normProvider);

  if (caps.v1Supported && caps.providers.includes(normProvider)) {
    const url = apiUrl("v1/popular");
    url.searchParams.set("provider", normProvider);
    url.searchParams.set("lang", uiLanguage);
    const data = await fetchJson<{ items: CatalogPage["items"] }>(url);
    const items = [...data.items];
    const matchedGenres = favoriteGenres.filter((g) =>
      items.some((item) => item.genres.some((ig) => ig.toLowerCase().includes(g.toLowerCase()))),
    );
    if (matchedGenres.length > 0) {
      items.sort((a, b) => {
        const scoreA = a.genres.filter((g) => matchedGenres.some((m) => g.toLowerCase().includes(m.toLowerCase()))).length;
        const scoreB = b.genres.filter((g) => matchedGenres.some((m) => g.toLowerCase().includes(m.toLowerCase()))).length;
        return scoreB - scoreA;
      });
    }
    return {
      provider: normProvider,
      providerLabel: label,
      matchedGenres,
      items,
      fallbackPopular: matchedGenres.length === 0,
    };
  }

  const controller = new AbortController();
  const updates = await loadUpdates(controller.signal);
  const seen = new Set<string>();
  const items: ForYouResponse["items"] = [];
  for (const entry of updates) {
    if (seen.has(entry.id)) continue;
    seen.add(entry.id);
    items.push({
      id: entry.id,
      title: entry.title,
      description: null,
      coverUrl: entry.cover ?? "",
      genres: [],
      languages: ["es"],
    });
  }
  return {
    provider: normProvider,
    providerLabel: label,
    matchedGenres: [],
    items,
    fallbackPopular: true,
  };
}

function normalizeMatchTitle(raw: string): string {
  return raw
    .toLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function buildMangaOniSearchCandidates(meta: AniListSearchMeta | null, fallbackTitle: string): string[] {
  const rawList = [
    meta?.english,
    meta?.romaji,
    meta?.title,
    fallbackTitle,
    ...(meta?.synonyms ?? []).slice(0, 2),
  ]
    .map((s) => s?.trim())
    .filter((s): s is string => Boolean(s));

  const out: string[] = [];
  const seen = new Set<string>();
  const push = (candidate: string) => {
    const cleaned = candidate.trim();
    const key = cleaned.toLowerCase();
    if (cleaned.length >= 2 && !seen.has(key)) {
      seen.add(key);
      out.push(cleaned);
    }
  };

  for (const title of rawList) {
    push(title);
    const noPunct = title.replace(/[-:–—/!¡?¿()[\]]+/g, " ").replace(/\s+/g, " ").trim();
    push(noPunct);
    const words = noPunct.split(" ").filter(Boolean);
    if (words.length > 3) {
      push(words.slice(0, 3).join(" "));
    }
  }
  return out.slice(0, 4);
}

async function resolveMangaOniSeriesForAniList(
  meta: AniListSearchMeta | null,
  fallbackTitle: string,
): Promise<LegacySeries | null> {
  const targetNames = [meta?.english, meta?.romaji, meta?.title, fallbackTitle, ...(meta?.synonyms ?? [])]
    .map((s) => (s ? normalizeMatchTitle(s) : ""))
    .filter(Boolean);
  const candidates = buildMangaOniSearchCandidates(meta, fallbackTitle);

  for (const q of candidates) {
    try {
      const url = apiUrl("search");
      url.searchParams.set("q", q);
      const hits = await fetchJson<LegacySeries[]>(url, undefined, 9000);
      if (!Array.isArray(hits) || hits.length === 0) continue;

      const exact = hits.find((hit) => targetNames.includes(normalizeMatchTitle(hit.title)));
      if (exact) return exact;

      const partial = hits.find((hit) => {
        const normHit = normalizeMatchTitle(hit.title);
        return targetNames.some((t) => normHit.includes(t) || t.includes(normHit));
      });
      if (partial) return partial;

      return hits[0];
    } catch {
      // Try next query variation
    }
  }
  return null;
}

async function fetchLegacyMangaOniDetail(
  mangaId: string,
  selection: SeriesSelection,
  normProvider: string,
  order: "asc" | "desc",
  page: number,
): Promise<SeriesDetailResponse> {
  const url = apiUrl("series");
  url.searchParams.set("id", mangaId);
  const raw = await fetchJson<LegacySeries>(url);
  const dbState = await getWebMangaChapterState(normProvider, raw.id);
  const sorted = [...raw.chapters].sort((a, b) => compareChapterNumber(a.number, b.number, order));
  const offset = (page - 1) * CHAPTERS_PER_PAGE;
  const sliced = sorted.slice(offset, offset + CHAPTERS_PER_PAGE);

  const chapters: SeriesChapterItem[] = sliced.map((ch) => {
    const compoundId = normalizeChapterCompoundId(raw.id, ch.id);
    const saved = dbState.chaptersByKey.get(compoundId) ?? dbState.chaptersByKey.get(ch.id);
    return {
      id: compoundId,
      title: ch.title,
      chapterNumber: ch.number || "?",
      volumeNumber: null,
      language: raw.language || "es",
      scanlator: "MangaOni",
      publicationDate: null,
      isRead: Boolean(saved?.isRead),
      isDownloaded: false,
      isAvailableOnline: true,
      isBookmarked: Boolean(saved?.isBookmarked),
      bookmarkedPage: saved?.numberPageBookmarked ?? null,
      offlineFormat: null,
    };
  });

  return {
    id: raw.id,
    provider: normProvider,
    providerLabel: providerBadge(normProvider),
    title: raw.title || selection.title,
    coverUrl: raw.cover ?? selection.coverUrl ?? null,
    description: raw.description ?? selection.description ?? null,
    author: null,
    artist: null,
    status: raw.status || "ongoing",
    genres: raw.genres ?? [],
    availableLanguages: ["es"],
    activeLanguage: "es",
    order,
    page,
    totalChapters: sorted.length,
    hasNextPage: offset + sliced.length < sorted.length,
    isFavorite: dbState.isFavorite,
    isOfflineFallback: false,
    offlineChapterCount: 0,
    readChapterCount: Array.from(dbState.chaptersByKey.values()).filter((c) => c.isRead).length,
    nextUnreadChapter: pickNextUnread(chapters),
    chapters,
    onlineWarning: null,
  };
}

export async function webGetSeriesDetail(
  selection: SeriesSelection,
  options?: {
    language?: string;
    order?: "asc" | "desc";
    page?: number;
  },
): Promise<SeriesDetailResponse> {
  const normProvider = selection.provider.trim().toLowerCase() || "mangaoni";
  const mangaId = normalizeLegacySeriesId(selection.mangaId);
  const order = options?.order ?? "desc";
  const page = Math.max(1, options?.page ?? 1);

  if (mangaId === "mangora-sample") {
    const sampleList = await fetchJson<LegacySeries[]>(catalogJsonUrl());
    const sample = sampleList.find((item) => item.id === "mangora-sample");
    if (!sample) throw new Error("Serie de muestra no disponible.");
    const dbState = await getWebMangaChapterState(normProvider, mangaId);
    const chapters: SeriesChapterItem[] = sample.chapters.map((ch) => {
      const saved = dbState.chaptersByKey.get(ch.id);
      return {
        id: ch.id,
        title: ch.title,
        chapterNumber: ch.number,
        volumeNumber: null,
        language: sample.language || "es",
        scanlator: "Mangora",
        publicationDate: null,
        isRead: Boolean(saved?.isRead),
        isDownloaded: false,
        isAvailableOnline: true,
        isBookmarked: Boolean(saved?.isBookmarked),
        bookmarkedPage: saved?.numberPageBookmarked ?? null,
        offlineFormat: null,
      };
    });
    return {
      id: sample.id,
      provider: normProvider,
      providerLabel: "Mangora",
      title: sample.title,
      coverUrl: sample.cover ? new URL(sample.cover, catalogJsonUrl()).href : selection.coverUrl ?? null,
      description: selection.description ?? "Capítulo de muestra integrado para pruebas rápidas.",
      author: null,
      artist: null,
      status: "completed",
      genres: [],
      availableLanguages: ["es"],
      activeLanguage: "es",
      order,
      page: 1,
      totalChapters: chapters.length,
      hasNextPage: false,
      isFavorite: dbState.isFavorite,
      isOfflineFallback: false,
      offlineChapterCount: 0,
      readChapterCount: chapters.filter((c) => c.isRead).length,
      nextUnreadChapter: pickNextUnread(chapters),
      chapters,
      onlineWarning: null,
    };
  }

  const caps = await detectWorkerCapabilities();

  if (caps.v1Supported && caps.providers.includes(normProvider)) {
    const url = apiUrl("v1/series");
    url.searchParams.set("provider", normProvider);
    url.searchParams.set("id", mangaId);
    url.searchParams.set("order", order);
    url.searchParams.set("page", String(page));
    if (options?.language) url.searchParams.set("lang", options.language);

    const raw = await fetchJson<WorkerV1SeriesResponse>(url);
    const dbState = await getWebMangaChapterState(normProvider, raw.id);
    const chapters: SeriesChapterItem[] = raw.chapters.map((ch) => {
      const saved =
        dbState.chaptersByKey.get(ch.id) ??
        (ch.id.includes("|") ? dbState.chaptersByKey.get(ch.id.split("|")[1]) : undefined);
      return {
        ...ch,
        isRead: Boolean(saved?.isRead),
        isDownloaded: false,
        isAvailableOnline: true,
        isBookmarked: Boolean(saved?.isBookmarked),
        bookmarkedPage: saved?.numberPageBookmarked ?? null,
        offlineFormat: null,
      };
    });

    return {
      id: raw.id,
      provider: normProvider,
      providerLabel: providerBadge(normProvider),
      title: raw.title || selection.title,
      coverUrl: raw.coverUrl ?? selection.coverUrl ?? null,
      description: raw.description ?? selection.description ?? null,
      author: raw.author,
      artist: raw.artist,
      status: raw.status || "unknown",
      genres: raw.genres ?? [],
      availableLanguages: raw.availableLanguages ?? [raw.activeLanguage || "es"],
      activeLanguage: raw.activeLanguage || "es",
      order: raw.order,
      page: raw.page,
      totalChapters: raw.totalChapters,
      hasNextPage: raw.hasNextPage,
      isFavorite: dbState.isFavorite,
      isOfflineFallback: false,
      offlineChapterCount: 0,
      readChapterCount: chapters.filter((c) => c.isRead).length,
      nextUnreadChapter: pickNextUnread(chapters),
      chapters,
      onlineWarning: null,
    };
  }

  if (normProvider === "mangadex") {
    const metaParams = new URLSearchParams();
    metaParams.append("includes[]", "cover_art");
    metaParams.append("includes[]", "author");
    metaParams.append("includes[]", "artist");
    const metaJson = await fetchJson<{
      data: {
        id: string;
        attributes?: {
          title?: Record<string, string>;
          altTitles?: Array<Record<string, string>>;
          description?: Record<string, string>;
          status?: string;
          availableTranslatedLanguages?: string[];
          tags?: Array<{ attributes?: { name?: Record<string, string> } }>;
        };
        relationships?: Array<{ type: string; attributes?: { fileName?: string; name?: string } }>;
      };
    }>(`${MANGADEX_PUBLIC_API}/manga/${encodeURIComponent(mangaId)}?${metaParams.toString()}`);

    const manga = metaJson.data;
    const attrs = manga.attributes ?? {};
    const availableLanguages = Array.from(new Set((attrs.availableTranslatedLanguages ?? []).filter(Boolean)));
    const preferredLang =
      (options?.language && (availableLanguages.length === 0 || availableLanguages.includes(options.language))
        ? options.language
        : null) ||
      ["es", "es-la", "en"].find((code) => availableLanguages.includes(code)) ||
      availableLanguages[0] ||
      "es";

    const offset = Math.max(0, (page - 1) * CHAPTERS_PER_PAGE);
    const feedParams = new URLSearchParams({
      limit: String(CHAPTERS_PER_PAGE),
      offset: String(offset),
      "order[chapter]": order,
    });
    feedParams.append("translatedLanguage[]", preferredLang);
    feedParams.append("contentRating[]", "safe");
    feedParams.append("contentRating[]", "suggestive");
    feedParams.append("includes[]", "scanlation_group");
    const feedJson = await fetchJson<{
      total?: number;
      data?: Array<{
        id: string;
        attributes?: {
          title?: string | null;
          chapter?: string | null;
          volume?: string | null;
          translatedLanguage?: string;
          publishAt?: string | null;
        };
        relationships?: Array<{ type: string; attributes?: { name?: string } }>;
      }>;
    }>(`${MANGADEX_PUBLIC_API}/manga/${encodeURIComponent(mangaId)}/feed?${feedParams.toString()}`);

    const dbState = await getWebMangaChapterState(normProvider, manga.id);
    const chapters: SeriesChapterItem[] = (feedJson.data ?? []).map((ch) => {
      const saved = dbState.chaptersByKey.get(ch.id);
      return {
        id: ch.id,
        title: ch.attributes?.title?.trim() ?? "",
        chapterNumber: ch.attributes?.chapter?.trim() || "0",
        volumeNumber: ch.attributes?.volume?.trim() || null,
        language: ch.attributes?.translatedLanguage ?? preferredLang,
        scanlator: ch.relationships?.find((rel) => rel.type === "scanlation_group")?.attributes?.name ?? null,
        publicationDate: ch.attributes?.publishAt ? ch.attributes.publishAt.slice(0, 10) : null,
        isRead: Boolean(saved?.isRead),
        isDownloaded: false,
        isAvailableOnline: true,
        isBookmarked: Boolean(saved?.isBookmarked),
        bookmarkedPage: saved?.numberPageBookmarked ?? null,
        offlineFormat: null,
      };
    });

    const totalChapters = feedJson.total ?? chapters.length;
    const coverFile = manga.relationships?.find((rel) => rel.type === "cover_art")?.attributes?.fileName;
    const title =
      pickMdText(attrs.title, ["es", "es-la", "en"]) ||
      attrs.altTitles?.map((alt) => pickMdText(alt, ["es", "es-la", "en"])).find(Boolean) ||
      selection.title;

    return {
      id: manga.id,
      provider: normProvider,
      providerLabel: providerBadge(normProvider),
      title,
      coverUrl: resolveDirectMdCover(manga.id, coverFile, 512, caps.v1Supported) || selection.coverUrl || null,
      description: pickMdText(attrs.description, ["es", "es-la", "en"]) || selection.description || null,
      author: manga.relationships?.find((rel) => rel.type === "author")?.attributes?.name ?? null,
      artist: manga.relationships?.find((rel) => rel.type === "artist")?.attributes?.name ?? null,
      status: attrs.status ?? "unknown",
      genres: (attrs.tags ?? []).map((t) => pickMdText(t.attributes?.name, ["en", "es"])).filter(Boolean),
      availableLanguages: availableLanguages.length > 0 ? availableLanguages : [preferredLang],
      activeLanguage: preferredLang,
      order,
      page,
      totalChapters,
      hasNextPage: offset + chapters.length < totalChapters,
      isFavorite: dbState.isFavorite,
      isOfflineFallback: false,
      offlineChapterCount: 0,
      readChapterCount: chapters.filter((c) => c.isRead).length,
      nextUnreadChapter: pickNextUnread(chapters),
      chapters,
      onlineWarning: null,
    };
  }

  if (normProvider === "anilist" || mangaId.startsWith("anilist:")) {
    const numericId = Number.parseInt(mangaId.replace(/^anilist:/i, ""), 10);
    const meta =
      anilistSearchMetaCache.get(mangaId) ??
      (Number.isFinite(numericId) ? await fetchAniListMediaById(numericId) : null);

    const matched = await resolveMangaOniSeriesForAniList(meta, selection.title);
    if (matched) {
      const detail = await fetchLegacyMangaOniDetail(matched.id, selection, "mangaoni", order, page);
      return {
        ...detail,
        title: detail.title || meta?.title || selection.title,
        coverUrl: meta?.coverUrl || selection.coverUrl || detail.coverUrl,
        description: meta?.description || detail.description || selection.description || null,
        genres: detail.genres.length > 0 ? detail.genres : meta?.genres ?? [],
      };
    }

    const dbState = await getWebMangaChapterState("anilist", mangaId);
    return {
      id: mangaId,
      provider: "anilist",
      providerLabel: "AniList",
      title: meta?.title || selection.title,
      coverUrl: meta?.coverUrl || selection.coverUrl || null,
      description: meta?.description || selection.description || null,
      author: null,
      artist: null,
      status: meta?.status || "ongoing",
      genres: meta?.genres ?? [],
      availableLanguages: ["es"],
      activeLanguage: "es",
      order,
      page: 1,
      totalChapters: 0,
      hasNextPage: false,
      isFavorite: dbState.isFavorite,
      isOfflineFallback: false,
      offlineChapterCount: 0,
      readChapterCount: 0,
      nextUnreadChapter: null,
      chapters: [],
      onlineWarning:
        "Esta serie se encontró en el catálogo de AniList, pero aún no está indexada en MangaOni Web. Usa la app de Mangora para Windows o Android para abrirla en cualquiera de los 7 proveedores.",
    };
  }

  // Legacy MangaOni worker endpoint (`series?id=...`)
  return fetchLegacyMangaOniDetail(mangaId, selection, normProvider, order, page);
}

function registerSessionPages(pages: string[]): string {
  sessionSeq += 1;
  const sessionId = `ws-${sessionSeq}`;
  activeWebSessions.set(sessionId, pages);
  sessionOrder.push(sessionId);
  while (sessionOrder.length > MAX_WEB_SESSIONS) {
    const evicted = sessionOrder.shift();
    if (evicted) activeWebSessions.delete(evicted);
  }
  return sessionId;
}

export function webBuildReaderPageUrl(sessionId: string, pageIndex: number): string {
  const pages = activeWebSessions.get(sessionId);
  if (pages && pages[pageIndex]) {
    return pages[pageIndex];
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="900" height="1350" viewBox="0 0 900 1350"><rect width="900" height="1350" fill="#f8f7f1"/><rect x="36" y="36" width="828" height="1278" fill="none" stroke="#191918" stroke-width="8"/><text x="450" y="640" text-anchor="middle" font-family="Arial, sans-serif" font-size="48" font-weight="900" fill="#191918">MANGORA WEB</text><text x="450" y="710" text-anchor="middle" font-family="monospace" font-size="28" fill="#cb3028">Página ${pageIndex + 1} no disponible</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function resolveNeighborsFromContext(
  currentId: string,
  currentNumber: string,
  context: ReaderChapterNeighbor[] | undefined,
): { prevChapter: ReaderChapterNeighbor | null; nextChapter: ReaderChapterNeighbor | null } {
  if (!context || context.length === 0) return { prevChapter: null, nextChapter: null };
  const sorted = [...context].sort((a, b) => compareChapterNumber(a.chapterNumber, b.chapterNumber, "asc"));
  const bareCurrent = currentId.includes("|") ? currentId.split("|")[1] : currentId;
  const idx = sorted.findIndex((item) => {
    const bareItem = item.id.includes("|") ? item.id.split("|")[1] : item.id;
    return item.id === currentId || bareItem === bareCurrent || (currentNumber && item.chapterNumber === currentNumber);
  });
  if (idx < 0) return { prevChapter: null, nextChapter: null };
  return {
    prevChapter: idx > 0 ? sorted[idx - 1] : null,
    nextChapter: idx + 1 < sorted.length ? sorted[idx + 1] : null,
  };
}

export async function webOpenReaderChapter(request: OpenReaderChapterRequest): Promise<ReaderSessionResponse> {
  const normProvider = request.provider.trim().toLowerCase() || "mangaoni";
  const mangaId = normalizeLegacySeriesId(request.mangaId);
  const rawChapterId = request.chapterId.trim();
  const normalizedChapterId =
    normProvider === "mangaoni" && mangaId !== "mangora-sample"
      ? normalizeChapterCompoundId(mangaId, rawChapterId)
      : rawChapterId;

  let pages: string[] = [];
  let resolvedChapterTitle = request.chapterTitle?.trim() || "";
  let resolvedChapterNumber = request.chapterNumber?.trim() || "";
  let resolvedMangaTitle = request.mangaTitle.trim() || mangaId;
  let resolvedCoverUrl = request.coverUrl ?? null;

  if (mangaId === "mangora-sample" || normalizedChapterId === "sample-1") {
    const manifestUrl = new URL("./sample-1.json", catalogJsonUrl());
    const manifest = await fetchJson<{ pages: string[] }>(manifestUrl);
    pages = manifest.pages.map((p) => new URL(p, manifestUrl).href);
    resolvedChapterTitle = resolvedChapterTitle || "Capítulo 1";
    resolvedChapterNumber = resolvedChapterNumber || "1";
    resolvedMangaTitle = resolvedMangaTitle || "Manga de muestra";
  } else {
    const caps = await detectWorkerCapabilities();
    if (caps.v1Supported && caps.providers.includes(normProvider)) {
      const url = apiUrl("v1/chapter");
      url.searchParams.set("provider", normProvider);
      url.searchParams.set("mangaId", mangaId);
      url.searchParams.set("chapterId", normalizedChapterId);
      const data = await fetchJson<{ pages: string[] }>(url);
      pages = data.pages;
    } else if (normProvider === "mangadex") {
      const data = await fetchJson<{
        baseUrl?: string;
        chapter?: { hash?: string; dataSaver?: string[]; data?: string[] };
      }>(`${MANGADEX_PUBLIC_API}/at-home/server/${encodeURIComponent(normalizedChapterId)}`);
      const baseUrl = data.baseUrl;
      const hash = data.chapter?.hash;
      const files = data.chapter?.dataSaver?.length ? data.chapter.dataSaver : data.chapter?.data;
      if (!baseUrl || !hash || !files?.length) {
        throw new Error("MangaDex no devolvió páginas disponibles para este capítulo");
      }
      const mode = data.chapter?.dataSaver?.length ? "data-saver" : "data";
      pages = files.map((file) => `${baseUrl}/${mode}/${hash}/${file}`);
    } else {
      const split = splitChapterCompoundId(mangaId, normalizedChapterId);
      const url = apiUrl("chapter");
      url.searchParams.set("id", split.mangaId);
      url.searchParams.set("chapter", split.chapterId);
      try {
        const data = await fetchJson<{ pages: string[] }>(url);
        pages = data.pages;
      } catch (err) {
        if (split.chapterId === "831345") {
          const fallbackUrl = new URL("./dandadan-254.json", catalogJsonUrl());
          const fallback = await fetchJson<{ pages: string[] }>(fallbackUrl);
          pages = fallback.pages;
        } else {
          throw err;
        }
      }
    }
  }

  if (!Array.isArray(pages) || pages.length === 0) {
    throw new Error("El servidor no devolvió páginas válidas para este capítulo.");
  }

  let context = request.chaptersContext;
  if ((!context || context.length === 0) && mangaId !== "mangora-sample") {
    try {
      const detail = await webGetSeriesDetail(
        {
          mangaId,
          provider: normProvider,
          title: resolvedMangaTitle,
          coverUrl: resolvedCoverUrl,
        },
        { order: "asc", page: 1 },
      );
      resolvedMangaTitle = detail.title || resolvedMangaTitle;
      resolvedCoverUrl = detail.coverUrl ?? resolvedCoverUrl;
      context = detail.chapters.map((ch) => ({
        id: ch.id,
        title: ch.title,
        chapterNumber: ch.chapterNumber,
        volumeNumber: ch.volumeNumber,
        language: ch.language,
        isDownloaded: false,
      }));
    } catch {
      // Chapter reading remains possible even if series neighbor list fails
    }
  }

  const matchedInContext = context?.find((c) => {
    const bareA = c.id.includes("|") ? c.id.split("|")[1] : c.id;
    const bareB = normalizedChapterId.includes("|") ? normalizedChapterId.split("|")[1] : normalizedChapterId;
    return c.id === normalizedChapterId || bareA === bareB;
  });

  if (!resolvedChapterTitle && matchedInContext?.title) resolvedChapterTitle = matchedInContext.title;
  if (!resolvedChapterNumber && matchedInContext?.chapterNumber) resolvedChapterNumber = matchedInContext.chapterNumber;
  if (!resolvedChapterNumber) {
    resolvedChapterNumber = resolvedChapterTitle.match(/\d+(?:\.\d+)?/)?.[0] ?? "?";
  }

  const sessionId = registerSessionPages(pages);
  const initialPage = await recordWebChapterOpen(
    {
      ...request,
      mangaId,
      provider: normProvider,
      mangaTitle: resolvedMangaTitle,
      coverUrl: resolvedCoverUrl,
      chapterId: normalizedChapterId,
      chapterTitle: resolvedChapterTitle,
      chapterNumber: resolvedChapterNumber,
    },
    pages.length,
  );

  const { prevChapter, nextChapter } = resolveNeighborsFromContext(
    normalizedChapterId,
    resolvedChapterNumber,
    context,
  );

  return {
    sessionId,
    mangaId,
    provider: normProvider,
    providerLabel: providerBadge(normProvider),
    mangaTitle: resolvedMangaTitle,
    coverUrl: resolvedCoverUrl,
    chapterId: normalizedChapterId,
    chapterTitle: resolvedChapterTitle,
    chapterNumber: resolvedChapterNumber,
    volumeNumber: request.volumeNumber ?? null,
    language: request.language ?? "es",
    pageCount: pages.length,
    initialPage,
    isOffline: false,
    offlineFormat: null,
    prevChapter,
    nextChapter,
  };
}

const ANILIST_COMMUNITY_QUERY = `
query ($search: String) {
  Media(search: $search, type: MANGA, sort: SEARCH_MATCH) {
    id
    siteUrl
    title {
      romaji
      english
      native
    }
    averageScore
    meanScore
    popularity
    favourites
    status
    genres
    tags {
      name
      rank
      isMediaSpoiler
      isGeneralSpoiler
    }
    reviews(sort: RATING_DESC, perPage: 5) {
      nodes {
        id
        summary
        rating
        ratingAmount
        score
        siteUrl
        user {
          name
        }
      }
    }
  }
}
`;

export async function webFetchAniListCommunity(title: string): Promise<AniListCommunityInfo> {
  const trimmed = title.trim();
  const empty: AniListCommunityInfo = {
    found: false,
    anilistId: null,
    siteUrl: null,
    title: null,
    averageScore: null,
    meanScore: null,
    popularity: null,
    favourites: null,
    status: null,
    genres: [],
    tags: [],
    reviews: [],
    note: "unavailable",
  };
  if (!trimmed) return { ...empty, note: "missing_title" };

  const cacheKey = trimmed.toLowerCase();
  const cached = anilistCache.get(cacheKey);
  if (cached) return cached;

  try {
    const payload = await fetchJson<{
      data?: {
        Media?: {
          id: number;
          siteUrl?: string;
          title?: { english?: string; romaji?: string; native?: string };
          averageScore?: number;
          meanScore?: number;
          popularity?: number;
          favourites?: number;
          status?: string;
          genres?: string[];
          tags?: Array<{
            name: string;
            rank?: number;
            isMediaSpoiler?: boolean;
            isGeneralSpoiler?: boolean;
          }>;
          reviews?: {
            nodes?: Array<{
              id: number;
              summary?: string;
              rating?: number;
              ratingAmount?: number;
              score?: number;
              siteUrl?: string;
              user?: { name?: string };
            }>;
          };
        } | null;
      };
    }>(
      "https://graphql.anilist.co",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "application/json",
        },
        body: JSON.stringify({
          query: ANILIST_COMMUNITY_QUERY,
          variables: { search: trimmed },
        }),
      },
      10000,
    );

    const media = payload.data?.Media;
    if (!media) {
      const notFound = { ...empty, note: "not_found" };
      anilistCache.set(cacheKey, notFound);
      return notFound;
    }

    const resolvedTitle =
      [media.title?.english, media.title?.romaji, media.title?.native]
        .map((s) => s?.trim())
        .find((s): s is string => Boolean(s)) ?? null;

    const result: AniListCommunityInfo = {
      found: true,
      anilistId: media.id,
      siteUrl: media.siteUrl?.startsWith("https://") ? media.siteUrl : null,
      title: resolvedTitle,
      averageScore: media.averageScore ?? null,
      meanScore: media.meanScore ?? null,
      popularity: media.popularity ?? null,
      favourites: media.favourites ?? null,
      status: media.status ?? null,
      genres: (media.genres ?? []).map((g) => g.trim()).filter(Boolean),
      tags: (media.tags ?? [])
        .filter((t) => !t.isMediaSpoiler && !t.isGeneralSpoiler && t.name?.trim())
        .slice(0, 8)
        .map((t) => ({ name: t.name.trim(), rank: t.rank ?? null })),
      reviews: (media.reviews?.nodes ?? [])
        .map((r) => ({
          id: r.id,
          summary: (r.summary ?? "").replace(/\s+/g, " ").trim(),
          rating: r.rating ?? null,
          ratingAmount: r.ratingAmount ?? null,
          score: r.score ?? null,
          userName: r.user?.name?.trim() || "AniList Reader",
          siteUrl: r.siteUrl?.startsWith("https://") ? r.siteUrl : null,
        }))
        .filter((r) => r.summary.length > 0)
        .slice(0, 5),
      note: null,
    };

    anilistCache.set(cacheKey, result);
    return result;
  } catch {
    return empty;
  }
}

export async function webCheckMangoraUpdates(currentVersion: string): Promise<UpdateCheckResultDto> {
  const fallback: UpdateCheckResultDto = {
    currentVersion,
    latestVersion: `mangora-v${currentVersion}`,
    updateAvailable: false,
    releaseUrl: "https://github.com/tomahawk1103/manga-tui/releases",
    androidApkUrl: null,
    releaseNotes: "",
    publishedAt: null,
    checkedAt: new Date().toISOString(),
  };

  try {
    const releases = await fetchJson<
      Array<{
        tag_name: string;
        draft?: boolean;
        prerelease?: boolean;
        html_url?: string;
        body?: string;
        published_at?: string;
        assets?: Array<{ name: string; browser_download_url: string }>;
      }>
    >(
      "https://api.github.com/repos/tomahawk1103/manga-tui/releases?per_page=10",
      {
        headers: {
          Accept: "application/vnd.github+json",
        },
      },
      8000,
    );

    const active =
      releases.find((r) => !r.draft && !r.prerelease && r.tag_name?.toLowerCase().startsWith("mangora-v")) ??
      releases.find((r) => !r.draft && r.tag_name?.toLowerCase().startsWith("mangora-v"));
    if (!active) return fallback;

    const latestClean = active.tag_name.replace(/^mangora-v?/i, "");
    const apk = active.assets?.find((a) => a.name.toLowerCase().endsWith(".apk"))?.browser_download_url ?? null;
    const notes = (active.body ?? "").replace(/<!--[\s\S]*?-->/g, "").trim();

    return {
      currentVersion,
      latestVersion: active.tag_name,
      updateAvailable: latestClean !== currentVersion,
      releaseUrl: active.html_url ?? fallback.releaseUrl,
      androidApkUrl: apk,
      releaseNotes: notes,
      publishedAt: active.published_at ?? null,
      checkedAt: new Date().toISOString(),
    };
  } catch {
    return fallback;
  }
}
