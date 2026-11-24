//! Visual customization and layout preferences for the Mangora desktop UI.
//!
//! Persists palette selection, card density, collection sorting, view mode, and
//! reader background preferences inside `[mangora_appearance]` in the shared
//! `config.toml` file without affecting `manga-tui` compatibility.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

use crate::config::MangaTuiConfig;

const MANGORA_APPEARANCE_TABLE: &str = "mangora_appearance";

fn default_true() -> bool {
    true
}

/// Editorial palettes inspired by paper, ink, and classic manga print editions.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EditorialPalette {
    #[default]
    ClassicPaper,
    NightInk,
    EditorialSepia,
    ShonenRed,
    SumiSlate,
}

impl EditorialPalette {
    /// Returns the canonical TOML/CSS identifier for this palette.
    pub const fn toml_value(self) -> &'static str {
        match self {
            Self::ClassicPaper => "classic_paper",
            Self::NightInk => "night_ink",
            Self::EditorialSepia => "editorial_sepia",
            Self::ShonenRed => "shonen_red",
            Self::SumiSlate => "sumi_slate",
        }
    }

    /// Returns the full visual token specification for this palette.
    pub fn spec(self) -> PaletteSpec {
        match self {
            Self::ClassicPaper => PaletteSpec {
                id: self,
                name_es: "Papel Clásico".to_string(),
                name_en: "Classic Paper".to_string(),
                description_es: "Papel editorial cálido y tinta negra de alto contraste.".to_string(),
                description_en: "Warm editorial paper with high-contrast black ink.".to_string(),
                is_dark: false,
                paper: "#f8f7f1".to_string(),
                surface: "#ecebe3".to_string(),
                ink: "#191918".to_string(),
                red: "#cb3028".to_string(),
                red_hover: "#a1201b".to_string(),
                muted: "#55554f".to_string(),
                line: "#c8c7bc".to_string(),
                focus: "#0077b6".to_string(),
                reader_bar_bg: "#191918".to_string(),
                reader_bar_fg: "#f8f7f1".to_string(),
            },
            Self::NightInk => PaletteSpec {
                id: self,
                name_es: "Tinta Nocturna".to_string(),
                name_en: "Night Ink".to_string(),
                description_es: "Modo oscuro en tinta mate pensado para lectura nocturna.".to_string(),
                description_en: "Matte ink dark mode designed for late-night reading.".to_string(),
                is_dark: true,
                paper: "#141413".to_string(),
                surface: "#1f1f1d".to_string(),
                ink: "#f8f7f1".to_string(),
                red: "#ff8a76".to_string(),
                red_hover: "#cb3028".to_string(),
                muted: "#b5b4a8".to_string(),
                line: "#3a3a35".to_string(),
                focus: "#4cc9f0".to_string(),
                reader_bar_bg: "#0c0c0b".to_string(),
                reader_bar_fg: "#f8f7f1".to_string(),
            },
            Self::EditorialSepia => PaletteSpec {
                id: self,
                name_es: "Sepia Editorial".to_string(),
                name_en: "Editorial Sepia".to_string(),
                description_es: "Tonos de tomo antiguo y tinta marrón suave para descansar la vista.".to_string(),
                description_en: "Vintage volume paper tones with soft brown ink for gentle contrast.".to_string(),
                is_dark: false,
                paper: "#f4ecd8".to_string(),
                surface: "#e7dcc2".to_string(),
                ink: "#2b2118".to_string(),
                red: "#a83226".to_string(),
                red_hover: "#832218".to_string(),
                muted: "#665746".to_string(),
                line: "#cbbba0".to_string(),
                focus: "#1d6fa5".to_string(),
                reader_bar_bg: "#2b2118".to_string(),
                reader_bar_fg: "#f4ecd8".to_string(),
            },
            Self::ShonenRed => PaletteSpec {
                id: self,
                name_es: "Shonen Rojo".to_string(),
                name_en: "Shonen Red".to_string(),
                description_es: "Papel blanco vivo con acentos rojos intensos de revista semanal.".to_string(),
                description_en: "Crisp white stock with bold weekly-magazine crimson accents.".to_string(),
                is_dark: false,
                paper: "#fffaf5".to_string(),
                surface: "#fcebe6".to_string(),
                ink: "#171414".to_string(),
                red: "#d91e2a".to_string(),
                red_hover: "#a6111c".to_string(),
                muted: "#5a4c4c".to_string(),
                line: "#e3c2bd".to_string(),
                focus: "#0077b6".to_string(),
                reader_bar_bg: "#171414".to_string(),
                reader_bar_fg: "#fffaf5".to_string(),
            },
            Self::SumiSlate => PaletteSpec {
                id: self,
                name_es: "Sumi Grafito".to_string(),
                name_en: "Sumi Slate".to_string(),
                description_es: "Gris grafito equilibrado con trazos de tinta clara y coral.".to_string(),
                description_en: "Balanced graphite slate with crisp light ink and coral highlights.".to_string(),
                is_dark: true,
                paper: "#1c1f24".to_string(),
                surface: "#252930".to_string(),
                ink: "#eff1f5".to_string(),
                red: "#f06553".to_string(),
                red_hover: "#cb3028".to_string(),
                muted: "#a6adb9".to_string(),
                line: "#3b424d".to_string(),
                focus: "#66b2ff".to_string(),
                reader_bar_bg: "#121417".to_string(),
                reader_bar_fg: "#eff1f5".to_string(),
            },
        }
    }
}

/// Card grid density for Library and Explore views.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CardDensity {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

impl CardDensity {
    pub const fn toml_value(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
            Self::Spacious => "spacious",
        }
    }
}

/// Display layout mode for the Library collection.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum LibraryViewMode {
    #[default]
    Grid,
    List,
}

impl LibraryViewMode {
    pub const fn toml_value(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::List => "list",
        }
    }
}

/// Custom sorting options for the Library view.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum LibrarySortBy {
    #[default]
    LastRead,
    TitleAsc,
    TitleDesc,
    ChaptersReadDesc,
    DateAddedDesc,
    FavoritesFirst,
}

impl LibrarySortBy {
    pub const fn toml_value(self) -> &'static str {
        match self {
            Self::LastRead => "last_read",
            Self::TitleAsc => "title_asc",
            Self::TitleDesc => "title_desc",
            Self::ChaptersReadDesc => "chapters_read_desc",
            Self::DateAddedDesc => "date_added_desc",
            Self::FavoritesFirst => "favorites_first",
        }
    }
}

/// Custom sorting options for the Explore / Search catalog.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ExploreSortBy {
    #[default]
    Relevance,
    TitleAsc,
    TitleDesc,
    StatusOngoingFirst,
}

impl ExploreSortBy {
    pub const fn toml_value(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::TitleAsc => "title_asc",
            Self::TitleDesc => "title_desc",
            Self::StatusOngoingFirst => "status_ongoing_first",
        }
    }
}

/// Subtle canvas background options inside the HD manga reader.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ReaderBackground {
    #[default]
    ThemeDefault,
    PurePaper,
    WarmSepia,
    SoftGray,
    MatteInk,
    PitchBlack,
    SubtleScreentone,
}

impl ReaderBackground {
    pub const fn toml_value(self) -> &'static str {
        match self {
            Self::ThemeDefault => "theme_default",
            Self::PurePaper => "pure_paper",
            Self::WarmSepia => "warm_sepia",
            Self::SoftGray => "soft_gray",
            Self::MatteInk => "matte_ink",
            Self::PitchBlack => "pitch_black",
            Self::SubtleScreentone => "subtle_screentone",
        }
    }

    pub fn spec(self) -> ReaderBackgroundSpec {
        match self {
            Self::ThemeDefault => ReaderBackgroundSpec {
                id: self,
                name_es: "Según tema activo".to_string(),
                name_en: "Match active theme".to_string(),
                description_es: "Usa el fondo principal de la paleta editorial elegida.".to_string(),
                description_en: "Uses the main canvas color of the active editorial palette.".to_string(),
                css_value: "var(--paper)".to_string(),
                is_dark: false,
                has_pattern: false,
            },
            Self::PurePaper => ReaderBackgroundSpec {
                id: self,
                name_es: "Papel Puro".to_string(),
                name_en: "Pure Paper".to_string(),
                description_es: "Fondo claro de papel manga para lectura diurna.".to_string(),
                description_en: "Clean manga paper canvas for daytime reading.".to_string(),
                css_value: "#f8f7f1".to_string(),
                is_dark: false,
                has_pattern: false,
            },
            Self::WarmSepia => ReaderBackgroundSpec {
                id: self,
                name_es: "Sepia Suave".to_string(),
                name_en: "Warm Sepia".to_string(),
                description_es: "Papel cálido de volumen impreso que reduce el deslumbramiento.".to_string(),
                description_en: "Warm printed volume backdrop that softens glare.".to_string(),
                css_value: "#f4ecd8".to_string(),
                is_dark: false,
                has_pattern: false,
            },
            Self::SoftGray => ReaderBackgroundSpec {
                id: self,
                name_es: "Gris Neutro".to_string(),
                name_en: "Soft Gray".to_string(),
                description_es: "Gris editorial tenue para resaltar los márgenes blancos de la página.".to_string(),
                description_en: "Neutral editorial gray that frames white manga margins cleanly.".to_string(),
                css_value: "#e6e5de".to_string(),
                is_dark: false,
                has_pattern: false,
            },
            Self::MatteInk => ReaderBackgroundSpec {
                id: self,
                name_es: "Tinta Mate".to_string(),
                name_en: "Matte Ink".to_string(),
                description_es: "Fondo oscuro suave sin reflejos intensos.".to_string(),
                description_en: "Low-glare matte dark canvas.".to_string(),
                css_value: "#191918".to_string(),
                is_dark: true,
                has_pattern: false,
            },
            Self::PitchBlack => ReaderBackgroundSpec {
                id: self,
                name_es: "Negro Absoluto".to_string(),
                name_en: "Pitch Black".to_string(),
                description_es: "Contraste máximo en pantallas OLED o habitaciones a oscuras.".to_string(),
                description_en: "Maximum contrast for OLED displays and dark rooms.".to_string(),
                css_value: "#090909".to_string(),
                is_dark: true,
                has_pattern: false,
            },
            Self::SubtleScreentone => ReaderBackgroundSpec {
                id: self,
                name_es: "Trama Screentone".to_string(),
                name_en: "Subtle Screentone".to_string(),
                description_es: "Patrón sutil de puntos de imprenta manga alrededor de las viñetas.".to_string(),
                description_en: "Subtle manga print halftone dot pattern framing the page.".to_string(),
                css_value: "radial-gradient(rgba(25, 25, 24, 0.08) 1px, transparent 1.2px) 0 0 / 8px 8px, var(--paper)".to_string(),
                is_dark: false,
                has_pattern: true,
            },
        }
    }
}

/// Complete design-token definition for an editorial palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteSpec {
    pub id: EditorialPalette,
    pub name_es: String,
    pub name_en: String,
    pub description_es: String,
    pub description_en: String,
    pub is_dark: bool,
    pub paper: String,
    pub surface: String,
    pub ink: String,
    pub red: String,
    pub red_hover: String,
    pub muted: String,
    pub line: String,
    pub focus: String,
    pub reader_bar_bg: String,
    pub reader_bar_fg: String,
}

/// Complete specification for a reader canvas background.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderBackgroundSpec {
    pub id: ReaderBackground,
    pub name_es: String,
    pub name_en: String,
    pub description_es: String,
    pub description_en: String,
    pub css_value: String,
    pub is_dark: bool,
    pub has_pattern: bool,
}

/// Persisted visual customization configuration for Mangora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MangoraAppearanceConfig {
    pub palette: EditorialPalette,
    pub library_density: CardDensity,
    pub explore_density: CardDensity,
    pub library_view_mode: LibraryViewMode,
    pub library_sort: LibrarySortBy,
    pub explore_sort: ExploreSortBy,
    pub reader_background: ReaderBackground,
    #[serde(default = "default_true")]
    pub show_screentone_accents: bool,
}

impl Default for MangoraAppearanceConfig {
    fn default() -> Self {
        Self {
            palette: EditorialPalette::default(),
            library_density: CardDensity::default(),
            explore_density: CardDensity::default(),
            library_view_mode: LibraryViewMode::default(),
            library_sort: LibrarySortBy::default(),
            explore_sort: ExploreSortBy::default(),
            reader_background: ReaderBackground::default(),
            show_screentone_accents: true,
        }
    }
}

/// Returns all editorial palettes in display order.
pub fn all_palette_specs() -> Vec<PaletteSpec> {
    EditorialPalette::iter().map(EditorialPalette::spec).collect()
}

/// Returns all subtle reader background presets in display order.
pub fn all_reader_background_specs() -> Vec<ReaderBackgroundSpec> {
    ReaderBackground::iter().map(ReaderBackground::spec).collect()
}

/// Formats `[mangora_appearance]` as a clean TOML table block.
pub fn format_mangora_appearance_toml_table(appearance: &MangoraAppearanceConfig) -> String {
    format!(
        "# Visual customization settings for the Mangora desktop interface\n\
         [{MANGORA_APPEARANCE_TABLE}]\n\
         palette = \"{}\"\n\
         library_density = \"{}\"\n\
         explore_density = \"{}\"\n\
         library_view_mode = \"{}\"\n\
         library_sort = \"{}\"\n\
         explore_sort = \"{}\"\n\
         reader_background = \"{}\"\n\
         show_screentone_accents = {}\n",
        appearance.palette.toml_value(),
        appearance.library_density.toml_value(),
        appearance.explore_density.toml_value(),
        appearance.library_view_mode.toml_value(),
        appearance.library_sort.toml_value(),
        appearance.explore_sort.toml_value(),
        appearance.reader_background.toml_value(),
        appearance.show_screentone_accents,
    )
}

/// Inserts or replaces the `[mangora_appearance]` table inside `config.toml` contents while
/// preserving root keys, comments, and unrelated tables such as `[anilist]`.
pub fn update_mangora_appearance_in_toml(contents: &str, appearance: &MangoraAppearanceConfig) -> String {
    let table_block = format_mangora_appearance_toml_table(appearance);
    let expected_header = format!("[{MANGORA_APPEARANCE_TABLE}]");
    let lines: Vec<&str> = contents.lines().collect();

    let header_idx = lines.iter().position(|line| line.trim() == expected_header);

    match header_idx {
        Some(start_idx) => {
            let mut end_idx = lines.len();
            for (idx, line) in lines.iter().enumerate().skip(start_idx + 1) {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    end_idx = idx;
                    break;
                }
            }

            // Include the comment line immediately above `[mangora_appearance]` if it is ours
            let mut replace_from = start_idx;
            if replace_from > 0 && lines[replace_from - 1].trim().starts_with("# Visual customization") {
                replace_from -= 1;
            }

            let mut out = String::new();
            for line in &lines[..replace_from] {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str(&table_block);

            if end_idx < lines.len() {
                out.push('\n');
                for line in &lines[end_idx..] {
                    out.push_str(line);
                    out.push('\n');
                }
            }

            out
        },
        None => {
            let mut out = contents.trim_end_matches(['\r', '\n']).to_string();
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&table_block);
            out
        },
    }
}

/// Loads `MangoraAppearanceConfig` from `dir/config.toml`, falling back safely to defaults.
pub fn load_appearance_from_dir(dir: &Path) -> MangoraAppearanceConfig {
    let config_path = dir.join("config.toml");
    let Ok(contents) = fs::read_to_string(&config_path) else {
        return MangoraAppearanceConfig::default();
    };

    if let Ok(full_config) = toml::from_str::<MangaTuiConfig>(&contents) {
        return full_config.mangora_appearance;
    }

    // Fallback: parse just `[mangora_appearance]` if another unrelated section had malformed TOML.
    if let Ok(table) = toml::from_str::<toml::Table>(&contents)
        && let Some(section) = table.get(MANGORA_APPEARANCE_TABLE)
        && let Ok(parsed) = section.clone().try_into::<MangoraAppearanceConfig>()
    {
        return parsed;
    }

    MangoraAppearanceConfig::default()
}

/// Atomically saves `MangoraAppearanceConfig` into `dir/config.toml`.
pub fn save_appearance_to_dir(dir: &Path, appearance: &MangoraAppearanceConfig) -> Result<(), String> {
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    crate::config::ensure_config_file_in_dir(dir).map_err(|e| e.to_string())?;

    let config_path = dir.join("config.toml");
    let current = fs::read_to_string(&config_path).unwrap_or_default();
    let updated = update_mangora_appearance_in_toml(&current, appearance);

    // Sanity check that the resulting TOML still parses cleanly as a full `MangaTuiConfig` table.
    let _: toml::Table = toml::from_str(&updated).map_err(|e| format!("Invalid TOML generated: {e}"))?;

    let tmp_path = dir.join("config.toml.tmp");
    {
        let mut tmp = File::create(&tmp_path).map_err(|e| e.to_string())?;
        tmp.write_all(updated.as_bytes()).map_err(|e| e.to_string())?;
        tmp.flush().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp_path, &config_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Loads `MangoraAppearanceConfig` from the active configuration directory.
pub fn load_appearance() -> MangoraAppearanceConfig {
    if cfg!(any(test, feature = "test-utils")) {
        return MangoraAppearanceConfig::default();
    }
    let dir = crate::config::get_config_directory_path();
    load_appearance_from_dir(&dir)
}

/// Persists `MangoraAppearanceConfig` to the active configuration directory.
pub fn save_appearance(appearance: &MangoraAppearanceConfig) -> Result<(), String> {
    if cfg!(any(test, feature = "test-utils")) {
        return Ok(());
    }
    let dir = crate::config::get_config_directory_path();
    save_appearance_to_dir(&dir, appearance)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn editorial_palettes_match_design_tokens() {
        let specs = all_palette_specs();
        assert_eq!(specs.len(), 5);

        let classic = EditorialPalette::ClassicPaper.spec();
        assert_eq!(classic.paper, "#f8f7f1");
        assert_eq!(classic.surface, "#ecebe3");
        assert_eq!(classic.ink, "#191918");
        assert_eq!(classic.red, "#cb3028");
        assert_eq!(classic.red_hover, "#a1201b");
        assert_eq!(classic.muted, "#55554f");
        assert_eq!(classic.line, "#c8c7bc");
        assert_eq!(classic.focus, "#0077b6");
        assert!(!classic.is_dark);

        let night = EditorialPalette::NightInk.spec();
        assert_eq!(night.paper, "#141413");
        assert_eq!(night.surface, "#1f1f1d");
        assert_eq!(night.ink, "#f8f7f1");
        assert_eq!(night.red, "#ff8a76");
        assert_eq!(night.line, "#3a3a35");
        assert!(night.is_dark);

        let backgrounds = all_reader_background_specs();
        assert_eq!(backgrounds.len(), 7);
        assert!(ReaderBackground::SubtleScreentone.spec().has_pattern);
        assert!(ReaderBackground::PitchBlack.spec().is_dark);
    }

    #[test]
    fn mangora_appearance_serde_defaults_handle_partial_json() {
        let parsed: MangoraAppearanceConfig = serde_json::from_str(r#"{"palette": "editorial_sepia"}"#).unwrap();
        assert_eq!(parsed.palette, EditorialPalette::EditorialSepia);
        assert_eq!(parsed.library_density, CardDensity::Comfortable);
        assert_eq!(parsed.explore_density, CardDensity::Comfortable);
        assert_eq!(parsed.library_view_mode, LibraryViewMode::Grid);
        assert_eq!(parsed.library_sort, LibrarySortBy::LastRead);
        assert_eq!(parsed.explore_sort, ExploreSortBy::Relevance);
        assert_eq!(parsed.reader_background, ReaderBackground::ThemeDefault);
        assert!(parsed.show_screentone_accents);
    }

    #[test]
    fn update_mangora_appearance_in_toml_preserves_comments_and_other_tables() {
        let base_toml = r#"# User settings
ui_language = "es"
download_type = "cbz"

[anilist]
client_id = "abc"
access_token = "xyz"
"#;

        let custom = MangoraAppearanceConfig {
            palette: EditorialPalette::ShonenRed,
            library_density: CardDensity::Compact,
            explore_density: CardDensity::Spacious,
            library_view_mode: LibraryViewMode::List,
            library_sort: LibrarySortBy::TitleAsc,
            explore_sort: ExploreSortBy::StatusOngoingFirst,
            reader_background: ReaderBackground::MatteInk,
            show_screentone_accents: false,
        };

        let with_table = update_mangora_appearance_in_toml(base_toml, &custom);
        assert!(with_table.contains("# User settings"));
        assert!(with_table.contains("[anilist]\nclient_id = \"abc\""));
        assert!(with_table.contains("palette = \"shonen_red\""));
        assert!(with_table.contains("show_screentone_accents = false"));

        let updated = MangoraAppearanceConfig {
            palette: EditorialPalette::NightInk,
            library_density: CardDensity::Spacious,
            show_screentone_accents: true,
            ..custom
        };

        let replaced = update_mangora_appearance_in_toml(&with_table, &updated);
        assert_eq!(replaced.matches("[mangora_appearance]").count(), 1);
        assert!(replaced.contains("[anilist]\nclient_id = \"abc\""));
        assert!(replaced.contains("palette = \"night_ink\""));
        assert!(replaced.contains("library_density = \"spacious\""));
        assert!(replaced.contains("show_screentone_accents = true"));
    }

    #[test]
    fn save_and_load_appearance_roundtrip_across_restarts() {
        let temp_dir = PathBuf::from(format!("./test_results/mangora_appearance_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let expected = MangoraAppearanceConfig {
            palette: EditorialPalette::SumiSlate,
            library_density: CardDensity::Compact,
            explore_density: CardDensity::Spacious,
            library_view_mode: LibraryViewMode::List,
            library_sort: LibrarySortBy::FavoritesFirst,
            explore_sort: ExploreSortBy::TitleDesc,
            reader_background: ReaderBackground::SubtleScreentone,
            show_screentone_accents: false,
        };

        save_appearance_to_dir(&temp_dir, &expected).unwrap();
        let loaded = load_appearance_from_dir(&temp_dir);
        assert_eq!(loaded, expected);

        let full_toml = fs::read_to_string(temp_dir.join("config.toml")).unwrap();
        let parsed_full: MangaTuiConfig = toml::from_str(&full_toml).unwrap();
        assert_eq!(parsed_full.mangora_appearance, expected);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
