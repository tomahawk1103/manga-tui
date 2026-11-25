import { useEffect, useRef, useState } from "react";
import {
  Check,
  Cloud,
  ExternalLink,
  FolderOpen,
  Link2,
  MessageSquarePlus,
  Moon,
  RefreshCw,
  RotateCcw,
  Sun,
  Terminal,
  Unlink,
} from "lucide-react";
import type {
  InitialAppState,
  IntegrationsOverviewDto,
  OAuthStartResultDto,
  ThemeMode,
  UiLanguage,
  UpdateCheckResultDto,
} from "../types/mangora";
import { GENRE_OPTIONS, getTranslations } from "../lib/i18n";
import {
  checkMangoraUpdates,
  completeOAuthCallback,
  connectGoogleDrive,
  connectLocalCloudFolder,
  connectMyAnimeList,
  disconnectAnilist,
  disconnectCloud,
  disconnectMyAnimeList,
  isTauriRuntime,
  runCloudSyncNow,
  saveAnilistCredentials,
  syncMyAnimeListNow,
} from "../lib/tauri";

interface SettingsViewProps {
  appState: InitialAppState;
  overview: IntegrationsOverviewDto | null;
  focusGenres: boolean;
  isSaving: boolean;
  onChangeLanguage: (lang: UiLanguage) => Promise<void>;
  onChangeTheme: (mode: ThemeMode) => Promise<void>;
  onChangeProvider: (provider: string) => Promise<void>;
  onSaveGenres: (genres: string[]) => Promise<void>;
  onRelaunchTour: () => void;
  onOpenDataFolder: () => Promise<void>;
  onOpenExternal: (url: string) => Promise<void>;
  onOpenFeedback: () => void;
  onOpenMangaTui: () => Promise<void>;
  onOverviewChanged: (overview: IntegrationsOverviewDto, updatedState?: InitialAppState) => void;
  onNotify: (message: string) => void;
}

export function SettingsView({
  appState,
  overview,
  focusGenres,
  isSaving,
  onChangeLanguage,
  onChangeTheme,
  onChangeProvider,
  onSaveGenres,
  onRelaunchTour,
  onOpenDataFolder,
  onOpenExternal,
  onOpenFeedback,
  onOpenMangaTui,
  onOverviewChanged,
  onNotify,
}: SettingsViewProps) {
  const t = getTranslations(appState.uiLanguage);
  const desktop = isTauriRuntime();
  const genresHeading = useRef<HTMLHeadingElement>(null);
  const [draftGenres, setDraftGenres] = useState<string[]>(appState.favoriteGenres);

  // Cloud & OAuth state
  const [cloudBusy, setCloudBusy] = useState(false);
  const [cloudMode, setCloudMode] = useState<"libraryOnly" | "fullSync">("libraryOnly");
  const [showCloudAdvanced, setShowCloudAdvanced] = useState(false);
  const [googleClientId, setGoogleClientId] = useState("");
  const [googleClientSecret, setGoogleClientSecret] = useState("");
  const [manualFolderPath, setManualFolderPath] = useState("");

  // Trackers state
  const [anilistClientId, setAnilistClientId] = useState("");
  const [anilistToken, setAnilistToken] = useState("");
  const [anilistBusy, setAnilistBusy] = useState(false);
  const [malBusy, setMalBusy] = useState(false);
  const [showMalAdvanced, setShowMalAdvanced] = useState(false);
  const [malClientId, setMalClientId] = useState("");
  const [malClientSecret, setMalClientSecret] = useState("");

  // Deep-link OAuth pending state (for mobile / manual code completion)
  const [pendingOAuth, setPendingOAuth] = useState<OAuthStartResultDto | null>(null);
  const [oauthCallbackInput, setOauthCallbackInput] = useState("");

  // Updates check state
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResultDto | null>(null);

  // Action error banner within Settings
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    setDraftGenres(appState.favoriteGenres);
  }, [appState.favoriteGenres]);

  useEffect(() => {
    if (focusGenres) {
      genresHeading.current?.scrollIntoView({ block: "start" });
      genresHeading.current?.focus({ preventScroll: true });
    }
  }, [focusGenres]);

  const toggleDraftGenre = (id: string) => {
    setDraftGenres((prev) => (prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]));
  };

  const genresDirty =
    draftGenres.length !== appState.favoriteGenres.length ||
    draftGenres.some((g, idx) => g !== appState.favoriteGenres[idx]);

  const handleConnectDrive = async () => {
    setCloudBusy(true);
    setActionError(null);
    try {
      const res = await connectGoogleDrive({
        clientId: googleClientId.trim() || undefined,
        clientSecret: googleClientSecret.trim() || undefined,
        useDeeplink: appState.isMobile,
      });
      onOverviewChanged(res.overview);
      if (res.completed) {
        onNotify(t.settings.cloudConnectedBadge);
      } else {
        setPendingOAuth(res);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setActionError(msg);
      if (msg.toLowerCase().includes("client id")) {
        setShowCloudAdvanced(true);
      }
    } finally {
      setCloudBusy(false);
    }
  };

  const handleLinkCloudFolder = async (folderPath: string, providerLabel?: string) => {
    if (!folderPath.trim()) return;
    setCloudBusy(true);
    setActionError(null);
    try {
      const next = await connectLocalCloudFolder(folderPath.trim(), providerLabel);
      onOverviewChanged(next);
      setManualFolderPath("");
      onNotify(t.settings.savedToast);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setCloudBusy(false);
    }
  };

  const handleSyncCloudNow = async () => {
    setCloudBusy(true);
    setActionError(null);
    try {
      const res = await runCloudSyncNow(cloudMode);
      onOverviewChanged(res.overview, res.appState);
      onNotify(t.settings.cloudSyncDoneToast(res.report.totalMangas));
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setCloudBusy(false);
    }
  };

  const handleDisconnectCloud = async () => {
    setCloudBusy(true);
    setActionError(null);
    try {
      const next = await disconnectCloud();
      onOverviewChanged(next);
      onNotify(t.settings.savedToast);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setCloudBusy(false);
    }
  };

  const handleSaveAnilist = async (ev: React.FormEvent) => {
    ev.preventDefault();
    setAnilistBusy(true);
    setActionError(null);
    try {
      const next = await saveAnilistCredentials({
        clientId: anilistClientId,
        accessToken: anilistToken,
        verify: true,
      });
      onOverviewChanged(next);
      setAnilistToken("");
      onNotify(t.settings.savedToast);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setAnilistBusy(false);
    }
  };

  const handleDisconnectAnilist = async () => {
    setAnilistBusy(true);
    setActionError(null);
    try {
      const next = await disconnectAnilist();
      onOverviewChanged(next);
      onNotify(t.settings.savedToast);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setAnilistBusy(false);
    }
  };

  const handleConnectMal = async () => {
    setMalBusy(true);
    setActionError(null);
    try {
      const res = await connectMyAnimeList({
        clientId: malClientId.trim() || undefined,
        clientSecret: malClientSecret.trim() || undefined,
        useDeeplink: appState.isMobile,
      });
      onOverviewChanged(res.overview);
      if (res.completed) {
        onNotify(t.settings.savedToast);
      } else {
        setPendingOAuth(res);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setActionError(msg);
      if (msg.toLowerCase().includes("client id")) {
        setShowMalAdvanced(true);
      }
    } finally {
      setMalBusy(false);
    }
  };

  const handleSyncMalNow = async () => {
    setMalBusy(true);
    setActionError(null);
    try {
      const res = await syncMyAnimeListNow();
      onOverviewChanged(res.overview, res.appState);
      onNotify(t.settings.malSyncDoneToast(res.report.importedFromMal, res.report.pushedToMal));
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setMalBusy(false);
    }
  };

  const handleDisconnectMal = async () => {
    setMalBusy(true);
    setActionError(null);
    try {
      const next = await disconnectMyAnimeList();
      onOverviewChanged(next);
      onNotify(t.settings.savedToast);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setMalBusy(false);
    }
  };

  const handleCompletePendingOAuth = async () => {
    if (!pendingOAuth) return;
    const provider = pendingOAuth.provider === "myAnimeList" ? "myAnimeList" : "googleDrive";
    setActionError(null);
    setCloudBusy(true);
    setMalBusy(true);
    try {
      const next = await completeOAuthCallback(provider, oauthCallbackInput);
      onOverviewChanged(next);
      setPendingOAuth(null);
      setOauthCallbackInput("");
      onNotify(t.settings.savedToast);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setCloudBusy(false);
      setMalBusy(false);
    }
  };

  const handleCheckUpdates = async () => {
    setCheckingUpdates(true);
    setActionError(null);
    try {
      const info = await checkMangoraUpdates();
      setUpdateInfo(info);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setCheckingUpdates(false);
    }
  };

  const cloud = overview?.cloud;
  const anilist = overview?.trackers.anilist;
  const mal = overview?.trackers.myAnimeList;

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow mb-3">{t.settings.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.settings.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{t.settings.subtitle}</p>
      </div>

      {actionError && (
        <div className="editorial-callout px-4 py-3 text-xs font-bold text-[var(--red-text)]">
          {actionError}
        </div>
      )}

      {/* Appearance & Language */}
      <section className="editorial-card p-6">
        <h2 className="font-display mb-4 text-2xl">{t.settings.appearanceTitle}</h2>

        <div className="grid gap-6 md:grid-cols-2">
          <div>
            <div className="mb-2 text-sm font-bold">{t.settings.languageLabel}</div>
            <div className="grid grid-cols-2 gap-3">
              {(
                [
                  { id: "es", label: "Español (ES)" },
                  { id: "en", label: "English (EN)" },
                ] as const
              ).map((option) => {
                const active = appState.uiLanguage === option.id;
                return (
                  <button
                    key={option.id}
                    type="button"
                    disabled={isSaving}
                    onClick={() => void onChangeLanguage(option.id)}
                    className={`cursor-pointer border-2 border-[var(--ink)] px-4 py-3 text-left text-sm font-bold ${
                      active
                        ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                        : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                    }`}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>
          </div>

          <div>
            <div className="mb-2 text-sm font-bold">{t.settings.themeLabel}</div>
            <div className="grid grid-cols-2 gap-3">
              <button
                type="button"
                disabled={isSaving}
                onClick={() => void onChangeTheme("paper")}
                className={`cursor-pointer border-2 border-[var(--ink)] p-3 text-left ${
                  appState.themeMode === "paper"
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                    : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                }`}
              >
                <div className="mb-1 flex items-center gap-2 text-sm font-bold">
                  <Sun className="h-4 w-4" aria-hidden="true" />
                  <span>{t.settings.paperName}</span>
                </div>
                <div
                  className={`text-xs ${
                    appState.themeMode === "paper" ? "text-[var(--paper)]" : "text-[var(--muted)]"
                  }`}
                >
                  {t.settings.paperDesc}
                </div>
              </button>

              <button
                type="button"
                disabled={isSaving}
                onClick={() => void onChangeTheme("ink")}
                className={`cursor-pointer border-2 border-[var(--ink)] p-3 text-left ${
                  appState.themeMode === "ink"
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                    : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                }`}
              >
                <div className="mb-1 flex items-center gap-2 text-sm font-bold">
                  <Moon className="h-4 w-4" aria-hidden="true" />
                  <span>{t.settings.inkName}</span>
                </div>
                <div
                  className={`text-xs ${
                    appState.themeMode === "ink" ? "text-[var(--paper)]" : "text-[var(--muted)]"
                  }`}
                >
                  {t.settings.inkDesc}
                </div>
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* Provider & Genres */}
      <div className="grid gap-6 lg:grid-cols-2">
        <section className="editorial-card-muted p-6">
          <h2 className="font-display mb-1 text-2xl">{t.settings.providerTitle}</h2>
          <p className="mb-4 text-sm text-[var(--muted)]">{t.settings.providerDesc}</p>
          <div className="grid gap-2 sm:grid-cols-2">
            {appState.availableProviders.map((prov) => {
              const active = appState.activeProvider === prov.id;
              return (
                <button
                  key={prov.id}
                  type="button"
                  disabled={isSaving}
                  onClick={() => void onChangeProvider(prov.id)}
                  className={`flex cursor-pointer items-center justify-between border-2 border-[var(--ink)] px-3 py-2.5 text-left text-xs font-bold ${
                    active
                      ? "bg-[var(--ink)] text-[var(--paper)]"
                      : "bg-[var(--paper)] text-[var(--ink)] hover:bg-[var(--surface)]"
                  }`}
                >
                  <span>{prov.badge}</span>
                  {active && <Check className="h-3.5 w-3.5 text-[var(--paper)]" />}
                </button>
              );
            })}
          </div>
        </section>

        <section className="editorial-card-muted p-6">
          <h2 ref={genresHeading} tabIndex={-1} className="font-display mb-1 scroll-mt-6 text-2xl">
            {t.settings.genresTitle}
          </h2>
          <p className="mb-4 text-sm text-[var(--muted)]">{t.settings.genresDesc}</p>
          <div className="mb-4 flex flex-wrap gap-2">
            {GENRE_OPTIONS.map((genre) => {
              const selected = draftGenres.includes(genre.id);
              return (
                <button
                  key={genre.id}
                  type="button"
                  data-selected={selected}
                  onClick={() => toggleDraftGenre(genre.id)}
                  className="editorial-chip px-3 py-1.5 text-xs"
                >
                  {selected && <Check className="h-3 w-3 text-[var(--red-text)]" />}
                  <span>{appState.uiLanguage === "en" ? genre.labelEn : genre.labelEs}</span>
                </button>
              );
            })}
          </div>
          <button
            type="button"
            disabled={isSaving || !genresDirty}
            onClick={() => void onSaveGenres(draftGenres)}
            className="editorial-btn-primary text-xs"
          >
            <span>{t.settings.saveGenresBtn}</span>
          </button>
        </section>
      </div>

      {/* Pending Mobile / Deep-link OAuth completion box */}
      {pendingOAuth && (
        <section className="editorial-callout space-y-3 p-5">
          <h3 className="font-display text-xl">{t.settings.oauthPendingTitle}</h3>
          <p className="text-sm text-[var(--muted)]">{t.settings.oauthPendingDesc}</p>
          <div className="flex flex-wrap gap-2">
            <input
              type="text"
              value={oauthCallbackInput}
              onChange={(e) => setOauthCallbackInput(e.target.value)}
              placeholder={t.settings.oauthCallbackPlaceholder}
              className="min-w-[240px] flex-1 border-2 border-[var(--ink)] bg-[var(--paper)] px-3 py-2 font-mono-editorial text-xs text-[var(--ink)]"
            />
            <button
              type="button"
              disabled={!oauthCallbackInput.trim() || cloudBusy || malBusy}
              onClick={() => void handleCompletePendingOAuth()}
              className="editorial-btn-primary text-xs"
            >
              {t.settings.oauthCompleteBtn}
            </button>
          </div>
        </section>
      )}

      {/* Cloud Sync & Trackers (Hito 4) */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* Cloud Sync */}
        <section className="editorial-card space-y-4 p-6">
          <div className="flex items-start justify-between gap-2">
            <div>
              <h2 className="font-display flex items-center gap-2 text-2xl">
                <Cloud className="h-5 w-5 text-[var(--red-text)]" aria-hidden="true" />
                <span>{t.settings.cloudTitle}</span>
              </h2>
              <p className="mt-1 text-sm text-[var(--muted)]">{t.settings.cloudDesc}</p>
            </div>
            <span
              className={`shrink-0 border border-[var(--ink)] px-2 py-1 font-mono-editorial text-[11px] font-bold uppercase ${
                cloud?.connected
                  ? "bg-[var(--ink)] text-[var(--paper)]"
                  : "bg-[var(--surface)] text-[var(--muted)]"
              }`}
            >
              {cloud?.connected ? t.settings.cloudConnectedBadge : t.settings.cloudDisconnectedBadge}
            </span>
          </div>

          <div className="editorial-box-thin p-3 font-mono-editorial text-xs">
            <div className="font-bold">{cloud?.summary ?? t.settings.cloudDisconnectedBadge}</div>
            {cloud?.lastSyncAt && (
              <div className="mt-1 text-[var(--muted)]">
                {t.settings.cloudLastSyncLabel} {cloud.lastSyncAt}
              </div>
            )}
          </div>

          {cloud?.connected ? (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <select
                  aria-label="Cloud sync mode"
                  value={cloudMode}
                  onChange={(e) => setCloudMode(e.target.value as "libraryOnly" | "fullSync")}
                  className="min-h-[42px] border-2 border-[var(--ink)] bg-[var(--paper)] px-3 text-xs font-bold text-[var(--ink)]"
                >
                  <option value="libraryOnly">{t.settings.cloudModeLibrary}</option>
                  <option value="fullSync">{t.settings.cloudModeFull}</option>
                </select>
                <button
                  type="button"
                  disabled={cloudBusy}
                  onClick={() => void handleSyncCloudNow()}
                  className="editorial-btn-primary text-xs"
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${cloudBusy ? "animate-spin" : ""}`} aria-hidden="true" />
                  <span>{cloudBusy ? t.settings.cloudSyncingBtn : t.settings.cloudSyncNowBtn}</span>
                </button>
                <button
                  type="button"
                  disabled={cloudBusy}
                  onClick={() => void handleDisconnectCloud()}
                  className="editorial-btn-secondary text-xs"
                >
                  <Unlink className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{t.settings.cloudDisconnectBtn}</span>
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  disabled={cloudBusy}
                  onClick={() => void handleConnectDrive()}
                  className="editorial-btn-primary text-xs"
                >
                  <Link2 className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{cloudBusy ? t.settings.cloudConnectingBtn : t.settings.cloudConnectDriveBtn}</span>
                </button>
                <button
                  type="button"
                  onClick={() => setShowCloudAdvanced((prev) => !prev)}
                  className="cursor-pointer font-mono-editorial text-xs font-bold underline"
                >
                  {t.settings.cloudAdvancedToggle}
                </button>
              </div>

              {showCloudAdvanced && (
                <div className="editorial-box-thin space-y-2 p-3">
                  {!cloud?.hasEmbeddedOauth && !cloud?.savedClientId && (
                    <p className="text-xs text-[var(--muted)]">{t.settings.cloudMissingClientIdHint}</p>
                  )}
                  <input
                    type="text"
                    value={googleClientId}
                    onChange={(e) => setGoogleClientId(e.target.value)}
                    placeholder={cloud?.savedClientId ?? t.settings.cloudClientIdPlaceholder}
                    className="w-full border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-mono-editorial text-xs text-[var(--ink)]"
                  />
                  <input
                    type="password"
                    value={googleClientSecret}
                    onChange={(e) => setGoogleClientSecret(e.target.value)}
                    placeholder={t.settings.cloudClientSecretPlaceholder}
                    className="w-full border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-mono-editorial text-xs text-[var(--ink)]"
                  />
                </div>
              )}

              {cloud && cloud.detectedFolders.length > 0 && (
                <div>
                  <div className="mb-1.5 text-xs font-bold text-[var(--muted)]">
                    {t.settings.cloudDetectedFoldersLabel}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    {cloud.detectedFolders.map((folder) => (
                      <button
                        key={folder.path}
                        type="button"
                        disabled={cloudBusy}
                        onClick={() => void handleLinkCloudFolder(folder.path, folder.providerLabel)}
                        className="editorial-chip px-3 py-1.5 text-xs"
                      >
                        <FolderOpen className="h-3.5 w-3.5 text-[var(--red-text)]" aria-hidden="true" />
                        <span>{folder.providerLabel}</span>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {!appState.isMobile && (
                <div className="flex flex-wrap gap-2">
                  <input
                    type="text"
                    value={manualFolderPath}
                    onChange={(e) => setManualFolderPath(e.target.value)}
                    placeholder={t.settings.cloudManualFolderPlaceholder}
                    className="min-w-[180px] flex-1 border-2 border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-mono-editorial text-xs text-[var(--ink)]"
                  />
                  <button
                    type="button"
                    disabled={cloudBusy || !manualFolderPath.trim()}
                    onClick={() => void handleLinkCloudFolder(manualFolderPath)}
                    className="editorial-btn-secondary text-xs"
                  >
                    {t.settings.cloudLinkFolderBtn}
                  </button>
                </div>
              )}
            </div>
          )}
        </section>

        {/* Trackers: AniList & MyAnimeList */}
        <section className="editorial-card space-y-5 p-6">
          <div>
            <h2 className="font-display text-2xl">{t.settings.trackersTitle}</h2>
            <p className="mt-1 text-sm text-[var(--muted)]">{t.settings.trackersDesc}</p>
          </div>

          {/* AniList block */}
          <div className="editorial-box-thin space-y-3 p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="font-display text-lg">{t.settings.anilistLabel}</span>
              <button
                type="button"
                onClick={() =>
                  void onOpenExternal(
                    "https://anilist.co/api/v2/oauth/authorize?client_id=18641&response_type=token",
                  )
                }
                className="cursor-pointer font-mono-editorial text-xs font-bold text-[var(--red-text)] underline"
              >
                {t.settings.anilistGetTokenLink}
              </button>
            </div>

            {anilist?.connected ? (
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-mono-editorial text-xs font-bold">
                  {t.settings.anilistConnectedLabel(anilist.clientIdHint)}
                </span>
                <button
                  type="button"
                  disabled={anilistBusy}
                  onClick={() => void handleDisconnectAnilist()}
                  className="editorial-btn-secondary text-xs"
                >
                  <Unlink className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{t.settings.anilistDisconnectBtn}</span>
                </button>
              </div>
            ) : (
              <form onSubmit={(e) => void handleSaveAnilist(e)} className="grid gap-2 sm:grid-cols-[0.7fr_1.3fr_auto]">
                <input
                  type="text"
                  value={anilistClientId}
                  onChange={(e) => setAnilistClientId(e.target.value)}
                  placeholder={t.settings.anilistClientIdLabel}
                  className="border-2 border-[var(--ink)] bg-[var(--paper)] px-2.5 py-2 font-mono-editorial text-xs text-[var(--ink)]"
                />
                <input
                  type="password"
                  value={anilistToken}
                  onChange={(e) => setAnilistToken(e.target.value)}
                  placeholder={t.settings.anilistTokenLabel}
                  className="border-2 border-[var(--ink)] bg-[var(--paper)] px-2.5 py-2 font-mono-editorial text-xs text-[var(--ink)]"
                />
                <button
                  type="submit"
                  disabled={anilistBusy || !anilistClientId.trim() || !anilistToken.trim()}
                  className="editorial-btn-primary text-xs"
                >
                  {anilistBusy ? t.settings.anilistSavingBtn : t.settings.anilistSaveBtn}
                </button>
              </form>
            )}
          </div>

          {/* MyAnimeList block */}
          <div className="editorial-box-thin space-y-3 p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="font-display text-lg">{t.settings.malLabel}</span>
              <span className="font-mono-editorial text-xs text-[var(--muted)]">
                {mal?.summary ?? t.settings.cloudDisconnectedBadge}
              </span>
            </div>

            {mal?.connected ? (
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  disabled={malBusy}
                  onClick={() => void handleSyncMalNow()}
                  className="editorial-btn-primary text-xs"
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${malBusy ? "animate-spin" : ""}`} aria-hidden="true" />
                  <span>{malBusy ? t.settings.malSyncingBtn : t.settings.malSyncBtn}</span>
                </button>
                <button
                  type="button"
                  disabled={malBusy}
                  onClick={() => void handleDisconnectMal()}
                  className="editorial-btn-secondary text-xs"
                >
                  <Unlink className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{t.settings.malDisconnectBtn}</span>
                </button>
              </div>
            ) : (
              <div className="space-y-2">
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    disabled={malBusy}
                    onClick={() => void handleConnectMal()}
                    className="editorial-btn-primary text-xs"
                  >
                    <Link2 className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{t.settings.malConnectBtn}</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => setShowMalAdvanced((prev) => !prev)}
                    className="cursor-pointer font-mono-editorial text-xs font-bold underline"
                  >
                    {t.settings.malAdvancedToggle}
                  </button>
                </div>

                {showMalAdvanced && (
                  <div className="grid gap-2 sm:grid-cols-2">
                    <input
                      type="text"
                      value={malClientId}
                      onChange={(e) => setMalClientId(e.target.value)}
                      placeholder={mal?.savedClientId ?? "MAL Client ID..."}
                      className="border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-mono-editorial text-xs text-[var(--ink)]"
                    />
                    <input
                      type="password"
                      value={malClientSecret}
                      onChange={(e) => setMalClientSecret(e.target.value)}
                      placeholder="MAL Client Secret (opcional)"
                      className="border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-mono-editorial text-xs text-[var(--ink)]"
                    />
                  </div>
                )}
              </div>
            )}
          </div>
        </section>
      </div>

      {/* Updates, Mailbox & Desktop TUI switch */}
      <div className="grid gap-6 lg:grid-cols-2">
        <section className="editorial-card-muted space-y-4 p-6">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 className="font-display text-2xl">{t.settings.updatesTitle}</h2>
              <p className="font-mono-editorial text-xs text-[var(--muted)]">
                {t.settings.updatesCurrentLabel}: v{appState.appVersion}
              </p>
            </div>
            <button
              type="button"
              disabled={checkingUpdates}
              onClick={() => void handleCheckUpdates()}
              className="editorial-btn-secondary text-xs"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${checkingUpdates ? "animate-spin" : ""}`} aria-hidden="true" />
              <span>{checkingUpdates ? t.settings.updatesCheckingBtn : t.settings.updatesCheckBtn}</span>
            </button>
          </div>

          {updateInfo && (
            <div className="editorial-callout space-y-2 p-4 text-xs">
              <div className="font-bold">
                {updateInfo.updateAvailable
                  ? t.settings.updatesAvailableLabel(updateInfo.latestVersion)
                  : t.settings.updatesUpToDate}
              </div>
              {updateInfo.releaseNotes && (
                <p className="whitespace-pre-wrap text-[var(--muted)]">{updateInfo.releaseNotes}</p>
              )}
              <button
                type="button"
                onClick={() => void onOpenExternal(updateInfo.releaseUrl)}
                className="editorial-btn-primary text-xs"
              >
                <span>{t.settings.updatesOpenReleaseBtn}</span>
                <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
              </button>
            </div>
          )}
        </section>

        <section className="editorial-card-muted flex flex-col justify-between gap-4 p-6">
          <div>
            <h2 className="font-display flex items-center gap-2 text-2xl">
              <MessageSquarePlus className="h-5 w-5 text-[var(--red-text)]" aria-hidden="true" />
              <span>{t.settings.mailboxCardTitle}</span>
            </h2>
            <p className="mt-1 text-sm text-[var(--muted)]">{t.settings.mailboxCardDesc}</p>
          </div>
          <div className="flex flex-wrap gap-3">
            <button type="button" onClick={onOpenFeedback} className="editorial-btn-primary text-xs">
              <MessageSquarePlus className="h-4 w-4" aria-hidden="true" />
              <span>{t.settings.mailboxOpenBtn}</span>
            </button>
            {overview?.mangaTuiDetected && !appState.isMobile && (
              <button
                type="button"
                onClick={() => void onOpenMangaTui()}
                className="editorial-btn-secondary text-xs"
              >
                <Terminal className="h-4 w-4" aria-hidden="true" />
                <span>{t.settings.desktopTuiOpenBtn}</span>
              </button>
            )}
          </div>
          {overview?.mangaTuiDetected && !appState.isMobile && (
            <p className="text-xs text-[var(--muted)]">{t.settings.desktopTuiDesc}</p>
          )}
        </section>
      </div>

      {/* Shared Storage & Tour */}
      <div className="grid gap-6 lg:grid-cols-[1.3fr_0.7fr]">
        <section className="editorial-box-thin p-6">
          <h2 className="font-display mb-4 text-2xl">{t.settings.storageTitle}</h2>
          <dl className="space-y-2.5 text-xs">
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">
                {t.settings.storageModeLabel}
              </dt>
              <dd className="font-mono-editorial font-bold">{appState.storageModeLabel}</dd>
            </div>
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">{t.settings.dataDirLabel}</dt>
              <dd className="font-mono-editorial break-all">{appState.dataDir}</dd>
            </div>
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">
                {t.settings.configDirLabel}
              </dt>
              <dd className="font-mono-editorial break-all">{appState.configDir}</dd>
            </div>
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">
                {t.settings.downloadsDirLabel}
              </dt>
              <dd className="font-mono-editorial break-all">{appState.downloadsDir}</dd>
            </div>
          </dl>

          {appState.canOpenDataFolder ? (
            <>
              <button
                type="button"
                disabled={!desktop}
                onClick={() => void onOpenDataFolder()}
                className="editorial-btn-secondary mt-5 text-xs"
              >
                <FolderOpen className="h-4 w-4" aria-hidden="true" />
                <span>{t.settings.openFolderBtn}</span>
              </button>
              {!desktop && <p className="mt-2 text-xs text-[var(--muted)]">{t.folderDesktopOnly}</p>}
            </>
          ) : (
            <p className="editorial-callout mt-5 p-3 text-xs text-[var(--muted)]">
              {t.settings.mobileStorageNote}
            </p>
          )}
        </section>

        <section className="editorial-callout flex flex-col justify-between p-6">
          <div>
            <h2 className="font-display mb-2 text-2xl">{t.settings.tourTitle}</h2>
            <p className="mb-5 text-sm text-[var(--muted)]">{t.settings.tourDesc}</p>
          </div>
          <button
            type="button"
            onClick={onRelaunchTour}
            className="editorial-btn-primary text-xs"
          >
            <RotateCcw className="h-4 w-4" aria-hidden="true" />
            <span>{t.settings.relaunchTourBtn}</span>
          </button>
        </section>
      </div>
    </div>
  );
}
