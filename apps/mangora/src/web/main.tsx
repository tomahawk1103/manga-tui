import { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { MangoraAppearanceProvider } from "../features/hito5";
import { getInitialState, saveReaderPrefs } from "../lib/tauri";
import type { ReaderPrefs } from "../types/mangora";
import { ReaderView } from "../views/ReaderView";
import { ChapterUpdates } from "../components/ChapterUpdates";
import { chapterRequest, liveMangaOni, loadCatalog, makeReaderSource, openMangaOniSeries, savedPage, searchMangaOni, type Series } from "./catalog";
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
  const [submitted, setSubmitted] = useState("");
  const [details, setDetails] = useState<Series | null>(null);
  const [detailError, setDetailError] = useState("");
  const [section, seriesId, chapterId] = route.split("/");
  const routedId = seriesId === "mangaoni-dandadan" ? "manga%2Fdandadan" : seriesId;
  const liveId = liveMangaOni && (section === "#serie" || section === "#leer") && routedId?.includes("%2F") ? routedId : undefined;
  useEffect(() => {
    setDetails(null);
    setDetailError("");
    if (!liveId) return;
    const controller = new AbortController();
    Promise.resolve().then(() => openMangaOniSeries(decodeURIComponent(liveId), controller.signal))
      .then(series => { if (!controller.signal.aborted) setDetails(series); })
      .catch(() => { if (!controller.signal.aborted) setDetailError("No se pudo cargar la lista de capítulos de MangaOni."); });
    return () => controller.abort();
  }, [liveId, attempt]);

  useEffect(() => {
    const controller = new AbortController();
    setCatalog(null);
    setError("");
    const results = liveMangaOni
      ? submitted ? searchMangaOni(submitted, controller.signal) : loadCatalog()
      : loadCatalog();
    results.then((items) => {
      if (!controller.signal.aborted) setCatalog(items);
    }).catch(() => {
      if (!controller.signal.aborted) setError("No se ha podido cargar el catálogo del servidor.");
    });
    return () => controller.abort();
  }, [attempt, submitted]);

  useEffect(() => {
    const syncRoute = () => {
      setRoute(window.location.hash);
      setNotice("");
    };
    window.addEventListener("hashchange", syncRoute);
    return () => window.removeEventListener("hashchange", syncRoute);
  }, []);

  const isChapter = section === "#leer";
  const legacyChapter = route === "#leer/sample-1";
  const series = liveId ? (details && encodeURIComponent(details.id) === liveId ? details : null) :
    catalog?.find((item) => encodeURIComponent(item.id) === (legacyChapter ? "mangora-sample" : seriesId));
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
          onNavigateChapter={id => { window.location.hash = chapterRoute(request.mangaId, id); }}
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

  const matches = liveMangaOni ? catalog ?? [] : catalog?.filter((item) =>
    item.title.toLocaleLowerCase("es").includes(query.trim().toLocaleLowerCase("es"))) ?? [];

  return (
    <main className="mx-auto min-h-dvh max-w-4xl px-4 py-8 text-[var(--ink)] sm:px-8">
      <header className="mb-8 flex flex-wrap items-center justify-between gap-3 border-b border-[var(--ink)] pb-4">
        <a href="#" className="flex items-center gap-3 font-display text-2xl font-black">
          <span className="mark" aria-hidden="true">M!</span> Mangora
        </a>
        <nav aria-label="Principal" className="flex gap-4 text-sm font-bold">
          <a href="#" aria-current={!section || section === "#" ? "page" : undefined}>Novedades</a>
          <a href="#buscar" aria-current={section === "#buscar" ? "page" : undefined}>Buscar</a>
        </nav>
      </header>
      {(!route || route === "#") ? <ChapterUpdates language="es" /> : (liveId ? detailError : error) ? (
        <div className="space-y-4">
          <p role="alert">{liveId ? detailError : error}</p>
          <button type="button" className="editorial-btn-primary" onClick={() => {
            setError("");
            setAttempt((value) => value + 1);
          }}>Reintentar</button>
        </div>
      ) : liveId && !series ? (
        <p role="status">Cargando capítulos…</p>
      ) : !liveId && !catalog && (section === "#serie" || isChapter) ? (
        <p role="status">Cargando catálogo…</p>
      ) : isChapter || (section === "#serie" && !series) ? (
        <p role="status">Esta serie o capítulo ya no está disponible. <a href="#">Volver al catálogo</a></p>
      ) : section === "#serie" && series ? (
        <section aria-labelledby="series-title" className={`editorial-card grid gap-5 p-5 ${series.cover ? "sm:grid-cols-[11rem_1fr]" : ""}`}>
          {series.cover && <img src={series.cover} alt="" className="w-32 self-start sm:w-full" />}
          <div className="space-y-4">
            <h1 id="series-title" className="font-display text-3xl">{series.title}</h1>
            <h2 className="font-display text-xl">Capítulos ({series.chapters.length})</h2>
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
          <form className="space-y-2" onSubmit={event => {
            event.preventDefault();
            setSubmitted(query.trim());
          }}>
          <label className="block max-w-xl space-y-2">
            <span className="block font-bold">Buscar por título</span>
            <input type="search" maxLength={120} value={query} placeholder={liveMangaOni ? "Buscar en MangaOni" : "Buscar en el catálogo"}
              onChange={(event) => setQuery(event.target.value)}
              className="min-h-[44px] w-full border border-[var(--ink)] bg-[var(--surface)] p-3" />
          </label>
          {liveMangaOni && <button className="editorial-btn-primary" disabled={!query.trim()}>Buscar en MangaOni</button>}
          </form>
          {matches.length ? matches.map((item) => (
            <a key={item.id} href={seriesRoute(item.id)} className="editorial-card flex max-w-md items-center gap-4 p-3">
              {item.cover && <img src={item.cover} alt="" className="w-20" />}
              <div>
                <h2 className="font-display text-xl">{item.title}</h2>
                <p className="text-sm text-[var(--muted)]">{liveMangaOni ? "Ver capítulos" : `${item.chapters.length} ${item.chapters.length === 1 ? "capítulo" : "capítulos"}`}</p>
              </div>
            </a>
          )) : !catalog ? <p role="status">Cargando catálogo…</p> : (!liveMangaOni || submitted) && <p role="status">No se encontraron series.</p>}
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
