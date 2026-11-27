import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowUpDown,
  BookOpen,
  Check,
  CheckSquare,
  ChevronLeft,
  ChevronRight,
  Download,
  ExternalLink,
  HardDrive,
  Heart,
  Languages,
  RefreshCw,
  Sparkles,
  Square,
  Star,
  Tag,
  WifiOff,
} from "lucide-react";
import { defaultHito5Bridge } from "../features/hito5";
import { getTranslations } from "../lib/i18n";
import { getAniListCommunity, getSeriesDetail, isTauriRuntime, resolveCoverImageUrl } from "../lib/tauri";
import {
  type AniListCommunityInfo,
  type EnqueueChaptersRequest,
  type OpenReaderChapterRequest,
  type ReaderChapterNeighbor,
  type SeriesChapterItem,
  type SeriesDetailResponse,
  type SeriesSelection,
  type ToggleChapterReadPayload,
  type ToggleFavoritePayload,
  type UiLanguage,
  libraryKey,
} from "../types/mangora";

const LANGUAGE_LABELS: Record<string, string> = {
  es: "Español (ES)",
  "es-la": "Español (LATAM)",
  en: "English",
  ja: "日本語",
  pt: "Português",
  "pt-br": "Português (BR)",
  fr: "Français",
  it: "Italiano",
  de: "Deutsch",
};

export interface SeriesDetailViewProps {
  selection: SeriesSelection;
  language: UiLanguage;
  isSaving: boolean;
  isFavoriteInLibrary?: boolean;
  favoriteKeys?: Set<string>;
  activeDownloadChapterIds?: Set<string>;
  downloadCompletedTick?: number;
  onBack: () => void;
  onToggleFavorite: (payload: ToggleFavoritePayload) => Promise<void>;
  onToggleChapterRead: (payload: ToggleChapterReadPayload) => Promise<boolean | void>;
  onOpenReader: (request: OpenReaderChapterRequest) => void;
  onEnqueueDownload: (payload: EnqueueChaptersRequest) => Promise<void>;
  onSelectTag?: (tag: string) => void;
  onNotify: (message: string) => void;
  onOpenExternal: (url: string) => Promise<void>;
}

function formatLanguageName(code: string): string {
  const normalized = code.trim().toLowerCase();
  if (!normalized) return "—";
  return LANGUAGE_LABELS[normalized] ?? normalized.toUpperCase();
}

function resolveStatusLabel(
  rawStatus: string | null | undefined,
  names: ReturnType<typeof getTranslations>["series"]["statusNames"],
): string {
  const key = (rawStatus ?? "").trim().toLowerCase();
  if (key.includes("ongoing") || key.includes("curso") || key.includes("public")) {
    return names.ongoing;
  }
  if (key.includes("complet") || key.includes("final")) {
    return names.completed;
  }
  if (key.includes("hiatus") || key.includes("paus")) {
    return names.hiatus;
  }
  if (key.includes("cancel") || key.includes("drop")) {
    return names.cancelled;
  }
  return names.unknown;
}

function seriesMonogram(title: string): string {
  const words = title
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return "M";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return `${words[0][0] ?? ""}${words[1][0] ?? ""}`.toUpperCase();
}

function pickNextUnreadChapter(chapters: SeriesChapterItem[]): SeriesChapterItem | null {
  const unread = chapters.filter((chapter) => !chapter.isRead);
  if (unread.length === 0) return null;

  let best = unread[0];
  let bestNum = Number.parseFloat(best.chapterNumber.trim());
  for (let idx = 1; idx < unread.length; idx += 1) {
    const candidate = unread[idx];
    const candidateNum = Number.parseFloat(candidate.chapterNumber.trim());
    const bestFinite = Number.isFinite(bestNum);
    const candidateFinite = Number.isFinite(candidateNum);
    if (candidateFinite && (!bestFinite || candidateNum < bestNum)) {
      best = candidate;
      bestNum = candidateNum;
    }
  }
  return best;
}

export function SeriesDetailView({
  selection,
  language,
  isSaving,
  isFavoriteInLibrary,
  favoriteKeys,
  activeDownloadChapterIds,
  downloadCompletedTick = 0,
  onBack,
  onToggleFavorite,
  onToggleChapterRead,
  onOpenReader,
  onEnqueueDownload,
  onSelectTag,
  onNotify,
  onOpenExternal,
}: SeriesDetailViewProps) {
  const t = getTranslations(language);
  const s = t.series;

  const [detail, setDetail] = useState<SeriesDetailResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [chapterOrder, setChapterOrder] = useState<"asc" | "desc">("desc");
  const [selectedChapterLang, setSelectedChapterLang] = useState<string | undefined>(undefined);
  const [page, setPage] = useState(1);
  const [offlineOnly, setOfflineOnly] = useState(false);
  const [togglingChapterId, setTogglingChapterId] = useState<string | null>(null);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedChapterIds, setSelectedChapterIds] = useState<Set<string>>(() => new Set());
  const [enqueuingDownloads, setEnqueuingDownloads] = useState(false);
  const [coverFailed, setCoverFailed] = useState(false);

  const [community, setCommunity] = useState<AniListCommunityInfo | null>(null);
  const [communityLoading, setCommunityLoading] = useState(true);
  const [communityError, setCommunityError] = useState<string | null>(null);

  useEffect(() => {
    setDetail(null);
    setLoadError(null);
    setSelectedChapterLang(undefined);
    setChapterOrder("desc");
    setPage(1);
    setOfflineOnly(false);
    setSelectionMode(false);
    setSelectedChapterIds(new Set());
  }, [selection.mangaId, selection.provider]);

  const loadSeries = useCallback(async () => {
    setIsLoading(true);
    setLoadError(null);
    try {
      const response = await getSeriesDetail(selection, {
        language: selectedChapterLang,
        order: chapterOrder,
        page,
      });
      setDetail(response);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoading(false);
    }
  }, [selection, selectedChapterLang, chapterOrder, page]);

  useEffect(() => {
    let active = true;
    setIsLoading(true);
    setLoadError(null);

    getSeriesDetail(selection, {
      language: selectedChapterLang,
      order: chapterOrder,
      page,
    })
      .then((response) => {
        if (!active) return;
        setDetail(response);
      })
      .catch((error) => {
        if (!active) return;
        setLoadError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (active) setIsLoading(false);
      });

    return () => {
      active = false;
    };
  }, [selection, selectedChapterLang, chapterOrder, page]);

  useEffect(() => {
    if (downloadCompletedTick <= 0) return;
    let active = true;
    getSeriesDetail(selection, {
      language: selectedChapterLang,
      order: chapterOrder,
      page,
    })
      .then((response) => {
        if (active) setDetail(response);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [downloadCompletedTick, selection, selectedChapterLang, chapterOrder, page]);

  useEffect(() => {
    if (!detail || !detail.genres || detail.genres.length === 0) return;
    void defaultHito5Bridge
      .saveMangaGenres({
        manga_id: detail.id,
        provider: detail.provider,
        genres: detail.genres,
      })
      .catch(() => {
        // Best-effort local genre indexing for ReaderMetricsDashboard
      });
  }, [detail]);

  const communityQueryTitle = detail?.title?.trim() || selection.title.trim();

  useEffect(() => {
    if (!communityQueryTitle) {
      setCommunityLoading(false);
      return;
    }
    let active = true;
    setCommunityLoading(true);
    setCommunityError(null);

    getAniListCommunity(communityQueryTitle)
      .then((info) => {
        if (!active) return;
        setCommunity(info);
      })
      .catch((error) => {
        if (!active) return;
        setCommunityError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (active) setCommunityLoading(false);
      });

    return () => {
      active = false;
    };
  }, [communityQueryTitle]);

  const displayTitle = detail?.title || selection.title;
  const displayCover = detail?.coverUrl ?? selection.coverUrl ?? null;
  const displayDescription = detail?.description ?? selection.description ?? null;
  const resolvedSeriesId = detail?.id ?? selection.mangaId;
  const resolvedSeriesProvider = detail?.provider ?? selection.provider;
  const isUnresolvedAniList = resolvedSeriesProvider.toLowerCase() === "anilist";
  const isFavorite = favoriteKeys
    ? favoriteKeys.has(libraryKey(resolvedSeriesId, resolvedSeriesProvider))
    : (isFavoriteInLibrary ?? detail?.isFavorite ?? false);

  useEffect(() => {
    setCoverFailed(false);
  }, [displayCover]);

  const visibleChapters = useMemo(() => {
    const list = detail?.chapters ?? [];
    return offlineOnly ? list.filter((chapter) => chapter.isDownloaded) : list;
  }, [detail?.chapters, offlineOnly]);

  const handleToggleFavoriteClick = async () => {
    await onToggleFavorite({
      mangaId: detail?.id ?? selection.mangaId,
      provider: detail?.provider ?? selection.provider,
      title: displayTitle,
      coverUrl: displayCover,
    });
  };

  const handleToggleChapterClick = async (chapter: SeriesChapterItem) => {
    if (togglingChapterId) return;
    setTogglingChapterId(chapter.id);
    try {
      const result = await onToggleChapterRead({
        mangaId: detail?.id ?? selection.mangaId,
        provider: detail?.provider ?? selection.provider,
        mangaTitle: displayTitle,
        coverUrl: displayCover,
        chapterId: chapter.id,
        chapterTitle: chapter.title,
        translatedLanguage: chapter.language || detail?.activeLanguage || "es",
      });
      const nextRead = typeof result === "boolean" ? result : !chapter.isRead;
      setDetail((prev) => {
        if (!prev) return prev;
        const delta = nextRead === chapter.isRead ? 0 : nextRead ? 1 : -1;
        const updatedChapters = prev.chapters.map((item) =>
          item.id === chapter.id ? { ...item, isRead: nextRead } : item,
        );
        return {
          ...prev,
          readChapterCount: Math.max(0, prev.readChapterCount + delta),
          nextUnreadChapter: pickNextUnreadChapter(updatedChapters),
          chapters: updatedChapters,
        };
      });
      onNotify(nextRead ? s.markedReadToast : s.markedUnreadToast);
    } catch (error) {
      onNotify(error instanceof Error ? error.message : String(error));
    } finally {
      setTogglingChapterId(null);
    }
  };

  const handleOpenReaderClick = (chapter: SeriesChapterItem) => {
    const chaptersContext: ReaderChapterNeighbor[] = (detail?.chapters ?? []).map((item) => ({
      id: item.id,
      title: item.title,
      chapterNumber: item.chapterNumber,
      volumeNumber: item.volumeNumber,
      language: item.language || detail?.activeLanguage || "es",
      isDownloaded: item.isDownloaded,
    }));

    onOpenReader({
      mangaId: detail?.id ?? selection.mangaId,
      provider: detail?.provider ?? selection.provider,
      mangaTitle: displayTitle,
      coverUrl: displayCover,
      chapterId: chapter.id,
      chapterTitle: chapter.title,
      chapterNumber: chapter.chapterNumber,
      volumeNumber: chapter.volumeNumber,
      language: chapter.language || detail?.activeLanguage || "es",
      chaptersContext,
    });
  };

  const enqueueChaptersList = async (chapters: SeriesChapterItem[]) => {
    if (chapters.length === 0 || enqueuingDownloads) return;
    setEnqueuingDownloads(true);
    try {
      await onEnqueueDownload({
        mangaId: detail?.id ?? selection.mangaId,
        provider: detail?.provider ?? selection.provider,
        mangaTitle: displayTitle,
        coverUrl: displayCover,
        chapters: chapters.map((ch) => ({
          id: ch.id,
          title: ch.title,
          chapterNumber: ch.chapterNumber,
          volumeNumber: ch.volumeNumber,
          language: ch.language || detail?.activeLanguage || "es",
          scanlator: ch.scanlator,
        })),
      });
      onNotify(s.enqueuedToast(chapters.length));
      setSelectionMode(false);
      setSelectedChapterIds(new Set());
    } catch (error) {
      onNotify(error instanceof Error ? error.message : String(error));
    } finally {
      setEnqueuingDownloads(false);
    }
  };

  const tauriRuntime = isTauriRuntime();
  const downloadableVisibleChapters = useMemo(
    () =>
      tauriRuntime
        ? visibleChapters.filter(
            (ch) => ch.isAvailableOnline && !ch.isDownloaded && !activeDownloadChapterIds?.has(ch.id),
          )
        : [],
    [tauriRuntime, visibleChapters, activeDownloadChapterIds],
  );

  const selectedDownloadChapters = useMemo(
    () => downloadableVisibleChapters.filter((ch) => selectedChapterIds.has(ch.id)),
    [downloadableVisibleChapters, selectedChapterIds],
  );

  const availableLanguages = detail?.availableLanguages ?? [];
  const activeLanguage = selectedChapterLang ?? detail?.activeLanguage ?? "";

  return (
    <div className="space-y-6 pb-8">
      {/* Top Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <button
          type="button"
          onClick={onBack}
          className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-2 px-4 py-2 text-xs font-bold uppercase tracking-[0.14em]"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          <span>{s.backBtn}</span>
        </button>

        <div className="flex flex-wrap items-center gap-2">
          <span className="editorial-chip">
            {detail?.providerLabel || selection.provider}
          </span>
          <button
            type="button"
            disabled={isSaving || isUnresolvedAniList}
            aria-pressed={isFavorite}
            onClick={() => void handleToggleFavoriteClick()}
            className={`${
              isFavorite ? "editorial-btn-primary" : "editorial-btn-secondary"
            } inline-flex min-h-[44px] touch-manipulation items-center gap-2 px-4 py-2 text-xs font-bold uppercase tracking-[0.14em] disabled:opacity-50`}
          >
            <Heart
              className="h-4 w-4"
              fill={isFavorite ? "currentColor" : "none"}
              aria-hidden="true"
            />
            <span>
              {isFavorite ? t.library.removeFavorite : t.library.addFavorite}
            </span>
          </button>
        </div>
      </div>

      {/* Hero Series Card */}
      <section className="editorial-card overflow-hidden">
        <div className="grid grid-cols-1 md:grid-cols-[220px_minmax(0,1fr)]">
          <div className="screentone-bg flex items-center justify-center border-b-2 border-[var(--ink)] p-5 md:border-b-0 md:border-r-2">
            <div className="w-36 overflow-hidden border-2 border-[var(--ink)] bg-[var(--surface)] shadow-[4px_4px_0_var(--shadow-color)] md:w-full">
              {displayCover && !coverFailed ? (
                <img
                  src={resolveCoverImageUrl(displayCover, detail?.provider ?? selection.provider)}
                  alt={displayTitle}
                  referrerPolicy="no-referrer"
                  className="aspect-[3/4] w-full object-cover"
                  onError={() => setCoverFailed(true)}
                />
              ) : (
                <div className="flex aspect-[3/4] w-full items-center justify-center bg-[var(--paper)] font-display text-3xl font-bold text-[var(--ink)]">
                  {seriesMonogram(displayTitle)}
                </div>
              )}
            </div>
          </div>

          <div className="flex flex-col justify-between gap-4 p-5 sm:p-6">
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono-editorial text-[11px] font-bold uppercase tracking-[0.18em] text-[var(--red-text)]">
                  {s.eyebrow}
                </span>
                {detail ? (
                  <span className="editorial-chip">
                    {s.statusLabel}: {resolveStatusLabel(detail.status, s.statusNames)}
                  </span>
                ) : null}
              </div>

              <h1 className="font-display text-2xl leading-tight tracking-tight text-[var(--ink)] sm:text-3xl">
                {displayTitle}
              </h1>

              {(detail?.author || detail?.artist) && (
                <div className="flex flex-wrap gap-x-5 gap-y-1 font-mono-editorial text-xs text-[var(--muted)]">
                  {detail.author ? (
                    <span>
                      <strong className="uppercase tracking-[0.1em] text-[var(--ink)]">
                        {s.authorLabel}:
                      </strong>{" "}
                      {detail.author}
                    </span>
                  ) : null}
                  {detail.artist ? (
                    <span>
                      <strong className="uppercase tracking-[0.1em] text-[var(--ink)]">
                        {s.artistLabel}:
                      </strong>{" "}
                      {detail.artist}
                    </span>
                  ) : null}
                </div>
              )}

              {detail && detail.genres.length > 0 ? (
                <div className="flex flex-wrap gap-1.5">
                  {detail.genres.slice(0, 8).map((genre) =>
                    onSelectTag ? (
                      <button
                        key={genre}
                        type="button"
                        onClick={() => onSelectTag(genre)}
                        title={s.exploreTagTooltip(genre)}
                        className="cursor-pointer touch-manipulation border border-[var(--ink)] bg-[var(--surface)] px-2 py-0.5 font-mono-editorial text-[11px] font-bold text-[var(--ink)] transition-colors hover:bg-[var(--ink)] hover:text-[var(--paper)]"
                      >
                        {genre}
                      </button>
                    ) : (
                      <span
                        key={genre}
                        className="border border-[var(--ink)] bg-[var(--surface)] px-2 py-0.5 font-mono-editorial text-[11px] font-bold text-[var(--ink)]"
                      >
                        {genre}
                      </span>
                    ),
                  )}
                </div>
              ) : null}

              <p className="text-sm leading-relaxed text-[var(--muted)]">
                {displayDescription?.trim() ? displayDescription : s.noSynopsis}
              </p>
            </div>

            {/* Quick metrics + Next Unread CTA */}
            {detail ? (
              <div className="space-y-3 border-t border-[var(--line)] pt-4">
                <div className="flex flex-wrap items-center gap-2 font-mono-editorial text-xs font-semibold">
                  <span className="editorial-chip">
                    {s.chaptersCount(detail.totalChapters)}
                  </span>
                  <span className="editorial-chip">
                    {s.readCountLabel(detail.readChapterCount)}
                  </span>
                  <span className="editorial-chip">
                    {s.offlineCountLabel(detail.offlineChapterCount)}
                  </span>
                </div>

                {detail.nextUnreadChapter ? (
                  <div className="editorial-callout flex flex-col gap-3 p-3.5 sm:flex-row sm:items-center sm:justify-between">
                    <div className="min-w-0">
                      <p className="font-mono-editorial text-[11px] font-bold uppercase tracking-[0.14em] text-[var(--red-text)]">
                        {s.nextUnreadLabel}
                      </p>
                      <p className="font-display truncate text-base text-[var(--ink)]">
                        {s.chapterPrefix} {detail.nextUnreadChapter.chapterNumber}{" "}
                        {detail.nextUnreadChapter.title
                          ? `· ${detail.nextUnreadChapter.title}`
                          : ""}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleOpenReaderClick(detail.nextUnreadChapter!)}
                      className="editorial-btn-primary inline-flex min-h-[44px] shrink-0 touch-manipulation items-center justify-center gap-2 px-4 py-2 text-xs font-bold uppercase tracking-[0.12em]"
                    >
                      <BookOpen className="h-4 w-4" aria-hidden="true" />
                      <span>{s.nextUnreadAction}</span>
                    </button>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>
      </section>

      {/* Banners (Offline Fallback / Online Warning / Error) */}
      {isLoading && !detail ? (
        <div className="editorial-card-muted flex items-center gap-3 p-5 text-sm text-[var(--muted)]">
          <RefreshCw className="h-4 w-4 animate-spin text-[var(--red)]" aria-hidden="true" />
          <span>{s.loading}</span>
        </div>
      ) : null}

      {loadError ? (
        <div className="editorial-callout flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-3">
            <AlertTriangle
              className="mt-0.5 h-5 w-5 shrink-0 text-[var(--red-text)]"
              aria-hidden="true"
            />
            <div>
              <p className="font-display text-base text-[var(--ink)]">
                {s.loadErrorTitle}
              </p>
              <p className="text-xs text-[var(--muted)]">{loadError}</p>
            </div>
          </div>
          <button
            type="button"
            onClick={() => void loadSeries()}
            className="editorial-btn-secondary inline-flex min-h-[44px] shrink-0 touch-manipulation items-center gap-2 px-4 py-2 text-xs font-bold uppercase tracking-[0.12em]"
          >
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            <span>{t.errors.retry}</span>
          </button>
        </div>
      ) : null}

      {detail?.isOfflineFallback ? (
        <div className="editorial-callout flex items-start gap-3 p-4">
          <WifiOff
            className="mt-0.5 h-5 w-5 shrink-0 text-[var(--red-text)]"
            aria-hidden="true"
          />
          <div>
            <p className="font-display text-base text-[var(--ink)]">
              {s.offlineBannerTitle}
            </p>
            <p className="text-xs text-[var(--muted)]">{s.offlineBannerDesc}</p>
          </div>
        </div>
      ) : detail?.onlineWarning ? (
        <div className="editorial-box-thin flex items-start gap-3 p-3.5 text-xs text-[var(--muted)]">
          <AlertTriangle
            className="mt-0.5 h-4 w-4 shrink-0 text-[var(--red-text)]"
            aria-hidden="true"
          />
          <div>
            <span className="font-bold text-[var(--ink)]">
              {s.warningBannerTitle}:{" "}
            </span>
            <span>{detail.onlineWarning}</span>
          </div>
        </div>
      ) : null}

      {/* Chapter List */}
      {detail ? (
        <section className="editorial-card p-5 sm:p-6">
          <div className="flex flex-col gap-3 border-b-2 border-[var(--ink)] pb-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="font-display text-xl text-[var(--ink)]">
                {s.chaptersTitle}
              </h2>
              <p className="font-mono-editorial text-xs text-[var(--muted)]">
                {s.chaptersCount(detail.totalChapters)} ·{" "}
                {s.readCountLabel(detail.readChapterCount)}
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              {availableLanguages.length > 1 ? (
                <label className="inline-flex min-h-[44px] items-center gap-2 border-2 border-[var(--ink)] bg-[var(--surface)] px-3 py-1.5 text-xs font-semibold text-[var(--ink)]">
                  <Languages className="h-4 w-4 text-[var(--muted)]" aria-hidden="true" />
                  <span className="sr-only">{s.languageLabel}</span>
                  <select
                    value={activeLanguage}
                    onChange={(e) => {
                      setSelectedChapterLang(e.target.value);
                      setPage(1);
                    }}
                    className="bg-transparent text-xs font-bold text-[var(--ink)] focus:outline-none"
                  >
                    {availableLanguages.map((langCode) => (
                      <option key={langCode} value={langCode}>
                        {formatLanguageName(langCode)}
                      </option>
                    ))}
                  </select>
                </label>
              ) : null}

              <button
                type="button"
                onClick={() => {
                  setChapterOrder((prev) => (prev === "desc" ? "asc" : "desc"));
                  setPage(1);
                }}
                className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-2 px-3.5 py-2 text-xs font-bold uppercase tracking-[0.1em]"
              >
                <ArrowUpDown className="h-4 w-4" aria-hidden="true" />
                <span>{chapterOrder === "desc" ? s.sortNewest : s.sortOldest}</span>
              </button>

              {detail.offlineChapterCount > 0 ? (
                <button
                  type="button"
                  onClick={() => setOfflineOnly((prev) => !prev)}
                  className={`${
                    offlineOnly ? "editorial-btn-primary" : "editorial-btn-secondary"
                  } inline-flex min-h-[44px] touch-manipulation items-center gap-2 px-3.5 py-2 text-xs font-bold uppercase tracking-[0.1em]`}
                >
                  <HardDrive className="h-4 w-4" aria-hidden="true" />
                  <span>{offlineOnly ? s.filterOffline : s.filterAll}</span>
                </button>
              ) : null}

              {!detail.isOfflineFallback && downloadableVisibleChapters.length > 0 ? (
                <button
                  type="button"
                  onClick={() => {
                    setSelectionMode((prev) => !prev);
                    setSelectedChapterIds(new Set());
                  }}
                  className={`${
                    selectionMode ? "editorial-btn-primary" : "editorial-btn-secondary"
                  } inline-flex min-h-[44px] touch-manipulation items-center gap-2 px-3.5 py-2 text-xs font-bold uppercase tracking-[0.1em]`}
                >
                  <CheckSquare className="h-4 w-4" aria-hidden="true" />
                  <span>{selectionMode ? s.cancelSelectBtn : s.selectChaptersBtn}</span>
                </button>
              ) : null}
            </div>
          </div>

          {selectionMode ? (
            <div className="mt-3 flex flex-wrap items-center justify-between gap-2 border-2 border-[var(--ink)] bg-[var(--surface)] p-3">
              <button
                type="button"
                onClick={() => {
                  if (selectedDownloadChapters.length === downloadableVisibleChapters.length) {
                    setSelectedChapterIds(new Set());
                  } else {
                    setSelectedChapterIds(new Set(downloadableVisibleChapters.map((c) => c.id)));
                  }
                }}
                className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs font-bold"
              >
                <CheckSquare className="h-4 w-4" aria-hidden="true" />
                <span>{s.selectAllPageBtn}</span>
              </button>

              <button
                type="button"
                disabled={selectedDownloadChapters.length === 0 || enqueuingDownloads}
                onClick={() => void enqueueChaptersList(selectedDownloadChapters)}
                className="editorial-btn-primary min-h-[44px] px-4 py-1.5 text-xs font-bold disabled:opacity-40"
              >
                <Download className="h-4 w-4" aria-hidden="true" />
                <span>{s.downloadSelectedBtn(selectedDownloadChapters.length)}</span>
              </button>
            </div>
          ) : null}

          {isLoading ? (
            <div className="flex items-center gap-2 py-6 text-xs text-[var(--muted)]">
              <RefreshCw
                className="h-4 w-4 animate-spin text-[var(--red)]"
                aria-hidden="true"
              />
              <span>{s.loading}</span>
            </div>
          ) : null}

          {!isLoading && visibleChapters.length === 0 ? (
            <div className="py-8 text-center text-sm text-[var(--muted)]">
              {offlineOnly ? s.emptyOfflineChapters : s.emptyChapters}
            </div>
          ) : (
            <ul className="divide-y divide-[var(--line)]">
              {visibleChapters.map((chapter) => {
                const isBusy = togglingChapterId === chapter.id;
                const isQueued = activeDownloadChapterIds?.has(chapter.id) ?? false;
                const canDownload =
                  tauriRuntime && chapter.isAvailableOnline && !chapter.isDownloaded && !detail.id.startsWith("offline::");
                const isChecked = selectedChapterIds.has(chapter.id);
                const headingText = chapter.volumeNumber
                  ? `Vol. ${chapter.volumeNumber} · ${s.chapterPrefix} ${chapter.chapterNumber}`
                  : `${s.chapterPrefix} ${chapter.chapterNumber}`;

                return (
                  <li
                    key={chapter.id}
                    className={`flex flex-col gap-3 py-3.5 sm:flex-row sm:items-center sm:justify-between ${
                      chapter.isRead ? "opacity-75" : ""
                    }`}
                  >
                    <div className="flex min-w-0 flex-1 items-start gap-3">
                      {selectionMode && canDownload && !isQueued ? (
                        <button
                          type="button"
                          aria-pressed={isChecked}
                          onClick={() =>
                            setSelectedChapterIds((prev) => {
                              const next = new Set(prev);
                              if (next.has(chapter.id)) next.delete(chapter.id);
                              else next.add(chapter.id);
                              return next;
                            })
                          }
                          className="mt-0.5 inline-flex min-h-[44px] min-w-[44px] shrink-0 cursor-pointer items-center justify-center border-2 border-[var(--ink)] bg-[var(--paper)] text-[var(--ink)]"
                        >
                          {isChecked ? (
                            <CheckSquare className="h-4 w-4 text-[var(--red-text)]" aria-hidden="true" />
                          ) : (
                            <Square className="h-4 w-4" aria-hidden="true" />
                          )}
                        </button>
                      ) : null}

                      <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-display text-base text-[var(--ink)]">
                            {headingText}
                          </span>
                          {chapter.title ? (
                            <span className="truncate text-sm text-[var(--ink)]">
                              — {chapter.title}
                            </span>
                          ) : null}
                        </div>

                        <div className="flex flex-wrap items-center gap-2 font-mono-editorial text-[11px]">
                          <span
                            className={`inline-flex items-center gap-1 border px-2 py-0.5 font-bold uppercase tracking-[0.08em] ${
                              chapter.isRead
                                ? "border-[var(--ink)] bg-[var(--surface)] text-[var(--ink)]"
                                : "border-[var(--red)] bg-[var(--red)] text-white"
                            }`}
                          >
                            {chapter.isRead ? (
                              <Check className="h-3 w-3" aria-hidden="true" />
                            ) : null}
                            {chapter.isRead ? s.statusRead : s.statusUnread}
                          </span>

                          {chapter.isDownloaded ? (
                            <span className="inline-flex items-center gap-1 border border-[var(--ink)] bg-[var(--ink)] px-2 py-0.5 font-bold uppercase tracking-[0.06em] text-[var(--paper)]">
                              <HardDrive className="h-3 w-3" aria-hidden="true" />
                              {s.statusOffline}
                              {chapter.offlineFormat ? ` · ${chapter.offlineFormat}` : ""}
                            </span>
                          ) : chapter.isAvailableOnline ? (
                            <span className="border border-[var(--line)] px-2 py-0.5 font-medium text-[var(--muted)]">
                              {s.statusOnline}
                            </span>
                          ) : null}

                          {chapter.isBookmarked && chapter.bookmarkedPage !== null ? (
                            <span className="border border-[var(--red)] px-2 py-0.5 font-bold text-[var(--red-text)]">
                              p.{chapter.bookmarkedPage + 1}
                            </span>
                          ) : null}

                          {chapter.scanlator ? (
                            <span className="truncate text-[var(--muted)]">
                              {chapter.scanlator}
                            </span>
                          ) : null}

                          {chapter.publicationDate ? (
                            <span className="text-[var(--muted)]">
                              {chapter.publicationDate}
                            </span>
                          ) : null}
                        </div>
                      </div>
                    </div>

                    <div className="flex flex-wrap items-center gap-2 sm:shrink-0">
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() => void handleToggleChapterClick(chapter)}
                        className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-1.5 px-3 py-2 text-xs font-bold uppercase tracking-[0.08em] disabled:opacity-50"
                      >
                        <Check className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>
                          {chapter.isRead ? s.markUnreadBtn : s.markReadBtn}
                        </span>
                      </button>

                      {canDownload ? (
                        <button
                          type="button"
                          disabled={isQueued || enqueuingDownloads}
                          onClick={() => void enqueueChaptersList([chapter])}
                          className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-1.5 px-3 py-2 text-xs font-bold uppercase tracking-[0.08em] disabled:opacity-50"
                        >
                          <Download className="h-3.5 w-3.5" aria-hidden="true" />
                          <span>{isQueued ? s.downloadingBtn : s.downloadBtn}</span>
                        </button>
                      ) : null}

                      <button
                        type="button"
                        onClick={() => handleOpenReaderClick(chapter)}
                        className="editorial-btn-primary inline-flex min-h-[44px] touch-manipulation items-center gap-1.5 px-3.5 py-2 text-xs font-bold uppercase tracking-[0.08em]"
                      >
                        <BookOpen className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{s.openReaderBtn}</span>
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}

          {(page > 1 || detail.hasNextPage) && (
            <div className="mt-4 flex items-center justify-between border-t-2 border-[var(--ink)] pt-4">
              <button
                type="button"
                disabled={page <= 1 || isLoading}
                onClick={() => setPage((prev) => Math.max(1, prev - 1))}
                className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-1.5 px-3.5 py-2 text-xs font-bold uppercase tracking-[0.1em] disabled:opacity-40"
              >
                <ChevronLeft className="h-4 w-4" aria-hidden="true" />
                <span>{t.explore.previous}</span>
              </button>

              <span className="font-mono-editorial text-xs font-bold text-[var(--ink)]">
                #{page}
              </span>

              <button
                type="button"
                disabled={!detail.hasNextPage || isLoading}
                onClick={() => setPage((prev) => prev + 1)}
                className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-1.5 px-3.5 py-2 text-xs font-bold uppercase tracking-[0.1em] disabled:opacity-40"
              >
                <span>{t.explore.next}</span>
                <ChevronRight className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>
          )}
        </section>
      ) : null}

      {/* AniList Community */}
      <section className="editorial-card p-5 sm:p-6">
        <div className="flex flex-col gap-3 border-b-2 border-[var(--ink)] pb-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <p className="font-mono-editorial text-[11px] font-bold uppercase tracking-[0.16em] text-[var(--red-text)]">
              {s.communityEyebrow}
            </p>
            <h2 className="font-display text-xl text-[var(--ink)]">
              {s.communityTitle}
            </h2>
          </div>

          {community?.siteUrl ? (
            <button
              type="button"
              onClick={() => void onOpenExternal(community.siteUrl!)}
              className="editorial-btn-secondary inline-flex min-h-[44px] touch-manipulation items-center gap-2 self-start px-3.5 py-2 text-xs font-bold uppercase tracking-[0.1em]"
            >
              <ExternalLink className="h-4 w-4" aria-hidden="true" />
              <span>{s.communityOpenAniList}</span>
            </button>
          ) : null}
        </div>

        <div className="mt-4 space-y-5">
          {communityLoading ? (
            <div className="flex items-center gap-2 text-sm text-[var(--muted)]">
              <RefreshCw
                className="h-4 w-4 animate-spin text-[var(--red)]"
                aria-hidden="true"
              />
              <span>{s.communityLoading}</span>
            </div>
          ) : communityError ? (
            <p className="text-sm text-[var(--muted)]">
              {s.communityUnavailable} ({communityError})
            </p>
          ) : !community || !community.found ? (
            <p className="text-sm text-[var(--muted)]">
              {community?.note || s.communityNotFound}
            </p>
          ) : (
            <div className="space-y-4">
              {community.note ? (
                <p className="text-xs text-[var(--muted)]">{community.note}</p>
              ) : null}

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <div className="editorial-box-thin p-3.5">
                  <p className="font-mono-editorial text-[11px] font-bold uppercase tracking-[0.12em] text-[var(--muted)]">
                    {s.communityScoreLabel}
                  </p>
                  <p className="font-display mt-1 flex items-center gap-1.5 text-2xl text-[var(--ink)]">
                    <Star className="h-4 w-4 text-[var(--red-text)]" aria-hidden="true" />
                    <span>
                      {community.averageScore ?? community.meanScore ?? "—"}
                    </span>
                    <span className="font-mono-editorial text-xs font-normal text-[var(--muted)]">
                      / 100
                    </span>
                  </p>
                </div>

                <div className="editorial-box-thin p-3.5">
                  <p className="font-mono-editorial text-[11px] font-bold uppercase tracking-[0.12em] text-[var(--muted)]">
                    {s.communityPopularityLabel}
                  </p>
                  <p className="font-display mt-1 flex items-center gap-1.5 text-2xl text-[var(--ink)]">
                    <Sparkles className="h-4 w-4 text-[var(--red-text)]" aria-hidden="true" />
                    <span>
                      {community.popularity !== null
                        ? community.popularity.toLocaleString()
                        : "—"}
                    </span>
                  </p>
                </div>

                <div className="editorial-box-thin p-3.5">
                  <p className="font-mono-editorial text-[11px] font-bold uppercase tracking-[0.12em] text-[var(--muted)]">
                    {s.communityFavoritesLabel}
                  </p>
                  <p className="font-display mt-1 flex items-center gap-1.5 text-2xl text-[var(--ink)]">
                    <Heart className="h-4 w-4 text-[var(--red-text)]" aria-hidden="true" />
                    <span>
                      {community.favourites !== null
                        ? community.favourites.toLocaleString()
                        : "—"}
                    </span>
                  </p>
                </div>
              </div>

              {community.tags.length > 0 ? (
                <div className="space-y-2">
                  <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <p className="font-mono-editorial text-xs font-bold uppercase tracking-[0.12em] text-[var(--muted)]">
                      {s.communityTagsLabel}
                    </p>
                    {onSelectTag ? (
                      <span className="font-mono-editorial text-[11px] text-[var(--muted)]">
                        {s.communityTagsHint}
                      </span>
                    ) : null}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {community.tags.slice(0, 12).map((tag) =>
                      onSelectTag ? (
                        <button
                          key={tag.name}
                          type="button"
                          onClick={() => onSelectTag(tag.name)}
                          title={s.exploreTagTooltip(tag.name)}
                          className="editorial-chip text-xs hover:bg-[var(--ink)] hover:text-[var(--paper)] active:translate-x-[1px] active:translate-y-[1px]"
                        >
                          <Tag className="h-3 w-3 shrink-0 opacity-75" aria-hidden="true" />
                          <span>
                            {tag.name}
                            {tag.rank !== null ? ` · ${tag.rank}%` : ""}
                          </span>
                        </button>
                      ) : (
                        <span key={tag.name} className="editorial-chip text-xs">
                          {tag.name}
                          {tag.rank !== null ? ` · ${tag.rank}%` : ""}
                        </span>
                      ),
                    )}
                  </div>
                </div>
              ) : null}

              <div className="space-y-2.5">
                <h3 className="font-mono-editorial text-xs font-bold uppercase tracking-[0.14em] text-[var(--ink)]">
                  {s.communityReviewsTitle}
                </h3>
                {community.reviews.length === 0 ? (
                  <p className="text-xs text-[var(--muted)]">
                    {s.communityNoReviews}
                  </p>
                ) : (
                  <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                    {community.reviews.slice(0, 5).map((review) => (
                      <article
                        key={review.id}
                        className="editorial-box-thin flex flex-col justify-between gap-2.5 p-3.5"
                      >
                        <div className="space-y-1.5">
                          <div className="flex items-center justify-between gap-2 text-xs">
                            <span className="font-bold text-[var(--ink)]">
                              @{review.userName}
                            </span>
                            {review.score !== null ? (
                              <span className="editorial-chip">
                                {review.score}/100
                              </span>
                            ) : null}
                          </div>
                          <p className="text-xs leading-relaxed text-[var(--muted)]">
                            {review.summary}
                          </p>
                        </div>

                        {review.siteUrl ? (
                          <button
                            type="button"
                            onClick={() => void onOpenExternal(review.siteUrl!)}
                            className="inline-flex min-h-[36px] touch-manipulation items-center gap-1 self-start font-mono-editorial text-[11px] font-bold uppercase tracking-[0.1em] text-[var(--red-text)] hover:underline"
                          >
                            <ExternalLink className="h-3 w-3" aria-hidden="true" />
                            <span>{s.communityOpenAniList}</span>
                          </button>
                        ) : null}
                      </article>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
