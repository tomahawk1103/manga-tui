import { ExternalLink, Globe, HeartHandshake, MessageSquarePlus, ShieldCheck, Zap } from "lucide-react";
import type { InitialAppState, SupportConfigDto } from "../types/mangora";
import { getTranslations } from "../lib/i18n";

interface SupportViewProps {
  appState: InitialAppState;
  supportConfig: SupportConfigDto;
  onOpenExternal: (url: string) => Promise<void>;
  onOpenFeedback: () => void;
}

export function SupportView({
  appState,
  supportConfig,
  onOpenExternal,
  onOpenFeedback,
}: SupportViewProps) {
  const t = getTranslations(appState.uiLanguage);

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow mb-3">{t.support.eyebrow}</p>
        <h1 className="font-display mb-2 text-4xl sm:text-5xl">{t.support.title}</h1>
        <p className="max-w-2xl text-base text-[var(--muted)]">{t.support.subtitle}</p>
      </div>

      {/* Hero Support Card: Buy me a Monster */}
      <section className="editorial-card p-6">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 className="font-display flex items-center gap-2 text-2xl sm:text-3xl">
            <Zap className="h-6 w-6 text-[var(--red-text)]" aria-hidden="true" />
            <span>{t.support.monsterTitle}</span>
          </h2>
          <span className="border border-[var(--ink)] bg-[var(--surface)] px-2.5 py-1 font-mono-editorial text-xs font-bold">
            {t.support.monsterBadge}
          </span>
        </div>
        <p className="mb-4 max-w-3xl text-sm text-[var(--muted)]">{t.support.monsterDesc}</p>

        {!supportConfig.kofiUrl && (
          <p className="editorial-box-thin mb-4 p-3 text-xs text-[var(--muted)]">
            {t.support.monsterNoKofiNote}
          </p>
        )}

        <div className="flex flex-wrap gap-3">
          {supportConfig.kofiUrl ? (
            <>
              <button
                type="button"
                onClick={() => void onOpenExternal(supportConfig.kofiUrl!)}
                className="editorial-btn-primary text-sm"
              >
                <Zap className="h-4 w-4" aria-hidden="true" />
                <span>{t.support.monsterKofiBtn}</span>
                <ExternalLink className="h-4 w-4" aria-hidden="true" />
              </button>
              <button
                type="button"
                onClick={() => void onOpenExternal(supportConfig.sponsorsUrl)}
                className="editorial-btn-secondary text-sm"
              >
                <span>{t.support.monsterSponsorsBtn}</span>
                <ExternalLink className="h-4 w-4" aria-hidden="true" />
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() => void onOpenExternal(supportConfig.sponsorsUrl)}
              className="editorial-btn-primary text-sm"
            >
              <Zap className="h-4 w-4" aria-hidden="true" />
              <span>{t.support.monsterSponsorsBtn}</span>
              <ExternalLink className="h-4 w-4" aria-hidden="true" />
            </button>
          )}
        </div>
      </section>

      {/* Community & Mailbox grid */}
      <div className="grid gap-6 md:grid-cols-2">
        <article className="editorial-card-muted flex flex-col justify-between p-6">
          <div>
            <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
              {t.support.websiteTag}
            </div>
            <h2 className="font-display mb-2 text-2xl">{t.support.websiteTitle}</h2>
            <p className="mb-5 text-sm text-[var(--muted)]">{t.support.websiteDesc}</p>
          </div>
          <button
            type="button"
            onClick={() => void onOpenExternal(supportConfig.websiteUrl)}
            className="editorial-btn-primary text-xs"
          >
            <Globe className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{t.support.websiteBtn}</span>
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </article>

        <article className="editorial-card-muted flex flex-col justify-between p-6">
          <div>
            <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
              [DISCORD]
            </div>
            <h2 className="font-display mb-2 text-2xl">{t.support.discordTitle}</h2>
            <p className="mb-5 text-sm text-[var(--muted)]">{t.support.discordDesc}</p>
          </div>
          <button
            type="button"
            onClick={() => void onOpenExternal(supportConfig.discordUrl)}
            className="editorial-btn-primary text-xs"
          >
            <span>{t.support.discordBtn}</span>
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </article>

        <article className="editorial-card-muted flex flex-col justify-between p-6">
          <div>
            <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
              [BUZÓN]
            </div>
            <h2 className="font-display mb-2 text-2xl">{t.support.mailboxTitle}</h2>
            <p className="mb-5 text-sm text-[var(--muted)]">{t.support.mailboxDesc}</p>
          </div>
          <button type="button" onClick={onOpenFeedback} className="editorial-btn-primary text-xs">
            <MessageSquarePlus className="h-3.5 w-3.5" aria-hidden="true" />
            <span>{t.support.mailboxBtn}</span>
          </button>
        </article>

        <article className="editorial-card-muted flex flex-col justify-between p-6">
          <div>
            <div className="font-mono-editorial mb-2 text-xs font-bold uppercase text-[var(--red-text)]">
              [MIT · OPEN SOURCE]
            </div>
            <h2 className="font-display mb-2 text-2xl">{t.support.githubTitle}</h2>
            <p className="mb-5 text-sm text-[var(--muted)]">{t.support.githubDesc}</p>
          </div>
          <button
            type="button"
            onClick={() => void onOpenExternal(supportConfig.githubUrl)}
            className="editorial-btn-secondary text-xs"
          >
            <span>{t.support.githubBtn}</span>
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </article>
      </div>

      {/* Supporters Acknowledgments */}
      <section className="editorial-box-thin space-y-3 p-6">
        <div className="flex items-center gap-2">
          <HeartHandshake className="h-5 w-5 text-[var(--red-text)]" aria-hidden="true" />
          <h2 className="font-display text-2xl">{t.support.supportersTitle}</h2>
        </div>
        <p className="text-sm text-[var(--muted)]">{t.support.supportersDesc}</p>
        <ul className="space-y-1.5 text-sm">
          {t.support.supportersList.map((line) => (
            <li key={line} className="flex items-start gap-2">
              <span className="font-mono-editorial font-bold text-[var(--red-text)]">›</span>
              <span>{line}</span>
            </li>
          ))}
        </ul>
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
