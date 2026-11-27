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

Tras desplegar, configura la URL permanente en `apps/mangora/.env.production` (o variable de entorno en build):

```env
VITE_MANGORA_API_URL=https://mangora-public-api.<tu-subdominio>.workers.dev/
```

Y recompila la web pública hacia `docs/mangora-web/`:

```powershell
npm --prefix apps/mangora run build:web
```
