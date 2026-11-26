const origin = "https://manga-oni.com";
const seriesId = /^(manga|manhwa|manhua|novela)\/[a-z0-9_-]+$/i;
const chapterId = /^\d+$/;

async function upstream(path: string, init?: RequestInit) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 12000);
  try {
    const response = await fetch(origin + path, { ...init, redirect: "manual", signal: controller.signal });
    if (!response.ok) throw new Error("MangaOni respondió " + response.status);
    return response;
  } finally {
    clearTimeout(timer);
  }
}

async function search(query: string) {
  const home = await upstream("/");
  const token = (await home.text()).match(/name="csrf-token"\s+content="([^"]+)"/)?.[1];
  if (!token) throw new Error("Formulario de búsqueda no disponible");
  const cookie = home.headers.getSetCookie().map((value) => value.split(";")[0]).join("; ");
  const response = await upstream("/buscar", {
    method: "POST",
    headers: { Cookie: cookie, Referer: origin + "/", "X-Requested-With": "XMLHttpRequest", "X-CSRF-TOKEN": token },
    body: new URLSearchParams({ buscar: query, _token: token }),
  });
  const result = await response.json() as { mangas: {nombre: string; url: string; img?: string}[] };
  return result.mangas.flatMap((item) => {
    const url = new URL(item.url, origin);
    const id = url.pathname.replace(/^\/|\/$/g, "");
    return url.origin === origin && seriesId.test(id) ? [{
      id, title: item.nombre, cover: item.img ? new URL(item.img, origin).href : undefined,
      language: "es", chapters: [],
    }] : [];
  });
}

function plainText(html: string) {
  return html.replace(/<[^>]*>/g, "").replace(/&amp;/g, "&").replace(/&#39;/g, "'")
    .replace(/&quot;/g, '"').replace(/&nbsp;/g, " ").trim();
}

async function updates() {
  const html = await (await upstream("/")).text();
  const entries = [];
  const seen = new Set<string>();
  for (const row of html.split(/<div\s+class="NRoaY">/i).slice(1)) {
    const series = row.match(/<a\s+href="(?:https:\/\/manga-oni\.com)?\/([^"]+)\/"[^>]*data-test="latest-update-name"[^>]*>([\s\S]*?)<\/a>/i);
    if (!series || !seriesId.test(series[1])) continue;
    const id = series[1];
    const slug = id.split("/")[1];
    const cover = row.match(/data-src="(https:\/\/oni\.ntr-files\.online\/[^"]+)"/)?.[1];
    const published = row.match(/datetime="(\d{4}-\d{2}-\d{2})[ T]/)?.[1];
    for (const link of row.matchAll(/<a\s+href="(?:https:\/\/manga-oni\.com)?\/lector\/([^/"]+)\/(\d+)\/(?:cascada\/)?"[^>]*>([^<]+)<\/a>/gi)) {
      const [, pathSlug, cid, label] = link;
      if (slug !== pathSlug || seen.has(id + "|" + cid)) continue;
      seen.add(id + "|" + cid);
      entries.push({ id, title: plainText(series[2]), cover, published, chapterId: cid, chapterTitle: plainText(label) });
    }
  }
  if (!entries.length) throw new Error("La portada de MangaOni no contiene novedades reconocibles");
  return entries;
}

async function detail(id: string) {
  const html = await (await upstream("/" + id + "/")).text();
  const slug = id.split("/")[1];
  const seen = new Set<string>();
  const chapters = [];
  for (const link of html.matchAll(/<a\b[^>]*\bhref="(?:https:\/\/manga-oni\.com)?\/lector\/([^/"?#]+)\/(\d+)\/?"[^>]*>([\s\S]*?)<\/a>/gi)) {
    const [, pathSlug, cid, body] = link;
    if (pathSlug !== slug || seen.has(cid)) continue;
    const number = body.match(/data-num="([^"]+)"/)?.[1] ?? "";
    const title = plainText(body.match(/<h3\b[^>]*>([\s\S]*?)<\/h3>/i)?.[1] ?? "") || "Capítulo " + number;
    seen.add(cid);
    chapters.push({ id: cid, title, number, manifest: "chapter?" + new URLSearchParams({ id, chapter: cid }) });
  }
  // The shared reader treats the following element as the next chapter.
  chapters.sort((a, b) => Number(a.number) - Number(b.number));
  const title = plainText(html.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/i)?.[1] ?? slug);
  return { id, title, language: "es", chapters };
}

async function pages(id: string, chapter: string) {
  const html = await (await upstream("/lector/" + id.split("/")[1] + "/" + chapter + "/")).text();
  const encoded = html.match(/\bvar\s+unicap\s*=\s*'([A-Za-z0-9+/=]+)'/)?.[1];
  if (!encoded) throw new Error("El capítulo no contiene páginas");
  const [base, files] = new TextDecoder().decode(Uint8Array.from(atob(encoded), b => b.charCodeAt(0))).split("||");
  const names: unknown = JSON.parse(files);
  if (!Array.isArray(names) || !names.length) throw new Error("Lista de páginas vacía");
  return { pages: names.map((name) => {
    if (typeof name !== "string" || !name) throw new Error("Página inválida");
    const url = new URL(base + name);
    if (url.origin !== "https://oni.ntr-files.online" || url.username || url.password) throw new Error("CDN desconocido");
    return url.href;
  }) };
}

export async function mangaOniApi(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const id = url.searchParams.get("id") ?? "";
  const chapter = url.searchParams.get("chapter") ?? "";
  const query = url.searchParams.get("q")?.trim() ?? "";
  const endpoint = url.pathname.split("/").pop();
  const headers = { "Access-Control-Allow-Origin": "*", "Cache-Control": "no-store" };
  if (request.method !== "GET") return new Response(null, { status: 405, headers });
  try {
    if (endpoint === "updates") return Response.json(await updates(), {
      headers: { ...headers, "Cache-Control": "public, max-age=120" },
    });
    if (endpoint === "search" && query && query.length <= 120) return Response.json(await search(query), {headers});
    if (endpoint === "series" && seriesId.test(id)) return Response.json(await detail(id), {headers});
    if (endpoint === "chapter" && seriesId.test(id) && chapterId.test(chapter)) return Response.json(await pages(id, chapter), {headers});
    return Response.json({ error: "Consulta inválida" }, { status: 400, headers });
  } catch (error) {
    const upstreamStatus = error instanceof Error && /^MangaOni respondió \d{3}$/.test(error.message);
    const message = upstreamStatus ? error.message : "MangaOni no pudo entregar los datos solicitados.";
    console.warn("MangaOni", endpoint, error instanceof Error ? error.message : message);
    return Response.json({ error: message }, { status: 502, headers });
  }
}

export default { fetch: mangaOniApi };
