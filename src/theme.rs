use std::sync::atomic::{AtomicU8, Ordering};

use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter};

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

    #[inline]
    pub const fn palette(self) -> ThemePalette {
        match self {
            Self::TokyoNight => ThemePalette {
                accent_primary: Color::Rgb(0, 210, 255),
                accent_secondary: Color::Rgb(168, 128, 255),
                accent_highlight: Color::Rgb(224, 175, 104),
                border: Color::Rgb(75, 88, 120),
                border_accent: Color::Rgb(78, 138, 210),
                border_secondary: Color::Rgb(122, 96, 190),
                border_muted: Color::Rgb(52, 60, 86),
                bar_bg: Color::Rgb(24, 27, 36),
                chip_bg: Color::Rgb(41, 46, 66),
                title_chip_primary_bg: Color::Rgb(20, 42, 64),
                title_chip_secondary_bg: Color::Rgb(38, 28, 60),
                row_selected_bg: Color::Rgb(42, 49, 72),
                text_primary: Color::White,
                text_secondary: Color::Rgb(198, 206, 232),
                text_muted: Color::Rgb(136, 144, 176),
                text_dim: Color::Rgb(92, 100, 132),
                status_success: Color::Rgb(110, 220, 140),
                status_warning: Color::Rgb(255, 200, 80),
                status_error: Color::Rgb(247, 118, 142),
            },
            Self::Catppuccin => ThemePalette {
                accent_primary: Color::Rgb(137, 220, 235),
                accent_secondary: Color::Rgb(203, 166, 247),
                accent_highlight: Color::Rgb(249, 226, 175),
                border: Color::Rgb(88, 91, 112),
                border_accent: Color::Rgb(137, 180, 250),
                border_secondary: Color::Rgb(180, 142, 230),
                border_muted: Color::Rgb(69, 71, 90),
                bar_bg: Color::Rgb(24, 24, 37),
                chip_bg: Color::Rgb(49, 50, 68),
                title_chip_primary_bg: Color::Rgb(30, 48, 68),
                title_chip_secondary_bg: Color::Rgb(48, 34, 68),
                row_selected_bg: Color::Rgb(58, 60, 82),
                text_primary: Color::Rgb(205, 214, 244),
                text_secondary: Color::Rgb(186, 194, 222),
                text_muted: Color::Rgb(147, 153, 178),
                text_dim: Color::Rgb(108, 112, 134),
                status_success: Color::Rgb(166, 227, 161),
                status_warning: Color::Rgb(249, 226, 175),
                status_error: Color::Rgb(243, 139, 168),
            },
            Self::Classic => ThemePalette {
                accent_primary: Color::Cyan,
                accent_secondary: Color::Magenta,
                accent_highlight: Color::Yellow,
                border: Color::Rgb(75, 88, 120),
                border_accent: Color::Cyan,
                border_secondary: Color::Magenta,
                border_muted: Color::DarkGray,
                bar_bg: Color::Rgb(20, 22, 30),
                chip_bg: Color::Rgb(42, 48, 66),
                title_chip_primary_bg: Color::Rgb(18, 40, 58),
                title_chip_secondary_bg: Color::Rgb(42, 24, 56),
                row_selected_bg: Color::Rgb(38, 48, 72),
                text_primary: Color::White,
                text_secondary: Color::Gray,
                text_muted: Color::Gray,
                text_dim: Color::DarkGray,
                status_success: Color::Green,
                status_warning: Color::Yellow,
                status_error: Color::LightRed,
            },
        }
    }
}

/// Centralized semantic color palette shared across all screens and widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub accent_primary: Color,
    pub accent_secondary: Color,
    pub accent_highlight: Color,
    pub border: Color,
    pub border_accent: Color,
    pub border_secondary: Color,
    pub border_muted: Color,
    pub bar_bg: Color,
    pub chip_bg: Color,
    pub title_chip_primary_bg: Color,
    pub title_chip_secondary_bg: Color,
    pub row_selected_bg: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub status_success: Color,
    pub status_warning: Color,
    pub status_error: Color,
}

impl ThemePalette {
    #[inline]
    pub fn key_chip(&self, key: &str) -> Span<'static> {
        Span::styled(format!(" {key} "), Style::default().fg(self.accent_primary).bg(self.chip_bg).bold())
    }

    #[inline]
    pub fn highlight_key_chip(&self, key: &str) -> Span<'static> {
        Span::styled(format!(" {key} "), Style::default().fg(self.accent_highlight).bg(self.chip_bg).bold())
    }

    #[inline]
    pub fn action_chip_primary(&self, key: &str) -> Span<'static> {
        Span::styled(format!(" {key} "), Style::default().fg(Color::Black).bg(self.accent_primary).bold())
    }

    #[inline]
    pub fn action_chip_secondary(&self, key: &str) -> Span<'static> {
        Span::styled(format!(" {key} "), Style::default().fg(Color::Black).bg(self.accent_secondary).bold())
    }

    #[inline]
    pub fn rounded_block(&self) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
    }

    #[inline]
    pub fn rounded_accent_block(&self) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border_accent))
    }

    #[inline]
    pub fn rounded_secondary_block(&self) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border_secondary))
    }
}

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(u8::MAX);

/// Returns the currently active `ThemeVariant`.
#[inline]
pub fn current_theme() -> ThemeVariant {
    let raw = ACTIVE_THEME.load(Ordering::Relaxed);
    if raw == u8::MAX {
        let initial = crate::config::MangaTuiConfig::try_get().map(|c| c.theme).unwrap_or_default();
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

/// Returns the centralized `ThemePalette` for the currently active theme.
#[inline]
pub fn theme() -> ThemePalette {
    current_theme().palette()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycles_through_all_variants() {
        set_theme(ThemeVariant::TokyoNight);
        assert_eq!(current_theme(), ThemeVariant::TokyoNight);
        assert_eq!(theme(), ThemeVariant::TokyoNight.palette());

        let second = cycle_theme();
        assert_eq!(second, ThemeVariant::Catppuccin);
        assert_eq!(current_theme(), ThemeVariant::Catppuccin);
        assert_eq!(theme(), ThemeVariant::Catppuccin.palette());

        let third = cycle_theme();
        assert_eq!(third, ThemeVariant::Classic);
        assert_eq!(current_theme(), ThemeVariant::Classic);
        assert_eq!(theme(), ThemeVariant::Classic.palette());

        let back = cycle_theme();
        assert_eq!(back, ThemeVariant::TokyoNight);
    }
}
