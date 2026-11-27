# Mangora Public API Worker

Cloudflare Worker ligero para **Mangora Web** (`docs/mangora-web/`). Actúa como puente CORS, caché en *edge* y proxy de imágenes seguro para proveedores públicos (`MangaOni`, `MangaDex` y `WeebCentral`), conservando además las rutas legadas de MangaOni (`/updates`, `/search`, `/series`, `/chapter`).

## Desarrollo local

Desde `apps/mangora/`:

```powershell
npm run dev:web
```

El middleware de Vite sirve este mismo worker bajo `./api/mangaoni/` (incluyendo `./api/mangaoni/v1/*`).

## Despliegue en Cloudflare Workers (plan gratuito)

Desde `apps/mangora/worker/`:

```powershell
npx wrangler login
npx wrangler deploy
```

Tras desplegar, `DEFAULT_PUBLIC_API` en `apps/mangora/src/web/catalog.ts` ya apunta por defecto a la URL desplegada (`https://mangora-public-api.tomahawk1103.workers.dev/`). Si despliegas en otro subdominio, configura `apps/mangora/.env.production` (o variable de entorno en build):

```env
VITE_MANGORA_API_URL=https://mangora-public-api.tomahawk1103.workers.dev/
```

Y recompila la web pública hacia `docs/mangora-web/`:

```powershell
npm --prefix apps/mangora run build:web
```
