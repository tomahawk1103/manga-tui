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
  storageMode: "env" | "portable" | "system";
  storageModeLabel: string;
  dataDir: string;
  configDir: string;
  downloadsDir: string;
  activeProvider: string;
  recommendedProvider: string;
  availableProviders: ProviderOption[];
  favoriteGenres: string[];
  onboardingCompleted: boolean;
  library: LibraryDetection;
}

export interface OnboardingPreferencesPayload {
  uiLanguage: UiLanguage;
  favoriteGenres: string[];
  provider: string;
  onboardingCompleted: boolean;
  themeMode?: ThemeMode;
}
