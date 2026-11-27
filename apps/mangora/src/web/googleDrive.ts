/// <reference types="vite/client" />
import type {
  CloudSyncActionResultDto,
  CloudSyncReportDto,
  InitialAppState,
  IntegrationsOverviewDto,
  OAuthStartResultDto,
  OfflineSeriesItem,
  ProviderOption,
} from "../types/mangora";
import { formatByteSize, parseCloudChapterFilename } from "./cbzReader";
import {
  exportWebLibraryBackupJson,
  getWebAppState,
  getWebLibraryCoverAndReadStats,
  importWebLibraryBackupJson,
} from "./db";

export const GOOGLE_DRIVE_SCOPE = "https://www.googleapis.com/auth/drive.file";
const CLOUD_ROOT_FOLDER_NAME = "MangaTUI";
const CLOUD_DOWNLOADS_FOLDER_NAME = "downloads";
const CLOUD_LIBRARY_JSON_NAME = "library_sync.json";
const GOOGLE_DRIVE_API_BASE = "https://www.googleapis.com/drive/v3";
const GOOGLE_DRIVE_UPLOAD_BASE = "https://www.googleapis.com/upload/drive/v3";

const STORAGE_SESSION_KEY = "mangora.web.gdrive.session.v1";
const STORAGE_CATALOG_KEY = "mangora.web.gdrive.catalog.v1";
const MAX_DOWNLOAD_CACHE_ENTRIES = 3;
const DEFAULT_WEB_GOOGLE_CLIENT_ID =
  "316454749861-l20n0r134lbus5dasvbhmr40hk84g9p4.apps.googleusercontent.com";

interface GoogleTokenClient {
  requestAccessToken: (overrideConfig?: { prompt?: string }) => void;
}

interface GoogleIdentityGlobal {
  accounts?: {
    oauth2?: {
      initTokenClient: (config: {
        client_id: string;
        scope: string;
        prompt?: string;
        callback: (response: {
          access_token?: string;
          expires_in?: number;
          error?: string;
          error_description?: string;
        }) => void;
        error_callback?: (error: { type?: string; message?: string }) => void;
      }) => GoogleTokenClient;
    };
  };
}

interface DriveFileEntry {
  id: string;
  name: string;
  mimeType: string;
  size?: string;
  createdTime?: string;
  modifiedTime?: string;
}

export interface WebDriveChapter {
  id: string;
  fileName: string;
  title: string;
  chapterNumber: string;
  format: "CBZ" | "EPUB" | "PDF" | "ARCHIVE";
  sizeBytes: number;
  modifiedTime: string | null;
}

export interface WebDriveSeries {
  folderId: string;
  mangaId: string;
  title: string;
  chapters: WebDriveChapter[];
  totalSizeBytes: number;
  formats: string[];
}

interface StoredWebDriveSession {
  connected: boolean;
  clientId: string | null;
  accessToken: string | null;
  expiresAt: number;
  accountEmail: string | null;
  accountDisplayName: string | null;
  lastSyncAt: string | null;
  lastSyncReport: CloudSyncReportDto | null;
}

let gisScriptPromise: Promise<void> | null = null;
const chapterBytesCache = new Map<string, ArrayBuffer>();
const chapterBytesOrder: string[] = [];

function getEmbeddedWebClientId(): string | null {
  const val = (import.meta.env.VITE_MANGORA_GDRIVE_WEB_CLIENT_ID as string | undefined)?.trim();
  return val || DEFAULT_WEB_GOOGLE_CLIENT_ID;
}

function loadSession(): StoredWebDriveSession {
  const fallback: StoredWebDriveSession = {
    connected: false,
    clientId: getEmbeddedWebClientId(),
    accessToken: null,
    expiresAt: 0,
    accountEmail: null,
    accountDisplayName: null,
    lastSyncAt: null,
    lastSyncReport: null,
  };
  if (typeof localStorage === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(STORAGE_SESSION_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<StoredWebDriveSession>;
    return {
      ...fallback,
      ...parsed,
      clientId: parsed.clientId?.trim() || getEmbeddedWebClientId(),
    };
  } catch {
    return fallback;
  }
}

function saveSession(session: StoredWebDriveSession): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_SESSION_KEY, JSON.stringify(session));
  } catch {
    // Ignore storage quota errors
  }
}

function loadCachedCatalog(): WebDriveSeries[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_CATALOG_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as WebDriveSeries[]) : [];
  } catch {
    return [];
  }
}

function saveCachedCatalog(items: WebDriveSeries[]): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_CATALOG_KEY, JSON.stringify(items));
  } catch {
    // Ignore storage quota errors
  }
}

async function loadGisScript(): Promise<void> {
  const google = (window as unknown as { google?: GoogleIdentityGlobal }).google;
  if (google?.accounts?.oauth2?.initTokenClient) return;
  if (gisScriptPromise) return gisScriptPromise;

  gisScriptPromise = new Promise<void>((resolve, reject) => {
    const existing = document.querySelector<HTMLScriptElement>('script[data-mangora-gis="true"]');
    if (existing) {
      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener("error", () => reject(new Error("No se pudo cargar Google Identity Services.")), {
        once: true,
      });
      return;
    }

    const script = document.createElement("script");
    script.src = "https://accounts.google.com/gsi/client";
    script.async = true;
    script.defer = true;
    script.dataset.mangoraGis = "true";
    script.onload = () => resolve();
    script.onerror = () => {
      gisScriptPromise = null;
      reject(new Error("No se pudo cargar Google Identity Services (revisa bloqueadores o conexión)."));
    };
    document.head.appendChild(script);
  });

  return gisScriptPromise;
}

async function requestBrowserAccessToken(
  clientId: string,
  prompt: "" | "consent" | "select_account",
): Promise<{ accessToken: string; expiresAt: number }> {
  await loadGisScript();
  const google = (window as unknown as { google?: GoogleIdentityGlobal }).google;
  const initTokenClient = google?.accounts?.oauth2?.initTokenClient;
  if (!initTokenClient) {
    throw new Error("Google Identity Services no está disponible en este navegador.");
  }

  return new Promise<{ accessToken: string; expiresAt: number }>((resolve, reject) => {
    let settled = false;
    const timeout = window.setTimeout(() => {
      if (!settled) {
        settled = true;
        reject(new Error("El inicio de sesión con Google expiró sin respuesta."));
      }
    }, 120_000);

    const client = initTokenClient({
      client_id: clientId,
      scope: GOOGLE_DRIVE_SCOPE,
      prompt,
      callback: (resp) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timeout);
        if (resp.error) {
          reject(new Error(resp.error_description || `Google OAuth: ${resp.error}`));
          return;
        }
        if (!resp.access_token) {
          reject(new Error("Google no devolvió ningún token de acceso."));
          return;
        }
        const ttlSec = Math.max(300, Number(resp.expires_in ?? 3600));
        resolve({
          accessToken: resp.access_token,
          expiresAt: Date.now() + ttlSec * 1000,
        });
      },
      error_callback: (err) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timeout);
        if (err.type === "popup_closed") {
          reject(new Error("Se cerró la ventana emergente de Google antes de completar el acceso."));
        } else if (err.type === "popup_failed_to_open") {
          reject(new Error("El navegador bloqueó la ventana emergente de Google. Permite popups para este sitio."));
        } else {
          reject(new Error(err.message || `Error de autenticación con Google (${err.type ?? "unknown"}).`));
        }
      },
    });

    client.requestAccessToken({ prompt });
  });
}

async function ensureValidAccessToken(interactive: boolean): Promise<string> {
  const session = loadSession();
  if (session.accessToken && session.expiresAt > Date.now() + 60_000) {
    return session.accessToken;
  }
  const clientId = session.clientId || getEmbeddedWebClientId();
  if (!clientId) {
    throw new Error("Falta el Client ID Web de Google OAuth en Ajustes → Nube.");
  }
  if (!interactive) {
    throw new Error("La sesión de Google Drive ha caducado. Pulsa «Sincronizar» o «Reconectar» en Ajustes → Nube.");
  }
  const token = await requestBrowserAccessToken(clientId, "");
  saveSession({
    ...session,
    connected: true,
    clientId,
    accessToken: token.accessToken,
    expiresAt: token.expiresAt,
  });
  return token.accessToken;
}

async function driveJsonFetch<T>(url: string, accessToken: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      ...(init?.headers ?? {}),
    },
  });
  if (res.status === 401) {
    const session = loadSession();
    saveSession({ ...session, accessToken: null, expiresAt: 0 });
    throw new Error("El token de Google Drive caducó (401). Vuelve a sincronizar o conectar en Ajustes → Nube.");
  }
  if (!res.ok) {
    const errText = await res.text().catch(() => "");
    throw new Error(`Google Drive respondió ${res.status}: ${errText.slice(0, 180)}`);
  }
  return (await res.json()) as T;
}

async function driveFetchAbout(
  accessToken: string,
): Promise<{ emailAddress: string | null; displayName: string | null }> {
  const data = await driveJsonFetch<{ user?: { emailAddress?: string; displayName?: string } }>(
    `${GOOGLE_DRIVE_API_BASE}/about?fields=user(displayName,emailAddress)`,
    accessToken,
  );
  return {
    emailAddress: data.user?.emailAddress?.trim() || null,
    displayName: data.user?.displayName?.trim() || null,
  };
}

async function driveListFoldersByName(
  accessToken: string,
  folderName: string,
  parentId?: string,
): Promise<DriveFileEntry[]> {
  const escaped = folderName.replace(/\\/g, "\\\\").replace(/'/g, "\\'");
  const queries = parentId
    ? [
        `name='${escaped}' and mimeType='application/vnd.google-apps.folder' and '${parentId}' in parents and trashed=false`,
      ]
    : [
        `name='${escaped}' and mimeType='application/vnd.google-apps.folder' and 'root' in parents and trashed=false`,
        `name='${escaped}' and mimeType='application/vnd.google-apps.folder' and trashed=false`,
      ];

  for (const query of queries) {
    const list = await driveJsonFetch<{ files?: DriveFileEntry[] }>(
      `${GOOGLE_DRIVE_API_BASE}/files?q=${encodeURIComponent(query)}&orderBy=${encodeURIComponent("createdTime asc")}&fields=files(id,name,mimeType,createdTime,modifiedTime)&pageSize=100`,
      accessToken,
    );
    if (list.files && list.files.length > 0) {
      return list.files;
    }
  }
  return [];
}

async function driveMoveFileToFolder(
  accessToken: string,
  fileId: string,
  oldParentId: string,
  newParentId: string,
): Promise<void> {
  await driveJsonFetch<{ id: string }>(
    `${GOOGLE_DRIVE_API_BASE}/files/${encodeURIComponent(fileId)}?addParents=${encodeURIComponent(newParentId)}&removeParents=${encodeURIComponent(oldParentId)}&fields=id`,
    accessToken,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    },
  );
}

async function driveTrashFile(accessToken: string, fileId: string): Promise<void> {
  await driveJsonFetch<{ id: string }>(
    `${GOOGLE_DRIVE_API_BASE}/files/${encodeURIComponent(fileId)}?fields=id`,
    accessToken,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ trashed: true }),
    },
  );
}

async function driveMergeFolderInto(
  accessToken: string,
  sourceFolderId: string,
  targetFolderId: string,
  isRoot: boolean,
  extraSnapshots: string[],
): Promise<void> {
  const [sourceChildren, targetChildren] = await Promise.all([
    driveListFolder(accessToken, sourceFolderId),
    driveListFolder(accessToken, targetFolderId),
  ]);

  const targetFolders = new Map<string, string>();
  const targetFiles = new Set<string>();
  for (const entry of targetChildren) {
    if (entry.mimeType === "application/vnd.google-apps.folder") {
      if (!targetFolders.has(entry.name)) targetFolders.set(entry.name, entry.id);
    } else {
      targetFiles.add(entry.name);
    }
  }

  for (const child of sourceChildren) {
    if (child.mimeType === "application/vnd.google-apps.folder") {
      const existingTargetSubId = targetFolders.get(child.name);
      if (existingTargetSubId) {
        await driveMergeFolderInto(accessToken, child.id, existingTargetSubId, false, extraSnapshots);
        await driveTrashFile(accessToken, child.id).catch(() => {});
      } else {
        await driveMoveFileToFolder(accessToken, child.id, sourceFolderId, targetFolderId);
        targetFolders.set(child.name, child.id);
      }
    } else if (isRoot && child.name === CLOUD_LIBRARY_JSON_NAME) {
      try {
        const text = await driveDownloadText(accessToken, child.id);
        extraSnapshots.push(text);
      } catch {
        // Ignore corrupt/unreadable snapshot in duplicate folder
      }
      await driveTrashFile(accessToken, child.id).catch(() => {});
    } else if (isRoot && child.name === "manga-tui-history.db") {
      await driveTrashFile(accessToken, child.id).catch(() => {});
    } else if (targetFiles.has(child.name)) {
      await driveTrashFile(accessToken, child.id).catch(() => {});
    } else {
      await driveMoveFileToFolder(accessToken, child.id, sourceFolderId, targetFolderId);
      targetFiles.add(child.name);
    }
  }

  await driveTrashFile(accessToken, sourceFolderId).catch(() => {});
}

async function driveFindOrCreateCanonicalRootFolder(accessToken: string): Promise<{
  id: string;
  children: DriveFileEntry[];
  extraSnapshots: string[];
}> {
  const folders = await driveListFoldersByName(accessToken, CLOUD_ROOT_FOLDER_NAME);
  if (folders.length === 0) {
    const created = await driveJsonFetch<{ id: string }>(
      `${GOOGLE_DRIVE_API_BASE}/files?fields=id`,
      accessToken,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: CLOUD_ROOT_FOLDER_NAME,
          mimeType: "application/vnd.google-apps.folder",
        }),
      },
    );
    return { id: created.id, children: [], extraSnapshots: [] };
  }

  if (folders.length === 1) {
    const children = await driveListFolder(accessToken, folders[0].id);
    return { id: folders[0].id, children, extraSnapshots: [] };
  }

  const listings = await Promise.all(folders.map((f) => driveListFolder(accessToken, f.id)));
  let canonicalIdx = -1;
  let bestModified = "";
  listings.forEach((files, idx) => {
    for (const file of files) {
      if (file.name === CLOUD_LIBRARY_JSON_NAME && (file.modifiedTime ?? "") >= bestModified) {
        bestModified = file.modifiedTime ?? "";
        canonicalIdx = idx;
      }
    }
  });
  if (canonicalIdx < 0) {
    canonicalIdx = listings.findIndex((files) => files.length > 0);
  }
  if (canonicalIdx < 0) canonicalIdx = 0;

  const canonicalId = folders[canonicalIdx].id;
  const extraSnapshots: string[] = [];
  for (let i = 0; i < folders.length; i += 1) {
    if (i === canonicalIdx) continue;
    await driveMergeFolderInto(accessToken, folders[i].id, canonicalId, true, extraSnapshots);
  }

  const children = await driveListFolder(accessToken, canonicalId);
  return { id: canonicalId, children, extraSnapshots };
}

async function driveListFolder(accessToken: string, folderId: string): Promise<DriveFileEntry[]> {
  const query = `'${folderId}' in parents and trashed=false`;
  const data = await driveJsonFetch<{ files?: DriveFileEntry[] }>(
    `${GOOGLE_DRIVE_API_BASE}/files?q=${encodeURIComponent(query)}&fields=files(id,name,mimeType,size,modifiedTime)&pageSize=1000`,
    accessToken,
  );
  return data.files ?? [];
}

async function driveDownloadText(accessToken: string, fileId: string): Promise<string> {
  const res = await fetch(`${GOOGLE_DRIVE_API_BASE}/files/${encodeURIComponent(fileId)}?alt=media`, {
    headers: { Authorization: `Bearer ${accessToken}` },
  });
  if (!res.ok) throw new Error(`Error descargando archivo de Drive (${res.status})`);
  return res.text();
}

async function driveUploadOrUpdateJson(
  accessToken: string,
  parentId: string,
  fileName: string,
  existingFileId: string | undefined,
  jsonText: string,
): Promise<string> {
  let fileId = existingFileId;
  if (!fileId) {
    const created = await driveJsonFetch<{ id: string }>(
      `${GOOGLE_DRIVE_API_BASE}/files?fields=id`,
      accessToken,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: fileName, parents: [parentId] }),
      },
    );
    fileId = created.id;
  }

  const uploaded = await driveJsonFetch<{ id: string }>(
    `${GOOGLE_DRIVE_UPLOAD_BASE}/files/${encodeURIComponent(fileId)}?uploadType=media&fields=id`,
    accessToken,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json; charset=UTF-8" },
      body: jsonText,
    },
  );
  return uploaded.id;
}

function isSupportedCloudArchive(name: string): boolean {
  const lower = name.toLowerCase();
  return (
    lower.endsWith(".cbz") ||
    lower.endsWith(".zip") ||
    lower.endsWith(".epub") ||
    lower.endsWith(".pdf")
  );
}

function compareChapterNumberDesc(a: WebDriveChapter, b: WebDriveChapter): number {
  const na = Number.parseFloat(a.chapterNumber);
  const nb = Number.parseFloat(b.chapterNumber);
  if (Number.isFinite(na) && Number.isFinite(nb) && na !== nb) return nb - na;
  if (Number.isFinite(na) && !Number.isFinite(nb)) return -1;
  if (!Number.isFinite(na) && Number.isFinite(nb)) return 1;
  return b.fileName.localeCompare(a.fileName, undefined, { numeric: true, sensitivity: "base" });
}

export async function scanWebDriveDownloads(forceRefresh = false): Promise<WebDriveSeries[]> {
  if (!forceRefresh) {
    const cached = loadCachedCatalog();
    if (cached.length > 0) return cached;
  }

  const accessToken = await ensureValidAccessToken(true);
  const rootFolders = await driveListFoldersByName(accessToken, CLOUD_ROOT_FOLDER_NAME);
  if (rootFolders.length === 0) {
    saveCachedCatalog([]);
    return [];
  }

  const downloadsFolderIds: string[] = [];
  for (const rootFolder of rootFolders) {
    const dlFolders = await driveListFoldersByName(accessToken, CLOUD_DOWNLOADS_FOLDER_NAME, rootFolder.id);
    for (const dl of dlFolders) {
      downloadsFolderIds.push(dl.id);
    }
  }
  if (downloadsFolderIds.length === 0) {
    saveCachedCatalog([]);
    return [];
  }

  const seriesFolders: DriveFileEntry[] = [];
  for (const downloadsId of downloadsFolderIds) {
    const children = await driveListFolder(accessToken, downloadsId);
    for (const entry of children) {
      if (entry.mimeType === "application/vnd.google-apps.folder") {
        seriesFolders.push(entry);
      }
    }
  }

  const seriesByTitle = new Map<string, WebDriveSeries>();
  const batchSize = 4;
  for (let i = 0; i < seriesFolders.length; i += batchSize) {
    const slice = seriesFolders.slice(i, i + batchSize);
    const results = await Promise.all(
      slice.map(async (folder): Promise<WebDriveSeries | null> => {
        const files = await driveListFolder(accessToken, folder.id);
        const chapters: WebDriveChapter[] = [];
        const formatsSet = new Set<string>();
        let totalSizeBytes = 0;

        for (const file of files) {
          if (file.mimeType === "application/vnd.google-apps.folder") continue;
          if (!isSupportedCloudArchive(file.name)) continue;
          const parsed = parseCloudChapterFilename(file.name);
          const sizeBytes = Number.parseInt(file.size ?? "0", 10) || 0;
          totalSizeBytes += sizeBytes;
          formatsSet.add(parsed.format);
          chapters.push({
            id: file.id,
            fileName: file.name,
            title: parsed.title,
            chapterNumber: parsed.chapterNumber,
            format: parsed.format,
            sizeBytes,
            modifiedTime: file.modifiedTime ?? null,
          });
        }

        if (chapters.length === 0) return null;
        chapters.sort(compareChapterNumberDesc);

        return {
          folderId: folder.id,
          mangaId: `gdrive::${folder.id}`,
          title: folder.name,
          chapters,
          totalSizeBytes,
          formats: Array.from(formatsSet),
        };
      }),
    );

    for (const item of results) {
      if (!item) continue;
      const existing = seriesByTitle.get(item.title);
      if (!existing) {
        seriesByTitle.set(item.title, item);
      } else {
        const seenFiles = new Set(existing.chapters.map((c) => c.fileName));
        for (const ch of item.chapters) {
          if (!seenFiles.has(ch.fileName)) {
            existing.chapters.push(ch);
            existing.totalSizeBytes += ch.sizeBytes;
            seenFiles.add(ch.fileName);
          }
        }
        existing.formats = Array.from(new Set([...existing.formats, ...item.formats]));
        existing.chapters.sort(compareChapterNumberDesc);
      }
    }
  }

  const seriesList = Array.from(seriesByTitle.values());
  seriesList.sort((a, b) => a.title.localeCompare(b.title));
  saveCachedCatalog(seriesList);
  return seriesList;
}

export function getWebIntegrationsOverview(): IntegrationsOverviewDto {
  const session = loadSession();
  const hasEmbeddedOauth = Boolean(getEmbeddedWebClientId());
  const summary = !session.connected
    ? "Sin vincular"
    : session.accountEmail
      ? `Google Drive Web · ${session.accountEmail}`
      : session.accountDisplayName
        ? `Google Drive Web · ${session.accountDisplayName}`
        : "Google Drive Web";

  return {
    cloud: {
      connected: session.connected,
      backend: session.connected ? "googleDrive" : null,
      summary,
      accountEmail: session.accountEmail,
      accountDisplayName: session.accountDisplayName,
      localFolderPath: null,
      localFolderLabel: null,
      hasEmbeddedOauth,
      savedClientId: session.clientId,
      lastSyncAt: session.lastSyncAt,
      lastSyncReport: session.lastSyncReport,
      detectedFolders: [],
    },
    trackers: {
      anilist: {
        connected: false,
        clientIdHint: null,
        source: null,
        trackReadingHistory: false,
      },
      myAnimeList: {
        connected: false,
        username: null,
        summary: "Sin vincular",
        hasEmbeddedOauth: false,
        savedClientId: null,
        lastSyncAt: null,
        lastSyncReport: null,
      },
    },
    supportConfig: {
      kofiUrl: null,
      sponsorsUrl: "https://github.com/tomahawk1103/manga-tui",
      discordUrl: null,
      githubUrl: "https://github.com/tomahawk1103/manga-tui",
      githubIssuesUrl: "https://github.com/tomahawk1103/manga-tui/issues/new",
      websiteUrl: "https://tomahawk1103.github.io/manga-tui/",
    },
    feedbackConfig: {
      proxyConfigured: false,
      fallbackIssuesUrl: "https://github.com/tomahawk1103/manga-tui/issues/new",
    },
    mangaTuiDetected: false,
  };
}

export async function connectWebGoogleDrive(
  clientIdInput?: string,
): Promise<OAuthStartResultDto> {
  const existing = loadSession();
  const clientId = clientIdInput?.trim() || existing.clientId || getEmbeddedWebClientId();
  if (!clientId) {
    const originHint = typeof window !== "undefined" ? window.location.origin : "https://tomahawk1103.github.io";
    throw new Error(
      `Introduce un Client ID Web de Google OAuth (con ${originHint} en Orígenes de JavaScript autorizados).`,
    );
  }

  const token = await requestBrowserAccessToken(clientId, "select_account");
  const about = await driveFetchAbout(token.accessToken).catch(() => ({
    emailAddress: null,
    displayName: null,
  }));

  saveSession({
    ...existing,
    connected: true,
    clientId,
    accessToken: token.accessToken,
    expiresAt: token.expiresAt,
    accountEmail: about.emailAddress,
    accountDisplayName: about.displayName,
  });

  // Warm the cloud downloads catalog non-fatally right after connecting
  await scanWebDriveDownloads(true).catch(() => {});

  return {
    provider: "googleDrive",
    mode: "loopback",
    authorizeUrl: null,
    redirectUri: null,
    completed: true,
    overview: getWebIntegrationsOverview(),
  };
}

export async function runWebCloudSyncNow(
  mode: "libraryOnly" | "fullSync" | "pushDownloads" | "pullDownloads",
  appVersion: string,
  availableProviders: ProviderOption[],
): Promise<CloudSyncActionResultDto> {
  const accessToken = await ensureValidAccessToken(true);
  const { id: rootId, children: rootFiles, extraSnapshots } = await driveFindOrCreateCanonicalRootFolder(accessToken);
  const snapshotFiles = rootFiles
    .filter((f) => f.name === CLOUD_LIBRARY_JSON_NAME)
    .sort((a, b) => (b.modifiedTime ?? "").localeCompare(a.modifiedTime ?? ""));
  const remoteLibraryFile = snapshotFiles[0];

  let importResult = null;
  for (const extraJson of extraSnapshots) {
    try {
      importResult = await importWebLibraryBackupJson(extraJson, availableProviders);
    } catch {
      // Ignore corrupt snapshot from duplicate folder
    }
  }
  for (const file of [...snapshotFiles].reverse()) {
    try {
      const remoteJson = await driveDownloadText(accessToken, file.id);
      importResult = await importWebLibraryBackupJson(remoteJson, availableProviders);
    } catch {
      // If remote JSON is corrupt/empty, proceed with remaining snapshots or local state
    }
  }
  for (const dupFile of snapshotFiles.slice(1)) {
    await driveTrashFile(accessToken, dupFile.id).catch(() => {});
  }

  const mergedJson = await exportWebLibraryBackupJson(appVersion);
  await driveUploadOrUpdateJson(
    accessToken,
    rootId,
    CLOUD_LIBRARY_JSON_NAME,
    remoteLibraryFile?.id,
    mergedJson,
  );

  let scannedChapters = 0;
  if (mode !== "libraryOnly") {
    const series = await scanWebDriveDownloads(true);
    scannedChapters = series.reduce((acc, s) => acc + s.chapters.length, 0);
  }

  const appState: InitialAppState =
    importResult?.appState ?? (await getWebAppState(availableProviders));
  const syncedAt = new Date().toISOString();
  const report: CloudSyncReportDto = {
    syncedAt,
    totalMangas: importResult?.totalMangas ?? appState.library.libraryMangas,
    totalReadChapters: importResult?.totalReadChapters ?? appState.library.chaptersRead,
    totalFavorites: importResult?.totalFavorites ?? appState.library.favorites,
    chaptersUploaded: 0,
    chaptersDownloaded: scannedChapters,
    bytesTransferred: new TextEncoder().encode(mergedJson).byteLength,
  };

  const session = loadSession();
  saveSession({
    ...session,
    connected: true,
    lastSyncAt: syncedAt,
    lastSyncReport: report,
  });

  return {
    report,
    overview: getWebIntegrationsOverview(),
    appState,
  };
}

export function disconnectWebGoogleDrive(): IntegrationsOverviewDto {
  const session = loadSession();
  saveSession({
    connected: false,
    clientId: session.clientId,
    accessToken: null,
    expiresAt: 0,
    accountEmail: null,
    accountDisplayName: null,
    lastSyncAt: session.lastSyncAt,
    lastSyncReport: session.lastSyncReport,
  });
  if (typeof localStorage !== "undefined") {
    localStorage.removeItem(STORAGE_CATALOG_KEY);
  }
  chapterBytesCache.clear();
  chapterBytesOrder.length = 0;
  return getWebIntegrationsOverview();
}

export async function listWebDriveOfflineLibrary(): Promise<OfflineSeriesItem[]> {
  const session = loadSession();
  if (!session.connected) return [];

  let series = loadCachedCatalog();
  if (series.length === 0 && session.accessToken && session.expiresAt > Date.now() + 60_000) {
    series = await scanWebDriveDownloads(true).catch(() => []);
  }
  if (series.length === 0) return [];

  const stats = await getWebLibraryCoverAndReadStats();

  return series.map((item) => {
    const normTitle = item.title.trim().toLowerCase();
    const matched = stats.byNormalizedTitle.get(normTitle);
    return {
      mangaId: item.mangaId,
      title: item.title,
      provider: "gdrive",
      providerLabel: "Google Drive ☁",
      isOfflineOnly: true,
      chapterCount: item.chapters.length,
      readCount: stats.readCountByMangaId.get(item.mangaId) ?? matched?.readCount ?? 0,
      formats: item.formats.length > 0 ? item.formats : ["CBZ"],
      totalSizeBytes: item.totalSizeBytes,
      totalSizeLabel: formatByteSize(item.totalSizeBytes),
      coverUrl: matched?.coverUrl ?? null,
    };
  });
}

export function getWebDriveSeriesById(mangaId: string): WebDriveSeries | null {
  const catalog = loadCachedCatalog();
  return catalog.find((s) => s.mangaId === mangaId || s.folderId === mangaId) ?? null;
}

export async function downloadWebDriveChapterBytes(fileId: string): Promise<ArrayBuffer> {
  const cached = chapterBytesCache.get(fileId);
  if (cached) return cached;

  const accessToken = await ensureValidAccessToken(true);
  const res = await fetch(`${GOOGLE_DRIVE_API_BASE}/files/${encodeURIComponent(fileId)}?alt=media`, {
    headers: { Authorization: `Bearer ${accessToken}` },
  });
  if (res.status === 401) {
    const session = loadSession();
    saveSession({ ...session, accessToken: null, expiresAt: 0 });
    const freshToken = await ensureValidAccessToken(true);
    const retry = await fetch(`${GOOGLE_DRIVE_API_BASE}/files/${encodeURIComponent(fileId)}?alt=media`, {
      headers: { Authorization: `Bearer ${freshToken}` },
    });
    if (!retry.ok) throw new Error(`No se pudo descargar el capítulo de Drive (${retry.status}).`);
    const buf = await retry.arrayBuffer();
    cacheChapterBytes(fileId, buf);
    return buf;
  }
  if (!res.ok) {
    throw new Error(`No se pudo descargar el capítulo de Drive (${res.status}).`);
  }
  const buffer = await res.arrayBuffer();
  cacheChapterBytes(fileId, buffer);
  return buffer;
}

function cacheChapterBytes(fileId: string, buffer: ArrayBuffer): void {
  chapterBytesCache.set(fileId, buffer);
  chapterBytesOrder.push(fileId);
  while (chapterBytesOrder.length > MAX_DOWNLOAD_CACHE_ENTRIES) {
    const evicted = chapterBytesOrder.shift();
    if (evicted) chapterBytesCache.delete(evicted);
  }
}
