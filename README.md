<h1 align="center">
<strong> 📖 Manga-tui 🖥️ </strong>
</h1>

<h3 align="center">
    Terminal-based manga reader and downloader written in rust 🦀
</h3>
<div align="center">
    <a href="https://github.com/josueBarretogit/manga-tui/actions/workflows/test.yml">
        <img alt="test passing" src="https://img.shields.io/github/actions/workflow/status/josueBarretogit/manga-tui/test.yml?label=tests">
    </a>
    <a href="https://crates.io/crates/manga-tui">
        <img alt="Crates.io downloads" src="https://img.shields.io/crates/d/manga-tui">
    </a>
    <a href="https://github.com/josueBarretogit/manga-tui/releases/latest">
        <img alt="downloads" src="https://img.shields.io/github/downloads/josuebarretogit/manga-tui/total">
    </a>
    <a href="https://github.com/josueBarretogit/manga-tui/blob/main/LICENSE">
        <img alt="License" src="https://img.shields.io/github/license/josueBarretogit/Manga-tui?style=flat-square&color=blue">
    </a>
</div >

<p align="center">

<https://github.com/user-attachments/assets/2b693bd3-ec30-4d6e-bcc4-6cf457a860b1>

</p>

## Table of contents

- [Features](#features)
- [Dependencies](#dependencies)
- [Installation](#installation)
- [Image rendering](#image-rendering)
- [Usage](#usage)
- [Anime](#anime)
- [Manga providers](#manga-providers)
- [Configuration](#configuration)
- [Motivation](#motivation)
- [Credits](#credits)

## Features

- [Mangadex](https://mangadex.org/), [Weebcentral](https://weebcentral.com/) and [MangaPill](https://mangapill.com) are available as manga providers (many more willl be added!)
so if any one goes down you can still use manga-tui

- Track your reading history with [anilist integration](./docs/anilist.md)

- Advanced search (with filters)

<https://github.com/user-attachments/assets/c1e21aa1-8a51-4c47-baea-9f56dcd0d6a4>

- Read manga in your terminal with terminals such as: Wezterm, iTerm2, Kitty, Ghostty
  
<https://github.com/user-attachments/assets/70f321ff-13d1-4c4b-9c37-604271456ab2>

- Reading history is stored locally (with no login required)

 <https://github.com/user-attachments/assets/47e88e89-f73c-4575-9645-2abb80ca7d63>

- Download manga (available formats: cbz, epub, pdf and raw images)

<https://github.com/user-attachments/assets/ba785668-7cf1-4367-93f9-6e6e1f72c12c>

- Download all chapters of a manga (available formats: cbz, epub, pdf and raw images)

<https://github.com/user-attachments/assets/26ad493f-633c-41fc-9d09-49b316118923>

- Get an in-app prompt for new chapters of downloaded, ongoing manga. Checks run at startup and every 10 minutes while the app is open; press `<y>` to download or `<q>` to dismiss.

- Browse anime from AnimeAV1 and JKAnime, play episodes through the local web player, and keep favorites and playback history in SQLite.

### Join the [discord](https://discord.gg/SeqsWUCpVx) server for further help, feature requests or to chat with contributors

## Dependencies

On linux you may need to install the D-bus secret service library

### Debian

```shell
sudo apt install libdbus-1-dev pkg-config
```

### Fedora

```shell
sudo dnf install dbus-devel pkgconf-pkg-config
```

### Arch

```shell
sudo pacman -S dbus pkgconf
```

## Installation

### Using cargo

```shell
cargo install manga-tui --locked
```

### AUR

You can install `manga-tui` from the [AUR](https://aur.archlinux.org/packages/manga-tui) with using an [AUR helper](https://wiki.archlinux.org/title/AUR_helpers).

```shell
paru -S manga-tui
```

### Nix

If you have the [Nix package manager](https://nixos.org/), this repo provides a flake that builds the latest git version from source.

Simply run the following:

```sh
nix run 'github:josueBarretogit/manga-tui'
```

Or, to install persistently:

```sh
nix profile install 'github:josueBarretogit/manga-tui'
```

## Binary release

Download a binary (including the **Windows Installer `.exe`** and **Windows Portable `.zip`**) from the [releases page](https://github.com/tomahawk1103/manga-tui/releases/latest).

### Windows Installer (`.exe`)

Run `manga-tui-*-windows-msvc-setup.exe` for a standard per-user Windows installation (no admin rights required, with optional all-users mode, Start Menu / Desktop shortcuts with custom icon, optional `PATH` registration, and clean uninstaller).

- Installed mode stores user data in `%APPDATA%\manga-tui` (`config.toml`, SQLite history, and `mangaDownloads/`), completely separated from the installation folder.
- Upgrading or uninstalling preserves all your user configuration, reading history, and downloaded chapters.

### Windows Portable Mode

Extract `manga-tui-*-windows-msvc-portable.zip` to any folder and double-click `manga-tui.exe` (it automatically launches maximized in Windows Terminal if installed).

Data directory precedence:
1. `MANGA_TUI_DATA_DIR` environment variable
2. `./data/` next to `manga-tui.exe` (activated automatically when `portable.txt` or `data/` exists next to the executable)
3. Standard OS directories (`%APPDATA%` / `%LOCALAPPDATA%`)

To update in portable mode, simply replace `manga-tui.exe` and keep `portable.txt` and `data/` intact (`config.toml`, reading history, and `mangaDownloads/` are preserved).

> [!NOTE]
> AniList credentials set via CLI are stored in the operating system's credential manager (`keyring`), so when moving a portable folder to a new PC you will need to set them again on that PC or configure `[anilist]` directly inside `data/config.toml`.

### Modo Portable en Windows (Español)

Descomprime `manga-tui-*-windows-msvc-portable.zip` en cualquier carpeta con permisos de escritura y haz **doble clic** en `manga-tui.exe` (se abrirá automáticamente maximizado en Windows Terminal si está instalado; en consolas sin soporte gráfico abrirá el **Visor Web HD** automáticamente al leer).

Orden de prioridad del directorio de datos:
1. Variable de entorno `MANGA_TUI_DATA_DIR`
2. `./data/` junto a `manga-tui.exe` (se activa automáticamente cuando existe `portable.txt` o `data/` junto al ejecutable)
3. Directorios estándar del sistema (`%APPDATA%` / `%LOCALAPPDATA%`)

Para actualizar sin perder datos, sustituye únicamente `manga-tui.exe` manteniendo intactos `portable.txt` y `data/` (se conservan `config.toml`, historial SQLite y `mangaDownloads/`).

> [!NOTE]
> Las credenciales de AniList configuradas por CLI se guardan en el almacén de credenciales de Windows (`keyring`), por lo que al mover la carpeta portable a otro equipo habrá que volver a configurarlas en ese PC o definir `[anilist]` dentro de `data/config.toml`. Para la guía completa en castellano incluida en el ZIP, consulta [docs/LEEME_PORTABLE.txt](docs/LEEME_PORTABLE.txt).

### Instalador de Windows (Español)

También puedes descargar `manga-tui-*-windows-msvc-setup.exe` desde la página de releases para instalarlo de forma clásica en Windows (por usuario sin necesidad de administrador, o para todos los usuarios), con:

- Icono oficial multi-resolución y metadatos Win32 embebidos en `manga-tui.exe`.
- Accesos directos en el Menú Inicio y (opcionalmente) en el Escritorio.
- Registro opcional en el `PATH` del sistema/usuario para ejecutar `manga-tui` desde cualquier terminal.
- Desinstalador limpio que **conserva siempre tus datos de usuario** (`config.toml`, historial SQLite y descargas en `%APPDATA%\manga-tui`) tanto al actualizar como al desinstalar.

## Image rendering

Use a terminal that can render images such as [Wezterm](https://wezfurlong.org/wezterm/index.html) (Personally I recommend using this one It's the one used in the videos), [iTerm2](https://iterm2.com/), [Kitty](https://sw.kovidgoyal.net/kitty/) and [Ghostty](https://ghostty.org/download)  <br />
For more information see: [image-support](https://github.com/benjajaja/ratatui-image?tab=readme-ov-file#compatibility-matrix)

> [!WARNING]
> On windows image display is very buggy, see [this issue](https://github.com/josueBarretogit/manga-tui/issues/26) for more information

No images will be displayed if the terminal does not have image support  (but `manga-tui` will still work as a manga downloader)

## Usage

After installation just run the binary

```shell
manga-tui
```

## Anime

Open **Anime** with `F9` or click the tab. Search for a series, press `Enter` to open its episodes, and press `Enter` again to play the selected episode. Use `Tab` to move between the search field, results, and episodes.

| Key | Action |
| --- | --- |
| `p` | Switch between AnimeAV1 and JKAnime |
| `s` | Pick a mirror and its SUB or DUB variant |
| `w` / `v` | Open in the browser or an external player |
| `h` / `b` | Open recent history or favorites |
| `f` / `m` | Toggle favorite or watched status |

The browser player uses a local proxy and saves your position. When playback reaches 85%, the episode is marked as watched by default. If the selected mirror cannot serve media, the app tries compatible mirrors and can switch catalogs when it finds a unique matching title.

Use **Settings** (`F5`) to select the default provider, player and variant. For a VLC or mpv installation outside your PATH, set `custom_player_path` in `config.toml`.

```toml
[anime]
preferred_provider = "animeav1"
player = "web" # web, vlc, mpv
custom_player_path = ""
preferred_variant = "sub" # sub, dub
auto_mark_watched_percent = 85
```

## Manga providers

> [!WARNING]
> Expect any manga provider to fail at some point, either due to them closing operations due to a [lawsuit](https://www.japantimes.co.jp/news/2024/04/18/japan/crime-legal/manga-mura-copyright-ruling/) or the provider itself having issues on their end like [manganato](https://github.com/josueBarretogit/manga-tui/issues/132)

By default when you run `manga-tui` Mangadex will be used

```shell
manga-tui
```

If you want to use Weebcentral or any other provider available then run:

```shell
manga-tui -p weebcentral
```

## Configuration

The config file is located at `XDG_CONFIG_HOME/manga-tui/config.toml`, to know where it is you can run:

```shell
manga-tui --config-dir 

# or

manga-tui -c
```

Which provides the following configuration:

```toml
# The format of the manga downloaded
# values: cbz, raw, epub, pdf
# default: "cbz"
download_type = "cbz"

# Download image quality, low quality means images are compressed and is recommended for slow internet connections
# values: low, high 
# default: "low"
image_quality = "low"

# Pages around the currently selected page to try to prefetch
# values: 0-255
# default: 5
amount_pages = 5

# Whether or not bookmarking is done automatically, if false you decide which chapter to bookmark
# values: true, false
# default: true
auto_bookmark = true

# Whether or not downloading a manga counts as reading it on services like anilist
# values: true, false
# default: false
track_reading_when_download = false

# Enable / disable checking for new updates
# values: true, false
# default: true
check_new_updates = true

# Sets which manga provider will be used when running manga-tui, 
# you can override it by running manga-tui with the -p flag like this: manga-tui -p weebcentral
# values: mangadex, weebcentral, mangapill
# default: "mangadex"
default_manga_provider = "mangadex"

# Enable / disable tracking reading history with services like `anilist`
# values: true, false
# default: true
track_reading_history = true

# Anilist-related config, if you want `manga-tui` to read your anilist credentials from this file then place them here
[anilist]
# Your client id from your anilist account
# leave it as an empty string "" if you don't want to use the config file to read your anilist credentials
# values: string
# default: ""
client_id = ""

# Your acces token from your anilist account
# leave it as an empty string "" if you don't want to use the config file to read your anilist credentials
# values: string
# default: ""
access_token = ""
```

Manga downloads and reading history is stored in the `manga-tui` directory, to know where it is run:

```shell
manga-tui --data-dir 

# or

manga-tui -d
```

On linux it will output something like: `~/.local/share/manga-tui` <br />

On the `manga-tui` directory there will be 3 directories

- `history`, which contains a sqlite database to store reading history
- `mangaDownloads`, where manga will be downloaded by default
- `errorLogs`, for storing posible errors / bugs

To customize only the download location (e.g. for Google Drive or an external drive), set `download_dir` in `config.toml`:
```toml
download_dir = "/some-directory/Mangas"
```

You can view the active download directory by running:
```shell
manga-tui --download-dir
```

If you want to change the location of the entire data directory you can set the environment variable `MANGA_TUI_DATA_DIR` to some path pointing to a directory, like: <br />

```shell
export MANGA_TUI_DATA_DIR="/home/user/Desktop/mangas"
```

> [!NOTE]
> Mangadex-only feature
By default `manga-tui` will search mangas in english, you can change the language by running:

```shell
# `es` corresponds to the Iso code for spanish
manga-tui lang --set 'es'
```

> [!NOTE]
> Mangadex-only feature
Check the available languages and their Iso codes by running:

```shell
manga-tui lang --print
```

## Motivation

I wanted to make a "How linux user does ..." but for manga, [here is the video](https://www.youtube.com/watch?v=K0FsGRqEc1c) also this is a great excuse to start reading manga again

## Credits

Many thanks to Mangadex for providing the free API please consider supporting them ❤️  <br />
Many thanks to the [Ratatui organization](https://github.com/ratatui-org) for making such a good library for making TUI's in rust 🐭 <br />
Many thanks to the developer of the [Ratatui-image crate](https://crates.io/crates/ratatui-image) for providing a widget that renders images in the terminal 🖼️ <br />

Consider giving a star to this project ⭐
