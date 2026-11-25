export type ThemeMode = "paper" | "ink";

export type UiLanguage = "es" | "en";

export type NavSection = "home" | "explore" | "downloads" | "settings" | "support";

export type ReaderViewMode = "cascade" | "single" | "double";

export type ReadingDirection = "rtl" | "ltr";

export type PageFitMode = "fitWidth" | "fitHeight" | "original";

export type DownloadFormat = "cbz" | "pdf" | "epub" | "raw";

export interface ReaderPrefs {
  viewMode: ReaderViewMode;
  direction: ReadingDirection;
  fitMode: PageFitMode;
  zoomPercent: number;
  cascadeGapPx: number;
  firstPageSingle: boolean;
  pinToolbars: boolean;
  darkCanvas: boolean;
}

export interface LibraryDetection {
  hasExistingData: boolean;
  libraryMangas: number;
  chaptersRead: number;
  totalPagesRead: number;
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
  downloadFormat: DownloadFormat;
  readerPrefs: ReaderPrefs;
  supportBannerDismissed: boolean;
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

export interface ReaderChapterNeighbor {
  id: string;
  title: string;
  chapterNumber: string;
  volumeNumber: string | null;
  language: string;
  isDownloaded: boolean;
}

export interface OpenReaderChapterRequest {
  mangaId: string;
  provider: string;
  mangaTitle: string;
  coverUrl?: string | null;
  chapterId: string;
  chapterTitle?: string | null;
  chapterNumber?: string | null;
  volumeNumber?: string | null;
  language?: string | null;
  chaptersContext?: ReaderChapterNeighbor[];
}

export interface ReaderSessionResponse {
  sessionId: string;
  mangaId: string;
  provider: string;
  providerLabel: string;
  mangaTitle: string;
  coverUrl: string | null;
  chapterId: string;
  chapterTitle: string;
  chapterNumber: string;
  volumeNumber: string | null;
  language: string;
  pageCount: number;
  initialPage: number;
  isOffline: boolean;
  offlineFormat: string | null;
  prevChapter: ReaderChapterNeighbor | null;
  nextChapter: ReaderChapterNeighbor | null;
}

export interface SaveReaderProgressPayload {
  mangaId: string;
  provider: string;
  mangaTitle: string;
  coverUrl?: string | null;
  chapterId: string;
  chapterTitle: string;
  language: string;
  pageIndex: number;
  totalPages?: number;
}

export interface SaveReaderProgressResult {
  pageIndex: number;
  isRead: boolean;
}

export type DownloadTaskStatus =
  | "queued"
  | "downloading"
  | "packaging"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface DownloadTaskItem {
  taskId: string;
  mangaId: string;
  mangaTitle: string;
  mangaCoverUrl: string | null;
  provider: string;
  providerLabel: string;
  language: string;
  chapterId: string;
  chapterTitle: string;
  chapterNumber: string;
  volumeNumber: string | null;
  format: string;
  status: DownloadTaskStatus;
  progressPercent: number;
  pagesDownloaded: number;
  totalPages: number;
  bytesDownloaded: number;
  speedBytesPerSec: number;
  errorMessage: string | null;
  outputPath: string | null;
}

export interface DownloadQueueSnapshot {
  isPaused: boolean;
  isDownloading: boolean;
  activeFormat: DownloadFormat;
  downloadsDir: string;
  queuedCount: number;
  downloadingCount: number;
  pausedCount: number;
  completedCount: number;
  failedCount: number;
  overallProgressPercent: number;
  speedBytesPerSec: number;
  items: DownloadTaskItem[];
}

export interface DownloadChapterFinishedEvent {
  taskId: string;
  mangaId: string;
  mangaTitle: string;
  chapterId: string;
  chapterNumber: string;
  format: string;
  outputPath: string | null;
}

export interface EnqueueChapterInput {
  id: string;
  title: string;
  chapterNumber: string;
  volumeNumber?: string | null;
  language: string;
  scanlator?: string | null;
}

export interface EnqueueChaptersRequest {
  mangaId: string;
  provider: string;
  mangaTitle: string;
  coverUrl?: string | null;
  formatOverride?: DownloadFormat;
  chapters: EnqueueChapterInput[];
}

export type DownloadQueueAction =
  | "pauseAll"
  | "resumeAll"
  | "cancelAll"
  | "retryAllFailed"
  | "clearFinished"
  | "pauseTask"
  | "resumeTask"
  | "cancelTask"
  | "retryTask"
  | "removeTask";

export interface SetDownloadFormatResult {
  queue: DownloadQueueSnapshot;
  appState: InitialAppState;
}

export interface OfflineSeriesItem {
  mangaId: string;
  title: string;
  provider: string;
  providerLabel: string;
  isOfflineOnly: boolean;
  chapterCount: number;
  readCount: number;
  formats: string[];
  totalSizeBytes: number;
  totalSizeLabel: string;
  coverUrl: string | null;
}

export type FeedbackCategory = "suggestion" | "bugReport" | "brokenProvider";

export interface SubmitFeedbackRequest {
  category: FeedbackCategory;
  message: string;
  includeDiagnostics: boolean;
}

export interface SubmitFeedbackResponse {
  delivered: boolean;
  mode: "proxy" | "githubFallback" | string;
  statusMessage: string;
  fallbackIssueUrl: string | null;
}

export interface FeedbackConfigDto {
  proxyConfigured: boolean;
  fallbackIssuesUrl: string;
}

export interface DetectedCloudFolderDto {
  providerLabel: string;
  path: string;
}

export interface CloudSyncReportDto {
  syncedAt: string;
  totalMangas: number;
  totalReadChapters: number;
  totalFavorites: number;
  chaptersUploaded: number;
  chaptersDownloaded: number;
  bytesTransferred: number;
}

export interface CloudStatusDto {
  connected: boolean;
  backend: "googleDrive" | "localFolder" | null;
  summary: string;
  accountEmail: string | null;
  accountDisplayName: string | null;
  localFolderPath: string | null;
  localFolderLabel: string | null;
  hasEmbeddedOauth: boolean;
  savedClientId: string | null;
  lastSyncAt: string | null;
  lastSyncReport: CloudSyncReportDto | null;
  detectedFolders: DetectedCloudFolderDto[];
}

export interface AnilistStatusDto {
  connected: boolean;
  clientIdHint: string | null;
  source: string | null;
  trackReadingHistory: boolean;
}

export interface MalSyncReportDto {
  syncedAt: string;
  importedFromMal: number;
  pushedToMal: number;
  totalMalEntries: number;
}

export interface MalStatusDto {
  connected: boolean;
  username: string | null;
  summary: string;
  hasEmbeddedOauth: boolean;
  savedClientId: string | null;
  lastSyncAt: string | null;
  lastSyncReport: MalSyncReportDto | null;
}

export interface TrackersStatusDto {
  anilist: AnilistStatusDto;
  myAnimeList: MalStatusDto;
}

export interface SupportConfigDto {
  kofiUrl: string | null;
  sponsorsUrl: string;
  discordUrl: string;
  githubUrl: string;
  githubIssuesUrl: string;
  websiteUrl: string;
}

export interface IntegrationsOverviewDto {
  cloud: CloudStatusDto;
  trackers: TrackersStatusDto;
  supportConfig: SupportConfigDto;
  feedbackConfig: FeedbackConfigDto;
  mangaTuiDetected: boolean;
}

export interface OAuthStartResultDto {
  provider: "googleDrive" | "myAnimeList" | string;
  mode: "loopback" | "loopback_async" | "deeplink" | string;
  authorizeUrl: string | null;
  redirectUri: string | null;
  completed: boolean;
  overview: IntegrationsOverviewDto;
}

export interface OAuthCompletedEventDto {
  provider: "googleDrive" | "myAnimeList" | string;
  ok: boolean;
  error: string | null;
  overview: IntegrationsOverviewDto | null;
}

export interface CloudSyncActionResultDto {
  report: CloudSyncReportDto;
  overview: IntegrationsOverviewDto;
  appState: InitialAppState;
}

export interface MalSyncActionResultDto {
  report: MalSyncReportDto;
  overview: IntegrationsOverviewDto;
  appState: InitialAppState;
}

export interface UpdateCheckResultDto {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
  androidApkUrl?: string | null;
  iosIpaUrl?: string | null;
  releaseNotes: string;
  publishedAt: string | null;
  checkedAt: string;
}

export interface PortableBackupImportResultDto {
  totalMangas: number;
  totalReadChapters: number;
  totalFavorites: number;
  appState: InitialAppState;
  library: LibraryOverview;
}

export function libraryKey(mangaId: string, provider: string): string {
  return `${provider.trim().toLowerCase()}::${mangaId.trim()}`;
}
