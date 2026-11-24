import { useEffect, useRef, useState } from "react";
import { getTranslations } from "../lib/i18n";
import { isTauriRuntime, searchCatalog, type CatalogItem, type CatalogPage } from "../lib/tauri";
import type { ProviderOption, UiLanguage } from "../types/mangora";

interface SearchRequest {
  provider: string;
  query: string;
  page: number;
}

interface CatalogSearchProps {
  provider: ProviderOption;
  language: UiLanguage;
  focusSearch: boolean;
  onChooseSource?: () => void;
}

export function CatalogSearch({ provider, language, focusSearch, onChooseSource }: CatalogSearchProps) {
  const translations = getTranslations(language);
  const t = translations.explore;
  const searchInput = useRef<HTMLInputElement>(null);
  const details = useRef<HTMLDialogElement>(null);
  const [selected, setSelected] = useState<CatalogItem | null>(null);
  const [input, setInput] = useState("");
  const [request, setRequest] = useState<SearchRequest | null>(null);
  const [result, setResult] = useState<CatalogPage | null>(null);
  const [failed, setFailed] = useState(false);
  const [genre, setGenre] = useState("");
  const [lang, setLang] = useState("");
  const desktop = isTauriRuntime();
  const page = request?.provider === provider.id ? request.page : 1;

  useEffect(() => {
    if (focusSearch && desktop) searchInput.current?.focus();
  }, [focusSearch, desktop]);

  useEffect(() => {
    const dialog = details.current;
    if (selected && dialog && !dialog.open) dialog.showModal();
  }, [selected]);

  useEffect(() => {
    if (!request) return;
    let active = true;
    setResult(null);
    setFailed(false);
    setGenre("");
    setLang("");
    searchCatalog(provider.id, request.query, page).then(
      (response) => { if (active) setResult(response); },
      () => { if (active) setFailed(true); },
    );
    return () => { active = false; };
  }, [provider.id, request, page]);

  const items = result?.items ?? [];
  const genres = [...new Set(items.flatMap(item => item.genres))].sort();
  const languages = [...new Set(items.flatMap(item => item.languages))].sort();
  const visible = items.filter(item => (!genre || item.genres.includes(genre)) && (!lang || item.languages.includes(lang)));
  const languageNames = new Intl.DisplayNames([language], { type: "language" });
  return (
    <section className="space-y-4" aria-label={t.search}>
      <form className="flex flex-wrap items-end gap-3" onSubmit={event => {
        event.preventDefault();
        if (desktop && input.trim()) setRequest({ provider: provider.id, query: input.trim(), page: 1 });
      }}>
        <label className="min-w-48 flex-1 font-bold">{t.searchTitle}
          <input ref={searchInput} type="search" value={input} onChange={event => setInput(event.target.value)}
            className="mt-1 block w-full border-2 border-[var(--ink)] bg-[var(--paper)] p-2" />
        </label>
        <button className="editorial-btn-primary" disabled={!desktop || !input.trim()}>{t.search} · {provider.name}</button>
      </form>
      {!desktop && <p>{t.desktopOnly}</p>}
      {request && !result && <p role="status">{failed ? t.unavailable : t.loading}</p>}
      {request && failed && (
        <button
          type="button"
          className="editorial-btn-secondary"
          onClick={() => setRequest({ provider: provider.id, query: request.query, page })}
        >
          {translations.errors.retry}
        </button>
      )}
      {result && request && <>
        <div className="flex flex-wrap gap-4">
          {languages.length > 0 && <label>{t.language} <select value={lang} onChange={event => setLang(event.target.value)}>
            <option value="">{t.all}</option>
            {languages.map(value => <option key={value}>{value}</option>)}
          </select></label>}
          {genres.length > 0 && <label>{t.genre} <select value={genre} onChange={event => setGenre(event.target.value)}>
            <option value="">{t.all}</option>
            {genres.map(value => <option key={value}>{value}</option>)}
          </select></label>}
        </div>
        {!visible.length && <p role="status">{t.empty}</p>}
        <ul className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {visible.map(item => <li key={item.id} className="editorial-card overflow-hidden">
            <button
              type="button"
              aria-haspopup="dialog"
              className="block h-full w-full cursor-pointer text-left hover:bg-[var(--surface)] focus-visible:outline-offset-[-4px]"
              onClick={() => setSelected(item)}
            >
              {/^https?:\/\//i.test(item.coverUrl) && <img src={item.coverUrl} alt="" loading="lazy"
                className="aspect-[2/3] w-full object-cover" onError={event => { event.currentTarget.hidden = true; }} />}
              <span className="font-display block p-3">{item.title}</span>
            </button>
          </li>)}
        </ul>
      </>}
      {onChooseSource && (failed || (result && items.length === 0)) && (
        <button type="button" className="editorial-btn-secondary" onClick={onChooseSource}>
          {t.chooseAnotherSource}
        </button>
      )}
      {request && (result || failed) && (page > 1 || result?.hasNext) && <div className="flex gap-3">
        <button className="editorial-btn-secondary" disabled={page === 1}
          onClick={() => setRequest({ ...request, provider: provider.id, page: page - 1 })}>{t.previous}</button>
        <button className="editorial-btn-secondary" disabled={!result?.hasNext}
          onClick={() => setRequest({ ...request, provider: provider.id, page: page + 1 })}>{t.next}</button>
      </div>}
      <dialog
        ref={details}
        aria-labelledby="catalog-details-title"
        onClose={() => setSelected(null)}
        className="editorial-card fixed inset-0 m-auto max-h-[85vh] w-[calc(100%-2rem)] max-w-2xl overflow-y-auto p-5 text-[var(--ink)] backdrop:bg-black/65"
      >
        {selected && (
          <div className="space-y-4">
            <div className="flex items-start justify-between gap-4">
              <h2 id="catalog-details-title" className="font-display text-2xl">{selected.title}</h2>
              <form method="dialog">
                <button type="submit" className="editorial-btn-secondary text-xs">{t.closeDetails}</button>
              </form>
            </div>
            {/^https?:\/\//i.test(selected.coverUrl) && (
              <img src={selected.coverUrl} alt="" className="max-h-64 max-w-full object-contain"
                onError={event => { event.currentTarget.hidden = true; }} />
            )}
            <p className="text-sm text-[var(--muted)]">{t.providerLabel}: {provider.name}</p>
            <p className="whitespace-pre-line">{selected.description?.trim() || t.noSynopsis}</p>
            {selected.genres.length > 0 && <p>{t.genre}: {selected.genres.join(", ")}</p>}
            {selected.languages.length > 0 && (
              <p>{t.language}: {selected.languages.map(code => {
                try {
                  return languageNames.of(code) ?? code;
                } catch {
                  return code;
                }
              }).join(", ")}</p>
            )}
          </div>
        )}
      </dialog>
    </section>
  );
}
