const cacheName = "mangora-web-demo-v1";
const assets = [
  "./",
  "./index.html",
  "./reader.css",
  "./reader.js",
  "./manifest.webmanifest",
  "../images/mangora-icon.png",
  "../images/pagina-1.jpg",
  "../images/pagina-2.jpg",
  "../images/pagina-3.jpg",
];
const cachedUrls = new Set(assets.map(path => new URL(path, self.registration.scope).href));

self.addEventListener("install", event => {
  event.waitUntil(caches.open(cacheName).then(cache => cache.addAll(assets)));
});

self.addEventListener("activate", event => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(names
      .filter(name => name.startsWith("mangora-web-demo-") && name !== cacheName)
      .map(name => caches.delete(name)));
    await self.clients.claim();
  })());
});

self.addEventListener("fetch", event => {
  if (event.request.method !== "GET" || !cachedUrls.has(event.request.url)) return;
  event.respondWith(caches.open(cacheName)
    .then(cache => cache.match(event.request))
    .then(response => response || fetch(event.request)));
});
