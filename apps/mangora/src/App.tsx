import { useCallback, useEffect, useState } from "react";
import { BottomNav } from "./components/BottomNav";
import { OnboardingWizard } from "./components/OnboardingWizard";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { getTranslations } from "./lib/i18n";
import {
  getInitialState,
  openDataFolder,
  openDownloadsFolder,
  openExternalUrl,
  saveOnboardingPreferences,
  setDefaultProvider,
  setFavoriteGenres,
  setThemeMode,
  setUiLanguage,
} from "./lib/tauri";
import type { InitialAppState, NavSection, ThemeMode } from "./types/mangora";
import { DownloadsView } from "./views/DownloadsView";
import { ExploreView } from "./views/ExploreView";
import { HomeView } from "./views/HomeView";
import { SettingsView } from "./views/SettingsView";
import { SupportView } from "./views/SupportView";

export function App() {
  const [appState, setAppState] = useState<InitialAppState | null>(null);
  const [activeSection, setActiveSection] = useState<NavSection>("home");
  const [navigationTarget, setNavigationTarget] = useState<"genres" | "search" | null>(null);
  const [replayOnboarding, setReplayOnboarding] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const loadInitial = useCallback(async () => {
    setErrorMessage(null);
    try {
      const next = await getInitialState();
      setAppState(next);
    } catch (err) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void loadInitial();
  }, [loadInitial]);

  useEffect(() => {
    if (!appState) return;
    document.documentElement.dataset.theme = appState.themeMode;
    document.documentElement.lang = appState.uiLanguage;
    const themeColorMeta = document.querySelector('meta[name="theme-color"]');
    if (themeColorMeta) {
      themeColorMeta.setAttribute("content", appState.themeMode === "ink" ? "#141413" : "#f8f7f1");
    }
  }, [appState]);

  const showToast = useCallback((message: string) => {
    setToastMessage(message);
    window.setTimeout(() => {
      setToastMessage((prev) => (prev === message ? null : prev));
    }, 2200);
  }, []);

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
            setReplayOnboarding(false);
            setActiveSection("home");
            return next;
          });
        }}
        onCancelReplay={() => setReplayOnboarding(false)}
      />
    );
  }

  const nextTheme: ThemeMode = appState.themeMode === "paper" ? "ink" : "paper";

  const navigate = (section: NavSection) => {
    setNavigationTarget(null);
    setActiveSection(section);
  };

  return (
    <div className="flex h-dvh overflow-hidden bg-[var(--paper)] text-[var(--ink)]">
      <Sidebar
        activeSection={activeSection}
        onSelectSection={navigate}
        themeMode={appState.themeMode}
        onToggleTheme={() => void runMutation(() => setThemeMode(nextTheme), true)}
        uiLanguage={appState.uiLanguage}
        appVersion={appState.appVersion}
      />

      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar
          activeSection={activeSection}
          uiLanguage={appState.uiLanguage}
          themeMode={appState.themeMode}
          activeProvider={appState.activeProvider}
          availableProviders={appState.availableProviders}
          onToggleTheme={() => void runMutation(() => setThemeMode(nextTheme), true)}
          onChangeLanguage={(lang) => void runMutation(() => setUiLanguage(lang), true)}
        />

        <main className="flex-1 overflow-x-hidden overflow-y-auto px-4 py-5 sm:px-6 md:px-10 md:py-9">
          <div className="mx-auto max-w-5xl">
            {errorMessage && (
              <div className="editorial-callout mb-6 px-4 py-3 font-mono-editorial text-xs text-[var(--red-text)]">
                {errorMessage}
              </div>
            )}

            {activeSection === "home" && (
              <HomeView
                appState={appState}
                onSearch={() => {
                  setNavigationTarget("search");
                  setActiveSection("explore");
                }}
                onRelaunchTour={() => setReplayOnboarding(true)}
              />
            )}

            {activeSection === "explore" && (
              <ExploreView
                appState={appState}
                isSaving={isSaving}
                focusSearch={navigationTarget === "search"}
                onSelectProvider={(provider) =>
                  runMutation(() => setDefaultProvider(provider), true)
                }
                onEditGenres={() => {
                  setNavigationTarget("genres");
                  setActiveSection("settings");
                }}
              />
            )}

            {activeSection === "downloads" && (
              <DownloadsView
                appState={appState}
                onOpenDownloadsFolder={async () => {
                  try {
                    await openDownloadsFolder();
                  } catch (err) {
                    setErrorMessage(err instanceof Error ? err.message : String(err));
                  }
                }}
              />
            )}

            {activeSection === "settings" && (
              <SettingsView
                appState={appState}
                focusGenres={navigationTarget === "genres"}
                isSaving={isSaving}
                onChangeLanguage={(lang) => runMutation(() => setUiLanguage(lang), true)}
                onChangeTheme={(mode) => runMutation(() => setThemeMode(mode), true)}
                onChangeProvider={(provider) =>
                  runMutation(() => setDefaultProvider(provider), true)
                }
                onSaveGenres={(genres) => runMutation(() => setFavoriteGenres(genres), true)}
                onRelaunchTour={() => setReplayOnboarding(true)}
                onOpenDataFolder={async () => {
                  try {
                    await openDataFolder();
                  } catch (err) {
                    setErrorMessage(err instanceof Error ? err.message : String(err));
                  }
                }}
              />
            )}

            {activeSection === "support" && (
              <SupportView
                appState={appState}
                onOpenExternal={async (url) => {
                  try {
                    await openExternalUrl(url);
                  } catch (err) {
                    setErrorMessage(err instanceof Error ? err.message : String(err));
                  }
                }}
              />
            )}
          </div>
        </main>

        <BottomNav
          activeSection={activeSection}
          onSelectSection={navigate}
          uiLanguage={appState.uiLanguage}
        />
      </div>

      {toastMessage && (
        <div
          role="status"
          aria-live="polite"
          className="ink-bar fixed right-4 bottom-20 border-2 border-[var(--ink)] px-4 py-2 text-xs font-bold shadow-[4px_4px_0_var(--red)] md:right-6 md:bottom-5"
        >
          ✓ {toastMessage}
        </div>
      )}
    </div>
  );
}

export default App;
