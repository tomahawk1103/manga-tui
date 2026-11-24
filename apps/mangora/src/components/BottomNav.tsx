import { BookOpen, Compass, Download, HeartHandshake, Settings } from "lucide-react";
import { getTranslations } from "../lib/i18n";
import type { NavSection, UiLanguage } from "../types/mangora";

interface BottomNavProps {
  activeSection: NavSection;
  onSelectSection: (section: NavSection) => void;
  uiLanguage: UiLanguage;
  activeDownloadCount?: number;
}

export function BottomNav({
  activeSection,
  onSelectSection,
  uiLanguage,
  activeDownloadCount = 0,
}: BottomNavProps) {
  const t = getTranslations(uiLanguage);

  const items: Array<{ id: NavSection; label: string; icon: typeof BookOpen }> = [
    { id: "home", label: t.navMobile.home, icon: BookOpen },
    { id: "explore", label: t.navMobile.explore, icon: Compass },
    { id: "downloads", label: t.navMobile.downloads, icon: Download },
    { id: "settings", label: t.navMobile.settings, icon: Settings },
    { id: "support", label: t.navMobile.support, icon: HeartHandshake },
  ];

  return (
    <nav
      aria-label="Mobile navigation"
      className="safe-bottom-nav grid shrink-0 grid-cols-5 border-t-2 border-[var(--ink)] bg-[var(--surface)] px-1 pt-1 md:hidden"
    >
      {items.map((item) => {
        const Icon = item.icon;
        const isActive = activeSection === item.id;
        const isSupport = item.id === "support";
        return (
          <button
            key={item.id}
            type="button"
            aria-current={isActive ? "page" : undefined}
            onClick={() => onSelectSection(item.id)}
            className={`flex min-h-[54px] cursor-pointer touch-manipulation flex-col items-center justify-center gap-1 border-t-2 px-1 py-1.5 text-[11px] font-bold transition-colors [-webkit-tap-highlight-color:transparent] ${
              isActive
                ? isSupport
                  ? "border-t-[var(--ink)] bg-[var(--red)] text-white"
                  : "border-t-[var(--red)] bg-[var(--ink)] text-[var(--paper)]"
                : "border-t-transparent bg-transparent text-[var(--ink)] hover:bg-[var(--paper)]"
            }`}
          >
            <div className="relative">
              <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
              {item.id === "downloads" && activeDownloadCount > 0 ? (
                <span className="absolute -top-1.5 -right-2.5 min-w-[16px] bg-[var(--red)] px-1 text-center font-mono-editorial text-[9px] font-bold text-white">
                  {activeDownloadCount}
                </span>
              ) : null}
            </div>
            <span className="max-w-full truncate font-mono-editorial text-[10px] uppercase tracking-tight">
              {item.label}
            </span>
          </button>
        );
      })}
    </nav>
  );
}
