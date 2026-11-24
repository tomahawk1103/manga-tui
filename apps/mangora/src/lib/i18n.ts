import type { NavSection, UiLanguage } from "../types/mangora";

export interface GenreOption {
  id: string;
  labelEs: string;
  labelEn: string;
  hintEs: string;
  hintEn: string;
}

export const GENRE_OPTIONS: GenreOption[] = [
  {
    id: "shonen",
    labelEs: "Shonen / Acción",
    labelEn: "Shonen / Action",
    hintEs: "Aventuras, combates y ritmo ágil",
    hintEn: "Adventures, battles, and high energy",
  },
  {
    id: "seinen",
    labelEs: "Seinen",
    labelEn: "Seinen",
    hintEs: "Tramas adultas, psicológicas y densas",
    hintEn: "Mature, psychological, and grounded stories",
  },
  {
    id: "romance",
    labelEs: "Romance",
    labelEn: "Romance",
    hintEs: "Historias emocionales y relaciones",
    hintEn: "Emotional stories and relationships",
  },
  {
    id: "fantasy",
    labelEs: "Fantasía",
    labelEn: "Fantasy",
    hintEs: "Mundos mágicos, gremios e isekai",
    hintEn: "Magic worlds, guilds, and isekai",
  },
  {
    id: "slice_of_life",
    labelEs: "Slice of Life",
    labelEn: "Slice of Life",
    hintEs: "Vida cotidiana tranquila y costumbrista",
    hintEn: "Calm everyday moments and cozy pacing",
  },
  {
    id: "comedy",
    labelEs: "Comedia",
    labelEn: "Comedy",
    hintEs: "Humor rápido y situaciones absurdas",
    hintEn: "Sharp humor and playful situations",
  },
  {
    id: "mystery",
    labelEs: "Misterio",
    labelEn: "Mystery",
    hintEs: "Detectives, suspense e intriga",
    hintEn: "Detectives, suspense, and intrigue",
  },
  {
    id: "scifi",
    labelEs: "Ciencia Ficción",
    labelEn: "Sci-Fi",
    hintEs: "Cyberpunk, espacio y distopías",
    hintEn: "Cyberpunk, space, and dystopian futures",
  },
  {
    id: "horror",
    labelEs: "Terror",
    labelEn: "Horror",
    hintEs: "Atmósfera oscura y horror sobrenatural",
    hintEn: "Dark atmosphere and supernatural dread",
  },
  {
    id: "sports",
    labelEs: "Deportes",
    labelEn: "Sports",
    hintEs: "Superación, equipo y competición",
    hintEn: "Teamwork, grit, and competition",
  },
];

export interface TourCardCopy {
  glyph: string;
  section: NavSection;
  title: string;
  description: string;
}

export interface Translations {
  brandSubtitle: string;
  nav: Record<NavSection, string>;
  navMobile: Record<NavSection, string>;
  themePaper: string;
  themeInk: string;
  themeToggleLabel: string;
  folderDesktopOnly: string;
  sharedStorageBadge: string;
  topBar: {
    providerPrefix: string;
    languageLabel: string;
  };
  onboarding: {
    eyebrow: string;
    stepOf: (step: number, total: number) => string;
    step1Title: string;
    step1Subtitle: string;
    esCardTitle: string;
    esCardDesc: string;
    enCardTitle: string;
    enCardDesc: string;
    step2Title: string;
    step2Subtitle: string;
    genresLabel: string;
    genresHelper: string;
    recommendedTitle: string;
    recommendedNote: string;
    changeProviderToggle: string;
    hideProviderToggle: string;
    recommendedTag: string;
    step3DetectedTitle: string;
    step3DetectedSubtitle: string;
    step3DetectedBadge: string;
    step3ShowTourAlso: string;
    step3HideTourAlso: string;
    step3TourTitle: string;
    step3TourSubtitle: string;
    back: string;
    next: string;
    finish: string;
    saving: string;
    cancelReplay: string;
    statsSeries: string;
    statsChapters: string;
    statsFavorites: string;
    statsOffline: string;
    lastReadPrefix: string;
    tourCards: TourCardCopy[];
  };
  home: {
    eyebrow: string;
    titleLine1: string;
    titleLine2: string;
    subtitle: string;
    detectedBannerTitle: string;
    detectedBannerDesc: string;
    emptyBannerTitle: string;
    emptyBannerDesc: string;
    ctaExplore: string;
    ctaTour: string;
    statSeries: string;
    statChaptersRead: string;
    statFavorites: string;
    statOfflineSeries: string;
    lastReadLabel: string;
    activeProfileTitle: string;
    activeProviderLabel: string;
    activeGenresLabel: string;
    noGenresYet: string;
    nextMilestoneTitle: string;
    nextMilestoneDesc: string;
  };
  library: {
    eyebrow: string;
    title: string;
    subtitle: string;
    searchPlaceholder: string;
    refreshBtn: string;
    loading: string;
    tabs: {
      continue: string;
      favorites: string;
      recent: string;
    };
    addFavorite: string;
    removeFavorite: string;
    favoriteAdded: string;
    favoriteRemoved: string;
    chaptersReadBadge: (count: number) => string;
    downloadedBadge: (count: number) => string;
    lastChapterPrefix: string;
    bookmarkBadge: (chapter: string, page?: number | null) => string;
    planToReadBadge: string;
    emptyContinue: string;
    emptyFavorites: string;
    emptyRecent: string;
    emptyFilter: string;
    emptyCta: string;
  };
  explore: {
    eyebrow: string;
    title: string;
    subtitle: string;
    forYouTitle: string;
    noGenresHint: string;
    editGenresBtn: string;
    providerSelectorTitle: string;
    providerSelectorDesc: string;
    activeTag: string;
    recommendedTag: string;
    search: string;
    searchTitle: string;
    loading: string;
    empty: string;
    unavailable: string;
    chooseAnotherSource: string;
    changeSource: string;
    viewDetails: string;
    providerLabel: string;
    closeDetails: string;
    noSynopsis: string;
    desktopOnly: string;
    previous: string;
    next: string;
    language: string;
    genre: string;
    all: string;
  };
  downloads: {
    eyebrow: string;
    title: string;
    subtitle: string;
    ribbonLabel: string;
    formats: string[];
    folderCardTitle: string;
    folderCardDesc: string;
    openFolderBtn: string;
    mobileStorageNote: string;
    detectedOfflineTitle: string;
    detectedOfflineCount: (count: number) => string;
    queuePreviewTitle: string;
    queuePreviewDesc: string;
  };
  settings: {
    eyebrow: string;
    title: string;
    subtitle: string;
    appearanceTitle: string;
    languageLabel: string;
    themeLabel: string;
    paperName: string;
    paperDesc: string;
    inkName: string;
    inkDesc: string;
    providerTitle: string;
    providerDesc: string;
    genresTitle: string;
    genresDesc: string;
    saveGenresBtn: string;
    storageTitle: string;
    storageModeLabel: string;
    dataDirLabel: string;
    configDirLabel: string;
    downloadsDirLabel: string;
    openFolderBtn: string;
    mobileStorageNote: string;
    tourTitle: string;
    tourDesc: string;
    relaunchTourBtn: string;
    savedToast: string;
  };
  support: {
    eyebrow: string;
    title: string;
    subtitle: string;
    discordTitle: string;
    discordDesc: string;
    discordBtn: string;
    githubTitle: string;
    githubDesc: string;
    githubBtn: string;
    monsterTitle: string;
    monsterDesc: string;
    monsterBadge: string;
    privacyTitle: string;
    privacyDesc: string;
  };
  errors: {
    loadFailed: string;
    retry: string;
  };
}

const ES: Translations = {
  brandSubtitle: "Lector de manga",
  nav: {
    home: "Inicio / Biblioteca",
    explore: "Explorar",
    downloads: "Descargas",
    settings: "Ajustes",
    support: "Apoyar / Discord",
  },
  navMobile: {
    home: "Biblioteca",
    explore: "Explorar",
    downloads: "Descargas",
    settings: "Ajustes",
    support: "Apoyar",
  },
  themePaper: "Modo Papel",
  themeInk: "Modo Tinta",
  themeToggleLabel: "Tema visual",
  folderDesktopOnly: "Disponible en la aplicación de escritorio.",
  sharedStorageBadge: "Base compartida con manga-tui",
  topBar: {
    providerPrefix: "Servidor",
    languageLabel: "Idioma",
  },
  onboarding: {
    eyebrow: "Primeros pasos · Mangora",
    stepOf: (step, total) => `Paso ${step} de ${total}`,
    step1Title: "Elige tu idioma",
    step1Subtitle:
      "Mangora ajustará todos los menús y te recomendará el mejor servidor de manga para empezar sin configuraciones complicadas.",
    esCardTitle: "Español",
    esCardDesc: "Interfaz en español y prioridad a servidores hispanos (InManga recomendado).",
    enCardTitle: "English",
    enCardDesc: "English interface with high-availability English catalog (WeebCentral recommended).",
    step2Title: "Tus gustos y servidor",
    step2Subtitle:
      "Marca los géneros que más sueles leer para personalizar tus recomendaciones y confirma el servidor inicial.",
    genresLabel: "Géneros que te interesan (opcional, puedes elegir varios)",
    genresHelper: "Usaremos esta selección en Explorar para mostrarte series afines.",
    recommendedTitle: "Servidor recomendado para tu idioma",
    recommendedNote:
      "Lo seleccionamos automáticamente para que puedas buscar y leer al instante. Siempre podrás cambiarlo en Explorar o Ajustes.",
    changeProviderToggle: "Elegir otro servidor ahora",
    hideProviderToggle: "Usar el recomendado y ocultar lista",
    recommendedTag: "Recomendado",
    step3DetectedTitle: "Biblioteca detectada y lista",
    step3DetectedSubtitle:
      "Hemos encontrado datos previos de manga-tui en este equipo. Tu historial, favoritos y descargas ya están sincronizados con Mangora.",
    step3DetectedBadge: "Compatibilidad total SQLite + mangaDownloads",
    step3ShowTourAlso: "Ver también el mini-tour de secciones",
    step3HideTourAlso: "Ocultar mini-tour",
    step3TourTitle: "Tu escritorio de manga en 4 zonas",
    step3TourSubtitle:
      "Todo está organizado en papel y tinta para que encuentres tu próxima lectura en segundos, sin comandos de terminal.",
    back: "Atrás",
    next: "Siguiente",
    finish: "Empezar con Mangora",
    saving: "Guardando...",
    cancelReplay: "Volver a la app",
    statsSeries: "Series en biblioteca",
    statsChapters: "Capítulos leídos",
    statsFavorites: "Favoritos",
    statsOffline: "Series descargadas",
    lastReadPrefix: "Última lectura detectada:",
    tourCards: [
      {
        glyph: "[?]",
        section: "explore",
        title: "Explorar",
        description:
          "Busca series por título, filtra por idioma o cambia de servidor en un clic con portadas en alta definición.",
      },
      {
        glyph: "[■]",
        section: "home",
        title: "Biblioteca",
        description:
          "Retoma donde lo dejaste, gestiona tus favoritos y comparte progreso al instante con manga-tui.",
      },
      {
        glyph: "[↓]",
        section: "downloads",
        title: "Descargas",
        description:
          "Guarda capítulos en CBZ, EPUB, PDF o imágenes RAW para leer 100 % sin conexión donde quieras.",
      },
      {
        glyph: "[⚙]",
        section: "settings",
        title: "Ajustes",
        description:
          "Cambia entre Modo Papel y Modo Tinta nocturno, idioma, servidor por defecto o repite este tour cuando quieras.",
      },
    ],
  },
  home: {
    eyebrow: "Inicio · Biblioteca compartida",
    titleLine1: "Tu manga,",
    titleLine2: "sin terminal.",
    subtitle:
      "Interfaz editorial en papel y tinta conectada al mismo motor y base de datos que manga-tui.",
    detectedBannerTitle: "Biblioteca compartida activa",
    detectedBannerDesc:
      "Mangora está leyendo directamente tu SQLite y carpeta de descargas existentes. Cualquier cambio aquí o en manga-tui se refleja al instante.",
    emptyBannerTitle: "Listo para tu primera serie",
    emptyBannerDesc:
      "Tu biblioteca local todavía no tiene lecturas registradas. Pasa por Explorar para añadir tus primeros mangas o abre tus archivos offline.",
    ctaExplore: "Buscar manga",
    ctaTour: "Ver guía rápida",
    statSeries: "Series",
    statChaptersRead: "Capítulos leídos",
    statFavorites: "Favoritos",
    statOfflineSeries: "Series offline",
    lastReadLabel: "Última lectura",
    activeProfileTitle: "Perfil inicial configurado",
    activeProviderLabel: "Servidor principal",
    activeGenresLabel: "Tus gustos guardados",
    noGenresYet: "Sin géneros seleccionados todavía (puedes elegirlos en Ajustes).",
    nextMilestoneTitle: "Próximo paso en camino (Hito 2)",
    nextMilestoneDesc:
      "La biblioteca ya lee y escribe tu SQLite en vivo; en el siguiente bloque conectaremos la Ficha de Serie completa con capítulos, reseñas de AniList y bloque «Para ti».",
  },
  library: {
    eyebrow: "Biblioteca interactiva · SQLite",
    title: "Tus estantes en vivo.",
    subtitle:
      "Sincronizada automáticamente con manga-tui: filtra por título o marca y desmarca favoritos al instante.",
    searchPlaceholder: "Filtrar biblioteca por título...",
    refreshBtn: "Actualizar",
    loading: "Cargando biblioteca local...",
    tabs: {
      continue: "Continuar leyendo",
      favorites: "Siguiendo / Favoritos",
      recent: "Historial reciente",
    },
    addFavorite: "Añadir a favoritos",
    removeFavorite: "Quitar favorito",
    favoriteAdded: "Añadido a favoritos",
    favoriteRemoved: "Quitado de favoritos",
    chaptersReadBadge: (count) =>
      count === 1 ? "1 cap. leído" : `${count} caps. leídos`,
    downloadedBadge: (count) =>
      count === 1 ? "1 offline" : `${count} offline`,
    lastChapterPrefix: "Último:",
    bookmarkBadge: (chapter, page) =>
      page && page > 0 ? `Marcador: ${chapter} · p.${page}` : `Marcador: ${chapter}`,
    planToReadBadge: "Por leer",
    emptyContinue: "Todavía no tienes lecturas en curso en este dispositivo.",
    emptyFavorites: "Aún no tienes series en favoritos. Busca una en Explorar y márcala con el corazón.",
    emptyRecent: "No hay historial reciente registrado todavía.",
    emptyFilter: "Ninguna serie de esta pestaña coincide con tu filtro.",
    emptyCta: "Ir a Explorar",
  },
  explore: {
    eyebrow: "Explorar · Catálogo y servidores",
    title: "Encuentra tu próxima serie.",
    subtitle:
      "Busca una serie por su título en el catálogo del servidor que elijas.",
    forYouTitle: "Tus géneros favoritos",
    noGenresHint: "Aún no has elegido géneros favoritos. Márcalos en Ajustes para personalizar esta zona.",
    editGenresBtn: "Editar gustos en Ajustes",
    providerSelectorTitle: "Dónde buscar",
    providerSelectorDesc:
      "Al cambiar el servidor aquí se guarda automáticamente en config.toml y lo comparte con manga-tui.",
    activeTag: "Activo",
    recommendedTag: "Recomendado",
    search: "Buscar",
    searchTitle: "Título del manga",
    loading: "Buscando en el catálogo…",
    empty: "No hay coincidencias en esta página.",
    unavailable: "La fuente no ha respondido.",
    chooseAnotherSource: "Buscar en otra fuente",
    changeSource: "Cambiar fuente",
    viewDetails: "Ver ficha",
    providerLabel: "Fuente",
    closeDetails: "Cerrar",
    noSynopsis: "Esta fuente no incluye una sinopsis en el resultado.",
    desktopOnly: "Abre la app de Mangora (escritorio o móvil) para buscar en el catálogo.",
    previous: "Página anterior",
    next: "Página siguiente",
    language: "Idioma en esta página",
    genre: "Género en esta página",
    all: "Todos",
  },
  downloads: {
    eyebrow: "Descargas · Lectura offline",
    title: "Llévatelo sin conexión.",
    subtitle:
      "Mangora comparte la carpeta `mangaDownloads/` y soporta exactamente los mismos cuatro formatos de archivo que manga-tui.",
    ribbonLabel: "Formatos compatibles",
    formats: ["CBZ", "EPUB", "PDF", "Imágenes RAW"],
    folderCardTitle: "Carpeta compartida de descargas",
    folderCardDesc:
      "Los capítulos descargados desde manga-tui o Mangora conviven en la misma ubicación local.",
    openFolderBtn: "Abrir carpeta de descargas en el Explorador",
    mobileStorageNote:
      "En móvil las descargas se guardan en el almacenamiento privado de Mangora y puedes sincronizarlas con Google Drive en Ajustes.",
    detectedOfflineTitle: "Escáner local conectado",
    detectedOfflineCount: (count) =>
      count === 1
        ? "1 serie offline detectada actualmente en disco."
        : `${count} series offline detectadas actualmente en disco.`,
    queuePreviewTitle: "Cola en vivo y visor HD offline (Hito 3)",
    queuePreviewDesc:
      "En el Hito 3 incorporaremos la cola de descargas en tiempo real y el visor de páginas locales CBZ/EPUB/PDF/RAW sin internet.",
  },
  settings: {
    eyebrow: "Ajustes · Preferencias",
    title: "Configuración clara.",
    subtitle:
      "Sin editar archivos de texto a mano: idioma y servidor se sincronizan con `config.toml`; el tema visual y tus gustos se guardan aparte para Mangora.",
    appearanceTitle: "Apariencia e idioma",
    languageLabel: "Idioma de la aplicación",
    themeLabel: "Modo visual (Papel / Tinta)",
    paperName: "Modo Papel",
    paperDesc: "Fondo papel cálido #f8f7f1 con tinta oscura para lectura diurna.",
    inkName: "Modo Tinta",
    inkDesc: "Fondo oscuro #141413 con alto contraste suave para leer de noche.",
    providerTitle: "Servidor por defecto",
    providerDesc: "Fuente prioritaria al buscar o abrir series nuevas.",
    genresTitle: "Tus gustos / géneros favoritos",
    genresDesc: "Selecciona los géneros que mejor encajan contigo.",
    saveGenresBtn: "Guardar gustos",
    storageTitle: "Almacenamiento compartido con manga-tui",
    storageModeLabel: "Modo activo",
    dataDirLabel: "Carpeta de datos",
    configDirLabel: "Configuración",
    downloadsDirLabel: "Descargas",
    openFolderBtn: "Abrir carpeta de datos",
    mobileStorageNote:
      "En Android e iOS los datos viven en el sandbox privado de la app; usa Google Drive para sincronizarlos entre dispositivos.",
    tourTitle: "Asistente de primeros pasos",
    tourDesc: "Puedes repetir la bienvenida en cualquier momento para repasar las secciones o reconfigurar todo.",
    relaunchTourBtn: "Volver a lanzar el tour",
    savedToast: "Preferencia guardada",
  },
  support: {
    eyebrow: "Comunidad y apoyo",
    title: "Hecho con tinta abierta.",
    subtitle:
      "Mangora y manga-tui son software libre bajo licencia MIT. Tu lectura es privada y nunca enviamos telemetría oculta.",
    discordTitle: "Únete al Discord oficial",
    discordDesc:
      "Pide funciones, reporta un servidor caído o charla con otros lectores y colaboradores del proyecto.",
    discordBtn: "Abrir Discord ↗",
    githubTitle: "Código fuente en GitHub",
    githubDesc:
      "Revisa cada línea de Rust y TypeScript, consulta lanzamientos oficiales o abre una incidencia técnica.",
    githubBtn: "Ver repositorio ↗",
    monsterTitle: "¿Me invitas a un Monster? ⚡ 2 €",
    monsterDesc:
      "El apoyo voluntario ayuda a mantener el proyecto vivo. Nunca habrá muros de pago ni funciones bloqueadas. En el Hito 4 conectaremos los enlaces directos a Ko-fi y GitHub Sponsors.",
    monsterBadge: "100 % voluntario · Sin paywalls",
    privacyTitle: "Privacidad local garantizada",
    privacyDesc:
      "Tu historial, favoritos, gustos y biblioteca viven únicamente en tu equipo. Puedes usar la app sin crear ninguna cuenta.",
  },
  errors: {
    loadFailed: "No se pudo inicializar el almacenamiento local de Mangora.",
    retry: "Reintentar",
  },
};

const EN: Translations = {
  brandSubtitle: "Manga reader",
  nav: {
    home: "Home / Library",
    explore: "Explore",
    downloads: "Downloads",
    settings: "Settings",
    support: "Support / Discord",
  },
  navMobile: {
    home: "Library",
    explore: "Explore",
    downloads: "Downloads",
    settings: "Settings",
    support: "Support",
  },
  themePaper: "Paper Mode",
  themeInk: "Ink Mode",
  themeToggleLabel: "Visual theme",
  folderDesktopOnly: "Available in the desktop app.",
  sharedStorageBadge: "Shared storage with manga-tui",
  topBar: {
    providerPrefix: "Source",
    languageLabel: "Language",
  },
  onboarding: {
    eyebrow: "Getting started · Mangora",
    stepOf: (step, total) => `Step ${step} of ${total}`,
    step1Title: "Choose your language",
    step1Subtitle:
      "Mangora updates all menus immediately and picks the best manga source for your language with zero terminal setup.",
    esCardTitle: "Español",
    esCardDesc: "Spanish interface prioritizing Spanish manga sources (InManga recommended).",
    enCardTitle: "English",
    enCardDesc: "English interface with high-availability English catalog (WeebCentral recommended).",
    step2Title: "Your tastes & source",
    step2Subtitle:
      "Pick the genres you enjoy most to personalize recommendations and confirm your starting source.",
    genresLabel: "Favorite genres (optional, pick as many as you like)",
    genresHelper: "We will highlight these genres inside Explore.",
    recommendedTitle: "Recommended source for your language",
    recommendedNote:
      "Selected automatically so you can start reading right away. You can switch sources anytime in Explore or Settings.",
    changeProviderToggle: "Choose a different source now",
    hideProviderToggle: "Keep recommended and hide list",
    recommendedTag: "Recommended",
    step3DetectedTitle: "Library detected and ready",
    step3DetectedSubtitle:
      "We found existing manga-tui data on this machine. Your reading history, favorites, and downloads are already connected.",
    step3DetectedBadge: "Full SQLite + mangaDownloads compatibility",
    step3ShowTourAlso: "Also show the quick section tour",
    step3HideTourAlso: "Hide quick tour",
    step3TourTitle: "Your manga desk in 4 clear areas",
    step3TourSubtitle:
      "Everything follows the clean paper-and-ink layout so you can jump straight to reading without terminal commands.",
    back: "Back",
    next: "Next",
    finish: "Start using Mangora",
    saving: "Saving...",
    cancelReplay: "Return to app",
    statsSeries: "Series in library",
    statsChapters: "Chapters read",
    statsFavorites: "Favorites",
    statsOffline: "Downloaded series",
    lastReadPrefix: "Last read detected:",
    tourCards: [
      {
        glyph: "[?]",
        section: "explore",
        title: "Explore",
        description:
          "Search titles with HD covers, filter by language, or switch manga sources in one click.",
      },
      {
        glyph: "[■]",
        section: "home",
        title: "Library",
        description:
          "Pick up where you left off, manage favorites, and stay synced with manga-tui automatically.",
      },
      {
        glyph: "[↓]",
        section: "downloads",
        title: "Downloads",
        description:
          "Save chapters as CBZ, EPUB, PDF, or RAW images to read 100% offline anywhere.",
      },
      {
        glyph: "[⚙]",
        section: "settings",
        title: "Settings",
        description:
          "Toggle Paper and Ink modes, switch language or default source, or replay this tour anytime.",
      },
    ],
  },
  home: {
    eyebrow: "Home · Shared library",
    titleLine1: "Your manga,",
    titleLine2: "zero terminal.",
    subtitle:
      "An editorial paper-and-ink desktop experience built directly on top of the manga-tui Rust core and database.",
    detectedBannerTitle: "Shared library active",
    detectedBannerDesc:
      "Mangora reads directly from your existing SQLite history and downloads folder. Any update works across both apps.",
    emptyBannerTitle: "Ready for your first series",
    emptyBannerDesc:
      "No reading history is recorded yet. Head to Explore to browse sources or open your local offline downloads folder.",
    ctaExplore: "Find manga",
    ctaTour: "Open quick tour",
    statSeries: "Series",
    statChaptersRead: "Chapters read",
    statFavorites: "Favorites",
    statOfflineSeries: "Offline series",
    lastReadLabel: "Last read",
    activeProfileTitle: "Starter profile configured",
    activeProviderLabel: "Default source",
    activeGenresLabel: "Saved genres",
    noGenresYet: "No favorite genres selected yet (you can pick them in Settings).",
    nextMilestoneTitle: "Coming next (Milestone 2)",
    nextMilestoneDesc:
      "Your live SQLite shelves are connected; next up are the full Series page with chapters, AniList reviews, and the For You shelf.",
  },
  library: {
    eyebrow: "Interactive library · SQLite",
    title: "Your live shelves.",
    subtitle:
      "Automatically shared with manga-tui: filter by title or toggle favorites in one click.",
    searchPlaceholder: "Filter library by title...",
    refreshBtn: "Refresh",
    loading: "Loading local library...",
    tabs: {
      continue: "Continue reading",
      favorites: "Following / Favorites",
      recent: "Recent history",
    },
    addFavorite: "Add to favorites",
    removeFavorite: "Remove favorite",
    favoriteAdded: "Added to favorites",
    favoriteRemoved: "Removed from favorites",
    chaptersReadBadge: (count) =>
      count === 1 ? "1 ch. read" : `${count} chs. read`,
    downloadedBadge: (count) =>
      count === 1 ? "1 offline" : `${count} offline`,
    lastChapterPrefix: "Latest:",
    bookmarkBadge: (chapter, page) =>
      page && page > 0 ? `Bookmark: ${chapter} · p.${page}` : `Bookmark: ${chapter}`,
    planToReadBadge: "Plan to read",
    emptyContinue: "No active reading progress recorded on this device yet.",
    emptyFavorites: "No favorites yet. Search a series in Explore and mark it with the heart button.",
    emptyRecent: "No recent reading history recorded yet.",
    emptyFilter: "No series in this tab match your filter.",
    emptyCta: "Go to Explore",
  },
  explore: {
    eyebrow: "Explore · Catalog & sources",
    title: "Find your next series.",
    subtitle:
      "Search for a series by title in your chosen source’s catalog.",
    forYouTitle: "Your favorite genres",
    noGenresHint: "No favorite genres chosen yet. Select a few in Settings to personalize this section.",
    editGenresBtn: "Edit genres in Settings",
    providerSelectorTitle: "Where to search",
    providerSelectorDesc:
      "Switching the source here persists straight to config.toml and stays shared with manga-tui.",
    activeTag: "Active",
    recommendedTag: "Recommended",
    search: "Search",
    searchTitle: "Manga title",
    loading: "Searching the catalog…",
    empty: "No matches on this page.",
    unavailable: "The source didn’t respond.",
    chooseAnotherSource: "Search another source",
    changeSource: "Change source",
    viewDetails: "View details",
    providerLabel: "Source",
    closeDetails: "Close",
    noSynopsis: "This source doesn’t include a synopsis in the result.",
    desktopOnly: "Open the Mangora app (desktop or mobile) to search the catalog.",
    previous: "Previous page",
    next: "Next page",
    language: "Language on this page",
    genre: "Genre on this page",
    all: "All",
  },
  downloads: {
    eyebrow: "Downloads · Offline reading",
    title: "Read anywhere offline.",
    subtitle:
      "Mangora shares the `mangaDownloads/` folder and supports the exact same four formats as manga-tui.",
    ribbonLabel: "Supported formats",
    formats: ["CBZ", "EPUB", "PDF", "RAW Images"],
    folderCardTitle: "Shared downloads folder",
    folderCardDesc:
      "Chapters downloaded from manga-tui or Mangora live together in the same folder.",
    openFolderBtn: "Open downloads folder in Explorer",
    mobileStorageNote:
      "On mobile, downloads are stored in Mangora's private app storage and can be synced with Google Drive in Settings.",
    detectedOfflineTitle: "Local scanner connected",
    detectedOfflineCount: (count) =>
      count === 1
        ? "1 offline series currently detected on disk."
        : `${count} offline series currently detected on disk.`,
    queuePreviewTitle: "Live queue & offline HD reader (Milestone 3)",
    queuePreviewDesc:
      "Milestone 3 adds the real-time download queue and native offline reader for CBZ, EPUB, PDF, and RAW chapters.",
  },
  settings: {
    eyebrow: "Settings · Preferences",
    title: "Simple controls.",
    subtitle:
      "No hand-editing TOML files: language and default source sync with `config.toml`, while visual theme and tastes stay stored for Mangora.",
    appearanceTitle: "Appearance & language",
    languageLabel: "Application language",
    themeLabel: "Visual mode (Paper / Ink)",
    paperName: "Paper Mode",
    paperDesc: "Warm #f8f7f1 paper background with crisp dark ink for daytime reading.",
    inkName: "Ink Mode",
    inkDesc: "Deep #141413 ink background with comfortable contrast for night reading.",
    providerTitle: "Default manga source",
    providerDesc: "Primary source used when browsing or opening series.",
    genresTitle: "Favorite genres",
    genresDesc: "Pick the genres that best match your reading taste.",
    saveGenresBtn: "Save genres",
    storageTitle: "Shared storage with manga-tui",
    storageModeLabel: "Active mode",
    dataDirLabel: "Data folder",
    configDirLabel: "Config folder",
    downloadsDirLabel: "Downloads",
    openFolderBtn: "Open data folder",
    mobileStorageNote:
      "On Android and iOS, data lives in the app's private sandbox; use Google Drive to sync across devices.",
    tourTitle: "First-steps guide",
    tourDesc: "You can relaunch the onboarding assistant anytime to review sections or update your setup.",
    relaunchTourBtn: "Relaunch onboarding tour",
    savedToast: "Preference saved",
  },
  support: {
    eyebrow: "Community & support",
    title: "Built with open ink.",
    subtitle:
      "Mangora and manga-tui are MIT-licensed open source software. Your reading stays private with zero hidden telemetry.",
    discordTitle: "Join the official Discord",
    discordDesc:
      "Suggest features, report broken sources, or chat with contributors and fellow readers.",
    discordBtn: "Open Discord ↗",
    githubTitle: "Source code on GitHub",
    githubDesc:
      "Inspect every line of Rust and TypeScript, track official releases, or open a bug report.",
    githubBtn: "View repository ↗",
    monsterTitle: "Buy me a Monster? ⚡ €2",
    monsterDesc:
      "Voluntary support helps keep development moving. There are never paywalls or locked features. Milestone 4 wires direct Ko-fi and GitHub Sponsors links.",
    monsterBadge: "100% voluntary · No paywalls",
    privacyTitle: "Local-first privacy",
    privacyDesc:
      "Your history, favorites, genres, and library live locally on your computer. No account required.",
  },
  errors: {
    loadFailed: "Could not initialize Mangora local storage.",
    retry: "Retry",
  },
};

export function getTranslations(lang: UiLanguage): Translations {
  return lang === "en" ? EN : ES;
}

export function getGenreLabel(id: string, lang: UiLanguage): string {
  const found = GENRE_OPTIONS.find((item) => item.id === id);
  if (!found) return id;
  return lang === "en" ? found.labelEn : found.labelEs;
}
