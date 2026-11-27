import { useMemo, useState } from "react";
import { BookOpen, Bookmark, Compass, HardDrive, Heart, RefreshCw, Search } from "lucide-react";
import { CoverImage } from "./CoverImage";
import {
  CollectionDisplayToolbar,
  type LibrarySortBy,
  sortLibraryItems,
  useMangoraAppearance,
} from "../features/hito5";
import { getTranslations } from "../lib/i18n";
import {
  type LibraryEntry,
  type LibraryOverview,
  type SeriesSelection,
  type ToggleFavoritePayload,
  type UiLanguage,
  libraryKey,
} from "../types/mangora";

type LibraryTab = "continue" | "favorites" | "recent";

interface LibraryShelfProps {
  library: LibraryOverview | null;
  language: UiLanguage;
  isLoading: boolean;
  isMutating: boolean;
  favoriteKeys: Set<string>;
  onRefresh: (search?: string) => Promise<void>;
  onToggleFavorite: (payload: ToggleFavoritePayload) => Promise<void>;
  onOpenSeries: (series: SeriesSelection) => void;
  onExplore: () => void;
}

export function LibraryShelf({
  library,
  language,
  isLoading,
  isMutating,
  favoriteKeys,
  onRefresh,
  onToggleFavorite,
  onOpenSeries,
  onExplore,
}: LibraryShelfProps) {
  const t = getTranslations(language).library;
  const { config: appearance, updateAppearance } = useMangoraAppearance();
  const [activeTab, setActiveTab] = useState<LibraryTab>("continue");
  const [filterText, setFilterText] = useState("");

  const tabs: Array<{ id: LibraryTab; label: string; count: number }> = [
    { id: "continue", label: t.tabs.continue, count: library?.totalContinueReading ?? 0 },
    { id: "favorites", label: t.tabs.favorites, count: library?.totalFavorites ?? 0 },
    { id: "recent", label: t.tabs.recent, count: library?.totalRecent ?? 0 },
  ];

  const rawItems: LibraryEntry[] = !library
    ? []
    : activeTab === "continue"
      ? library.continueReading
      : activeTab === "favorites"
        ? library.favorites
        : library.recentHistory;

  const needle = filterText.trim().toLowerCase();
  const visibleItems = useMemo(() => {
    const filtered = needle
      ? rawItems.filter((item) => item.title.toLowerCase().includes(needle))
      : rawItems;
    const mapped = filtered.map((item) => ({
      ...item,
      last_read: item.lastRead,
      chapters_read: item.totalReadCount,
      is_favorite: item.isFavorite || favoriteKeys.has(libraryKey(item.id, item.provider)),
    }));
    return sortLibraryItems(mapped, appearance.library_sort);
  }, [rawItems, needle, favoriteKeys, appearance.library_sort]);

  const emptyCopy = needle
    ? t.emptyFilter
    : activeTab === "continue"
      ? t.emptyContinue
      : activeTab === "favorites"
        ? t.emptyFavorites
        : t.emptyRecent;

  return (
    <section className="space-y-5" aria-label={t.title}>
      <div className="flex flex-wrap items-end justify-between gap-4 border-b-2 border-[var(--ink)] pb-4">
        <div>
          <p className="eyebrow mb-2">{t.eyebrow}</p>
          <h2 className="font-display text-3xl sm:text-4xl">{t.title}</h2>
          <p className="mt-1 max-w-2xl text-sm text-[var(--muted)]">{t.subtitle}</p>
        </div>

        <button
          type="button"
          disabled={isLoading}
          onClick={() => void onRefresh(filterText)}
          className="editorial-btn-secondary w-full text-xs sm:w-auto"
        >
          <RefreshCw className={`h-4 w-4 ${isLoading ? "animate-spin" : ""}`} aria-hidden="true" />
          <span>{t.refreshBtn}</span>
        </button>
      </div>

      {/* Tabs + local search filter */}
      <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
        <div role="tablist" aria-label={t.title} className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          {tabs.map((tab) => {
            const selected = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={selected}
                onClick={() => setActiveTab(tab.id)}
                className={`flex min-h-[44px] cursor-pointer touch-manipulation items-center justify-between gap-2 border-2 border-[var(--ink)] px-3.5 py-2 text-left text-xs font-bold transition-colors ${
                  selected
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                    : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                }`}
              >
                <span className="truncate">{tab.label}</span>
                <span
                  className={`border px-1.5 py-0.5 font-mono-editorial text-[11px] ${
                    selected
                      ? "border-[var(--red)] bg-[var(--red)] text-white"
                      : "border-[var(--ink)] bg-[var(--paper)] text-[var(--ink)]"
                  }`}
                >
                  {tab.count}
                </span>
              </button>
            );
          })}
        </div>

        <label className="relative block w-full lg:w-72">
          <span className="sr-only">{t.searchPlaceholder}</span>
          <Search
            className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-[var(--muted)]"
            aria-hidden="true"
          />
          <input
            type="search"
            value={filterText}
            onChange={(event) => setFilterText(event.target.value)}
            placeholder={t.searchPlaceholder}
            className="min-h-[44px] w-full border-2 border-[var(--ink)] bg-[var(--paper)] py-2 pr-3 pl-9 text-sm text-[var(--ink)] placeholder:text-[var(--muted)]"
          />
        </label>
      </div>

      {/* Hito 5 Display Toolbar */}
      <CollectionDisplayToolbar
        mode="library"
        language={language}
        density={appearance.library_density}
        onDensityChange={(library_density) => void updateAppearance({ library_density })}
        viewMode={appearance.library_view_mode}
        onViewModeChange={(library_view_mode) => void updateAppearance({ library_view_mode })}
        sortBy={appearance.library_sort}
        onSortChange={(sort) => void updateAppearance({ library_sort: sort as LibrarySortBy })}
        itemCount={visibleItems.length}
      />

      {isLoading && !library ? (
        <div className="editorial-card-muted p-6 font-mono-editorial text-xs text-[var(--muted)]">
          {t.loading}
        </div>
      ) : visibleItems.length === 0 ? (
        <div className="editorial-callout flex flex-col items-start justify-between gap-4 p-5 sm:flex-row sm:items-center">
          <p className="text-sm text-[var(--muted)]">{emptyCopy}</p>
          <button
            type="button"
            onClick={onExplore}
            className="editorial-btn-primary shrink-0 text-xs"
          >
            <Compass className="h-4 w-4" aria-hidden="true" />
            <span>{t.emptyCta}</span>
          </button>
        </div>
      ) : (
        <ul
          className={
            appearance.library_view_mode === "list"
              ? "mangora-library-list"
              : "mangora-library-grid"
          }
        >
          {visibleItems.map((entry) => {
            const isFav =
              entry.isFavorite || favoriteKeys.has(libraryKey(entry.id, entry.provider));
            return (
              <li
                key={libraryKey(entry.id, entry.provider)}
                className="editorial-card flex min-w-0 flex-col justify-between overflow-hidden"
              >
                <button
                  type="button"
                  onClick={() =>
                    onOpenSeries({
                      mangaId: entry.id,
                      provider: entry.provider,
                      title: entry.title,
                      coverUrl: entry.coverUrl ?? null,
                    })
                  }
                  className="group flex w-full min-w-0 flex-1 cursor-pointer touch-manipulation gap-3.5 p-3.5 text-left transition-colors hover:bg-[var(--surface)]/50"
                >
                  <div className="relative h-28 w-20 shrink-0 overflow-hidden border-2 border-[var(--ink)] bg-[var(--surface)]">
                    <CoverImage
                      src={entry.coverUrl}
                      provider={entry.provider}
                      title={entry.title}
                      className="h-full w-full"
                      fallbackTextClassName="px-1.5 py-0.5 font-mono-editorial text-xs font-bold"
                    />
                  </div>

                  <div className="flex min-w-0 flex-1 flex-col justify-between gap-2">
                    <div>
                      <div className="mb-1 flex flex-wrap items-center gap-1.5">
                        <span className="border border-[var(--ink)] bg-[var(--surface)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold">
                          {entry.providerLabel}
                        </span>
                        {entry.downloadedCount > 0 && (
                          <span className="inline-flex items-center gap-1 border border-[var(--ink)] bg-[var(--ink)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold text-[var(--paper)]">
                            <HardDrive className="h-2.5 w-2.5" aria-hidden="true" />
                            {t.downloadedBadge(entry.downloadedCount)}
                          </span>
                        )}
                      </div>
                      <h3 className="font-display line-clamp-2 text-base leading-tight group-hover:text-[var(--red-text)]">
                        {entry.title}
                      </h3>
                    </div>

                    <div className="space-y-1 font-mono-editorial text-[11px] text-[var(--muted)]">
                      {entry.lastReadChapter && (
                        <div className="truncate text-[var(--ink)]">
                          {t.lastChapterPrefix} {entry.lastReadChapter}
                        </div>
                      )}
                      {entry.bookmarkChapter && (
                        <div className="inline-flex max-w-full items-center gap-1 truncate text-[var(--red-text)]">
                          <Bookmark className="h-3 w-3 shrink-0" aria-hidden="true" />
                          <span className="truncate">
                            {t.bookmarkBadge(entry.bookmarkChapter, entry.bookmarkPage)}
                          </span>
                        </div>
                      )}
                      {entry.totalReadCount > 0 && <div>{t.chaptersReadBadge(entry.totalReadCount)}</div>}
                      {!entry.totalReadCount && entry.isPlanToRead && (
                        <div>{t.planToReadBadge}</div>
                      )}
                    </div>
                  </div>
                </button>

                <div className="flex items-center justify-between gap-2 border-t-2 border-[var(--ink)] bg-[var(--surface)] px-3 py-2">
                  <button
                    type="button"
                    onClick={() =>
                      onOpenSeries({
                        mangaId: entry.id,
                        provider: entry.provider,
                        title: entry.title,
                        coverUrl: entry.coverUrl ?? null,
                      })
                    }
                    className="editorial-btn-secondary inline-flex min-h-[44px] min-w-0 flex-1 cursor-pointer touch-manipulation items-center justify-center gap-1.5 px-3 py-1.5 text-xs font-bold"
                  >
                    <BookOpen className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                    <span className="truncate">{t.openSeriesBtn}</span>
                  </button>
                  <button
                    type="button"
                    disabled={isMutating}
                    aria-pressed={isFav}
                    aria-label={isFav ? t.removeFavorite : t.addFavorite}
                    title={isFav ? t.removeFavorite : t.addFavorite}
                    onClick={() =>
                      void onToggleFavorite({
                        mangaId: entry.id,
                        provider: entry.provider,
                        title: entry.title,
                        coverUrl: entry.coverUrl,
                      })
                    }
                    className={`inline-flex min-h-[44px] min-w-[44px] shrink-0 cursor-pointer touch-manipulation items-center justify-center border-2 border-[var(--ink)] transition-colors ${
                      isFav
                        ? "bg-[var(--red)] text-white"
                        : "bg-[var(--paper)] text-[var(--ink)] hover:bg-[var(--ink)] hover:text-[var(--paper)]"
                    }`}
                  >
                    <Heart
                      className={`h-4 w-4 ${isFav ? "fill-current" : ""}`}
                      aria-hidden="true"
                    />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
