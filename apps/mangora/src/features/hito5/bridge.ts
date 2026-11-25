import {
  BUILTIN_PALETTES,
  BUILTIN_READER_BACKGROUNDS,
  DEFAULT_APPEARANCE_CONFIG,
  getPaletteSpec,
} from "./palettes";
import type {
  AppearanceCatalogResponse,
  DailyReadingActivity,
  EditorialStatsCardSvgResponse,
  ExportStatsCardRequest,
  ExtendedReaderMetrics,
  MangoraAppearanceConfig,
  RecordReadingSessionRequest,
  SaveMangaGenresRequest,
} from "./types";

export type TauriInvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke?: TauriInvokeFn;
    };
  }
}

const APPEARANCE_STORAGE_KEY = "mangora.hito5.appearance.v1";
const SESSIONS_STORAGE_KEY = "mangora.hito5.sessions.v1";
const GENRES_STORAGE_KEY = "mangora.hito5.genres.v1";

function resolveTauriInvoke(override?: TauriInvokeFn): TauriInvokeFn | null {
  if (override) return override;
  if (typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__?.invoke === "function") {
    return window.__TAURI_INTERNALS__.invoke;
  }
  return null;
}

function readLocalJson<T>(key: string, fallback: T): T {
  if (typeof window === "undefined") return fallback;
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function writeLocalJson<T>(key: string, value: T): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Ignore quota errors in browser preview mode.
  }
}

function escapeXml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function buildFallbackMetrics(): ExtendedReaderMetrics {
  const sessions = readLocalJson<RecordReadingSessionRequest[]>(SESSIONS_STORAGE_KEY, []);
  const genresMap = readLocalJson<Record<string, string[]>>(GENRES_STORAGE_KEY, {});
  const now = new Date();
  const todayIso = now.toISOString().slice(0, 10);

  const completedChapters = new Set<string>();
  const activeDates = new Set<string>();
  const mangaSet = new Set<string>();
  let totalSeconds = 0;
  let totalPages = 0;

  for (const session of sessions) {
    totalSeconds += Math.min(session.duration_seconds, 43_200);
    totalPages += session.pages_read;
    mangaSet.add(`${session.provider}:${session.manga_id}`);
    const dateKey = (session.started_at ?? todayIso).slice(0, 10);
    activeDates.add(dateKey);
    if (session.completed_chapter) {
      completedChapters.add(session.chapter_id);
    }
  }

  const daily: DailyReadingActivity[] = [];
  for (let offset = 13; offset >= 0; offset--) {
    const d = new Date(now);
    d.setUTCDate(now.getUTCDate() - offset);
    const iso = d.toISOString().slice(0, 10);
    const daySessions = sessions.filter((s) => (s.started_at ?? todayIso).slice(0, 10) === iso);
    const daySecs = daySessions.reduce((acc, s) => acc + Math.min(s.duration_seconds, 43_200), 0);
    const dayChapters = new Set(daySessions.filter((s) => s.completed_chapter).map((s) => s.chapter_id)).size;
    const dayPages = daySessions.reduce((acc, s) => acc + s.pages_read, 0);
    daily.push({
      date: iso,
      chapters_read: dayChapters,
      reading_minutes: daySecs === 0 ? 0 : Math.max(1, Math.round(daySecs / 60)),
      pages_read: dayPages,
    });
  }

  const totalMins = totalSeconds === 0 ? 0 : Math.max(1, Math.round(totalSeconds / 60));
  const totalCh = completedChapters.size;
  const genreCounts = new Map<string, number>();
  for (const list of Object.values(genresMap)) {
    for (const g of list) {
      genreCounts.set(g, (genreCounts.get(g) ?? 0) + 1);
    }
  }

  const topGenres = Array.from(genreCounts.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([genre, count]) => ({
      genre,
      mangas_count: count,
      chapters_read: totalCh,
      reading_minutes: totalMins,
      share_percent: genreCounts.size > 0 ? Math.round((100 / genreCounts.size) * 10) / 10 : 0,
    }));

  return {
    total_chapters_read: totalCh,
    chapters_read_today: daily[daily.length - 1]?.chapters_read ?? 0,
    chapters_read_last_7_days: daily.slice(-7).reduce((acc, d) => acc + d.chapters_read, 0),
    chapters_read_last_30_days: totalCh,
    total_pages_read: totalPages,
    recorded_reading_seconds: totalSeconds,
    estimated_total_reading_minutes: totalMins,
    average_minutes_per_chapter: totalCh > 0 ? Math.round((totalMins / totalCh) * 10) / 10 : 0,
    active_days_count: activeDates.size,
    current_streak_days: activeDates.has(todayIso) ? 1 : 0,
    longest_streak_days: activeDates.size > 0 ? 1 : 0,
    reading_mangas: mangaSet.size,
    plan_to_read_mangas: 0,
    favorites_count: 0,
    shelves_count: 0,
    downloaded_chapters: 0,
    top_genres: topGenres,
    top_read_mangas: [],
    provider_breakdown: [],
    daily_activity_last_14_days: daily,
    generated_at: now.toISOString(),
    privacy_local_only: true,
  };
}

function buildFallbackStatsCardSvg(
  metrics: ExtendedReaderMetrics,
  request: ExportStatsCardRequest,
): EditorialStatsCardSvgResponse {
  const spec = getPaletteSpec(request.palette);
  const isEs = request.language === "es";
  const datePart = metrics.generated_at.slice(0, 10) || "summary";
  const alias = request.reader_alias?.trim();
  const title = alias
    ? isEs
      ? `Bitácora de ${escapeXml(alias)},`
      : `${escapeXml(alias)}'s log,`
    : isEs
      ? "Tu lectura,"
      : "Your manga,";
  const accent = isEs ? "en tinta." : "in ink.";
  const barText = isEs ? "&gt;_ mangora / resumen del lector" : "&gt;_ mangora / reader summary";
  const footer = isEs
    ? "100 % local en dispositivo · cero telemetría · Mangora"
    : "100 % on-device local · zero telemetry · Mangora";

  const svg = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="675" viewBox="0 0 1200 675">`,
    `  <rect x="0" y="0" width="1200" height="675" fill="${spec.paper}"/>`,
    `  <rect x="44" y="44" width="1120" height="595" fill="${spec.ink}"/>`,
    `  <rect x="36" y="36" width="1120" height="595" fill="${spec.paper}" stroke="${spec.ink}" stroke-width="3"/>`,
    `  <rect x="36" y="36" width="1120" height="44" fill="${spec.reader_bar_bg}"/>`,
    `  <text x="60" y="63" font-family="'Cascadia Mono', Consolas, monospace" font-size="15" font-weight="700" fill="${spec.reader_bar_fg}">${barText}</text>`,
    `  <text x="64" y="160" font-family="'Arial Black', Arial, sans-serif" font-size="38" font-weight="900" fill="${spec.ink}">${title} <tspan fill="${spec.red}">${accent}</tspan></text>`,
    `  <text x="64" y="245" font-family="'Arial Black', Arial, sans-serif" font-size="32" font-weight="900" fill="${spec.ink}">${metrics.total_chapters_read} ${isEs ? "capítulos" : "chapters"} · ${metrics.total_pages_read} ${isEs ? "páginas" : "pages"} · ${metrics.estimated_total_reading_minutes} min</text>`,
    `  <text x="64" y="604" font-family="'Cascadia Mono', Consolas, monospace" font-size="12" font-weight="700" fill="${spec.muted}">${escapeXml(footer)}</text>`,
    `</svg>`,
  ].join("\n");

  return {
    svg,
    suggested_filename: `mangora-stats-${datePart}.svg`,
    width: 1200,
    height: 675,
    generated_at: metrics.generated_at,
  };
}

export interface Hito5Bridge {
  isTauriRuntime(): boolean;
  getAppearanceCatalog(): Promise<AppearanceCatalogResponse>;
  getAppearanceConfig(): Promise<MangoraAppearanceConfig>;
  saveAppearanceConfig(config: MangoraAppearanceConfig): Promise<MangoraAppearanceConfig>;
  recordReadingSession(request: RecordReadingSessionRequest): Promise<number>;
  saveMangaGenres(request: SaveMangaGenresRequest): Promise<number>;
  getExtendedReaderMetrics(): Promise<ExtendedReaderMetrics>;
  exportStatsCardSvg(request: ExportStatsCardRequest): Promise<EditorialStatsCardSvgResponse>;
}

/**
 * Creates a Hito 5 backend bridge using Tauri v2 `invoke` when available,
 * falling back cleanly to 100 % local browser storage in Vite preview mode.
 */
export function createHito5Bridge(customInvoke?: TauriInvokeFn): Hito5Bridge {
  return {
    isTauriRuntime(): boolean {
      return resolveTauriInvoke(customInvoke) !== null;
    },

    async getAppearanceCatalog(): Promise<AppearanceCatalogResponse> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<AppearanceCatalogResponse>("mangora_get_appearance_catalog");
      }
      return {
        palettes: BUILTIN_PALETTES,
        reader_backgrounds: BUILTIN_READER_BACKGROUNDS,
      };
    },

    async getAppearanceConfig(): Promise<MangoraAppearanceConfig> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<MangoraAppearanceConfig>("mangora_get_appearance_config");
      }
      const stored = readLocalJson<Partial<MangoraAppearanceConfig>>(APPEARANCE_STORAGE_KEY, {});
      return { ...DEFAULT_APPEARANCE_CONFIG, ...stored };
    },

    async saveAppearanceConfig(config: MangoraAppearanceConfig): Promise<MangoraAppearanceConfig> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<MangoraAppearanceConfig>("mangora_save_appearance_config", { config });
      }
      writeLocalJson(APPEARANCE_STORAGE_KEY, config);
      return config;
    },

    async recordReadingSession(request: RecordReadingSessionRequest): Promise<number> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<number>("mangora_record_reading_session", { request });
      }
      const list = readLocalJson<RecordReadingSessionRequest[]>(SESSIONS_STORAGE_KEY, []);
      list.push(request);
      writeLocalJson(SESSIONS_STORAGE_KEY, list);
      return list.length;
    },

    async saveMangaGenres(request: SaveMangaGenresRequest): Promise<number> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<number>("mangora_save_manga_genres", { request });
      }
      const map = readLocalJson<Record<string, string[]>>(GENRES_STORAGE_KEY, {});
      const key = `${request.provider}:${request.manga_id}`;
      const existing = new Set((map[key] ?? []).map((g) => g.toLowerCase()));
      const merged = [...(map[key] ?? [])];
      let added = 0;
      for (const raw of request.genres) {
        const trimmed = raw.trim();
        if (trimmed && !existing.has(trimmed.toLowerCase())) {
          existing.add(trimmed.toLowerCase());
          merged.push(trimmed);
          added++;
        }
      }
      map[key] = merged;
      writeLocalJson(GENRES_STORAGE_KEY, map);
      return added;
    },

    async getExtendedReaderMetrics(): Promise<ExtendedReaderMetrics> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<ExtendedReaderMetrics>("mangora_get_extended_reader_metrics");
      }
      return buildFallbackMetrics();
    },

    async exportStatsCardSvg(
      request: ExportStatsCardRequest,
    ): Promise<EditorialStatsCardSvgResponse> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<EditorialStatsCardSvgResponse>("mangora_export_stats_card_svg", { request });
      }
      return buildFallbackStatsCardSvg(buildFallbackMetrics(), request);
    },
  };
}

export const defaultHito5Bridge = createHito5Bridge();
