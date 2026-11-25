import {
  BookOpen,
  Compass,
  Download,
  ExternalLink,
  Globe,
  HeartHandshake,
  MessageSquarePlus,
  Moon,
  Settings,
  Sun,
} from "lucide-react";
import type { NavSection, ThemeMode, UiLanguage } from "../types/mangora";
import { getTranslations } from "../lib/i18n";

interface SidebarProps {
  activeSection: NavSection;
  onSelectSection: (section: NavSection) => void;
  themeMode: ThemeMode;
  onToggleTheme: () => void;
  onOpenFeedback: () => void;
  onOpenWebsite: () => void;
  uiLanguage: UiLanguage;
  appVersion: string;
  activeDownloadCount?: number;
}

export function Sidebar({
  activeSection,
  onSelectSection,
  themeMode,
  onToggleTheme,
  onOpenFeedback,
  onOpenWebsite,
  uiLanguage,
  appVersion,
  activeDownloadCount = 0,
}: SidebarProps) {
  const t = getTranslations(uiLanguage);

  const primaryItems: Array<{ id: NavSection; label: string; icon: typeof BookOpen }> = [
    { id: "home", label: t.nav.home, icon: BookOpen },
    { id: "explore", label: t.nav.explore, icon: Compass },
    { id: "downloads", label: t.nav.downloads, icon: Download },
    { id: "settings", label: t.nav.settings, icon: Settings },
  ];

  return (
    <aside
      aria-label="Mangora"
      className="hidden w-64 shrink-0 flex-col justify-between border-r-2 border-[var(--ink)] bg-[var(--surface)] md:flex"
    >
      <div>
        <div className="border-b-2 border-[var(--ink)] bg-[var(--paper)] px-5 py-4">
          <div className="flex items-center gap-3">
            <span className="mark" aria-hidden="true">
              M!
            </span>
            <div className="min-w-0">
              <div className="font-display text-2xl tracking-tight text-[var(--ink)]">Mangora</div>
              <div className="font-mono-editorial text-[11px] uppercase tracking-wider text-[var(--muted)]">
                {t.brandSubtitle} · v{appVersion}
              </div>
            </div>
          </div>
        </div>

        <nav className="flex flex-col gap-2 p-4" aria-label="Main navigation">
          {primaryItems.map((item) => {
            const Icon = item.icon;
            const isActive = activeSection === item.id;
            return (
              <button
                key={item.id}
                type="button"
                aria-current={isActive ? "page" : undefined}
                onClick={() => onSelectSection(item.id)}
                className={`flex w-full cursor-pointer items-center gap-3 border-2 border-[var(--ink)] px-3.5 py-2.5 text-left text-sm font-bold transition-colors ${
                  isActive
                    ? "bg-[var(--ink)] text-[var(--paper)] shadow-[3px_3px_0_var(--red)]"
                    : "bg-[var(--paper)] text-[var(--ink)] hover:bg-[var(--ink)] hover:text-[var(--paper)]"
                }`}
              >
                <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.id === "downloads" && activeDownloadCount > 0 ? (
                  <span className="border border-current bg-[var(--red)] px-1.5 py-0.5 font-mono-editorial text-[10px] font-bold text-white">
                    {activeDownloadCount}
                  </span>
                ) : null}
              </button>
            );
          })}
        </nav>
      </div>

      <div className="flex flex-col gap-2.5 border-t-2 border-[var(--ink)] bg-[var(--paper)] p-4">
        <button
          type="button"
          onClick={onOpenWebsite}
          className="flex w-full cursor-pointer items-center gap-2.5 border-2 border-[var(--ink)] bg-[var(--surface)] px-3.5 py-2 text-left text-xs font-bold text-[var(--ink)] transition-colors hover:bg-[var(--ink)] hover:text-[var(--paper)]"
        >
          <Globe className="h-4 w-4 shrink-0 text-[var(--red-text)]" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate">{t.support.sidebarWebsiteBtn}</span>
          <ExternalLink className="h-3.5 w-3.5 shrink-0 opacity-75" aria-hidden="true" />
        </button>

        <button
          type="button"
          onClick={onOpenFeedback}
          className="flex w-full cursor-pointer items-center gap-2.5 border-2 border-[var(--ink)] bg-[var(--surface)] px-3.5 py-2 text-left text-xs font-bold text-[var(--ink)] transition-colors hover:bg-[var(--ink)] hover:text-[var(--paper)]"
        >
          <MessageSquarePlus className="h-4 w-4 shrink-0 text-[var(--red-text)]" aria-hidden="true" />
          <span className="truncate">{t.feedback.sidebarBtn}</span>
        </button>

        <button
          type="button"
          aria-current={activeSection === "support" ? "page" : undefined}
          onClick={() => onSelectSection("support")}
          className={`flex w-full cursor-pointer items-center gap-2.5 border-2 border-[var(--ink)] px-3.5 py-2.5 text-left text-sm font-bold transition-colors ${
            activeSection === "support"
              ? "bg-[var(--red)] text-white shadow-[3px_3px_0_var(--shadow-color)]"
              : "bg-[var(--surface)] text-[var(--ink)] hover:bg-[var(--red)] hover:text-white"
          }`}
        >
          <HeartHandshake className="h-4 w-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{t.nav.support}</span>
        </button>

        <button
          type="button"
          onClick={onToggleTheme}
          className="flex w-full cursor-pointer items-center justify-between border border-[var(--ink)] bg-[var(--surface)] px-3 py-2 text-xs font-bold text-[var(--ink)] transition-colors hover:bg-[var(--ink)] hover:text-[var(--paper)]"
        >
          <span className="font-mono-editorial uppercase tracking-wide">
            {themeMode === "paper" ? t.themePaper : t.themeInk}
          </span>
          {themeMode === "paper" ? (
            <Moon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          ) : (
            <Sun className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          )}
        </button>
      </div>
    </aside>
  );
}
