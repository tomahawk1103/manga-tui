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
    openSeriesBtn: string;
  };
  explore: {
    eyebrow: string;
    title: string;
    subtitle: string;
    forYouTitle: string;
    forYouLoading: string;
    forYouMatched: (genresLabel: string) => string;
    forYouPopularFallback: string;
    forYouUnavailable: string;
    forYouEmpty: string;
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
  series: {
    backBtn: string;
    eyebrow: string;
    loading: string;
    loadErrorTitle: string;
    offlineBannerTitle: string;
    offlineBannerDesc: string;
    warningBannerTitle: string;
    authorLabel: string;
    artistLabel: string;
    statusLabel: string;
    statusNames: Record<string, string>;
    noSynopsis: string;
    nextUnreadLabel: string;
    nextUnreadAction: string;
    readerComingSoonToast: string;
    chaptersTitle: string;
    chaptersCount: (total: number) => string;
    readCountLabel: (count: number) => string;
    offlineCountLabel: (count: number) => string;
    sortNewest: string;
    sortOldest: string;
    languageLabel: string;
    filterAll: string;
    filterOffline: string;
    emptyChapters: string;
    emptyOfflineChapters: string;
    chapterPrefix: string;
    statusRead: string;
    statusUnread: string;
    statusOffline: string;
    statusOnline: string;
    markReadBtn: string;
    markUnreadBtn: string;
    markedReadToast: string;
    markedUnreadToast: string;
    openReaderBtn: string;
    downloadBtn: string;
    downloadingBtn: string;
    selectChaptersBtn: string;
    cancelSelectBtn: string;
    selectAllPageBtn: string;
    downloadSelectedBtn: (count: number) => string;
    enqueuedToast: (count: number) => string;
    communityEyebrow: string;
    communityTitle: string;
    communityLoading: string;
    communityUnavailable: string;
    communityNotFound: string;
    communityScoreLabel: string;
    communityPopularityLabel: string;
    communityFavoritesLabel: string;
    communityTagsLabel: string;
    communityReviewsTitle: string;
    communityNoReviews: string;
    communityOpenAniList: string;
    communityDiscordTitle: string;
    communityDiscordDesc: string;
    communityDiscordBtn: string;
  };
  reader: {
    backToSeries: string;
    loading: string;
    loadErrorTitle: string;
    retry: string;
    offlineBadge: (fmt: string | null) => string;
    onlineBadge: (provider: string) => string;
    chapterLabel: (num: string) => string;
    pageOf: (current: number, total: number) => string;
    prevChapter: string;
    nextChapter: string;
    prevPage: string;
    nextPage: string;
    modeLabel: string;
    modeCascade: string;
    modeSingle: string;
    modeDouble: string;
    directionLabel: string;
    dirRtl: string;
    dirLtr: string;
    fitLabel: string;
    fitWidth: string;
    fitHeight: string;
    fitOriginal: string;
    zoomOut: string;
    zoomIn: string;
    zoomReset: string;
    firstPageSingleLabel: string;
    darkCanvasLabel: string;
    pinHudLabel: string;
    fullscreenBtn: string;
    settingsBtn: string;
    endOfChapterTitle: string;
    endOfChapterSubtitle: string;
    noNextChapter: string;
    shortcutsHint: string;
    touchHint: string;
    bookmarkSavedToast: string;
  };
  downloads: {
    eyebrow: string;
    title: string;
    subtitle: string;
    ribbonLabel: string;
    formats: string[];
    formatSelectorLabel: string;
    formatDescriptions: Record<"cbz" | "pdf" | "epub" | "raw", string>;
    folderCardTitle: string;
    folderCardDesc: string;
    openFolderBtn: string;
    mobileStorageNote: string;
    detectedOfflineTitle: string;
    detectedOfflineCount: (count: number) => string;
    queueTitle: string;
    queueEmpty: string;
    pauseAllBtn: string;
    resumeAllBtn: string;
    retryFailedBtn: string;
    clearFinishedBtn: string;
    cancelAllBtn: string;
    overallProgressLabel: string;
    pagesProgress: (done: number, total: number) => string;
    statusNames: Record<string, string>;
    pauseTaskBtn: string;
    resumeTaskBtn: string;
    retryTaskBtn: string;
    cancelTaskBtn: string;
    removeTaskBtn: string;
    offlineShelfTitle: string;
    offlineShelfSubtitle: string;
    offlineShelfEmpty: string;
    offlineChaptersRead: (read: number, total: number) => string;
    openOfflineSeriesBtn: string;
    finishedToast: (title: string, ch: string) => string;
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
    openSeriesBtn: "Ver ficha y capítulos",
  },
  explore: {
    eyebrow: "Explorar · Catálogo y servidores",
    title: "Encuentra tu próxima serie.",
    subtitle:
      "Busca una serie por su título en el catálogo del servidor que elijas.",
    forYouTitle: "Para ti según tus gustos",
    forYouLoading: "Buscando recomendaciones en tu fuente activa…",
    forYouMatched: (genresLabel) => `Priorizado por tus gustos: ${genresLabel}`,
    forYouPopularFallback: "Mostrando destacados populares de tu fuente activa.",
    forYouUnavailable: "No pudimos cargar sugerencias ahora mismo.",
    forYouEmpty: "Usa el buscador de arriba para explorar el catálogo de esta fuente.",
    noGenresHint: "Aún no has elegido géneros favoritos. Márcalos en Ajustes para afinar estas recomendaciones.",
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
  series: {
    backBtn: "Volver",
    eyebrow: "Ficha de serie",
    loading: "Cargando ficha y listado de capítulos…",
    loadErrorTitle: "No se pudo cargar esta serie",
    offlineBannerTitle: "Mostrando copia local offline",
    offlineBannerDesc:
      "La fuente online no devolvió capítulos o no hay conexión, así que ves tus capítulos descargados en este dispositivo.",
    warningBannerTitle: "Aviso de conexión con el servidor",
    authorLabel: "Autor",
    artistLabel: "Arte",
    statusLabel: "Estado",
    statusNames: {
      ongoing: "En publicación",
      completed: "Finalizado",
      hiatus: "En pausa",
      cancelled: "Cancelado",
      unknown: "Desconocido",
    },
    noSynopsis: "Esta fuente no incluye sinopsis para esta serie.",
    nextUnreadLabel: "Siguiente capítulo pendiente",
    nextUnreadAction: "Leer / Marcar leído",
    readerComingSoonToast:
      "El lector HD integrado llega en el Hito 3. De momento puedes marcar el progreso desde aquí.",
    chaptersTitle: "Capítulos",
    chaptersCount: (total) => (total === 1 ? "1 capítulo" : `${total} capítulos`),
    readCountLabel: (count) => (count === 1 ? "1 leído" : `${count} leídos`),
    offlineCountLabel: (count) => (count === 1 ? "1 offline" : `${count} offline`),
    sortNewest: "Más recientes primero",
    sortOldest: "Desde el cap. 1",
    languageLabel: "Idioma",
    filterAll: "Todos",
    filterOffline: "Solo offline",
    emptyChapters: "No hay capítulos disponibles en este idioma o página.",
    emptyOfflineChapters: "No tienes capítulos descargados para esta serie todavía.",
    chapterPrefix: "Cap.",
    statusRead: "Leído",
    statusUnread: "Pendiente",
    statusOffline: "Offline",
    statusOnline: "Online",
    markReadBtn: "Marcar leído",
    markUnreadBtn: "Quitar leído",
    markedReadToast: "Capítulo marcado como leído",
    markedUnreadToast: "Capítulo marcado como pendiente",
    openReaderBtn: "Abrir lector",
    downloadBtn: "Descargar",
    downloadingBtn: "En cola",
    selectChaptersBtn: "Seleccionar varios",
    cancelSelectBtn: "Cancelar selección",
    selectAllPageBtn: "Marcar página",
    downloadSelectedBtn: (count) => `Descargar (${count})`,
    enqueuedToast: (count) =>
      count === 1 ? "1 capítulo añadido a la cola de descargas" : `${count} capítulos añadidos a la cola de descargas`,
    communityEyebrow: "Comunidad · AniList + Discord",
    communityTitle: "Opinión y reseñas",
    communityLoading: "Consultando ficha pública de AniList…",
    communityUnavailable: "AniList no respondió en este momento; tu ficha y capítulos siguen operativos.",
    communityNotFound: "No encontramos coincidencia pública automática en AniList para este título.",
    communityScoreLabel: "Puntuación media",
    communityPopularityLabel: "Lectores AniList",
    communityFavoritesLabel: "Favoritos AniList",
    communityTagsLabel: "Etiquetas destacadas",
    communityReviewsTitle: "Reseñas públicas de lectores",
    communityNoReviews: "Todavía no hay reseñas públicas registradas en AniList para esta serie.",
    communityOpenAniList: "Ver en AniList ↗",
    communityDiscordTitle: "Comenta esta serie en Discord",
    communityDiscordDesc:
      "¿Quieres recomendarla, comentar el último capítulo o avisar si un servidor falla? Pásate por el Discord oficial.",
    communityDiscordBtn: "Abrir Discord ↗",
  },
  reader: {
    backToSeries: "Volver",
    loading: "Abriendo páginas del capítulo…",
    loadErrorTitle: "No se pudo abrir el capítulo",
    retry: "Reintentar",
    offlineBadge: (fmt) => (fmt ? `OFFLINE / LOCAL · ${fmt}` : "OFFLINE / LOCAL"),
    onlineBadge: (provider) => `ONLINE · ${provider}`,
    chapterLabel: (num) => `Cap. ${num}`,
    pageOf: (current, total) => `Pág. ${current} / ${total}`,
    prevChapter: "Cap. anterior",
    nextChapter: "Siguiente cap.",
    prevPage: "Página anterior",
    nextPage: "Página siguiente",
    modeLabel: "Modo de vista",
    modeCascade: "Cascada",
    modeSingle: "Página única",
    modeDouble: "Doble página",
    directionLabel: "Sentido",
    dirRtl: "Manga (D ← I)",
    dirLtr: "Occidental (I → D)",
    fitLabel: "Ajuste",
    fitWidth: "Ancho",
    fitHeight: "Alto",
    fitOriginal: "Original",
    zoomOut: "Alejar",
    zoomIn: "Acercar",
    zoomReset: "100%",
    firstPageSingleLabel: "Portada sola en doble página",
    darkCanvasLabel: "Fondo oscuro de lectura",
    pinHudLabel: "Mantener barras visibles",
    fullscreenBtn: "Pantalla completa",
    settingsBtn: "Ajustes de lectura",
    endOfChapterTitle: "Fin del capítulo",
    endOfChapterSubtitle: "El progreso ya está guardado en tu biblioteca SQLite.",
    noNextChapter: "No hay un capítulo siguiente en esta lista.",
    shortcutsHint: "Atajos: ← / → o Espacio para avanzar · F pantalla completa · Esc salir",
    touchHint: "Toca los laterales o desliza para pasar página · Pellizca o doble toque para zoom",
    bookmarkSavedToast: "Marcador guardado",
  },
  downloads: {
    eyebrow: "Descargas · Lectura offline",
    title: "Llévatelo sin conexión.",
    subtitle:
      "Mangora comparte `mangaDownloads/` con manga-tui, descarga en segundo plano y lee CBZ, EPUB, PDF y RAW sin internet.",
    ribbonLabel: "Formatos compatibles",
    formats: ["CBZ", "EPUB", "PDF", "Imágenes RAW"],
    formatSelectorLabel: "Formato de descarga por defecto",
    formatDescriptions: {
      cbz: "Archivo cómico estándar, rápido y compatible con casi cualquier lector (Recomendado).",
      pdf: "Documento portátil listo para compartir o abrir en tablet.",
      epub: "Libro electrónico compatible con e-readers.",
      raw: "Carpeta con imágenes sueltas sin empaquetar.",
    },
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
    queueTitle: "Cola de descargas en vivo",
    queueEmpty: "No hay capítulos en la cola ahora mismo. Puedes descargarlos desde la ficha de cualquier serie.",
    pauseAllBtn: "Pausar todo",
    resumeAllBtn: "Reanudar todo",
    retryFailedBtn: "Reintentar fallidos",
    clearFinishedBtn: "Limpiar terminados",
    cancelAllBtn: "Cancelar todo",
    overallProgressLabel: "Progreso global",
    pagesProgress: (done, total) => `${done}/${total} págs.`,
    statusNames: {
      queued: "En cola",
      downloading: "Descargando",
      packaging: "Empaquetando",
      paused: "Pausado",
      completed: "Completado",
      failed: "Error",
      cancelled: "Cancelado",
    },
    pauseTaskBtn: "Pausar",
    resumeTaskBtn: "Reanudar",
    retryTaskBtn: "Reintentar",
    cancelTaskBtn: "Cancelar",
    removeTaskBtn: "Quitar",
    offlineShelfTitle: "Series disponibles 100 % offline",
    offlineShelfSubtitle: "Toca cualquier serie descargada para ver sus capítulos locales y abrir el lector sin internet.",
    offlineShelfEmpty: "Aún no hay capítulos descargados en tu carpeta local.",
    offlineChaptersRead: (read, total) => `${read} / ${total} leídos`,
    openOfflineSeriesBtn: "Abrir serie",
    finishedToast: (title, ch) => `Descarga lista: ${title} · Cap. ${ch}`,
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
    openSeriesBtn: "View series & chapters",
  },
  explore: {
    eyebrow: "Explore · Catalog & sources",
    title: "Find your next series.",
    subtitle:
      "Search for a series by title in your chosen source’s catalog.",
    forYouTitle: "For you based on your tastes",
    forYouLoading: "Loading picks from your active source…",
    forYouMatched: (genresLabel) => `Matched with your genres: ${genresLabel}`,
    forYouPopularFallback: "Showing popular highlights from your active source.",
    forYouUnavailable: "Could not load recommendations right now.",
    forYouEmpty: "Use the search bar above to browse titles from this source.",
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
  series: {
    backBtn: "Back",
    eyebrow: "Series page",
    loading: "Loading series metadata and chapters…",
    loadErrorTitle: "Could not load this series",
    offlineBannerTitle: "Showing local offline copy",
    offlineBannerDesc:
      "The online source is unavailable or returned no chapters, so your downloaded local chapters are shown instead.",
    warningBannerTitle: "Source connection notice",
    authorLabel: "Author",
    artistLabel: "Artist",
    statusLabel: "Status",
    statusNames: {
      ongoing: "Ongoing",
      completed: "Completed",
      hiatus: "Hiatus",
      cancelled: "Cancelled",
      unknown: "Unknown",
    },
    noSynopsis: "This source didn’t include a synopsis for this series.",
    nextUnreadLabel: "Next unread chapter",
    nextUnreadAction: "Read / Mark read",
    readerComingSoonToast:
      "The built-in HD reader arrives in Milestone 3. You can track chapter progress from here right now.",
    chaptersTitle: "Chapters",
    chaptersCount: (total) => (total === 1 ? "1 chapter" : `${total} chapters`),
    readCountLabel: (count) => (count === 1 ? "1 read" : `${count} read`),
    offlineCountLabel: (count) => (count === 1 ? "1 offline" : `${count} offline`),
    sortNewest: "Newest first",
    sortOldest: "From Ch. 1",
    languageLabel: "Language",
    filterAll: "All",
    filterOffline: "Offline only",
    emptyChapters: "No chapters available for this language or page.",
    emptyOfflineChapters: "No downloaded chapters found for this series yet.",
    chapterPrefix: "Ch.",
    statusRead: "Read",
    statusUnread: "Unread",
    statusOffline: "Offline",
    statusOnline: "Online",
    markReadBtn: "Mark read",
    markUnreadBtn: "Mark unread",
    markedReadToast: "Chapter marked as read",
    markedUnreadToast: "Chapter marked as unread",
    openReaderBtn: "Open reader",
    downloadBtn: "Download",
    downloadingBtn: "Queued",
    selectChaptersBtn: "Select multiple",
    cancelSelectBtn: "Cancel selection",
    selectAllPageBtn: "Select page",
    downloadSelectedBtn: (count) => `Download (${count})`,
    enqueuedToast: (count) =>
      count === 1 ? "1 chapter added to the download queue" : `${count} chapters added to the download queue`,
    communityEyebrow: "Community · AniList + Discord",
    communityTitle: "Community & reviews",
    communityLoading: "Fetching public AniList data…",
    communityUnavailable: "AniList didn’t respond right now; series details and chapters remain available.",
    communityNotFound: "No automatic public AniList match was found for this title.",
    communityScoreLabel: "Average score",
    communityPopularityLabel: "AniList readers",
    communityFavoritesLabel: "AniList favorites",
    communityTagsLabel: "Top tags",
    communityReviewsTitle: "Public reader reviews",
    communityNoReviews: "No public AniList reviews yet for this series.",
    communityOpenAniList: "Open on AniList ↗",
    communityDiscordTitle: "Chat about this series on Discord",
    communityDiscordDesc:
      "Share thoughts, ask for features, or report a broken source in the official Discord server.",
    communityDiscordBtn: "Open Discord ↗",
  },
  reader: {
    backToSeries: "Back",
    loading: "Loading chapter pages…",
    loadErrorTitle: "Could not open chapter",
    retry: "Retry",
    offlineBadge: (fmt) => (fmt ? `OFFLINE / LOCAL · ${fmt}` : "OFFLINE / LOCAL"),
    onlineBadge: (provider) => `ONLINE · ${provider}`,
    chapterLabel: (num) => `Ch. ${num}`,
    pageOf: (current, total) => `Page ${current} / ${total}`,
    prevChapter: "Prev chapter",
    nextChapter: "Next chapter",
    prevPage: "Previous page",
    nextPage: "Next page",
    modeLabel: "View mode",
    modeCascade: "Cascade",
    modeSingle: "Single page",
    modeDouble: "Double page",
    directionLabel: "Direction",
    dirRtl: "Manga (R ← L)",
    dirLtr: "Western (L → R)",
    fitLabel: "Fit",
    fitWidth: "Fit width",
    fitHeight: "Fit height",
    fitOriginal: "Original",
    zoomOut: "Zoom out",
    zoomIn: "Zoom in",
    zoomReset: "100%",
    firstPageSingleLabel: "Single cover in double spread",
    darkCanvasLabel: "Dark reading canvas",
    pinHudLabel: "Keep toolbars pinned",
    fullscreenBtn: "Fullscreen",
    settingsBtn: "Reader settings",
    endOfChapterTitle: "End of chapter",
    endOfChapterSubtitle: "Your progress is saved to the shared SQLite library.",
    noNextChapter: "No next chapter in this list.",
    shortcutsHint: "Shortcuts: ← / → or Space to turn pages · F fullscreen · Esc close",
    touchHint: "Tap sides or swipe to turn pages · Pinch or double-tap to zoom",
    bookmarkSavedToast: "Bookmark saved",
  },
  downloads: {
    eyebrow: "Downloads · Offline reading",
    title: "Read anywhere offline.",
    subtitle:
      "Mangora shares `mangaDownloads/` with manga-tui, queues downloads in the background, and reads CBZ, EPUB, PDF, and RAW offline.",
    ribbonLabel: "Supported formats",
    formats: ["CBZ", "EPUB", "PDF", "RAW Images"],
    formatSelectorLabel: "Default download format",
    formatDescriptions: {
      cbz: "Standard comic archive, fast and compatible with almost every reader (Recommended).",
      pdf: "Portable document ready for sharing or tablet reading.",
      epub: "E-book container for e-readers.",
      raw: "Unpacked folder with individual image files.",
    },
    folderCardTitle: "Shared downloads folder",
    folderCardDesc:
      "Chapters downloaded from manga-tui or Mangora live together in the same local folder.",
    openFolderBtn: "Open downloads folder in Explorer",
    mobileStorageNote:
      "On mobile, downloads are stored in Mangora's private app storage and can be synced with Google Drive in Settings.",
    detectedOfflineTitle: "Local scanner connected",
    detectedOfflineCount: (count) =>
      count === 1
        ? "1 offline series currently detected on disk."
        : `${count} offline series currently detected on disk.`,
    queueTitle: "Live download queue",
    queueEmpty: "No chapters in the queue right now. You can queue downloads from any series page.",
    pauseAllBtn: "Pause all",
    resumeAllBtn: "Resume all",
    retryFailedBtn: "Retry failed",
    clearFinishedBtn: "Clear finished",
    cancelAllBtn: "Cancel all",
    overallProgressLabel: "Overall progress",
    pagesProgress: (done, total) => `${done}/${total} pages`,
    statusNames: {
      queued: "Queued",
      downloading: "Downloading",
      packaging: "Packaging",
      paused: "Paused",
      completed: "Completed",
      failed: "Failed",
      cancelled: "Cancelled",
    },
    pauseTaskBtn: "Pause",
    resumeTaskBtn: "Resume",
    retryTaskBtn: "Retry",
    cancelTaskBtn: "Cancel",
    removeTaskBtn: "Remove",
    offlineShelfTitle: "Available 100% offline",
    offlineShelfSubtitle: "Tap any downloaded series to browse local chapters and open the offline reader.",
    offlineShelfEmpty: "No downloaded chapters found in your local folder yet.",
    offlineChaptersRead: (read, total) => `${read} / ${total} read`,
    openOfflineSeriesBtn: "Open series",
    finishedToast: (title, ch) => `Download ready: ${title} · Ch. ${ch}`,
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
