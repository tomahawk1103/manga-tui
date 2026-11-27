import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, Check, Moon, Sparkles, Sun, X } from "lucide-react";
import type { InitialAppState, NavSection, ProviderOption, ThemeMode, UiLanguage } from "../types/mangora";
import { GENRE_OPTIONS, getTranslations } from "../lib/i18n";
import { isTauriRuntime } from "../lib/tauri";

interface OnboardingWizardProps {
  appState: InitialAppState;
  isReplay: boolean;
  isSaving: boolean;
  errorMessage: string | null;
  onComplete: (payload: {
    uiLanguage: UiLanguage;
    favoriteGenres: string[];
    provider: string;
    themeMode: ThemeMode;
  }) => Promise<void>;
  onCancelReplay: () => void;
  onJumpToSection?: (section: NavSection) => void;
}

function recommendedForLang(lang: UiLanguage, availableProviders: ProviderOption[]): string {
  if (lang === "es") {
    if (availableProviders.some((p) => p.id === "inmanga")) return "inmanga";
    if (availableProviders.some((p) => p.id === "mangaoni")) return "mangaoni";
    return availableProviders[0]?.id ?? "mangaoni";
  }
  if (availableProviders.some((p) => p.id === "weebcentral")) return "weebcentral";
  if (availableProviders.some((p) => p.id === "mangadex")) return "mangadex";
  return availableProviders[0]?.id ?? "weebcentral";
}

export function OnboardingWizard({
  appState,
  isReplay,
  isSaving,
  errorMessage,
  onComplete,
  onCancelReplay,
}: OnboardingWizardProps) {
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [selectedLang, setSelectedLang] = useState<UiLanguage>(appState.uiLanguage);
  const [selectedTheme, setSelectedTheme] = useState<ThemeMode>(appState.themeMode);
  const [selectedGenres, setSelectedGenres] = useState<string[]>(appState.favoriteGenres);
  const [selectedProvider, setSelectedProvider] = useState<string>(
    isReplay
      ? appState.activeProvider
      : recommendedForLang(appState.uiLanguage, appState.availableProviders),
  );
  const [providerPickerOpen, setProviderPickerOpen] = useState(false);
  const [showTourAlongsideDetection, setShowTourAlongsideDetection] = useState(false);

  const isWeb = !isTauriRuntime();
  const t = getTranslations(selectedLang);
  const recommendedId = recommendedForLang(selectedLang, appState.availableProviders);
  const esRecommendedId = recommendedForLang("es", appState.availableProviders);
  const enRecommendedId = recommendedForLang("en", appState.availableProviders);
  const esRecommendedName =
    appState.availableProviders.find((p) => p.id === esRecommendedId)?.name ?? "MangaOni";
  const enRecommendedName =
    appState.availableProviders.find((p) => p.id === enRecommendedId)?.name ?? "WeebCentral";
  const activeProviderObj = appState.availableProviders.find((p) => p.id === selectedProvider);
  const tourCards = isWeb ? t.onboarding.webTourCards : t.onboarding.tourCards;
  const step3DetectedBadge = isWeb
    ? t.onboarding.webStep3DetectedBadge
    : t.onboarding.step3DetectedBadge;
  const step3DetectedSubtitle = isWeb
    ? t.onboarding.webStep3DetectedSubtitle
    : t.onboarding.step3DetectedSubtitle;

  const handleSelectLanguage = (lang: UiLanguage) => {
    setSelectedLang(lang);
    setSelectedProvider(recommendedForLang(lang, appState.availableProviders));
  };

  const toggleGenre = (genreId: string) => {
    setSelectedGenres((prev) =>
      prev.includes(genreId) ? prev.filter((item) => item !== genreId) : [...prev, genreId],
    );
  };

  const handleFinish = async () => {
    await onComplete({
      uiLanguage: selectedLang,
      favoriteGenres: selectedGenres,
      provider: selectedProvider,
      themeMode: selectedTheme,
    });
  };

  const nextTheme: ThemeMode = selectedTheme === "paper" ? "ink" : "paper";

  useEffect(() => {
    const themeColorMeta = document.querySelector('meta[name="theme-color"]');
    if (themeColorMeta) {
      themeColorMeta.setAttribute("content", selectedTheme === "ink" ? "#141413" : "#f8f7f1");
    }
  }, [selectedTheme]);

  return (
    <div
      data-theme={selectedTheme}
      lang={selectedLang}
      className="min-h-dvh overflow-x-hidden bg-[var(--paper)] px-4 pt-[max(1.5rem,var(--safe-top))] pb-[max(1.5rem,var(--safe-bottom))] text-[var(--ink)] md:px-8 md:py-10"
    >
      <div className="mx-auto max-w-4xl">
        {/* Editorial header bar */}
        <div className="mb-6 flex flex-wrap items-center justify-between gap-3 border-b-2 border-[var(--ink)] pb-4">
          <div className="flex items-center gap-3">
            <span className="mark" aria-hidden="true">
              M!
            </span>
            <div>
              <div className="font-display text-2xl">Mangora</div>
              <div className="font-mono-editorial text-xs text-[var(--muted)]">
                {t.onboarding.stepOf(step, 3)}
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setSelectedTheme(nextTheme)}
              disabled={isSaving}
              className="editorial-btn-secondary px-3 py-1.5 text-xs"
            >
              {selectedTheme === "paper" ? (
                <Moon className="h-3.5 w-3.5" aria-hidden="true" />
              ) : (
                <Sun className="h-3.5 w-3.5" aria-hidden="true" />
              )}
              <span>{selectedTheme === "paper" ? t.themePaper : t.themeInk}</span>
            </button>

            {isReplay && (
              <button
                type="button"
                onClick={onCancelReplay}
                disabled={isSaving}
                className="editorial-btn-secondary px-3 py-1.5 text-xs"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" />
                <span>{t.onboarding.cancelReplay}</span>
              </button>
            )}
          </div>
        </div>

        {errorMessage && <p role="alert" className="editorial-callout mb-4 p-3">{errorMessage}</p>}

        {/* Step progress strip */}
        <div className="mb-6 grid grid-cols-3 gap-2 font-mono-editorial text-xs font-bold">
          {[1, 2, 3].map((num) => {
            const reached = step >= num;
            const current = step === num;
            return (
              <div
                key={num}
                className={`border-2 border-[var(--ink)] px-3 py-2 transition-colors ${
                  current
                    ? "bg-[var(--red)] text-white shadow-[3px_3px_0_var(--shadow-color)]"
                    : reached
                      ? "bg-[var(--ink)] text-[var(--paper)]"
                      : "bg-[var(--surface)] text-[var(--muted)]"
                }`}
              >
                0{num}
              </div>
            );
          })}
        </div>

        {/* Main wizard card with screentone accent */}
        <div className="relative isolate">
          <div
            aria-hidden="true"
            className="screentone-bg pointer-events-none absolute -right-2 -bottom-2 top-3 left-6 -z-10 border border-[var(--ink)]"
          />

          <section className="editorial-card overflow-hidden">
            <div className="ink-bar flex items-center justify-between border-b-2 border-[var(--ink)] px-5 py-2.5 text-xs">
              <span>&gt;_ mangora / onboarding-0{step}</span>
              <span className="ink-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
            </div>

            <div className="p-6 md:p-8">
              <p className="eyebrow mb-3">{t.onboarding.eyebrow}</p>

              {/* STEP 1: LANGUAGE */}
              {step === 1 && (
                <div>
                  <h1 className="font-display mb-3 text-3xl md:text-5xl">{t.onboarding.step1Title}</h1>
                  <p className="mb-7 max-w-2xl text-base text-[var(--muted)]">{t.onboarding.step1Subtitle}</p>

                  <div className="grid gap-4 md:grid-cols-2">
                    <button
                      type="button"
                      onClick={() => handleSelectLanguage("es")}
                      disabled={isSaving}
                      className={`cursor-pointer border-2 border-[var(--ink)] p-5 text-left transition-colors ${
                        selectedLang === "es"
                          ? "bg-[var(--ink)] text-[var(--paper)] shadow-[5px_5px_0_var(--red)]"
                          : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                      }`}
                    >
                      <div className="mb-2 flex items-center justify-between">
                        <span className="font-mono-editorial text-xs font-bold uppercase tracking-wider">
                          ES · ESPAÑOL
                        </span>
                        {selectedLang === "es" && (
                          <span className="inline-flex items-center gap-1 bg-[var(--red)] px-2 py-0.5 font-mono-editorial text-[11px] font-bold text-white">
                            <Check className="h-3 w-3" /> OK
                          </span>
                        )}
                      </div>
                      <div className="font-display mb-1.5 text-2xl">{t.onboarding.esCardTitle}</div>
                      <p
                        className={`text-sm ${
                          selectedLang === "es" ? "text-[var(--paper)]" : "text-[var(--muted)]"
                        }`}
                      >
                        {t.onboarding.esCardDesc(esRecommendedName)}
                      </p>
                    </button>

                    <button
                      type="button"
                      onClick={() => handleSelectLanguage("en")}
                      disabled={isSaving}
                      className={`cursor-pointer border-2 border-[var(--ink)] p-5 text-left transition-colors ${
                        selectedLang === "en"
                          ? "bg-[var(--ink)] text-[var(--paper)] shadow-[5px_5px_0_var(--red)]"
                          : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--paper)]"
                      }`}
                    >
                      <div className="mb-2 flex items-center justify-between">
                        <span className="font-mono-editorial text-xs font-bold uppercase tracking-wider">
                          EN · ENGLISH
                        </span>
                        {selectedLang === "en" && (
                          <span className="inline-flex items-center gap-1 bg-[var(--red)] px-2 py-0.5 font-mono-editorial text-[11px] font-bold text-white">
                            <Check className="h-3 w-3" /> OK
                          </span>
                        )}
                      </div>
                      <div className="font-display mb-1.5 text-2xl">{t.onboarding.enCardTitle}</div>
                      <p
                        className={`text-sm ${
                          selectedLang === "en" ? "text-[var(--paper)]" : "text-[var(--muted)]"
                        }`}
                      >
                        {t.onboarding.enCardDesc(enRecommendedName)}
                      </p>
                    </button>
                  </div>
                </div>
              )}

              {/* STEP 2: GENRES & RECOMMENDED PROVIDER */}
              {step === 2 && (
                <div>
                  <h1 className="font-display mb-3 text-3xl md:text-4xl">{t.onboarding.step2Title}</h1>
                  <p className="mb-6 max-w-2xl text-base text-[var(--muted)]">{t.onboarding.step2Subtitle}</p>

                  <div className="mb-7">
                    <div className="mb-1 text-sm font-bold">{t.onboarding.genresLabel}</div>
                    <p className="mb-3 text-xs text-[var(--muted)]">{t.onboarding.genresHelper}</p>
                    <div className="flex flex-wrap gap-2.5">
                      {GENRE_OPTIONS.map((genre) => {
                        const selected = selectedGenres.includes(genre.id);
                        return (
                          <button
                            key={genre.id}
                            type="button"
                            data-selected={selected}
                            onClick={() => toggleGenre(genre.id)}
                            title={selectedLang === "en" ? genre.hintEn : genre.hintEs}
                            className="editorial-chip text-sm"
                          >
                            {selected && <Check className="h-3.5 w-3.5 text-[var(--red-text)]" />}
                            <span>{selectedLang === "en" ? genre.labelEn : genre.labelEs}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  <div className="editorial-callout p-4 md:p-5">
                    <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                      <div>
                        <div className="font-mono-editorial text-xs font-bold uppercase text-[var(--red-text)]">
                          {t.onboarding.recommendedTitle}
                        </div>
                        <div className="font-display mt-1 text-2xl">
                          {activeProviderObj?.badge ?? selectedProvider}
                        </div>
                      </div>
                      {selectedProvider === recommendedId && (
                        <span className="border border-[var(--ink)] bg-[var(--ink)] px-2.5 py-1 font-mono-editorial text-xs font-bold text-[var(--paper)]">
                          ★ {t.onboarding.recommendedTag}
                        </span>
                      )}
                    </div>
                    <p className="mb-3 text-sm text-[var(--muted)]">
                      {selectedLang === "en"
                        ? activeProviderObj?.descriptionEn
                        : activeProviderObj?.descriptionEs}{" "}
                      {t.onboarding.recommendedNote}
                    </p>

                    <button
                      type="button"
                      onClick={() => {
                        if (providerPickerOpen) {
                          setSelectedProvider(recommendedId);
                        }
                        setProviderPickerOpen((v) => !v);
                      }}
                      className="cursor-pointer border border-[var(--ink)] bg-[var(--paper)] px-3 py-1.5 font-mono-editorial text-xs font-bold text-[var(--ink)] hover:bg-[var(--ink)] hover:text-[var(--paper)]"
                    >
                      {providerPickerOpen
                        ? t.onboarding.hideProviderToggle
                        : t.onboarding.changeProviderToggle}
                    </button>

                    {providerPickerOpen && (
                      <div className="mt-4 grid gap-2 sm:grid-cols-2">
                        {appState.availableProviders.map((prov) => {
                          const isSelected = selectedProvider === prov.id;
                          return (
                            <button
                              key={prov.id}
                              type="button"
                              onClick={() => setSelectedProvider(prov.id)}
                              className={`cursor-pointer border-2 border-[var(--ink)] p-3 text-left transition-colors ${
                                isSelected
                                  ? "bg-[var(--ink)] text-[var(--paper)]"
                                  : "bg-[var(--paper)] text-[var(--ink)] hover:bg-[var(--surface)]"
                              }`}
                            >
                              <div className="flex items-center justify-between gap-2 text-sm font-bold">
                                <span>{prov.badge}</span>
                                {prov.id === recommendedId && (
                                  <span className="bg-[var(--red)] px-1.5 py-0.5 font-mono-editorial text-[10px] text-white">
                                    {t.onboarding.recommendedTag}
                                  </span>
                                )}
                              </div>
                              <div
                                className={`mt-1 text-xs ${
                                  isSelected ? "text-[var(--paper)]" : "text-[var(--muted)]"
                                }`}
                              >
                                {selectedLang === "en" ? prov.descriptionEn : prov.descriptionEs}
                              </div>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* STEP 3: LIBRARY DETECTION OR MINI TOUR */}
              {step === 3 && (
                <div>
                  {appState.library.hasExistingData ? (
                    <div>
                      <div className="mb-3 inline-flex items-center gap-2 border border-[var(--ink)] bg-[var(--ink)] px-3 py-1 font-mono-editorial text-xs font-bold text-[var(--paper)]">
                        <Sparkles className="h-3.5 w-3.5 text-[#ff8a76]" aria-hidden="true" />
                        <span>{step3DetectedBadge}</span>
                      </div>
                      <h1 className="font-display mb-3 text-3xl md:text-4xl">
                        {t.onboarding.step3DetectedTitle}
                      </h1>
                      <p className="mb-6 max-w-2xl text-base text-[var(--muted)]">
                        {step3DetectedSubtitle}
                      </p>

                      <div className="mb-5 grid grid-cols-2 gap-3 sm:grid-cols-4">
                        <div className="editorial-card-muted p-3.5">
                          <div className="font-display text-2xl">{appState.library.libraryMangas}</div>
                          <div className="font-mono-editorial text-xs text-[var(--muted)]">
                            {t.onboarding.statsSeries}
                          </div>
                        </div>
                        <div className="editorial-card-muted p-3.5">
                          <div className="font-display text-2xl">{appState.library.chaptersRead}</div>
                          <div className="font-mono-editorial text-xs text-[var(--muted)]">
                            {t.onboarding.statsChapters}
                          </div>
                        </div>
                        <div className="editorial-card-muted p-3.5">
                          <div className="font-display text-2xl">{appState.library.favorites}</div>
                          <div className="font-mono-editorial text-xs text-[var(--muted)]">
                            {t.onboarding.statsFavorites}
                          </div>
                        </div>
                        <div className="editorial-card-muted p-3.5">
                          <div className="font-display text-2xl">
                            {appState.library.offlineSeriesCount}
                          </div>
                          <div className="font-mono-editorial text-xs text-[var(--muted)]">
                            {t.onboarding.statsOffline}
                          </div>
                        </div>
                      </div>

                      {appState.library.lastReadTitle && (
                        <div className="editorial-callout mb-5 px-4 py-3 text-sm">
                          <span className="font-mono-editorial font-bold">{t.onboarding.lastReadPrefix}</span>{" "}
                          <span className="font-bold">{appState.library.lastReadTitle}</span>
                        </div>
                      )}

                      <button
                        type="button"
                        onClick={() => setShowTourAlongsideDetection((v) => !v)}
                        className="editorial-btn-secondary mb-4 px-3 py-2 text-xs"
                      >
                        {showTourAlongsideDetection
                          ? t.onboarding.step3HideTourAlso
                          : t.onboarding.step3ShowTourAlso}
                      </button>

                      {showTourAlongsideDetection && (
                        <div className="grid gap-3 border-t-2 border-[var(--ink)] pt-4 sm:grid-cols-2">
                          {tourCards.map((card) => (
                            <div key={card.section} className="editorial-box-thin p-4">
                              <div className="font-mono-editorial mb-1 text-base font-bold text-[var(--red-text)]">
                                {card.glyph}
                              </div>
                              <div className="font-display mb-1 text-lg">{card.title}</div>
                              <p className="text-xs text-[var(--muted)]">{card.description}</p>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  ) : (
                    <div>
                      <h1 className="font-display mb-3 text-3xl md:text-4xl">
                        {t.onboarding.step3TourTitle}
                      </h1>
                      <p className="mb-6 max-w-2xl text-base text-[var(--muted)]">
                        {t.onboarding.step3TourSubtitle}
                      </p>

                      <div className="grid gap-4 sm:grid-cols-2">
                        {tourCards.map((card) => (
                          <article key={card.section} className="editorial-card-muted p-4">
                            <span className="font-mono-editorial mb-2 block text-xl font-bold text-[var(--red-text)]">
                              {card.glyph}
                            </span>
                            <h2 className="font-display mb-1.5 text-xl">{card.title}</h2>
                            <p className="text-sm text-[var(--muted)]">{card.description}</p>
                          </article>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* Wizard Footer Actions */}
              <div className="mt-8 flex flex-wrap items-center justify-between gap-3 border-t-2 border-[var(--ink)] pt-5">
                <div>
                  {step > 1 && (
                    <button
                      type="button"
                      disabled={isSaving}
                      onClick={() => setStep((prev) => (prev === 3 ? 2 : 1))}
                      className="editorial-btn-secondary text-sm"
                    >
                      <ArrowLeft className="h-4 w-4" aria-hidden="true" />
                      <span>{t.onboarding.back}</span>
                    </button>
                  )}
                </div>

                <div className="flex items-center gap-3">
                  {step < 3 ? (
                    <button
                      type="button"
                      disabled={isSaving}
                      onClick={() => setStep((prev) => (prev === 1 ? 2 : 3))}
                      className="editorial-btn-primary text-sm"
                    >
                      <span>{t.onboarding.next}</span>
                      <ArrowRight className="h-4 w-4" aria-hidden="true" />
                    </button>
                  ) : (
                    <button
                      type="button"
                      disabled={isSaving}
                      onClick={() => void handleFinish()}
                      className="editorial-btn-primary text-sm"
                    >
                      <Check className="h-4 w-4" aria-hidden="true" />
                      <span>{isSaving ? t.onboarding.saving : t.onboarding.finish}</span>
                    </button>
                  )}
                </div>
              </div>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
