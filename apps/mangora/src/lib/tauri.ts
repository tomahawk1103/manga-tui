import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  resolveWebProviders,
  webBuildReaderPageUrl,
  webCheckMangoraUpdates,
  webFetchAniListCommunity,
  webGetForYouRecommendations,
  webGetSeriesDetail,
  webOpenReaderChapter,
  webSearchCatalog,
} from "../web/catalog";
import {
  exportWebLibraryBackupJson,
  getWebAppState,
  getWebLibraryOverview,
  importWebLibraryBackupJson,
  patchWebAppState,
  saveWebReaderProgress,
  toggleWebChapterRead,
  toggleWebFavorite,
} from "../web/db";
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
  LibraryOverview,
  MalSyncActionResultDto,
  OAuthCompletedEventDto,
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

export function searchCatalog(provider: string, query: string, page: number): Promise<CatalogPage> {
  if (isTauriRuntime()) {
    return invoke<CatalogPage>("search_catalog", { provider, query, page });
  }
  return webSearchCatalog(provider, query, page);
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

async function getWebProviders(): Promise<ProviderOption[]> {
  return resolveWebProviders(FALLBACK_PROVIDERS);
}

export async function getInitialState(): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("get_initial_state");
  }
  return getWebAppState(await getWebProviders());
}

export async function saveOnboardingPreferences(
  payload: OnboardingPreferencesPayload,
): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("save_onboarding_preferences", { payload });
  }
  const providers = await getWebProviders();
  return patchWebAppState(
    {
      uiLanguage: payload.uiLanguage,
      favoriteGenres: payload.favoriteGenres,
      activeProvider: payload.provider,
      onboardingCompleted: payload.onboardingCompleted,
      ...(payload.themeMode ? { themeMode: payload.themeMode } : {}),
    },
    providers,
  );
}

export async function setUiLanguage(language: UiLanguage): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_ui_language", { language });
  }
  return patchWebAppState({ uiLanguage: language }, await getWebProviders());
}

export async function setThemeMode(themeMode: ThemeMode): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_theme_mode", { themeMode });
  }
  return patchWebAppState({ themeMode }, await getWebProviders());
}

export async function setDefaultProvider(provider: string): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_default_provider", { provider });
  }
  return patchWebAppState({ activeProvider: provider }, await getWebProviders());
}

export async function setFavoriteGenres(genres: string[]): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_favorite_genres", { genres });
  }
  return patchWebAppState({ favoriteGenres: genres }, await getWebProviders());
}

export async function setOnboardingCompleted(completed: boolean): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_onboarding_completed", { completed });
  }
  return patchWebAppState({ onboardingCompleted: completed }, await getWebProviders());
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

export async function getLibraryOverview(search?: string): Promise<LibraryOverview> {
  if (isTauriRuntime()) {
    return invoke<LibraryOverview>("get_library_overview", {
      search: search?.trim() || null,
    });
  }
  return getWebLibraryOverview(search, await getWebProviders());
}

export async function toggleLibraryFavorite(
  payload: ToggleFavoritePayload,
): Promise<ToggleFavoriteResult> {
  if (isTauriRuntime()) {
    return invoke<ToggleFavoriteResult>("toggle_library_favorite", { payload });
  }
  return toggleWebFavorite(payload, await getWebProviders());
}

export async function toggleChapterRead(
  payload: ToggleChapterReadPayload,
): Promise<ToggleChapterReadResult> {
  if (isTauriRuntime()) {
    return invoke<ToggleChapterReadResult>("toggle_chapter_read", { payload });
  }
  return toggleWebChapterRead(payload, await getWebProviders());
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
  return webGetSeriesDetail(selection, options);
}

export async function getAniListCommunity(title: string): Promise<AniListCommunityInfo> {
  if (isTauriRuntime()) {
    return invoke<AniListCommunityInfo>("get_anilist_community", { title });
  }
  return webFetchAniListCommunity(title);
}

export async function getForYouRecommendations(provider?: string): Promise<ForYouResponse> {
  if (isTauriRuntime()) {
    return invoke<ForYouResponse>("get_for_you_recommendations", {
      provider: provider ?? null,
    });
  }
  const state = await getInitialState();
  return webGetForYouRecommendations(
    provider ?? state.activeProvider,
    state.favoriteGenres,
    state.uiLanguage,
  );
}

export function buildReaderPageUrl(sessionId: string, pageIndex: number): string {
  if (isTauriRuntime()) {
    return convertFileSrc(`page/${sessionId}/${pageIndex}`, "mangora");
  }
  return webBuildReaderPageUrl(sessionId, pageIndex);
}

export async function openReaderChapter(
  request: OpenReaderChapterRequest,
): Promise<ReaderSessionResponse> {
  if (isTauriRuntime()) {
    return invoke<ReaderSessionResponse>("open_reader_chapter", { request });
  }
  return webOpenReaderChapter(request);
}

export async function saveReaderProgress(
  payload: SaveReaderProgressPayload,
): Promise<SaveReaderProgressResult> {
  if (isTauriRuntime()) {
    return invoke<SaveReaderProgressResult>("save_reader_progress", { payload });
  }
  return saveWebReaderProgress(payload);
}

export async function saveReaderPrefs(prefs: ReaderPrefs): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("save_reader_prefs", { prefs });
  }
  return patchWebAppState({ readerPrefs: prefs }, await getWebProviders());
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
  const state = await getInitialState();
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
  const next = await patchWebAppState({ downloadFormat: format }, await getWebProviders());
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
      websiteUrl: "https://tomahawk1103.github.io/manga-tui/",
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

export async function subscribeOAuthCompleted(
  handler: (event: OAuthCompletedEventDto) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen<OAuthCompletedEventDto>("mangora://oauth-completed", (event) => {
    handler(event.payload);
  });
}

export async function checkMangoraUpdates(): Promise<UpdateCheckResultDto> {
  if (!isTauriRuntime()) {
    const state = await getInitialState();
    return webCheckMangoraUpdates(state.appVersion);
  }
  return invoke<UpdateCheckResultDto>("check_mangora_updates");
}

export async function exportLibraryBackupJson(): Promise<string> {
  if (isTauriRuntime()) {
    return invoke<string>("export_library_backup_json");
  }
  const state = await getInitialState();
  return exportWebLibraryBackupJson(state.appVersion);
}

export async function exportLibraryBackupFile(): Promise<string> {
  if (isTauriRuntime()) {
    return invoke<string>("export_library_backup_file");
  }
  throw new Error("Disponible en la aplicación nativa.");
}

export async function importLibraryBackupJson(
  jsonPayload: string,
): Promise<PortableBackupImportResultDto> {
  if (isTauriRuntime()) {
    return invoke<PortableBackupImportResultDto>("import_library_backup_json", {
      jsonPayload,
    });
  }
  return importWebLibraryBackupJson(jsonPayload, await getWebProviders());
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
  return patchWebAppState({ supportBannerDismissed: true }, await getWebProviders());
}

export async function openMangaTuiDesktop(): Promise<void> {
  if (isTauriRuntime()) {
    return invoke<void>("open_manga_tui_desktop");
  }
  throw new Error("Disponible en la aplicación de escritorio.");
}
