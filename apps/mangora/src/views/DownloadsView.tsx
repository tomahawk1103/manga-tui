import { FolderOpen } from "lucide-react";
import type { InitialAppState } from "../types/mangora";
import { getTranslations } from "../lib/i18n";

interface DownloadsViewProps {
  appState: InitialAppState;
  onOpenDataFolder: () => Promise<void>;
}

export function DownloadsView({ appState, onOpenDataFolder }: DownloadsViewProps) {
  const t = getTranslations(appState.uiLanguage);

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow mb-3">{t.downloads.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.downloads.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{t.downloads.subtitle}</p>
      </div>

      {/* Format ribbon matching docs/index.html */}
      <div className="ink-bar flex flex-wrap items-center justify-between gap-3 border-2 border-[var(--ink)] px-5 py-3 text-sm">
        <b className="font-semibold text-[#ff8a76]">{t.downloads.ribbonLabel}</b>
        <div className="flex flex-wrap gap-4 font-bold">
          {t.downloads.formats.map((fmt) => (
            <span key={fmt}>{fmt}</span>
          ))}
        </div>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        <section className="editorial-card p-5">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [↓] {t.downloads.folderCardTitle}
          </div>
          <p className="mb-4 text-sm text-[var(--muted)]">{t.downloads.folderCardDesc}</p>
          <div className="mb-4 border border-[var(--ink)] bg-[var(--surface)] px-3 py-2 font-mono-editorial text-xs break-all">
            {appState.downloadsDir}
          </div>
          <button
            type="button"
            onClick={() => void onOpenDataFolder()}
            className="editorial-btn-primary text-xs"
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            <span>{t.downloads.openFolderBtn}</span>
          </button>
        </section>

        <section className="editorial-card-muted p-5">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [OFFLINE] {t.downloads.detectedOfflineTitle}
          </div>
          <div className="font-display mb-2 text-4xl">{appState.library.offlineSeriesCount}</div>
          <p className="mb-5 text-sm text-[var(--muted)]">
            {t.downloads.detectedOfflineCount(appState.library.offlineSeriesCount)}
          </p>

          <div className="border-t border-[var(--line)] pt-4">
            <div className="font-mono-editorial mb-1 text-xs font-bold">
              {t.downloads.queuePreviewTitle}
            </div>
            <p className="text-xs text-[var(--muted)]">{t.downloads.queuePreviewDesc}</p>
          </div>
        </section>
      </div>
    </div>
  );
}
