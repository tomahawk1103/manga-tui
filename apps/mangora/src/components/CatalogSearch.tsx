import { useEffect, useState } from "react";
import { getTranslations } from "../lib/i18n";
import { isTauriRuntime, searchCatalog, type CatalogPage } from "../lib/tauri";
import type { ProviderOption, UiLanguage } from "../types/mangora";

interface SearchRequest {
  provider: string;
  query: string;
  page: number;
}

export function CatalogSearch({ provider, language }: { provider: ProviderOption; language: UiLanguage }) {
  const t = getTranslations(language).explore;
  const [input, setInput] = useState("");
  const [request, setRequest] = useState<SearchRequest | null>(null);
  const [result, setResult] = useState<CatalogPage | null>(null);
  const [failed, setFailed] = useState(false);
  const [genre, setGenre] = useState("");
  const [lang, setLang] = useState("");
  const desktop = isTauriRuntime();
  const page = request?.provider === provider.id ? request.page : 1;

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
  return (
    <section className="space-y-4" aria-label={t.search}>
      <form className="flex flex-wrap items-end gap-3" onSubmit={event => {
        event.preventDefault();
        if (desktop && input.trim()) setRequest({ provider: provider.id, query: input.trim(), page: 1 });
      }}>
        <label className="min-w-48 flex-1 font-bold">{t.searchTitle}
          <input type="search" value={input} onChange={event => setInput(event.target.value)}
            className="mt-1 block w-full border-2 border-[var(--ink)] bg-[var(--paper)] p-2" />
        </label>
        <button className="editorial-btn-primary" disabled={!desktop || !input.trim()}>{t.search} · {provider.name}</button>
      </form>
      {!desktop && <p>{t.desktopOnly}</p>}
      {request && !result && <p role="status">{failed ? t.unavailable : t.loading}</p>}
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
            {/^https?:\/\//i.test(item.coverUrl) && <img src={item.coverUrl} alt="" loading="lazy"
              className="aspect-[2/3] w-full object-cover" onError={event => { event.currentTarget.hidden = true; }} />}
            <h3 className="font-display p-3">{item.title}</h3>
          </li>)}
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
