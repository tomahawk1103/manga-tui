import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AniListCommunityInfo,
  CloudSyncActionResultDto,
  DownloadChapterFinishedEvent,
  DownloadFormat,
  DownloadQueueAction,
  DownloadQueueSnapshot,
  EnqueueChaptersRequest,
  ForYouResponse,
  InitialAppState,
  IntegrationsOverviewDto,
  LibraryEntry,
  LibraryOverview,
  MalSyncActionResultDto,
  OAuthStartResultDto,
  OfflineSeriesItem,
  OnboardingPreferencesPayload,
  OpenReaderChapterRequest,
  PortableBackupImportResultDto,
  ProviderOption,
  ReaderPrefs,
  ReaderSessionResponse,
  SaveReaderProgressPayload,
  SaveReaderProgressResult,
  SeriesDetailResponse,
  SeriesSelection,
  SetDownloadFormatResult,
  SubmitFeedbackRequest,
  SubmitFeedbackResponse,
  ThemeMode,
  ToggleChapterReadPayload,
  ToggleChapterReadResult,
  ToggleFavoritePayload,
  ToggleFavoriteResult,
  UiLanguage,
  UpdateCheckResultDto,
} from "../types/mangora";

const BROWSER_STORAGE_KEY = "mangora_browser_fallback_v1";
const BROWSER_FAVORITES_KEY = "mangora_browser_favorites_v1";

export interface CatalogItem {
  id: string;
  title: string;
  description: string | null;
  coverUrl: string;
  genres: string[];
  languages: string[];
}

export interface CatalogPage {
  items: CatalogItem[];
  hasNext: boolean;
}

export function searchCatalog(provider: string, query: string, page: number) {
  return invoke<CatalogPage>("search_catalog", { provider, query, page });
}

const FALLBACK_PROVIDERS: ProviderOption[] = [
  {
    id: "mangadex",
    name: "MangaDex",
    badge: "MangaDex 🌐",
    languageGroup: "multi",
    descriptionEs: "Catálogo internacional multilingüe con metadatos completos.",
    descriptionEn: "International multi-language catalog with rich metadata.",
  },
  {
    id: "inmanga",
    name: "InManga",
    badge: "InManga 🇪🇸",
    languageGroup: "es",
    descriptionEs: "Recomendado en español. Catálogo amplio y carga rápida.",
    descriptionEn: "Recommended for Spanish readers. Wide catalog and fast loading.",
  },
  {
    id: "mangaoni",
    name: "MangaOni",
    badge: "MangaOni 🇪🇸",
    languageGroup: "es",
    descriptionEs: "Alternativa en español con buen catálogo de series populares.",
    descriptionEn: "Spanish alternative with a solid selection of popular series.",
  },
  {
    id: "leercapitulo",
    name: "LeerCapítulo",
    badge: "LeerCapítulo 🇪🇸",
    languageGroup: "es",
    descriptionEs: "Fuente adicional en español para capítulos recientes.",
    descriptionEn: "Additional Spanish source for recent chapter releases.",
  },
  {
    id: "weebcentral",
    name: "WeebCentral",
    badge: "WeebCentral 🇬🇧",
    languageGroup: "en",
    descriptionEs: "Recomendado en inglés. Gran catálogo en alta calidad.",
    descriptionEn: "Recommended for English readers. Large high-quality catalog.",
  },
  {
    id: "mangapill",
    name: "MangaPill",
    badge: "MangaPill 🇬🇧",
    languageGroup: "en",
    descriptionEs: "Fuente rápida en inglés con buena disponibilidad de capítulos.",
    descriptionEn: "Fast English source with reliable chapter availability.",
  },
  {
    id: "comizy",
    name: "Comizy",
    badge: "Comizy 🇬🇧",
    languageGroup: "en",
    descriptionEs: "Fuente complementaria en inglés con lanzamientos recientes.",
    descriptionEn: "Complementary English source for recent releases.",
  },
];

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function defaultBrowserState(): InitialAppState {
  return {
    appVersion: "0.1.0",
    uiLanguage: "es",
    themeMode: "paper",
    platform: "unknown",
    isMobile: false,
    canOpenDataFolder: true,
    storageMode: "system",
    storageModeLabel: "Browser Preview",
    dataDir: "AppData/manga-tui/data",
    configDir: "AppData/manga-tui/config",
    downloadsDir: "AppData/manga-tui/data/mangaDownloads",
    activeProvider: "inmanga",
    recommendedProvider: "inmanga",
    availableProviders: FALLBACK_PROVIDERS,
    favoriteGenres: [],
    onboardingCompleted: false,
    downloadFormat: "cbz",
    supportBannerDismissed: false,
    readerPrefs: {
      viewMode: "cascade",
      direction: "rtl",
      fitMode: "fitWidth",
      zoomPercent: 100,
      cascadeGapPx: 8,
      firstPageSingle: true,
      pinToolbars: false,
      darkCanvas: true,
    },
    library: {
      hasExistingData: false,
      libraryMangas: 0,
      chaptersRead: 0,
      totalPagesRead: 0,
      favorites: 0,
      downloadedChapters: 0,
      offlineSeriesCount: 0,
      lastReadTitle: null,
    },
  };
}

function readBrowserState(): InitialAppState {
  const base = defaultBrowserState();
  try {
    const raw = window.localStorage.getItem(BROWSER_STORAGE_KEY);
    if (!raw) return base;
    const parsed = JSON.parse(raw) as Partial<InitialAppState>;
    return {
      ...base,
      ...parsed,
      availableProviders: FALLBACK_PROVIDERS,
    };
  } catch {
    return base;
  }
}

function writeBrowserState(next: InitialAppState): InitialAppState {
  try {
    window.localStorage.setItem(BROWSER_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Ignore storage quota errors in fallback mode
  }
  return next;
}

function recommendedForLang(lang: UiLanguage): string {
  return lang === "es" ? "inmanga" : "weebcentral";
}

export async function getInitialState(): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("get_initial_state");
  }
  return readBrowserState();
}

export async function saveOnboardingPreferences(
  payload: OnboardingPreferencesPayload,
): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("save_onboarding_preferences", { payload });
  }
  const current = readBrowserState();
  return writeBrowserState({
    ...current,
    uiLanguage: payload.uiLanguage,
    favoriteGenres: payload.favoriteGenres,
    activeProvider: payload.provider,
    recommendedProvider: recommendedForLang(payload.uiLanguage),
    onboardingCompleted: payload.onboardingCompleted,
    themeMode: payload.themeMode ?? current.themeMode,
  });
}

export async function setUiLanguage(language: UiLanguage): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_ui_language", { language });
  }
  const current = readBrowserState();
  return writeBrowserState({
    ...current,
    uiLanguage: language,
    recommendedProvider: recommendedForLang(language),
  });
}

export async function setThemeMode(themeMode: ThemeMode): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_theme_mode", { themeMode });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, themeMode });
}

export async function setDefaultProvider(provider: string): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_default_provider", { provider });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, activeProvider: provider });
}

export async function setFavoriteGenres(genres: string[]): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_favorite_genres", { genres });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, favoriteGenres: genres });
}

export async function setOnboardingCompleted(completed: boolean): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_onboarding_completed", { completed });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, onboardingCompleted: completed });
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauriRuntime()) {
    await invoke<void>("open_external_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function openDataFolder(): Promise<void> {
  if (isTauriRuntime()) {
    await invoke<void>("open_data_folder");
  }
}

export async function openDownloadsFolder(): Promise<void> {
  if (isTauriRuntime()) {
    await invoke<void>("open_downloads_folder");
  }
}

function readBrowserFavorites(): LibraryEntry[] {
  try {
    const raw = window.localStorage.getItem(BROWSER_FAVORITES_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as LibraryEntry[]) : [];
  } catch {
    return [];
  }
}

function writeBrowserFavorites(entries: LibraryEntry[]): LibraryEntry[] {
  try {
    window.localStorage.setItem(BROWSER_FAVORITES_KEY, JSON.stringify(entries));
  } catch {
    // Ignore storage quota errors in fallback mode
  }
  return entries;
}

function buildBrowserOverview(entries: LibraryEntry[], search?: string): LibraryOverview {
  const needle = search?.trim().toLowerCase() ?? "";
  const filtered = needle
    ? entries.filter((item) => item.title.toLowerCase().includes(needle))
    : entries;
  return {
    continueReading: [],
    favorites: filtered,
    recentHistory: [],
    totalContinueReading: 0,
    totalFavorites: filtered.length,
    totalRecent: 0,
  };
}

export async function getLibraryOverview(search?: string): Promise<LibraryOverview> {
  if (isTauriRuntime()) {
    return invoke<LibraryOverview>("get_library_overview", {
      search: search?.trim() || null,
    });
  }
  return buildBrowserOverview(readBrowserFavorites(), search);
}

export async function toggleLibraryFavorite(
  payload: ToggleFavoritePayload,
): Promise<ToggleFavoriteResult> {
  if (isTauriRuntime()) {
    return invoke<ToggleFavoriteResult>("toggle_library_favorite", { payload });
  }
  const currentState = readBrowserState();
  const providerMeta = FALLBACK_PROVIDERS.find((p) => p.id === payload.provider);
  const existing = readBrowserFavorites();
  const matchIdx = existing.findIndex(
    (item) => item.id === payload.mangaId && item.provider === payload.provider,
  );

  let isFavorite = false;
  let nextFavorites: LibraryEntry[];
  if (matchIdx >= 0) {
    nextFavorites = existing.filter((_, idx) => idx !== matchIdx);
  } else {
    isFavorite = true;
    nextFavorites = [
      {
        id: payload.mangaId,
        title: payload.title,
        provider: payload.provider,
        providerLabel: providerMeta?.badge ?? payload.provider,
        coverUrl: payload.coverUrl ?? null,
        lastRead: null,
        lastReadChapter: null,
        totalReadCount: 0,
        bookmarkChapter: null,
        bookmarkPage: null,
        isFavorite: true,
        isPlanToRead: false,
        downloadedCount: 0,
        shelfName: null,
      },
      ...existing,
    ];
  }

  writeBrowserFavorites(nextFavorites);
  const appState = writeBrowserState({
    ...currentState,
    library: {
      ...currentState.library,
      hasExistingData:
        nextFavorites.length > 0 ||
        currentState.library.chaptersRead > 0 ||
        currentState.library.libraryMangas > 0,
      favorites: nextFavorites.length,
    },
  });

  return {
    isFavorite,
    appState,
    library: buildBrowserOverview(nextFavorites),
  };
}

export async function toggleChapterRead(
  payload: ToggleChapterReadPayload,
): Promise<ToggleChapterReadResult> {
  if (isTauriRuntime()) {
    return invoke<ToggleChapterReadResult>("toggle_chapter_read", { payload });
  }
  const appState = readBrowserState();
  return {
    isRead: true,
    appState,
    library: buildBrowserOverview(readBrowserFavorites()),
  };
}

export async function getSeriesDetail(
  selection: SeriesSelection,
  options?: {
    language?: string;
    order?: "asc" | "desc";
    page?: number;
  },
): Promise<SeriesDetailResponse> {
  if (isTauriRuntime()) {
    return invoke<SeriesDetailResponse>("get_series_detail", {
      request: {
        mangaId: selection.mangaId,
        provider: selection.provider,
        fallbackTitle: selection.title,
        fallbackCoverUrl: selection.coverUrl ?? null,
        fallbackDescription: selection.description ?? null,
        language: options?.language ?? null,
        order: options?.order ?? "desc",
        page: options?.page ?? 1,
      },
    });
  }
  const providerMeta = FALLBACK_PROVIDERS.find((p) => p.id === selection.provider);
  const favs = readBrowserFavorites();
  const isFavorite = favs.some(
    (entry) => entry.id === selection.mangaId && entry.provider === selection.provider,
  );
  return {
    id: selection.mangaId,
    provider: selection.provider,
    providerLabel: providerMeta?.badge ?? selection.provider,
    title: selection.title,
    coverUrl: selection.coverUrl ?? null,
    description: selection.description ?? null,
    author: null,
    artist: null,
    status: "unknown",
    genres: [],
    availableLanguages: [],
    activeLanguage: options?.language ?? "es",
    order: options?.order ?? "desc",
    page: options?.page ?? 1,
    totalChapters: 0,
    hasNextPage: false,
    isFavorite,
    isOfflineFallback: false,
    offlineChapterCount: 0,
    readChapterCount: 0,
    nextUnreadChapter: null,
    chapters: [],
    onlineWarning: "Browser preview",
  };
}

export async function getAniListCommunity(title: string): Promise<AniListCommunityInfo> {
  if (isTauriRuntime()) {
    return invoke<AniListCommunityInfo>("get_anilist_community", { title });
  }
  return {
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
    note: "preview",
  };
}

export async function getForYouRecommendations(provider?: string): Promise<ForYouResponse> {
  if (isTauriRuntime()) {
    return invoke<ForYouResponse>("get_for_you_recommendations", {
      provider: provider ?? null,
    });
  }
  const state = readBrowserState();
  const providerId = provider ?? state.activeProvider;
  const providerMeta = FALLBACK_PROVIDERS.find((p) => p.id === providerId);
  return {
    provider: providerId,
    providerLabel: providerMeta?.badge ?? providerId,
    matchedGenres: state.favoriteGenres,
    items: [],
    fallbackPopular: true,
  };
}

export function buildReaderPageUrl(sessionId: string, pageIndex: number): string {
  if (isTauriRuntime()) {
    return convertFileSrc(`page/${sessionId}/${pageIndex}`, "mangora");
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="900" height="1350" viewBox="0 0 900 1350"><rect width="900" height="1350" fill="#f8f7f1"/><rect x="36" y="36" width="828" height="1278" fill="none" stroke="#191918" stroke-width="8"/><text x="450" y="640" text-anchor="middle" font-family="Arial, sans-serif" font-size="64" font-weight="900" fill="#191918">MANGORA PREVIEW</text><text x="450" y="720" text-anchor="middle" font-family="monospace" font-size="36" fill="#cb3028">${sessionId} · Page ${pageIndex + 1}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

export async function openReaderChapter(
  request: OpenReaderChapterRequest,
): Promise<ReaderSessionResponse> {
  if (isTauriRuntime()) {
    return invoke<ReaderSessionResponse>("open_reader_chapter", { request });
  }
  return {
    sessionId: "preview-1",
    mangaId: request.mangaId,
    provider: request.provider,
    providerLabel: request.provider,
    mangaTitle: request.mangaTitle,
    coverUrl: request.coverUrl ?? null,
    chapterId: request.chapterId,
    chapterTitle: request.chapterTitle ?? "Cap. 1",
    chapterNumber: request.chapterNumber ?? "1",
    volumeNumber: request.volumeNumber ?? null,
    language: request.language ?? "es",
    pageCount: 3,
    initialPage: 0,
    isOffline: false,
    offlineFormat: null,
    prevChapter: null,
    nextChapter: null,
  };
}

export async function saveReaderProgress(
  payload: SaveReaderProgressPayload,
): Promise<SaveReaderProgressResult> {
  if (isTauriRuntime()) {
    return invoke<SaveReaderProgressResult>("save_reader_progress", { payload });
  }
  return { pageIndex: payload.pageIndex, isRead: true };
}

export async function saveReaderPrefs(prefs: ReaderPrefs): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("save_reader_prefs", { prefs });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, readerPrefs: prefs });
}

export async function toggleNativeFullscreen(): Promise<boolean> {
  if (isTauriRuntime()) {
    try {
      const win = getCurrentWindow();
      const next = !(await win.isFullscreen());
      await win.setFullscreen(next);
      return next;
    } catch {
      // Fallback to DOM fullscreen if platform disallows native window fullscreen (e.g. mobile)
    }
  }
  if (document.fullscreenElement) {
    await document.exitFullscreen().catch(() => {});
    return false;
  }
  await document.documentElement.requestFullscreen?.().catch(() => {});
  return Boolean(document.fullscreenElement);
}

export async function getDownloadQueueSnapshot(): Promise<DownloadQueueSnapshot> {
  if (isTauriRuntime()) {
    return invoke<DownloadQueueSnapshot>("get_download_queue_snapshot");
  }
  const state = readBrowserState();
  return {
    isPaused: false,
    isDownloading: false,
    activeFormat: state.downloadFormat,
    downloadsDir: state.downloadsDir,
    queuedCount: 0,
    downloadingCount: 0,
    pausedCount: 0,
    completedCount: 0,
    failedCount: 0,
    overallProgressPercent: 0,
    speedBytesPerSec: 0,
    items: [],
  };
}

export async function enqueueChaptersDownload(
  payload: EnqueueChaptersRequest,
): Promise<DownloadQueueSnapshot> {
  if (isTauriRuntime()) {
    return invoke<DownloadQueueSnapshot>("enqueue_chapters_download", { payload });
  }
  return getDownloadQueueSnapshot();
}

export async function controlDownloadQueue(
  action: DownloadQueueAction,
  taskId?: string,
): Promise<DownloadQueueSnapshot> {
  if (isTauriRuntime()) {
    return invoke<DownloadQueueSnapshot>("control_download_queue", {
      request: { action, taskId: taskId ?? null },
    });
  }
  return getDownloadQueueSnapshot();
}

export async function setDownloadFormat(
  format: DownloadFormat,
): Promise<SetDownloadFormatResult> {
  if (isTauriRuntime()) {
    return invoke<SetDownloadFormatResult>("set_download_format", { format });
  }
  const next = writeBrowserState({ ...readBrowserState(), downloadFormat: format });
  const queue = await getDownloadQueueSnapshot();
  return { queue, appState: next };
}

export async function listOfflineLibrary(): Promise<OfflineSeriesItem[]> {
  if (isTauriRuntime()) {
    return invoke<OfflineSeriesItem[]>("list_offline_library");
  }
  return [];
}

export async function subscribeDownloadQueue(
  handler: (snapshot: DownloadQueueSnapshot) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<DownloadQueueSnapshot>("mangora://download-queue", (event) => {
    handler(event.payload);
  });
}

export async function subscribeDownloadChapterFinished(
  handler: (event: DownloadChapterFinishedEvent) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<DownloadChapterFinishedEvent>("mangora://download-chapter-finished", (event) => {
    handler(event.payload);
  });
}

export function defaultBrowserIntegrationsOverview(): IntegrationsOverviewDto {
  return {
    cloud: {
      connected: false,
      backend: null,
      summary: "Sin vincular",
      accountEmail: null,
      accountDisplayName: null,
      localFolderPath: null,
      localFolderLabel: null,
      hasEmbeddedOauth: false,
      savedClientId: null,
      lastSyncAt: null,
      lastSyncReport: null,
      detectedFolders: [],
    },
    trackers: {
      anilist: {
        connected: false,
        clientIdHint: null,
        source: null,
        trackReadingHistory: false,
      },
      myAnimeList: {
        connected: false,
        username: null,
        summary: "Sin vincular",
        hasEmbeddedOauth: false,
        savedClientId: null,
        lastSyncAt: null,
        lastSyncReport: null,
      },
    },
    supportConfig: {
      kofiUrl: null,
      sponsorsUrl: "https://github.com/sponsors/tomahawk1103",
      discordUrl: "https://discord.gg/SeqsWUCpVx",
      githubUrl: "https://github.com/tomahawk1103/manga-tui",
      githubIssuesUrl: "https://github.com/tomahawk1103/manga-tui/issues/new",
    },
    feedbackConfig: {
      proxyConfigured: false,
      fallbackIssuesUrl: "https://github.com/tomahawk1103/manga-tui/issues/new",
    },
    mangaTuiDetected: false,
  };
}

export async function getIntegrationsOverview(): Promise<IntegrationsOverviewDto> {
  if (isTauriRuntime()) {
    return invoke<IntegrationsOverviewDto>("get_integrations_overview");
  }
  return defaultBrowserIntegrationsOverview();
}

export async function connectGoogleDrive(params: {
  clientId?: string;
  clientSecret?: string;
  useDeeplink?: boolean;
}): Promise<OAuthStartResultDto> {
  if (!isTauriRuntime()) {
    throw new Error("Conectar Google Drive requiere la aplicación nativa Mangora.");
  }
  return invoke<OAuthStartResultDto>("connect_google_drive", {
    clientId: params.clientId ?? null,
    clientSecret: params.clientSecret ?? null,
    useDeeplink: params.useDeeplink ?? false,
  });
}

export async function connectLocalCloudFolder(
  folderPath: string,
  providerLabel?: string,
): Promise<IntegrationsOverviewDto> {
  if (!isTauriRuntime()) {
    throw new Error("Vincular carpeta sincronizada requiere la aplicación nativa Mangora.");
  }
  return invoke<IntegrationsOverviewDto>("connect_local_cloud_folder", {
    folderPath,
    providerLabel: providerLabel ?? null,
  });
}

export async function runCloudSyncNow(
  mode: "libraryOnly" | "fullSync" | "pushDownloads" | "pullDownloads",
): Promise<CloudSyncActionResultDto> {
  if (!isTauriRuntime()) {
    throw new Error("Sincronizar nube requiere la aplicación nativa Mangora.");
  }
  return invoke<CloudSyncActionResultDto>("run_cloud_sync_now", { mode });
}

export async function disconnectCloud(): Promise<IntegrationsOverviewDto> {
  if (isTauriRuntime()) {
    return invoke<IntegrationsOverviewDto>("disconnect_cloud");
  }
  return defaultBrowserIntegrationsOverview();
}

export async function saveAnilistCredentials(params: {
  clientId: string;
  accessToken: string;
  verify?: boolean;
}): Promise<IntegrationsOverviewDto> {
  if (!isTauriRuntime()) {
    throw new Error("Configurar AniList requiere la aplicación nativa Mangora.");
  }
  return invoke<IntegrationsOverviewDto>("save_anilist_credentials", {
    clientId: params.clientId,
    accessToken: params.accessToken,
    verify: params.verify ?? true,
  });
}

export async function disconnectAnilist(): Promise<IntegrationsOverviewDto> {
  if (isTauriRuntime()) {
    return invoke<IntegrationsOverviewDto>("disconnect_anilist");
  }
  return defaultBrowserIntegrationsOverview();
}

export async function connectMyAnimeList(params: {
  clientId?: string;
  clientSecret?: string;
  useDeeplink?: boolean;
}): Promise<OAuthStartResultDto> {
  if (!isTauriRuntime()) {
    throw new Error("Conectar MyAnimeList requiere la aplicación nativa Mangora.");
  }
  return invoke<OAuthStartResultDto>("connect_myanimelist", {
    clientId: params.clientId ?? null,
    clientSecret: params.clientSecret ?? null,
    useDeeplink: params.useDeeplink ?? false,
  });
}

export async function syncMyAnimeListNow(): Promise<MalSyncActionResultDto> {
  if (!isTauriRuntime()) {
    throw new Error("Sincronizar MyAnimeList requiere la aplicación nativa Mangora.");
  }
  return invoke<MalSyncActionResultDto>("sync_myanimelist_now");
}

export async function disconnectMyAnimeList(): Promise<IntegrationsOverviewDto> {
  if (isTauriRuntime()) {
    return invoke<IntegrationsOverviewDto>("disconnect_myanimelist");
  }
  return defaultBrowserIntegrationsOverview();
}

export async function completeOAuthCallback(
  provider: "googleDrive" | "myAnimeList",
  callbackInput: string,
): Promise<IntegrationsOverviewDto> {
  if (!isTauriRuntime()) {
    throw new Error("Completar OAuth requiere la aplicación nativa Mangora.");
  }
  return invoke<IntegrationsOverviewDto>("complete_oauth_callback", {
    provider,
    callbackInput,
  });
}

export async function checkMangoraUpdates(): Promise<UpdateCheckResultDto> {
  if (!isTauriRuntime()) {
    return {
      currentVersion: readBrowserState().appVersion,
      latestVersion: `mangora-v${readBrowserState().appVersion}`,
      updateAvailable: false,
      releaseUrl: "https://github.com/tomahawk1103/manga-tui/releases",
      androidApkUrl: null,
      iosIpaUrl: null,
      releaseNotes: "Preview web: abre la aplicación nativa para consultar releases reales.",
      publishedAt: null,
      checkedAt: new Date().toISOString(),
    };
  }
  return invoke<UpdateCheckResultDto>("check_mangora_updates");
}

export async function exportLibraryBackupJson(): Promise<string> {
  if (isTauriRuntime()) {
    return invoke<string>("export_library_backup_json");
  }
  const state = readBrowserState();
  return JSON.stringify(
    {
      schema_version: 1,
      app_version: state.appVersion,
      synced_at: new Date().toISOString(),
      mangas: [],
      chapters: [],
      favorites: [],
      dismissed_notifications: [],
      search_history: [],
      collections: [],
      manga_genres: [],
    },
    null,
    2,
  );
}

export async function importLibraryBackupJson(
  jsonPayload: string,
): Promise<PortableBackupImportResultDto> {
  if (isTauriRuntime()) {
    return invoke<PortableBackupImportResultDto>("import_library_backup_json", {
      jsonPayload,
    });
  }
  const parsed = JSON.parse(jsonPayload) as {
    mangas?: unknown[];
    chapters?: Array<{ is_read?: boolean }>;
    favorites?: unknown[];
  };
  return {
    totalMangas: Array.isArray(parsed.mangas) ? parsed.mangas.length : 0,
    totalReadChapters: Array.isArray(parsed.chapters)
      ? parsed.chapters.filter((c) => Boolean(c.is_read)).length
      : 0,
    totalFavorites: Array.isArray(parsed.favorites) ? parsed.favorites.length : 0,
    appState: readBrowserState(),
    library: await getLibraryOverview(),
  };
}

export async function submitFeedback(
  request: SubmitFeedbackRequest,
): Promise<SubmitFeedbackResponse> {
  if (isTauriRuntime()) {
    return invoke<SubmitFeedbackResponse>("submit_feedback", { request });
  }
  const url = `https://github.com/tomahawk1103/manga-tui/issues/new?title=${encodeURIComponent(`[Mangora][${request.category}]`)}&body=${encodeURIComponent(request.message)}`;
  return {
    delivered: false,
    mode: "githubFallback",
    statusMessage: "Preview web: preparamos tu mensaje en GitHub Issues.",
    fallbackIssueUrl: url,
  };
}

export async function dismissSupportBanner(): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("dismiss_support_banner");
  }
  return writeBrowserState({ ...readBrowserState(), supportBannerDismissed: true });
}

export async function openMangaTuiDesktop(): Promise<void> {
  if (isTauriRuntime()) {
    return invoke<void>("open_manga_tui_desktop");
  }
  throw new Error("Disponible en la aplicación de escritorio.");
}
