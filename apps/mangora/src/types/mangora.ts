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

export function libraryKey(mangaId: string, provider: string): string {
  return `${provider.trim().toLowerCase()}::${mangaId.trim()}`;
}
