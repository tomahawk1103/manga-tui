import type { Connect, Plugin } from "vite";
import { mangaOniApi } from "./mangaoni-api";

export function mangaOniPreview(): Plugin {
  const serve: Connect.NextHandleFunction = async (request, response, next) => {
    const { url, method } = request as { url?: string; method?: string };
    if (!url?.startsWith("/api/mangaoni/")) return next();
    const result = await mangaOniApi(new Request(new URL(url, "http://localhost"), { method }));
    response.statusCode = result.status;
    result.headers.forEach((value, name) => response.setHeader(name, value));
    response.end(await result.text());
  };
  return {
    name: "mangora-mangaoni-preview",
    configureServer(server) { server.middlewares.use(serve); },
    configurePreviewServer(server) { server.middlewares.use(serve); },
  };
}
