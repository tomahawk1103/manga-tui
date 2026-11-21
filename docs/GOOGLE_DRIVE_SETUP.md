# Guía rápida: Inicio de sesión con Google Drive en 1 clic (3 minutos)

Para que cualquier persona pueda pulsar **Iniciar sesión con Google Drive** dentro de `manga-tui` sin introducir claves ni saber qué es una API, solo necesitas configurar un proyecto gratuito en Google Cloud **una sola vez**.

> **Nota sobre seguridad:** El archivo `.env` está en `.gitignore` para que tus credenciales nunca se suban al repositorio de GitHub.

---

## Paso 1: Crear el proyecto y activar Google Drive API

1. Entra en [Google Cloud Console](https://console.cloud.google.com/).
2. Arriba a la izquierda, crea un proyecto nuevo (por ejemplo, `Manga TUI`).
3. En el menú lateral ve a **APIs y servicios → Biblioteca** (*Library*).
4. Busca **Google Drive API** y pulsa **Habilitar** (*Enable*).

---

## Paso 2: Configurar la pantalla de consentimiento

1. Ve a **APIs y servicios → Pantalla de consentimiento de OAuth** (*OAuth consent screen*) o **Google Auth Platform**.
2. Elige tipo **Externo** (*External*) y rellena:
   - **Nombre de la aplicación:** `Manga TUI`
   - **Correo de asistencia** y **Correo del desarrollador:** tu Gmail.
3. En la sección **Permisos / Scopes**, añade únicamente:
   - `https://www.googleapis.com/auth/drive.file`
   *(Permite a Manga TUI crear y leer solo su propia carpeta `MangaTUI/`, sin acceso al resto de archivos de tu Google Drive).*
4. En **Usuarios de prueba** (*Test users*), añade tu Gmail (y hasta 100 cuentas de amigos/testers si quieres probar al instante sin publicar).
   - *Cuando quieras que funcione para cualquier usuario sin añadirlo a mano, pulsa **Publicar aplicación**. Como `drive.file` es un permiso restringido únicamente a los archivos propios de la app, no requiere auditoría de pago.*

---

## Paso 3: Crear el ID de cliente (Desktop App)

1. Ve a **APIs y servicios → Credenciales** (*Credentials*).
2. Pulsa **+ Crear credenciales → ID de cliente de OAuth**.
3. En **Tipo de aplicación**, selecciona **Aplicación de escritorio** (*Desktop app*).
4. Ponle un nombre (por ejemplo, `Manga TUI Desktop`) y pulsa **Crear**.
5. Copia los dos valores que aparecen:
   - **ID de cliente** (`...apps.googleusercontent.com`)
   - **Secreto de cliente**

---

## Paso 4: Integrarlo en tu PC y en las releases de GitHub

### A) En tu PC (para compilar e instalar localmente)
1. En la raíz del proyecto, copia `.env.example` como `.env`:
   ```powershell
   Copy-Item .env.example .env
   ```
2. Abre `.env` y pega tus credenciales:
   ```env
   MANGA_TUI_GDRIVE_CLIENT_ID=tu-id.apps.googleusercontent.com
   MANGA_TUI_GDRIVE_CLIENT_SECRET=tu-client-secret
   ```
3. Recompila e instala:
   ```powershell
   cargo install --path .
   ```
   A partir de ese momento, al abrir `manga-tui` → **`C`** → **`g`**, el navegador se abrirá **directamente en 1 clic** con el login de Google.

### B) En GitHub Actions (para que los `.exe` / ZIP descargables ya vengan listos)
1. En tu repositorio de GitHub ve a **Settings → Secrets and variables → Actions**.
2. Pulsa **New repository secret** y crea estos dos secretos con los mismos valores:
   - `MANGA_TUI_GDRIVE_CLIENT_ID`
   - `MANGA_TUI_GDRIVE_CLIENT_SECRET`
3. Los workflows `nightly.yml` y `release.yml` los leerán automáticamente al generar las próximas builds.
