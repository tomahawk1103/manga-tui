import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { MangoraAppearanceProvider } from "../features/hito5";
import { getInitialState, saveReaderPrefs } from "../lib/tauri";
import type { ReaderPrefs } from "../types/mangora";
import { ReaderView } from "../views/ReaderView";
import { sampleChapter, sampleSource, savedPage } from "./catalog";
import "../styles/editorial.css";

const seriesRoute = "#serie/mangora-sample";
const chapterRoute = "#leer/sample-1";
const root = createRoot(document.getElementById("root")!);

function WebApp({ initialPrefs }: { initialPrefs: ReaderPrefs }) {
  const [readerPrefs, setReaderPrefs] = useState(initialPrefs);
  const [route, setRoute] = useState(window.location.hash);
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState("");

  useEffect(() => {
    const syncRoute = () => {
      setRoute(window.location.hash);
      setNotice("");
    };
    window.addEventListener("hashchange", syncRoute);
    return () => window.removeEventListener("hashchange", syncRoute);
  }, []);

  if (route === chapterRoute) {
    return (
      <>
        <ReaderView
          request={sampleChapter}
          source={sampleSource}
          language="es"
          initialPrefs={readerPrefs}
          onUpdatePrefs={async (prefs) => {
            await saveReaderPrefs(prefs);
            setReaderPrefs(prefs);
          }}
          onClose={() => { window.location.hash = seriesRoute; }}
          onNotify={setNotice}
        />
        {notice && <p role="status" className="editorial-card fixed right-4 bottom-20 z-[60] p-3">{notice}</p>}
      </>
    );
  }

  const isSeries = route === seriesRoute;
  const matches = sampleChapter.mangaTitle.toLocaleLowerCase("es").includes(query.trim().toLocaleLowerCase("es"));
  const resumePage = savedPage();

  return (
    <main className="mx-auto min-h-dvh max-w-4xl px-4 py-8 text-[var(--ink)] sm:px-8">
      <header className="mb-8 flex items-center gap-3 border-b border-[var(--ink)] pb-4">
        <a href="#" className="flex items-center gap-3 font-display text-2xl font-black">
          <span className="mark" aria-hidden="true">M!</span> Mangora
        </a>
      </header>
      {isSeries ? (
        <section aria-labelledby="series-title" className="editorial-card grid gap-5 p-5 sm:grid-cols-[11rem_1fr]">
          <img src={sampleChapter.coverUrl} alt="" className="w-32 self-start sm:w-full" />
          <div className="space-y-4">
            <h1 id="series-title" className="font-display text-3xl">{sampleChapter.mangaTitle}</h1>
            <p className="text-[var(--muted)]">Catálogo de prueba · Español</p>
            <h2 className="font-display text-xl">Capítulos</h2>
            <a className="editorial-btn-primary min-h-[44px]" href={chapterRoute}>
              {resumePage === null ? "Leer capítulo 1" : `Continuar capítulo 1 · Página ${resumePage + 1}`}
            </a>
          </div>
        </section>
      ) : (
        <section aria-labelledby="catalog-title" className="space-y-5">
          <h1 id="catalog-title" className="font-display text-3xl">Catálogo</h1>
          <label className="block max-w-md space-y-2">
            <span className="block font-bold">Buscar por título</span>
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="min-h-[44px] w-full border border-[var(--ink)] bg-[var(--surface)] p-3"
            />
          </label>
          {matches ? (
            <a href={seriesRoute} className="editorial-card flex max-w-md items-center gap-4 p-3">
              <img src={sampleChapter.coverUrl} alt="" className="w-20" />
              <div>
                <h2 className="font-display text-xl">{sampleChapter.mangaTitle}</h2>
                <p className="text-sm text-[var(--muted)]">1 capítulo · Español</p>
              </div>
            </a>
          ) : <p role="status">No se encontraron series.</p>}
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
