use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget, Wrap};

use crate::backend::manga_provider::{Languages, MangaProviders};
use crate::backend::multi_provider::{ProviderHealthStatus, get_provider_health_status};
use crate::config::{AnimePlayer, AnimeProviders, AnimeVariant, DownloadType, ImageProtocol, UiLanguage};
use crate::i18n::tr;
pub use crate::runtime_settings::*;
use crate::theme::{ThemePalette, theme};
use crate::utils::centered_rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    DefaultProvider,
    DownloadFormat,
    ChapterLanguage,
    UiLanguage,
    Theme,
    ImageProtocol,
    AnimeProvider,
    AnimePlayer,
    AnimeVariant,
}

impl SettingsField {
    pub const ALL: [SettingsField; 9] = [
        SettingsField::DefaultProvider,
        SettingsField::DownloadFormat,
        SettingsField::ChapterLanguage,
        SettingsField::UiLanguage,
        SettingsField::Theme,
        SettingsField::ImageProtocol,
        SettingsField::AnimeProvider,
        SettingsField::AnimePlayer,
        SettingsField::AnimeVariant,
    ];

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(SettingsField::DefaultProvider)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::AnimeProvider => tr("Proveedor de anime", "Anime provider"),
            Self::AnimePlayer => tr("Reproductor de anime", "Anime player"),
            Self::AnimeVariant => tr("Variante de anime", "Anime variant"),
            Self::DefaultProvider => tr("📡 Proveedor por defecto", "📡 Default Provider"),
            Self::DownloadFormat => tr("📦 Formato de descarga", "📦 Download Format"),
            Self::ChapterLanguage => tr("🌐 Idioma de capítulos", "🌐 Chapter Language"),
            Self::UiLanguage => tr("🗣 Idioma de la interfaz", "🗣 Interface Language"),
            Self::Theme => tr("🎨 Tema visual", "🎨 Visual Theme"),
            Self::ImageProtocol => tr("🖼  Protocolo gráfico", "🖼  Graphics Protocol"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::AnimeProvider => tr("Fuente preferida al abrir la pestaña Anime", "Preferred source when opening the Anime tab"),
            Self::AnimePlayer => tr("Reproductor web local, VLC o mpv", "Local web player, VLC or mpv"),
            Self::AnimeVariant => tr("Subtitulado o doblado según disponibilidad", "Subbed or dubbed when available"),
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
            Self::ImageProtocol => tr(
                "Protocolo de renderizado TUI (Auto, Sixel, Kitty, iTerm2 o solo Visor Web HD)",
                "Terminal graphics rendering protocol (Auto, Sixel, Kitty, iTerm2, or Web HD only)",
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
    ImageProtocolChanged(ImageProtocol),
    CheckForUpdates,
    OpenCloudSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsModalState {
    pub cursor: usize,
    pub status_message: Option<String>,
    pub status_is_error: bool,
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
            status_is_error: false,
        }
    }

    fn set_persist_status(&mut self, label: &str, value: &str, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.status_is_error = false;
                self.status_message =
                    Some(format!("✓ {label}: {value} ({})", tr("guardado en config.toml", "saved to config.toml")));
            },
            Err(err) => {
                self.status_is_error = true;
                self.status_message = Some(format!(
                    "⚠ {label}: {value} ({}: {err})",
                    tr("solo sesión actual, error al guardar", "session only, save failed")
                ));
            },
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
            field @ (SettingsField::AnimeProvider | SettingsField::AnimePlayer | SettingsField::AnimeVariant) => {
                let active = active_anime_config();
                let (setting, value) = match field {
                    SettingsField::AnimeProvider => {
                        let next = cycle_anime_choice(
                            [AnimeProviders::AnimeAv1, AnimeProviders::JkAnime],
                            active.preferred_provider,
                            forward,
                        );
                        (AnimeSetting::Provider(next), next.to_string())
                    },
                    SettingsField::AnimePlayer => {
                        let next =
                            cycle_anime_choice([AnimePlayer::Web, AnimePlayer::Vlc, AnimePlayer::Mpv], active.player, forward);
                        (AnimeSetting::Player(next), next.label().to_string())
                    },
                    SettingsField::AnimeVariant => {
                        let next = cycle_anime_choice([AnimeVariant::Sub, AnimeVariant::Dub], active.preferred_variant, forward);
                        (AnimeSetting::Variant(next), next.to_string())
                    },
                    _ => unreachable!(),
                };
                self.set_persist_status(field.title(), &value, apply_anime_setting(setting));
                SettingsOutcome::Noop
            },
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
                let res = persist_setting_to_disk("default_manga_provider", &next_provider.to_string());
                self.set_persist_status(
                    tr("Proveedor activo y por defecto", "Active & default provider"),
                    next_provider.badge_label(),
                    res,
                );
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
                let res = persist_setting_to_disk("download_type", download_type_toml_value(next_format));
                self.set_persist_status(tr("Formato de descarga", "Download format"), &next_format.to_string().to_uppercase(), res);
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
                let res = persist_setting_to_disk("default_language", &next_lang.to_string().to_lowercase());
                let display_val = format!("{} {}", next_lang.as_emoji(), next_lang.as_human_readable());
                self.set_persist_status(tr("Idioma de capítulos", "Chapter language"), &display_val, res);
                SettingsOutcome::ChapterLanguageChanged(next_lang)
            },
            SettingsField::UiLanguage => {
                let current = active_ui_language();
                let next_lang = match current {
                    UiLanguage::Es => UiLanguage::En,
                    UiLanguage::En => UiLanguage::Es,
                };
                set_active_ui_language(next_lang);
                let res = persist_setting_to_disk("ui_language", ui_lang_toml_value(next_lang));
                let label = match next_lang {
                    UiLanguage::Es => "🇪🇸 Español",
                    UiLanguage::En => "🇬🇧 English",
                };
                self.set_persist_status(tr("Idioma de la interfaz", "Interface language"), label, res);
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
                let res = persist_setting_to_disk("theme", theme_toml_value(next_theme));
                self.set_persist_status(tr("Tema visual", "Visual theme"), next_theme.label(), res);
                SettingsOutcome::ThemeChanged(next_theme)
            },
            SettingsField::ImageProtocol => {
                let current = active_image_protocol();
                let idx = ALL_IMAGE_PROTOCOLS.iter().position(|&p| p == current).unwrap_or(0);
                let next_idx = if forward {
                    (idx + 1) % ALL_IMAGE_PROTOCOLS.len()
                } else if idx == 0 {
                    ALL_IMAGE_PROTOCOLS.len() - 1
                } else {
                    idx - 1
                };
                let next_proto = ALL_IMAGE_PROTOCOLS[next_idx];
                set_active_image_protocol(next_proto);
                let res = persist_setting_to_disk("image_protocol", next_proto.toml_value());
                self.set_persist_status(tr("Protocolo gráfico", "Graphics protocol"), next_proto.label(), res);
                SettingsOutcome::ImageProtocolChanged(next_proto)
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
            KeyCode::Char('6') => {
                self.cursor = 5;
                SettingsOutcome::Noop
            },
            KeyCode::Char('o') | KeyCode::Char('O') => {
                let target_dir = crate::backend::AppDirectories::MangaDownloads.get_full_path();
                #[cfg(not(test))]
                let _ = open::that(&target_dir);
                self.status_is_error = false;
                self.status_message = Some(format!("📂 {}: {}", tr("Abriendo carpeta", "Opening folder"), target_dir.display()));
                SettingsOutcome::Noop
            },
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.status_is_error = false;
                self.status_message =
                    Some(tr("🔍 Buscando actualizaciones en GitHub...", "🔍 Checking GitHub for updates...").to_string());
                SettingsOutcome::CheckForUpdates
            },
            KeyCode::Char('g') | KeyCode::Char('G') | KeyCode::F(6) => SettingsOutcome::OpenCloudSync,
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::F(5) | KeyCode::Char('q') | KeyCode::Esc => SettingsOutcome::Close,
            _ => SettingsOutcome::Noop,
        }
    }
}

fn cycle_anime_choice<T: Copy + Eq, const N: usize>(choices: [T; N], current: T, forward: bool) -> T {
    let index = choices.iter().position(|choice| *choice == current).unwrap_or(0);
    choices[(index + if forward { 1 } else { N - 1 }) % N]
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
            let p = ThemePalette::for_variant(*variant);
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

    fn render_image_protocol_selector(is_focused: bool) -> Vec<Span<'static>> {
        let t = theme();
        let current = active_image_protocol();
        let idx = ALL_IMAGE_PROTOCOLS.iter().position(|&p| p == current).unwrap_or(0) + 1;
        let total = ALL_IMAGE_PROTOCOLS.len();

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
            Span::styled(format!(" {} ", current.label()), pill_style),
            Span::styled(" ▶", arrow_style),
            Span::styled(format!("  ({idx}/{total})"), Style::default().fg(t.text_muted)),
        ]
    }
}

impl<'a> Widget for SettingsModal<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let modal_area = centered_rect(area, 78, 86);
        Clear.render(modal_area, buf);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent))
            .title(Line::from(vec![Span::styled(
                tr(" ⚙  Menú de Ajustes Interactivos <c / F5> ", " ⚙  Interactive Settings Menu <c / F5> "),
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
            )]))
            .title_bottom(
                Line::from(vec![
                    Span::raw(" "),
                    t.key_chip("j/k · ↑/↓"),
                    Span::styled(tr(" Elegir  ", " Select  "), Style::default().fg(t.text_muted)),
                    t.highlight_key_chip("h/l · Enter"),
                    Span::styled(tr(" Cambiar  ", " Change  "), Style::default().fg(t.text_muted)),
                    t.key_chip("o"),
                    Span::styled(tr(" Carpeta  ", " Folder  "), Style::default().fg(t.text_muted)),
                    t.key_chip("g"),
                    Span::styled(tr(" Nube  ", " Cloud  "), Style::default().fg(t.text_muted)),
                    t.key_chip("u"),
                    Span::styled(tr(" Actualizar  ", " Update  "), Style::default().fg(t.text_muted)),
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

        let row_height = if rows_area.height >= (SettingsField::ALL.len() as u16) * 3 { 3 } else { 2 };
        let visible = usize::from((rows_area.height / row_height).max(1)).min(SettingsField::ALL.len());
        let offset = self.state.cursor.saturating_add(1).saturating_sub(visible);
        let constraints = vec![Constraint::Length(row_height); visible];
        let row_rects = Layout::default().direction(Direction::Vertical).constraints(constraints).split(rows_area);

        let anime = active_anime_config();
        for ((idx, field), &row_area) in SettingsField::ALL.iter().enumerate().skip(offset).zip(row_rects.iter()) {
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

            let title_width = if row_area.width >= 45 { 28 } else { 0 };
            let mut top_spans =
                vec![cursor_indicator, Span::styled(format!("{:<title_width$}", field.title()), label_style), Span::raw("  ")];

            let selector_spans = match field {
                SettingsField::DefaultProvider => Self::render_provider_selector(is_focused),
                SettingsField::DownloadFormat => Self::render_download_format_selector(is_focused),
                SettingsField::ChapterLanguage => Self::render_chapter_language_selector(is_focused),
                SettingsField::UiLanguage => Self::render_ui_language_selector(is_focused),
                SettingsField::Theme => Self::render_theme_selector(is_focused),
                SettingsField::ImageProtocol => Self::render_image_protocol_selector(is_focused),
                SettingsField::AnimeProvider => vec![Span::styled(anime.preferred_provider.to_string(), label_style)],
                SettingsField::AnimePlayer => vec![Span::styled(anime.player.label(), label_style)],
                SettingsField::AnimeVariant => vec![Span::styled(anime.preferred_variant.to_string(), label_style)],
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

        let storage_mode = crate::backend::current_storage_mode().label();
        let footer_line = if let Some(msg) = &self.state.status_message {
            let msg_color = if self.state.status_is_error { t.status_warning } else { t.status_success };
            Line::from(vec![Span::styled(msg.as_str(), Style::default().fg(msg_color).add_modifier(Modifier::BOLD))])
        } else {
            Line::from(vec![Span::styled(
                format!(
                    "📡 {}  │  📦 {}  │  🌐 {}  │  🎨 {}  │  🖼  {}  │  💾 {}",
                    active_default_provider(),
                    active_download_type().to_string().to_uppercase(),
                    active_chapter_language(),
                    current_theme().label(),
                    active_image_protocol().label(),
                    storage_mode,
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
    fn test_settings_modal_navigation_and_cycling() {
        set_active_default_provider(MangaProviders::Mangadex);
        set_active_download_type(DownloadType::Cbz);
        set_active_chapter_language(Languages::Spanish);
        set_active_ui_language(UiLanguage::Es);
        set_theme(ThemeVariant::TokyoNight);
        set_active_image_protocol(ImageProtocol::Auto);

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

        // Move down to ImageProtocol and cycle: Auto -> Sixel -> Kitty
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::ImageProtocol);
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::ImageProtocolChanged(ImageProtocol::Sixel));
        assert_eq!(active_image_protocol(), ImageProtocol::Sixel);
        assert_eq!(state.handle_key_event(KeyCode::Right.into()), SettingsOutcome::ImageProtocolChanged(ImageProtocol::Kitty));
        assert_eq!(active_image_protocol(), ImageProtocol::Kitty);

        // Closing with 'c' or F5
        assert_eq!(state.handle_key_event(KeyCode::Char('c').into()), SettingsOutcome::Close);
        assert_eq!(state.handle_key_event(KeyCode::F(5).into()), SettingsOutcome::Close);

        // Reset defaults for other tests
        set_active_default_provider(MangaProviders::Mangadex);
        set_active_download_type(DownloadType::Cbz);
        set_active_chapter_language(Languages::English);
        set_active_ui_language(UiLanguage::Es);
        set_theme(ThemeVariant::TokyoNight);
        set_active_image_protocol(ImageProtocol::Auto);
    }

    #[test]
    fn anime_settings_can_be_changed_in_a_short_terminal() {
        let mut state = SettingsModalState {
            cursor: SettingsField::AnimeProvider as usize,
            ..Default::default()
        };
        apply_anime_setting(AnimeSetting::Provider(AnimeProviders::AnimeAv1)).unwrap();
        apply_anime_setting(AnimeSetting::Player(AnimePlayer::Web)).unwrap();
        apply_anime_setting(AnimeSetting::Variant(AnimeVariant::Sub)).unwrap();
        assert_eq!(state.current_field(), SettingsField::AnimeProvider);
        state.handle_key_event(KeyCode::Right.into());
        assert_eq!(active_anime_config().preferred_provider, AnimeProviders::JkAnime);
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::AnimePlayer);
        state.handle_key_event(KeyCode::Left.into());
        assert_eq!(active_anime_config().player, AnimePlayer::Mpv);
        state.handle_key_event(KeyCode::Down.into());
        assert_eq!(state.current_field(), SettingsField::AnimeVariant);
        state.handle_key_event(KeyCode::Enter.into());
        assert_eq!(active_anime_config().preferred_variant, AnimeVariant::Dub);

        let area = Rect::new(0, 0, 60, 18);
        let mut buffer = Buffer::empty(area);
        SettingsModal::new(&state).render(area, &mut buffer);
        assert!(
            buffer
                .content
                .chunks(60)
                .any(|row| { row.iter().map(|cell| cell.symbol()).collect::<String>().contains("  DUB") })
        );

        apply_anime_setting(AnimeSetting::Provider(AnimeProviders::AnimeAv1)).unwrap();
        apply_anime_setting(AnimeSetting::Player(AnimePlayer::Web)).unwrap();
        apply_anime_setting(AnimeSetting::Variant(AnimeVariant::Sub)).unwrap();
    }

    #[test]
    fn test_settings_modal_folder_and_release_shortcuts() {
        let mut state = SettingsModalState::new();

        assert_eq!(state.handle_key_event(KeyCode::Char('o').into()), SettingsOutcome::Noop);
        assert!(state.status_message.as_deref().is_some_and(|m| m.contains("mangaDownloads")));

        assert_eq!(state.handle_key_event(KeyCode::Char('u').into()), SettingsOutcome::CheckForUpdates);
        assert!(state.status_message.as_deref().is_some_and(|m| m.contains("GitHub")));
    }

    #[test]
    fn test_settings_modal_persist_status_reports_errors_honestly() {
        let mut state = SettingsModalState::new();
        state.set_persist_status("Tema visual", "Catppuccin", Err("permission denied".into()));

        assert!(state.status_is_error);
        let msg = state.status_message.as_deref().unwrap();
        assert!(!msg.contains("guardado en config.toml"));
        assert!(!msg.contains("saved to config.toml"));
        assert!(msg.contains("permission denied"));

        state.set_persist_status("Tema visual", "Catppuccin", Ok(()));
        assert!(!state.status_is_error);
        let ok_msg = state.status_message.as_deref().unwrap();
        assert!(ok_msg.contains("guardado en config.toml") || ok_msg.contains("saved to config.toml"));
    }
}
