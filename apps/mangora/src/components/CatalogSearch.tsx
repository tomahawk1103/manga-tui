import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, Heart, Tag, X } from "lucide-react";
import {
  CollectionDisplayToolbar,
  type ExploreSortBy,
  sortExploreItems,
  useMangoraAppearance,
} from "../features/hito5";
import { CATEGORY_TAG_OPTIONS, getTranslations } from "../lib/i18n";
import { searchCatalog, type CatalogPage } from "../lib/tauri";
import {
  type ProviderOption,
  type SeriesSelection,
  type ToggleFavoritePayload,
  type UiLanguage,
  libraryKey,
} from "../types/mangora";

interface SearchRequest {
  provider: string;
  query: string;
  tags: string[];
  page: number;
}

interface CatalogSearchProps {
  provider: ProviderOption;
  language: UiLanguage;
  focusSearch: boolean;
  isMutating?: boolean;
  favoriteKeys?: Set<string>;
  onToggleFavorite?: (payload: ToggleFavoritePayload) => Promise<void>;
  onOpenSeries: (series: SeriesSelection) => void;
  onChooseSource?: () => void;
}

export function CatalogSearch({
  provider,
  language,
  focusSearch,
  isMutating = false,
  favoriteKeys,
  onToggleFavorite,
  onOpenSeries,
  onChooseSource,
}: CatalogSearchProps) {
  const translations = getTranslations(language);
  const t = translations.explore;
  const { config: appearance, updateAppearance } = useMangoraAppearance();
  const searchInput = useRef<HTMLInputElement>(null);
  const [input, setInput] = useState("");
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [tagFilter, setTagFilter] = useState("");
  const [showAllTags, setShowAllTags] = useState(false);
  const [request, setRequest] = useState<SearchRequest | null>(null);
  const [result, setResult] = useState<CatalogPage | null>(null);
  const [failed, setFailed] = useState(false);
  const [genre, setGenre] = useState("");
  const [lang, setLang] = useState("");
  const page = request?.provider === provider.id ? request.page : 1;

  useEffect(() => {
    if (focusSearch) searchInput.current?.focus();
  }, [focusSearch]);

  useEffect(() => {
    if (!request) return;
    let active = true;
    setResult(null);
    setFailed(false);
    setGenre("");
    setLang("");
    searchCatalog(provider.id, request.query, page, request.tags, language).then(
      (response) => { if (active) setResult(response); },
      () => { if (active) setFailed(true); },
    );
    return () => { active = false; };
  }, [provider.id, request, page, language]);

  const filteredCategoryTags = useMemo(() => {
    const needle = tagFilter.trim().toLowerCase();
    if (needle) {
      return CATEGORY_TAG_OPTIONS.filter(
        (tag) =>
          tag.id.includes(needle) ||
          tag.labelEs.toLowerCase().includes(needle) ||
          tag.labelEn.toLowerCase().includes(needle) ||
          tag.aliases.some((alias) => alias.includes(needle)),
      );
    }
    if (showAllTags) return CATEGORY_TAG_OPTIONS;
    return CATEGORY_TAG_OPTIONS.filter((tag) => tag.primary || selectedTags.includes(tag.id));
  }, [tagFilter, showAllTags, selectedTags]);

  const primaryTagCount = useMemo(
    () => CATEGORY_TAG_OPTIONS.filter((tag) => !tag.primary).length,
    [],
  );

  const toggleTag = (tagId: string) => {
    const nextTags = selectedTags.includes(tagId)
      ? selectedTags.filter((id) => id !== tagId)
      : [...selectedTags, tagId];
    setSelectedTags(nextTags);
    const trimmed = input.trim();
    if (trimmed || nextTags.length > 0) {
      setRequest({ provider: provider.id, query: trimmed, tags: nextTags, page: 1 });
    } else {
      setRequest(null);
      setResult(null);
      setFailed(false);
    }
  };

  const clearTags = () => {
    setSelectedTags([]);
    const trimmed = input.trim();
    if (trimmed) {
      setRequest({ provider: provider.id, query: trimmed, tags: [], page: 1 });
    } else {
      setRequest(null);
      setResult(null);
      setFailed(false);
    }
  };

  const items = result?.items ?? [];
  const usingTagFallbackSource =
    Boolean(request?.tags.length) &&
    provider.id !== "mangadex" &&
    items.some((item) => item.provider === "mangadex");
  const genres = [...new Set(items.flatMap(item => item.genres))].sort();
  const languages = [...new Set(items.flatMap(item => item.languages))].sort();
  const visible = useMemo(() => {
    const filtered = items.filter(
      (item) => (!genre || item.genres.includes(genre)) && (!lang || item.languages.includes(lang)),
    );
    return sortExploreItems(filtered, appearance.explore_sort);
  }, [items, genre, lang, appearance.explore_sort]);
  const canSubmit = Boolean(input.trim() || selectedTags.length > 0);
  return (
    <section className="space-y-4" aria-label={t.search}>
      <form className="flex flex-wrap items-end gap-3" onSubmit={event => {
        event.preventDefault();
        if (canSubmit) {
          setRequest({ provider: provider.id, query: input.trim(), tags: selectedTags, page: 1 });
        }
      }}>
        <label className="min-w-48 flex-1 font-bold">{t.searchTitle}
          <input ref={searchInput} type="search" value={input} onChange={event => setInput(event.target.value)}
            className="mt-1 block min-h-[44px] w-full border-2 border-[var(--ink)] bg-[var(--paper)] px-3 py-2" />
        </label>
        <button type="submit" className="editorial-btn-primary w-full sm:w-auto" disabled={!canSubmit}>{t.search}</button>
      </form>

      <div className="editorial-box-thin space-y-3 bg-[var(--surface)] p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-[var(--ink)]">
            <Tag className="h-3.5 w-3.5 text-[var(--red-text)]" aria-hidden="true" />
            <span>{t.tagsTitle}</span>
            {selectedTags.length > 0 && (
              <span className="border border-[var(--ink)] bg-[var(--red)] px-1.5 py-0.5 font-mono-editorial text-[10px] text-white">
                {t.activeTagsSummary(selectedTags.length)}
              </span>
            )}
          </div>
          {selectedTags.length > 0 && (
            <button
              type="button"
              onClick={clearTags}
              className="inline-flex min-h-[36px] cursor-pointer items-center gap-1 border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1 text-xs font-bold text-[var(--ink)] hover:bg-[var(--ink)] hover:text-[var(--paper)]"
            >
              <X className="h-3 w-3" aria-hidden="true" />
              <span>{t.clearTags}</span>
            </button>
          )}
        </div>

        <input
          type="search"
          value={tagFilter}
          onChange={(event) => setTagFilter(event.target.value)}
          placeholder={t.tagsFilterPlaceholder}
          aria-label={t.tagsTitle}
          className="block min-h-[40px] w-full border-2 border-[var(--ink)] bg-[var(--paper)] px-3 py-1.5 text-sm"
        />

        <div className="flex flex-wrap gap-2">
          {filteredCategoryTags.map((tag) => {
            const active = selectedTags.includes(tag.id);
            return (
              <button
                key={tag.id}
                type="button"
                aria-pressed={active}
                data-selected={active ? "true" : "false"}
                onClick={() => toggleTag(tag.id)}
                className="editorial-chip text-xs"
              >
                {language === "en" ? tag.labelEn : tag.labelEs}
              </button>
            );
          })}
          {!tagFilter.trim() && primaryTagCount > 0 && (
            <button
              type="button"
              onClick={() => setShowAllTags((prev) => !prev)}
              className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
            >
              {showAllTags ? t.showLessTags : t.showMoreTags(primaryTagCount)}
            </button>
          )}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-sm">
        <p className="text-[var(--muted)]">
          {t.providerLabel}: <strong className="text-[var(--ink)]">{provider.name}</strong>
        </p>
        {onChooseSource && (
          <button type="button" className="editorial-btn-secondary px-3 py-1.5 text-xs" onClick={onChooseSource}>
            {failed || (result && items.length === 0) ? t.chooseAnotherSource : t.changeSource}
          </button>
        )}
      </div>
      {usingTagFallbackSource && (
        <p className="font-mono-editorial text-xs text-[var(--muted)]">{t.tagsSourceNote}</p>
      )}
      {request && !result && <p role="status">{failed ? t.unavailable : t.loading}</p>}
      {request && failed && (
        <button
          type="button"
          className="editorial-btn-secondary"
          onClick={() => setRequest({ ...request, provider: provider.id, page })}
        >
          {translations.errors.retry}
        </button>
      )}
      {result && request && <>
        <div className="flex flex-wrap gap-4">
          {languages.length > 0 && <label className="flex items-center gap-2">{t.language} <select value={lang} onChange={event => setLang(event.target.value)} className="min-h-[44px] border-2 border-[var(--ink)] bg-[var(--paper)] px-2.5 py-2">
            <option value="">{t.all}</option>
            {languages.map(value => <option key={value}>{value}</option>)}
          </select></label>}
          {genres.length > 0 && <label className="flex items-center gap-2">{t.genre} <select value={genre} onChange={event => setGenre(event.target.value)} className="min-h-[44px] border-2 border-[var(--ink)] bg-[var(--paper)] px-2.5 py-2">
            <option value="">{t.all}</option>
            {genres.map(value => <option key={value}>{value}</option>)}
          </select></label>}
        </div>
        <CollectionDisplayToolbar
          mode="explore"
          language={language}
          density={appearance.explore_density}
          onDensityChange={(explore_density) => void updateAppearance({ explore_density })}
          sortBy={appearance.explore_sort}
          onSortChange={(sort) => void updateAppearance({ explore_sort: sort as ExploreSortBy })}
          itemCount={visible.length}
        />
        {!visible.length && <p role="status">{t.empty}</p>}
        <ul className="mangora-explore-grid">
          {visible.map((item) => {
            const resolvedProvider = item.provider || provider.id;
            const isFavorite = favoriteKeys?.has(libraryKey(item.id, resolvedProvider)) ?? false;
            const seriesPayload: SeriesSelection = {
              mangaId: item.id,
              provider: resolvedProvider,
              title: item.title,
              coverUrl: item.coverUrl || null,
              description: item.description,
            };

            return (
              <li
                key={`${resolvedProvider}:${item.id}`}
                className="editorial-card flex flex-col justify-between overflow-hidden"
              >
                <button
                  type="button"
                  className="group flex flex-1 cursor-pointer touch-manipulation flex-col text-left hover:bg-[var(--surface)] focus-visible:outline-offset-[-4px]"
                  onClick={() => onOpenSeries(seriesPayload)}
                >
                  {/^https?:\/\//i.test(item.coverUrl) ? (
                    <img
                      src={item.coverUrl}
                      alt=""
                      loading="lazy"
                      referrerPolicy="no-referrer"
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
                  <span className="font-display line-clamp-2 block p-3 text-sm leading-snug group-hover:text-[var(--red-text)] sm:text-base">
                    {item.title}
                  </span>
                </button>

                <div className="flex items-center justify-between gap-2 border-t border-[var(--line)] bg-[var(--surface)] px-3 py-2">
                  <button
                    type="button"
                    onClick={() => onOpenSeries(seriesPayload)}
                    className="inline-flex min-h-[44px] cursor-pointer touch-manipulation items-center gap-1.5 text-xs font-bold text-[var(--red-text)] hover:underline"
                  >
                    <span>{t.viewDetails}</span>
                    <ArrowRight size={14} aria-hidden="true" />
                  </button>

                  {onToggleFavorite && (
                    <button
                      type="button"
                      disabled={isMutating}
                      aria-pressed={isFavorite}
                      aria-label={
                        isFavorite
                          ? translations.library.removeFavorite
                          : translations.library.addFavorite
                      }
                      onClick={() =>
                        void onToggleFavorite({
                          mangaId: item.id,
                          provider: resolvedProvider,
                          title: item.title,
                          coverUrl: item.coverUrl || null,
                        })
                      }
                      className={`inline-flex min-h-[44px] min-w-[44px] cursor-pointer touch-manipulation items-center justify-center border-2 border-[var(--ink)] transition-colors ${
                        isFavorite
                          ? "bg-[var(--red)] text-white"
                          : "bg-[var(--paper)] text-[var(--ink)] hover:bg-[var(--ink)] hover:text-[var(--paper)]"
                      }`}
                    >
                      <Heart
                        className={`h-4 w-4 ${isFavorite ? "fill-current" : ""}`}
                        aria-hidden="true"
                      />
                    </button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      </>}
      {request && (result || failed) && (page > 1 || result?.hasNext) && <div className="flex gap-3">
        <button className="editorial-btn-secondary" disabled={page === 1}
          onClick={() => setRequest({ ...request, provider: provider.id, page: page - 1 })}>{t.previous}</button>
        <button className="editorial-btn-secondary" disabled={!result?.hasNext}
          onClick={() => setRequest({ ...request, provider: provider.id, page: page + 1 })}>{t.next}</button>
      </div>}
    </section>
  );
}
