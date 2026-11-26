const MANGAONI_ORIGIN = "https://manga-oni.com";
const MANGADEX_API = "https://api.mangadex.org";
const MANGAONI_SERIES_ID = /^(manga|manhwa|manhua|novela)\/[a-z0-9_-]+$/i;
const MANGADEX_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const CHAPTER_DIGITS = /^\d+$/;
const CHAPTERS_PER_PAGE = 50;
const USER_AGENT = "MangoraWeb/1.0 (https://github.com/tomahawk1103/manga-tui)";
const ALLOWED_IMAGE_HOSTS = new Set([
  "uploads.mangadex.org",
  "oni.ntr-files.online",
  "manga-oni.com",
]);

const CORS_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

interface WorkerCacheStorage {
  default?: Cache;
}

interface NormalizedSeriesSummary {
  id: string;
  title: string;
  description: string | null;
  coverUrl: string;
  genres: string[];
  languages: string[];
}

interface NormalizedChapterItem {
  id: string;
  title: string;
  chapterNumber: string;
  volumeNumber: string | null;
  language: string;
  scanlator: string | null;
  publicationDate: string | null;
}

interface NormalizedSeriesDetail {
  id: string;
  provider: string;
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
  chapters: NormalizedChapterItem[];
}

function getEdgeCache(): Cache | undefined {
  return (globalThis as { caches?: WorkerCacheStorage }).caches?.default;
}

function jsonResponse(data: unknown, status = 200, maxAge = 0): Response {
  const cacheControl = maxAge > 0 ? `public, max-age=${maxAge}` : "no-store";
  return Response.json(data, {
    status,
    headers: {
      ...CORS_HEADERS,
      "Cache-Control": cacheControl,
    },
  });
}

function getSetCookies(headers: Headers): string[] {
  const extended = headers as Headers & { getSetCookie?: () => string[] };
  if (typeof extended.getSetCookie === "function") {
    return extended.getSetCookie();
  }
  const combined = headers.get("set-cookie");
  return combined ? [combined] : [];
}

function plainText(html: string): string {
  return html
    .replace(/<[^>]*>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function buildImageProxyUrl(requestUrl: URL, targetUrl: string): string {
  const proxy = new URL("./v1/image", requestUrl);
  // Ensure relative resolution works both at `/v1/...` and `/api/mangaoni/v1/...`
  const marker = requestUrl.pathname.indexOf("/v1/");
  if (marker >= 0) {
    proxy.pathname = requestUrl.pathname.slice(0, marker) + "/v1/image";
  } else {
    proxy.pathname = "/v1/image";
  }
  proxy.search = "";
  proxy.searchParams.set("url", targetUrl);
  return proxy.href;
}

async function fetchWithTimeout(url: string, init: RequestInit = {}, timeoutMs = 12000): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, { ...init, signal: controller.signal });
    return response;
  } finally {
    clearTimeout(timer);
  }
}

async function upstreamMangaOni(path: string, init?: RequestInit): Promise<Response> {
  const response = await fetchWithTimeout(MANGAONI_ORIGIN + path, {
    ...init,
    redirect: "manual",
    headers: {
      "User-Agent": USER_AGENT,
      "Accept-Language": "es-ES,es;q=0.9,en;q=0.8",
      ...init?.headers,
    },
  });
  if (!response.ok) throw new Error(`MangaOni respondió ${response.status}`);
  return response;
}

async function upstreamMangaDex(path: string): Promise<Response> {
  const response = await fetchWithTimeout(MANGADEX_API + path, {
    headers: {
      "User-Agent": USER_AGENT,
      Accept: "application/json",
    },
  });
  if (!response.ok) throw new Error(`MangaDex respondió ${response.status}`);
  return response;
}

async function mangaOniSearchRaw(query: string) {
  const home = await upstreamMangaOni("/");
  const token = (await home.text()).match(/name="csrf-token"\s+content="([^"]+)"/)?.[1];
  if (!token) throw new Error("Formulario de búsqueda no disponible");
  const cookie = getSetCookies(home.headers).map((value) => value.split(";")[0]).join("; ");
  const response = await upstreamMangaOni("/buscar", {
    method: "POST",
    headers: {
      Cookie: cookie,
      Referer: `${MANGAONI_ORIGIN}/`,
      "X-Requested-With": "XMLHttpRequest",
      "X-CSRF-TOKEN": token,
    },
    body: new URLSearchParams({ buscar: query, _token: token }),
  });
  const result = (await response.json()) as { mangas?: { nombre: string; url: string; img?: string }[] };
  return (result.mangas ?? []).flatMap((item) => {
    const url = new URL(item.url, MANGAONI_ORIGIN);
    const id = url.pathname.replace(/^\/|\/$/g, "");
    return url.origin === MANGAONI_ORIGIN && MANGAONI_SERIES_ID.test(id)
      ? [
          {
            id,
            title: plainText(item.nombre),
            cover: item.img ? new URL(item.img, MANGAONI_ORIGIN).href : undefined,
            language: "es",
            chapters: [],
          },
        ]
      : [];
  });
}

async function mangaOniUpdates() {
  const html = await (await upstreamMangaOni("/")).text();
  const entries = [];
  const seen = new Set<string>();
  for (const row of html.split(/<div\s+class="NRoaY">/i).slice(1)) {
    const series = row.match(
      /<a\s+href="(?:https:\/\/manga-oni\.com)?\/([^"]+)\/"[^>]*data-test="latest-update-name"[^>]*>([\s\S]*?)<\/a>/i,
    );
    if (!series || !MANGAONI_SERIES_ID.test(series[1])) continue;
    const id = series[1];
    const slug = id.split("/")[1];
    const cover = row.match(/data-src="(https:\/\/oni\.ntr-files\.online\/[^"]+)"/)?.[1];
    const published = row.match(/datetime="(\d{4}-\d{2}-\d{2})[ T]/)?.[1];
    for (const link of row.matchAll(
      /<a\s+href="(?:https:\/\/manga-oni\.com)?\/lector\/([^/"]+)\/(\d+)\/(?:cascada\/)?"[^>]*>([^<]+)<\/a>/gi,
    )) {
      const [, pathSlug, cid, label] = link;
      if (slug !== pathSlug || seen.has(`${id}|${cid}`)) continue;
      seen.add(`${id}|${cid}`);
      entries.push({
        id,
        title: plainText(series[2]),
        cover,
        published,
        chapterId: cid,
        chapterTitle: plainText(label),
      });
    }
  }
  if (!entries.length) throw new Error("La portada de MangaOni no contiene novedades reconocibles");
  return entries;
}

async function mangaOniDetailRaw(id: string) {
  const html = await (await upstreamMangaOni(`/${id}/`)).text();
  const slug = id.split("/")[1];
  const seen = new Set<string>();
  const chapters: Array<{ id: string; title: string; number: string; manifest: string; date: string | null }> = [];
  for (const link of html.matchAll(
    /<a\b[^>]*\bhref="(?:https:\/\/manga-oni\.com)?\/lector\/([^/"?#]+)\/(\d+)\/?"[^>]*>([\s\S]*?)<\/a>/gi,
  )) {
    const [, pathSlug, cid, body] = link;
    if (pathSlug !== slug || seen.has(cid)) continue;
    const number = body.match(/data-num="([^"]+)"/)?.[1] ?? "";
    const title = plainText(body.match(/<h3\b[^>]*>([\s\S]*?)<\/h3>/i)?.[1] ?? "") || `Capítulo ${number}`;
    const date = body.match(/(\d{4}-\d{2}-\d{2})/)?.[1] ?? null;
    seen.add(cid);
    chapters.push({
      id: cid,
      title,
      number,
      manifest: `chapter?${new URLSearchParams({ id, chapter: cid }).toString()}`,
      date,
    });
  }
  // Legacy order: ascending
  chapters.sort((a, b) => Number(a.number) - Number(b.number));
  const title = plainText(html.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/i)?.[1] ?? slug);
  const cover =
    html.match(/<meta[^>]+(?:property|name)="og:image"[^>]+content="([^"]+)"/i)?.[1] ??
    html.match(/<meta[^>]+content="([^"]+)"[^>]+(?:property|name)="og:image"/i)?.[1] ??
    html.match(/src="(https:\/\/oni\.ntr-files\.online\/[^"]+)"/i)?.[1] ??
    undefined;
  const rawDescription =
    html.match(/<p[^>]+class="[^"]*sinopsis[^"]*"[^>]*>([\s\S]*?)<\/p>/i)?.[1] ??
    html.match(/<div[^>]+id="sinopsis"[^>]*>([\s\S]*?)<\/div>/i)?.[1] ??
    html.match(/<meta[^>]+name="description"[^>]+content="([^"]+)"/i)?.[1] ??
    "";
  const description = rawDescription ? plainText(rawDescription) : null;
  const status = html.includes("Finalizado") || html.toLowerCase().includes("completed")
    ? "completed"
    : html.includes("Pausa") || html.toLowerCase().includes("hiatus")
      ? "hiatus"
      : "ongoing";
  const genres = Array.from(
    new Set(
      Array.from(html.matchAll(/<a[^>]+href="[^"]*\/genero\/[^"]*"[^>]*>([\s\S]*?)<\/a>/gi))
        .map((match) => plainText(match[1]))
        .filter(Boolean),
    ),
  );
  return { id, title, cover, description, status, genres, language: "es", chapters };
}

async function mangaOniPages(id: string, chapter: string) {
  const slug = id.split("/")[1];
  const html = await (await upstreamMangaOni(`/lector/${slug}/${chapter}/`)).text();
  const encoded = html.match(/\bvar\s+unicap\s*=\s*'([A-Za-z0-9+/=]+)'/)?.[1];
  if (!encoded) throw new Error("El capítulo no contiene páginas");
  const [base, files] = new TextDecoder()
    .decode(Uint8Array.from(atob(encoded), (b) => b.charCodeAt(0)))
    .split("||");
  const names: unknown = JSON.parse(files);
  if (!Array.isArray(names) || !names.length) throw new Error("Lista de páginas vacía");
  return {
    pages: names.map((name) => {
      if (typeof name !== "string" || !name) throw new Error("Página inválida");
      const url = new URL(base + name);
      if (url.origin !== "https://oni.ntr-files.online" || url.username || url.password) {
        throw new Error("CDN desconocido");
      }
      return url.href;
    }),
  };
}

interface MdRelationship {
  type: string;
  attributes?: {
    fileName?: string;
    name?: string;
  };
}

interface MdManga {
  id: string;
  attributes: {
    title?: Record<string, string>;
    altTitles?: Array<Record<string, string>>;
    description?: Record<string, string>;
    status?: string;
    availableTranslatedLanguages?: string[];
    tags?: Array<{ attributes?: { name?: Record<string, string> } }>;
  };
  relationships?: MdRelationship[];
}

function pickLocalizedText(
  map: Record<string, string> | undefined,
  preferred: string[] = ["es", "es-la", "en", "ja-ro", "ja"],
): string {
  if (!map) return "";
  for (const key of preferred) {
    const val = map[key]?.trim();
    if (val) return val;
  }
  const first = Object.values(map).find((v) => typeof v === "string" && v.trim());
  return first?.trim() ?? "";
}

function mapMangaDexSummary(manga: MdManga, requestUrl: URL, preferredLangs: string[]): NormalizedSeriesSummary {
  const title =
    pickLocalizedText(manga.attributes.title, preferredLangs) ||
    manga.attributes.altTitles?.map((alt) => pickLocalizedText(alt, preferredLangs)).find(Boolean) ||
    manga.id;
  const desc = pickLocalizedText(manga.attributes.description, preferredLangs) || null;
  const coverFile = manga.relationships?.find((rel) => rel.type === "cover_art")?.attributes?.fileName;
  const coverRaw = coverFile ? `https://uploads.mangadex.org/covers/${manga.id}/${coverFile}.256.jpg` : "";
  const coverUrl = coverRaw ? buildImageProxyUrl(requestUrl, coverRaw) : "";
  const genres = (manga.attributes.tags ?? [])
    .map((tag) => pickLocalizedText(tag.attributes?.name, ["en", "es"]))
    .filter(Boolean);
  const languages = (manga.attributes.availableTranslatedLanguages ?? []).filter(Boolean);
  return {
    id: manga.id,
    title,
    description: desc,
    coverUrl,
    genres,
    languages,
  };
}

async function mangaDexPopular(requestUrl: URL, lang: string): Promise<{ items: NormalizedSeriesSummary[] }> {
  const preferredLangs = lang === "en" ? ["en", "es", "es-la"] : ["es", "es-la", "en"];
  const params = new URLSearchParams({
    limit: "16",
    "order[followedCount]": "desc",
  });
  params.append("contentRating[]", "safe");
  params.append("contentRating[]", "suggestive");
  params.append("includes[]", "cover_art");
  params.append("availableTranslatedLanguage[]", lang === "en" ? "en" : "es");
  if (lang !== "en") params.append("availableTranslatedLanguage[]", "es-la");
  const resp = await upstreamMangaDex(`/manga?${params.toString()}`);
  const payload = (await resp.json()) as { data?: MdManga[] };
  return {
    items: (payload.data ?? []).map((item) => mapMangaDexSummary(item, requestUrl, preferredLangs)),
  };
}

async function mangaDexSearch(requestUrl: URL, query: string, page: number): Promise<{ items: NormalizedSeriesSummary[]; hasNext: boolean }> {
  const limit = 20;
  const offset = Math.max(0, (page - 1) * limit);
  const params = new URLSearchParams({
    title: query,
    limit: String(limit),
    offset: String(offset),
  });
  params.append("contentRating[]", "safe");
  params.append("contentRating[]", "suggestive");
  params.append("includes[]", "cover_art");
  const resp = await upstreamMangaDex(`/manga?${params.toString()}`);
  const payload = (await resp.json()) as { data?: MdManga[]; total?: number };
  const items = (payload.data ?? []).map((item) => mapMangaDexSummary(item, requestUrl, ["es", "es-la", "en"]));
  const total = payload.total ?? items.length;
  return {
    items,
    hasNext: offset + items.length < total && items.length > 0,
  };
}

async function mangaDexSeriesDetail(
  requestUrl: URL,
  id: string,
  requestedLang: string | null,
  order: "asc" | "desc",
  page: number,
): Promise<NormalizedSeriesDetail> {
  const metaParams = new URLSearchParams();
  metaParams.append("includes[]", "cover_art");
  metaParams.append("includes[]", "author");
  metaParams.append("includes[]", "artist");
  const metaResp = await upstreamMangaDex(`/manga/${id}?${metaParams.toString()}`);
  const metaJson = (await metaResp.json()) as { data: MdManga };
  const manga = metaJson.data;
  const availableLanguages = Array.from(
    new Set((manga.attributes.availableTranslatedLanguages ?? []).filter(Boolean)),
  );
  const preferredLang =
    (requestedLang && (availableLanguages.length === 0 || availableLanguages.includes(requestedLang))
      ? requestedLang
      : null) ||
    ["es", "es-la", "en"].find((code) => availableLanguages.includes(code)) ||
    availableLanguages[0] ||
    "es";

  const offset = Math.max(0, (page - 1) * CHAPTERS_PER_PAGE);
  const feedParams = new URLSearchParams({
    limit: String(CHAPTERS_PER_PAGE),
    offset: String(offset),
    "order[chapter]": order,
  });
  feedParams.append("translatedLanguage[]", preferredLang);
  feedParams.append("contentRating[]", "safe");
  feedParams.append("contentRating[]", "suggestive");
  feedParams.append("includes[]", "scanlation_group");
  const feedResp = await upstreamMangaDex(`/manga/${id}/feed?${feedParams.toString()}`);
  const feedJson = (await feedResp.json()) as {
    total?: number;
    data?: Array<{
      id: string;
      attributes: {
        title?: string | null;
        chapter?: string | null;
        volume?: string | null;
        translatedLanguage?: string;
        publishAt?: string | null;
      };
      relationships?: MdRelationship[];
    }>;
  };

  const chapters: NormalizedChapterItem[] = (feedJson.data ?? []).map((ch) => ({
    id: ch.id,
    title: ch.attributes.title?.trim() ?? "",
    chapterNumber: ch.attributes.chapter?.trim() || "0",
    volumeNumber: ch.attributes.volume?.trim() || null,
    language: ch.attributes.translatedLanguage ?? preferredLang,
    scanlator: ch.relationships?.find((rel) => rel.type === "scanlation_group")?.attributes?.name ?? null,
    publicationDate: ch.attributes.publishAt ? ch.attributes.publishAt.slice(0, 10) : null,
  }));

  const totalChapters = feedJson.total ?? chapters.length;
  const coverFile = manga.relationships?.find((rel) => rel.type === "cover_art")?.attributes?.fileName;
  const coverRaw = coverFile ? `https://uploads.mangadex.org/covers/${manga.id}/${coverFile}.512.jpg` : "";
  const author = manga.relationships?.find((rel) => rel.type === "author")?.attributes?.name ?? null;
  const artist = manga.relationships?.find((rel) => rel.type === "artist")?.attributes?.name ?? null;
  const summary = mapMangaDexSummary(manga, requestUrl, ["es", "es-la", "en"]);

  return {
    id: manga.id,
    provider: "mangadex",
    title: summary.title,
    coverUrl: coverRaw ? buildImageProxyUrl(requestUrl, coverRaw) : null,
    description: summary.description,
    author,
    artist,
    status: manga.attributes.status ?? "unknown",
    genres: summary.genres,
    availableLanguages: availableLanguages.length > 0 ? availableLanguages : [preferredLang],
    activeLanguage: preferredLang,
    order,
    page,
    totalChapters,
    hasNextPage: offset + chapters.length < totalChapters,
    chapters,
  };
}

async function mangaDexPages(chapterId: string): Promise<{ pages: string[] }> {
  const resp = await upstreamMangaDex(`/at-home/server/${chapterId}`);
  const payload = (await resp.json()) as {
    baseUrl?: string;
    chapter?: {
      hash?: string;
      dataSaver?: string[];
      data?: string[];
    };
  };
  const baseUrl = payload.baseUrl;
  const hash = payload.chapter?.hash;
  const files = payload.chapter?.dataSaver?.length ? payload.chapter.dataSaver : payload.chapter?.data;
  if (!baseUrl || !hash || !files?.length) {
    throw new Error("MangaDex no devolvió páginas disponibles para este capítulo");
  }
  const mode = payload.chapter?.dataSaver?.length ? "data-saver" : "data";
  return {
    pages: files.map((file) => `${baseUrl}/${mode}/${hash}/${file}`),
  };
}

async function proxyImage(rawUrl: string): Promise<Response> {
  let target: URL;
  try {
    target = new URL(rawUrl);
  } catch {
    return jsonResponse({ error: "URL de imagen inválida" }, 400);
  }
  if (
    target.protocol !== "https:" ||
    target.username ||
    target.password ||
    !ALLOWED_IMAGE_HOSTS.has(target.hostname.toLowerCase())
  ) {
    return jsonResponse({ error: "Origen de imagen no permitido" }, 403);
  }

  const referer =
    target.hostname.toLowerCase() === "uploads.mangadex.org"
      ? "https://mangadex.org/"
      : `${MANGAONI_ORIGIN}/`;

  const resp = await fetchWithTimeout(
    target.href,
    {
      headers: {
        "User-Agent": USER_AGENT,
        Referer: referer,
        Accept: "image/avif,image/webp,image/apng,image/*,*/*;q=0.8",
      },
    },
    15000,
  );
  if (!resp.ok) {
    return jsonResponse({ error: `Imagen no disponible (${resp.status})` }, 502);
  }
  const contentType = resp.headers.get("content-type") ?? "";
  if (!contentType.toLowerCase().startsWith("image/")) {
    return jsonResponse({ error: "El servidor remoto no devolvió una imagen válida" }, 502);
  }
  return new Response(resp.body, {
    status: 200,
    headers: {
      ...CORS_HEADERS,
      "Content-Type": contentType,
      "Cache-Control": "public, max-age=86400",
    },
  });
}

async function withEdgeCache(request: Request, handler: () => Promise<Response>): Promise<Response> {
  const cache = getEdgeCache();
  if (cache && request.method === "GET") {
    const cached = await cache.match(request);
    if (cached) return cached;
  }
  const response = await handler();
  if (cache && request.method === "GET" && response.ok && response.headers.get("Cache-Control")?.includes("public")) {
    await cache.put(request, response.clone()).catch(() => {});
  }
  return response;
}

export async function mangoraWorkerFetch(request: Request): Promise<Response> {
  if (request.method === "OPTIONS") {
    return new Response(null, { status: 204, headers: CORS_HEADERS });
  }
  if (request.method !== "GET") {
    return new Response(null, { status: 405, headers: CORS_HEADERS });
  }

  const url = new URL(request.url);
  const path = url.pathname;
  const v1Marker = path.indexOf("/v1/");
  const v1Route = v1Marker >= 0 ? path.slice(v1Marker) : null;

  try {
    if (v1Route) {
      if (v1Route === "/v1/providers") {
        return jsonResponse({ version: 1, providers: ["mangaoni", "mangadex"] }, 200, 300);
      }

      if (v1Route === "/v1/image") {
        const imageUrl = url.searchParams.get("url") ?? "";
        return withEdgeCache(request, () => proxyImage(imageUrl));
      }

      const provider = (url.searchParams.get("provider") ?? "mangaoni").trim().toLowerCase();

      if (v1Route === "/v1/updates") {
        if (provider !== "mangaoni") return jsonResponse({ error: "Proveedor no soportado en updates" }, 400);
        return withEdgeCache(request, async () => jsonResponse(await mangaOniUpdates(), 200, 120));
      }

      if (v1Route === "/v1/popular") {
        const lang = (url.searchParams.get("lang") ?? "es").trim().toLowerCase();
        return withEdgeCache(request, async () => {
          if (provider === "mangadex") {
            return jsonResponse(await mangaDexPopular(url, lang), 200, 300);
          }
          if (provider === "mangaoni") {
            const updates = await mangaOniUpdates();
            const seen = new Set<string>();
            const items: NormalizedSeriesSummary[] = [];
            for (const entry of updates) {
              if (seen.has(entry.id)) continue;
              seen.add(entry.id);
              items.push({
                id: entry.id,
                title: entry.title,
                description: null,
                coverUrl: entry.cover ?? "",
                genres: [],
                languages: ["es"],
              });
            }
            return jsonResponse({ items }, 200, 180);
          }
          return jsonResponse({ error: "Proveedor no soportado" }, 400);
        });
      }

      if (v1Route === "/v1/search") {
        const query = url.searchParams.get("q")?.trim() ?? "";
        const page = Math.max(1, Number.parseInt(url.searchParams.get("page") ?? "1", 10) || 1);
        if (!query || query.length > 120) return jsonResponse({ error: "Consulta inválida" }, 400);
        if (provider === "mangadex") {
          return withEdgeCache(request, async () => jsonResponse(await mangaDexSearch(url, query, page), 200, 120));
        }
        if (provider === "mangaoni") {
          const raw = await mangaOniSearchRaw(query);
          const items: NormalizedSeriesSummary[] = raw.map((item) => ({
            id: item.id,
            title: item.title,
            description: null,
            coverUrl: item.cover ?? "",
            genres: [],
            languages: ["es"],
          }));
          return jsonResponse({ items, hasNext: false });
        }
        return jsonResponse({ error: "Proveedor no soportado" }, 400);
      }

      if (v1Route === "/v1/series") {
        const id = (url.searchParams.get("id") ?? "").trim();
        const order = url.searchParams.get("order") === "asc" ? "asc" : "desc";
        const page = Math.max(1, Number.parseInt(url.searchParams.get("page") ?? "1", 10) || 1);
        const lang = url.searchParams.get("lang")?.trim() || null;

        if (provider === "mangadex" && MANGADEX_UUID.test(id)) {
          return withEdgeCache(
            request,
            async () => jsonResponse(await mangaDexSeriesDetail(url, id, lang, order, page), 200, 120),
          );
        }

        if (provider === "mangaoni" && MANGAONI_SERIES_ID.test(id)) {
          return withEdgeCache(request, async () => {
            const detail = await mangaOniDetailRaw(id);
            const sorted = [...detail.chapters].sort((a, b) =>
              order === "asc" ? Number(a.number) - Number(b.number) : Number(b.number) - Number(a.number),
            );
            const offset = (page - 1) * CHAPTERS_PER_PAGE;
            const sliced = sorted.slice(offset, offset + CHAPTERS_PER_PAGE);
            const response: NormalizedSeriesDetail = {
              id: detail.id,
              provider: "mangaoni",
              title: detail.title,
              coverUrl: detail.cover ?? null,
              description: detail.description,
              author: null,
              artist: null,
              status: detail.status,
              genres: detail.genres,
              availableLanguages: ["es"],
              activeLanguage: "es",
              order,
              page,
              totalChapters: sorted.length,
              hasNextPage: offset + sliced.length < sorted.length,
              chapters: sliced.map((ch) => ({
                id: `${detail.id}|${ch.id}`,
                title: ch.title,
                chapterNumber: ch.number || "?",
                volumeNumber: null,
                language: "es",
                scanlator: "MangaOni",
                publicationDate: ch.date,
              })),
            };
            return jsonResponse(response, 200, 120);
          });
        }
        return jsonResponse({ error: "Identificador de serie inválido" }, 400);
      }

      if (v1Route === "/v1/chapter") {
        const mangaId = (url.searchParams.get("mangaId") ?? "").trim();
        const rawChapterId = (url.searchParams.get("chapterId") ?? "").trim();
        if (provider === "mangadex" && MANGADEX_UUID.test(rawChapterId)) {
          return withEdgeCache(request, async () => jsonResponse(await mangaDexPages(rawChapterId), 200, 120));
        }
        if (provider === "mangaoni") {
          const [compoundMangaId, compoundChapter] = rawChapterId.includes("|")
            ? rawChapterId.split("|", 2)
            : [mangaId, rawChapterId];
          if (MANGAONI_SERIES_ID.test(compoundMangaId) && CHAPTER_DIGITS.test(compoundChapter)) {
            return withEdgeCache(
              request,
              async () => jsonResponse(await mangaOniPages(compoundMangaId, compoundChapter), 200, 300),
            );
          }
        }
        return jsonResponse({ error: "Identificador de capítulo inválido" }, 400);
      }

      return jsonResponse({ error: "Ruta v1 no encontrada" }, 404);
    }

    // Legacy root endpoints for backwards compatibility with earlier Mangora clients
    const id = url.searchParams.get("id") ?? "";
    const chapter = url.searchParams.get("chapter") ?? "";
    const query = url.searchParams.get("q")?.trim() ?? "";
    const endpoint = path.split("/").pop();

    if (endpoint === "updates") {
      return withEdgeCache(request, async () => jsonResponse(await mangaOniUpdates(), 200, 120));
    }
    if (endpoint === "search" && query && query.length <= 120) {
      return jsonResponse(await mangaOniSearchRaw(query));
    }
    if (endpoint === "series" && MANGAONI_SERIES_ID.test(id)) {
      return withEdgeCache(request, async () => jsonResponse(await mangaOniDetailRaw(id), 200, 120));
    }
    if (endpoint === "chapter" && MANGAONI_SERIES_ID.test(id) && CHAPTER_DIGITS.test(chapter)) {
      return withEdgeCache(request, async () => jsonResponse(await mangaOniPages(id, chapter), 200, 300));
    }

    return jsonResponse({ error: "Consulta inválida" }, 400);
  } catch (error) {
    const upstreamStatus = error instanceof Error && /^(MangaOni|MangaDex) respondió \d{3}$/.test(error.message);
    const message = upstreamStatus ? error.message : "El proveedor no pudo entregar los datos solicitados.";
    console.warn("MangoraWorker", v1Route ?? path, error instanceof Error ? error.message : message);
    return jsonResponse({ error: message }, 502);
  }
}

export default { fetch: mangoraWorkerFetch };
