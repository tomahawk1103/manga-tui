use once_cell::sync::Lazy;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType};

pub use crate::runtime_settings::{ThemeVariant, current_theme, cycle_theme, set_theme};

pub static INSTRUCTIONS_STYLE: Lazy<Style> = Lazy::new(|| Style::default().bold().underlined().yellow());

pub static ERROR_STYLE: Lazy<Style> = Lazy::new(|| Style::default().bold().underlined().red().on_black());

pub static CURRENT_LIST_ITEM_STYLE: Lazy<Style> = Lazy::new(|| Style::default().on_blue());

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
    pub const fn for_variant(variant: ThemeVariant) -> Self {
        match variant {
            ThemeVariant::TokyoNight => ThemePalette {
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
            ThemeVariant::Catppuccin => ThemePalette {
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
            ThemeVariant::Classic => ThemePalette {
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

/// Returns the centralized `ThemePalette` for the currently active theme.
#[inline]
pub fn theme() -> ThemePalette {
    ThemePalette::for_variant(current_theme())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycles_through_all_variants() {
        set_theme(ThemeVariant::TokyoNight);
        assert_eq!(current_theme(), ThemeVariant::TokyoNight);
        assert_eq!(theme(), ThemePalette::for_variant(ThemeVariant::TokyoNight));

        let second = cycle_theme();
        assert_eq!(second, ThemeVariant::Catppuccin);
        assert_eq!(current_theme(), ThemeVariant::Catppuccin);
        assert_eq!(theme(), ThemePalette::for_variant(ThemeVariant::Catppuccin));

        let third = cycle_theme();
        assert_eq!(third, ThemeVariant::Classic);
        assert_eq!(current_theme(), ThemeVariant::Classic);
        assert_eq!(theme(), ThemePalette::for_variant(ThemeVariant::Classic));

        let back = cycle_theme();
        assert_eq!(back, ThemeVariant::TokyoNight);
    }
}
