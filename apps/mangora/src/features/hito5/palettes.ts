import type {
  MangoraAppearanceConfig,
  PaletteSpec,
  ReaderBackgroundSpec,
  UiLanguage,
} from "./types";

export const DEFAULT_APPEARANCE_CONFIG: MangoraAppearanceConfig = {
  palette: "classic_paper",
  library_density: "comfortable",
  explore_density: "comfortable",
  library_view_mode: "grid",
  library_sort: "last_read",
  explore_sort: "relevance",
  reader_background: "theme_default",
  show_screentone_accents: true,
};

export const BUILTIN_PALETTES: PaletteSpec[] = [
  {
    id: "classic_paper",
    name_es: "Papel Clásico",
    name_en: "Classic Paper",
    description_es: "Papel editorial cálido y tinta negra de alto contraste.",
    description_en: "Warm editorial paper with high-contrast black ink.",
    is_dark: false,
    paper: "#f8f7f1",
    surface: "#ecebe3",
    ink: "#191918",
    red: "#cb3028",
    red_hover: "#a1201b",
    muted: "#55554f",
    line: "#c8c7bc",
    focus: "#0077b6",
    reader_bar_bg: "#191918",
    reader_bar_fg: "#f8f7f1",
  },
  {
    id: "night_ink",
    name_es: "Tinta Nocturna",
    name_en: "Night Ink",
    description_es: "Modo oscuro en tinta mate pensado para lectura nocturna.",
    description_en: "Matte ink dark mode designed for late-night reading.",
    is_dark: true,
    paper: "#141413",
    surface: "#1f1f1d",
    ink: "#f8f7f1",
    red: "#ff8a76",
    red_hover: "#cb3028",
    muted: "#b5b4a8",
    line: "#3a3a35",
    focus: "#4cc9f0",
    reader_bar_bg: "#0c0c0b",
    reader_bar_fg: "#f8f7f1",
  },
  {
    id: "editorial_sepia",
    name_es: "Sepia Editorial",
    name_en: "Editorial Sepia",
    description_es: "Tonos de tomo antiguo y tinta marrón suave para descansar la vista.",
    description_en: "Vintage volume paper tones with soft brown ink for gentle contrast.",
    is_dark: false,
    paper: "#f4ecd8",
    surface: "#e7dcc2",
    ink: "#2b2118",
    red: "#a83226",
    red_hover: "#832218",
    muted: "#665746",
    line: "#cbbba0",
    focus: "#1d6fa5",
    reader_bar_bg: "#2b2118",
    reader_bar_fg: "#f4ecd8",
  },
  {
    id: "shonen_red",
    name_es: "Shonen Rojo",
    name_en: "Shonen Red",
    description_es: "Papel blanco vivo con acentos rojos intensos de revista semanal.",
    description_en: "Crisp white stock with bold weekly-magazine crimson accents.",
    is_dark: false,
    paper: "#fffaf5",
    surface: "#fcebe6",
    ink: "#171414",
    red: "#d91e2a",
    red_hover: "#a6111c",
    muted: "#5a4c4c",
    line: "#e3c2bd",
    focus: "#0077b6",
    reader_bar_bg: "#171414",
    reader_bar_fg: "#fffaf5",
  },
  {
    id: "sumi_slate",
    name_es: "Sumi Grafito",
    name_en: "Sumi Slate",
    description_es: "Gris grafito equilibrado con trazos de tinta clara y coral.",
    description_en: "Balanced graphite slate with crisp light ink and coral highlights.",
    is_dark: true,
    paper: "#1c1f24",
    surface: "#252930",
    ink: "#eff1f5",
    red: "#f06553",
    red_hover: "#cb3028",
    muted: "#a6adb9",
    line: "#3b424d",
    focus: "#66b2ff",
    reader_bar_bg: "#121417",
    reader_bar_fg: "#eff1f5",
  },
];

export const BUILTIN_READER_BACKGROUNDS: ReaderBackgroundSpec[] = [
  {
    id: "theme_default",
    name_es: "Según tema activo",
    name_en: "Match active theme",
    description_es: "Usa el fondo principal de la paleta editorial elegida.",
    description_en: "Uses the main canvas color of the active editorial palette.",
    css_value: "var(--paper)",
    is_dark: false,
    has_pattern: false,
  },
  {
    id: "pure_paper",
    name_es: "Papel Puro",
    name_en: "Pure Paper",
    description_es: "Fondo claro de papel manga para lectura diurna.",
    description_en: "Clean manga paper canvas for daytime reading.",
    css_value: "#f8f7f1",
    is_dark: false,
    has_pattern: false,
  },
  {
    id: "warm_sepia",
    name_es: "Sepia Suave",
    name_en: "Warm Sepia",
    description_es: "Papel cálido de volumen impreso que reduce el deslumbramiento.",
    description_en: "Warm printed volume backdrop that softens glare.",
    css_value: "#f4ecd8",
    is_dark: false,
    has_pattern: false,
  },
  {
    id: "soft_gray",
    name_es: "Gris Neutro",
    name_en: "Soft Gray",
    description_es: "Gris editorial tenue para resaltar los márgenes blancos de la página.",
    description_en: "Neutral editorial gray that frames white manga margins cleanly.",
    css_value: "#e6e5de",
    is_dark: false,
    has_pattern: false,
  },
  {
    id: "matte_ink",
    name_es: "Tinta Mate",
    name_en: "Matte Ink",
    description_es: "Fondo oscuro suave sin reflejos intensos.",
    description_en: "Low-glare matte dark canvas.",
    css_value: "#191918",
    is_dark: true,
    has_pattern: false,
  },
  {
    id: "pitch_black",
    name_es: "Negro Absoluto",
    name_en: "Pitch Black",
    description_es: "Contraste máximo en pantallas OLED o habitaciones a oscuras.",
    description_en: "Maximum contrast for OLED displays and dark rooms.",
    css_value: "#090909",
    is_dark: true,
    has_pattern: false,
  },
  {
    id: "subtle_screentone",
    name_es: "Trama Screentone",
    name_en: "Subtle Screentone",
    description_es: "Patrón sutil de puntos de imprenta manga alrededor de las viñetas.",
    description_en: "Subtle manga print halftone dot pattern framing the page.",
    css_value:
      "radial-gradient(rgba(25, 25, 24, 0.08) 1px, transparent 1.2px) 0 0 / 8px 8px, var(--paper)",
    is_dark: false,
    has_pattern: true,
  },
];

export function getPaletteSpec(
  paletteId: MangoraAppearanceConfig["palette"],
  catalog: PaletteSpec[] = BUILTIN_PALETTES,
): PaletteSpec {
  return catalog.find((p) => p.id === paletteId) ?? BUILTIN_PALETTES[0];
}

export function getReaderBackgroundClass(
  background: MangoraAppearanceConfig["reader_background"],
): string {
  return `mangora-reader-canvas mangora-reader-bg--${background}`;
}

/**
 * Applies appearance tokens and data attributes live to `document.documentElement`
 * (or any explicit container element).
 */
export function applyAppearanceToDocument(
  config: MangoraAppearanceConfig,
  target?: HTMLElement | null,
  catalog: PaletteSpec[] = BUILTIN_PALETTES,
): void {
  const el = target ?? (typeof document !== "undefined" ? document.documentElement : null);
  if (!el) return;

  const spec = getPaletteSpec(config.palette, catalog);
  el.dataset.mangoraPalette = config.palette;
  el.dataset.mangoraLibraryDensity = config.library_density;
  el.dataset.mangoraExploreDensity = config.explore_density;
  el.dataset.mangoraLibraryView = config.library_view_mode;
  el.dataset.mangoraReaderBg = config.reader_background;
  el.dataset.mangoraScreentone = config.show_screentone_accents ? "true" : "false";

  el.style.setProperty("--paper", spec.paper);
  el.style.setProperty("--surface", spec.surface);
  el.style.setProperty("--ink", spec.ink);
  el.style.setProperty("--red", spec.red);
  el.style.setProperty("--red-hover", spec.red_hover);
  el.style.setProperty("--muted", spec.muted);
  el.style.setProperty("--line", spec.line);
  el.style.setProperty("--focus-ring", spec.focus);
  el.style.setProperty("--reader-bar-bg", spec.reader_bar_bg);
  el.style.setProperty("--reader-bar-fg", spec.reader_bar_fg);
  el.style.setProperty("--button-fg", spec.is_dark ? spec.paper : "#ffffff");
  el.style.colorScheme = spec.is_dark ? "dark" : "light";
}

export interface Hito5Labels {
  appearanceEyebrow: string;
  appearanceHeadingPrefix: string;
  appearanceHeadingAccent: string;
  appearanceSubtitle: string;
  paletteSection: string;
  librarySection: string;
  exploreSection: string;
  readerSection: string;
  screentoneToggle: string;
  screentoneHelp: string;
  resetDefaults: string;
  densityLabel: string;
  densityCompact: string;
  densityComfortable: string;
  densitySpacious: string;
  viewModeLabel: string;
  viewGrid: string;
  viewList: string;
  sortLabel: string;
  librarySortOptions: Record<MangoraAppearanceConfig["library_sort"], string>;
  exploreSortOptions: Record<MangoraAppearanceConfig["explore_sort"], string>;
  metricsEyebrow: string;
  metricsHeadingPrefix: string;
  metricsHeadingAccent: string;
  metricsPrivacyBadge: string;
  refreshStats: string;
  exportPng: string;
  exportSvg: string;
  aliasPlaceholder: string;
  aliasLabel: string;
  chaptersReadLabel: string;
  readingTimeLabel: string;
  streakLabel: string;
  librarySummaryLabel: string;
  topGenresTitle: string;
  topSeriesTitle: string;
  providersTitle: string;
  activity14DaysTitle: string;
  cardPreviewTitle: string;
  emptyGenres: string;
  emptySeries: string;
  savingText: string;
  savedText: string;
}

const LABELS_ES: Hito5Labels = {
  appearanceEyebrow: "Edición Visual · Hito 5",
  appearanceHeadingPrefix: "Personaliza tu lectura,",
  appearanceHeadingAccent: "a tu estilo.",
  appearanceSubtitle:
    "Elige paletas inspiradas en papel manga y tinta, ajusta la densidad de tarjetas y define fondos suaves para leer sin fatiga.",
  paletteSection: "Paleta editorial",
  librarySection: "Visualización de Biblioteca",
  exploreSection: "Visualización de Explorar",
  readerSection: "Fondo sutil del lector",
  screentoneToggle: "Mostrar tramas screentone en tarjetas destacadas",
  screentoneHelp: "Añade la retícula de puntos rojos editorial heredada de la web oficial.",
  resetDefaults: "Restablecer estilo clásico",
  densityLabel: "Densidad",
  densityCompact: "Compacta",
  densityComfortable: "Equilibrada",
  densitySpacious: "Amplia",
  viewModeLabel: "Vista",
  viewGrid: "Cuadrícula",
  viewList: "Lista",
  sortLabel: "Orden",
  librarySortOptions: {
    last_read: "Última lectura",
    title_asc: "Título (A–Z)",
    title_desc: "Título (Z–A)",
    chapters_read_desc: "Más capítulos leídos",
    date_added_desc: "Añadidos recientemente",
    favorites_first: "Favoritos primero",
  },
  exploreSortOptions: {
    relevance: "Relevancia del catálogo",
    title_asc: "Título (A–Z)",
    title_desc: "Título (Z–A)",
    status_ongoing_first: "En emisión primero",
  },
  metricsEyebrow: "Métricas Personales · 100 % Local",
  metricsHeadingPrefix: "Tu bitácora,",
  metricsHeadingAccent: "en cifras.",
  metricsPrivacyBadge: "100 % PRIVADO EN ESTE EQUIPO · CERO TELEMETRÍA",
  refreshStats: "Actualizar",
  exportPng: "Exportar tarjeta PNG",
  exportSvg: "Exportar SVG",
  aliasLabel: "Firma / Alias opcional en la tarjeta",
  aliasPlaceholder: "Ej. Lector de domingo",
  chaptersReadLabel: "Capítulos leídos",
  readingTimeLabel: "Tiempo estimado",
  streakLabel: "Racha de días",
  librarySummaryLabel: "Series activas",
  topGenresTitle: "Géneros más leídos",
  topSeriesTitle: "Series con más capítulos leídos",
  providersTitle: "Fuentes en tu biblioteca",
  activity14DaysTitle: "Actividad de los últimos 14 días",
  cardPreviewTitle: "Tarjeta editorial lista para compartir",
  emptyGenres: "Abre o guarda fichas con géneros para ver aquí tu perfil por temática.",
  emptySeries: "Todavía no hay capítulos leídos registrados en tu historial local.",
  savingText: "Guardando…",
  savedText: "Guardado en config.toml",
};

const LABELS_EN: Hito5Labels = {
  appearanceEyebrow: "Visual Customization · Milestone 5",
  appearanceHeadingPrefix: "Tailor your reading,",
  appearanceHeadingAccent: "in paper & ink.",
  appearanceSubtitle:
    "Switch between editorial paper palettes, tune card density in Library and Explore, and pick subtle backgrounds for the HD reader.",
  paletteSection: "Editorial palette",
  librarySection: "Library layout",
  exploreSection: "Explore layout",
  readerSection: "Subtle reader canvas",
  screentoneToggle: "Enable editorial screentone dot accents",
  screentoneHelp: "Adds the halftone red dot backdrop inherited from the official landing page.",
  resetDefaults: "Reset to Classic Paper",
  densityLabel: "Density",
  densityCompact: "Compact",
  densityComfortable: "Comfortable",
  densitySpacious: "Spacious",
  viewModeLabel: "View",
  viewGrid: "Grid",
  viewList: "List",
  sortLabel: "Sort",
  librarySortOptions: {
    last_read: "Last read",
    title_asc: "Title (A–Z)",
    title_desc: "Title (Z–A)",
    chapters_read_desc: "Most chapters read",
    date_added_desc: "Recently added",
    favorites_first: "Favorites first",
  },
  exploreSortOptions: {
    relevance: "Catalog relevance",
    title_asc: "Title (A–Z)",
    title_desc: "Title (Z–A)",
    status_ongoing_first: "Ongoing first",
  },
  metricsEyebrow: "Personal Metrics · 100 % Local",
  metricsHeadingPrefix: "Your reading log,",
  metricsHeadingAccent: "by the numbers.",
  metricsPrivacyBadge: "100 % ON-DEVICE PRIVATE · ZERO TELEMETRY",
  refreshStats: "Refresh",
  exportPng: "Export PNG card",
  exportSvg: "Export SVG",
  aliasLabel: "Optional reader alias on card",
  aliasPlaceholder: "e.g. Weekend Reader",
  chaptersReadLabel: "Chapters read",
  readingTimeLabel: "Estimated time",
  streakLabel: "Reading streak",
  librarySummaryLabel: "Active series",
  topGenresTitle: "Most read genres",
  topSeriesTitle: "Top read series",
  providersTitle: "Sources in your library",
  activity14DaysTitle: "Last 14 days activity",
  cardPreviewTitle: "Shareable editorial summary card",
  emptyGenres: "Open or tag series with genres to see your thematic breakdown here.",
  emptySeries: "No read chapters recorded in your local SQLite history yet.",
  savingText: "Saving…",
  savedText: "Saved to config.toml",
};

export function getHito5Labels(language: UiLanguage = "es"): Hito5Labels {
  return language === "en" ? LABELS_EN : LABELS_ES;
}
