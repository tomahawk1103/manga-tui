import { useEffect, useRef, useState } from "react";
import { Check, FolderOpen, Moon, RotateCcw, Sun } from "lucide-react";
import type { InitialAppState, ThemeMode, UiLanguage } from "../types/mangora";
import { GENRE_OPTIONS, getTranslations } from "../lib/i18n";
import { isTauriRuntime } from "../lib/tauri";

interface SettingsViewProps {
  appState: InitialAppState;
  focusGenres: boolean;
  isSaving: boolean;
  onChangeLanguage: (lang: UiLanguage) => Promise<void>;
  onChangeTheme: (mode: ThemeMode) => Promise<void>;
  onChangeProvider: (provider: string) => Promise<void>;
  onSaveGenres: (genres: string[]) => Promise<void>;
  onRelaunchTour: () => void;
  onOpenDataFolder: () => Promise<void>;
}

export function SettingsView({
  appState,
  focusGenres,
  isSaving,
  onChangeLanguage,
  onChangeTheme,
  onChangeProvider,
  onSaveGenres,
  onRelaunchTour,
  onOpenDataFolder,
}: SettingsViewProps) {
  const t = getTranslations(appState.uiLanguage);
  const desktop = isTauriRuntime();
  const genresHeading = useRef<HTMLHeadingElement>(null);
  const [draftGenres, setDraftGenres] = useState<string[]>(appState.favoriteGenres);

  useEffect(() => {
    setDraftGenres(appState.favoriteGenres);
  }, [appState.favoriteGenres]);

  useEffect(() => {
    if (focusGenres) {
      genresHeading.current?.scrollIntoView({ block: "start" });
      genresHeading.current?.focus({ preventScroll: true });
    }
  }, [focusGenres]);

  const toggleDraftGenre = (id: string) => {
    setDraftGenres((prev) => (prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]));
  };

  const genresDirty =
    draftGenres.length !== appState.favoriteGenres.length ||
    draftGenres.some((g, idx) => g !== appState.favoriteGenres[idx]);

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow mb-3">{t.settings.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.settings.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{t.settings.subtitle}</p>
      </div>

      {/* Appearance & Language */}
      <section className="editorial-card p-6">
        <h2 className="font-display mb-4 text-2xl">{t.settings.appearanceTitle}</h2>

        <div className="grid gap-6 md:grid-cols-2">
          <div>
            <div className="mb-2 text-sm font-bold">{t.settings.languageLabel}</div>
            <div className="grid grid-cols-2 gap-3">
              {(
                [
                  { id: "es", label: "Español (ES)" },
                  { id: "en", label: "English (EN)" },
                ] as const
              ).map((option) => {
                const active = appState.uiLanguage === option.id;
                return (
                  <button
                    key={option.id}
                    type="button"
                    disabled={isSaving}
                    onClick={() => void onChangeLanguage(option.id)}
                    className={`cursor-pointer border-2 border-[var(--ink)] px-4 py-3 text-left text-sm font-bold ${
                      active
                        ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                        : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                    }`}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>
          </div>

          <div>
            <div className="mb-2 text-sm font-bold">{t.settings.themeLabel}</div>
            <div className="grid grid-cols-2 gap-3">
              <button
                type="button"
                disabled={isSaving}
                onClick={() => void onChangeTheme("paper")}
                className={`cursor-pointer border-2 border-[var(--ink)] p-3 text-left ${
                  appState.themeMode === "paper"
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                    : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                }`}
              >
                <div className="mb-1 flex items-center gap-2 text-sm font-bold">
                  <Sun className="h-4 w-4" aria-hidden="true" />
                  <span>{t.settings.paperName}</span>
                </div>
                <div
                  className={`text-xs ${
                    appState.themeMode === "paper" ? "text-[var(--paper)]" : "text-[var(--muted)]"
                  }`}
                >
                  {t.settings.paperDesc}
                </div>
              </button>

              <button
                type="button"
                disabled={isSaving}
                onClick={() => void onChangeTheme("ink")}
                className={`cursor-pointer border-2 border-[var(--ink)] p-3 text-left ${
                  appState.themeMode === "ink"
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                    : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                }`}
              >
                <div className="mb-1 flex items-center gap-2 text-sm font-bold">
                  <Moon className="h-4 w-4" aria-hidden="true" />
                  <span>{t.settings.inkName}</span>
                </div>
                <div
                  className={`text-xs ${
                    appState.themeMode === "ink" ? "text-[var(--paper)]" : "text-[var(--muted)]"
                  }`}
                >
                  {t.settings.inkDesc}
                </div>
              </button>
            </div>
          </div>
        </div>
      </section>

      {/* Provider & Genres */}
      <div className="grid gap-6 lg:grid-cols-2">
        <section className="editorial-card-muted p-6">
          <h2 className="font-display mb-1 text-2xl">{t.settings.providerTitle}</h2>
          <p className="mb-4 text-sm text-[var(--muted)]">{t.settings.providerDesc}</p>
          <div className="grid gap-2 sm:grid-cols-2">
            {appState.availableProviders.map((prov) => {
              const active = appState.activeProvider === prov.id;
              return (
                <button
                  key={prov.id}
                  type="button"
                  disabled={isSaving}
                  onClick={() => void onChangeProvider(prov.id)}
                  className={`flex cursor-pointer items-center justify-between border-2 border-[var(--ink)] px-3 py-2.5 text-left text-xs font-bold ${
                    active
                      ? "bg-[var(--ink)] text-[var(--paper)]"
                      : "bg-[var(--paper)] text-[var(--ink)] hover:bg-[var(--surface)]"
                  }`}
                >
                  <span>{prov.badge}</span>
                  {active && <Check className="h-3.5 w-3.5 text-[var(--paper)]" />}
                </button>
              );
            })}
          </div>
        </section>

        <section className="editorial-card-muted p-6">
          <h2 ref={genresHeading} tabIndex={-1} className="font-display mb-1 scroll-mt-6 text-2xl">
            {t.settings.genresTitle}
          </h2>
          <p className="mb-4 text-sm text-[var(--muted)]">{t.settings.genresDesc}</p>
          <div className="mb-4 flex flex-wrap gap-2">
            {GENRE_OPTIONS.map((genre) => {
              const selected = draftGenres.includes(genre.id);
              return (
                <button
                  key={genre.id}
                  type="button"
                  data-selected={selected}
                  onClick={() => toggleDraftGenre(genre.id)}
                  className="editorial-chip px-3 py-1.5 text-xs"
                >
                  {selected && <Check className="h-3 w-3 text-[var(--red-text)]" />}
                  <span>{appState.uiLanguage === "en" ? genre.labelEn : genre.labelEs}</span>
                </button>
              );
            })}
          </div>
          <button
            type="button"
            disabled={isSaving || !genresDirty}
            onClick={() => void onSaveGenres(draftGenres)}
            className="editorial-btn-primary text-xs"
          >
            <span>{t.settings.saveGenresBtn}</span>
          </button>
        </section>
      </div>

      {/* Shared Storage & Tour */}
      <div className="grid gap-6 lg:grid-cols-[1.3fr_0.7fr]">
        <section className="editorial-box-thin p-6">
          <h2 className="font-display mb-4 text-2xl">{t.settings.storageTitle}</h2>
          <dl className="space-y-2.5 text-xs">
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">
                {t.settings.storageModeLabel}
              </dt>
              <dd className="font-mono-editorial font-bold">{appState.storageModeLabel}</dd>
            </div>
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">{t.settings.dataDirLabel}</dt>
              <dd className="font-mono-editorial break-all">{appState.dataDir}</dd>
            </div>
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">
                {t.settings.configDirLabel}
              </dt>
              <dd className="font-mono-editorial break-all">{appState.configDir}</dd>
            </div>
            <div>
              <dt className="font-mono-editorial text-[var(--muted)]">
                {t.settings.downloadsDirLabel}
              </dt>
              <dd className="font-mono-editorial break-all">{appState.downloadsDir}</dd>
            </div>
          </dl>

          <button
            type="button"
            disabled={!desktop}
            onClick={() => void onOpenDataFolder()}
            className="editorial-btn-secondary mt-5 text-xs"
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            <span>{t.settings.openFolderBtn}</span>
          </button>
          {!desktop && <p className="mt-2 text-xs text-[var(--muted)]">{t.folderDesktopOnly}</p>}
        </section>

        <section className="editorial-callout flex flex-col justify-between p-6">
          <div>
            <h2 className="font-display mb-2 text-2xl">{t.settings.tourTitle}</h2>
            <p className="mb-5 text-sm text-[var(--muted)]">{t.settings.tourDesc}</p>
          </div>
          <button
            type="button"
            onClick={onRelaunchTour}
            className="editorial-btn-primary text-xs"
          >
            <RotateCcw className="h-4 w-4" aria-hidden="true" />
            <span>{t.settings.relaunchTourBtn}</span>
          </button>
        </section>
      </div>
    </div>
  );
}
