import { useEffect, useRef, useState } from "react";
import { ArrowDown, ArrowRight, Check, RefreshCw, SlidersHorizontal } from "lucide-react";
import { CatalogSearch } from "../components/CatalogSearch";
import { ChapterUpdates } from "../components/ChapterUpdates";
import type {
  ForYouResponse,
  InitialAppState,
  OpenReaderChapterRequest,
  SeriesSelection,
  ToggleFavoritePayload,
} from "../types/mangora";
import { getGenreLabel, getTranslations } from "../lib/i18n";
import { getForYouRecommendations, isTauriRuntime, openExternalUrl } from "../lib/tauri";

interface ExploreViewProps {
  appState: InitialAppState;
  isSaving: boolean;
  focusSearch: boolean;
  favoriteKeys: Set<string>;
  onToggleFavorite: (payload: ToggleFavoritePayload) => Promise<void>;
  onOpenSeries: (series: SeriesSelection) => void;
  onOpenReader: (request: OpenReaderChapterRequest) => void;
  onSelectProvider: (providerId: string) => Promise<void>;
  onEditGenres: () => void;
}

export function ExploreView({
  appState,
  isSaving,
  focusSearch,
  favoriteKeys,
  onToggleFavorite,
  onOpenSeries,
  onOpenReader,
  onSelectProvider,
  onEditGenres,
}: ExploreViewProps) {
  const t = getTranslations(appState.uiLanguage);
  const sourcesHeading = useRef<HTMLHeadingElement>(null);
  const [forYou, setForYou] = useState<ForYouResponse | null>(null);
  const [forYouLoading, setForYouLoading] = useState(true);
  const [forYouError, setForYouError] = useState(false);
  const [forYouReloadNonce, setForYouReloadNonce] = useState(0);

  const providers = [...appState.availableProviders].sort((a, b) =>
    Number(b.id === appState.recommendedProvider) - Number(a.id === appState.recommendedProvider));
  const active = providers.find(provider => provider.id === appState.activeProvider);
  const genresSignature = appState.favoriteGenres.join(",");

  useEffect(() => {
    let activeRequest = true;
    setForYouLoading(true);
    setForYouError(false);

    getForYouRecommendations(appState.activeProvider)
      .then((response) => {
        if (!activeRequest) return;
        setForYou(response);
      })
      .catch(() => {
        if (!activeRequest) return;
        setForYouError(true);
      })
      .finally(() => {
        if (activeRequest) setForYouLoading(false);
      });

    return () => {
      activeRequest = false;
    };
  }, [appState.activeProvider, genresSignature, forYouReloadNonce]);

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="eyebrow mb-3">{t.explore.eyebrow}</p>
          <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.explore.title}</h1>
          <p className="max-w-2xl text-base text-[var(--muted)]">{t.explore.subtitle}</p>
        </div>
        <button
          type="button"
          onClick={() => {
            document.getElementById("updates-title")?.scrollIntoView({ behavior: "smooth", block: "start" });
          }}
          className="editorial-btn-secondary min-h-[44px] px-3.5 py-2 text-xs"
        >
          <ArrowDown className="h-3.5 w-3.5" aria-hidden="true" />
          <span>{appState.uiLanguage === "en" ? "Latest chapters" : "Novedades"}</span>
        </button>
      </div>

      {active && (
        <CatalogSearch
          provider={active}
          language={appState.uiLanguage}
          focusSearch={focusSearch}
          isMutating={isSaving}
          favoriteKeys={favoriteKeys}
          onToggleFavorite={onToggleFavorite}
          onOpenSeries={onOpenSeries}
          onChooseSource={providers.length > 1 ? () => {
            sourcesHeading.current?.scrollIntoView({ block: "start" });
            sourcesHeading.current?.focus({ preventScroll: true });
          } : undefined}
        />
      )}

      {/* For you genres & recommendations */}
      <section className="editorial-callout space-y-4 p-5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 className="font-display text-2xl">{t.explore.forYouTitle}</h2>
            {forYou && !forYouLoading && !forYouError ? (
              <p className="font-mono-editorial mt-1 text-xs text-[var(--muted)]">
                {forYou.matchedGenres.length > 0
                  ? t.explore.forYouMatched(
                      forYou.matchedGenres
                        .map((genre) => getGenreLabel(genre, appState.uiLanguage))
                        .join(", "),
                    )
                  : forYou.fallbackPopular
                    ? t.explore.forYouPopularFallback
                    : ""}
              </p>
            ) : null}
          </div>
          <button
            type="button"
            onClick={onEditGenres}
            className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
          >
            <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{t.explore.editGenresBtn}</span>
          </button>
        </div>

        {appState.favoriteGenres.length > 0 ? (
          <div className="flex flex-wrap gap-2">
            {appState.favoriteGenres.map((genreId) => (
              <span
                key={genreId}
                className="border-2 border-[var(--ink)] bg-[var(--paper)] px-3 py-1 font-mono-editorial text-xs font-bold shadow-[2px_2px_0_var(--shadow-color)]"
              >
                {getGenreLabel(genreId, appState.uiLanguage)}
              </span>
            ))}
          </div>
        ) : (
          <p className="font-mono-editorial text-xs text-[var(--muted)]">{t.explore.noGenresHint}</p>
        )}

        {forYouLoading ? (
          <div className="flex items-center gap-2 text-xs text-[var(--muted)]">
            <RefreshCw className="h-4 w-4 animate-spin text-[var(--red)]" aria-hidden="true" />
            <span>{t.explore.forYouLoading}</span>
          </div>
        ) : forYouError ? (
          <div className="flex flex-wrap items-center justify-between gap-3 border-2 border-[var(--ink)] bg-[var(--paper)] p-3.5 text-xs">
            <span className="text-[var(--muted)]">{t.explore.forYouUnavailable}</span>
            <button
              type="button"
              onClick={() => setForYouReloadNonce((prev) => prev + 1)}
              className="editorial-btn-secondary inline-flex min-h-[44px] items-center gap-1.5 px-3 py-1.5 text-xs"
            >
              <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
              <span>{t.errors.retry}</span>
            </button>
          </div>
        ) : forYou && forYou.items.length > 0 ? (
          <ul className="grid grid-cols-2 gap-3 md:grid-cols-4">
            {forYou.items.slice(0, 8).map((item) => (
              <li key={item.id} className="editorial-card overflow-hidden">
                <button
                  type="button"
                  onClick={() =>
                    onOpenSeries({
                      mangaId: item.id,
                      provider: forYou.provider,
                      title: item.title,
                      coverUrl: item.coverUrl || null,
                      description: item.description,
                    })
                  }
                  className="group flex h-full w-full cursor-pointer touch-manipulation flex-col text-left hover:bg-[var(--surface)] focus-visible:outline-offset-[-4px]"
                >
                  {/^https?:\/\//i.test(item.coverUrl) ? (
                    <img
                      src={item.coverUrl}
                      alt=""
                      loading="lazy"
                      className="aspect-[2/3] w-full border-b border-[var(--ink)] object-cover"
                      onError={(event) => {
                        event.currentTarget.hidden = true;
                      }}
                    />
                  ) : (
                    <div className="screentone-bg flex aspect-[2/3] w-full items-center justify-center border-b border-[var(--ink)] p-3">
                      <span className="border-2 border-[var(--ink)] bg-[var(--paper)] px-2 py-1 font-display text-lg text-[var(--ink)]">
                        {item.title.slice(0, 2).toUpperCase()}
                      </span>
                    </div>
                  )}
                  <span className="font-display line-clamp-2 block p-2.5 text-sm leading-snug group-hover:text-[var(--red-text)]">
                    {item.title}
                  </span>
                  <span className="mt-auto flex items-center gap-1.5 px-2.5 pb-2.5 text-xs font-bold text-[var(--red-text)]">
                    <span>{t.explore.viewDetails}</span>
                    <ArrowRight size={13} aria-hidden="true" />
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-xs text-[var(--muted)]">{t.explore.forYouEmpty}</p>
        )}
      </section>

      {/* Provider selector */}
      <section className="space-y-4">
        <div>
          <h2 ref={sourcesHeading} tabIndex={-1} className="font-display scroll-mt-6 text-2xl">
            {t.explore.providerSelectorTitle}
          </h2>
          <p className="text-sm text-[var(--muted)]">{t.explore.providerSelectorDesc}</p>
        </div>

        <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
          {providers.map((prov) => {
            const isActive = appState.activeProvider === prov.id;
            const isRecommended = appState.recommendedProvider === prov.id;
            return (
              <button
                key={prov.id}
                type="button"
                disabled={isSaving}
                aria-pressed={isActive}
                onClick={() => void onSelectProvider(prov.id)}
                className={`cursor-pointer border-2 border-[var(--ink)] p-4 text-left transition-colors ${
                  isActive
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[4px_4px_0_var(--red)]"
                    : "bg-[var(--paper)] text-[var(--ink)] shadow-[3px_3px_0_var(--shadow-color)] hover:bg-[var(--surface)]"
                }`}
              >
                <div className="mb-2 flex items-center justify-between gap-2">
                  <span className="font-display text-lg">{prov.badge}</span>
                  <div className="flex items-center gap-1.5">
                    {isRecommended && (
                      <span className="border border-[var(--ink)] bg-[var(--red)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold text-white">
                        {t.explore.recommendedTag}
                      </span>
                    )}
                    {isActive && (
                      <span className="inline-flex items-center gap-1 border border-current px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold">
                        <Check className="h-3 w-3" /> {t.explore.activeTag}
                      </span>
                    )}
                  </div>
                </div>
                <p
                  className={`text-xs ${
                    isActive ? "text-[var(--paper)]" : "text-[var(--muted)]"
                  }`}
                >
                  {appState.uiLanguage === "en" ? prov.descriptionEn : prov.descriptionEs}
                </p>
              </button>
            );
          })}
        </div>
      </section>

      {!isTauriRuntime() && (
        <section className="editorial-callout flex flex-col gap-3 p-5 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0 space-y-1">
            <h2 className="font-display text-xl">{t.explore.webSourcesTitle}</h2>
            <p className="text-xs text-[var(--muted)]">{t.explore.webSourcesDesc}</p>
          </div>
          <button
            type="button"
            onClick={() => void openExternalUrl("https://tomahawk1103.github.io/manga-tui/#descargas")}
            className="editorial-btn-primary shrink-0 text-xs"
          >
            {t.explore.webDownloadAppBtn}
          </button>
        </section>
      )}

      <ChapterUpdates
        language={appState.uiLanguage}
        onSeries={(item) => {
          const provider = item.provider ?? "mangaoni";
          onOpenSeries({ mangaId: item.id, provider, title: item.title, coverUrl: item.cover });
        }}
        onChapter={(item) => {
          const provider = item.provider ?? "mangaoni";
          const chapterId =
            provider === "mangaoni" && !item.chapterId.includes("|")
              ? `${item.id}|${item.chapterId}`
              : item.chapterId;
          onOpenReader({
            mangaId: item.id,
            provider,
            mangaTitle: item.title,
            coverUrl: item.cover,
            chapterId,
            chapterTitle: item.chapterTitle,
            chapterNumber: item.chapterTitle.match(/\d+(?:\.\d+)?/)?.[0],
            language: appState.uiLanguage,
          });
        }}
      />
    </div>
  );
}
