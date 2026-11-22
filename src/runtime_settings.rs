use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

use crate::backend::manga_provider::{Languages, MangaProviders};
use crate::config::{DownloadType, ImageProtocol, MangaTuiConfig, UiLanguage};
use crate::i18n::tr;

/// Visual theme variants supported across all tabs and widgets in `manga-tui`.
#[derive(Default, Debug, Serialize, Deserialize, Display, EnumIter, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeVariant {
    #[default]
    #[serde(alias = "tokyonight", alias = "tokyo-night", alias = "tokyo")]
    TokyoNight,
    #[serde(alias = "catppuccin_mocha", alias = "mocha")]
    Catppuccin,
    #[serde(alias = "clasico", alias = "clásico", alias = "default")]
    Classic,
}

impl ThemeVariant {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::TokyoNight => 0,
            Self::Catppuccin => 1,
            Self::Classic => 2,
        }
    }

    #[inline]
    pub const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::TokyoNight,
            1 => Self::Catppuccin,
            2 => Self::Classic,
            _ => Self::TokyoNight,
        }
    }

    #[inline]
    pub const fn next(self) -> Self {
        match self {
            Self::TokyoNight => Self::Catppuccin,
            Self::Catppuccin => Self::Classic,
            Self::Classic => Self::TokyoNight,
        }
    }

    #[inline]
    pub fn label(self) -> &'static str {
        match self {
            Self::TokyoNight => "Tokyo Night",
            Self::Catppuccin => "Catppuccin",
            Self::Classic => tr("Clásico", "Classic"),
        }
    }
}

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(u8::MAX);

/// Returns the currently active `ThemeVariant`.
#[inline]
pub fn current_theme() -> ThemeVariant {
    let raw = ACTIVE_THEME.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = MangaTuiConfig::try_get().map(|c| c.theme).unwrap_or_default();
        ACTIVE_THEME.store(initial.as_u8(), Ordering::Relaxed);
        initial
    } else {
        ThemeVariant::from_u8(raw)
    }
}

/// Sets the active `ThemeVariant` globally.
#[inline]
pub fn set_theme(variant: ThemeVariant) {
    ACTIVE_THEME.store(variant.as_u8(), Ordering::Relaxed);
}

/// Cycles to the next `ThemeVariant` (`Tokyo Night` → `Catppuccin` → `Clásico`) and returns it.
#[inline]
pub fn cycle_theme() -> ThemeVariant {
    let next = current_theme().next();
    set_theme(next);
    next
}

static ACTIVE_PROVIDER: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_DOWNLOAD_TYPE: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_CHAPTER_LANG: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_UI_LANG: AtomicU8 = AtomicU8::new(u8::MAX);
static ACTIVE_IMAGE_PROTOCOL: AtomicU8 = AtomicU8::new(u8::MAX);

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

pub const ALL_IMAGE_PROTOCOLS: [ImageProtocol; 5] =
    [ImageProtocol::Auto, ImageProtocol::Sixel, ImageProtocol::Kitty, ImageProtocol::Iterm2, ImageProtocol::None];

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

pub fn provider_to_u8(p: MangaProviders) -> u8 {
    ALL_PROVIDERS.iter().position(|&item| item == p).unwrap_or(0) as u8
}

pub fn provider_from_u8(val: u8) -> MangaProviders {
    ALL_PROVIDERS.get(val as usize).copied().unwrap_or_default()
}

pub fn download_type_to_u8(dt: DownloadType) -> u8 {
    ALL_DOWNLOAD_FORMATS.iter().position(|&item| item == dt).unwrap_or(0) as u8
}

pub fn download_type_from_u8(val: u8) -> DownloadType {
    ALL_DOWNLOAD_FORMATS.get(val as usize).copied().unwrap_or_default()
}

pub fn ui_lang_to_u8(lang: UiLanguage) -> u8 {
    match lang {
        UiLanguage::Es => 0,
        UiLanguage::En => 1,
    }
}

pub fn ui_lang_from_u8(val: u8) -> UiLanguage {
    match val {
        1 => UiLanguage::En,
        _ => UiLanguage::Es,
    }
}

pub fn image_protocol_to_u8(proto: ImageProtocol) -> u8 {
    match proto {
        ImageProtocol::Auto => 0,
        ImageProtocol::Sixel => 1,
        ImageProtocol::Halfblocks => 2,
        ImageProtocol::Kitty => 3,
        ImageProtocol::Iterm2 => 4,
        ImageProtocol::None => 5,
    }
}

pub fn image_protocol_from_u8(val: u8) -> ImageProtocol {
    match val {
        1 => ImageProtocol::Sixel,
        2 => ImageProtocol::Halfblocks,
        3 => ImageProtocol::Kitty,
        4 => ImageProtocol::Iterm2,
        5 => ImageProtocol::None,
        _ => ImageProtocol::Auto,
    }
}

pub fn language_to_u8(lang: Languages) -> u8 {
    Languages::iter().position(|item| item == lang).unwrap_or(1) as u8
}

pub fn language_from_u8(val: u8) -> Languages {
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

#[inline]
pub fn active_image_protocol() -> ImageProtocol {
    let raw = ACTIVE_IMAGE_PROTOCOL.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = MangaTuiConfig::try_get().map(|c| c.image_protocol).unwrap_or_default();
        ACTIVE_IMAGE_PROTOCOL.store(image_protocol_to_u8(initial), Ordering::Relaxed);
        initial
    } else {
        image_protocol_from_u8(raw)
    }
}

#[inline]
pub fn set_active_image_protocol(proto: ImageProtocol) {
    ACTIVE_IMAGE_PROTOCOL.store(image_protocol_to_u8(proto), Ordering::Relaxed);
}

/// Initializes the live atomic settings from a `MangaTuiConfig`.
pub fn init_runtime_settings(config: &MangaTuiConfig) {
    set_active_default_provider(config.default_manga_provider);
    set_active_download_type(config.download_type);
    set_active_chapter_language(config.default_language);
    set_active_ui_language(config.ui_language);
    set_theme(config.theme);
    set_active_image_protocol(config.image_protocol);
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
}
