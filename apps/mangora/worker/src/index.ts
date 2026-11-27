const MANGAONI_ORIGIN = "https://manga-oni.com";
const MANGADEX_API = "https://api.mangadex.org";
const WEEBCENTRAL_ORIGIN = "https://weebcentral.com";
const MANGAONI_SERIES_ID = /^(manga|manhwa|manhua|novela)\/[a-z0-9_-]+$/i;
const MANGADEX_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const WEEBCENTRAL_ID = /^[A-Z0-9]{10,40}$/i;
const CHAPTER_DIGITS = /^\d+$/;
const CHAPTERS_PER_PAGE = 50;
const USER_AGENT = "MangoraWeb/1.0 (https://github.com/tomahawk1103/manga-tui)";
const WEEBCENTRAL_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0";
const EXACT_ALLOWED_IMAGE_HOSTS = new Set([
  "uploads.mangadex.org",
  "oni.ntr-files.online",
  "manga-oni.com",
  "temp.compsci88.com",
  "weebcentral.com",
]);
const ALLOWED_IMAGE_SUFFIXES = [
  ".planeptune.us",
  ".lastation.us",
  ".lowee.us",
  ".leanbox.us",
  ".compsci88.com",
  ".weebcentral.com",
];

function isAllowedImageHost(hostname: string): boolean {
  const lower = hostname.toLowerCase();
  if (EXACT_ALLOWED_IMAGE_HOSTS.has(lower)) return true;
  return ALLOWED_IMAGE_SUFFIXES.some((suffix) => lower.endsWith(suffix));
}

function isWeebCentralHost(hostname: string): boolean {
  const lower = hostname.toLowerCase();
  if (lower === "weebcentral.com" || lower === "temp.compsci88.com") return true;
  return ALLOWED_IMAGE_SUFFIXES.some((suffix) => lower.endsWith(suffix));
}

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
  provider?: string;
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

async function upstreamWeebCentral(path: string): Promise<Response> {
  const response = await fetchWithTimeout(WEEBCENTRAL_ORIGIN + path, {
    headers: {
      "User-Agent": WEEBCENTRAL_USER_AGENT,
      Referer: `${WEEBCENTRAL_ORIGIN}/`,
      Accept: "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
      "Accept-Language": "en-US,en;q=0.9",
    },
  });
  if (!response.ok) throw new Error(`WeebCentral respondió ${response.status}`);
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

let mangaDexTagMapCache: Map<string, string> | null = null;

const CANONICAL_MANGADEX_TAGS: Record<string, string> = {
  slice_of_life: "slice of life",
  scifi: "sci-fi",
  martial_arts: "martial arts",
};

async function getMangaDexTagMap(): Promise<Map<string, string>> {
  if (mangaDexTagMapCache) return mangaDexTagMapCache;
  const map = new Map<string, string>();
  try {
    const resp = await upstreamMangaDex("/manga/tag");
    const payload = (await resp.json()) as {
      data?: Array<{ id: string; attributes?: { name?: Record<string, string> } }>;
    };
    for (const tag of payload.data ?? []) {
      const name = pickLocalizedText(tag.attributes?.name, ["en"]).toLowerCase();
      if (name && tag.id) map.set(name, tag.id);
    }
    if (map.size > 0) mangaDexTagMapCache = map;
  } catch {
    // Non-fatal: demographics and rating filters still work without tag UUIDs
  }
  return map;
}

async function mangaDexLatestChapters(
  requestUrl: URL,
  page: number,
  lang: string,
): Promise<{
  items: Array<{
    id: string;
    provider: string;
    title: string;
    cover?: string;
    published?: string;
    chapterId: string;
    chapterTitle: string;
  }>;
  hasMore: boolean;
}> {
  const limit = 24;
  const offset = Math.max(0, (page - 1) * limit);
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
    "order[readableAt]": "desc",
  });
  params.append("contentRating[]", "safe");
  params.append("contentRating[]", "suggestive");
  params.append("includes[]", "manga");
  if (lang === "en") {
    params.append("translatedLanguage[]", "en");
  } else {
    params.append("translatedLanguage[]", "es");
    params.append("translatedLanguage[]", "es-la");
  }

  const resp = await upstreamMangaDex(`/chapter?${params.toString()}`);
  const payload = (await resp.json()) as {
    total?: number;
    data?: Array<{
      id: string;
      attributes?: {
        chapter?: string | null;
        title?: string | null;
        readableAt?: string | null;
        publishAt?: string | null;
      };
      relationships?: Array<{
        id: string;
        type: string;
        attributes?: { title?: Record<string, string> };
      }>;
    }>;
  };

  const chapters = payload.data ?? [];
  const mangaIds = Array.from(
    new Set(
      chapters
        .map((ch) => ch.relationships?.find((rel) => rel.type === "manga")?.id)
        .filter((id): id is string => Boolean(id)),
    ),
  );

  const mangaCovers = new Map<string, { title: string; cover?: string }>();
  if (mangaIds.length > 0) {
    const mangaParams = new URLSearchParams({ limit: String(mangaIds.length) });
    mangaParams.append("includes[]", "cover_art");
    for (const id of mangaIds) mangaParams.append("ids[]", id);
    const mangaResp = await upstreamMangaDex(`/manga?${mangaParams.toString()}`).catch(() => null);
    if (mangaResp) {
      const mangaJson = (await mangaResp.json()) as { data?: MdManga[] };
      for (const m of mangaJson.data ?? []) {
        const mapped = mapMangaDexSummary(m, requestUrl, lang === "en" ? ["en", "es"] : ["es", "es-la", "en"]);
        mangaCovers.set(m.id, { title: mapped.title, cover: mapped.coverUrl || undefined });
      }
    }
  }

  const items = chapters.flatMap((ch) => {
    const mangaRel = ch.relationships?.find((rel) => rel.type === "manga");
    if (!mangaRel?.id) return [];
    const meta = mangaCovers.get(mangaRel.id);
    const title = meta?.title || pickLocalizedText(mangaRel.attributes?.title, ["es", "es-la", "en"]) || mangaRel.id;
    const num = ch.attributes?.chapter?.trim();
    const chLabel = num ? (lang === "en" ? `Ch. ${num}` : `Cap. ${num}`) : ch.attributes?.title?.trim() || "Cap.";
    const published = (ch.attributes?.readableAt || ch.attributes?.publishAt || "").slice(0, 10) || undefined;
    return [
      {
        id: mangaRel.id,
        provider: "mangadex",
        title,
        cover: meta?.cover,
        published,
        chapterId: ch.id,
        chapterTitle: chLabel,
      },
    ];
  });

  const total = payload.total ?? offset + chapters.length;
  return {
    items,
    hasMore: chapters.length > 0 && offset + chapters.length < total && offset + limit < 2000,
  };
}

async function mangaDexSearch(
  requestUrl: URL,
  query: string,
  page: number,
  tags: string[] = [],
  lang = "es",
): Promise<{ items: NormalizedSeriesSummary[]; hasNext: boolean }> {
  const limit = 20;
  const offset = Math.max(0, (page - 1) * limit);
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  if (query.trim()) {
    params.set("title", query.trim());
  } else {
    params.set("order[followedCount]", "desc");
  }

  const normalizedTags = tags.map((t) => t.trim().toLowerCase()).filter(Boolean);
  const hasEcchi = normalizedTags.includes("ecchi");
  if (hasEcchi) {
    params.append("contentRating[]", "suggestive");
    params.append("contentRating[]", "erotica");
  } else {
    params.append("contentRating[]", "safe");
    params.append("contentRating[]", "suggestive");
  }

  if (normalizedTags.length > 0) {
    if (lang === "en") {
      params.append("availableTranslatedLanguage[]", "en");
    } else {
      params.append("availableTranslatedLanguage[]", "es");
      params.append("availableTranslatedLanguage[]", "es-la");
    }

    const demographics = new Set(["shonen", "shounen", "seinen", "shojo", "shoujo", "josei"]);
    const tagMap = await getMangaDexTagMap();
    for (const tag of normalizedTags) {
      if (tag === "ecchi") continue;
      if (demographics.has(tag)) {
        const mappedDemo = tag === "shonen" ? "shounen" : tag === "shojo" ? "shoujo" : tag;
        params.append("publicationDemographic[]", mappedDemo);
        continue;
      }
      const canonical = CANONICAL_MANGADEX_TAGS[tag] ?? tag;
      const uuid = tagMap.get(canonical);
      if (uuid) params.append("includedTags[]", uuid);
    }
  }

  params.append("includes[]", "cover_art");
  const resp = await upstreamMangaDex(`/manga?${params.toString()}`);
  const payload = (await resp.json()) as { data?: MdManga[]; total?: number };
  const items = (payload.data ?? []).map((item) => ({
    ...mapMangaDexSummary(item, requestUrl, lang === "en" ? ["en", "es", "es-la"] : ["es", "es-la", "en"]),
    provider: "mangadex",
  }));
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

async function weebCentralSearch(
  requestUrl: URL,
  query: string,
  page: number,
): Promise<{ items: NormalizedSeriesSummary[]; hasNext: boolean }> {
  const limit = 24;
  const offset = Math.max(0, (page - 1) * limit);
  const trimmed = query.trim();
  const params = new URLSearchParams({
    text: trimmed,
    limit: String(limit),
    offset: String(offset),
    sort: trimmed ? "Best Match" : "Popularity",
    order: "Descending",
    official: "Any",
    anime: "Any",
    adult: "Any",
    display_mode: "Full Display",
  });
  const html = await (await upstreamWeebCentral(`/search/data?${params.toString()}`)).text();
  const blocks = html.split(/<article\s+class="bg-base-300\b/i).slice(1);
  const items: NormalizedSeriesSummary[] = [];
  const seenIds = new Set<string>();

  for (const block of blocks) {
    const idMatch = block.match(/href="(?:https:\/\/weebcentral\.com)?\/series\/([A-Z0-9]+)(?:\/[^"]*)?"/i);
    if (!idMatch) continue;
    const id = idMatch[1];
    if (!WEEBCENTRAL_ID.test(id) || seenIds.has(id)) continue;
    seenIds.add(id);

    const coverRaw =
      block.match(/<source[^>]+srcset="(https:\/\/[^"]+)"/i)?.[1] ??
      block.match(/<img[^>]+src="(https:\/\/[^"]+)"/i)?.[1] ??
      "";
    const altTitle = block.match(/<img[^>]+alt="([^"]+)"/i)?.[1]?.replace(/\s+cover$/i, "") ?? "";
    const anchorTitle =
      block.match(
        /<a[^>]+href="(?:https:\/\/weebcentral\.com)?\/series\/[A-Z0-9]+\/[^"]*"[^>]*class="[^"]*link-hover[^"]*"[^>]*>([\s\S]*?)<\/a>/i,
      )?.[1] ?? "";
    const title = plainText(anchorTitle || altTitle) || id;

    const genres = Array.from(
      new Set(
        Array.from(block.matchAll(/<a[^>]+included_tag=[^>]*>([\s\S]*?)<\/a>/gi))
          .map((m) => plainText(m[1]))
          .filter(Boolean),
      ),
    );

    items.push({
      id,
      provider: "weebcentral",
      title,
      description: null,
      coverUrl: coverRaw ? buildImageProxyUrl(requestUrl, coverRaw) : "",
      genres,
      languages: ["en"],
    });
  }

  return {
    items,
    hasNext: items.length >= limit,
  };
}

async function weebCentralPopular(requestUrl: URL): Promise<{ items: NormalizedSeriesSummary[] }> {
  const res = await weebCentralSearch(requestUrl, "", 1);
  return { items: res.items.slice(0, 16) };
}

async function weebCentralSeriesDetail(
  requestUrl: URL,
  id: string,
  order: "asc" | "desc",
  page: number,
): Promise<NormalizedSeriesDetail> {
  const [seriesHtml, chaptersHtml] = await Promise.all([
    (await upstreamWeebCentral(`/series/${id}`)).text(),
    (await upstreamWeebCentral(`/series/${id}/full-chapter-list`)).text(),
  ]);

  const title =
    plainText(seriesHtml.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/i)?.[1] ?? "") ||
    plainText(seriesHtml.match(/<title>([\s\S]*?)\s*\|\s*Weeb Central<\/title>/i)?.[1] ?? "") ||
    id;

  const coverRaw =
    seriesHtml.match(/<meta[^>]+property="og:image"[^>]+content="([^"]+)"/i)?.[1] ??
    seriesHtml.match(/<meta[^>]+content="([^"]+)"[^>]+property="og:image"/i)?.[1] ??
    "";
  const coverUrl = coverRaw ? buildImageProxyUrl(requestUrl, coverRaw) : null;

  const rawDesc =
    seriesHtml.match(/<p[^>]*class="[^"]*whitespace-pre-line[^"]*"[^>]*>([\s\S]*?)<\/p>/i)?.[1] ??
    seriesHtml.match(/<meta[^>]+name="description"[^>]+content="([^"]+)"/i)?.[1] ??
    "";
  const description = rawDesc ? plainText(rawDesc) : null;

  const lowerStatus = (
    seriesHtml.match(/Status:<\/strong>[\s\S]*?<a[^>]*>([\s\S]*?)<\/a>/i)?.[1] ??
    seriesHtml.match(/Status:<\/strong>\s*([^<\n]+)/i)?.[1] ??
    ""
  ).toLowerCase();
  const status = lowerStatus.includes("complete")
    ? "completed"
    : lowerStatus.includes("hiatus")
      ? "hiatus"
      : lowerStatus.includes("cancel")
        ? "cancelled"
        : "ongoing";

  const genres = Array.from(
    new Set(
      Array.from(seriesHtml.matchAll(/<a[^>]+href="[^"]*\/search\?[^"]*included_tag=[^"]*"[^>]*>([\s\S]*?)<\/a>/gi))
        .map((m) => plainText(m[1]))
        .filter(Boolean),
    ),
  );

  const author =
    plainText(
      seriesHtml.match(/<a[^>]+href="[^"]*\/search\?[^"]*author=[^"]*"[^>]*>([\s\S]*?)<\/a>/i)?.[1] ?? "",
    ) || null;

  const parsedChapters: NormalizedChapterItem[] = [];
  const seenChapters = new Set<string>();
  const chunks = chaptersHtml.split(/(?=<a\b[^>]*href="(?:https:\/\/weebcentral\.com)?\/chapters\/)/i);
  let lastChunkDate: string | null = null;

  for (const chunk of chunks) {
    const chunkDate =
      chunk.match(/datetime="(\d{4}-\d{2}-\d{2})/i)?.[1] ??
      chunk.match(/checkNewChapter\('(\d{4}-\d{2}-\d{2})/i)?.[1] ??
      null;
    const chMatch = chunk.match(
      /^<a\b[^>]*href="(?:https:\/\/weebcentral\.com)?\/chapters\/([A-Z0-9]+)"[^>]*>([\s\S]*?)<\/a>/i,
    );
    if (!chMatch) {
      if (chunkDate) lastChunkDate = chunkDate;
      continue;
    }

    const chId = chMatch[1];
    if (!WEEBCENTRAL_ID.test(chId) || seenChapters.has(chId)) continue;
    seenChapters.add(chId);

    const anchorText = plainText(chMatch[2])
      .replace(/\bLast Read\b/gi, "")
      .replace(/\d{4}-\d{2}-\d{2}T[\d:.]+Z?/g, "")
      .trim();
    const numMatch = anchorText.match(/\d+(?:\.\d+)?/);
    const chNum = numMatch?.[0] ?? "?";
    const pubDate = chunkDate ?? lastChunkDate;
    lastChunkDate = chunkDate;

    parsedChapters.push({
      id: chId,
      title: anchorText || `Chapter ${chNum}`,
      chapterNumber: chNum,
      volumeNumber: null,
      language: "en",
      scanlator: "WeebCentral",
      publicationDate: pubDate,
    });
  }

  parsedChapters.sort((a, b) => {
    const na = Number.parseFloat(a.chapterNumber);
    const nb = Number.parseFloat(b.chapterNumber);
    if (Number.isFinite(na) && Number.isFinite(nb) && na !== nb) {
      return order === "asc" ? na - nb : nb - na;
    }
    return order === "asc" ? a.id.localeCompare(b.id) : b.id.localeCompare(a.id);
  });

  const offset = (page - 1) * CHAPTERS_PER_PAGE;
  const sliced = parsedChapters.slice(offset, offset + CHAPTERS_PER_PAGE);

  return {
    id,
    provider: "weebcentral",
    title,
    coverUrl,
    description,
    author,
    artist: null,
    status,
    genres,
    availableLanguages: ["en"],
    activeLanguage: "en",
    order,
    page,
    totalChapters: parsedChapters.length,
    hasNextPage: offset + sliced.length < parsedChapters.length,
    chapters: sliced,
  };
}

async function weebCentralPages(requestUrl: URL, chapterId: string): Promise<{ pages: string[] }> {
  const html = await (
    await upstreamWeebCentral(`/chapters/${chapterId}/images?is_prev=False&current_page=1&reading_style=long_strip`)
  ).text();
  const pages: string[] = [];
  for (const match of html.matchAll(/<img\b[^>]*\bsrc="(https:\/\/[^"]+)"/gi)) {
    const imgUrl = match[1];
    if (!imgUrl || imgUrl.includes("/static/")) continue;
    pages.push(buildImageProxyUrl(requestUrl, imgUrl));
  }
  if (pages.length === 0) {
    throw new Error("WeebCentral no devolvió páginas para este capítulo");
  }
  return { pages };
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
    !isAllowedImageHost(target.hostname)
  ) {
    return jsonResponse({ error: "Origen de imagen no permitido" }, 403);
  }

  const referer =
    target.hostname.toLowerCase() === "uploads.mangadex.org"
      ? "https://mangadex.org/"
      : isWeebCentralHost(target.hostname)
        ? `${WEEBCENTRAL_ORIGIN}/`
        : `${MANGAONI_ORIGIN}/`;
  const ua = isWeebCentralHost(target.hostname) ? WEEBCENTRAL_USER_AGENT : USER_AGENT;

  const resp = await fetchWithTimeout(
    target.href,
    {
      headers: {
        "User-Agent": ua,
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
        return jsonResponse({ version: 1, providers: ["mangaoni", "mangadex", "weebcentral"] }, 200, 300);
      }

      if (v1Route === "/v1/image") {
        const imageUrl = url.searchParams.get("url") ?? "";
        return withEdgeCache(request, () => proxyImage(imageUrl));
      }

      const provider = (url.searchParams.get("provider") ?? "mangaoni").trim().toLowerCase();

      if (v1Route === "/v1/updates") {
        const page = Math.max(1, Number.parseInt(url.searchParams.get("page") ?? "1", 10) || 1);
        const lang = (url.searchParams.get("lang") ?? "es").trim().toLowerCase();
        if (page === 1 && provider === "mangaoni" && !url.searchParams.has("page")) {
          return withEdgeCache(request, async () => jsonResponse(await mangaOniUpdates(), 200, 120));
        }
        return withEdgeCache(request, async () => {
          if (provider === "mangaoni" && page === 1) {
            const items = (await mangaOniUpdates()).map((entry) => ({ ...entry, provider: "mangaoni" }));
            return jsonResponse({ items, hasMore: true }, 200, 120);
          }
          const mdPage = provider === "mangadex" ? page : Math.max(1, page - 1);
          return jsonResponse(await mangaDexLatestChapters(url, mdPage, lang), 200, 120);
        });
      }

      if (v1Route === "/v1/popular") {
        const lang = (url.searchParams.get("lang") ?? "es").trim().toLowerCase();
        return withEdgeCache(request, async () => {
          if (provider === "mangadex") {
            return jsonResponse(await mangaDexPopular(url, lang), 200, 300);
          }
          if (provider === "weebcentral") {
            return jsonResponse(await weebCentralPopular(url), 200, 300);
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
        const tags = (url.searchParams.get("tags") ?? "")
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean);
        const lang = (url.searchParams.get("lang") ?? "es").trim().toLowerCase();
        const page = Math.max(1, Number.parseInt(url.searchParams.get("page") ?? "1", 10) || 1);
        if ((!query && tags.length === 0) || query.length > 120) {
          return jsonResponse({ error: "Consulta inválida" }, 400);
        }
        if (provider === "weebcentral" && tags.length === 0) {
          return withEdgeCache(
            request,
            async () => jsonResponse(await weebCentralSearch(url, query, page), 200, 120),
          );
        }
        if (provider === "mangadex" || tags.length > 0) {
          return withEdgeCache(
            request,
            async () => jsonResponse(await mangaDexSearch(url, query, page, tags, lang), 200, 120),
          );
        }
        if (provider === "mangaoni") {
          const raw = await mangaOniSearchRaw(query);
          const items: NormalizedSeriesSummary[] = raw.map((item) => ({
            id: item.id,
            provider: "mangaoni",
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

        if (provider === "weebcentral" && WEEBCENTRAL_ID.test(id)) {
          return withEdgeCache(
            request,
            async () => jsonResponse(await weebCentralSeriesDetail(url, id, order, page), 200, 120),
          );
        }

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
        if (provider === "weebcentral" && WEEBCENTRAL_ID.test(rawChapterId)) {
          return withEdgeCache(request, async () => jsonResponse(await weebCentralPages(url, rawChapterId), 200, 120));
        }
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
