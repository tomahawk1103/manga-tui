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

export async function loadUpdates(signal: AbortSignal): Promise<ChapterUpdate[]> {
  const response = await fetch(apiUrl("updates"), { signal });
  if (!response.ok) throw new Error(String(response.status));
  return (await response.json()) as ChapterUpdate[];
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

export async function webSearchCatalog(provider: string, query: string, page: number): Promise<CatalogPage> {
  const trimmed = query.trim();
  if (!trimmed) return { items: [], hasNext: false };

  const caps = await detectWorkerCapabilities();
  const normProvider = provider.trim().toLowerCase() || "mangaoni";

  if (caps.v1Supported && caps.providers.includes(normProvider)) {
    const url = apiUrl("v1/search");
    url.searchParams.set("provider", normProvider);
    url.searchParams.set("q", trimmed);
    url.searchParams.set("page", String(page));
    return fetchJson<CatalogPage>(url);
  }

  try {
    const url = apiUrl("search");
    url.searchParams.set("q", trimmed);
    const items = await fetchJson<LegacySeries[]>(url);
    return {
      items: items.map((item) => ({
        id: item.id,
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

  // Legacy MangaOni worker endpoint (`series?id=...`)
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
