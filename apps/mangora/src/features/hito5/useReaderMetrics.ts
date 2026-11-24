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
}

export function useReaderMetrics({
  bridge = defaultHito5Bridge,
  palette,
  language,
  includeScreentone,
  readerAlias,
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
  }, [refresh]);

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
  enabled?: boolean;
  bridge?: Hito5Bridge;
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
  enabled = true,
  bridge = defaultHito5Bridge,
}: UseReadingSessionTrackerOptions): void {
  const activeSecondsRef = useRef(0);
  const visitedPagesRef = useRef<Set<number>>(new Set());
  const completedRef = useRef(false);
  const flushedRef = useRef(false);

  // Increment active seconds only while document is visible (declared before page tracker so reset runs first)
  useEffect(() => {
    if (!enabled || !mangaId || !chapterId) return;
    activeSecondsRef.current = 0;
    visitedPagesRef.current = new Set();
    completedRef.current = false;
    flushedRef.current = false;

    const timer = setInterval(() => {
      if (typeof document === "undefined" || document.visibilityState === "visible") {
        activeSecondsRef.current += 1;
      }
    }, 1000);

    return () => {
      clearInterval(timer);
      if (flushedRef.current) return;
      const elapsed = activeSecondsRef.current;
      const pagesCount = visitedPagesRef.current.size;
      const completed = completedRef.current;
      if (elapsed >= 3 || completed || pagesCount > 0) {
        flushedRef.current = true;
        void bridge
          .recordReadingSession({
            manga_id: mangaId,
            provider,
            chapter_id: chapterId,
            duration_seconds: elapsed,
            pages_read: pagesCount,
            completed_chapter: completed,
          })
          .catch(() => {
            // Non-blocking best-effort local persistence
          });
      }
    };
  }, [enabled, mangaId, provider, chapterId, bridge]);

  // Track visited pages and chapter completion
  useEffect(() => {
    if (!enabled || !chapterId) return;
    if (currentPage > 0) {
      visitedPagesRef.current.add(currentPage);
    }
    if (totalPages > 0 && currentPage >= totalPages) {
      completedRef.current = true;
    }
  }, [enabled, chapterId, currentPage, totalPages]);
}
