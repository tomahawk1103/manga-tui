import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";
import { RefreshCw } from "lucide-react";
import { isTauriRuntime } from "../lib/tauri";
import { loadUpdatesPage, type ChapterUpdate } from "../web/catalog";

interface Props {
  language: "es" | "en";
  onSeries?: (item: ChapterUpdate) => void;
  onChapter?: (item: ChapterUpdate) => void;
}

const INITIAL_VISIBLE_COUNT = 8;
const REVEAL_BATCH_SIZE = 8;

export function ChapterUpdates({ language, onSeries, onChapter }: Props) {
  const [items, setItems] = useState<ChapterUpdate[]>([]);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [attempt, setAttempt] = useState(0);
  const [visibleCount, setVisibleCount] = useState(INITIAL_VISIBLE_COUNT);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);

  const es = language === "es";
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const loadingMoreRef = useRef(false);

  useEffect(() => {
    const controller = new AbortController();
    setStatus("loading");
    setVisibleCount(INITIAL_VISIBLE_COUNT);
    setPage(1);
    setHasMore(true);
    setLoadingMore(false);
    loadingMoreRef.current = false;

    loadUpdatesPage(1, language, controller.signal)
      .then((result) => {
        if (!controller.signal.aborted) {
          setItems(result.items);
          setHasMore(result.hasMore);
          setStatus("ready");
        }
      })
      .catch(() => {
        if (!controller.signal.aborted) setStatus("error");
      });
    return () => controller.abort();
  }, [attempt, language]);

  const loadMore = useCallback(async () => {
    if (visibleCount < items.length) {
      setVisibleCount((n) => Math.min(items.length, n + REVEAL_BATCH_SIZE));
      return;
    }
    if (!hasMore || loadingMoreRef.current) return;
    loadingMoreRef.current = true;
    setLoadingMore(true);
    const nextPage = page + 1;
    const controller = new AbortController();
    try {
      const result = await loadUpdatesPage(nextPage, language, controller.signal);
      setItems((prev) => {
        const seen = new Set(
          prev.map((item) => `${item.provider ?? "mangaoni"}:${item.id}:${item.chapterId}`),
        );
        const fresh = result.items.filter(
          (item) => !seen.has(`${item.provider ?? "mangaoni"}:${item.id}:${item.chapterId}`),
        );
        const merged = [...prev, ...fresh];
        setVisibleCount((v) => Math.min(merged.length, v + REVEAL_BATCH_SIZE));
        return merged;
      });
      setPage(nextPage);
      setHasMore(result.hasMore);
    } catch {
      setHasMore(false);
    } finally {
      loadingMoreRef.current = false;
      setLoadingMore(false);
    }
  }, [visibleCount, items.length, hasMore, page, language]);

  useEffect(() => {
    if (status !== "ready") return;
    const sentinel = sentinelRef.current;
    const root = scrollContainerRef.current;
    if (!sentinel || !root) return;

    if (typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          void loadMore();
        }
      },
      { root, rootMargin: "220px 0px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [status, loadMore]);

  function follow(event: MouseEvent<HTMLAnchorElement>, action: Props["onSeries"], item: ChapterUpdate) {
    if (!action || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    action(item);
  }

  const base = onSeries && isTauriRuntime() ? "https://tomahawk1103.github.io/manga-tui/mangora-web/" : "";
  const dateFormat = new Intl.DateTimeFormat(language, { day: "numeric", month: "long", timeZone: "UTC" });
  const visibleItems = items.slice(0, visibleCount);

  return (
    <section aria-labelledby="updates-title" className="space-y-4">
      <div className="flex flex-wrap items-end justify-between gap-2 border-b-4 border-[var(--ink)] pb-3">
        <div>
          <h1 id="updates-title" className="font-display text-4xl sm:text-5xl">
            {es ? "Novedades" : "Latest chapters"}
          </h1>
          <p className="mt-1 text-sm text-[var(--muted)]">
            MangaOni &amp; MangaDex · {es ? "En español" : "Latest releases"}
          </p>
        </div>
        {status === "ready" && items.length > 0 && (
          <span className="font-mono-editorial border border-[var(--ink)] bg-[var(--surface)] px-2.5 py-1 text-xs font-bold text-[var(--ink)]">
            {es ? `Mostrando ${visibleItems.length}` : `Showing ${visibleItems.length}`}
          </span>
        )}
      </div>

      {status === "loading" && (
        <p role="status">{es ? "Cargando novedades…" : "Loading new chapters…"}</p>
      )}
      {status === "error" && (
        <div className="space-y-2">
          <p role="alert">
            {es ? "No se pudieron cargar los capítulos recientes." : "Recent chapters could not be loaded."}
          </p>
          <button
            type="button"
            className="editorial-btn-secondary"
            onClick={() => setAttempt((value) => value + 1)}
          >
            {es ? "Reintentar" : "Retry"}
          </button>
        </div>
      )}

      {status === "ready" && (
        <div
          ref={scrollContainerRef}
          className="max-h-[540px] space-y-4 overflow-y-auto pr-1 sm:max-h-[600px]"
        >
          <ul className="grid gap-4 sm:grid-cols-2">
            {visibleItems.map((item) => {
              const prov = item.provider ?? "mangaoni";
              const series = `${base}#serie/${encodeURIComponent(prov)}/${encodeURIComponent(item.id)}`;
              const chapter = `${base}#leer/${encodeURIComponent(prov)}/${encodeURIComponent(item.id)}/${encodeURIComponent(item.chapterId)}`;
              return (
                <li
                  key={`${prov}:${item.id}:${item.chapterId}`}
                  className="editorial-card flex overflow-hidden bg-[var(--paper)]"
                >
                  {item.cover && (
                    <a
                      href={chapter}
                      tabIndex={-1}
                      aria-hidden="true"
                      onClick={(event) => follow(event, onChapter, item)}
                      className="w-24 shrink-0 sm:w-28"
                    >
                      <img
                        src={item.cover}
                        loading="lazy"
                        referrerPolicy="no-referrer"
                        alt=""
                        className="h-full min-h-36 w-full object-cover"
                      />
                    </a>
                  )}
                  <div className="flex min-w-0 flex-1 flex-col items-start justify-between gap-2 p-3">
                    <div className="flex flex-wrap items-center gap-2">
                      {item.published && (
                        <time
                          dateTime={item.published}
                          className="font-mono-editorial text-xs text-[var(--muted)]"
                        >
                          {dateFormat.format(new Date(item.published + "T12:00:00Z"))}
                        </time>
                      )}
                      {prov === "mangadex" && (
                        <span className="border border-[var(--ink)] bg-[var(--surface)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold uppercase text-[var(--ink)]">
                          MangaDex
                        </span>
                      )}
                    </div>
                    <h2 className="font-display text-lg leading-tight">
                      <a href={series} onClick={(event) => follow(event, onSeries, item)}>
                        {item.title}
                      </a>
                    </h2>
                    <a
                      href={chapter}
                      onClick={(event) => follow(event, onChapter, item)}
                      aria-label={item.title + " · " + item.chapterTitle}
                      className="editorial-btn-primary min-h-[44px] text-xs"
                    >
                      {item.chapterTitle} →
                    </a>
                  </div>
                </li>
              );
            })}
          </ul>

          <div ref={sentinelRef} className="flex min-h-[36px] items-center justify-center py-2">
            {loadingMore ? (
              <span className="inline-flex items-center gap-2 font-mono-editorial text-xs text-[var(--muted)]">
                <RefreshCw className="h-3.5 w-3.5 animate-spin text-[var(--red)]" aria-hidden="true" />
                <span>{es ? "Cargando más novedades…" : "Loading more chapters…"}</span>
              </span>
            ) : !hasMore && visibleCount >= items.length ? (
              <span className="font-mono-editorial text-xs text-[var(--muted)]">
                {es ? "Has llegado al final de las novedades recientes." : "You reached the end of recent releases."}
              </span>
            ) : null}
          </div>
        </div>
      )}
    </section>
  );
}
