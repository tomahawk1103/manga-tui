import {
  CheckCircle2,
  FolderOpen,
  HardDrive,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { getTranslations } from "../lib/i18n";
import { isTauriRuntime, openExternalUrl } from "../lib/tauri";
import type {
  DownloadFormat,
  DownloadQueueAction,
  DownloadQueueSnapshot,
  InitialAppState,
  OfflineSeriesItem,
  SeriesSelection,
} from "../types/mangora";

const FORMAT_OPTIONS: DownloadFormat[] = ["cbz", "pdf", "epub", "raw"];

interface DownloadsViewProps {
  appState: InitialAppState;
  queue: DownloadQueueSnapshot | null;
  offlineSeries: OfflineSeriesItem[];
  offlineLoading: boolean;
  isMutating: boolean;
  onOpenDownloadsFolder: () => Promise<void>;
  onChangeDownloadFormat: (format: DownloadFormat) => Promise<void>;
  onControlQueue: (action: DownloadQueueAction, taskId?: string) => Promise<void>;
  onRefreshOffline: () => Promise<void>;
  onOpenSeries: (series: SeriesSelection) => void;
}

function formatSpeed(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec <= 1) return "0 KB/s";
  if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)} B/s`;
  const kb = bytesPerSec / 1024;
  if (kb < 1024) return `${kb.toFixed(0)} KB/s`;
  return `${(kb / 1024).toFixed(1)} MB/s`;
}

export function DownloadsView({
  appState,
  queue,
  offlineSeries,
  offlineLoading,
  isMutating,
  onOpenDownloadsFolder,
  onChangeDownloadFormat,
  onControlQueue,
  onRefreshOffline,
  onOpenSeries,
}: DownloadsViewProps) {
  const t = getTranslations(appState.uiLanguage);
  const d = t.downloads;
  const desktop = isTauriRuntime();

  const activeFormat: DownloadFormat = queue?.activeFormat ?? appState.downloadFormat ?? "cbz";
  const items = queue?.items ?? [];
  const hasActiveTasks =
    (queue?.queuedCount ?? 0) + (queue?.downloadingCount ?? 0) + (queue?.pausedCount ?? 0) > 0;
  const hasTerminalTasks = items.some(
    (item) => item.status === "completed" || item.status === "failed" || item.status === "cancelled",
  );

  return (
    <div className="space-y-8 pb-8">
      <div>
        <p className="eyebrow mb-3">{d.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{d.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{d.subtitle}</p>
      </div>

      {!desktop && (
        <section className="editorial-callout flex flex-col gap-3 p-5 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0 space-y-1">
            <h2 className="font-display text-xl">{d.webBannerTitle}</h2>
            <p className="text-xs text-[var(--muted)]">{d.webBannerDesc}</p>
          </div>
          <button
            type="button"
            onClick={() => void openExternalUrl("https://tomahawk1103.github.io/manga-tui/#descargas")}
            className="editorial-btn-primary shrink-0 text-xs"
          >
            {d.webDownloadAppBtn}
          </button>
        </section>
      )}

      {/* Format ribbon + Format selector */}
      <section className="editorial-card overflow-hidden">
        <div className="ink-bar flex flex-wrap items-center justify-between gap-3 border-b-2 border-[var(--ink)] px-5 py-3 text-sm">
          <b className="font-semibold text-[#ff8a76]">{d.ribbonLabel}</b>
          <div className="flex flex-wrap gap-4 font-bold">
            {d.formats.map((fmt) => (
              <span key={fmt}>{fmt}</span>
            ))}
          </div>
        </div>

        <div className="space-y-3 p-5">
          <p className="font-mono-editorial text-xs font-bold uppercase text-[var(--ink)]">
            {d.formatSelectorLabel}
          </p>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            {FORMAT_OPTIONS.map((fmt) => {
              const selected = activeFormat === fmt;
              return (
                <button
                  key={fmt}
                  type="button"
                  disabled={isMutating}
                  aria-pressed={selected}
                  onClick={() => void onChangeDownloadFormat(fmt)}
                  className={`${
                    selected ? "editorial-btn-primary" : "editorial-btn-secondary"
                  } min-h-[44px] justify-center font-mono-editorial text-xs uppercase disabled:opacity-50`}
                >
                  {fmt.toUpperCase()}
                </button>
              );
            })}
          </div>
          <p className="text-xs text-[var(--muted)]">{d.formatDescriptions[activeFormat]}</p>
        </div>
      </section>

      {/* Folder & Local count summary */}
      <div className="grid gap-6 md:grid-cols-2">
        <section className="editorial-card p-5">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [↓] {d.folderCardTitle}
          </div>
          <p className="mb-4 text-sm text-[var(--muted)]">{d.folderCardDesc}</p>
          <div className="mb-4 border border-[var(--ink)] bg-[var(--surface)] px-3 py-2 font-mono-editorial text-xs break-all">
            {queue?.downloadsDir ?? appState.downloadsDir}
          </div>
          {appState.canOpenDataFolder ? (
            <>
              <button
                type="button"
                disabled={!desktop}
                onClick={() => void onOpenDownloadsFolder()}
                className="editorial-btn-primary min-h-[44px] text-xs"
              >
                <FolderOpen className="h-4 w-4" aria-hidden="true" />
                <span>{d.openFolderBtn}</span>
              </button>
              {!desktop && <p className="mt-2 text-xs text-[var(--muted)]">{t.folderDesktopOnly}</p>}
            </>
          ) : (
            <p className="editorial-callout p-3 text-xs text-[var(--muted)]">{d.mobileStorageNote}</p>
          )}
        </section>

        <section className="editorial-card-muted p-5">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [OFFLINE] {d.detectedOfflineTitle}
          </div>
          <div className="font-display mb-2 text-4xl">
            {Math.max(appState.library.offlineSeriesCount, offlineSeries.length)}
          </div>
          <p className="text-sm text-[var(--muted)]">
            {d.detectedOfflineCount(Math.max(appState.library.offlineSeriesCount, offlineSeries.length))}
          </p>
        </section>
      </div>

      {/* Live Download Queue */}
      <section className="editorial-card space-y-4 p-5 sm:p-6">
        <div className="flex flex-col gap-3 border-b-2 border-[var(--ink)] pb-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="font-display text-2xl">{d.queueTitle}</h2>
            {queue && items.length > 0 ? (
              <p className="font-mono-editorial mt-1 text-xs text-[var(--muted)]">
                {d.overallProgressLabel}: {Math.round(queue.overallProgressPercent)}% ·{" "}
                {queue.isDownloading ? formatSpeed(queue.speedBytesPerSec) : `${queue.completedCount}/${items.length}`}
              </p>
            ) : null}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {hasActiveTasks ? (
              <button
                type="button"
                onClick={() => void onControlQueue(queue?.isPaused ? "resumeAll" : "pauseAll")}
                className="editorial-btn-secondary min-h-[44px] px-3 py-2 text-xs"
              >
                {queue?.isPaused ? (
                  <>
                    <Play className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{d.resumeAllBtn}</span>
                  </>
                ) : (
                  <>
                    <Pause className="h-3.5 w-3.5" aria-hidden="true" />
                    <span>{d.pauseAllBtn}</span>
                  </>
                )}
              </button>
            ) : null}

            {(queue?.failedCount ?? 0) > 0 ? (
              <button
                type="button"
                onClick={() => void onControlQueue("retryAllFailed")}
                className="editorial-btn-secondary min-h-[44px] px-3 py-2 text-xs"
              >
                <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{d.retryFailedBtn}</span>
              </button>
            ) : null}

            {hasTerminalTasks ? (
              <button
                type="button"
                onClick={() => void onControlQueue("clearFinished")}
                className="editorial-btn-secondary min-h-[44px] px-3 py-2 text-xs"
              >
                <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{d.clearFinishedBtn}</span>
              </button>
            ) : null}

            {hasActiveTasks ? (
              <button
                type="button"
                onClick={() => void onControlQueue("cancelAll")}
                className="editorial-btn-secondary min-h-[44px] px-3 py-2 text-xs text-[var(--red-text)]"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{d.cancelAllBtn}</span>
              </button>
            ) : null}
          </div>
        </div>

        {queue && items.length > 0 ? (
          <div className="h-3 w-full overflow-hidden border-2 border-[var(--ink)] bg-[var(--surface)]">
            <div
              style={{ width: `${Math.max(2, Math.min(100, queue.overallProgressPercent))}%` }}
              className="h-full bg-[var(--red)] transition-all duration-200"
            />
          </div>
        ) : null}

        {items.length === 0 ? (
          <p className="py-4 text-sm text-[var(--muted)]">{d.queueEmpty}</p>
        ) : (
          <ul className="divide-y divide-[var(--line)]">
            {items.map((item) => {
              const statusLabel = d.statusNames[item.status] ?? item.status;
              const isPaused = item.status === "paused";
              const isActive = item.status === "queued" || item.status === "downloading";
              const isRetryable = item.status === "failed" || item.status === "cancelled";

              return (
                <li
                  key={item.taskId}
                  className="flex flex-col gap-3 py-3.5 sm:flex-row sm:items-center sm:justify-between"
                >
                  <div className="min-w-0 flex-1 space-y-1.5">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-display text-base text-[var(--ink)]">
                        {item.mangaTitle}
                      </span>
                      <span className="font-mono-editorial text-xs font-bold text-[var(--muted)]">
                        Cap. {item.chapterNumber}
                        {item.chapterTitle ? ` · ${item.chapterTitle}` : ""}
                      </span>
                    </div>

                    <div className="flex flex-wrap items-center gap-2 font-mono-editorial text-[11px]">
                      <span
                        className={`inline-flex items-center gap-1 border px-2 py-0.5 font-bold uppercase ${
                          item.status === "completed"
                            ? "border-[var(--ok)] bg-[var(--ok)] text-white"
                            : item.status === "failed"
                              ? "border-[var(--red)] bg-[var(--red)] text-white"
                              : "border-[var(--ink)] bg-[var(--surface)] text-[var(--ink)]"
                        }`}
                      >
                        {item.status === "completed" ? (
                          <CheckCircle2 className="h-3 w-3" aria-hidden="true" />
                        ) : null}
                        {statusLabel}
                      </span>

                      <span className="border border-[var(--ink)] bg-[var(--paper)] px-2 py-0.5 font-bold uppercase">
                        {item.providerLabel} · {item.format}
                      </span>

                      {item.totalPages > 0 ? (
                        <span className="text-[var(--muted)]">
                          {d.pagesProgress(item.pagesDownloaded, item.totalPages)}
                        </span>
                      ) : null}

                      {item.status === "downloading" ? (
                        <span className="font-bold text-[var(--red-text)]">
                          {Math.round(item.progressPercent)}% · {formatSpeed(item.speedBytesPerSec)}
                        </span>
                      ) : null}
                    </div>

                    {isActive || item.status === "packaging" ? (
                      <div className="h-2 w-full max-w-md overflow-hidden border border-[var(--ink)] bg-[var(--surface)]">
                        <div
                          style={{ width: `${Math.max(4, Math.min(100, item.progressPercent))}%` }}
                          className="h-full bg-[var(--ink)] transition-all duration-150"
                        />
                      </div>
                    ) : null}

                    {item.errorMessage ? (
                      <p className="font-mono-editorial text-xs text-[var(--red-text)]">
                        {item.errorMessage}
                      </p>
                    ) : null}
                  </div>

                  <div className="flex flex-wrap items-center gap-2 sm:shrink-0">
                    {isActive ? (
                      <button
                        type="button"
                        onClick={() => void onControlQueue("pauseTask", item.taskId)}
                        className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                      >
                        <Pause className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{d.pauseTaskBtn}</span>
                      </button>
                    ) : null}

                    {isPaused ? (
                      <button
                        type="button"
                        onClick={() => void onControlQueue("resumeTask", item.taskId)}
                        className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                      >
                        <Play className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{d.resumeTaskBtn}</span>
                      </button>
                    ) : null}

                    {isRetryable ? (
                      <button
                        type="button"
                        onClick={() => void onControlQueue("retryTask", item.taskId)}
                        className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                      >
                        <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{d.retryTaskBtn}</span>
                      </button>
                    ) : null}

                    {item.status !== "completed" && item.status !== "cancelled" ? (
                      <button
                        type="button"
                        onClick={() => void onControlQueue("cancelTask", item.taskId)}
                        className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                      >
                        <X className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{d.cancelTaskBtn}</span>
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={() => void onControlQueue("removeTask", item.taskId)}
                        className="editorial-btn-secondary min-h-[44px] px-3 py-1.5 text-xs"
                      >
                        <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                        <span>{d.removeTaskBtn}</span>
                      </button>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {/* Offline Downloaded Series Shelf */}
      <section className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="font-display text-2xl">{d.offlineShelfTitle}</h2>
            <p className="text-sm text-[var(--muted)]">{d.offlineShelfSubtitle}</p>
          </div>
          <button
            type="button"
            disabled={offlineLoading}
            onClick={() => void onRefreshOffline()}
            className="editorial-btn-secondary min-h-[44px] px-3.5 py-2 text-xs"
          >
            <RefreshCw className={`h-4 w-4 ${offlineLoading ? "animate-spin" : ""}`} aria-hidden="true" />
            <span>{t.library.refreshBtn}</span>
          </button>
        </div>

        {offlineSeries.length === 0 ? (
          <div className="editorial-card-muted p-6 text-sm text-[var(--muted)]">
            {d.offlineShelfEmpty}
          </div>
        ) : (
          <ul className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {offlineSeries.map((series) => {
              const hasCover = Boolean(series.coverUrl && /^https?:\/\//i.test(series.coverUrl));
              const openSelection = () =>
                onOpenSeries({
                  mangaId: series.mangaId,
                  provider: series.provider,
                  title: series.title,
                  coverUrl: series.coverUrl,
                });

              return (
                <li
                  key={`${series.provider}::${series.mangaId}`}
                  className="editorial-card flex flex-col justify-between overflow-hidden"
                >
                  <button
                    type="button"
                    onClick={openSelection}
                    className="group flex w-full flex-1 cursor-pointer touch-manipulation gap-3.5 p-3.5 text-left hover:bg-[var(--surface)]/50"
                  >
                    <div className="h-24 w-16 shrink-0 overflow-hidden border-2 border-[var(--ink)] bg-[var(--surface)]">
                      {hasCover ? (
                        <img
                          src={series.coverUrl ?? ""}
                          alt=""
                          loading="lazy"
                          className="h-full w-full object-cover"
                          onError={(e) => {
                            e.currentTarget.hidden = true;
                          }}
                        />
                      ) : (
                        <div className="screentone-bg flex h-full w-full items-center justify-center p-1 text-center">
                          <span className="border border-[var(--ink)] bg-[var(--paper)] px-1.5 py-0.5 font-mono-editorial text-xs font-bold">
                            {series.title.slice(0, 2).toUpperCase()}
                          </span>
                        </div>
                      )}
                    </div>

                    <div className="min-w-0 flex-1 space-y-1.5">
                      <div className="flex flex-wrap items-center gap-1.5">
                        <span className="border border-[var(--ink)] bg-[var(--surface)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold">
                          {series.providerLabel}
                        </span>
                        {series.formats.map((fmt) => (
                          <span
                            key={fmt}
                            className="inline-flex items-center gap-1 border border-[var(--ink)] bg-[var(--ink)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold text-[var(--paper)]"
                          >
                            <HardDrive className="h-2.5 w-2.5" aria-hidden="true" />
                            {fmt}
                          </span>
                        ))}
                      </div>
                      <h3 className="font-display line-clamp-2 text-base leading-tight group-hover:text-[var(--red-text)]">
                        {series.title}
                      </h3>
                      <p className="font-mono-editorial text-[11px] text-[var(--muted)]">
                        {d.offlineChaptersRead(series.readCount, series.chapterCount)} ·{" "}
                        {series.totalSizeLabel}
                      </p>
                    </div>
                  </button>

                  <div className="flex items-center justify-end border-t-2 border-[var(--ink)] bg-[var(--surface)] px-3.5 py-2">
                    <button
                      type="button"
                      onClick={openSelection}
                      className="editorial-btn-primary min-h-[44px] px-3.5 py-1.5 text-xs"
                    >
                      {d.openOfflineSeriesBtn}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </div>
  );
}
