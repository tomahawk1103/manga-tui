/// <reference types="vite/client" />
import type { OpenReaderChapterRequest, ReaderChapterNeighbor } from "../types/mangora";
import type { ReaderSource } from "../views/ReaderView";

export interface Chapter {
  id: string;
  title: string;
  number: string;
  manifest: string;
}

export interface Series {
  id: string;
  title: string;
  cover?: string;
  language: string;
  chapters: Chapter[];
}

interface ChapterManifest {
  width?: number;
  height?: number;
  pages: string[];
}

const defaultCatalog = import.meta.env.MODE === "mangaoni" ? "./api/mangaoni/catalog.json" : "./api/catalog.json";
const catalogUrl = new URL(import.meta.env.VITE_MANGORA_CATALOG_URL || defaultCatalog, document.baseURI);

async function fetchJson<T>(url: URL): Promise<T> {
  const response = await fetch(url, { cache: "no-cache" });
  if (!response.ok) throw new Error(`El servidor respondió con el código ${response.status}.`);
  return response.json() as Promise<T>;
}

export async function loadCatalog(): Promise<Series[]> {
  const series = await fetchJson<Series[]>(catalogUrl);
  return series.map((item) => ({ ...item, cover: item.cover && new URL(item.cover, catalogUrl).href }));
}

export function chapterRequest(series: Series, chapter: Chapter): OpenReaderChapterRequest {
  return {
    mangaId: series.id,
    mangaTitle: series.title,
    provider: "mangora-web",
    coverUrl: series.cover,
    chapterId: chapter.id,
    chapterTitle: chapter.title,
    chapterNumber: chapter.number,
    language: series.language,
  };
}

function progressKey(seriesId: string, chapterId: string): string {
  if (seriesId === "mangora-sample" && chapterId === "sample-1") return "mangora-web-sample-chapter-v1";
  return `mangora-web-progress:${JSON.stringify([seriesId, chapterId])}`;
}

export function savedPage(seriesId: string, chapterId: string): number | null {
  try {
    const value = localStorage.getItem(progressKey(seriesId, chapterId));
    if (value !== null && /^[0-9]+$/.test(value) && Number.isSafeInteger(Number(value))) return Number(value);
  } catch {
    // The chapter is still readable when storage is disabled.
  }
  return null;
}

export function makeReaderSource(series: Series): ReaderSource {
  const loaded = new Map<string, ChapterManifest>();

  function neighbor(chapter: Chapter | undefined): ReaderChapterNeighbor | null {
    if (!chapter) return null;
    return {
      id: chapter.id,
      title: chapter.title,
      chapterNumber: chapter.number,
      volumeNumber: null,
      language: series.language,
      isDownloaded: false,
    };
  }

  return {
    pageSize(sessionId) {
      const chapter = loaded.get(sessionId);
      if (chapter?.width && chapter.height) return { width: chapter.width, height: chapter.height };
    },
    pageUrl: (sessionId, index) => loaded.get(sessionId)!.pages[index],
    async openChapter(request) {
      const index = series.chapters.findIndex((chapter) => chapter.id === request.chapterId);
      const chapter = series.chapters[index];
      if (!chapter || request.mangaId !== series.id) throw new Error("Este capítulo no está en el catálogo web.");
      const url = new URL(chapter.manifest, catalogUrl);
      const manifest = await fetchJson<ChapterManifest>(url);
      if (!Array.isArray(manifest.pages) || manifest.pages.length === 0 ||
          manifest.pages.some((page) => typeof page !== "string" || !page)) {
        throw new Error("El servidor no devolvió páginas válidas para este capítulo.");
      }
      const pages = manifest.pages.map((page) => new URL(page, url).href);
      loaded.set(chapter.id, { ...manifest, pages });
      return {
        sessionId: chapter.id,
        mangaId: series.id,
        mangaTitle: series.title,
        coverUrl: series.cover ?? null,
        provider: request.provider,
        providerLabel: "Mangora",
        chapterId: chapter.id,
        chapterTitle: chapter.title,
        chapterNumber: chapter.number,
        volumeNumber: null,
        language: series.language,
        pageCount: pages.length,
        initialPage: Math.min(savedPage(series.id, chapter.id) ?? 0, pages.length - 1),
        isOffline: false,
        offlineFormat: null,
        prevChapter: neighbor(series.chapters[index - 1]),
        nextChapter: neighbor(series.chapters[index + 1]),
      };
    },
    async saveProgress({ mangaId, chapterId, pageIndex, totalPages }) {
      localStorage.setItem(progressKey(mangaId, chapterId), String(pageIndex));
      return { pageIndex, isRead: pageIndex === (totalPages ?? 0) - 1 };
    },
  };
}
