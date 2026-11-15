use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget, Wrap};
use strum::IntoEnumIterator;

use crate::backend::manga_provider::{Languages, MangaProviders};
use crate::backend::multi_provider::{ProviderHealthStatus, get_provider_health_status};
use crate::config::{DownloadType, MangaTuiConfig, UiLanguage};
use crate::i18n::tr;
use crate::theme::{ThemeVariant, current_theme, set_theme, theme};
use crate::utils::centered_rect;

static ACTIVE_PROVIDER: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_DOWNLOAD_TYPE: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_CHAPTER_LANG: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_UI_LANG: AtomicU8 = AtomicU8::new(u8::MAX);

pub const ALL_PROVIDERS: [MangaProviders; 7] = [
    MangaProviders::Mangadex,
    MangaProviders::Inmanga,
    MangaProviders::Mangaoni,
    MangaProviders::Leercapitulo,
    MangaProviders::Weebcentral,
    MangaProviders::Mangapill,
    MangaProviders::Comizy,
];

pub const ALL_DOWNLOAD_FORMATS: [DownloadType; 4] = [DownloadType::Cbz, DownloadType::Pdf, DownloadType::Epub, DownloadType::Raw];

pub const ALL_UI_LANGUAGES: [UiLanguage; 2] = [UiLanguage::Es, UiLanguage::En];

pub const ALL_THEMES: [ThemeVariant; 3] = [ThemeVariant::TokyoNight, ThemeVariant::Catppuccin, ThemeVariant::Classic];

/// Returns the ordered list of chapter languages with the most common ones first for quick cycling.
pub fn ordered_chapter_languages() -> Vec<Languages> {
    let mut list = vec![
        Languages::Spanish,
        Languages::SpanishLa,
        Languages::English,
        Languages::BrazilianPortuguese,
        Languages::Portuguese,
        Languages::French,
        Languages::Italian,
        Languages::German,
        Languages::Japanese,
    ];
    for lang in Languages::iterate() {
        if !list.contains(&lang) {
            list.push(lang);
        }
    }
    list
}

fn provider_to_u8(p: MangaProviders) -> u8 {
    ALL_PROVIDERS.iter().position(|&item| item == p).unwrap_or(0) as u8
}

fn provider_from_u8(val: u8) -> MangaProviders {
    ALL_PROVIDERS.get(val as usize).copied().unwrap_or_default()
}

fn download_type_to_u8(dt: DownloadType) -> u8 {
    ALL_DOWNLOAD_FORMATS.iter().position(|&item| item == dt).unwrap_or(0) as u8
}

fn download_type_from_u8(val: u8) -> DownloadType {
    ALL_DOWNLOAD_FORMATS.get(val as usize).copied().unwrap_or_default()
}

fn ui_lang_to_u8(lang: UiLanguage) -> u8 {
    match lang {
        UiLanguage::Es => 0,
        UiLanguage::En => 1,
    }
}

fn ui_lang_from_u8(val: u8) -> UiLanguage {
    match val {
        1 => UiLanguage::En,
        _ => UiLanguage::Es,
    }
}

fn language_to_u8(lang: Languages) -> u8 {
    Languages::iter().position(|item| item == lang).unwrap_or(1) as u8
}

fn language_from_u8(val: u8) -> Languages {
    Languages::iter().nth(val as usize).unwrap_or_default()
}

/// Returns the static reference for a `Languages` variant so callers expecting `&'static Languages` work lock-free.
pub const fn language_as_static_ref(lang: Languages) -> &'static Languages {
    match lang {
        Languages::French => &Languages::French,
        Languages::English => &Languages::English,
        Languages::Spanish => &Languages::Spanish,
        Languages::SpanishLa => &Languages::SpanishLa,
        Languages::Italian => &Languages::Italian,
        Languages::Japanese => &Languages::Japanese,
        Languages::Korean => &Languages::Korean,
        Languages::BrazilianPortuguese => &Languages::BrazilianPortuguese,
        Languages::Portuguese => &Languages::Portuguese,
        Languages::TraditionalChinese => &Languages::TraditionalChinese,
        Languages::SimplifiedChinese => &Languages::SimplifiedChinese,
        Languages::Russian => &Languages::Russian,
        Languages::German => &Languages::German,
        Languages::Burmese => &Languages::Burmese,
        Languages::Arabic => &Languages::Arabic,
        Languages::Bulgarian => &Languages::Bulgarian,
        Languages::Vietnamese => &Languages::Vietnamese,
        Languages::Croatian => &Languages::Croatian,
        Languages::Hungarian => &Languages::Hungarian,
        Languages::Dutch => &Languages::Dutch,
        Languages::Mongolian => &Languages::Mongolian,
        Languages::Turkish => &Languages::Turkish,
        Languages::Ukrainian => &Languages::Ukrainian,
        Languages::Thai => &Languages::Thai,
        Languages::Catalan => &Languages::Catalan,
        Languages::Indonesian => &Languages::Indonesian,
        Languages::Filipino => &Languages::Filipino,
        Languages::Hindi => &Languages::Hindi,
        Languages::Romanian => &Languages::Romanian,
        Languages::Hebrew => &Languages::Hebrew,
        Languages::Polish => &Languages::Polish,
        Languages::Persian => &Languages::Persian,
        Languages::Unkown => &Languages::Unkown,
    }
}

#[inline]
pub fn active_default_provider() -> MangaProviders {
    let raw = ACTIVE_PROVIDER.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = MangaTuiConfig::try_get().map(|c| c.default_manga_provider).unwrap_or_default();
        ACTIVE_PROVIDER.store(provider_to_u8(initial), Ordering::Relaxed);
        initial
    } else {
        provider_from_u8(raw)
    }
}

#[inline]
pub fn set_active_default_provider(provider: MangaProviders) {
    ACTIVE_PROVIDER.store(provider_to_u8(provider), Ordering::Relaxed);
}

#[inline]
pub fn active_download_type() -> DownloadType {
    let raw = ACTIVE_DOWNLOAD_TYPE.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = MangaTuiConfig::try_get().map(|c| c.download_type).unwrap_or_default();
        ACTIVE_DOWNLOAD_TYPE.store(download_type_to_u8(initial), Ordering::Relaxed);
        initial
    } else {
        download_type_from_u8(raw)
    }
}

#[inline]
pub fn set_active_download_type(download_type: DownloadType) {
    ACTIVE_DOWNLOAD_TYPE.store(download_type_to_u8(download_type), Ordering::Relaxed);
}

#[inline]
pub fn active_chapter_language() -> Languages {
    let raw = ACTIVE_CHAPTER_LANG.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = crate::global::PREFERRED_LANGUAGE
            .get()
            .copied()
            .or_else(|| MangaTuiConfig::try_get().map(|c| c.default_language))
            .unwrap_or_default();
        ACTIVE_CHAPTER_LANG.store(language_to_u8(initial), Ordering::Relaxed);
        initial
    } else {
        language_from_u8(raw)
    }
}

#[inline]
pub fn active_chapter_language_ref() -> &'static Languages {
    let raw = ACTIVE_CHAPTER_LANG.load(Ordering::Relaxed);
    if raw != u8::MAX {
        language_as_static_ref(language_from_u8(raw))
    } else {
        crate::global::PREFERRED_LANGUAGE.get_or_init(|| MangaTuiConfig::try_get().map(|c| c.default_language).unwrap_or_default())
    }
}

#[inline]
pub fn set_active_chapter_language(lang: Languages) {
    ACTIVE_CHAPTER_LANG.store(language_to_u8(lang), Ordering::Relaxed);
}

#[inline]
pub fn active_ui_language() -> UiLanguage {
    let raw = ACTIVE_UI_LANG.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = MangaTuiConfig::try_get().map(|c| c.ui_language).unwrap_or_default();
        ACTIVE_UI_LANG.store(ui_lang_to_u8(initial), Ordering::Relaxed);
        initial
    } else {
        ui_lang_from_u8(raw)
    }
}

#[inline]
pub fn set_active_ui_language(lang: UiLanguage) {
    ACTIVE_UI_LANG.store(ui_lang_to_u8(lang), Ordering::Relaxed);
}

#[inline]
pub fn set_active_theme(variant: ThemeVariant) {
    set_theme(variant);
    let _ = persist_setting_to_disk("theme", theme_toml_value(variant));
}

/// Initializes the live atomic settings from a `MangaTuiConfig`.
pub fn init_runtime_settings(config: &MangaTuiConfig) {
    set_active_default_provider(config.default_manga_provider);
    set_active_download_type(config.download_type);
    set_active_chapter_language(config.default_language);
    set_active_ui_language(config.ui_language);
    set_theme(config.theme);
}

/// Updates a root-level TOML key (`key = "value"`) in `contents` while preserving comments and tables.
pub fn update_toml_key_in_content(contents: &str, key: &str, quoted_value: &str) -> String {
    let new_assignment = format!("{key} = {quoted_value}");
    let mut lines: Vec<String> = Vec::new();
    let mut in_table = false;
    let mut updated = false;
    let mut first_table_line_idx: Option<usize> = None;

    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if first_table_line_idx.is_none() {
                first_table_line_idx = Some(idx);
            }
            in_table = true;
        }

        if !in_table && !updated && !trimmed.starts_with('#') {
            if let Some((lhs, _rhs)) = trimmed.split_once('=') {
                if lhs.trim() == key {
                    lines.push(new_assignment.clone());
                    updated = true;
                    continue;
                }
            }
        }

        lines.push(line.to_string());
    }

    if !updated {
        match first_table_line_idx {
            Some(table_idx) => {
                lines.insert(table_idx, String::new());
                lines.insert(table_idx, new_assignment);
            },
            None => {
                if !lines.is_empty() && !lines.last().unwrap().is_empty() {
                    lines.push(String::new());
                }
                lines.push(new_assignment);
            },
        }
    }

    let mut out = lines.join("\n");
    if contents.ends_with('\n') || !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Atomically updates a single root-level key in the user's `config.toml` file if the config directory exists.
pub fn persist_setting_to_disk(key: &str, unquoted_value: &str) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let dir = crate::config::get_config_directory_path();
    persist_setting_in_dir(&dir, key, unquoted_value)
}

pub fn persist_setting_in_dir(dir: &Path, key: &str, unquoted_value: &str) -> Result<(), String> {
    let config_path = dir.join("config.toml");
    if !config_path.exists() {
        return Ok(());
    }
    let current = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let quoted = format!("\"{}\"", unquoted_value.replace('"', "\\\""));
    let updated = update_toml_key_in_content(&current, key, &quoted);

    let tmp_path = dir.join("config.toml.tmp");
    {
        let mut tmp = File::create(&tmp_path).map_err(|e| e.to_string())?;
        tmp.write_all(updated.as_bytes()).map_err(|e| e.to_string())?;
        tmp.flush().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp_path, &config_path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn theme_toml_value(variant: ThemeVariant) -> &'static str {
    match variant {
        ThemeVariant::TokyoNight => "tokyo_night",
        ThemeVariant::Catppuccin => "catppuccin",
        ThemeVariant::Classic => "classic",
    }
}

pub fn ui_lang_toml_value(lang: UiLanguage) -> &'static str {
    match lang {
        UiLanguage::Es => "es",
        UiLanguage::En => "en",
    }
}

pub fn download_type_toml_value(dt: DownloadType) -> &'static str {
    match dt {
        DownloadType::Cbz => "cbz",
        DownloadType::Pdf => "pdf",
        DownloadType::Epub => "epub",
        DownloadType::Raw => "raw",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    DefaultProvider,
    DownloadFormat,
    ChapterLanguage,
    UiLanguage,
    Theme,
}

impl SettingsField {
    pub const ALL: [SettingsField; 5] = [
        SettingsField::DefaultProvider,
        SettingsField::DownloadFormat,
        SettingsField::ChapterLanguage,
        SettingsField::UiLanguage,
        SettingsField::Theme,
    ];

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(SettingsField::DefaultProvider)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::DefaultProvider => tr("📡 Proveedor por defecto", "📡 Default Provider"),
            Self::DownloadFormat => tr("📦 Formato de descarga", "📦 Download Format"),
            Self::ChapterLanguage => tr("🌐 Idioma de capítulos", "🌐 Chapter Language"),
            Self::UiLanguage => tr("🗣 Idioma de la interfaz", "🗣 Interface Language"),
            Self::Theme => tr("🎨 Tema visual", "🎨 Visual Theme"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::DefaultProvider => tr(
                "Cambia la fuente activa al instante y la guarda como predeterminada al iniciar",
                "Switches active source immediately and saves it as startup default",
            ),
            Self::DownloadFormat => tr(
                "Formato de salida para nuevas descargas (CBZ, PDF, EPUB o imágenes RAW)",
                "Output archive format for new chapter downloads (CBZ, PDF, EPUB, or RAW)",
            ),
            Self::ChapterLanguage => tr(
                "Idioma preferido al listar y abrir capítulos (p. ej. Español, Latam, Inglés)",
                "Preferred language when listing and reading chapters (e.g. Spanish, English)",
            ),
            Self::UiLanguage => tr(
                "Idioma de menús, barras de estado, atajos y notificaciones",
                "Language used for menus, status bars, shortcuts, and notifications",
            ),
            Self::Theme => tr(
                "Paleta de colores en vivo para toda la interfaz sin reiniciar la aplicación",
                "Live color palette applied across the entire interface without restarting",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsOutcome {
    Noop,
    Close,
    ProviderChanged(MangaProviders),
    DownloadFormatChanged(DownloadType),
    ChapterLanguageChanged(Languages),
    UiLanguageChanged(UiLanguage),
    ThemeChanged(ThemeVariant),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsModalState {
    pub cursor: usize,
    pub status_message: Option<String>,
}

impl Default for SettingsModalState {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsModalState {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            status_message: None,
        }
    }

    pub fn current_field(&self) -> SettingsField {
        SettingsField::from_index(self.cursor)
    }

    pub fn select_next(&mut self) {
        self.cursor = (self.cursor + 1) % SettingsField::ALL.len();
    }

    pub fn select_previous(&mut self) {
        if self.cursor == 0 {
            self.cursor = SettingsField::ALL.len() - 1;
        } else {
            self.cursor -= 1;
        }
    }

    pub fn cycle_current(&mut self, forward: bool) -> SettingsOutcome {
        match self.current_field() {
            SettingsField::DefaultProvider => {
                let current = active_default_provider();
                let idx = ALL_PROVIDERS.iter().position(|&p| p == current).unwrap_or(0);
                let next_idx = if forward {
                    (idx + 1) % ALL_PROVIDERS.len()
                } else if idx == 0 {
                    ALL_PROVIDERS.len() - 1
                } else {
                    idx - 1
                };
                let next_provider = ALL_PROVIDERS[next_idx];
                set_active_default_provider(next_provider);
                let _ = persist_setting_to_disk("default_manga_provider", &next_provider.to_string());
                self.status_message = Some(format!(
                    "✓ {}: {} ({})",
                    tr("Proveedor activo y por defecto", "Active & default provider"),
                    next_provider.badge_label(),
                    tr("guardado en config.toml", "saved to config.toml")
                ));
                SettingsOutcome::ProviderChanged(next_provider)
            },
            SettingsField::DownloadFormat => {
                let current = active_download_type();
                let idx = ALL_DOWNLOAD_FORMATS.iter().position(|&f| f == current).unwrap_or(0);
                let next_idx = if forward {
                    (idx + 1) % ALL_DOWNLOAD_FORMATS.len()
                } else if idx == 0 {
                    ALL_DOWNLOAD_FORMATS.len() - 1
                } else {
                    idx - 1
                };
                let next_format = ALL_DOWNLOAD_FORMATS[next_idx];
                set_active_download_type(next_format);
                let _ = persist_setting_to_disk("download_type", download_type_toml_value(next_format));
                self.status_message = Some(format!(
                    "✓ {}: {} ({})",
                    tr("Formato de descarga", "Download format"),
                    next_format.to_string().to_uppercase(),
                    tr("guardado en config.toml", "saved to config.toml")
                ));
                SettingsOutcome::DownloadFormatChanged(next_format)
            },
            SettingsField::ChapterLanguage => {
                let langs = ordered_chapter_languages();
                let current = active_chapter_language();
                let idx = langs.iter().position(|&l| l == current).unwrap_or(0);
                let next_idx = if forward {
                    (idx + 1) % langs.len()
                } else if idx == 0 {
                    langs.len() - 1
                } else {
                    idx - 1
                };
                let next_lang = langs[next_idx];
                set_active_chapter_language(next_lang);
                let _ = persist_setting_to_disk("default_language", &next_lang.to_string().to_lowercase());
                self.status_message = Some(format!(
                    "✓ {}: {} {} ({})",
                    tr("Idioma de capítulos", "Chapter language"),
                    next_lang.as_emoji(),
                    next_lang.as_human_readable(),
                    tr("guardado en config.toml", "saved to config.toml")
                ));
                SettingsOutcome::ChapterLanguageChanged(next_lang)
            },
            SettingsField::UiLanguage => {
                let current = active_ui_language();
                let next_lang = match current {
                    UiLanguage::Es => UiLanguage::En,
                    UiLanguage::En => UiLanguage::Es,
                };
                set_active_ui_language(next_lang);
                let _ = persist_setting_to_disk("ui_language", ui_lang_toml_value(next_lang));
                let label = match next_lang {
                    UiLanguage::Es => "🇪🇸 Español",
                    UiLanguage::En => "🇬🇧 English",
                };
                self.status_message = Some(format!(
                    "✓ {}: {} ({})",
                    tr("Idioma de la interfaz", "Interface language"),
                    label,
                    tr("guardado en config.toml", "saved to config.toml")
                ));
                SettingsOutcome::UiLanguageChanged(next_lang)
            },
            SettingsField::Theme => {
                let current = current_theme();
                let idx = ALL_THEMES.iter().position(|&t| t == current).unwrap_or(0);
                let next_idx = if forward {
                    (idx + 1) % ALL_THEMES.len()
                } else if idx == 0 {
                    ALL_THEMES.len() - 1
                } else {
                    idx - 1
                };
                let next_theme = ALL_THEMES[next_idx];
                set_theme(next_theme);
                let _ = persist_setting_to_disk("theme", theme_toml_value(next_theme));
                self.status_message = Some(format!(
                    "✓ {}: {} ({})",
                    tr("Tema visual", "Visual theme"),
                    next_theme.label(),
                    tr("guardado en config.toml", "saved to config.toml")
                ));
                SettingsOutcome::ThemeChanged(next_theme)
            },
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> SettingsOutcome {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                self.select_next();
                SettingsOutcome::Noop
            },
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                self.select_previous();
                SettingsOutcome::Noop
            },
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => self.cycle_current(true),
            KeyCode::Left | KeyCode::Char('h') => self.cycle_current(false),
            KeyCode::Char('1') => {
                self.cursor = 0;
                SettingsOutcome::Noop
            },
            KeyCode::Char('2') => {
                self.cursor = 1;
                SettingsOutcome::Noop
            },
            KeyCode::Char('3') => {
                self.cursor = 2;
                SettingsOutcome::Noop
            },
            KeyCode::Char('4') => {
                self.cursor = 3;
                SettingsOutcome::Noop
            },
            KeyCode::Char('5') => {
                self.cursor = 4;
                SettingsOutcome::Noop
            },
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::F(5) | KeyCode::Char('q') | KeyCode::Esc => SettingsOutcome::Close,
            _ => SettingsOutcome::Noop,
        }
    }
}

#[derive(Debug)]
pub struct SettingsModal<'a> {
    state: &'a SettingsModalState,
}

impl<'a> SettingsModal<'a> {
    #[inline]
    pub fn new(state: &'a SettingsModalState) -> Self {
        Self { state }
    }

    fn render_provider_selector(is_focused: bool) -> Vec<Span<'static>> {
        let t = theme();
        let current = active_default_provider();
        let idx = ALL_PROVIDERS.iter().position(|&p| p == current).unwrap_or(0) + 1;
        let total = ALL_PROVIDERS.len();
        let health = get_provider_health_status(current);
        let (health_dot, health_color) = match health {
            ProviderHealthStatus::Healthy => ("● Online", t.status_success),
            ProviderHealthStatus::Degraded => (tr("◐ Lento", "◐ Slow"), t.status_warning),
            ProviderHealthStatus::CircuitOpen => (tr("⚠ Caído", "⚠ Down"), t.status_error),
        };

        let arrow_style = if is_focused {
            Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_dim)
        };

        let pill_style = if is_focused {
            Style::default().fg(Color::Black).bg(t.accent_primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_primary).bg(t.chip_bg).add_modifier(Modifier::BOLD)
        };

        vec![
            Span::styled("◀ ", arrow_style),
            Span::styled(format!(" {} ", current.badge_label()), pill_style),
            Span::raw(" "),
            Span::styled(health_dot, Style::default().fg(health_color).add_modifier(Modifier::BOLD)),
            Span::styled(" ▶", arrow_style),
            Span::styled(format!("  ({idx}/{total})"), Style::default().fg(t.text_muted)),
        ]
    }

    fn render_download_format_selector(is_focused: bool) -> Vec<Span<'static>> {
        let t = theme();
        let current = active_download_type();
        let arrow_style = if is_focused {
            Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_dim)
        };

        let mut spans = vec![Span::styled("◀ ", arrow_style)];
        for (idx, fmt) in ALL_DOWNLOAD_FORMATS.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(" "));
            }
            let label = fmt.to_string().to_uppercase();
            if *fmt == current {
                spans.push(Span::styled(
                    format!(" {label} "),
                    Style::default().fg(Color::Black).bg(t.accent_primary).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(format!(" {label} "), Style::default().fg(t.text_muted).bg(t.chip_bg)));
            }
        }
        spans.push(Span::styled(" ▶", arrow_style));
        spans
    }

    fn render_chapter_language_selector(is_focused: bool) -> Vec<Span<'static>> {
        let t = theme();
        let langs = ordered_chapter_languages();
        let current = active_chapter_language();
        let idx = langs.iter().position(|&l| l == current).unwrap_or(0) + 1;
        let total = langs.len();

        let arrow_style = if is_focused {
            Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_dim)
        };

        let pill_style = if is_focused {
            Style::default().fg(Color::Black).bg(t.accent_primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_primary).bg(t.chip_bg).add_modifier(Modifier::BOLD)
        };

        let emoji = current.as_emoji();
        let prefix = if emoji.is_empty() { String::new() } else { format!("{emoji} ") };

        vec![
            Span::styled("◀ ", arrow_style),
            Span::styled(format!(" {prefix}{} ({}) ", current.as_human_readable(), current.as_iso_code()), pill_style),
            Span::styled(" ▶", arrow_style),
            Span::styled(format!("  ({idx}/{total})"), Style::default().fg(t.text_muted)),
        ]
    }

    fn render_ui_language_selector(is_focused: bool) -> Vec<Span<'static>> {
        let t = theme();
        let current = active_ui_language();
        let arrow_style = if is_focused {
            Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_dim)
        };

        let mut spans = vec![Span::styled("◀ ", arrow_style)];
        for (idx, (lang, text)) in [(UiLanguage::Es, "🇪🇸 Español"), (UiLanguage::En, "🇬🇧 English")].iter().enumerate()
        {
            if idx > 0 {
                spans.push(Span::raw(" "));
            }
            if *lang == current {
                spans.push(Span::styled(
                    format!(" {text} "),
                    Style::default().fg(Color::Black).bg(t.accent_primary).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(format!(" {text} "), Style::default().fg(t.text_muted).bg(t.chip_bg)));
            }
        }
        spans.push(Span::styled(" ▶", arrow_style));
        spans
    }

    fn render_theme_selector(is_focused: bool) -> Vec<Span<'static>> {
        let t = theme();
        let current = current_theme();
        let arrow_style = if is_focused {
            Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.text_dim)
        };

        let mut spans = vec![Span::styled("◀ ", arrow_style)];
        for (idx, variant) in ALL_THEMES.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(" "));
            }
            let p = variant.palette();
            let label = variant.label();
            if *variant == current {
                spans.push(Span::styled(
                    format!(" {label} "),
                    Style::default().fg(Color::Black).bg(t.accent_secondary).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled("█", Style::default().fg(p.accent_primary)));
                spans.push(Span::styled("█", Style::default().fg(p.accent_secondary)));
                spans.push(Span::styled("█", Style::default().fg(p.accent_highlight)));
            } else {
                spans.push(Span::styled(format!(" {label} "), Style::default().fg(t.text_muted).bg(t.chip_bg)));
            }
        }
        spans.push(Span::styled(" ▶", arrow_style));
        spans
    }
}

impl<'a> Widget for SettingsModal<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let modal_area = centered_rect(area, 76, 74);
        Clear.render(modal_area, buf);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent))
            .title(Line::from(vec![Span::styled(
                tr(" ⚙ Menú de Ajustes Interactivos <c / F5> ", " ⚙ Interactive Settings Menu <c / F5> "),
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
            )]))
            .title_bottom(
                Line::from(vec![
                    Span::raw(" "),
                    t.key_chip("j/k · ↑/↓"),
                    Span::styled(tr(" Elegir ajuste  ", " Select  "), Style::default().fg(t.text_muted)),
                    t.highlight_key_chip("h/l · ←/→ · Enter"),
                    Span::styled(tr(" Cambiar valor  ", " Change value  "), Style::default().fg(t.text_muted)),
                    t.key_chip("c / F5 / Esc"),
                    Span::styled(tr(" Cerrar ", " Close "), Style::default().fg(t.text_muted)),
                ])
                .alignment(Alignment::Center),
            );

        let inner = block.inner(modal_area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        block.render(modal_area, buf);

        if inner.height < 6 || inner.width < 24 {
            return;
        }

        let [header_area, rows_area, footer_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(4), Constraint::Length(2)])
            .areas(inner);

        Paragraph::new(Line::from(vec![
            Span::styled("✦ ", Style::default().fg(t.accent_secondary).add_modifier(Modifier::BOLD)),
            Span::styled(
                tr(
                    "Los cambios se aplican al instante en toda la app y se guardan en config.toml",
                    "Changes apply immediately across the app and are saved to config.toml",
                ),
                Style::default().fg(t.text_secondary).add_modifier(Modifier::ITALIC),
            ),
        ]))
        .render(header_area, buf);

        let mut constraints = Vec::with_capacity(SettingsField::ALL.len());
        for _ in SettingsField::ALL {
            constraints.push(Constraint::Length(3));
        }
        let row_rects = Layout::default().direction(Direction::Vertical).constraints(constraints).split(rows_area);

        for (idx, field) in SettingsField::ALL.iter().enumerate() {
            let Some(&row_area) = row_rects.get(idx) else {
                break;
            };
            if row_area.height == 0 || row_area.width == 0 {
                continue;
            }

            let is_focused = idx == self.state.cursor;
            if is_focused {
                for y in row_area.top()..row_area.bottom() {
                    for x in row_area.left()..row_area.right() {
                        buf[(x, y)].set_bg(t.row_selected_bg);
                    }
                }
            }

            let cursor_indicator = if is_focused {
                Span::styled(" ▸ ", Style::default().fg(t.accent_highlight).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(format!(" {} ", idx + 1), Style::default().fg(t.text_dim))
            };

            let label_style = if is_focused {
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.text_primary).add_modifier(Modifier::BOLD)
            };

            let mut top_spans =
                vec![cursor_indicator, Span::styled(format!("{:<28}", field.title()), label_style), Span::raw("  ")];

            let selector_spans = match field {
                SettingsField::DefaultProvider => Self::render_provider_selector(is_focused),
                SettingsField::DownloadFormat => Self::render_download_format_selector(is_focused),
                SettingsField::ChapterLanguage => Self::render_chapter_language_selector(is_focused),
                SettingsField::UiLanguage => Self::render_ui_language_selector(is_focused),
                SettingsField::Theme => Self::render_theme_selector(is_focused),
            };
            top_spans.extend(selector_spans);

            let top_line_area = Rect {
                x: row_area.x,
                y: row_area.y,
                width: row_area.width,
                height: 1,
            };
            Paragraph::new(Line::from(top_spans)).render(top_line_area, buf);

            if row_area.height >= 2 {
                let desc_area = Rect {
                    x: row_area.x.saturating_add(3),
                    y: row_area.y.saturating_add(1),
                    width: row_area.width.saturating_sub(3),
                    height: 1,
                };
                let desc_style = if is_focused { Style::default().fg(t.text_secondary) } else { Style::default().fg(t.text_dim) };
                Paragraph::new(Line::from(Span::styled(field.description(), desc_style)))
                    .wrap(Wrap { trim: true })
                    .render(desc_area, buf);
            }
        }

        let footer_line = if let Some(msg) = &self.state.status_message {
            Line::from(vec![Span::styled(msg.as_str(), Style::default().fg(t.status_success).add_modifier(Modifier::BOLD))])
        } else {
            Line::from(vec![Span::styled(
                format!(
                    "📡 {}  │  📦 {}  │  🌐 {}  │  🎨 {}",
                    active_default_provider(),
                    active_download_type().to_string().to_uppercase(),
                    active_chapter_language(),
                    current_theme().label()
                ),
                Style::default().fg(t.text_muted),
            )])
        };

        Paragraph::new(footer_line).alignment(Alignment::Center).render(footer_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_update_toml_key_in_content_preserves_comments_and_tables() {
        let sample = r#"# Language of the user interface
ui_language = "es"

# The format of the manga downloaded
download_type = "cbz"

[anilist]
client_id = "123"
"#;

        let updated = update_toml_key_in_content(sample, "download_type", "\"pdf\"");
        assert!(updated.contains("# The format of the manga downloaded"));
        assert!(updated.contains("download_type = \"pdf\""));
        assert!(updated.contains("[anilist]\nclient_id = \"123\""));

        // Inserting a missing key places it before [anilist]
        let with_theme = update_toml_key_in_content(&updated, "theme", "\"catppuccin\"");
        let theme_pos = with_theme.find("theme = \"catppuccin\"").expect("theme should be inserted");
        let table_pos = with_theme.find("[anilist]").expect("table should remain");
        assert!(theme_pos < table_pos, "root key must be inserted before [anilist] table");
    }

    #[test]
    fn test_settings_modal_navigation_and_cycling() {
        set_active_default_provider(MangaProviders::Mangadex);
        set_active_download_type(DownloadType::Cbz);
        set_active_chapter_language(Languages::Spanish);
        set_active_ui_language(UiLanguage::Es);
        set_theme(ThemeVariant::TokyoNight);

        let mut state = SettingsModalState::new();
        assert_eq!(state.current_field(), SettingsField::DefaultProvider);

        // Cycle provider forward: Mangadex -> Inmanga
        let outcome = state.handle_key_event(KeyCode::Right.into());
        assert_eq!(outcome, SettingsOutcome::ProviderChanged(MangaProviders::Inmanga));
        assert_eq!(active_default_provider(), MangaProviders::Inmanga);

        // Move down to DownloadFormat and cycle: Cbz -> Pdf -> Epub
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::DownloadFormat);
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::DownloadFormatChanged(DownloadType::Pdf));
        assert_eq!(active_download_type(), DownloadType::Pdf);
        assert_eq!(state.handle_key_event(KeyCode::Enter.into()), SettingsOutcome::DownloadFormatChanged(DownloadType::Epub));
        assert_eq!(active_download_type(), DownloadType::Epub);

        // Move down to ChapterLanguage and cycle: Spanish -> SpanishLa -> English
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::ChapterLanguage);
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::ChapterLanguageChanged(Languages::SpanishLa));
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::ChapterLanguageChanged(Languages::English));
        assert_eq!(*active_chapter_language_ref(), Languages::English);

        // Move down to UiLanguage and toggle: Es -> En -> Es
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::UiLanguage);
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::UiLanguageChanged(UiLanguage::En));
        assert_eq!(active_ui_language(), UiLanguage::En);
        assert_eq!(state.handle_key_event(KeyCode::Left.into()), SettingsOutcome::UiLanguageChanged(UiLanguage::Es));
        assert_eq!(active_ui_language(), UiLanguage::Es);

        // Move down to Theme and cycle: TokyoNight -> Catppuccin -> Classic
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::Theme);
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::ThemeChanged(ThemeVariant::Catppuccin));
        assert_eq!(current_theme(), ThemeVariant::Catppuccin);

        // Closing with 'c' or F5
        assert_eq!(state.handle_key_event(KeyCode::Char('c').into()), SettingsOutcome::Close);
        assert_eq!(state.handle_key_event(KeyCode::F(5).into()), SettingsOutcome::Close);

        // Reset defaults for other tests
        set_active_default_provider(MangaProviders::Mangadex);
        set_active_download_type(DownloadType::Cbz);
        set_active_chapter_language(Languages::English);
        set_active_ui_language(UiLanguage::Es);
        set_theme(ThemeVariant::TokyoNight);
    }
}
