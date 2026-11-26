import { useCallback, useEffect, useRef, useState } from "react";
import { defaultHito5Bridge, type Hito5Bridge } from "./bridge";
import type {
  EditorialPalette,
  EditorialStatsCardSvgResponse,
  ExportStatsCardRequest,
  ExtendedReaderMetrics,
  UiLanguage,
} from "./types";

function triggerBrowserDownload(blob: Blob, filename: string): void {
  if (typeof document === "undefined") return;
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function rasterizeSvgToPngBlob(svg: string, width: number, height: number): Promise<Blob> {
  const scale = 2; // 2x HiDPI crisp editorial export (2400x1350)
  const svgBlob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  const svgUrl = URL.createObjectURL(svgBlob);

  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () => reject(new Error("Failed to load SVG into canvas"));
      image.src = svgUrl;
    });

    const canvas = document.createElement("canvas");
    canvas.width = width * scale;
    canvas.height = height * scale;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      throw new Error("Canvas 2D context is unavailable");
    }
    ctx.scale(scale, scale);
    ctx.drawImage(img, 0, 0, width, height);

    const pngBlob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (blob) => {
          if (blob) resolve(blob);
          else reject(new Error("Canvas PNG encoding failed"));
        },
        "image/png",
        1.0,
      );
    });
    return pngBlob;
  } finally {
    URL.revokeObjectURL(svgUrl);
  }
}

export interface UseReaderMetricsOptions {
  bridge?: Hito5Bridge;
  palette: EditorialPalette;
  language: UiLanguage;
  includeScreentone: boolean;
  readerAlias?: string;
  refreshTrigger?: number;
}

export function useReaderMetrics({
  bridge = defaultHito5Bridge,
  palette,
  language,
  includeScreentone,
  readerAlias,
  refreshTrigger = 0,
}: UseReaderMetricsOptions) {
  const [metrics, setMetrics] = useState<ExtendedReaderMetrics | null>(null);
  const [cardPreview, setCardPreview] = useState<EditorialStatsCardSvgResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isExporting, setIsExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const request: ExportStatsCardRequest = {
        palette,
        language,
        reader_alias: readerAlias?.trim() || null,
        include_screentone: includeScreentone,
      };
      const [nextMetrics, nextCard] = await Promise.all([
        bridge.getExtendedReaderMetrics(),
        bridge.exportStatsCardSvg(request),
      ]);
      setMetrics(nextMetrics);
      setCardPreview(nextCard);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [bridge, palette, language, includeScreentone, readerAlias]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshTrigger]);

  const exportSvg = useCallback(async () => {
    setIsExporting(true);
    setError(null);
    try {
      const card = await bridge.exportStatsCardSvg({
        palette,
        language,
        reader_alias: readerAlias?.trim() || null,
        include_screentone: includeScreentone,
      });
      setCardPreview(card);
      const blob = new Blob([card.svg], { type: "image/svg+xml;charset=utf-8" });
      triggerBrowserDownload(blob, card.suggested_filename);
      return card;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return null;
    } finally {
      setIsExporting(false);
    }
  }, [bridge, palette, language, includeScreentone, readerAlias]);

  const exportPng = useCallback(async () => {
    setIsExporting(true);
    setError(null);
    try {
      const card = await bridge.exportStatsCardSvg({
        palette,
        language,
        reader_alias: readerAlias?.trim() || null,
        include_screentone: includeScreentone,
      });
      setCardPreview(card);
      const pngBlob = await rasterizeSvgToPngBlob(card.svg, card.width, card.height);
      const pngName = card.suggested_filename.replace(/\.svg$/i, ".png");
      triggerBrowserDownload(pngBlob, pngName);
      return pngName;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return null;
    } finally {
      setIsExporting(false);
    }
  }, [bridge, palette, language, includeScreentone, readerAlias]);

  return {
    metrics,
    cardPreview,
    isLoading,
    isExporting,
    error,
    refresh,
    exportSvg,
    exportPng,
  };
}

export interface UseReadingSessionTrackerOptions {
  mangaId: string;
  provider: string;
  chapterId: string;
  currentPage: number;
  totalPages: number;
  initialPage?: number;
  visiblePages?: number[];
  enabled?: boolean;
  bridge?: Hito5Bridge;
}

export interface UseReadingSessionTrackerResult {
  flushSession: () => Promise<void>;
}

const SEEN_PAGES_STORAGE_KEY = "mangora.seen_pages.v1";
const MAX_SEEN_PAGE_CHAPTERS = 500;

function loadSeenPagesForChapter(chapterStorageKey: string): Set<number> {
  if (typeof window === "undefined") return new Set();
  try {
    const raw = window.localStorage.getItem(SEEN_PAGES_STORAGE_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw) as Record<string, number[]>;
    const list = parsed?.[chapterStorageKey];
    if (!Array.isArray(list)) return new Set();
    return new Set(list.filter((n) => typeof n === "number" && Number.isFinite(n) && n > 0));
  } catch {
    return new Set();
  }
}

function saveSeenPagesForChapter(chapterStorageKey: string, pages: Set<number>): void {
  if (typeof window === "undefined") return;
  try {
    const raw = window.localStorage.getItem(SEEN_PAGES_STORAGE_KEY);
    const parsed: Record<string, number[]> = raw ? (JSON.parse(raw) as Record<string, number[]>) : {};
    const ordered: Record<string, number[]> = {};
    for (const key of Object.keys(parsed)) {
      if (key !== chapterStorageKey) {
        ordered[key] = parsed[key];
      }
    }
    ordered[chapterStorageKey] = Array.from(pages).sort((a, b) => a - b);
    const keys = Object.keys(ordered);
    if (keys.length > MAX_SEEN_PAGE_CHAPTERS) {
      for (const oldKey of keys.slice(0, keys.length - MAX_SEEN_PAGE_CHAPTERS)) {
        delete ordered[oldKey];
      }
    }
    window.localStorage.setItem(SEEN_PAGES_STORAGE_KEY, JSON.stringify(ordered));
  } catch {
    // Ignore storage quota or restricted WebView errors
  }
}

/**
 * Tracks active local reading time while the reader is visible and persists the
 * session to local SQLite upon chapter completion, chapter change, or unmount.
 */
export function useReadingSessionTracker({
  mangaId,
  provider,
  chapterId,
  currentPage,
  totalPages,
  initialPage = 1,
  visiblePages,
  enabled = true,
  bridge = defaultHito5Bridge,
}: UseReadingSessionTrackerOptions): UseReadingSessionTrackerResult {
  const activeSecondsRef = useRef(0);
  const alreadySeenRef = useRef<Set<number>>(new Set());
  const newPagesRef = useRef<Set<number>>(new Set());
  const chapterKeyRef = useRef("");
  const completedRef = useRef(false);
  const flushedRef = useRef(false);

  const flushSession = useCallback(async () => {
    if (!enabled || !mangaId || !chapterId || flushedRef.current) return;
    const elapsed = activeSecondsRef.current;
    const pagesCount = newPagesRef.current.size;
    const completed = completedRef.current;
    if (elapsed >= 1 || completed || pagesCount > 0) {
      flushedRef.current = true;
      try {
        await bridge.recordReadingSession({
          manga_id: mangaId,
          provider,
          chapter_id: chapterId,
          duration_seconds: elapsed,
          pages_read: pagesCount,
          completed_chapter: completed,
        });
      } catch {
        // Non-blocking best-effort local persistence
      }
    }
  }, [enabled, mangaId, provider, chapterId, bridge]);

  // Increment active seconds only while document is visible (declared before page tracker so reset runs first)
  useEffect(() => {
    if (!enabled || !mangaId || !chapterId) return;
    const storageKey = `${provider.trim().toLowerCase()}::${mangaId}::${chapterId}`;
    chapterKeyRef.current = storageKey;
    const seen = loadSeenPagesForChapter(storageKey);
    if (initialPage > 1) {
      for (let page = 1; page <= initialPage; page += 1) {
        seen.add(page);
      }
      saveSeenPagesForChapter(storageKey, seen);
    }

    activeSecondsRef.current = 0;
    alreadySeenRef.current = seen;
    newPagesRef.current = new Set();
    completedRef.current = false;
    flushedRef.current = false;

    const timer = setInterval(() => {
      if (typeof document === "undefined" || document.visibilityState === "visible") {
        activeSecondsRef.current += 1;
      }
    }, 1000);

    return () => {
      clearInterval(timer);
      void flushSession();
    };
  }, [enabled, mangaId, provider, chapterId, initialPage, flushSession]);

  // Track newly discovered pages (ignoring already-visited or backward pages) and chapter completion
  const visibleSignature = visiblePages?.join(",") ?? "";
  useEffect(() => {
    if (!enabled || !chapterId) return;
    const pagesNow =
      visiblePages && visiblePages.length > 0 ? visiblePages : currentPage > 0 ? [currentPage] : [];
    let added = false;
    for (const pageNum of pagesNow) {
      if (pageNum > 0 && !alreadySeenRef.current.has(pageNum)) {
        alreadySeenRef.current.add(pageNum);
        newPagesRef.current.add(pageNum);
        added = true;
      }
    }
    if (added && chapterKeyRef.current) {
      saveSeenPagesForChapter(chapterKeyRef.current, alreadySeenRef.current);
    }
    const maxVisible = pagesNow.length > 0 ? Math.max(...pagesNow) : currentPage;
    if (totalPages > 0 && maxVisible >= totalPages) {
      completedRef.current = true;
    }
  }, [enabled, chapterId, currentPage, visibleSignature, visiblePages, totalPages]);

  return { flushSession };
}
