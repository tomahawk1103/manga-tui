import type { Connect, Plugin } from "vite";

const readerUrl = "https://manga-oni.com/lector/dandadan/831345/";
const catalog = [{
  id: "mangaoni-dandadan",
  title: "Dandadan",
  language: "es",
  chapters: [{ id: "831345", title: "Capítulo de prueba · MangaOni", number: "254", manifest: "./chapter.json" }],
}];

async function loadChapter() {
  const response = await fetch(readerUrl, {
    redirect: "error",
    signal: AbortSignal.timeout(10_000),
  });
  if (!response.ok) throw new Error(`MangaOni respondió HTTP ${response.status}`);
  // MangaOni's public reader carries its page list in this data field.
  const encoded = (await response.text()).match(/\bvar\s+unicap\s*=\s*'([A-Za-z0-9+/=]+)'/)?.[1];
  if (!encoded) throw new Error("El lector de MangaOni no devolvió su lista de páginas");
  const decoded = new TextDecoder().decode(Uint8Array.from(atob(encoded), (byte) => byte.charCodeAt(0)));
  const [base, files] = decoded.split("||");
  const filenames: unknown = JSON.parse(files);
  if (!Array.isArray(filenames) || filenames.length === 0 ||
      filenames.some((name) => typeof name !== "string" || !name)) {
    throw new Error("Lista de páginas inválida");
  }
  const pages = filenames.map((file) => {
    const page = new URL(base + file);
    if (page.origin !== "https://oni.ntr-files.online" || page.username || page.password) {
      throw new Error("MangaOni devolvió una página fuera de su CDN");
    }
    return page.href;
  });
  return { pages };
}

export function mangaOniPreview(): Plugin {
  let cached: { expires: number; data: Awaited<ReturnType<typeof loadChapter>> } | undefined;
  let pending: ReturnType<typeof loadChapter> | undefined;

  const serve: Connect.NextHandleFunction = async (request, response, next) => {
    const { url, method } = request as { url?: string; method?: string };
    const path = url?.split("?")[0];
    if (method !== "GET" || (path !== "/api/mangaoni/catalog.json" && path !== "/api/mangaoni/chapter.json")) {
      next();
      return;
    }
    response.setHeader("Content-Type", "application/json; charset=utf-8");
    response.setHeader("Cache-Control", "no-store");
    try {
      if (path.endsWith("/catalog.json")) {
        response.end(JSON.stringify(catalog));
        return;
      }
      if (!cached || cached.expires <= Date.now()) {
        pending ??= loadChapter().then((data) => {
          cached = { expires: Date.now() + 120_000, data };
          return data;
        }).finally(() => { pending = undefined; });
        response.end(JSON.stringify(await pending));
        return;
      }
      response.end(JSON.stringify(cached.data));
    } catch (error) {
      console.warn("MangaOni preview:", error);
      response.statusCode = 502;
      response.end(JSON.stringify({ error: "No se pudo obtener el capítulo de MangaOni." }));
    }
  };

  return {
    name: "mangora-mangaoni-preview",
    configureServer(server) { server.middlewares.use(serve); },
    configurePreviewServer(server) { server.middlewares.use(serve); },
  };
}
