import { Moon, Sun } from "lucide-react";
import type { NavSection, ProviderOption, ThemeMode, UiLanguage } from "../types/mangora";
import { getTranslations } from "../lib/i18n";

interface TopBarProps {
  activeSection: NavSection;
  uiLanguage: UiLanguage;
  themeMode: ThemeMode;
  activeProvider: string;
  availableProviders: ProviderOption[];
  onToggleTheme: () => void;
  onChangeLanguage: (lang: UiLanguage) => void;
  onGoHome?: () => void;
}

const SECTION_PATH: Record<NavSection, string> = {
  home: "biblioteca",
  explore: "explorar",
  downloads: "descargas",
  settings: "ajustes",
  support: "comunidad",
};

export function TopBar({
  activeSection,
  uiLanguage,
  themeMode,
  activeProvider,
  availableProviders,
  onToggleTheme,
  onChangeLanguage,
  onGoHome,
}: TopBarProps) {
  const t = getTranslations(uiLanguage);
  const currentProvider = availableProviders.find((p) => p.id === activeProvider);

  return (
    <header className="ink-bar safe-top-bar flex shrink-0 items-center justify-between gap-2 border-b-2 border-[var(--ink)] px-3 pb-2.5 text-xs sm:gap-4 sm:px-6">
      <div className="flex min-w-0 items-center gap-2 sm:gap-3">
        <button
          type="button"
          onClick={onGoHome}
          aria-label={`Mangora — ${t.nav.home}`}
          className="flex min-w-0 cursor-pointer touch-manipulation items-center gap-2 text-left transition-opacity hover:opacity-85"
        >
          <span
            className="grid h-6 w-6 shrink-0 place-items-center border border-[#f8f7f1] bg-[var(--red)] font-mono-editorial text-[11px] font-bold tracking-tighter text-white md:hidden"
            aria-hidden="true"
          >
            M!
          </span>
          <span className="ink-dots hidden shrink-0 md:flex" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <span className="truncate font-bold tracking-tight">
            &gt;_ mangora / {SECTION_PATH[activeSection]}
          </span>
        </button>
        <span className="hidden border border-[#55554f] px-2 py-0.5 text-[11px] text-[#ff8a76] sm:inline-block">
          {t.topBar.providerPrefix}: {currentProvider?.badge ?? activeProvider}
        </span>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <div
          role="group"
          aria-label={t.topBar.languageLabel}
          className="inline-flex border border-[#55554f]"
        >
          {(["es", "en"] as const).map((lang) => {
            const active = uiLanguage === lang;
            return (
              <button
                key={lang}
                type="button"
                onClick={() => onChangeLanguage(lang)}
                className={`min-h-[36px] cursor-pointer touch-manipulation px-2.5 py-1 text-[11px] font-bold uppercase transition-colors ${
                  active ? "bg-[var(--red)] text-white" : "bg-transparent text-[#e4e3db] hover:bg-[#2d2d2a]"
                }`}
              >
                {lang}
              </button>
            );
          })}
        </div>

        <button
          type="button"
          onClick={onToggleTheme}
          title={t.themeToggleLabel}
          className="inline-flex min-h-[36px] cursor-pointer touch-manipulation items-center gap-1.5 border border-[#55554f] px-2.5 py-1 text-[11px] font-bold text-[#f8f7f1] transition-colors hover:border-[var(--red)] hover:bg-[var(--red)]"
        >
          {themeMode === "paper" ? (
            <Moon className="h-3.5 w-3.5" aria-hidden="true" />
          ) : (
            <Sun className="h-3.5 w-3.5" aria-hidden="true" />
          )}
          <span className="hidden md:inline">
            {themeMode === "paper" ? t.themePaper : t.themeInk}
          </span>
        </button>
      </div>
    </header>
  );
}
