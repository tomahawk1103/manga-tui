import { invoke } from "@tauri-apps/api/core";
import type {
  InitialAppState,
  OnboardingPreferencesPayload,
  ProviderOption,
  ThemeMode,
  UiLanguage,
} from "../types/mangora";

const BROWSER_STORAGE_KEY = "mangora_browser_fallback_v1";

export interface CatalogPage {
  items: { id: string; title: string; coverUrl: string; genres: string[]; languages: string[] }[];
  hasNext: boolean;
}

export function searchCatalog(provider: string, query: string, page: number) {
  return invoke<CatalogPage>("search_catalog", { provider, query, page });
}

const FALLBACK_PROVIDERS: ProviderOption[] = [
  {
    id: "mangadex",
    name: "MangaDex",
    badge: "MangaDex 🌐",
    languageGroup: "multi",
    descriptionEs: "Catálogo internacional multilingüe con metadatos completos.",
    descriptionEn: "International multi-language catalog with rich metadata.",
  },
  {
    id: "inmanga",
    name: "InManga",
    badge: "InManga 🇪🇸",
    languageGroup: "es",
    descriptionEs: "Recomendado en español. Catálogo amplio y carga rápida.",
    descriptionEn: "Recommended for Spanish readers. Wide catalog and fast loading.",
  },
  {
    id: "mangaoni",
    name: "MangaOni",
    badge: "MangaOni 🇪🇸",
    languageGroup: "es",
    descriptionEs: "Alternativa en español con buen catálogo de series populares.",
    descriptionEn: "Spanish alternative with a solid selection of popular series.",
  },
  {
    id: "leercapitulo",
    name: "LeerCapítulo",
    badge: "LeerCapítulo 🇪🇸",
    languageGroup: "es",
    descriptionEs: "Fuente adicional en español para capítulos recientes.",
    descriptionEn: "Additional Spanish source for recent chapter releases.",
  },
  {
    id: "weebcentral",
    name: "WeebCentral",
    badge: "WeebCentral 🇬🇧",
    languageGroup: "en",
    descriptionEs: "Recomendado en inglés. Gran catálogo en alta calidad.",
    descriptionEn: "Recommended for English readers. Large high-quality catalog.",
  },
  {
    id: "mangapill",
    name: "MangaPill",
    badge: "MangaPill 🇬🇧",
    languageGroup: "en",
    descriptionEs: "Fuente rápida en inglés con buena disponibilidad de capítulos.",
    descriptionEn: "Fast English source with reliable chapter availability.",
  },
  {
    id: "comizy",
    name: "Comizy",
    badge: "Comizy 🇬🇧",
    languageGroup: "en",
    descriptionEs: "Fuente complementaria en inglés con lanzamientos recientes.",
    descriptionEn: "Complementary English source for recent releases.",
  },
];

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function defaultBrowserState(): InitialAppState {
  return {
    appVersion: "0.1.0",
    uiLanguage: "es",
    themeMode: "paper",
    storageMode: "system",
    storageModeLabel: "Browser Preview",
    dataDir: "AppData/manga-tui/data",
    configDir: "AppData/manga-tui/config",
    downloadsDir: "AppData/manga-tui/data/mangaDownloads",
    activeProvider: "inmanga",
    recommendedProvider: "inmanga",
    availableProviders: FALLBACK_PROVIDERS,
    favoriteGenres: [],
    onboardingCompleted: false,
    library: {
      hasExistingData: false,
      libraryMangas: 0,
      chaptersRead: 0,
      favorites: 0,
      downloadedChapters: 0,
      offlineSeriesCount: 0,
      lastReadTitle: null,
    },
  };
}

function readBrowserState(): InitialAppState {
  const base = defaultBrowserState();
  try {
    const raw = window.localStorage.getItem(BROWSER_STORAGE_KEY);
    if (!raw) return base;
    const parsed = JSON.parse(raw) as Partial<InitialAppState>;
    return {
      ...base,
      ...parsed,
      availableProviders: FALLBACK_PROVIDERS,
    };
  } catch {
    return base;
  }
}

function writeBrowserState(next: InitialAppState): InitialAppState {
  try {
    window.localStorage.setItem(BROWSER_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Ignore storage quota errors in fallback mode
  }
  return next;
}

function recommendedForLang(lang: UiLanguage): string {
  return lang === "es" ? "inmanga" : "weebcentral";
}

export async function getInitialState(): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("get_initial_state");
  }
  return readBrowserState();
}

export async function saveOnboardingPreferences(
  payload: OnboardingPreferencesPayload,
): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("save_onboarding_preferences", { payload });
  }
  const current = readBrowserState();
  return writeBrowserState({
    ...current,
    uiLanguage: payload.uiLanguage,
    favoriteGenres: payload.favoriteGenres,
    activeProvider: payload.provider,
    recommendedProvider: recommendedForLang(payload.uiLanguage),
    onboardingCompleted: payload.onboardingCompleted,
    themeMode: payload.themeMode ?? current.themeMode,
  });
}

export async function setUiLanguage(language: UiLanguage): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_ui_language", { language });
  }
  const current = readBrowserState();
  return writeBrowserState({
    ...current,
    uiLanguage: language,
    recommendedProvider: recommendedForLang(language),
  });
}

export async function setThemeMode(themeMode: ThemeMode): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_theme_mode", { themeMode });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, themeMode });
}

export async function setDefaultProvider(provider: string): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_default_provider", { provider });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, activeProvider: provider });
}

export async function setFavoriteGenres(genres: string[]): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_favorite_genres", { genres });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, favoriteGenres: genres });
}

export async function setOnboardingCompleted(completed: boolean): Promise<InitialAppState> {
  if (isTauriRuntime()) {
    return invoke<InitialAppState>("set_onboarding_completed", { completed });
  }
  const current = readBrowserState();
  return writeBrowserState({ ...current, onboardingCompleted: completed });
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauriRuntime()) {
    await invoke<void>("open_external_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export async function openDataFolder(): Promise<void> {
  if (isTauriRuntime()) {
    await invoke<void>("open_data_folder");
  }
}
