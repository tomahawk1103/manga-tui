import { ExternalLink, ShieldCheck, Zap } from "lucide-react";
import type { InitialAppState } from "../types/mangora";
import { getTranslations } from "../lib/i18n";

interface SupportViewProps {
  appState: InitialAppState;
  onOpenExternal: (url: string) => Promise<void>;
}

const DISCORD_URL = "https://discord.gg/SeqsWUCpVx";
const GITHUB_URL = "https://github.com/tomahawk1103/manga-tui";

export function SupportView({ appState, onOpenExternal }: SupportViewProps) {
  const t = getTranslations(appState.uiLanguage);

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow mb-3">{t.support.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.support.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{t.support.subtitle}</p>
      </div>

      <div className="grid gap-6 md:grid-cols-2">
        <article className="editorial-card p-6">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [DISCORD]
          </div>
          <h2 className="font-display mb-2 text-2xl">{t.support.discordTitle}</h2>
          <p className="mb-5 text-sm text-[var(--muted)]">{t.support.discordDesc}</p>
          <button
            type="button"
            onClick={() => void onOpenExternal(DISCORD_URL)}
            className="editorial-btn-primary text-sm"
          >
            <span>{t.support.discordBtn}</span>
            <ExternalLink className="h-4 w-4" aria-hidden="true" />
          </button>
        </article>

        <article className="editorial-card p-6">
          <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
            [MIT · OPEN SOURCE]
          </div>
          <h2 className="font-display mb-2 text-2xl">{t.support.githubTitle}</h2>
          <p className="mb-5 text-sm text-[var(--muted)]">{t.support.githubDesc}</p>
          <button
            type="button"
            onClick={() => void onOpenExternal(GITHUB_URL)}
            className="editorial-btn-secondary text-sm"
          >
            <span>{t.support.githubBtn}</span>
            <ExternalLink className="h-4 w-4" aria-hidden="true" />
          </button>
        </article>
      </div>

      <section className="editorial-card-muted p-6">
        <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
          <h2 className="font-display flex items-center gap-2 text-2xl">
            <Zap className="h-6 w-6 text-[var(--red-text)]" aria-hidden="true" />
            <span>{t.support.monsterTitle}</span>
          </h2>
          <span className="border border-[var(--ink)] bg-[var(--paper)] px-2.5 py-1 font-mono-editorial text-xs font-bold">
            {t.support.monsterBadge}
          </span>
        </div>
        <p className="max-w-3xl text-sm text-[var(--muted)]">{t.support.monsterDesc}</p>
      </section>

      <section className="editorial-callout p-5">
        <div className="mb-1 flex items-center gap-2 text-base font-bold">
          <ShieldCheck className="h-5 w-5 text-[var(--red-text)]" aria-hidden="true" />
          <span>{t.support.privacyTitle}</span>
        </div>
        <p className="text-sm text-[var(--muted)]">{t.support.privacyDesc}</p>
      </section>
    </div>
  );
}
