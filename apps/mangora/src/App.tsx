import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BottomNav } from "./components/BottomNav";
import { FeedbackModal } from "./components/FeedbackModal";
import { OnboardingWizard } from "./components/OnboardingWizard";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { useMangoraAppearance } from "./features/hito5";
import { getTranslations } from "./lib/i18n";
import { useScreenWakeLock } from "./lib/mobilePlatform";
import {
  controlDownloadQueue,
  defaultBrowserIntegrationsOverview,
  dismissSupportBanner,
  enqueueChaptersDownload,
  getDownloadQueueSnapshot,
  getInitialState,
  getIntegrationsOverview,
  getLibraryOverview,
  listOfflineLibrary,
  openDataFolder,
  openDownloadsFolder,
  openExternalUrl,
  openMangaTuiDesktop,
  saveOnboardingPreferences,
  saveReaderPrefs,
  setDefaultProvider,
  setDownloadFormat,
  setFavoriteGenres,
  setThemeMode,
  setUiLanguage,
  subscribeDownloadChapterFinished,
  subscribeDownloadQueue,
  toggleChapterRead,
  toggleLibraryFavorite,
} from "./lib/tauri";
import {
  type DownloadFormat,
  type DownloadQueueAction,
  type DownloadQueueSnapshot,
  type EnqueueChaptersRequest,
  type InitialAppState,
  type IntegrationsOverviewDto,
  type LibraryOverview,
  type NavSection,
  type OfflineSeriesItem,
  type OpenReaderChapterRequest,
  type ReaderPrefs,
  type SeriesSelection,
  type ThemeMode,
  type ToggleChapterReadPayload,
  type ToggleFavoritePayload,
  libraryKey,
} from "./types/mangora";
import { DownloadsView } from "./views/DownloadsView";
import { ExploreView } from "./views/ExploreView";
import { HomeView } from "./views/HomeView";
import { ReaderView } from "./views/ReaderView";
import { SeriesDetailView } from "./views/SeriesDetailView";
import { SettingsView } from "./views/SettingsView";
import { SupportView } from "./views/SupportView";

export function App() {
  const [appState, setAppState] = useState<InitialAppState | null>(null);
  const [libraryOverview, setLibraryOverview] = useState<LibraryOverview | null>(null);
  const [libraryLoading, setLibraryLoading] = useState(false);
  const [activeSection, setActiveSection] = useState<NavSection>("home");
  const [selectedSeries, setSelectedSeries] = useState<SeriesSelection | null>(null);
  const [readerTarget, setReaderTarget] = useState<OpenReaderChapterRequest | null>(null);
  const [downloadQueue, setDownloadQueue] = useState<DownloadQueueSnapshot | null>(null);
  const [offlineSeries, setOfflineSeries] = useState<OfflineSeriesItem[]>([]);
  const [offlineLoading, setOfflineLoading] = useState(false);
  const [integrationsOverview, setIntegrationsOverview] = useState<IntegrationsOverviewDto | null>(null);
  const [feedbackOpen, setFeedbackOpen] = useState(false);
  const [downloadCompletedTick, setDownloadCompletedTick] = useState(0);
  const [navigationTarget, setNavigationTarget] = useState<"genres" | "search" | null>(null);
  const [replayOnboarding, setReplayOnboarding] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const mainRef = useRef<HTMLElement>(null);
  const uiLangRef = useRef<"es" | "en">("es");
  const readerBackHandlerRef = useRef<(() => Promise<boolean>) | null>(null);
  const backStateRef = useRef({
    feedbackOpen: false,
    hasReader: false,
    hasSeries: false,
    replayOnboarding: false,
    activeSection: "home" as NavSection,
  });
  const { config: appearanceConfig, palettes: appearancePalettes, updateAppearance } = useMangoraAppearance();

  const pushBackGuard = useCallback(() => {
    try {
      window.history.pushState({ mangoraBackGuard: true }, "");
    } catch {
      // Ignore history API errors in restricted WebViews
    }
  }, []);

  const loadLibrary = useCallback(async (search?: string) => {
    setLibraryLoading(true);
    try {
      const overview = await getLibraryOverview(search);
      setLibraryOverview(overview);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setLibraryLoading(false);
    }
  }, []);

  const loadOfflineSeries = useCallback(async () => {
    setOfflineLoading(true);
    try {
      const list = await listOfflineLibrary();
      setOfflineSeries(list);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setOfflineLoading(false);
    }
  }, []);

  const initialHomeLoadedRef = useRef(false);

  const loadInitial = useCallback(async () => {
    setErrorMessage(null);
    try {
      const [next, overview] = await Promise.all([
        getInitialState(),
        getLibraryOverview().catch(() => null),
      ]);
      setAppState(next);
      uiLangRef.current = next.uiLanguage;
      if (overview) {
        setLibraryOverview(overview);
        initialHomeLoadedRef.current = true;
      }
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void loadInitial();
  }, [loadInitial]);

  useEffect(() => {
    if (appState?.onboardingCompleted && activeSection === "home") {
      if (initialHomeLoadedRef.current) {
        initialHomeLoadedRef.current = false;
      } else {
        void loadLibrary();
      }
    }
    if (appState?.onboardingCompleted && activeSection === "downloads") {
      void getDownloadQueueSnapshot().then(setDownloadQueue).catch(() => {});
      void loadOfflineSeries();
    }
    if (
      appState?.onboardingCompleted &&
      (activeSection === "settings" || activeSection === "support")
    ) {
      void getIntegrationsOverview().then(setIntegrationsOverview).catch(() => {});
    }
  }, [activeSection, appState?.onboardingCompleted, loadLibrary, loadOfflineSeries]);

  const showToast = useCallback((message: string) => {
    setToastMessage(message);
    window.setTimeout(() => {
      setToastMessage((prev) => (prev === message ? null : prev));
    }, 2200);
  }, []);

  useEffect(() => {
    let active = true;
    let unlistenQueue: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;

    void getDownloadQueueSnapshot().then((snap) => {
      if (active) setDownloadQueue(snap);
    });

    void subscribeDownloadQueue((snap) => {
      if (active) setDownloadQueue(snap);
    }).then((fn) => {
      if (active) unlistenQueue = fn;
      else fn();
    });

    void subscribeDownloadChapterFinished((event) => {
      if (!active) return;
      setDownloadCompletedTick((prev) => prev + 1);
      const copy = getTranslations(uiLangRef.current).downloads;
      showToast(copy.finishedToast(event.mangaTitle, event.chapterNumber));
      void loadInitial();
      void loadOfflineSeries();
    }).then((fn) => {
      if (active) unlistenDone = fn;
      else fn();
    });

    return () => {
      active = false;
      unlistenQueue?.();
      unlistenDone?.();
    };
  }, [loadInitial, loadOfflineSeries, showToast]);

  const favoriteKeys = useMemo(() => {
    const keys = new Set<string>();
    if (!libraryOverview) return keys;
    for (const entry of [
      ...libraryOverview.favorites,
      ...libraryOverview.continueReading,
      ...libraryOverview.recentHistory,
    ]) {
      if (entry.isFavorite) {
        keys.add(libraryKey(entry.id, entry.provider));
      }
    }
    return keys;
  }, [libraryOverview]);

  useEffect(() => {
    if (!appState) return;
    uiLangRef.current = appState.uiLanguage;
    document.documentElement.lang = appState.uiLanguage;
  }, [appState]);

  const activeDownloadCount = useMemo(() => {
    if (!downloadQueue) return 0;
    return (
      downloadQueue.queuedCount +
      downloadQueue.downloadingCount +
      downloadQueue.pausedCount
    );
  }, [downloadQueue]);

  useScreenWakeLock(activeDownloadCount > 0);

  const activePaletteSpec = appearancePalettes.find((p) => p.id === appearanceConfig.palette);
  const effectiveThemeMode: ThemeMode = activePaletteSpec?.is_dark ? "ink" : "paper";
  const nextTheme: ThemeMode = effectiveThemeMode === "paper" ? "ink" : "paper";

  useEffect(() => {
    const meta = document.querySelector('meta[name="theme-color"]');
    if (meta) {
      meta.setAttribute("content", effectiveThemeMode === "ink" ? "#0c0c0b" : "#191918");
    }
  }, [effectiveThemeMode]);

  useEffect(() => {
    backStateRef.current = {
      feedbackOpen,
      hasReader: Boolean(readerTarget),
      hasSeries: Boolean(selectedSeries),
      replayOnboarding,
      activeSection,
    };
  }, [feedbackOpen, readerTarget, selectedSeries, replayOnboarding, activeSection]);

  useEffect(() => {
    const onPopState = () => {
      const current = backStateRef.current;
      if (current.feedbackOpen) {
        setFeedbackOpen(false);
        return;
      }
      if (current.hasReader) {
        const handler = readerBackHandlerRef.current;
        if (handler) {
          void handler().then((closed) => {
            if (!closed) {
              pushBackGuard();
            }
          });
        } else {
          setReaderTarget(null);
        }
        return;
      }
      if (current.hasSeries) {
        setSelectedSeries(null);
        return;
      }
      if (current.replayOnboarding) {
        setReplayOnboarding(false);
        return;
      }
      if (current.activeSection !== "home") {
        setActiveSection("home");
      }
    };

    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, [pushBackGuard]);

  const activeDownloadChapterIds = useMemo(() => {
    const ids = new Set<string>();
    if (!downloadQueue || !selectedSeries) return ids;
    for (const item of downloadQueue.items) {
      if (
        item.mangaId === selectedSeries.mangaId &&
        (item.status === "queued" ||
          item.status === "downloading" ||
          item.status === "packaging" ||
          item.status === "paused")
      ) {
        ids.add(item.chapterId);
      }
    }
    return ids;
  }, [downloadQueue, selectedSeries]);

  const runMutation = async (action: () => Promise<InitialAppState>, announceSave = false) => {
    setIsSaving(true);
    setErrorMessage(null);
    try {
      const next = await action();
      setAppState(next);
      if (announceSave) {
        showToast(getTranslations(next.uiLanguage).settings.savedToast);
      }
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSaving(false);
    }
  };

  const handleToggleFavorite = async (payload: ToggleFavoritePayload) => {
    setIsSaving(true);
    setErrorMessage(null);
    try {
      const result = await toggleLibraryFavorite(payload);
      setAppState(result.appState);
      setLibraryOverview(result.library);
      const copy = getTranslations(result.appState.uiLanguage).library;
      showToast(result.isFavorite ? copy.favoriteAdded : copy.favoriteRemoved);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSaving(false);
    }
  };

  const handleToggleChapterRead = async (payload: ToggleChapterReadPayload): Promise<boolean> => {
    setErrorMessage(null);
    const result = await toggleChapterRead(payload);
    setAppState(result.appState);
    setLibraryOverview(result.library);
    return result.isRead;
  };

  const openSeries = (series: SeriesSelection) => {
    setErrorMessage(null);
    if (!selectedSeries) {
      pushBackGuard();
    }
    setSelectedSeries(series);
    mainRef.current?.scrollTo({ top: 0 });
  };

  const openReader = (req: OpenReaderChapterRequest) => {
    if (!readerTarget) {
      pushBackGuard();
    }
    setReaderTarget(req);
  };

  const openFeedback = () => {
    if (!feedbackOpen) {
      pushBackGuard();
    }
    setFeedbackOpen(true);
  };

  const startTourReplay = () => {
    setSelectedSeries(null);
    if (!replayOnboarding) {
      pushBackGuard();
    }
    setReplayOnboarding(true);
  };

  const handleEnqueueDownload = async (payload: EnqueueChaptersRequest) => {
    const snapshot = await enqueueChaptersDownload(payload);
    setDownloadQueue(snapshot);
  };

  const handleControlQueue = async (action: DownloadQueueAction, taskId?: string) => {
    try {
      const snapshot = await controlDownloadQueue(action, taskId);
      setDownloadQueue(snapshot);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleChangeDownloadFormat = async (format: DownloadFormat) => {
    setIsSaving(true);
    setErrorMessage(null);
    try {
      const res = await setDownloadFormat(format);
      setDownloadQueue(res.queue);
      setAppState(res.appState);
      showToast(getTranslations(res.appState.uiLanguage).settings.savedToast);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSaving(false);
    }
  };

  const handleUpdateReaderPrefs = async (prefs: ReaderPrefs) => {
    try {
      const next = await saveReaderPrefs(prefs);
      setAppState(next);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const handleCloseReader = async () => {
    setReaderTarget(null);
    setDownloadCompletedTick((prev) => prev + 1);
    await Promise.all([loadInitial(), loadLibrary()]);
  };

  if (!appState) {
    const fallbackT = getTranslations("es");
    return (
      <div className="flex min-h-dvh items-center justify-center bg-[var(--paper)] p-6 text-[var(--ink)]">
        <div className="editorial-card max-w-md p-6">
          <div className="mb-3 flex items-center gap-3">
            <span className="mark" aria-hidden="true">
              M!
            </span>
            <h1 className="font-display text-2xl">Mangora</h1>
          </div>
          {errorMessage ? (
            <div className="space-y-4">
              <p className="text-sm text-[var(--red-text)]">{fallbackT.errors.loadFailed}</p>
              <pre className="border border-[var(--ink)] bg-[var(--surface)] p-3 font-mono-editorial text-xs whitespace-pre-wrap">
                {errorMessage}
              </pre>
              <button
                type="button"
                onClick={() => void loadInitial()}
                className="editorial-btn-primary text-xs"
              >
                {fallbackT.errors.retry}
              </button>
            </div>
          ) : (
            <p className="font-mono-editorial text-xs text-[var(--muted)]">
              &gt;_ Inicializando almacenamiento compartido...
            </p>
          )}
        </div>
      </div>
    );
  }

  const showOnboarding = !appState.onboardingCompleted || replayOnboarding;

  if (showOnboarding) {
    return (
      <OnboardingWizard
        appState={appState}
        isReplay={replayOnboarding}
        isSaving={isSaving}
        errorMessage={errorMessage}
        onComplete={async ({ uiLanguage, favoriteGenres, provider, themeMode }) => {
          await runMutation(async () => {
            const next = await saveOnboardingPreferences({
              uiLanguage, favoriteGenres, provider, themeMode, onboardingCompleted: true,
            });
            setSelectedSeries(null);
            setReplayOnboarding(false);
            setActiveSection("home");
            return next;
          });
        }}
        onCancelReplay={() => setReplayOnboarding(false)}
      />
    );
  }

  const handleSwitchThemeMode = async (mode: ThemeMode) => {
    await Promise.all([
      runMutation(() => setThemeMode(mode), true),
      updateAppearance({ palette: mode === "ink" ? "night_ink" : "classic_paper" }),
    ]);
  };
  const resolvedSupportConfig =
    integrationsOverview?.supportConfig ?? defaultBrowserIntegrationsOverview().supportConfig;

  const openExternalSafe = async (url: string) => {
    try {
      await openExternalUrl(url);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  };

  const navigate = (section: NavSection) => {
    if (section !== "home" && activeSection === "home" && !selectedSeries) {
      pushBackGuard();
    }
    setSelectedSeries(null);
    setNavigationTarget(null);
    setActiveSection(section);
  };

  return (
    <div className="flex h-dvh overflow-hidden bg-[var(--paper)] text-[var(--ink)]">
      <Sidebar
        activeSection={activeSection}
        onSelectSection={navigate}
        themeMode={effectiveThemeMode}
        onToggleTheme={() => void handleSwitchThemeMode(nextTheme)}
        onOpenFeedback={openFeedback}
        uiLanguage={appState.uiLanguage}
        appVersion={appState.appVersion}
        activeDownloadCount={activeDownloadCount}
      />

      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar
          activeSection={activeSection}
          uiLanguage={appState.uiLanguage}
          themeMode={effectiveThemeMode}
          activeProvider={appState.activeProvider}
          availableProviders={appState.availableProviders}
          onToggleTheme={() => void handleSwitchThemeMode(nextTheme)}
          onChangeLanguage={(lang) => void runMutation(() => setUiLanguage(lang), true)}
        />

        <main
          ref={mainRef}
          className="flex-1 overflow-x-hidden overflow-y-auto px-4 py-5 sm:px-6 md:px-10 md:py-9"
        >
          <div className="mx-auto max-w-5xl">
            {errorMessage && (
              <div className="editorial-callout mb-6 px-4 py-3 font-mono-editorial text-xs text-[var(--red-text)]">
                {errorMessage}
              </div>
            )}

            {selectedSeries ? (
              <SeriesDetailView
                selection={selectedSeries}
                language={appState.uiLanguage}
                isSaving={isSaving}
                isFavoriteInLibrary={favoriteKeys.has(
                  libraryKey(selectedSeries.mangaId, selectedSeries.provider),
                )}
                activeDownloadChapterIds={activeDownloadChapterIds}
                downloadCompletedTick={downloadCompletedTick}
                onBack={() => setSelectedSeries(null)}
                onToggleFavorite={handleToggleFavorite}
                onToggleChapterRead={handleToggleChapterRead}
                onOpenReader={openReader}
                onEnqueueDownload={handleEnqueueDownload}
                onNotify={showToast}
                onOpenExternal={async (url) => {
                  try {
                    await openExternalUrl(url);
                  } catch (err) {
                    setErrorMessage(err instanceof Error ? err.message : String(err));
                  }
                }}
              />
            ) : (
              <>
                {activeSection === "home" && (
                  <HomeView
                    appState={appState}
                    library={libraryOverview}
                    libraryLoading={libraryLoading}
                    isSaving={isSaving}
                    favoriteKeys={favoriteKeys}
                    metricsRefreshKey={downloadCompletedTick}
                    onRefreshLibrary={loadLibrary}
                    onToggleFavorite={handleToggleFavorite}
                    onOpenSeries={openSeries}
                    onSearch={() => {
                      setSelectedSeries(null);
                      setNavigationTarget("search");
                      navigate("explore");
                    }}
                    onRelaunchTour={startTourReplay}
                    onOpenSupport={() => navigate("support")}
                    onDismissSupportBanner={() => runMutation(() => dismissSupportBanner(), false)}
                  />
                )}

                {activeSection === "explore" && (
                  <ExploreView
                    appState={appState}
                    isSaving={isSaving}
                    focusSearch={navigationTarget === "search"}
                    favoriteKeys={favoriteKeys}
                    onToggleFavorite={handleToggleFavorite}
                    onOpenSeries={openSeries}
                    onSelectProvider={(provider) =>
                      runMutation(() => setDefaultProvider(provider), true)
                    }
                    onEditGenres={() => {
                      setSelectedSeries(null);
                      setNavigationTarget("genres");
                      setActiveSection("settings");
                    }}
                  />
                )}

                {activeSection === "downloads" && (
                  <DownloadsView
                    appState={appState}
                    queue={downloadQueue}
                    offlineSeries={offlineSeries}
                    offlineLoading={offlineLoading}
                    isMutating={isSaving}
                    onOpenDownloadsFolder={async () => {
                      try {
                        await openDownloadsFolder();
                      } catch (err) {
                        setErrorMessage(err instanceof Error ? err.message : String(err));
                      }
                    }}
                    onChangeDownloadFormat={handleChangeDownloadFormat}
                    onControlQueue={handleControlQueue}
                    onRefreshOffline={loadOfflineSeries}
                    onOpenSeries={openSeries}
                  />
                )}

                {activeSection === "settings" && (
                  <SettingsView
                    appState={appState}
                    overview={integrationsOverview}
                    focusGenres={navigationTarget === "genres"}
                    isSaving={isSaving}
                    onChangeLanguage={(lang) => runMutation(() => setUiLanguage(lang), true)}
                    onChangeTheme={(mode) => handleSwitchThemeMode(mode)}
                    onChangeProvider={(provider) =>
                      runMutation(() => setDefaultProvider(provider), true)
                    }
                    onSaveGenres={(genres) => runMutation(() => setFavoriteGenres(genres), true)}
                    onRelaunchTour={startTourReplay}
                    onOpenDataFolder={async () => {
                      try {
                        await openDataFolder();
                      } catch (err) {
                        setErrorMessage(err instanceof Error ? err.message : String(err));
                      }
                    }}
                    onOpenExternal={openExternalSafe}
                    onOpenFeedback={openFeedback}
                    onOpenMangaTui={async () => {
                      try {
                        await openMangaTuiDesktop();
                      } catch (err) {
                        setErrorMessage(err instanceof Error ? err.message : String(err));
                      }
                    }}
                    onOverviewChanged={(nextOverview, nextAppState) => {
                      setIntegrationsOverview(nextOverview);
                      if (nextAppState) {
                        setAppState(nextAppState);
                        void loadLibrary();
                      }
                    }}
                    onBackupImported={(res) => {
                      setAppState(res.appState);
                      setLibraryOverview(res.library);
                    }}
                    onNotify={showToast}
                  />
                )}

                {activeSection === "support" && (
                  <SupportView
                    appState={appState}
                    supportConfig={resolvedSupportConfig}
                    onOpenExternal={openExternalSafe}
                    onOpenFeedback={openFeedback}
                  />
                )}
              </>
            )}
          </div>
        </main>

        <BottomNav
          activeSection={activeSection}
          onSelectSection={navigate}
          uiLanguage={appState.uiLanguage}
          activeDownloadCount={activeDownloadCount}
        />
      </div>

      {readerTarget && (
        <ReaderView
          request={readerTarget}
          language={appState.uiLanguage}
          initialPrefs={appState.readerPrefs}
          onUpdatePrefs={handleUpdateReaderPrefs}
          onClose={handleCloseReader}
          onNotify={showToast}
          onRegisterBackHandler={(handler) => {
            readerBackHandlerRef.current = handler;
          }}
        />
      )}

      <FeedbackModal
        open={feedbackOpen}
        language={appState.uiLanguage}
        onClose={() => setFeedbackOpen(false)}
        onOpenExternal={openExternalSafe}
        onNotify={showToast}
      />

      {toastMessage && (
        <div
          role="status"
          aria-live="polite"
          className="ink-bar fixed right-4 bottom-[calc(4.75rem+var(--safe-bottom))] z-[60] border-2 border-[var(--ink)] px-4 py-2 text-xs font-bold shadow-[4px_4px_0_var(--red)] md:right-6 md:bottom-5"
        >
          ✓ {toastMessage}
        </div>
      )}
    </div>
  );
}

export default App;
