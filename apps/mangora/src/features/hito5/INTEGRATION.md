# Integración de Hito 5 en Mangora (`apps/mangora/`)

Este paquete bajo `apps/mangora/src/features/hito5/` está aislado intencionadamente del scaffold base (Hitos 1–4) para fusionarse sin conflictos de merge sobre las ramas anteriores.

## 1. Backend Tauri (`apps/mangora/src-tauri/`)

1. Copia [`tauriCommandsTemplate.rs`](./tauriCommandsTemplate.rs) a `apps/mangora/src-tauri/src/commands/hito5.rs`.
2. Registra los 7 comandos en `tauri::generate_handler![...]` dentro de `apps/mangora/src-tauri/src/lib.rs`:
   - `mangora_get_appearance_catalog`
   - `mangora_get_appearance_config`
   - `mangora_save_appearance_config`
   - `mangora_record_reading_session`
   - `mangora_save_manga_genres`
   - `mangora_get_extended_reader_metrics`
   - `mangora_export_stats_card_svg`
3. Asegura que al arrancar la app se ejecuta `manga_core::backend::migration::update_database_with_migrations` (incluye la migración `0.12.0` con `reading_sessions` y `manga_genres`).

## 2. Frontend React (`apps/mangora/src/`)

1. Envuelve la aplicación raíz en `MangoraAppearanceProvider` (importando `./features/hito5` para cargar `palettes.css` y aplicar las CSS variables `--paper`, `--surface`, `--ink`, `--red`, `--muted`, `--line`, `--focus-ring` en tiempo real):
   ```tsx
   import { MangoraAppearanceProvider } from "./features/hito5";

   <MangoraAppearanceProvider>
     <App />
   </MangoraAppearanceProvider>
   ```
2. **Ajustes -> Apariencia**: renderiza `<AppearanceCustomizerPanel language={lang} />`.
3. **Biblioteca / Explorar**: usa `<CollectionDisplayToolbar />` junto a `sortLibraryItems` / `sortExploreItems` y las clases `.mangora-library-grid`, `.mangora-library-list` o `.mangora-explore-grid`.
4. **Lector HD**: aplica `getReaderBackgroundClass(config.reader_background)` al contenedor de lectura y activa `useReadingSessionTracker(...)` para registrar duración local y páginas vistas. Al abrir la ficha de un manga, llama a `defaultHito5Bridge.saveMangaGenres(...)` para nutrir el desglose por géneros.
5. **Métricas y tarjeta compartible**: renderiza `<ReaderMetricsDashboard language={lang} />` con KPIs 100 % locales, vista previa editorial y exportación directa a **PNG 2x** o **SVG**.
