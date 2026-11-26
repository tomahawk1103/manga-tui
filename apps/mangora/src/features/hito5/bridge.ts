import {
  computeWebExtendedMetrics,
  getWebAppearanceConfig,
  recordWebReadingSession,
  saveWebAppearanceConfig,
  saveWebMangaGenres,
} from "../../web/db";
import {
  BUILTIN_PALETTES,
  BUILTIN_READER_BACKGROUNDS,
  getPaletteSpec,
} from "./palettes";
import type {
  AppearanceCatalogResponse,
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

function resolveTauriInvoke(override?: TauriInvokeFn): TauriInvokeFn | null {
  if (override) return override;
  if (typeof window !== "undefined" && typeof window.__TAURI_INTERNALS__?.invoke === "function") {
    return window.__TAURI_INTERNALS__.invoke;
  }
  return null;
}

function escapeXml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
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
      return getWebAppearanceConfig();
    },

    async saveAppearanceConfig(config: MangoraAppearanceConfig): Promise<MangoraAppearanceConfig> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<MangoraAppearanceConfig>("mangora_save_appearance_config", { config });
      }
      return saveWebAppearanceConfig(config);
    },

    async recordReadingSession(request: RecordReadingSessionRequest): Promise<number> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<number>("mangora_record_reading_session", { request });
      }
      return recordWebReadingSession(request);
    },

    async saveMangaGenres(request: SaveMangaGenresRequest): Promise<number> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<number>("mangora_save_manga_genres", { request });
      }
      return saveWebMangaGenres(request);
    },

    async getExtendedReaderMetrics(): Promise<ExtendedReaderMetrics> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<ExtendedReaderMetrics>("mangora_get_extended_reader_metrics");
      }
      return computeWebExtendedMetrics();
    },

    async exportStatsCardSvg(
      request: ExportStatsCardRequest,
    ): Promise<EditorialStatsCardSvgResponse> {
      const invoke = resolveTauriInvoke(customInvoke);
      if (invoke) {
        return invoke<EditorialStatsCardSvgResponse>("mangora_export_stats_card_svg", { request });
      }
      return buildFallbackStatsCardSvg(await computeWebExtendedMetrics(), request);
    },
  };
}

export const defaultHito5Bridge = createHito5Bridge();
