export type ThemeMode = "paper" | "ink";

export type UiLanguage = "es" | "en";

export type NavSection = "home" | "explore" | "downloads" | "settings" | "support";

export interface LibraryDetection {
  hasExistingData: boolean;
  libraryMangas: number;
  chaptersRead: number;
  favorites: number;
  downloadedChapters: number;
  offlineSeriesCount: number;
  lastReadTitle: string | null;
}

export interface ProviderOption {
  id: string;
  name: string;
  badge: string;
  languageGroup: "es" | "en" | "multi";
  descriptionEs: string;
  descriptionEn: string;
}

export interface InitialAppState {
  appVersion: string;
  uiLanguage: UiLanguage;
  themeMode: ThemeMode;
  platform: "windows" | "macos" | "linux" | "android" | "ios" | "unknown";
  isMobile: boolean;
  canOpenDataFolder: boolean;
  storageMode: "env" | "portable" | "system" | "sandbox";
  storageModeLabel: string;
  dataDir: string;
  configDir: string;
  downloadsDir: string;
  activeProvider: string;
  recommendedProvider: string;
  availableProviders: ProviderOption[];
  favoriteGenres: string[];
  onboardingCompleted: boolean;
  downloadFormat?: string;
  library: LibraryDetection;
}

export interface OnboardingPreferencesPayload {
  uiLanguage: UiLanguage;
  favoriteGenres: string[];
  provider: string;
  onboardingCompleted: boolean;
  themeMode?: ThemeMode;
}

export interface LibraryEntry {
  id: string;
  title: string;
  provider: string;
  providerLabel: string;
  coverUrl: string | null;
  lastRead: string | null;
  lastReadChapter: string | null;
  totalReadCount: number;
  bookmarkChapter: string | null;
  bookmarkPage: number | null;
  isFavorite: boolean;
  isPlanToRead: boolean;
  downloadedCount: number;
  shelfName: string | null;
}

export interface LibraryOverview {
  continueReading: LibraryEntry[];
  favorites: LibraryEntry[];
  recentHistory: LibraryEntry[];
  totalContinueReading: number;
  totalFavorites: number;
  totalRecent: number;
}

export interface ToggleFavoritePayload {
  mangaId: string;
  provider: string;
  title: string;
  coverUrl?: string | null;
}

export interface ToggleFavoriteResult {
  isFavorite: boolean;
  appState: InitialAppState;
  library: LibraryOverview;
}

export interface ToggleChapterReadPayload {
  mangaId: string;
  provider: string;
  mangaTitle: string;
  coverUrl?: string | null;
  chapterId: string;
  chapterTitle: string;
  translatedLanguage: string;
}

export interface ToggleChapterReadResult {
  isRead: boolean;
  appState: InitialAppState;
  library: LibraryOverview;
}

export interface SeriesSelection {
  mangaId: string;
  provider: string;
  title: string;
  coverUrl?: string | null;
  description?: string | null;
}

export interface SeriesChapterItem {
  id: string;
  title: string;
  chapterNumber: string;
  volumeNumber: string | null;
  language: string;
  scanlator: string | null;
  publicationDate: string | null;
  isRead: boolean;
  isDownloaded: boolean;
  isAvailableOnline: boolean;
  isBookmarked: boolean;
  bookmarkedPage: number | null;
  offlineFormat: string | null;
}

export interface SeriesDetailResponse {
  id: string;
  provider: string;
  providerLabel: string;
  title: string;
  coverUrl: string | null;
  description: string | null;
  author: string | null;
  artist: string | null;
  status: string;
  genres: string[];
  availableLanguages: string[];
  activeLanguage: string;
  order: "asc" | "desc";
  page: number;
  totalChapters: number;
  hasNextPage: boolean;
  isFavorite: boolean;
  isOfflineFallback: boolean;
  offlineChapterCount: number;
  readChapterCount: number;
  nextUnreadChapter: SeriesChapterItem | null;
  chapters: SeriesChapterItem[];
  onlineWarning: string | null;
}

export interface AniListTag {
  name: string;
  rank: number | null;
}

export interface AniListReview {
  id: number;
  summary: string;
  rating: number | null;
  ratingAmount: number | null;
  score: number | null;
  userName: string;
  siteUrl: string | null;
}

export interface AniListCommunityInfo {
  found: boolean;
  anilistId: number | null;
  siteUrl: string | null;
  title: string | null;
  averageScore: number | null;
  meanScore: number | null;
  popularity: number | null;
  favourites: number | null;
  status: string | null;
  genres: string[];
  tags: AniListTag[];
  reviews: AniListReview[];
  note: string | null;
}

export interface ForYouResponse {
  provider: string;
  providerLabel: string;
  matchedGenres: string[];
  items: Array<{
    id: string;
    title: string;
    description: string | null;
    coverUrl: string;
    genres: string[];
    languages: string[];
  }>;
  fallbackPopular: boolean;
}

export function libraryKey(mangaId: string, provider: string): string {
  return `${provider.trim().toLowerCase()}::${mangaId.trim()}`;
}
