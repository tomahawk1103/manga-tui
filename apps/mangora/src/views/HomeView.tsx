import { useState } from "react";
import { BarChart3, BookOpen, Compass, RotateCcw, X, Zap } from "lucide-react";
import { LibraryShelf } from "../components/LibraryShelf";
import { ChapterUpdates } from "../components/ChapterUpdates";
import { ReaderMetricsDashboard } from "../features/hito5";
import type {
  InitialAppState,
  OpenReaderChapterRequest,
  LibraryOverview,
  SeriesSelection,
  ToggleFavoritePayload,
} from "../types/mangora";
import { getTranslations } from "../lib/i18n";
import { isTauriRuntime } from "../lib/tauri";

interface HomeViewProps {
  appState: InitialAppState;
  library: LibraryOverview | null;
  libraryLoading: boolean;
  isSaving: boolean;
  favoriteKeys: Set<string>;
  metricsRefreshKey?: number;
  onRefreshLibrary: (search?: string) => Promise<void>;
  onToggleFavorite: (payload: ToggleFavoritePayload) => Promise<void>;
  onOpenSeries: (series: SeriesSelection) => void;
  onOpenReader: (request: OpenReaderChapterRequest) => void;
  onSearch: () => void;
  onRelaunchTour: () => void;
  onOpenSupport?: () => void;
  onDismissSupportBanner?: () => Promise<void>;
}

export function HomeView({
  appState,
  library,
  libraryLoading,
  isSaving,
  favoriteKeys,
  metricsRefreshKey = 0,
  onRefreshLibrary,
  onToggleFavorite,
  onOpenSeries,
  onOpenReader,
  onSearch,
  onRelaunchTour,
  onOpenSupport,
  onDismissSupportBanner,
}: HomeViewProps) {
  const t = getTranslations(appState.uiLanguage);
  const tauriRuntime = isTauriRuntime();
  const [homeTab, setHomeTab] = useState<"shelf" | "stats">("shelf");
  const showSupportMilestoneBanner =
    appState.library.chaptersRead >= 5 &&
    !appState.supportBannerDismissed &&
    Boolean(onOpenSupport) &&
    Boolean(onDismissSupportBanner);

  return (
    <div className="space-y-8">
      <ChapterUpdates language={appState.uiLanguage}
        onSeries={item => onOpenSeries({ mangaId: item.id, provider: "mangaoni", title: item.title, coverUrl: item.cover })}
        onChapter={item => onOpenReader({
          mangaId: item.id, provider: "mangaoni", mangaTitle: item.title, coverUrl: item.cover,
          chapterId: item.id + "|" + item.chapterId, chapterTitle: item.chapterTitle,
          chapterNumber: item.chapterTitle.match(/\d+(?:\.\d+)?/)?.[0], language: "es",
        })}
      />
      {/* Hero section inspired by docs/index.html */}
      <section className="grid items-center gap-8 lg:grid-cols-[1.1fr_0.9fr]">
        <div>
          <p className="eyebrow mb-4">{t.home.eyebrow}</p>
          <h1 className="font-display mb-4 text-5xl sm:text-6xl">
            {t.home.titleLine1}
            <em className="block not-italic text-[var(--red-text)]">{t.home.titleLine2}</em>
          </h1>
          <p className="mb-6 max-w-xl text-base text-[var(--muted)] sm:text-lg">{t.home.subtitle}</p>

          <div className="flex flex-wrap gap-3">
            <button
              type="button"
              onClick={onSearch}
              className="editorial-btn-primary text-sm"
            >
              <Compass className="h-4 w-4" aria-hidden="true" />
              <span>{t.home.ctaExplore}</span>
            </button>
            <button
              type="button"
              onClick={onRelaunchTour}
              className="editorial-btn-secondary text-sm"
            >
              <RotateCcw className="h-4 w-4" aria-hidden="true" />
              <span>{t.home.ctaTour}</span>
            </button>
          </div>
        </div>

        {/* Editorial manga frame card with screentone backdrop */}
        <div className="relative isolate p-2">
          <div
            aria-hidden="true"
            className="screentone-bg pointer-events-none absolute inset-0 left-8 -z-10 rotate-1 border border-[var(--ink)]"
          />
          <div className="editorial-card overflow-hidden">
            <div className="ink-bar flex items-center justify-between border-b-2 border-[var(--ink)] px-4 py-2 text-xs">
              <span>{tauriRuntime ? ">_ SQLite · manga-tui-history.db" : ">_ IndexedDB · mangora_web_db"}</span>
              <span className="ink-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
            </div>

            <div className="grid grid-cols-2 divide-x divide-y divide-[var(--line)]">
              <div className="p-4">
                <div className="font-display text-3xl">{appState.library.libraryMangas}</div>
                <div className="font-mono-editorial text-xs text-[var(--muted)]">
                  {t.home.statSeries}
                </div>
              </div>
              <div className="p-4">
                <div className="font-display text-3xl">{appState.library.chaptersRead}</div>
                <div className="font-mono-editorial text-xs text-[var(--muted)]">
                  {t.home.statChaptersRead}
                </div>
              </div>
              <div className="p-4">
                <div className="font-display text-3xl">{appState.library.favorites}</div>
                <div className="font-mono-editorial text-xs text-[var(--muted)]">
                  {t.home.statFavorites}
                </div>
              </div>
              <div className="p-4">
                <div className="font-display text-3xl">{appState.library.offlineSeriesCount}</div>
                <div className="font-mono-editorial text-xs text-[var(--muted)]">
                  {t.home.statOfflineSeries}
                </div>
              </div>
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--line)] bg-[var(--paper)] px-4 py-3">
              <div>
                <div className="font-mono-editorial text-[11px] uppercase text-[var(--muted)]">
                  {t.home.statPagesRead}
                </div>
                <div className="font-display text-2xl text-[var(--red-text)]">
                  {appState.library.totalPagesRead.toLocaleString()}
                </div>
              </div>
              <button
                type="button"
                onClick={() => setHomeTab("stats")}
                className="editorial-btn-secondary px-3 py-1.5 text-xs"
              >
                <BarChart3 className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{t.home.tabStats}</span>
              </button>
            </div>

            {appState.library.lastReadTitle && (
              <div className="border-t-2 border-[var(--ink)] bg-[var(--surface)] px-4 py-2.5 text-xs">
                <span className="font-mono-editorial uppercase text-[var(--muted)]">
                  {t.home.lastReadLabel}:{" "}
                </span>
                <span className="font-bold">{appState.library.lastReadTitle}</span>
              </div>
            )}
          </div>
        </div>
      </section>

      {/* Library / Local Stats Switcher (Hito 5) */}
      <div className="flex flex-wrap gap-2" role="tablist" aria-label={t.home.tabShelf}>
        <button
          type="button"
          role="tab"
          aria-selected={homeTab === "shelf"}
          onClick={() => setHomeTab("shelf")}
          className={`inline-flex min-h-[44px] cursor-pointer items-center gap-2 border-2 border-[var(--ink)] px-4 py-2 text-xs font-bold ${
            homeTab === "shelf"
              ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
              : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
          }`}
        >
          <BookOpen className="h-4 w-4" aria-hidden="true" />
          <span>{t.home.tabShelf}</span>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={homeTab === "stats"}
          onClick={() => setHomeTab("stats")}
          className={`inline-flex min-h-[44px] cursor-pointer items-center gap-2 border-2 border-[var(--ink)] px-4 py-2 text-xs font-bold ${
            homeTab === "stats"
              ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
              : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
          }`}
        >
          <BarChart3 className="h-4 w-4" aria-hidden="true" />
          <span>{t.home.tabStats}</span>
        </button>
      </div>

      {homeTab === "shelf" ? (
        <LibraryShelf
          library={library}
          language={appState.uiLanguage}
          isLoading={libraryLoading}
          isMutating={isSaving}
          favoriteKeys={favoriteKeys}
          onRefresh={onRefreshLibrary}
          onToggleFavorite={onToggleFavorite}
          onOpenSeries={onOpenSeries}
          onExplore={onSearch}
        />
      ) : (
        <ReaderMetricsDashboard language={appState.uiLanguage} refreshTrigger={metricsRefreshKey} />
      )}

      {/* Subtle permanently-dismissible reading-milestone support banner */}
      {showSupportMilestoneBanner && (
        <section className="editorial-box-thin flex flex-wrap items-center justify-between gap-4 bg-[var(--surface)] p-4">
          <div className="min-w-0 flex-1">
            <div className="font-mono-editorial mb-0.5 text-[11px] font-bold uppercase text-[var(--red-text)]">
              {t.support.bannerEyebrow}
            </div>
            <p className="text-xs text-[var(--muted)]">
              {t.support.bannerText(appState.library.chaptersRead)}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button type="button" onClick={onOpenSupport} className="editorial-btn-primary text-xs">
              <Zap className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{t.support.bannerCta}</span>
            </button>
            <button
              type="button"
              disabled={isSaving}
              onClick={() => void onDismissSupportBanner?.()}
              className="editorial-btn-secondary text-xs"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{t.support.bannerDismiss}</span>
            </button>
          </div>
        </section>
      )}

      {/* Shared library status callout */}
      <section className="editorial-callout p-5 sm:p-6">
        <h2 className="font-display mb-1.5 text-2xl">
          {appState.library.hasExistingData ? t.home.detectedBannerTitle : t.home.emptyBannerTitle}
        </h2>
        <p className="max-w-3xl text-sm text-[var(--muted)]">
          {tauriRuntime
            ? appState.library.hasExistingData
              ? t.home.detectedBannerDesc
              : t.home.emptyBannerDesc
            : appState.library.hasExistingData
              ? t.home.webDetectedBannerDesc
              : t.home.webEmptyBannerDesc}
        </p>
      </section>
    </div>
  );
}
