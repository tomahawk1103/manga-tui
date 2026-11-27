import { DEFAULT_APPEARANCE_CONFIG } from "../features/hito5/palettes";
import type {
  DailyReadingActivity,
  ExtendedReaderMetrics,
  MangoraAppearanceConfig,
  ProviderReadingStat,
  RecordReadingSessionRequest,
  SaveMangaGenresRequest,
  TopMangaStat,
} from "../features/hito5/types";
import type {
  InitialAppState,
  LibraryEntry,
  LibraryOverview,
  OpenReaderChapterRequest,
  PortableBackupImportResultDto,
  ProviderOption,
  SaveReaderProgressPayload,
  SaveReaderProgressResult,
  ToggleChapterReadPayload,
  ToggleChapterReadResult,
  ToggleFavoritePayload,
  ToggleFavoriteResult,
  UiLanguage,
} from "../types/mangora";

const DB_NAME = "mangora_web_db";
const DB_VERSION = 1;

const LEGACY_STATE_KEY = "mangora_browser_fallback_v1";
const LEGACY_FAVORITES_KEY = "mangora_browser_favorites_v1";
const LEGACY_APPEARANCE_KEY = "mangora.hito5.appearance.v1";
const LEGACY_SESSIONS_KEY = "mangora.hito5.sessions.v1";
const LEGACY_GENRES_KEY = "mangora.hito5.genres.v1";
const LEGACY_SAMPLE_PROGRESS_KEY = "mangora-web-sample-chapter-v1";
const LEGACY_PROGRESS_PREFIX = "mangora-web-progress:";
const LOCAL_FALLBACK_DUMP_KEY = "mangora_web_db_fallback_v1";

export interface WebMangaRecord {
  key: string;
  id: string;
  provider: string;
  providerLabel: string;
  title: string;
  coverUrl: string | null;
  description: string | null;
  genres: string[];
  createdAt: string;
  updatedAt: string;
  lastRead: string | null;
  lastReadChapter: string | null;
  inReadingHistory: boolean;
  inPlanToRead: boolean;
  isFavorite: boolean;
  favoriteCreatedAt: string | null;
  shelfName: string | null;
}

export interface WebChapterRecord {
  key: string;
  mangaKey: string;
  id: string;
  mangaId: string;
  provider: string;
  title: string;
  chapterNumber: string;
  translatedLanguage: string;
  isRead: boolean;
  isBookmarked: boolean;
  numberPageBookmarked: number | null;
  totalPages: number | null;
  updatedAt: string;
}

export interface WebMangaChapterState {
  isFavorite: boolean;
  chaptersByKey: Map<string, WebChapterRecord>;
}

interface StoredKvEntry {
  key: string;
  value: unknown;
}

interface StoredAppPrefs {
  uiLanguage: UiLanguage;
  themeMode: InitialAppState["themeMode"];
  activeProvider: string;
  favoriteGenres: string[];
  onboardingCompleted: boolean;
  downloadFormat: InitialAppState["downloadFormat"];
  supportBannerDismissed: boolean;
  readerPrefs: InitialAppState["readerPrefs"];
}

interface SyncMangaRecord {
  id: string;
  title: string;
  created_at: string | null;
  updated_at: string | null;
  last_read: string | null;
  deleted_at: string | null;
  img_url: string | null;
  manga_provider: string;
  in_reading_history: boolean;
  in_plan_to_read: boolean;
}

interface SyncChapterRecord {
  id: string;
  title: string;
  manga_id: string;
  is_read: boolean;
  is_downloaded: boolean;
  is_bookmarked: boolean;
  translated_language: string | null;
  number_page_bookmarked: number | null;
}

interface SyncFavoriteRecord {
  manga_id: string;
  provider: string;
  title: string;
  created_at: string | null;
}

interface SyncCollectionRecord {
  manga_id: string;
  provider: string;
  title: string;
  shelf_name: string;
  created_at: string | null;
}

interface SyncMangaGenreRecord {
  manga_id: string;
  provider: string;
  genre: string;
  created_at: string | null;
}

interface LibrarySyncSnapshot {
  schema_version: number;
  app_version: string;
  synced_at: string;
  mangas: SyncMangaRecord[];
  chapters: SyncChapterRecord[];
  favorites: SyncFavoriteRecord[];
  dismissed_notifications: unknown[];
  search_history: unknown[];
  collections: SyncCollectionRecord[];
  manga_genres: SyncMangaGenreRecord[];
}

interface MemoryStore {
  kv: Map<string, unknown>;
  mangas: Map<string, WebMangaRecord>;
  chapters: Map<string, WebChapterRecord>;
  sessions: Array<RecordReadingSessionRequest & { id: number }>;
  nextSessionId: number;
}

export const WEB_BUILD_STAMP = "2026.11.27-r8";

const memoryStore: MemoryStore = {
  kv: new Map(),
  mangas: new Map(),
  chapters: new Map(),
  sessions: [],
  nextSessionId: 1,
};

let memoryLoaded = false;
let forceMemoryFallback = false;
let dbPromise: Promise<IDBDatabase | null> | null = null;
let migrationPromise: Promise<void> | null = null;

function hasIndexedDb(): boolean {
  return typeof window !== "undefined" && typeof window.indexedDB !== "undefined";
}

function getTimerApi(): {
  setTimeoutFn: typeof globalThis.setTimeout;
  clearTimeoutFn: typeof globalThis.clearTimeout;
} {
  if (typeof window !== "undefined" && typeof window.setTimeout === "function") {
    return {
      setTimeoutFn: window.setTimeout.bind(window),
      clearTimeoutFn: window.clearTimeout.bind(window),
    };
  }
  return {
    setTimeoutFn: globalThis.setTimeout.bind(globalThis),
    clearTimeoutFn: globalThis.clearTimeout.bind(globalThis),
  };
}

function withIdbTimeout<T>(promise: Promise<T>, ms = 1800): Promise<T> {
  const { setTimeoutFn, clearTimeoutFn } = getTimerApi();
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeoutFn(() => {
      reject(new Error(`IndexedDB operation timed out after ${ms}ms`));
    }, ms);
    promise.then(
      (val) => {
        clearTimeoutFn(timer);
        resolve(val);
      },
      (err) => {
        clearTimeoutFn(timer);
        reject(err);
      },
    );
  });
}

function loadMemoryFallbackFromStorage(): void {
  if (memoryLoaded || typeof window === "undefined") return;
  memoryLoaded = true;
  try {
    const raw = window.localStorage.getItem(LOCAL_FALLBACK_DUMP_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw) as {
      kv?: Array<[string, unknown]>;
      mangas?: WebMangaRecord[];
      chapters?: WebChapterRecord[];
      sessions?: Array<RecordReadingSessionRequest & { id: number }>;
      nextSessionId?: number;
    };
    if (Array.isArray(parsed.kv)) {
      for (const [k, v] of parsed.kv) memoryStore.kv.set(k, v);
    }
    if (Array.isArray(parsed.mangas)) {
      for (const m of parsed.mangas) memoryStore.mangas.set(m.key, m);
    }
    if (Array.isArray(parsed.chapters)) {
      for (const c of parsed.chapters) memoryStore.chapters.set(c.key, c);
    }
    if (Array.isArray(parsed.sessions)) {
      memoryStore.sessions = parsed.sessions;
    }
    if (typeof parsed.nextSessionId === "number") {
      memoryStore.nextSessionId = parsed.nextSessionId;
    }
  } catch {
    // Ignore malformed fallback dump
  }
}

function persistMemoryFallbackToStorage(): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(
      LOCAL_FALLBACK_DUMP_KEY,
      JSON.stringify({
        kv: Array.from(memoryStore.kv.entries()),
        mangas: Array.from(memoryStore.mangas.values()),
        chapters: Array.from(memoryStore.chapters.values()),
        sessions: memoryStore.sessions,
        nextSessionId: memoryStore.nextSessionId,
      }),
    );
  } catch {
    // Ignore quota errors
  }
}

function openDb(): Promise<IDBDatabase | null> {
  if (forceMemoryFallback || !hasIndexedDb()) {
    loadMemoryFallbackFromStorage();
    return Promise.resolve(null);
  }
  if (dbPromise) return dbPromise;

  dbPromise = new Promise((resolve) => {
    const { setTimeoutFn, clearTimeoutFn } = getTimerApi();
    let settled = false;
    const finish = (db: IDBDatabase | null) => {
      if (settled) return;
      settled = true;
      if (!db) {
        forceMemoryFallback = true;
        loadMemoryFallbackFromStorage();
      }
      resolve(db);
    };
    const timer = setTimeoutFn(() => finish(null), 1500);
    try {
      const request = window.indexedDB.open(DB_NAME, DB_VERSION);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("kv")) {
          db.createObjectStore("kv", { keyPath: "key" });
        }
        if (!db.objectStoreNames.contains("mangas")) {
          db.createObjectStore("mangas", { keyPath: "key" });
        }
        if (!db.objectStoreNames.contains("chapters")) {
          const store = db.createObjectStore("chapters", { keyPath: "key" });
          store.createIndex("byManga", "mangaKey", { unique: false });
        }
        if (!db.objectStoreNames.contains("sessions")) {
          db.createObjectStore("sessions", { keyPath: "id", autoIncrement: true });
        }
      };
      request.onsuccess = () => {
        clearTimeoutFn(timer);
        const db = request.result;
        db.onversionchange = () => {
          db.close();
          dbPromise = null;
        };
        finish(db);
      };
      request.onerror = () => {
        clearTimeoutFn(timer);
        finish(null);
      };
      request.onblocked = () => {
        clearTimeoutFn(timer);
        finish(null);
      };
    } catch {
      clearTimeoutFn(timer);
      finish(null);
    }
  });
  return dbPromise;
}

function reqPromise<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("IndexedDB request error"));
  });
}

function txDone(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error("IndexedDB transaction error"));
    tx.onabort = () => reject(tx.error ?? new Error("IndexedDB transaction aborted"));
  });
}

async function runWithIdbFallback<T>(
  idbOp: (db: IDBDatabase) => Promise<T>,
  memOp: () => T,
): Promise<T> {
  const db = await openDb();
  if (!db) return memOp();
  try {
    return await withIdbTimeout(idbOp(db));
  } catch (err) {
    console.warn("[Mangora Web] IndexedDB fallback to memory store:", err);
    forceMemoryFallback = true;
    loadMemoryFallbackFromStorage();
    return memOp();
  }
}

async function kvGet<T>(key: string): Promise<T | undefined> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("kv", "readonly");
      const entry = (await reqPromise(tx.objectStore("kv").get(key))) as StoredKvEntry | undefined;
      return entry?.value as T | undefined;
    },
    () => memoryStore.kv.get(key) as T | undefined,
  );
}

async function kvSet(key: string, value: unknown): Promise<void> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("kv", "readwrite");
      const done = txDone(tx);
      tx.objectStore("kv").put({ key, value } satisfies StoredKvEntry);
      await done;
    },
    () => {
      memoryStore.kv.set(key, value);
      persistMemoryFallbackToStorage();
    },
  );
}

async function getAllMangas(): Promise<WebMangaRecord[]> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("mangas", "readonly");
      return (await reqPromise(tx.objectStore("mangas").getAll())) as WebMangaRecord[];
    },
    () => Array.from(memoryStore.mangas.values()),
  );
}

async function getManga(key: string): Promise<WebMangaRecord | undefined> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("mangas", "readonly");
      return (await reqPromise(tx.objectStore("mangas").get(key))) as WebMangaRecord | undefined;
    },
    () => memoryStore.mangas.get(key),
  );
}

async function putManga(record: WebMangaRecord): Promise<void> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("mangas", "readwrite");
      const done = txDone(tx);
      tx.objectStore("mangas").put(record);
      await done;
    },
    () => {
      memoryStore.mangas.set(record.key, record);
      persistMemoryFallbackToStorage();
    },
  );
}

async function getAllChapters(): Promise<WebChapterRecord[]> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("chapters", "readonly");
      return (await reqPromise(tx.objectStore("chapters").getAll())) as WebChapterRecord[];
    },
    () => Array.from(memoryStore.chapters.values()),
  );
}

async function getChaptersByMangaKey(mKey: string): Promise<WebChapterRecord[]> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("chapters", "readonly");
      const index = tx.objectStore("chapters").index("byManga");
      return (await reqPromise(index.getAll(mKey))) as WebChapterRecord[];
    },
    () => Array.from(memoryStore.chapters.values()).filter((ch) => ch.mangaKey === mKey),
  );
}

async function getChapter(key: string): Promise<WebChapterRecord | undefined> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("chapters", "readonly");
      return (await reqPromise(tx.objectStore("chapters").get(key))) as WebChapterRecord | undefined;
    },
    () => memoryStore.chapters.get(key),
  );
}

async function putChapter(record: WebChapterRecord): Promise<void> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("chapters", "readwrite");
      const done = txDone(tx);
      tx.objectStore("chapters").put(record);
      await done;
    },
    () => {
      memoryStore.chapters.set(record.key, record);
      persistMemoryFallbackToStorage();
    },
  );
}

async function getAllSessions(): Promise<RecordReadingSessionRequest[]> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("sessions", "readonly");
      return (await reqPromise(tx.objectStore("sessions").getAll())) as RecordReadingSessionRequest[];
    },
    () => memoryStore.sessions,
  );
}

async function addSessionRecord(session: RecordReadingSessionRequest): Promise<number> {
  return runWithIdbFallback(
    async (db) => {
      const tx = db.transaction("sessions", "readwrite");
      const done = txDone(tx);
      tx.objectStore("sessions").add({
        ...session,
        started_at: session.started_at ?? new Date().toISOString(),
      });
      await done;
      const all = await getAllSessions();
      return all.length;
    },
    () => {
      const id = memoryStore.nextSessionId++;
      memoryStore.sessions.push({
        ...session,
        started_at: session.started_at ?? new Date().toISOString(),
        id,
      });
      persistMemoryFallbackToStorage();
      return memoryStore.sessions.length;
    },
  );
}

export function normalizeProviderId(raw: string): string {
  return raw.trim().toLowerCase() || "mangaoni";
}

export function mangaKey(provider: string, mangaId: string): string {
  return `${normalizeProviderId(provider)}::${mangaId.trim()}`;
}

export function chapterKey(provider: string, mangaId: string, chapterId: string): string {
  return `${normalizeProviderId(provider)}::${mangaId.trim()}::${chapterId.trim()}`;
}

function providerLabelFor(provider: string, availableProviders?: ProviderOption[]): string {
  const norm = normalizeProviderId(provider);
  const found = availableProviders?.find((item) => item.id === norm);
  if (found) return found.badge;
  if (norm === "mangadex") return "MangaDex 🌐";
  if (norm === "mangaoni") return "MangaOni 🇪🇸";
  if (norm === "inmanga") return "InManga 🇪🇸";
  if (norm === "weebcentral") return "WeebCentral 🇬🇧";
  return provider;
}

function detectDefaultLanguage(): UiLanguage {
  if (typeof navigator !== "undefined" && navigator.language?.toLowerCase().startsWith("en")) {
    return "en";
  }
  return "es";
}

function detectIsMobile(): boolean {
  if (typeof window === "undefined") return false;
  const smallViewport = window.matchMedia?.("(max-width: 767px)")?.matches ?? false;
  const hasTouch = typeof navigator !== "undefined" && (navigator.maxTouchPoints ?? 0) > 0;
  return smallViewport || hasTouch;
}

function defaultWebPrefs(availableProviders: ProviderOption[]): StoredAppPrefs {
  const lang = detectDefaultLanguage();
  const hasMangadex = availableProviders.some((p) => p.id === "mangadex");
  const recommended = lang === "en" && hasMangadex ? "mangadex" : "mangaoni";
  return {
    uiLanguage: lang,
    themeMode: "paper",
    activeProvider: recommended,
    favoriteGenres: [],
    onboardingCompleted: true,
    downloadFormat: "cbz",
    supportBannerDismissed: false,
    readerPrefs: {
      viewMode: "cascade",
      direction: "rtl",
      fitMode: "fitWidth",
      pagedFitMode: "fitHeight",
      zoomPercent: 100,
      cascadeGapPx: 8,
      firstPageSingle: true,
      pinToolbars: false,
      darkCanvas: true,
    },
  };
}

export function ensureMigrated(): Promise<void> {
  if (migrationPromise) return migrationPromise;
  const run = (async () => {
    try {
      const done = await kvGet<boolean>("migrated_v1");
      if (done) return;
      if (typeof window === "undefined") {
        await kvSet("migrated_v1", true);
        return;
      }

      const nowIso = new Date().toISOString();

      // 1. Legacy app prefs
      try {
        const rawState = window.localStorage.getItem(LEGACY_STATE_KEY);
        if (rawState) {
          const parsed = JSON.parse(rawState) as Partial<InitialAppState>;
          const prefs: StoredAppPrefs = {
            uiLanguage: parsed.uiLanguage === "en" ? "en" : "es",
            themeMode: parsed.themeMode === "ink" ? "ink" : "paper",
            activeProvider: parsed.activeProvider ?? "mangaoni",
            favoriteGenres: Array.isArray(parsed.favoriteGenres) ? parsed.favoriteGenres : [],
            onboardingCompleted: true,
            downloadFormat: parsed.downloadFormat ?? "cbz",
            supportBannerDismissed: Boolean(parsed.supportBannerDismissed),
            readerPrefs: parsed.readerPrefs ?? defaultWebPrefs([]).readerPrefs,
          };
          await kvSet("app_state", prefs);
        }
      } catch {
        // Ignore malformed legacy state
      }

      // 2. Legacy favorites
      try {
        const rawFavs = window.localStorage.getItem(LEGACY_FAVORITES_KEY);
        if (rawFavs) {
          const favs = JSON.parse(rawFavs) as LibraryEntry[];
          if (Array.isArray(favs)) {
            for (const fav of favs) {
              if (!fav?.id || !fav?.provider) continue;
              const mKey = mangaKey(fav.provider, fav.id);
              await putManga({
                key: mKey,
                id: fav.id,
                provider: normalizeProviderId(fav.provider),
                providerLabel: fav.providerLabel || providerLabelFor(fav.provider),
                title: fav.title || fav.id,
                coverUrl: fav.coverUrl ?? null,
                description: null,
                genres: [],
                createdAt: nowIso,
                updatedAt: nowIso,
                lastRead: fav.lastRead ?? null,
                lastReadChapter: fav.lastReadChapter ?? null,
                inReadingHistory: Boolean(fav.lastRead),
                inPlanToRead: Boolean(fav.isPlanToRead),
                isFavorite: true,
                favoriteCreatedAt: nowIso,
                shelfName: fav.shelfName ?? null,
              });
            }
          }
        }
      } catch {
        // Ignore malformed legacy favorites
      }

      // 3. Legacy Hito 5 appearance, genres, sessions
      try {
        const rawAppearance = window.localStorage.getItem(LEGACY_APPEARANCE_KEY);
        if (rawAppearance) {
          await kvSet("appearance", {
            ...DEFAULT_APPEARANCE_CONFIG,
            ...(JSON.parse(rawAppearance) as Partial<MangoraAppearanceConfig>),
          });
        }
      } catch {
        // Ignore
      }

      try {
        const rawGenres = window.localStorage.getItem(LEGACY_GENRES_KEY);
        if (rawGenres) {
          const map = JSON.parse(rawGenres) as Record<string, string[]>;
          for (const [compoundKey, genres] of Object.entries(map)) {
            const idx = compoundKey.indexOf(":");
            if (idx <= 0 || !Array.isArray(genres)) continue;
            const provider = compoundKey.slice(0, idx);
            const mangaId = compoundKey.slice(idx + 1);
            const mKey = mangaKey(provider, mangaId);
            const existing = await getManga(mKey);
            if (existing) {
              existing.genres = Array.from(new Set([...existing.genres, ...genres]));
              await putManga(existing);
            }
          }
        }
      } catch {
        // Ignore
      }

      try {
        const rawSessions = window.localStorage.getItem(LEGACY_SESSIONS_KEY);
        if (rawSessions) {
          const list = JSON.parse(rawSessions) as RecordReadingSessionRequest[];
          if (Array.isArray(list)) {
            for (const item of list) {
              await addSessionRecord(item);
            }
          }
        }
      } catch {
        // Ignore
      }

      // 4. Legacy mangora-web chapter progress (use internal writer to avoid awaiting ensureMigrated recursively)
      const localKeys: string[] = [];
      try {
        for (let i = 0; i < window.localStorage.length; i += 1) {
          const k = window.localStorage.key(i);
          if (k) localKeys.push(k);
        }
      } catch {
        // Ignore storage iteration error
      }

      for (const k of localKeys) {
        try {
          if (k === LEGACY_SAMPLE_PROGRESS_KEY) {
            const val = Number.parseInt(window.localStorage.getItem(k) ?? "", 10);
            if (Number.isFinite(val)) {
              await saveWebReaderProgressInternal({
                mangaId: "mangora-sample",
                provider: "mangaoni",
                mangaTitle: "Manga de muestra",
                coverUrl: null,
                chapterId: "sample-1",
                chapterTitle: "Capítulo 1",
                language: "es",
                pageIndex: val,
                totalPages: 3,
              });
            }
          } else if (k.startsWith(LEGACY_PROGRESS_PREFIX)) {
            const tupleRaw = k.slice(LEGACY_PROGRESS_PREFIX.length);
            const [seriesId, chapId] = JSON.parse(tupleRaw) as [string, string];
            const val = Number.parseInt(window.localStorage.getItem(k) ?? "", 10);
            if (seriesId && chapId && Number.isFinite(val)) {
              const fullChapterId = chapId.includes("|") ? chapId : `${seriesId}|${chapId}`;
              await saveWebReaderProgressInternal({
                mangaId: seriesId,
                provider: "mangaoni",
                mangaTitle: seriesId.split("/").pop() ?? seriesId,
                coverUrl: null,
                chapterId: fullChapterId,
                chapterTitle: `Cap. ${chapId}`,
                language: "es",
                pageIndex: val,
              });
            }
          }
        } catch {
          // Ignore malformed legacy progress entry
        }
      }

      await kvSet("migrated_v1", true);
    } catch (err) {
      console.warn("[Mangora Web] Legacy migration skipped due to error:", err);
    }
  })();

  migrationPromise = new Promise<void>((resolve) => {
    const { setTimeoutFn, clearTimeoutFn } = getTimerApi();
    const timer = setTimeoutFn(() => {
      console.warn("[Mangora Web] Migration timed out after 2200ms; continuing startup.");
      resolve();
    }, 2200);
    void run.finally(() => {
      clearTimeoutFn(timer);
      resolve();
    });
  });

  return migrationPromise;
}

function validateProvider(candidate: string, availableProviders: ProviderOption[], fallback: string): string {
  const norm = normalizeProviderId(candidate);
  if (availableProviders.length === 0) return norm;
  if (availableProviders.some((p) => p.id === norm)) return norm;
  if (availableProviders.some((p) => p.id === fallback)) return fallback;
  return availableProviders[0].id;
}

function buildInitialAppStateFromData(params: {
  availableProviders: ProviderOption[];
  stored: Partial<StoredAppPrefs>;
  mangas: WebMangaRecord[];
  chapters: WebChapterRecord[];
  sessions: RecordReadingSessionRequest[];
  storageModeLabel?: string;
}): InitialAppState {
  const { availableProviders, stored, mangas, chapters, sessions, storageModeLabel } = params;
  const base = defaultWebPrefs(availableProviders);
  const uiLanguage: UiLanguage = stored.uiLanguage === "en" ? "en" : stored.uiLanguage === "es" ? "es" : base.uiLanguage;
  const hasMangadex = availableProviders.some((p) => p.id === "mangadex");
  const recommendedProvider = uiLanguage === "en" && hasMangadex ? "mangadex" : "mangaoni";
  const activeProvider = validateProvider(
    stored.activeProvider ?? base.activeProvider,
    availableProviders,
    recommendedProvider,
  );

  const readChapters = chapters.filter((c) => c.isRead);
  const favoritesCount = mangas.filter((m) => m.isFavorite).length;
  const libraryMangas = mangas.filter((m) => m.isFavorite || m.inReadingHistory || m.inPlanToRead).length;
  const sessionPages = sessions.reduce((acc, s) => acc + Math.max(0, s.pages_read || 0), 0);
  const bookmarkedPages = chapters.reduce(
    (acc, c) => acc + (c.numberPageBookmarked !== null ? c.numberPageBookmarked + 1 : 0),
    0,
  );
  const sortedByRead = [...mangas]
    .filter((m) => m.lastRead)
    .sort((a, b) => (b.lastRead ?? "").localeCompare(a.lastRead ?? ""));

  return {
    appVersion: "0.1.1",
    uiLanguage,
    themeMode: stored.themeMode === "ink" ? "ink" : "paper",
    platform: "unknown",
    isMobile: detectIsMobile(),
    canOpenDataFolder: false,
    storageMode: "sandbox",
    storageModeLabel:
      storageModeLabel ?? (uiLanguage === "en" ? "IndexedDB (Browser)" : "IndexedDB (Navegador)"),
    dataDir: "IndexedDB · mangora_web_db",
    configDir: "IndexedDB · app_state",
    downloadsDir: uiLanguage === "en" ? "Native app (Windows / Android)" : "App nativa (Windows / Android)",
    activeProvider,
    recommendedProvider,
    availableProviders,
    favoriteGenres: Array.isArray(stored.favoriteGenres) ? stored.favoriteGenres : [],
    onboardingCompleted: stored.onboardingCompleted ?? true,
    downloadFormat: stored.downloadFormat ?? "cbz",
    supportBannerDismissed: Boolean(stored.supportBannerDismissed),
    readerPrefs: {
      ...base.readerPrefs,
      ...(stored.readerPrefs ?? {}),
    },
    library: {
      hasExistingData: libraryMangas > 0 || readChapters.length > 0 || favoritesCount > 0,
      libraryMangas,
      chaptersRead: readChapters.length,
      totalPagesRead: Math.max(sessionPages, bookmarkedPages),
      favorites: favoritesCount,
      downloadedChapters: 0,
      offlineSeriesCount: 0,
      lastReadTitle: sortedByRead[0]?.title ?? null,
    },
  };
}

export function buildEmergencyWebAppState(availableProviders: ProviderOption[]): InitialAppState {
  loadMemoryFallbackFromStorage();
  const stored = (memoryStore.kv.get("app_state") as Partial<StoredAppPrefs> | undefined) ?? {};
  return buildInitialAppStateFromData({
    availableProviders,
    stored,
    mangas: Array.from(memoryStore.mangas.values()),
    chapters: Array.from(memoryStore.chapters.values()),
    sessions: memoryStore.sessions,
    storageModeLabel: detectDefaultLanguage() === "en" ? "Memory fallback (Browser)" : "Memoria local (Navegador)",
  });
}

export async function resetWebLocalDatabase(): Promise<void> {
  forceMemoryFallback = true;
  if (dbPromise) {
    try {
      const db = await withIdbTimeout(dbPromise, 500);
      db?.close();
    } catch {
      // Ignore close error
    }
    dbPromise = null;
  }
  migrationPromise = null;
  memoryStore.kv.clear();
  memoryStore.mangas.clear();
  memoryStore.chapters.clear();
  memoryStore.sessions = [];
  memoryStore.nextSessionId = 1;
  memoryLoaded = false;

  if (typeof window !== "undefined") {
    try {
      window.localStorage.removeItem(LOCAL_FALLBACK_DUMP_KEY);
    } catch {
      // Ignore storage error
    }
    if (hasIndexedDb()) {
      await new Promise<void>((resolve) => {
        const { setTimeoutFn, clearTimeoutFn } = getTimerApi();
        const timer = setTimeoutFn(resolve, 1000);
        try {
          const req = window.indexedDB.deleteDatabase(DB_NAME);
          req.onsuccess = () => {
            clearTimeoutFn(timer);
            resolve();
          };
          req.onerror = () => {
            clearTimeoutFn(timer);
            resolve();
          };
          req.onblocked = () => {
            clearTimeoutFn(timer);
            resolve();
          };
        } catch {
          clearTimeoutFn(timer);
          resolve();
        }
      });
    }
  }
}

export async function getWebAppState(availableProviders: ProviderOption[]): Promise<InitialAppState> {
  await ensureMigrated();
  const [stored, mangas, chapters, sessions] = await Promise.all([
    kvGet<Partial<StoredAppPrefs>>("app_state"),
    getAllMangas(),
    getAllChapters(),
    getAllSessions(),
  ]);
  return buildInitialAppStateFromData({
    availableProviders,
    stored: stored ?? {},
    mangas,
    chapters,
    sessions,
  });
}

export async function patchWebAppState(
  patch: Partial<StoredAppPrefs>,
  availableProviders: ProviderOption[],
): Promise<InitialAppState> {
  await ensureMigrated();
  const base = defaultWebPrefs(availableProviders);
  const stored = (await kvGet<Partial<StoredAppPrefs>>("app_state")) ?? {};
  const next: StoredAppPrefs = {
    ...base,
    ...stored,
    ...patch,
    readerPrefs: patch.readerPrefs
      ? { ...base.readerPrefs, ...(stored.readerPrefs ?? {}), ...patch.readerPrefs }
      : { ...base.readerPrefs, ...(stored.readerPrefs ?? {}) },
  };
  await kvSet("app_state", next);
  return getWebAppState(availableProviders);
}

export async function getWebLibraryOverview(
  search?: string,
  availableProviders?: ProviderOption[],
): Promise<LibraryOverview> {
  await ensureMigrated();
  const [mangas, chapters] = await Promise.all([getAllMangas(), getAllChapters()]);
  const chaptersByManga = new Map<string, WebChapterRecord[]>();
  for (const ch of chapters) {
    const list = chaptersByManga.get(ch.mangaKey) ?? [];
    list.push(ch);
    chaptersByManga.set(ch.mangaKey, list);
  }

  const needle = search?.trim().toLowerCase() ?? "";
  const entries: LibraryEntry[] = [];

  for (const manga of mangas) {
    if (needle && !manga.title.toLowerCase().includes(needle)) continue;
    const chList = chaptersByManga.get(manga.key) ?? [];
    const readCount = chList.filter((c) => c.isRead).length;
    const bookmarked = chList
      .filter((c) => c.isBookmarked && c.numberPageBookmarked !== null)
      .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))[0];

    entries.push({
      id: manga.id,
      title: manga.title,
      provider: manga.provider,
      providerLabel: providerLabelFor(manga.provider, availableProviders),
      coverUrl: manga.coverUrl,
      lastRead: manga.lastRead,
      lastReadChapter: manga.lastReadChapter,
      totalReadCount: readCount,
      bookmarkChapter: bookmarked ? bookmarked.title || `Cap. ${bookmarked.chapterNumber}` : null,
      bookmarkPage: bookmarked && bookmarked.numberPageBookmarked !== null ? bookmarked.numberPageBookmarked + 1 : null,
      isFavorite: manga.isFavorite,
      isPlanToRead: manga.inPlanToRead,
      downloadedCount: 0,
      shelfName: manga.shelfName,
    });
  }

  const byLastRead = (a: LibraryEntry, b: LibraryEntry) => (b.lastRead ?? "").localeCompare(a.lastRead ?? "");

  const continueReading = entries
    .filter((entry) => Boolean(entry.lastRead) && (entry.totalReadCount > 0 || entry.bookmarkChapter !== null))
    .sort(byLastRead);
  const favorites = entries.filter((entry) => entry.isFavorite).sort(byLastRead);
  const recentHistory = entries.filter((entry) => Boolean(entry.lastRead)).sort(byLastRead);

  return {
    continueReading,
    favorites,
    recentHistory,
    totalContinueReading: continueReading.length,
    totalFavorites: favorites.length,
    totalRecent: recentHistory.length,
  };
}

export async function toggleWebFavorite(
  payload: ToggleFavoritePayload,
  availableProviders: ProviderOption[],
): Promise<ToggleFavoriteResult> {
  await ensureMigrated();
  const nowIso = new Date().toISOString();
  const mKey = mangaKey(payload.provider, payload.mangaId);
  const existing = await getManga(mKey);
  const nextFavorite = !(existing?.isFavorite ?? false);

  const record: WebMangaRecord = existing
    ? {
        ...existing,
        title: payload.title || existing.title,
        coverUrl: payload.coverUrl ?? existing.coverUrl,
        isFavorite: nextFavorite,
        favoriteCreatedAt: nextFavorite ? existing.favoriteCreatedAt ?? nowIso : null,
        updatedAt: nowIso,
      }
    : {
        key: mKey,
        id: payload.mangaId,
        provider: normalizeProviderId(payload.provider),
        providerLabel: providerLabelFor(payload.provider, availableProviders),
        title: payload.title,
        coverUrl: payload.coverUrl ?? null,
        description: null,
        genres: [],
        createdAt: nowIso,
        updatedAt: nowIso,
        lastRead: null,
        lastReadChapter: null,
        inReadingHistory: false,
        inPlanToRead: false,
        isFavorite: nextFavorite,
        favoriteCreatedAt: nextFavorite ? nowIso : null,
        shelfName: null,
      };

  await putManga(record);
  const [appState, library] = await Promise.all([
    getWebAppState(availableProviders),
    getWebLibraryOverview(undefined, availableProviders),
  ]);
  return { isFavorite: nextFavorite, appState, library };
}

async function ensureMangaEntry(params: {
  mangaId: string;
  provider: string;
  title: string;
  coverUrl?: string | null;
  lastReadChapter?: string | null;
  availableProviders?: ProviderOption[];
}): Promise<WebMangaRecord> {
  const nowIso = new Date().toISOString();
  const mKey = mangaKey(params.provider, params.mangaId);
  const existing = await getManga(mKey);
  const next: WebMangaRecord = existing
    ? {
        ...existing,
        title: params.title || existing.title,
        coverUrl: params.coverUrl ?? existing.coverUrl,
        lastRead: nowIso,
        lastReadChapter: params.lastReadChapter ?? existing.lastReadChapter,
        inReadingHistory: true,
        updatedAt: nowIso,
      }
    : {
        key: mKey,
        id: params.mangaId,
        provider: normalizeProviderId(params.provider),
        providerLabel: providerLabelFor(params.provider, params.availableProviders),
        title: params.title,
        coverUrl: params.coverUrl ?? null,
        description: null,
        genres: [],
        createdAt: nowIso,
        updatedAt: nowIso,
        lastRead: nowIso,
        lastReadChapter: params.lastReadChapter ?? null,
        inReadingHistory: true,
        inPlanToRead: false,
        isFavorite: false,
        favoriteCreatedAt: null,
        shelfName: null,
      };
  await putManga(next);
  return next;
}

export async function toggleWebChapterRead(
  payload: ToggleChapterReadPayload,
  availableProviders: ProviderOption[],
): Promise<ToggleChapterReadResult> {
  await ensureMigrated();
  const nowIso = new Date().toISOString();
  const mKey = mangaKey(payload.provider, payload.mangaId);
  const cKey = chapterKey(payload.provider, payload.mangaId, payload.chapterId);
  const existing = await getChapter(cKey);
  const nextRead = !(existing?.isRead ?? false);

  await ensureMangaEntry({
    mangaId: payload.mangaId,
    provider: payload.provider,
    title: payload.mangaTitle,
    coverUrl: payload.coverUrl,
    lastReadChapter: nextRead ? payload.chapterTitle || payload.chapterId : undefined,
    availableProviders,
  });

  await putChapter({
    key: cKey,
    mangaKey: mKey,
    id: payload.chapterId,
    mangaId: payload.mangaId,
    provider: normalizeProviderId(payload.provider),
    title: payload.chapterTitle,
    chapterNumber: existing?.chapterNumber ?? (payload.chapterTitle.match(/\d+(?:\.\d+)?/)?.[0] ?? ""),
    translatedLanguage: payload.translatedLanguage || existing?.translatedLanguage || "es",
    isRead: nextRead,
    isBookmarked: existing?.isBookmarked ?? false,
    numberPageBookmarked: existing?.numberPageBookmarked ?? null,
    totalPages: existing?.totalPages ?? null,
    updatedAt: nowIso,
  });

  const [appState, library] = await Promise.all([
    getWebAppState(availableProviders),
    getWebLibraryOverview(undefined, availableProviders),
  ]);
  return { isRead: nextRead, appState, library };
}

async function saveWebReaderProgressInternal(
  payload: SaveReaderProgressPayload,
): Promise<SaveReaderProgressResult> {
  const nowIso = new Date().toISOString();
  const mKey = mangaKey(payload.provider, payload.mangaId);
  const cKey = chapterKey(payload.provider, payload.mangaId, payload.chapterId);
  const existing = await getChapter(cKey);
  const totalPages = payload.totalPages ?? existing?.totalPages ?? null;
  const reachedEnd = totalPages !== null && totalPages > 0 && payload.pageIndex >= totalPages - 1;
  const isRead = Boolean(existing?.isRead || reachedEnd);

  await ensureMangaEntry({
    mangaId: payload.mangaId,
    provider: payload.provider,
    title: payload.mangaTitle,
    coverUrl: payload.coverUrl,
    lastReadChapter: payload.chapterTitle || payload.chapterId,
  });

  await putChapter({
    key: cKey,
    mangaKey: mKey,
    id: payload.chapterId,
    mangaId: payload.mangaId,
    provider: normalizeProviderId(payload.provider),
    title: payload.chapterTitle || existing?.title || payload.chapterId,
    chapterNumber: existing?.chapterNumber ?? (payload.chapterTitle.match(/\d+(?:\.\d+)?/)?.[0] ?? ""),
    translatedLanguage: payload.language || existing?.translatedLanguage || "es",
    isRead,
    isBookmarked: true,
    numberPageBookmarked: payload.pageIndex,
    totalPages,
    updatedAt: nowIso,
  });

  return { pageIndex: payload.pageIndex, isRead };
}

export async function saveWebReaderProgress(
  payload: SaveReaderProgressPayload,
): Promise<SaveReaderProgressResult> {
  await ensureMigrated();
  return saveWebReaderProgressInternal(payload);
}

export async function recordWebChapterOpen(
  request: OpenReaderChapterRequest,
  pageCount: number,
): Promise<number> {
  await ensureMigrated();
  const cKey = chapterKey(request.provider, request.mangaId, request.chapterId);
  const altKey = request.chapterId.includes("|")
    ? chapterKey(request.provider, request.mangaId, request.chapterId.split("|")[1])
    : null;

  const existing = (await getChapter(cKey)) ?? (altKey ? await getChapter(altKey) : undefined);

  await ensureMangaEntry({
    mangaId: request.mangaId,
    provider: request.provider,
    title: request.mangaTitle,
    coverUrl: request.coverUrl,
    lastReadChapter: request.chapterTitle ?? (request.chapterNumber ? `Cap. ${request.chapterNumber}` : request.chapterId),
  });

  const savedPage = existing?.numberPageBookmarked ?? 0;
  return Math.max(0, Math.min(Math.max(0, pageCount - 1), savedPage));
}

export async function getWebMangaChapterState(
  provider: string,
  mangaId: string,
): Promise<WebMangaChapterState> {
  await ensureMigrated();
  const mKey = mangaKey(provider, mangaId);
  const [manga, chapters] = await Promise.all([getManga(mKey), getChaptersByMangaKey(mKey)]);
  const chaptersByKey = new Map<string, WebChapterRecord>();
  for (const ch of chapters) {
    chaptersByKey.set(ch.id, ch);
    if (ch.id.includes("|")) {
      chaptersByKey.set(ch.id.split("|")[1], ch);
    }
  }
  return {
    isFavorite: Boolean(manga?.isFavorite),
    chaptersByKey,
  };
}

export async function exportWebLibraryBackupJson(appVersion: string): Promise<string> {
  await ensureMigrated();
  const [mangas, chapters] = await Promise.all([getAllMangas(), getAllChapters()]);

  const syncMangas: SyncMangaRecord[] = mangas.map((m) => ({
    id: m.id,
    title: m.title,
    created_at: m.createdAt,
    updated_at: m.updatedAt,
    last_read: m.lastRead,
    deleted_at: null,
    img_url: m.coverUrl,
    manga_provider: m.provider,
    in_reading_history: m.inReadingHistory,
    in_plan_to_read: m.inPlanToRead,
  }));

  const syncChapters: SyncChapterRecord[] = chapters.map((c) => ({
    id: c.id,
    title: c.title,
    manga_id: c.mangaId,
    is_read: c.isRead,
    is_downloaded: false,
    is_bookmarked: c.isBookmarked,
    translated_language: c.translatedLanguage || null,
    number_page_bookmarked: c.numberPageBookmarked,
  }));

  const syncFavorites: SyncFavoriteRecord[] = mangas
    .filter((m) => m.isFavorite)
    .map((m) => ({
      manga_id: m.id,
      provider: m.provider,
      title: m.title,
      created_at: m.favoriteCreatedAt ?? m.createdAt,
    }));

  const syncGenres: SyncMangaGenreRecord[] = mangas.flatMap((m) =>
    m.genres.map((genre) => ({
      manga_id: m.id,
      provider: m.provider,
      genre,
      created_at: m.createdAt,
    })),
  );

  const snapshot: LibrarySyncSnapshot = {
    schema_version: 1,
    app_version: appVersion,
    synced_at: new Date().toISOString(),
    mangas: syncMangas,
    chapters: syncChapters,
    favorites: syncFavorites,
    dismissed_notifications: [],
    search_history: [],
    collections: [],
    manga_genres: syncGenres,
  };

  return JSON.stringify(snapshot, null, 2);
}

export async function importWebLibraryBackupJson(
  jsonPayload: string,
  availableProviders: ProviderOption[],
): Promise<PortableBackupImportResultDto> {
  await ensureMigrated();
  const trimmed = jsonPayload.trim();
  if (!trimmed) throw new Error("El archivo de copia está vacío.");
  const parsed = JSON.parse(trimmed) as Partial<LibrarySyncSnapshot>;
  if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.mangas) || !Array.isArray(parsed.chapters)) {
    throw new Error("El archivo library_sync.json no tiene el formato esperado.");
  }

  const nowIso = new Date().toISOString();
  const mangaProviderById = new Map<string, string>();

  for (const m of parsed.mangas) {
    if (!m?.id || !m?.manga_provider) continue;
    const provider = normalizeProviderId(m.manga_provider);
    mangaProviderById.set(m.id, provider);
    const mKey = mangaKey(provider, m.id);
    const existing = await getManga(mKey);
    const mergedLastRead =
      [existing?.lastRead, m.last_read].filter((v): v is string => Boolean(v)).sort().pop() ?? null;
    await putManga({
      key: mKey,
      id: m.id,
      provider,
      providerLabel: providerLabelFor(provider, availableProviders),
      title: m.title || existing?.title || m.id,
      coverUrl: m.img_url ?? existing?.coverUrl ?? null,
      description: existing?.description ?? null,
      genres: existing?.genres ?? [],
      createdAt: existing?.createdAt ?? m.created_at ?? nowIso,
      updatedAt: nowIso,
      lastRead: mergedLastRead,
      lastReadChapter: existing?.lastReadChapter ?? null,
      inReadingHistory: Boolean(existing?.inReadingHistory || m.in_reading_history),
      inPlanToRead: Boolean(existing?.inPlanToRead || m.in_plan_to_read),
      isFavorite: existing?.isFavorite ?? false,
      favoriteCreatedAt: existing?.favoriteCreatedAt ?? null,
      shelfName: existing?.shelfName ?? null,
    });
  }

  for (const fav of parsed.favorites ?? []) {
    if (!fav?.manga_id || !fav?.provider) continue;
    const provider = normalizeProviderId(fav.provider);
    mangaProviderById.set(fav.manga_id, provider);
    const mKey = mangaKey(provider, fav.manga_id);
    const existing = await getManga(mKey);
    await putManga({
      key: mKey,
      id: fav.manga_id,
      provider,
      providerLabel: providerLabelFor(provider, availableProviders),
      title: fav.title || existing?.title || fav.manga_id,
      coverUrl: existing?.coverUrl ?? null,
      description: existing?.description ?? null,
      genres: existing?.genres ?? [],
      createdAt: existing?.createdAt ?? fav.created_at ?? nowIso,
      updatedAt: nowIso,
      lastRead: existing?.lastRead ?? null,
      lastReadChapter: existing?.lastReadChapter ?? null,
      inReadingHistory: existing?.inReadingHistory ?? false,
      inPlanToRead: existing?.inPlanToRead ?? false,
      isFavorite: true,
      favoriteCreatedAt: existing?.favoriteCreatedAt ?? fav.created_at ?? nowIso,
      shelfName: existing?.shelfName ?? null,
    });
  }

  for (const mg of parsed.manga_genres ?? []) {
    if (!mg?.manga_id || !mg?.provider || !mg?.genre) continue;
    const mKey = mangaKey(mg.provider, mg.manga_id);
    const existing = await getManga(mKey);
    if (existing && !existing.genres.some((g) => g.toLowerCase() === mg.genre.toLowerCase())) {
      existing.genres.push(mg.genre);
      await putManga(existing);
    }
  }

  for (const ch of parsed.chapters) {
    if (!ch?.id || !ch?.manga_id) continue;
    const provider = mangaProviderById.get(ch.manga_id) ?? "mangadex";
    const mKey = mangaKey(provider, ch.manga_id);
    const cKey = chapterKey(provider, ch.manga_id, ch.id);
    const existing = await getChapter(cKey);
    const mergedPage =
      ch.number_page_bookmarked !== null && existing?.numberPageBookmarked !== null && existing?.numberPageBookmarked !== undefined
        ? Math.max(ch.number_page_bookmarked, existing.numberPageBookmarked)
        : ch.number_page_bookmarked ?? existing?.numberPageBookmarked ?? null;
    await putChapter({
      key: cKey,
      mangaKey: mKey,
      id: ch.id,
      mangaId: ch.manga_id,
      provider,
      title: ch.title || existing?.title || ch.id,
      chapterNumber: existing?.chapterNumber ?? (ch.title?.match(/\d+(?:\.\d+)?/)?.[0] ?? ""),
      translatedLanguage: ch.translated_language ?? existing?.translatedLanguage ?? "es",
      isRead: Boolean(existing?.isRead || ch.is_read),
      isBookmarked: Boolean(existing?.isBookmarked || ch.is_bookmarked),
      numberPageBookmarked: mergedPage,
      totalPages: existing?.totalPages ?? null,
      updatedAt: nowIso,
    });
  }

  const [allMangas, allChapters, appState, library] = await Promise.all([
    getAllMangas(),
    getAllChapters(),
    getWebAppState(availableProviders),
    getWebLibraryOverview(undefined, availableProviders),
  ]);

  return {
    totalMangas: allMangas.length,
    totalReadChapters: allChapters.filter((c) => c.isRead).length,
    totalFavorites: allMangas.filter((m) => m.isFavorite).length,
    appState,
    library,
  };
}

export async function getWebAppearanceConfig(): Promise<MangoraAppearanceConfig> {
  await ensureMigrated();
  const stored = await kvGet<Partial<MangoraAppearanceConfig>>("appearance");
  return { ...DEFAULT_APPEARANCE_CONFIG, ...(stored ?? {}) };
}

export async function saveWebAppearanceConfig(config: MangoraAppearanceConfig): Promise<MangoraAppearanceConfig> {
  await ensureMigrated();
  await kvSet("appearance", config);
  return config;
}

export async function recordWebReadingSession(request: RecordReadingSessionRequest): Promise<number> {
  await ensureMigrated();
  return addSessionRecord(request);
}

export async function saveWebMangaGenres(request: SaveMangaGenresRequest): Promise<number> {
  await ensureMigrated();
  const mKey = mangaKey(request.provider, request.manga_id);
  const existing = await getManga(mKey);
  if (!existing) return 0;
  const known = new Set(existing.genres.map((g) => g.toLowerCase()));
  let added = 0;
  for (const raw of request.genres) {
    const trimmed = raw.trim();
    if (trimmed && !known.has(trimmed.toLowerCase())) {
      known.add(trimmed.toLowerCase());
      existing.genres.push(trimmed);
      added += 1;
    }
  }
  if (added > 0) {
    existing.updatedAt = new Date().toISOString();
    await putManga(existing);
  }
  return added;
}

function computeStreaks(dates: Set<string>, todayIso: string): { current: number; longest: number } {
  if (dates.size === 0) return { current: 0, longest: 0 };
  const sorted = Array.from(dates).sort();
  let longest = 1;
  let run = 1;
  for (let i = 1; i < sorted.length; i += 1) {
    const prev = Date.parse(`${sorted[i - 1]}T00:00:00Z`);
    const cur = Date.parse(`${sorted[i]}T00:00:00Z`);
    if (Math.round((cur - prev) / 86_400_000) === 1) {
      run += 1;
      if (run > longest) longest = run;
    } else {
      run = 1;
    }
  }

  let current = 0;
  const cursor = new Date(`${todayIso}T00:00:00Z`);
  if (!dates.has(todayIso)) {
    cursor.setUTCDate(cursor.getUTCDate() - 1);
  }
  while (dates.has(cursor.toISOString().slice(0, 10))) {
    current += 1;
    cursor.setUTCDate(cursor.getUTCDate() - 1);
  }
  return { current, longest };
}

export async function computeWebExtendedMetrics(): Promise<ExtendedReaderMetrics> {
  await ensureMigrated();
  const [mangas, chapters, sessions] = await Promise.all([
    getAllMangas(),
    getAllChapters(),
    getAllSessions(),
  ]);

  const now = new Date();
  const todayIso = now.toISOString().slice(0, 10);
  const activeDates = new Set<string>();
  const completedSessionChapters = new Set<string>();
  const readByManga = new Map<string, number>();
  const minutesByManga = new Map<string, number>();

  for (const ch of chapters) {
    if (ch.isRead) {
      readByManga.set(ch.mangaKey, (readByManga.get(ch.mangaKey) ?? 0) + 1);
      if (ch.updatedAt) activeDates.add(ch.updatedAt.slice(0, 10));
    }
  }

  let totalSeconds = 0;
  let totalPages = 0;
  for (const session of sessions) {
    const secs = Math.min(Math.max(0, session.duration_seconds), 43_200);
    totalSeconds += secs;
    totalPages += Math.max(0, session.pages_read);
    const mKey = mangaKey(session.provider, session.manga_id);
    minutesByManga.set(mKey, (minutesByManga.get(mKey) ?? 0) + secs / 60);
    const dateKey = (session.started_at ?? todayIso).slice(0, 10);
    activeDates.add(dateKey);
    if (session.completed_chapter) {
      completedSessionChapters.add(chapterKey(session.provider, session.manga_id, session.chapter_id));
    }
  }

  const daily: DailyReadingActivity[] = [];
  for (let offset = 13; offset >= 0; offset -= 1) {
    const d = new Date(now);
    d.setUTCDate(now.getUTCDate() - offset);
    const iso = d.toISOString().slice(0, 10);
    const daySessions = sessions.filter((s) => (s.started_at ?? todayIso).slice(0, 10) === iso);
    const daySecs = daySessions.reduce((acc, s) => acc + Math.min(Math.max(0, s.duration_seconds), 43_200), 0);
    const daySessionCh = new Set(
      daySessions
        .filter((s) => s.completed_chapter)
        .map((s) => chapterKey(s.provider, s.manga_id, s.chapter_id)),
    ).size;
    const dayDbCh = chapters.filter((c) => c.isRead && c.updatedAt.slice(0, 10) === iso).length;
    const dayPages = daySessions.reduce((acc, s) => acc + Math.max(0, s.pages_read), 0);
    daily.push({
      date: iso,
      chapters_read: Math.max(daySessionCh, dayDbCh),
      reading_minutes: daySecs === 0 ? 0 : Math.max(1, Math.round(daySecs / 60)),
      pages_read: dayPages,
    });
  }

  const dbReadCount = chapters.filter((c) => c.isRead).length;
  const totalCh = Math.max(dbReadCount, completedSessionChapters.size);
  const totalMins = totalSeconds === 0 ? 0 : Math.max(1, Math.round(totalSeconds / 60));
  const streaks = computeStreaks(activeDates, todayIso);

  const genreCounts = new Map<string, { mangas: number; chapters: number; minutes: number }>();
  for (const manga of mangas) {
    const chRead = readByManga.get(manga.key) ?? 0;
    const mins = Math.round(minutesByManga.get(manga.key) ?? 0);
    for (const g of manga.genres) {
      const cur = genreCounts.get(g) ?? { mangas: 0, chapters: 0, minutes: 0 };
      cur.mangas += 1;
      cur.chapters += chRead;
      cur.minutes += mins;
      genreCounts.set(g, cur);
    }
  }
  const totalGenreWeight = Array.from(genreCounts.values()).reduce((acc, v) => acc + Math.max(1, v.chapters || v.mangas), 0);
  const topGenres = Array.from(genreCounts.entries())
    .sort((a, b) => b[1].chapters - a[1].chapters || b[1].mangas - a[1].mangas)
    .slice(0, 8)
    .map(([genre, stat]) => ({
      genre,
      mangas_count: stat.mangas,
      chapters_read: stat.chapters,
      reading_minutes: stat.minutes,
      share_percent:
        totalGenreWeight > 0
          ? Math.round((Math.max(1, stat.chapters || stat.mangas) / totalGenreWeight) * 1000) / 10
          : 0,
    }));

  const topReadMangas: TopMangaStat[] = mangas
    .map((m) => ({
      manga_id: m.id,
      title: m.title,
      provider: m.provider,
      chapters_read: readByManga.get(m.key) ?? 0,
      reading_minutes: Math.round(minutesByManga.get(m.key) ?? 0),
    }))
    .filter((m) => m.chapters_read > 0 || m.reading_minutes > 0)
    .sort((a, b) => b.chapters_read - a.chapters_read || b.reading_minutes - a.reading_minutes)
    .slice(0, 5);

  const providerMap = new Map<string, ProviderReadingStat>();
  for (const m of mangas) {
    const cur = providerMap.get(m.provider) ?? { provider: m.provider, mangas_count: 0, chapters_read: 0 };
    cur.mangas_count += 1;
    cur.chapters_read += readByManga.get(m.key) ?? 0;
    providerMap.set(m.provider, cur);
  }

  return {
    total_chapters_read: totalCh,
    chapters_read_today: daily[daily.length - 1]?.chapters_read ?? 0,
    chapters_read_last_7_days: daily.slice(-7).reduce((acc, d) => acc + d.chapters_read, 0),
    chapters_read_last_14_days: daily.reduce((acc, d) => acc + d.chapters_read, 0),
    chapters_read_last_30_days: totalCh,
    total_pages_read: totalPages,
    pages_read_last_14_days: daily.reduce((acc, d) => acc + d.pages_read, 0),
    recorded_reading_seconds: totalSeconds,
    estimated_total_reading_minutes: totalMins,
    average_minutes_per_chapter: totalCh > 0 ? Math.round((totalMins / totalCh) * 10) / 10 : 0,
    active_days_count: activeDates.size,
    current_streak_days: streaks.current,
    longest_streak_days: streaks.longest,
    reading_mangas: mangas.filter((m) => m.inReadingHistory).length,
    plan_to_read_mangas: mangas.filter((m) => m.inPlanToRead).length,
    favorites_count: mangas.filter((m) => m.isFavorite).length,
    shelves_count: 0,
    downloaded_chapters: 0,
    top_genres: topGenres,
    top_read_mangas: topReadMangas,
    provider_breakdown: Array.from(providerMap.values()),
    daily_activity_last_14_days: daily,
    generated_at: now.toISOString(),
    privacy_local_only: true,
  };
}

export async function getWebLibraryCoverAndReadStats(): Promise<{
  byNormalizedTitle: Map<string, { coverUrl: string | null; readCount: number }>;
  readCountByMangaId: Map<string, number>;
}> {
  await ensureMigrated();
  const [mangas, chapters] = await Promise.all([getAllMangas(), getAllChapters()]);

  const readByMangaKey = new Map<string, number>();
  const readCountByMangaId = new Map<string, number>();
  for (const ch of chapters) {
    if (!ch.isRead) continue;
    readByMangaKey.set(ch.mangaKey, (readByMangaKey.get(ch.mangaKey) ?? 0) + 1);
    readCountByMangaId.set(ch.mangaId, (readCountByMangaId.get(ch.mangaId) ?? 0) + 1);
  }

  const byNormalizedTitle = new Map<string, { coverUrl: string | null; readCount: number }>();
  for (const m of mangas) {
    const norm = m.title.trim().toLowerCase();
    if (!norm) continue;
    const prev = byNormalizedTitle.get(norm);
    const readCount = readByMangaKey.get(m.key) ?? 0;
    byNormalizedTitle.set(norm, {
      coverUrl: m.coverUrl ?? prev?.coverUrl ?? null,
      readCount: Math.max(prev?.readCount ?? 0, readCount),
    });
  }

  return { byNormalizedTitle, readCountByMangaId };
}
