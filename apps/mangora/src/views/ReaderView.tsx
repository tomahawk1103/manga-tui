import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type TouchEvent as ReactTouchEvent,
} from "react";
import {
  AlertTriangle,
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  HardDrive,
  Maximize2,
  Minimize2,
  Pin,
  PinOff,
  RefreshCw,
  Settings2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import {
  getHito5Labels,
  getReaderBackgroundClass,
  type ReaderBackground,
  useMangoraAppearance,
  useReadingSessionTracker,
} from "../features/hito5";
import { getTranslations } from "../lib/i18n";
import { useScreenWakeLock } from "../lib/mobilePlatform";
import {
  buildReaderPageUrl,
  openReaderChapter,
  saveReaderProgress,
  toggleNativeFullscreen,
} from "../lib/tauri";
import type {
  OpenReaderChapterRequest,
  PageFitMode,
  ReaderChapterNeighbor,
  ReaderPrefs,
  ReaderSessionResponse,
  ReaderViewMode,
  ReadingDirection,
  UiLanguage,
} from "../types/mangora";

export interface ReaderViewProps {
  request: OpenReaderChapterRequest;
  language: UiLanguage;
  initialPrefs: ReaderPrefs;
  onUpdatePrefs: (prefs: ReaderPrefs) => Promise<void>;
  onClose: () => Promise<void> | void;
  onNotify: (message: string) => void;
  onRegisterBackHandler?: (handler: (() => Promise<boolean>) | null) => void;
  source?: ReaderSource;
  onNavigateChapter?: (chapterId: string) => void;
}

export interface ReaderSource {
  openChapter: typeof openReaderChapter;
  pageUrl: typeof buildReaderPageUrl;
  saveProgress: typeof saveReaderProgress;
  pageSize?: (sessionId: string) => { width: number; height: number } | undefined;
}

const nativeSource: ReaderSource = {
  openChapter: openReaderChapter,
  pageUrl: buildReaderPageUrl,
  saveProgress: saveReaderProgress,
};

function buildSpreads(pageCount: number, firstPageSingle: boolean): number[][] {
  if (pageCount <= 0) return [];
  const spreads: number[][] = [];
  let idx = 0;
  if (firstPageSingle) {
    spreads.push([0]);
    idx = 1;
  }
  while (idx < pageCount) {
    if (idx + 1 < pageCount) {
      spreads.push([idx, idx + 1]);
      idx += 2;
    } else {
      spreads.push([idx]);
      idx += 1;
    }
  }
  return spreads;
}

function clampZoom(value: number): number {
  return Math.max(50, Math.min(250, Math.round(value)));
}

export function ReaderView({
  request,
  language,
  initialPrefs,
  onUpdatePrefs,
  onClose,
  onNotify,
  onRegisterBackHandler,
  source = nativeSource,
  onNavigateChapter,
}: ReaderViewProps) {
  const t = getTranslations(language).reader;
  const hito5Labels = getHito5Labels(language);
  const { config: appearance, readerBackgrounds, updateAppearance } = useMangoraAppearance();

  const [activeRequest, setActiveRequest] = useState<OpenReaderChapterRequest>(request);
  const [session, setSession] = useState<ReaderSessionResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [prefs, setPrefs] = useState<ReaderPrefs>(initialPrefs);
  const [currentPage, setCurrentPage] = useState(0);
  const [hudVisible, setHudVisible] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [reloadNonce, setReloadNonce] = useState(0);
  const [failedPages, setFailedPages] = useState<Record<number, number>>({});

  const cascadeContainerRef = useRef<HTMLDivElement>(null);
  const pageRefs = useRef<Array<HTMLDivElement | null>>([]);
  const latestSessionRef = useRef<ReaderSessionResponse | null>(null);
  const latestPageRef = useRef(0);
  const lastSavedKeyRef = useRef<string>("");

  const touchStartRef = useRef<{ x: number; y: number; time: number } | null>(null);
  const pinchStartRef = useRef<{ dist: number; zoom: number } | null>(null);
  const lastTapTimeRef = useRef(0);
  const pendingTapTimerRef = useRef<number | null>(null);

  useEffect(() => {
    setActiveRequest(request);
  }, [request]);

  useEffect(() => {
    latestSessionRef.current = session;
  }, [session]);

  useEffect(() => {
    latestPageRef.current = currentPage;
  }, [currentPage]);

  const spreads = useMemo(() => {
    if (!session) return [];
    if (prefs.viewMode === "double") {
      return buildSpreads(session.pageCount, prefs.firstPageSingle);
    }
    return Array.from({ length: session.pageCount }, (_, idx) => [idx]);
  }, [session, prefs.viewMode, prefs.firstPageSingle]);

  const currentSpreadIndex = useMemo(() => {
    if (spreads.length === 0) return 0;
    const found = spreads.findIndex((pair) => pair.includes(currentPage));
    return found >= 0 ? found : 0;
  }, [spreads, currentPage]);

  const activeSpreadPages = spreads[currentSpreadIndex] ?? [];

  const { flushSession } = useReadingSessionTracker({
    mangaId: session?.mangaId ?? "",
    provider: session?.provider ?? "",
    chapterId: session?.chapterId ?? "",
    currentPage: session ? currentPage + 1 : 0,
    totalPages: session?.pageCount ?? 0,
    initialPage: session ? session.initialPage + 1 : 1,
    visiblePages: session
      ? prefs.viewMode === "double"
        ? activeSpreadPages.map((idx) => idx + 1)
        : [currentPage + 1]
      : [],
    enabled: Boolean(session && !isLoading && !loadError),
  });

  const flushProgress = useCallback(
    async (targetSession: ReaderSessionResponse | null, pageIdx: number) => {
      if (!targetSession || targetSession.pageCount <= 0) return;
      const bounded = Math.max(0, Math.min(targetSession.pageCount - 1, pageIdx));
      const key = `${targetSession.chapterId}::${bounded}`;
      if (lastSavedKeyRef.current === key) return;
      lastSavedKeyRef.current = key;
      try {
        await source.saveProgress({
          mangaId: targetSession.mangaId,
          provider: targetSession.provider,
          mangaTitle: targetSession.mangaTitle,
          coverUrl: targetSession.coverUrl,
          chapterId: targetSession.chapterId,
          chapterTitle: targetSession.chapterTitle,
          language: targetSession.language,
          pageIndex: bounded,
          totalPages: targetSession.pageCount,
        });
      } catch {
        // Keep reading fluid even if transient DB busy lock happens
      }
    },
    [source],
  );

  // Open selected chapter session
  useEffect(() => {
    let active = true;
    setIsLoading(true);
    setLoadError(null);
    setFailedPages({});
    pageRefs.current = [];

    source.openChapter(activeRequest)
      .then((res) => {
        if (!active) return;
        setSession(res);
        setCurrentPage(res.initialPage);
        latestPageRef.current = res.initialPage;
        lastSavedKeyRef.current = `${res.chapterId}::${res.initialPage}`;
      })
      .catch((error) => {
        if (!active) return;
        setLoadError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (active) setIsLoading(false);
      });

    return () => {
      active = false;
    };
  }, [activeRequest, reloadNonce, source]);

  // Restore initial page scroll position in cascade mode, or reset to top in paged modes
  useEffect(() => {
    if (!session || prefs.viewMode !== "cascade" || session.initialPage <= 0) return;
    const timer = window.setTimeout(() => {
      const target = pageRefs.current[session.initialPage];
      target?.scrollIntoView({ behavior: "auto", block: "start" });
    }, 80);
    return () => window.clearTimeout(timer);
  }, [session?.sessionId, session?.initialPage, prefs.viewMode]);

  useEffect(() => {
    if (prefs.viewMode === "cascade") return;
    cascadeContainerRef.current?.scrollTo({ top: 0, left: 0, behavior: "auto" });
  }, [currentPage, prefs.viewMode, session?.sessionId]);

  useEffect(() => {
    const save = () => void flushProgress(latestSessionRef.current, latestPageRef.current);
    const onVisibilityChange = () => {
      if (document.visibilityState === "hidden") save();
    };
    window.addEventListener("pagehide", save);
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      window.removeEventListener("pagehide", save);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      save();
    };
  }, [flushProgress]);

  // Debounced auto-save reading progress on page change
  useEffect(() => {
    if (!session) return;
    const timer = window.setTimeout(() => {
      void flushProgress(session, currentPage);
    }, 400);
    return () => window.clearTimeout(timer);
  }, [session, currentPage, flushProgress]);

  // Preload next 3 pages in background
  useEffect(() => {
    if (!session || session.pageCount <= 0) return;
    const preloaded: HTMLImageElement[] = [];
    for (let offset = 1; offset <= 3; offset += 1) {
      const idx = currentPage + offset;
      if (idx < session.pageCount) {
        const img = new Image();
        img.src = source.pageUrl(session.sessionId, idx);
        preloaded.push(img);
      }
    }
    return () => {
      preloaded.forEach((img) => {
        img.src = "";
      });
    };
  }, [session, currentPage, source]);

  const applyPrefs = useCallback(
    (patch: Partial<ReaderPrefs>, persist = true) => {
      setPrefs((prev) => {
        const next: ReaderPrefs = {
          ...prev,
          ...patch,
          zoomPercent: clampZoom(patch.zoomPercent ?? prev.zoomPercent),
        };
        if (next.pinToolbars) {
          setHudVisible(true);
        }
        if (persist) {
          void onUpdatePrefs(next);
        }
        return next;
      });
    },
    [onUpdatePrefs],
  );

  const handleOpenNeighbor = useCallback(
    async (neighbor: ReaderChapterNeighbor | null) => {
      if (!neighbor) return;
      await Promise.all([
        flushProgress(latestSessionRef.current, latestPageRef.current),
        flushSession(),
      ]);
      if (onNavigateChapter) {
        onNavigateChapter(neighbor.id);
        return;
      }
      setActiveRequest((prev) => ({
        ...prev,
        chapterId: neighbor.id,
        chapterTitle: neighbor.title,
        chapterNumber: neighbor.chapterNumber,
        volumeNumber: neighbor.volumeNumber,
        language: neighbor.language,
      }));
    },
    [flushProgress, flushSession, onNavigateChapter],
  );

  const handleClose = useCallback(async () => {
    await Promise.all([
      flushProgress(latestSessionRef.current, latestPageRef.current),
      flushSession(),
    ]);
    if (isFullscreen) {
      await toggleNativeFullscreen().catch(() => false);
    }
    await onClose();
  }, [flushProgress, flushSession, isFullscreen, onClose]);

  const jumpToPage = useCallback(
    (targetPage: number) => {
      if (!session || session.pageCount <= 0) return;
      const clamped = Math.max(0, Math.min(session.pageCount - 1, targetPage));
      setCurrentPage(clamped);
      if (prefs.viewMode === "cascade") {
        pageRefs.current[clamped]?.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    },
    [session, prefs.viewMode],
  );

  const goLogicalNext = useCallback(() => {
    if (!session || spreads.length === 0) return;
    if (prefs.viewMode === "cascade") {
      const container = cascadeContainerRef.current;
      if (container) {
        container.scrollBy({ top: Math.round(container.clientHeight * 0.8), behavior: "smooth" });
      }
      return;
    }
    if (currentSpreadIndex + 1 < spreads.length) {
      setCurrentPage(spreads[currentSpreadIndex + 1][0]);
    } else if (session.nextChapter) {
      void handleOpenNeighbor(session.nextChapter);
    } else {
      onNotify(t.noNextChapter);
    }
  }, [session, spreads, prefs.viewMode, currentSpreadIndex, handleOpenNeighbor, onNotify, t.noNextChapter]);

  const goLogicalPrev = useCallback(() => {
    if (!session || spreads.length === 0) return;
    if (prefs.viewMode === "cascade") {
      const container = cascadeContainerRef.current;
      if (container) {
        container.scrollBy({ top: -Math.round(container.clientHeight * 0.8), behavior: "smooth" });
      }
      return;
    }
    if (currentSpreadIndex > 0) {
      setCurrentPage(spreads[currentSpreadIndex - 1][0]);
    } else if (session.prevChapter) {
      void handleOpenNeighbor(session.prevChapter);
    }
  }, [session, spreads, prefs.viewMode, currentSpreadIndex, handleOpenNeighbor]);

  const handleToggleFullscreen = useCallback(async () => {
    const next = await toggleNativeFullscreen();
    setIsFullscreen(next);
  }, []);

  useScreenWakeLock(Boolean(session && !isLoading && !loadError));

  useEffect(() => {
    if (!onRegisterBackHandler) return;
    onRegisterBackHandler(async () => {
      if (settingsOpen) {
        setSettingsOpen(false);
        return false;
      }
      await handleClose();
      return true;
    });
    return () => onRegisterBackHandler(null);
  }, [onRegisterBackHandler, settingsOpen, handleClose]);

  // Desktop keyboard shortcuts
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) {
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        if (settingsOpen) {
          setSettingsOpen(false);
        } else {
          void handleClose();
        }
        return;
      }
      if (event.key === "f" || event.key === "F") {
        event.preventDefault();
        void handleToggleFullscreen();
        return;
      }
      if (event.key === "+" || event.key === "=") {
        event.preventDefault();
        applyPrefs({ zoomPercent: prefs.zoomPercent + 15 });
        return;
      }
      if (event.key === "-") {
        event.preventDefault();
        applyPrefs({ zoomPercent: prefs.zoomPercent - 15 });
        return;
      }
      if (event.key === "0") {
        event.preventDefault();
        applyPrefs({ zoomPercent: 100 });
        return;
      }
      if (event.key === " " || event.key === "PageDown") {
        event.preventDefault();
        if (event.shiftKey) goLogicalPrev();
        else goLogicalNext();
        return;
      }
      if (event.key === "PageUp") {
        event.preventDefault();
        goLogicalPrev();
        return;
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        if (prefs.direction === "rtl") goLogicalPrev();
        else goLogicalNext();
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        if (prefs.direction === "rtl") goLogicalNext();
        else goLogicalPrev();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    settingsOpen,
    handleClose,
    handleToggleFullscreen,
    applyPrefs,
    prefs.zoomPercent,
    prefs.direction,
    goLogicalNext,
    goLogicalPrev,
  ]);

  const handleCascadeScroll = () => {
    const container = cascadeContainerRef.current;
    if (!container || !session) return;
    const containerRect = container.getBoundingClientRect();
    const probeY = containerRect.top + Math.min(180, containerRect.height * 0.25);

    let closestIdx = currentPage;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (let idx = 0; idx < session.pageCount; idx += 1) {
      const el = pageRefs.current[idx];
      if (!el) continue;
      const rect = el.getBoundingClientRect();
      if (rect.top <= probeY && rect.bottom >= probeY) {
        closestIdx = idx;
        bestDistance = 0;
        break;
      }
      const dist = Math.abs(rect.top - probeY);
      if (dist < bestDistance) {
        bestDistance = dist;
        closestIdx = idx;
      }
    }
    if (closestIdx !== currentPage) {
      setCurrentPage(closestIdx);
    }
  };

  const toggleHudIfUnpinned = useCallback(() => {
    if (!prefs.pinToolbars) {
      setHudVisible((prev) => !prev);
      setSettingsOpen(false);
    }
  }, [prefs.pinToolbars]);

  const handleViewportClick = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (prefs.viewMode === "cascade") {
      toggleHudIfUnpinned();
      return;
    }
    const rect = event.currentTarget.getBoundingClientRect();
    const ratio = (event.clientX - rect.left) / Math.max(1, rect.width);
    if (ratio < 0.28) {
      if (prefs.direction === "rtl") goLogicalNext();
      else goLogicalPrev();
    } else if (ratio > 0.72) {
      if (prefs.direction === "rtl") goLogicalPrev();
      else goLogicalNext();
    } else {
      toggleHudIfUnpinned();
    }
  };

  const handleTouchStart = (event: ReactTouchEvent<HTMLDivElement>) => {
    if (event.touches.length === 2) {
      const dx = event.touches[0].clientX - event.touches[1].clientX;
      const dy = event.touches[0].clientY - event.touches[1].clientY;
      pinchStartRef.current = {
        dist: Math.hypot(dx, dy),
        zoom: prefs.zoomPercent,
      };
      touchStartRef.current = null;
      return;
    }
    if (event.touches.length === 1) {
      touchStartRef.current = {
        x: event.touches[0].clientX,
        y: event.touches[0].clientY,
        time: Date.now(),
      };
    }
  };

  const handleTouchMove = (event: ReactTouchEvent<HTMLDivElement>) => {
    if (event.touches.length === 2 && pinchStartRef.current) {
      const dx = event.touches[0].clientX - event.touches[1].clientX;
      const dy = event.touches[0].clientY - event.touches[1].clientY;
      const currentDist = Math.hypot(dx, dy);
      if (pinchStartRef.current.dist > 10) {
        const scale = currentDist / pinchStartRef.current.dist;
        applyPrefs({ zoomPercent: clampZoom(pinchStartRef.current.zoom * scale) }, false);
      }
    }
  };

  const handleTouchEnd = (event: ReactTouchEvent<HTMLDivElement>) => {
    if (pinchStartRef.current && event.touches.length < 2) {
      pinchStartRef.current = null;
      void onUpdatePrefs(prefs);
      return;
    }
    const start = touchStartRef.current;
    touchStartRef.current = null;
    if (!start || event.changedTouches.length === 0) return;

    const endTouch = event.changedTouches[0];
    const dx = endTouch.clientX - start.x;
    const dy = endTouch.clientY - start.y;
    const elapsed = Date.now() - start.time;

    // Horizontal swipe in paged modes
    if (
      prefs.viewMode !== "cascade" &&
      elapsed < 500 &&
      Math.abs(dx) > 54 &&
      Math.abs(dx) > Math.abs(dy) * 1.4
    ) {
      event.preventDefault();
      if (dx < 0) {
        if (prefs.direction === "rtl") goLogicalPrev();
        else goLogicalNext();
      } else {
        if (prefs.direction === "rtl") goLogicalNext();
        else goLogicalPrev();
      }
      return;
    }

    // Double-tap zoom when stationary
    if (elapsed < 260 && Math.hypot(dx, dy) < 16) {
      const now = Date.now();
      if (now - lastTapTimeRef.current < 280) {
        if (pendingTapTimerRef.current) {
          window.clearTimeout(pendingTapTimerRef.current);
          pendingTapTimerRef.current = null;
        }
        lastTapTimeRef.current = 0;
        applyPrefs({ zoomPercent: prefs.zoomPercent > 115 ? 100 : 175 });
        event.preventDefault();
      } else {
        lastTapTimeRef.current = now;
      }
    }
  };

  const isPagedMode = prefs.viewMode !== "cascade";
  const activeFitMode: PageFitMode = isPagedMode
    ? prefs.pagedFitMode ?? "fitHeight"
    : prefs.fitMode;
  const fitEntirePage = isPagedMode && activeFitMode === "fitHeight" && prefs.zoomPercent <= 100;

  const renderPageImage = (pageIndex: number) => {
    if (!session) return null;
    const nonce = failedPages[pageIndex] ?? 0;
    const src = source.pageUrl(session.sessionId, pageIndex);
    const pageSize = source.pageSize?.(session.sessionId);
    const cacheBustedSrc = nonce > 0 ? `${src}${src.includes("?") ? "&" : "?"}r=${nonce}` : src;

    const isCascade = prefs.viewMode === "cascade";
    const imgStyle = fitEntirePage
      ? {
          maxHeight: `${prefs.zoomPercent}%`,
          maxWidth: `${prefs.zoomPercent}%`,
          width: "auto",
          height: "auto",
        }
      : activeFitMode === "fitHeight"
        ? {
            maxHeight: isCascade
              ? `${(85 * prefs.zoomPercent) / 100}dvh`
              : `${(86 * prefs.zoomPercent) / 100}dvh`,
            width: "auto",
            maxWidth: prefs.zoomPercent > 100 ? "none" : "100%",
          }
        : activeFitMode === "original"
          ? {
              width: "auto",
              maxWidth: "none",
              transform: prefs.zoomPercent !== 100 ? `scale(${prefs.zoomPercent / 100})` : undefined,
              transformOrigin: "top center",
            }
          : {
              width: prefs.viewMode === "double" ? "100%" : `${prefs.zoomPercent}%`,
              maxWidth:
                prefs.viewMode === "double"
                  ? "100%"
                  : prefs.zoomPercent <= 100
                    ? isCascade
                      ? "920px"
                      : "960px"
                    : "none",
              height: "auto",
            };

    return (
      <div
        key={`${session.sessionId}-${pageIndex}`}
        ref={(el) => {
          pageRefs.current[pageIndex] = el;
        }}
        className={`flex justify-center ${
          fitEntirePage
            ? `h-full min-h-0 items-center ${prefs.viewMode === "double" ? "min-w-0 flex-1" : "w-full"}`
            : prefs.viewMode === "double"
              ? "min-w-0 flex-1 items-start my-auto"
              : "w-full items-start"
        }`}
      >
        <img
          src={cacheBustedSrc}
          width={pageSize?.width}
          height={pageSize?.height}
          alt={`${session.mangaTitle} - ${pageIndex + 1}`}
          loading={Math.abs(pageIndex - currentPage) <= 3 ? "eager" : "lazy"}
          decoding="async"
          referrerPolicy="no-referrer"
          draggable={false}
          style={imgStyle}
          onError={() => {
            if ((failedPages[pageIndex] ?? 0) < 2) {
              setFailedPages((prev) => ({
                ...prev,
                [pageIndex]: (prev[pageIndex] ?? 0) + 1,
              }));
            }
          }}
          className="block select-none object-contain"
        />
      </div>
    );
  };

  const canvasBgClass =
    appearance.reader_background !== "theme_default"
      ? getReaderBackgroundClass(appearance.reader_background)
      : prefs.darkCanvas
        ? "bg-[#0e0e0d] text-[#f8f7f1]"
        : "bg-[var(--paper)] text-[var(--ink)]";

  const showBars = hudVisible || prefs.pinToolbars || settingsOpen || isLoading || Boolean(loadError);

  return (
    <div
      className={`fixed inset-0 z-50 flex flex-col overflow-hidden ${canvasBgClass}`}
      role="dialog"
      aria-modal="true"
      aria-label={session?.mangaTitle ?? activeRequest.mangaTitle}
    >
      {/* Top Retractable Ink Bar */}
      <header
        style={{ paddingTop: "var(--safe-top)" }}
        className={`ink-bar z-20 shrink-0 border-b-2 border-[var(--ink)] transition-transform duration-150 ${
          showBars ? "translate-y-0" : "-translate-y-full pointer-events-none absolute top-0 left-0 right-0"
        }`}
      >
        <div className="mx-auto flex max-w-6xl items-center justify-between gap-2 px-3 py-2.5 sm:px-5">
          <div className="flex min-w-0 items-center gap-2.5">
            <button
              type="button"
              onClick={() => void handleClose()}
              className="inline-flex min-h-[44px] cursor-pointer touch-manipulation items-center gap-1.5 border-2 border-[var(--bar-fg)] bg-transparent px-3 py-1.5 text-xs font-bold uppercase tracking-[0.12em] text-[var(--bar-fg)] hover:bg-[var(--bar-fg)] hover:text-[var(--bar-bg)]"
            >
              <ArrowLeft className="h-4 w-4" aria-hidden="true" />
              <span className="hidden sm:inline">{t.backToSeries}</span>
            </button>

            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="font-display truncate text-sm text-[var(--bar-fg)] sm:text-base">
                  {session?.mangaTitle ?? activeRequest.mangaTitle}
                </h1>
                {session?.isOffline ? (
                  <span className="inline-flex items-center gap-1 border border-[var(--ok)] bg-[var(--ok)] px-2 py-0.5 font-mono-editorial text-[10px] font-bold uppercase text-white">
                    <HardDrive className="h-3 w-3" aria-hidden="true" />
                    {t.offlineBadge(session.offlineFormat)}
                  </span>
                ) : session ? (
                  <span className="border border-[var(--bar-accent)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold uppercase text-[var(--bar-accent)]">
                    {t.onlineBadge(session.providerLabel)}
                  </span>
                ) : null}
              </div>
              <p className="truncate font-mono-editorial text-[11px] text-[var(--bar-fg)]/75">
                {session
                  ? `${t.chapterLabel(session.chapterNumber)}${session.chapterTitle ? ` · ${session.chapterTitle}` : ""}`
                  : activeRequest.chapterTitle ?? ""}
              </p>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-1.5">
            <button
              type="button"
              onClick={() => setSettingsOpen((prev) => !prev)}
              aria-expanded={settingsOpen}
              aria-label={t.settingsBtn}
              className={`inline-flex min-h-[44px] cursor-pointer touch-manipulation items-center gap-1.5 border-2 px-3 py-1.5 text-xs font-bold ${
                settingsOpen
                  ? "border-[var(--red)] bg-[var(--red)] text-white"
                  : "border-[var(--bar-fg)] bg-transparent text-[var(--bar-fg)] hover:bg-[var(--bar-fg)] hover:text-[var(--bar-bg)]"
              }`}
            >
              <Settings2 className="h-4 w-4" aria-hidden="true" />
              <span className="hidden md:inline">{t.settingsBtn}</span>
            </button>

            <button
              type="button"
              onClick={() => void handleToggleFullscreen()}
              aria-label={t.fullscreenBtn}
              className="inline-flex min-h-[44px] cursor-pointer touch-manipulation items-center gap-1.5 border-2 border-[var(--bar-fg)] bg-transparent px-3 py-1.5 text-xs font-bold text-[var(--bar-fg)] hover:bg-[var(--bar-fg)] hover:text-[var(--bar-bg)]"
            >
              {isFullscreen ? (
                <Minimize2 className="h-4 w-4" aria-hidden="true" />
              ) : (
                <Maximize2 className="h-4 w-4" aria-hidden="true" />
              )}
            </button>
          </div>
        </div>

        {/* Settings Drawer */}
        {settingsOpen ? (
          <div className="border-t-2 border-[var(--bar-fg)]/25 bg-[var(--surface)] px-4 py-4 text-[var(--ink)]">
            <div className="mx-auto grid max-w-6xl gap-4 sm:grid-cols-2 lg:grid-cols-4">
              {/* View mode */}
              <div className="space-y-1.5">
                <p className="font-mono-editorial text-[11px] font-bold uppercase text-[var(--muted)]">
                  {t.modeLabel}
                </p>
                <div className="grid grid-cols-3 gap-1.5">
                  {(
                    [
                      ["cascade", t.modeCascade],
                      ["single", t.modeSingle],
                      ["double", t.modeDouble],
                    ] as Array<[ReaderViewMode, string]>
                  ).map(([mode, label]) => (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => applyPrefs({ viewMode: mode })}
                      className={`min-h-[44px] cursor-pointer border-2 border-[var(--ink)] px-2 py-1 text-xs font-bold ${
                        prefs.viewMode === mode
                          ? "bg-[var(--ink)] text-[var(--paper)]"
                          : "bg-[var(--paper)] text-[var(--ink)]"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Direction */}
              <div className="space-y-1.5">
                <p className="font-mono-editorial text-[11px] font-bold uppercase text-[var(--muted)]">
                  {t.directionLabel}
                </p>
                <div className="grid grid-cols-2 gap-1.5">
                  {(
                    [
                      ["rtl", t.dirRtl],
                      ["ltr", t.dirLtr],
                    ] as Array<[ReadingDirection, string]>
                  ).map(([dir, label]) => (
                    <button
                      key={dir}
                      type="button"
                      onClick={() => applyPrefs({ direction: dir })}
                      className={`min-h-[44px] cursor-pointer border-2 border-[var(--ink)] px-2 py-1 text-xs font-bold ${
                        prefs.direction === dir
                          ? "bg-[var(--ink)] text-[var(--paper)]"
                          : "bg-[var(--paper)] text-[var(--ink)]"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Fit & Zoom */}
              <div className="space-y-1.5">
                <p className="font-mono-editorial text-[11px] font-bold uppercase text-[var(--muted)]">
                  {t.fitLabel} · {prefs.zoomPercent}%
                </p>
                <div className="grid grid-cols-3 gap-1.5">
                  {(
                    [
                      ["fitWidth", t.fitWidth],
                      ["fitHeight", isPagedMode ? t.fitPage : t.fitHeight],
                      ["original", t.fitOriginal],
                    ] as Array<[PageFitMode, string]>
                  ).map(([fit, label]) => (
                    <button
                      key={fit}
                      type="button"
                      onClick={() =>
                        applyPrefs(isPagedMode ? { pagedFitMode: fit } : { fitMode: fit })
                      }
                      className={`min-h-[44px] cursor-pointer border-2 border-[var(--ink)] px-2 py-1 text-xs font-bold ${
                        activeFitMode === fit
                          ? "bg-[var(--ink)] text-[var(--paper)]"
                          : "bg-[var(--paper)] text-[var(--ink)]"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <div className="flex items-center gap-1.5 pt-1">
                  <button
                    type="button"
                    onClick={() => applyPrefs({ zoomPercent: prefs.zoomPercent - 15 })}
                    aria-label={t.zoomOut}
                    className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                  >
                    <ZoomOut className="h-4 w-4" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    onClick={() => applyPrefs({ zoomPercent: 100 })}
                    className="editorial-btn-secondary min-h-[44px] flex-1 px-2 py-1.5 font-mono-editorial text-xs"
                  >
                    {prefs.zoomPercent}% ({t.zoomReset})
                  </button>
                  <button
                    type="button"
                    onClick={() => applyPrefs({ zoomPercent: prefs.zoomPercent + 15 })}
                    aria-label={t.zoomIn}
                    className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                  >
                    <ZoomIn className="h-4 w-4" aria-hidden="true" />
                  </button>
                </div>
              </div>

              {/* Toggles */}
              <div className="flex flex-col gap-2 font-mono-editorial text-xs">
                <label className="flex min-h-[36px] cursor-pointer items-center justify-between gap-2 border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-bold">
                  <span>{t.darkCanvasLabel}</span>
                  <input
                    type="checkbox"
                    checked={prefs.darkCanvas}
                    onChange={(e) => applyPrefs({ darkCanvas: e.target.checked })}
                    className="h-4 w-4 accent-[var(--red)]"
                  />
                </label>

                <label className="flex min-h-[36px] cursor-pointer items-center justify-between gap-2 border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-bold">
                  <span>{t.pinHudLabel}</span>
                  <input
                    type="checkbox"
                    checked={prefs.pinToolbars}
                    onChange={(e) => applyPrefs({ pinToolbars: e.target.checked })}
                    className="h-4 w-4 accent-[var(--red)]"
                  />
                </label>

                {prefs.viewMode === "double" ? (
                  <label className="flex min-h-[36px] cursor-pointer items-center justify-between gap-2 border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-bold">
                    <span>{t.firstPageSingleLabel}</span>
                    <input
                      type="checkbox"
                      checked={prefs.firstPageSingle}
                      onChange={(e) => applyPrefs({ firstPageSingle: e.target.checked })}
                      className="h-4 w-4 accent-[var(--red)]"
                    />
                  </label>
                ) : null}

                <label className="flex min-h-[36px] flex-col gap-1 border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1.5 font-bold">
                  <span className="text-[10px] uppercase text-[var(--muted)]">{hito5Labels.readerSection}</span>
                  <select
                    value={appearance.reader_background}
                    onChange={(e) =>
                      void updateAppearance({ reader_background: e.target.value as ReaderBackground })
                    }
                    className="border border-[var(--ink)] bg-[var(--surface)] px-1.5 py-1 text-xs font-bold text-[var(--ink)]"
                  >
                    {readerBackgrounds.map((bg) => (
                      <option key={bg.id} value={bg.id}>
                        {language === "en" ? bg.name_en : bg.name_es}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </div>

            <div className="mx-auto mt-3 flex max-w-6xl flex-wrap items-center justify-between gap-2 border-t border-[var(--line)] pt-2 font-mono-editorial text-[11px] text-[var(--muted)]">
              <span className="hidden md:inline">{t.shortcutsHint}</span>
              <span className="md:hidden">{t.touchHint}</span>
              <button
                type="button"
                onClick={() => applyPrefs({ pinToolbars: !prefs.pinToolbars })}
                className="inline-flex items-center gap-1 font-bold text-[var(--ink)] hover:underline"
              >
                {prefs.pinToolbars ? (
                  <PinOff className="h-3.5 w-3.5" aria-hidden="true" />
                ) : (
                  <Pin className="h-3.5 w-3.5" aria-hidden="true" />
                )}
                <span>{t.pinHudLabel}</span>
              </button>
            </div>
          </div>
        ) : null}
      </header>

      {/* Main Reading Viewport */}
      <main
        ref={cascadeContainerRef}
        onScroll={prefs.viewMode === "cascade" ? handleCascadeScroll : undefined}
        onClick={!isLoading && !loadError ? handleViewportClick : undefined}
        onTouchStart={handleTouchStart}
        onTouchMove={handleTouchMove}
        onTouchEnd={handleTouchEnd}
        className="relative min-h-0 flex-1 touch-pan-x touch-pan-y overflow-auto"
      >
        {isLoading ? (
          <div className="flex h-full min-h-[280px] flex-col items-center justify-center gap-3 p-6 text-center">
            <RefreshCw className="h-7 w-7 animate-spin text-[var(--red)]" aria-hidden="true" />
            <p className="font-mono-editorial text-sm">{t.loading}</p>
          </div>
        ) : loadError ? (
          <div className="flex h-full min-h-[280px] items-center justify-center p-6">
            <div className="editorial-card max-w-md space-y-4 bg-[var(--paper)] p-6 text-[var(--ink)]">
              <div className="flex items-start gap-3">
                <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-[var(--red-text)]" aria-hidden="true" />
                <div>
                  <h2 className="font-display text-xl">{t.loadErrorTitle}</h2>
                  <p className="mt-1 font-mono-editorial text-xs text-[var(--muted)]">{loadError}</p>
                </div>
              </div>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => setReloadNonce((n) => n + 1)}
                  className="editorial-btn-primary text-xs"
                >
                  <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{t.retry}</span>
                </button>
                <button
                  type="button"
                  onClick={() => void handleClose()}
                  className="editorial-btn-secondary text-xs"
                >
                  <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" />
                  <span>{t.backToSeries}</span>
                </button>
              </div>
            </div>
          </div>
        ) : session && prefs.viewMode === "cascade" ? (
          <div
            style={{ gap: `${prefs.cascadeGapPx}px` }}
            className="mx-auto flex w-full flex-col items-center px-1 py-4 sm:px-4"
          >
            {Array.from({ length: session.pageCount }, (_, idx) => renderPageImage(idx))}

            {/* End-of-chapter footer */}
            <div
              onClick={(e) => e.stopPropagation()}
              className="editorial-card my-6 w-full max-w-xl bg-[var(--paper)] p-5 text-[var(--ink)]"
            >
              <h2 className="font-display text-xl">{t.endOfChapterTitle}</h2>
              <p className="mt-1 text-xs text-[var(--muted)]">{t.endOfChapterSubtitle}</p>
              <div className="mt-4 flex flex-wrap gap-2">
                <button
                  type="button"
                  disabled={!session.prevChapter}
                  onClick={() => void handleOpenNeighbor(session.prevChapter)}
                  className="editorial-btn-secondary min-h-[44px] text-xs disabled:opacity-40"
                >
                  <ChevronLeft className="h-4 w-4" aria-hidden="true" />
                  <span>{t.prevChapter}</span>
                </button>
                {session.nextChapter ? (
                  <button
                    type="button"
                    onClick={() => void handleOpenNeighbor(session.nextChapter)}
                    className="editorial-btn-primary min-h-[44px] text-xs"
                  >
                    <span>
                      {t.nextChapter} ({session.nextChapter.chapterNumber})
                    </span>
                    <ChevronRight className="h-4 w-4" aria-hidden="true" />
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => void handleClose()}
                    className="editorial-btn-primary min-h-[44px] text-xs"
                  >
                    <ArrowLeft className="h-4 w-4" aria-hidden="true" />
                    <span>{t.backToSeries}</span>
                  </button>
                )}
              </div>
            </div>
          </div>
        ) : session ? (
          <div
            className={`mx-auto flex w-full justify-center gap-2 ${
              fitEntirePage ? "h-full items-center overflow-hidden p-2 sm:p-3" : "min-h-full items-start p-2 sm:p-4"
            } ${
              prefs.viewMode === "double" && prefs.direction === "rtl" ? "flex-row-reverse" : "flex-row"
            }`}
          >
            {activeSpreadPages.map((pageIndex) => renderPageImage(pageIndex))}
          </div>
        ) : null}
      </main>

      {/* Bottom Retractable Progress Bar */}
      {session && !loadError ? (
        <footer
          style={{ paddingBottom: "var(--safe-bottom)" }}
          className={`ink-bar z-20 shrink-0 border-t-2 border-[var(--ink)] transition-transform duration-150 ${
            showBars ? "translate-y-0" : "translate-y-full pointer-events-none absolute right-0 bottom-0 left-0"
          }`}
        >
          <div className="mx-auto flex max-w-6xl flex-col gap-2 px-3 py-2.5 sm:flex-row sm:items-center sm:gap-4 sm:px-5">
            <div className="flex items-center justify-between gap-2 sm:shrink-0">
              <button
                type="button"
                disabled={!session.prevChapter}
                onClick={() => void handleOpenNeighbor(session.prevChapter)}
                className="inline-flex min-h-[44px] cursor-pointer touch-manipulation items-center gap-1 border border-[var(--bar-fg)]/60 px-2.5 py-1 text-xs font-bold text-[var(--bar-fg)] disabled:opacity-35"
              >
                <ChevronLeft className="h-4 w-4" aria-hidden="true" />
                <span className="hidden sm:inline">{t.prevChapter}</span>
              </button>

              <span className="font-mono-editorial text-xs font-bold text-[var(--bar-fg)]">
                {t.pageOf(currentPage + 1, session.pageCount)}
              </span>

              <button
                type="button"
                disabled={!session.nextChapter}
                onClick={() => void handleOpenNeighbor(session.nextChapter)}
                className="inline-flex min-h-[44px] cursor-pointer touch-manipulation items-center gap-1 border border-[var(--bar-fg)]/60 px-2.5 py-1 text-xs font-bold text-[var(--bar-fg)] disabled:opacity-35"
              >
                <span className="hidden sm:inline">{t.nextChapter}</span>
                <ChevronRight className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>

            <div className="flex flex-1 items-center gap-2">
              {prefs.viewMode !== "cascade" ? (
                <button
                  type="button"
                  onClick={prefs.direction === "rtl" ? goLogicalNext : goLogicalPrev}
                  aria-label={prefs.direction === "rtl" ? t.nextPage : t.prevPage}
                  className="inline-flex min-h-[44px] min-w-[44px] cursor-pointer touch-manipulation items-center justify-center border border-[var(--bar-fg)]/60 text-[var(--bar-fg)]"
                >
                  <ChevronLeft className="h-4 w-4" aria-hidden="true" />
                </button>
              ) : null}

              <input
                type="range"
                min={0}
                max={Math.max(0, session.pageCount - 1)}
                value={currentPage}
                onChange={(e) => jumpToPage(Number(e.target.value))}
                dir={prefs.direction === "rtl" && prefs.viewMode !== "cascade" ? "rtl" : "ltr"}
                aria-label={t.pageOf(currentPage + 1, session.pageCount)}
                className="h-6 flex-1 cursor-pointer accent-[var(--red)]"
              />

              {prefs.viewMode !== "cascade" ? (
                <button
                  type="button"
                  onClick={prefs.direction === "rtl" ? goLogicalPrev : goLogicalNext}
                  aria-label={prefs.direction === "rtl" ? t.prevPage : t.nextPage}
                  className="inline-flex min-h-[44px] min-w-[44px] cursor-pointer touch-manipulation items-center justify-center border border-[var(--bar-fg)]/60 text-[var(--bar-fg)]"
                >
                  <ChevronRight className="h-4 w-4" aria-hidden="true" />
                </button>
              ) : null}
            </div>
          </div>
        </footer>
      ) : null}
    </div>
  );
}
