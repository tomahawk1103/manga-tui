/**
 * Mangora Hito 5 — TypeScript contracts matching `crates/manga-core` JSON payloads.
 */

export type UiLanguage = "es" | "en";

export type EditorialPalette =
  | "classic_paper"
  | "night_ink"
  | "editorial_sepia"
  | "shonen_red"
  | "sumi_slate";

export type CardDensity = "compact" | "comfortable" | "spacious";

export type LibraryViewMode = "grid" | "list";

export type LibrarySortBy =
  | "last_read"
  | "title_asc"
  | "title_desc"
  | "chapters_read_desc"
  | "date_added_desc"
  | "favorites_first";

export type ExploreSortBy =
  | "relevance"
  | "title_asc"
  | "title_desc"
  | "status_ongoing_first";

export type ReaderBackground =
  | "theme_default"
  | "pure_paper"
  | "warm_sepia"
  | "soft_gray"
  | "matte_ink"
  | "pitch_black"
  | "subtle_screentone";

export interface PaletteSpec {
  id: EditorialPalette;
  name_es: string;
  name_en: string;
  description_es: string;
  description_en: string;
  is_dark: boolean;
  paper: string;
  surface: string;
  ink: string;
  red: string;
  red_hover: string;
  muted: string;
  line: string;
  focus: string;
  reader_bar_bg: string;
  reader_bar_fg: string;
}

export interface ReaderBackgroundSpec {
  id: ReaderBackground;
  name_es: string;
  name_en: string;
  description_es: string;
  description_en: string;
  css_value: string;
  is_dark: boolean;
  has_pattern: boolean;
}

export interface MangoraAppearanceConfig {
  palette: EditorialPalette;
  library_density: CardDensity;
  explore_density: CardDensity;
  library_view_mode: LibraryViewMode;
  library_sort: LibrarySortBy;
  explore_sort: ExploreSortBy;
  reader_background: ReaderBackground;
  show_screentone_accents: boolean;
}

export interface AppearanceCatalogResponse {
  palettes: PaletteSpec[];
  reader_backgrounds: ReaderBackgroundSpec[];
}

export interface GenreReadingStat {
  genre: string;
  mangas_count: number;
  chapters_read: number;
  reading_minutes: number;
  share_percent: number;
}

export interface TopMangaStat {
  manga_id: string;
  title: string;
  provider: string;
  chapters_read: number;
  reading_minutes: number;
}

export interface ProviderReadingStat {
  provider: string;
  mangas_count: number;
  chapters_read: number;
}

export interface DailyReadingActivity {
  date: string;
  chapters_read: number;
  reading_minutes: number;
  pages_read: number;
}

export interface ExtendedReaderMetrics {
  total_chapters_read: number;
  chapters_read_today: number;
  chapters_read_last_7_days: number;
  chapters_read_last_30_days: number;
  total_pages_read: number;
  recorded_reading_seconds: number;
  estimated_total_reading_minutes: number;
  average_minutes_per_chapter: number;
  active_days_count: number;
  current_streak_days: number;
  longest_streak_days: number;
  reading_mangas: number;
  plan_to_read_mangas: number;
  favorites_count: number;
  shelves_count: number;
  downloaded_chapters: number;
  top_genres: GenreReadingStat[];
  top_read_mangas: TopMangaStat[];
  provider_breakdown: ProviderReadingStat[];
  daily_activity_last_14_days: DailyReadingActivity[];
  generated_at: string;
  privacy_local_only: boolean;
}

export interface RecordReadingSessionRequest {
  manga_id: string;
  provider: string;
  chapter_id: string;
  duration_seconds: number;
  pages_read: number;
  completed_chapter: boolean;
  started_at?: string | null;
}

export interface SaveMangaGenresRequest {
  manga_id: string;
  provider: string;
  genres: string[];
}

export interface ExportStatsCardRequest {
  palette: EditorialPalette;
  language: UiLanguage;
  reader_alias?: string | null;
  include_screentone: boolean;
}

export interface EditorialStatsCardSvgResponse {
  svg: string;
  suggested_filename: string;
  width: number;
  height: number;
  generated_at: string;
}

/**
 * Minimal structural item interfaces for custom sorting in Library and Explore.
 */
export interface SortableLibraryItem {
  id: string;
  title: string;
  last_read?: string | null;
  created_at?: string | null;
  chapters_read?: number;
  is_favorite?: boolean;
}

export interface SortableExploreItem {
  id: string;
  title: string;
  status?: string | null;
}
