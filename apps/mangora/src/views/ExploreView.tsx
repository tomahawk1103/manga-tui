import { Check, SlidersHorizontal } from "lucide-react";
import type { InitialAppState, NavSection } from "../types/mangora";
import { getGenreLabel, getTranslations } from "../lib/i18n";

interface ExploreViewProps {
  appState: InitialAppState;
  isSaving: boolean;
  onSelectProvider: (providerId: string) => Promise<void>;
  onNavigate: (section: NavSection) => void;
}

export function ExploreView({
  appState,
  isSaving,
  onSelectProvider,
  onNavigate,
}: ExploreViewProps) {
  const t = getTranslations(appState.uiLanguage);

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow mb-3">{t.explore.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.explore.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{t.explore.subtitle}</p>
      </div>

      {/* For you genres */}
      <section className="editorial-callout p-5">
        <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
          <h2 className="font-display text-2xl">{t.explore.forYouTitle}</h2>
          <button
            type="button"
            onClick={() => onNavigate("settings")}
            className="editorial-btn-secondary px-3 py-1.5 text-xs"
          >
            <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{t.explore.editGenresBtn}</span>
          </button>
        </div>
        <p className="mb-3 text-sm text-[var(--muted)]">{t.explore.forYouDesc}</p>
        {appState.favoriteGenres.length > 0 ? (
          <div className="flex flex-wrap gap-2">
            {appState.favoriteGenres.map((genreId) => (
              <span
                key={genreId}
                className="border-2 border-[var(--ink)] bg-[var(--paper)] px-3 py-1 font-mono-editorial text-xs font-bold shadow-[2px_2px_0_var(--shadow-color)]"
              >
                {getGenreLabel(genreId, appState.uiLanguage)}
              </span>
            ))}
          </div>
        ) : (
          <p className="font-mono-editorial text-xs text-[var(--muted)]">{t.explore.noGenresHint}</p>
        )}
      </section>

      {/* Provider selector */}
      <section className="space-y-4">
        <div>
          <h2 className="font-display text-2xl">{t.explore.providerSelectorTitle}</h2>
          <p className="text-sm text-[var(--muted)]">{t.explore.providerSelectorDesc}</p>
        </div>

        <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
          {appState.availableProviders.map((prov) => {
            const isActive = appState.activeProvider === prov.id;
            const isRecommended = appState.recommendedProvider === prov.id;
            return (
              <button
                key={prov.id}
                type="button"
                disabled={isSaving}
                onClick={() => void onSelectProvider(prov.id)}
                className={`cursor-pointer border-2 border-[var(--ink)] p-4 text-left transition-colors ${
                  isActive
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[4px_4px_0_var(--red)]"
                    : "bg-[var(--paper)] text-[var(--ink)] shadow-[3px_3px_0_var(--shadow-color)] hover:bg-[var(--surface)]"
                }`}
              >
                <div className="mb-2 flex items-center justify-between gap-2">
                  <span className="font-display text-lg">{prov.badge}</span>
                  <div className="flex items-center gap-1.5">
                    {isRecommended && (
                      <span className="border border-[var(--ink)] bg-[var(--red)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold text-white">
                        {t.explore.recommendedTag}
                      </span>
                    )}
                    {isActive && (
                      <span className="inline-flex items-center gap-1 border border-current px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold">
                        <Check className="h-3 w-3" /> {t.explore.activeTag}
                      </span>
                    )}
                  </div>
                </div>
                <p
                  className={`text-xs ${
                    isActive ? "text-[#d5d4cb]" : "text-[var(--muted)]"
                  }`}
                >
                  {appState.uiLanguage === "en" ? prov.descriptionEn : prov.descriptionEs}
                </p>
              </button>
            );
          })}
        </div>
      </section>

      <section className="editorial-card-muted p-5">
        <div className="font-mono-editorial mb-1 text-xs font-bold uppercase text-[var(--red-text)]">
          [HITO 2] {t.explore.h2PreviewTitle}
        </div>
        <p className="text-sm text-[var(--muted)]">{t.explore.h2PreviewDesc}</p>
      </section>
    </div>
  );
}
