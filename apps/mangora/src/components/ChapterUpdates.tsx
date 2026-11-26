import { useEffect, useState, type MouseEvent } from "react";
import { loadUpdates, type ChapterUpdate } from "../web/catalog";

interface Props {
  language: "es" | "en";
  onSeries?: (item: ChapterUpdate) => void;
  onChapter?: (item: ChapterUpdate) => void;
}

export function ChapterUpdates({ language, onSeries, onChapter }: Props) {
  const [items, setItems] = useState<ChapterUpdate[]>([]);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [attempt, setAttempt] = useState(0);
  const es = language === "es";
  const [visibleCount, setVisibleCount] = useState(6);
  useEffect(() => {
    const controller = new AbortController();
    setStatus("loading");
    loadUpdates(controller.signal).then(result => {
      if (!controller.signal.aborted) {
        setItems(result);
        setStatus("ready");
      }
    }).catch(() => { if (!controller.signal.aborted) setStatus("error"); });
    return () => controller.abort();
  }, [attempt]);

  function follow(event: MouseEvent<HTMLAnchorElement>, action: Props["onSeries"], item: ChapterUpdate) {
    if (!action || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    action(item);
  }
  const base = onSeries ? "https://tomahawk1103.github.io/manga-tui/mangora-web/" : "";
  const dateFormat = new Intl.DateTimeFormat(language, { day: "numeric", month: "long", timeZone: "UTC" });
  return (
    <section aria-labelledby="updates-title" className="space-y-5">
      <div className="border-b-4 border-[var(--ink)] pb-3">
        <h1 id="updates-title" className="font-display text-4xl sm:text-5xl">{es ? "Novedades" : "Latest chapters"}</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">MangaOni · {es ? "En español" : "Spanish releases"}</p>
      </div>
      {status === "loading" && <p role="status">{es ? "Cargando novedades…" : "Loading new chapters…"}</p>}
      {status === "error" && <div className="space-y-2">
        <p role="alert">{es ? "No se pudieron cargar los capítulos recientes." : "Recent chapters could not be loaded."}</p>
        <button className="editorial-btn-secondary" onClick={() => setAttempt(value => value + 1)}>{es ? "Reintentar" : "Retry"}</button>
      </div>}
      {status === "ready" && <ul className="grid gap-4 sm:grid-cols-2">
        {items.slice(0, visibleCount).map(item => {
          const series = base + "#serie/" + encodeURIComponent(item.id);
          const chapter = base + "#leer/" + encodeURIComponent(item.id) + "/" + encodeURIComponent(item.chapterId);
          return (
            <li key={chapter} className="editorial-card flex overflow-hidden bg-[var(--paper)]">
              {item.cover && <a href={chapter} tabIndex={-1} aria-hidden="true" onClick={event => follow(event, onChapter, item)} className="w-24 shrink-0 sm:w-28">
                <img src={item.cover} loading="lazy" alt="" className="h-full min-h-36 w-full object-cover" />
              </a>}
              <div className="flex min-w-0 flex-1 flex-col items-start justify-between gap-2 p-3">
                {item.published && <time dateTime={item.published} className="font-mono-editorial text-xs text-[var(--muted)]">
                  {dateFormat.format(new Date(item.published + "T12:00:00Z"))}
                </time>}
                <h2 className="font-display text-lg leading-tight"><a href={series} onClick={event => follow(event, onSeries, item)}>{item.title}</a></h2>
                <a href={chapter} onClick={event => follow(event, onChapter, item)}
                  aria-label={item.title + " · " + item.chapterTitle}
                  className="editorial-btn-primary min-h-[44px] text-xs">{item.chapterTitle} →</a>
              </div>
            </li>
          );
        })}
      </ul>}
      {status === "ready" && visibleCount < items.length && (
        <button type="button" className="editorial-btn-secondary" onClick={() => setVisibleCount(n => n + 6)}>
          {es ? "Ver más capítulos" : "Show more chapters"}
        </button>
      )}
    </section>
  );
}
