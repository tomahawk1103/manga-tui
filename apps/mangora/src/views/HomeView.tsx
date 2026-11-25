import { Compass, RotateCcw, X, Zap } from "lucide-react";
import { LibraryShelf } from "../components/LibraryShelf";
import type {
  InitialAppState,
  LibraryOverview,
  SeriesSelection,
  ToggleFavoritePayload,
} from "../types/mangora";
import { getGenreLabel, getTranslations } from "../lib/i18n";

interface HomeViewProps {
  appState: InitialAppState;
  library: LibraryOverview | null;
  libraryLoading: boolean;
  isSaving: boolean;
  favoriteKeys: Set<string>;
  onRefreshLibrary: (search?: string) => Promise<void>;
  onToggleFavorite: (payload: ToggleFavoritePayload) => Promise<void>;
  onOpenSeries: (series: SeriesSelection) => void;
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
  onRefreshLibrary,
  onToggleFavorite,
  onOpenSeries,
  onSearch,
  onRelaunchTour,
  onOpenSupport,
  onDismissSupportBanner,
}: HomeViewProps) {
  const t = getTranslations(appState.uiLanguage);
  const activeProvider = appState.availableProviders.find((p) => p.id === appState.activeProvider);
  const showSupportMilestoneBanner =
    appState.library.chaptersRead >= 5 &&
    !appState.supportBannerDismissed &&
    Boolean(onOpenSupport) &&
    Boolean(onDismissSupportBanner);

  return (
    <div className="space-y-8">
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
              <span>&gt;_ SQLite · manga-tui-history.db</span>
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

      {/* Interactive Shared SQLite Library */}
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
          {appState.library.hasExistingData ? t.home.detectedBannerDesc : t.home.emptyBannerDesc}
        </p>
      </section>

      {/* Two-column editorial summary */}
      <section className="grid gap-6 md:grid-cols-2">
        <article className="editorial-card-muted p-5">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [01] {t.home.activeProfileTitle}
          </div>
          <div className="mb-4">
            <div className="font-mono-editorial text-xs text-[var(--muted)]">
              {t.home.activeProviderLabel}
            </div>
            <div className="font-display mt-0.5 text-xl">
              {activeProvider?.badge ?? appState.activeProvider}
            </div>
          </div>

          <div>
            <div className="font-mono-editorial mb-2 text-xs text-[var(--muted)]">
              {t.home.activeGenresLabel}
            </div>
            {appState.favoriteGenres.length > 0 ? (
              <div className="flex flex-wrap gap-2">
                {appState.favoriteGenres.map((genreId) => (
                  <span
                    key={genreId}
                    className="border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1 font-mono-editorial text-xs font-bold"
                  >
                    {getGenreLabel(genreId, appState.uiLanguage)}
                  </span>
                ))}
              </div>
            ) : (
              <p className="text-xs text-[var(--muted)]">{t.home.noGenresYet}</p>
            )}
          </div>
        </article>

        <article className="editorial-card-muted p-5">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [02] {t.home.nextMilestoneTitle}
          </div>
          <p className="text-sm text-[var(--muted)]">{t.home.nextMilestoneDesc}</p>
        </article>
      </section>
    </div>
  );
}
