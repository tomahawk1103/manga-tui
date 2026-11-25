/// <reference types="vite/client" />
import firstPage from "../../../../docs/images/pagina-1.jpg";
import secondPage from "../../../../docs/images/pagina-2.jpg";
import thirdPage from "../../../../docs/images/pagina-3.jpg";
import type { OpenReaderChapterRequest, ReaderSessionResponse } from "../types/mangora";
import type { ReaderSource } from "../views/ReaderView";

const pages = [firstPage, secondPage, thirdPage];
const progressKey = "mangora-web-sample-chapter-v1";

export const sampleChapter = {
  mangaId: "mangora-sample",
  mangaTitle: "Manga de muestra",
  provider: "mangora-web",
  coverUrl: firstPage,
  chapterId: "sample-1",
  chapterTitle: "Capítulo 1",
  chapterNumber: "1",
  language: "es",
} satisfies OpenReaderChapterRequest;

export function savedPage(): number | null {
  try {
    const value = localStorage.getItem(progressKey);
    if (value !== null && /^[0-9]+$/.test(value) && Number(value) < pages.length) {
      return Number(value);
    }
  } catch {
    // The chapter is still readable when storage is disabled.
  }
  return null;
}

export const sampleSource: ReaderSource = {
  pageSize: { width: 1350, height: 1920 },
  pageUrl: (_, index) => pages[index],
  async openChapter(request): Promise<ReaderSessionResponse> {
    if (request.mangaId !== sampleChapter.mangaId || request.chapterId !== sampleChapter.chapterId) {
      throw new Error("Este capítulo no está en el catálogo web.");
    }
    return {
      ...sampleChapter,
      sessionId: sampleChapter.chapterId,
      providerLabel: "Mangora",
      volumeNumber: null,
      pageCount: pages.length,
      initialPage: savedPage() ?? 0,
      isOffline: false,
      offlineFormat: null,
      prevChapter: null,
      nextChapter: null,
    };
  },
  async saveProgress({ pageIndex }) {
    localStorage.setItem(progressKey, String(pageIndex));
    return { pageIndex, isRead: pageIndex === pages.length - 1 };
  },
};
