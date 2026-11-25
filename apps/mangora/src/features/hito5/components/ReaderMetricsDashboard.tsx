import { useMemo, useState } from "react";
import type { Hito5Bridge } from "../bridge";
import { getHito5Labels } from "../palettes";
import type { UiLanguage } from "../types";
import { useMangoraAppearance } from "../useMangoraAppearance";
import { useReaderMetrics } from "../useReaderMetrics";

export interface ReaderMetricsDashboardProps {
  language?: UiLanguage;
  bridge?: Hito5Bridge;
  refreshTrigger?: number;
}

function formatDurationMinutes(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m} min`;
  return `${h}h ${String(m).padStart(2, "0")}m`;
}

/**
 * 100 % device-local reader analytics dashboard styled with `docs/index.html`
 * paper-and-ink primitives, plus live editorial summary card preview & PNG/SVG export.
 */
export function ReaderMetricsDashboard({
  language = "es",
  bridge,
  refreshTrigger = 0,
}: ReaderMetricsDashboardProps) {
  const { config } = useMangoraAppearance();
  const [readerAlias, setReaderAlias] = useState("");
  const labels = getHito5Labels(language);

  const {
    metrics,
    cardPreview,
    isLoading,
    isExporting,
    error,
    refresh,
    exportPng,
    exportSvg,
  } = useReaderMetrics({
    bridge,
    palette: config.palette,
    language,
    includeScreentone: config.show_screentone_accents,
    readerAlias,
    refreshTrigger,
  });

  const svgDataUri = useMemo(() => {
    if (!cardPreview?.svg) return null;
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(cardPreview.svg)}`;
  }, [cardPreview]);

  const maxDailyChapters = useMemo(() => {
    if (!metrics?.daily_activity_last_14_days.length) return 1;
    return Math.max(1, ...metrics.daily_activity_last_14_days.map((d) => d.chapters_read));
  }, [metrics]);

  const pagesToday = useMemo(() => {
    const days = metrics?.daily_activity_last_14_days ?? [];
    return days.length > 0 ? days[days.length - 1].pages_read : 0;
  }, [metrics]);

  const pages14d = useMemo(() => {
    return (metrics?.daily_activity_last_14_days ?? []).reduce((sum, day) => sum + day.pages_read, 0);
  }, [metrics]);

  return (
    <section className="mangora-hito5-root mangora-card" aria-labelledby="mangora-metrics-title">
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          justifyContent: "space-between",
          alignItems: "flex-start",
          gap: "1rem",
          marginBottom: "1.25rem",
        }}
      >
        <div>
          <p className="mangora-eyebrow">{labels.metricsEyebrow}</p>
          <h2 id="mangora-metrics-title" className="mangora-editorial-title" style={{ fontSize: "2rem" }}>
            {labels.metricsHeadingPrefix} <em>{labels.metricsHeadingAccent}</em>
          </h2>
          <span
            className="mangora-mono-meta"
            style={{
              display: "inline-block",
              marginTop: "0.35rem",
              padding: "0.2rem 0.55rem",
              border: "1px solid var(--ink)",
              background: "var(--surface)",
              color: "var(--ink)",
            }}
          >
            {labels.metricsPrivacyBadge}
          </span>
        </div>

        <button
          type="button"
          className="mangora-btn-secondary"
          disabled={isLoading}
          onClick={() => void refresh()}
        >
          {labels.refreshStats}
        </button>
      </div>

      {error && (
        <div
          role="alert"
          className="mangora-accent-block"
          style={{ marginBottom: "1rem", color: "var(--red)" }}
        >
          {error}
        </div>
      )}

      {/* Primary KPI Grid */}
      <div className="mangora-kpi-grid" style={{ marginBottom: "1.35rem" }}>
        <article className="mangora-surface-card">
          <div className="mangora-mono-meta">{labels.chaptersReadLabel}</div>
          <div style={{ fontFamily: "var(--display)", fontSize: "2.25rem", fontWeight: 900 }}>
            {metrics?.total_chapters_read ?? 0}
          </div>
          <div className="mangora-mono-meta">
            {language === "en"
              ? `Today ${metrics?.chapters_read_today ?? 0} · 7d ${metrics?.chapters_read_last_7_days ?? 0}`
              : `Hoy ${metrics?.chapters_read_today ?? 0} · 7d ${metrics?.chapters_read_last_7_days ?? 0}`}
          </div>
        </article>

        <article className="mangora-surface-card">
          <div className="mangora-mono-meta">{labels.totalPagesLabel}</div>
          <div style={{ fontFamily: "var(--display)", fontSize: "2.25rem", fontWeight: 900 }}>
            {metrics?.total_pages_read ?? 0}
          </div>
          <div className="mangora-mono-meta">
            {language === "en"
              ? `Today ${pagesToday} · 14d ${pages14d}`
              : `Hoy ${pagesToday} · 14d ${pages14d}`}
          </div>
        </article>

        <article className="mangora-surface-card">
          <div className="mangora-mono-meta">{labels.readingTimeLabel}</div>
          <div style={{ fontFamily: "var(--display)", fontSize: "2.25rem", fontWeight: 900 }}>
            {formatDurationMinutes(metrics?.estimated_total_reading_minutes ?? 0)}
          </div>
          <div className="mangora-mono-meta">
            {(metrics?.average_minutes_per_chapter ?? 0).toFixed(1)}{" "}
            {language === "en" ? "min/ch" : "min/cap"}
          </div>
        </article>

        <article className="mangora-surface-card">
          <div className="mangora-mono-meta">{labels.streakLabel}</div>
          <div style={{ fontFamily: "var(--display)", fontSize: "2.25rem", fontWeight: 900 }}>
            {metrics?.current_streak_days ?? 0} d
          </div>
          <div className="mangora-mono-meta">
            {language === "en"
              ? `Best ${metrics?.longest_streak_days ?? 0} d · ${metrics?.active_days_count ?? 0} days`
              : `Mejor ${metrics?.longest_streak_days ?? 0} d · ${metrics?.active_days_count ?? 0} días`}
          </div>
        </article>

        <article className="mangora-surface-card">
          <div className="mangora-mono-meta">{labels.librarySummaryLabel}</div>
          <div style={{ fontFamily: "var(--display)", fontSize: "2.25rem", fontWeight: 900 }}>
            {metrics?.reading_mangas ?? 0}
          </div>
          <div className="mangora-mono-meta">
            {metrics?.favorites_count ?? 0} fav · {metrics?.downloaded_chapters ?? 0} offline
          </div>
        </article>
      </div>

      {/* Genres + Top Series & Providers */}
      <div className="mangora-two-col" style={{ marginBottom: "1.35rem" }}>
        <div className="mangora-accent-block">
          <h3 style={{ marginTop: 0, marginBottom: "0.75rem" }}>{labels.topGenresTitle}</h3>
          {!metrics?.top_genres.length ? (
            <p style={{ color: "var(--muted)", margin: 0 }}>{labels.emptyGenres}</p>
          ) : (
            <div style={{ display: "grid", gap: "0.65rem" }}>
              {metrics.top_genres.map((genre) => (
                <div key={genre.genre}>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      fontSize: "0.88rem",
                      marginBottom: "0.2rem",
                    }}
                  >
                    <strong>{genre.genre}</strong>
                    <span className="mangora-mono-meta">
                      {genre.chapters_read} {language === "en" ? "ch" : "cap"} ({genre.share_percent.toFixed(0)}%)
                    </span>
                  </div>
                  <div className="mangora-bar-track">
                    <div
                      className="mangora-bar-fill"
                      style={{ width: `${Math.max(6, Math.min(100, genre.share_percent))}%` }}
                    />
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="mangora-surface-card">
          <h3 style={{ marginTop: 0, marginBottom: "0.75rem" }}>{labels.topSeriesTitle}</h3>
          {!metrics?.top_read_mangas.length ? (
            <p style={{ color: "var(--muted)", margin: 0 }}>{labels.emptySeries}</p>
          ) : (
            <ol style={{ margin: 0, paddingLeft: "1.2rem", display: "grid", gap: "0.45rem" }}>
              {metrics.top_read_mangas.map((item) => (
                <li key={`${item.provider}:${item.manga_id}`}>
                  <div style={{ display: "flex", justifyContent: "space-between", gap: "0.75rem" }}>
                    <strong>{item.title}</strong>
                    <span className="mangora-mono-meta">
                      {item.chapters_read} {language === "en" ? "ch" : "cap"} · {item.reading_minutes}m
                    </span>
                  </div>
                </li>
              ))}
            </ol>
          )}

          {metrics?.provider_breakdown.length ? (
            <div style={{ marginTop: "1rem", paddingTop: "0.75rem", borderTop: "1px solid var(--line)" }}>
              <div className="mangora-mono-meta" style={{ marginBottom: "0.4rem" }}>
                {labels.providersTitle}
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "0.45rem" }}>
                {metrics.provider_breakdown.map((p) => (
                  <span
                    key={p.provider}
                    className="mangora-mono-meta"
                    style={{
                      padding: "0.2rem 0.5rem",
                      border: "1px solid var(--ink)",
                      background: "var(--paper)",
                      color: "var(--ink)",
                    }}
                  >
                    {p.provider}: {p.mangas_count} ({p.chapters_read})
                  </span>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      </div>

      {/* 14-day Activity Chart */}
      <div className="mangora-surface-card" style={{ marginBottom: "1.35rem" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3 style={{ margin: 0, fontSize: "1.05rem" }}>{labels.activity14DaysTitle}</h3>
          <span className="mangora-mono-meta">
            {language === "en"
              ? `Last 30d: ${metrics?.chapters_read_last_30_days ?? 0} ch`
              : `Últimos 30d: ${metrics?.chapters_read_last_30_days ?? 0} cap`}
          </span>
        </div>
        <div className="mangora-activity-bars" role="img" aria-label={labels.activity14DaysTitle}>
          {(metrics?.daily_activity_last_14_days ?? []).map((day) => {
            const pct =
              day.chapters_read === 0
                ? 8
                : Math.max(18, Math.round((day.chapters_read / maxDailyChapters) * 100));
            return (
              <div
                key={day.date}
                title={`${day.date}: ${day.chapters_read} (${day.reading_minutes}m)`}
                style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "0.25rem", height: "100%", justifyContent: "flex-end" }}
              >
                <div
                  style={{
                    width: "100%",
                    height: `${pct}%`,
                    background: day.chapters_read > 0 ? "var(--red)" : "var(--line)",
                    border: "1px solid var(--ink)",
                  }}
                />
                <span className="mangora-mono-meta" style={{ fontSize: "0.65rem" }}>
                  {day.date.slice(8, 10)}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Shareable Editorial Card Preview & Export */}
      <div className="mangora-screentone-stage">
        <figure style={{ margin: 0, border: "2px solid var(--ink)", background: "var(--paper)", boxShadow: "6px 6px var(--ink)" }}>
          <div className="mangora-ink-bar">
            <span>{labels.cardPreviewTitle}</span>
            <span>1200 × 675</span>
          </div>

          <div style={{ padding: "1rem", background: "var(--surface)" }}>
            <div
              style={{
                display: "flex",
                flexWrap: "wrap",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "0.75rem",
                marginBottom: "0.9rem",
              }}
            >
              <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: "0.5rem" }}>
                <label htmlFor="mangora-card-alias" className="mangora-mono-meta">
                  {labels.aliasLabel}:
                </label>
                <input
                  id="mangora-card-alias"
                  type="text"
                  maxLength={28}
                  className="mangora-input"
                  value={readerAlias}
                  placeholder={labels.aliasPlaceholder}
                  onChange={(e) => setReaderAlias(e.target.value)}
                />
              </div>

              <div style={{ display: "flex", flexWrap: "wrap", gap: "0.6rem" }}>
                <button
                  type="button"
                  className="mangora-btn-primary"
                  disabled={isExporting || !cardPreview}
                  onClick={() => void exportPng()}
                >
                  ↓ {labels.exportPng}
                </button>
                <button
                  type="button"
                  className="mangora-btn-secondary"
                  disabled={isExporting || !cardPreview}
                  onClick={() => void exportSvg()}
                >
                  {labels.exportSvg}
                </button>
              </div>
            </div>

            {svgDataUri && (
              <img
                src={svgDataUri}
                alt={labels.cardPreviewTitle}
                style={{
                  width: "100%",
                  height: "auto",
                  display: "block",
                  border: "1px solid var(--ink)",
                }}
              />
            )}
          </div>
        </figure>
      </div>
    </section>
  );
}
