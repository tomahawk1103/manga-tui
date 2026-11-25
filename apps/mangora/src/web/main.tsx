import { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { MangoraAppearanceProvider } from "../features/hito5";
import { getInitialState, saveReaderPrefs } from "../lib/tauri";
import type { ReaderPrefs } from "../types/mangora";
import { ReaderView } from "../views/ReaderView";
import { chapterRequest, loadCatalog, makeReaderSource, savedPage, type Series } from "./catalog";
import "../styles/editorial.css";

const seriesRoute = (seriesId: string) => `#serie/${encodeURIComponent(seriesId)}`;
const chapterRoute = (seriesId: string, chapterId: string) =>
  `#leer/${encodeURIComponent(seriesId)}/${encodeURIComponent(chapterId)}`;
const root = createRoot(document.getElementById("root")!);

function WebApp({ initialPrefs }: { initialPrefs: ReaderPrefs }) {
  const [readerPrefs, setReaderPrefs] = useState(initialPrefs);
  const [route, setRoute] = useState(window.location.hash);
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState("");
  const [catalog, setCatalog] = useState<Series[] | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    loadCatalog().then((items) => {
      if (active) setCatalog(items);
    }).catch(() => {
      if (active) setError("No se ha podido cargar el catálogo del servidor.");
    });
    return () => { active = false; };
  }, [attempt]);

  useEffect(() => {
    const syncRoute = () => {
      setRoute(window.location.hash);
      setNotice("");
    };
    window.addEventListener("hashchange", syncRoute);
    return () => window.removeEventListener("hashchange", syncRoute);
  }, []);

  const [section, seriesId, chapterId] = route.split("/");
  const isChapter = section === "#leer";
  const legacyChapter = route === "#leer/sample-1";
  const series = catalog?.find((item) => encodeURIComponent(item.id) === (legacyChapter ? "mangora-sample" : seriesId));
  const chapter = isChapter
    ? series?.chapters.find((item) => encodeURIComponent(item.id) === (legacyChapter ? "sample-1" : chapterId))
    : undefined;
  const source = useMemo(() => series && makeReaderSource(series), [series]);
  const request = useMemo(() => series && chapter && chapterRequest(series, chapter), [series, chapter]);

  if (request && source) {
    return (
      <>
        <ReaderView
          key={route}
          request={request}
          source={source}
          language="es"
          initialPrefs={readerPrefs}
          onUpdatePrefs={async (prefs) => {
            await saveReaderPrefs(prefs);
            setReaderPrefs(prefs);
          }}
          onClose={() => { window.location.hash = seriesRoute(request.mangaId); }}
          onNotify={setNotice}
        />
        {notice && <p role="status" className="editorial-card fixed right-4 bottom-20 z-[60] p-3">{notice}</p>}
      </>
    );
  }

  const matches = catalog?.filter((item) =>
    item.title.toLocaleLowerCase("es").includes(query.trim().toLocaleLowerCase("es"))) ?? [];

  return (
    <main className="mx-auto min-h-dvh max-w-4xl px-4 py-8 text-[var(--ink)] sm:px-8">
      <header className="mb-8 flex items-center gap-3 border-b border-[var(--ink)] pb-4">
        <a href="#" className="flex items-center gap-3 font-display text-2xl font-black">
          <span className="mark" aria-hidden="true">M!</span> Mangora
        </a>
      </header>
      {error ? (
        <div className="space-y-4">
          <p role="alert">{error}</p>
          <button type="button" className="editorial-btn-primary" onClick={() => {
            setError("");
            setAttempt((value) => value + 1);
          }}>Reintentar</button>
        </div>
      ) : !catalog ? (
        <p role="status">Cargando catálogo…</p>
      ) : isChapter || (section === "#serie" && !series) ? (
        <p role="status">Esta serie o capítulo ya no está disponible. <a href="#">Volver al catálogo</a></p>
      ) : section === "#serie" && series ? (
        <section aria-labelledby="series-title" className="editorial-card grid gap-5 p-5 sm:grid-cols-[11rem_1fr]">
          <img src={series.cover} alt="" className="w-32 self-start sm:w-full" />
          <div className="space-y-4">
            <h1 id="series-title" className="font-display text-3xl">{series.title}</h1>
            <h2 className="font-display text-xl">Capítulos</h2>
            <ul className="space-y-2">
              {series.chapters.map((item) => {
                const resumePage = savedPage(series.id, item.id);
                return (
                  <li key={item.id}>
                    <a className="editorial-btn-primary min-h-[44px]" href={chapterRoute(series.id, item.id)}>
                      {resumePage === null ? item.title : `Continuar · ${item.title} · Página ${resumePage + 1}`}
                    </a>
                  </li>
                );
              })}
            </ul>
          </div>
        </section>
      ) : (
        <section aria-labelledby="catalog-title" className="space-y-5">
          <h1 id="catalog-title" className="font-display text-3xl">Catálogo</h1>
          <form action="https://inmanga.com/manga/consult" method="get" className="flex max-w-xl flex-wrap items-end gap-2">
            <label className="min-w-40 flex-1 space-y-2">
              <span className="block font-bold">Buscar por título</span>
              <input type="search" name="suggestion" required value={query} placeholder="Ej. One Piece"
                onChange={(event) => setQuery(event.target.value)}
                className="min-h-[44px] w-full border border-[var(--ink)] bg-[var(--surface)] p-3" />
            </label>
            <button type="submit" disabled={!query.trim()} className="editorial-btn-primary min-h-[44px] disabled:opacity-50">Buscar en InManga</button>
          </form>
          <a className="inline-block text-sm underline" href="https://inmanga.com/manga/consult">Abrir catálogo de InManga</a>
          {matches.length ? matches.map((item) => (
            <a key={item.id} href={seriesRoute(item.id)} className="editorial-card flex max-w-md items-center gap-4 p-3">
              <img src={item.cover} alt="" className="w-20" />
              <div>
                <h2 className="font-display text-xl">{item.title}</h2>
                <p className="text-sm text-[var(--muted)]">{item.chapters.length} {item.chapters.length === 1 ? "capítulo" : "capítulos"}</p>
              </div>
            </a>
          )) : null}
        </section>
      )}
    </main>
  );
}

void getInitialState().then(({ readerPrefs }) => {
  root.render(
    <MangoraAppearanceProvider>
      <WebApp initialPrefs={readerPrefs} />
    </MangoraAppearanceProvider>,
  );
});
