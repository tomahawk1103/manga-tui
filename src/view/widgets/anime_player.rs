use std::io::Cursor;

use crossterm::event::{KeyCode, KeyEvent};
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use crate::common::render_stateful_image_cleared;
use crate::i18n::tr;
use crate::theme::theme;

pub const ANIME_SCENE_COUNT: usize = 3;

const FRIEREN_BREEZE_GIF: &[u8] = include_bytes!("../../../assets/frieren/frieren_breeze.gif");
const FRIEREN_SNOW_GIF: &[u8] = include_bytes!("../../../assets/frieren/frieren_snow.gif");
const FRIEREN_COZY_GIF: &[u8] = include_bytes!("../../../assets/frieren/frieren_cozy.gif");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimeSceneKind {
    /// Real anime clip: Frieren looking up into the winter breeze (`frieren_breeze.gif`)
    #[default]
    CyberpunkSakura = 0,
    /// Real anime clip: Frieren in the falling snow with her blue scarf (`frieren_snow.gif`)
    CrimsonMoonTorii = 1,
    /// Real anime clip: Cozy sleepy Frieren nodding in her scarf (`frieren_cozy.gif`)
    SpiritForestAurora = 2,
}

impl AnimeSceneKind {
    pub fn from_index(index: usize) -> Self {
        match index % ANIME_SCENE_COUNT {
            0 => Self::CyberpunkSakura,
            1 => Self::CrimsonMoonTorii,
            _ => Self::SpiritForestAurora,
        }
    }

    pub fn next(self) -> Self {
        Self::from_index(self as usize + 1)
    }

    pub fn previous(self) -> Self {
        Self::from_index((self as usize + ANIME_SCENE_COUNT - 1) % ANIME_SCENE_COUNT)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::CyberpunkSakura => tr("Frieren · Brisa de Invierno (Anime Clip)", "Frieren · Winter Breeze (Anime Clip)"),
            Self::CrimsonMoonTorii => tr("Frieren · Bajo la Nieve (Anime Clip)", "Frieren · Under the Snow (Anime Clip)"),
            Self::SpiritForestAurora => tr("Frieren · Siesta Acogedora (Anime Clip)", "Frieren · Cozy Nap (Anime Clip)"),
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::CyberpunkSakura => "🧝‍♀️ FRIEREN · CLIP 1",
            Self::CrimsonMoonTorii => "❄️ FRIEREN · CLIP 2",
            Self::SpiritForestAurora => "💤 FRIEREN · CLIP 3",
        }
    }

    fn gif_bytes(self) -> &'static [u8] {
        match self {
            Self::CyberpunkSakura => FRIEREN_BREEZE_GIF,
            Self::CrimsonMoonTorii => FRIEREN_SNOW_GIF,
            Self::SpiritForestAurora => FRIEREN_COZY_GIF,
        }
    }
}

/// Decodes all animation frames from the embedded real anime GIF for `scene`.
pub fn decode_anime_gif_frames(scene: AnimeSceneKind) -> Vec<DynamicImage> {
    let cursor = Cursor::new(scene.gif_bytes());
    if let Ok(decoder) = GifDecoder::new(cursor) {
        let frames: Vec<DynamicImage> = decoder
            .into_frames()
            .filter_map(|f| f.ok())
            .map(|frame| DynamicImage::ImageRgba8(frame.into_buffer()))
            .collect();
        if !frames.is_empty() {
            return frames;
        }
    }
    vec![DynamicImage::new_rgba8(64, 64)]
}

pub struct AnimePlayerState {
    pub scene: AnimeSceneKind,
    pub current_frame: usize,
    pub is_paused: bool,
    pub is_fullscreen_open: bool,
    frames_raw: Vec<DynamicImage>,
    hero_protocols: Vec<Option<Box<dyn StatefulProtocol>>>,
    modal_protocols: Vec<Option<Box<dyn StatefulProtocol>>>,
}

impl Default for AnimePlayerState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimePlayerState {
    pub fn new() -> Self {
        let scene = AnimeSceneKind::default();
        let frames_raw = decode_anime_gif_frames(scene);
        let count = frames_raw.len().max(1);
        let hero_protocols = (0..count).map(|_| None).collect();
        let modal_protocols = (0..count).map(|_| None).collect();
        Self {
            scene,
            current_frame: 0,
            is_paused: false,
            is_fullscreen_open: false,
            frames_raw,
            hero_protocols,
            modal_protocols,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.frames_raw.len().max(1)
    }

    pub fn set_scene(&mut self, scene: AnimeSceneKind) {
        if self.scene != scene {
            self.scene = scene;
            self.current_frame = 0;
            self.frames_raw = decode_anime_gif_frames(scene);
            self.invalidate_protocols();
        }
    }

    pub fn next_scene(&mut self) {
        self.set_scene(self.scene.next());
    }

    pub fn previous_scene(&mut self) {
        self.set_scene(self.scene.previous());
    }

    pub fn toggle_pause(&mut self) {
        self.is_paused = !self.is_paused;
    }

    pub fn toggle_fullscreen(&mut self) {
        self.is_fullscreen_open = !self.is_fullscreen_open;
    }

    pub fn tick(&mut self) {
        if !self.is_paused {
            let total = self.frame_count();
            self.current_frame = (self.current_frame + 1) % total;
        }
    }

    pub fn invalidate_protocols(&mut self) {
        let count = self.frame_count();
        self.hero_protocols = (0..count).map(|_| None).collect();
        self.modal_protocols = (0..count).map(|_| None).collect();
    }

    /// Renders the current Frieren anime frame into the Home Hero banner using high-res `Picker` (Sixel / Kitty / iTerm2) when
    /// available.
    pub fn render_hero_canvas(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>) {
        self.render_with_slot(area, buf, picker, false);
    }

    /// Renders the current Frieren anime frame into the fullscreen modal using high-res `Picker` (Sixel / Kitty / iTerm2) when
    /// available.
    pub fn render_canvas(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>) {
        self.render_with_slot(area, buf, picker, true);
    }

    fn render_with_slot(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>, is_modal: bool) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let total = self.frame_count();
        let frame_idx = self.current_frame % total;

        if let Some(picker) = picker {
            let need_init = if is_modal {
                self.modal_protocols.get(frame_idx).and_then(|p| p.as_ref()).is_none()
            } else {
                self.hero_protocols.get(frame_idx).and_then(|p| p.as_ref()).is_none()
            };
            if need_init && let Some(raw) = self.frames_raw.get(frame_idx) {
                let proto = picker.new_resize_protocol(raw.clone());
                let protocols = if is_modal { &mut self.modal_protocols } else { &mut self.hero_protocols };
                if let Some(slot) = protocols.get_mut(frame_idx) {
                    *slot = Some(proto);
                }
            }
            let protocols = if is_modal { &mut self.modal_protocols } else { &mut self.hero_protocols };
            if let Some(Some(protocol)) = protocols.get_mut(frame_idx) {
                render_stateful_image_cleared(protocol, area, area, buf, Color::Rgb(14, 12, 32));
                return;
            }
        }

        // Universal truecolor halfblock fallback (`▀` packing 2 vertical pixels per terminal cell)
        self.render_halfblock_fallback(area, buf, frame_idx);
    }

    fn render_halfblock_fallback(&self, area: Rect, buf: &mut Buffer, frame_idx: usize) {
        let Some(DynamicImage::ImageRgba8(img)) = self.frames_raw.get(frame_idx) else {
            return;
        };
        let img_w = img.width().max(1);
        let img_h = img.height().max(1);

        for row in 0..area.height {
            let y0 = ((row as u32 * 2) * img_h / (area.height as u32 * 2).max(1)).min(img_h - 1);
            let y1 = (((row as u32 * 2 + 1) * img_h) / (area.height as u32 * 2).max(1)).min(img_h - 1);
            for col in 0..area.width {
                let x = (col as u32 * img_w / (area.width as u32).max(1)).min(img_w - 1);
                let top = img.get_pixel(x, y0).0;
                let bot = img.get_pixel(x, y1).0;
                let cell = &mut buf[(area.x + col, area.y + row)];
                cell.reset();
                cell.set_symbol("▀")
                    .set_fg(Color::Rgb(top[0], top[1], top[2]))
                    .set_bg(Color::Rgb(bot[0], bot[1], bot[2]));
            }
        }
    }
}

pub struct AnimeVisualModal<'a> {
    state: &'a mut AnimePlayerState,
    picker: Option<&'a mut Picker>,
}

impl<'a> AnimeVisualModal<'a> {
    pub fn new(state: &'a mut AnimePlayerState, picker: Option<&'a mut Picker>) -> Self {
        Self { state, picker }
    }

    pub fn render(self, area: Rect, buf: &mut Buffer) {
        let t = theme();
        let modal_w = (area.width.saturating_mul(78) / 100).clamp(48.min(area.width), area.width.saturating_sub(2).max(1));
        let modal_h = (area.height.saturating_mul(84) / 100).clamp(16.min(area.height), area.height.saturating_sub(2).max(1));
        let x = area.x + area.width.saturating_sub(modal_w) / 2;
        let y = area.y + area.height.saturating_sub(modal_h) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        Clear.render(modal_area, buf);

        let status_badge =
            if self.state.is_paused { tr(" ⏸ PAUSADO ", " ⏸ PAUSED ") } else { tr(" ▶ ANIME GIF ", " ▶ ANIME GIF ") };

        let title_left = Line::from(vec![
            Span::styled(
                tr(" 🎬 Sousou no Frieren ", " 🎬 Frieren: Beyond Journey's End "),
                Style::default().fg(t.accent_primary).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("· {} ", self.state.scene.title()), Style::default().fg(t.text_primary).bold()),
        ]);

        let title_right = Line::from(vec![
            Span::styled(
                status_badge,
                Style::default()
                    .fg(Color::Black)
                    .bg(if self.state.is_paused { t.status_warning } else { t.status_success })
                    .bold(),
            ),
            Span::raw(" "),
            Span::styled(
                format!(" {}/{} ", self.state.scene as usize + 1, ANIME_SCENE_COUNT),
                Style::default().fg(t.accent_highlight).bg(t.chip_bg).bold(),
            ),
            Span::raw(" "),
        ])
        .right_aligned();

        let instructions = Line::from(vec![
            Span::raw(" "),
            t.key_chip("←/→ / v"),
            Span::styled(tr(" Cambiar clip de Frieren  ", " Switch Frieren clip  "), Style::default().fg(t.text_muted)),
            t.key_chip("Espacio"),
            Span::styled(tr(" Pausar/Reanudar  ", " Pause/Resume  "), Style::default().fg(t.text_muted)),
            t.action_chip_primary("V / Esc / q"),
            Span::styled(tr(" Cerrar ", " Close "), Style::default().fg(t.text_primary).bold()),
        ])
        .alignment(Alignment::Center);

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(t.border_accent).bold())
            .title(title_left)
            .title_top(title_right)
            .title_bottom(instructions);

        let inner = block.inner(modal_area).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        block.render(modal_area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let [canvas_area, footer_area] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);
        self.state.render_canvas(canvas_area, buf, self.picker);

        let total = self.state.frame_count();
        let progress_dots: Vec<Span<'static>> = (0..total)
            .map(|idx| {
                if idx == self.state.current_frame {
                    Span::styled("━", Style::default().fg(t.accent_primary).bold())
                } else {
                    Span::styled("─", Style::default().fg(t.border_muted))
                }
            })
            .collect();
        Paragraph::new(Line::from(progress_dots))
            .alignment(Alignment::Center)
            .render(footer_area, buf);
    }
}

pub fn handle_anime_modal_key(state: &mut AnimePlayerState, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Char('V') | KeyCode::Esc => {
            state.is_fullscreen_open = false;
            true
        },
        KeyCode::Char(' ') | KeyCode::Char('p') => {
            state.toggle_pause();
            false
        },
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('v') | KeyCode::Tab => {
            state.next_scene();
            false
        },
        KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => {
            state.previous_scene();
            false
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anime_scene_cycles_and_decodes_real_frieren_gif_frames() {
        let mut state = AnimePlayerState::new();
        assert_eq!(state.scene, AnimeSceneKind::CyberpunkSakura);
        assert!(state.frame_count() > 5, "expected real multi-frame animated GIF of Frieren");
        assert_eq!(state.current_frame, 0);

        state.tick();
        assert_eq!(state.current_frame, 1);

        state.toggle_pause();
        state.tick();
        assert_eq!(state.current_frame, 1, "paused player must not advance frame");

        state.toggle_pause();
        state.next_scene();
        assert_eq!(state.scene, AnimeSceneKind::CrimsonMoonTorii);
        assert!(state.frame_count() > 5);
        assert_eq!(state.current_frame, 0);

        state.next_scene();
        assert_eq!(state.scene, AnimeSceneKind::SpiritForestAurora);
        assert!(state.frame_count() > 3);

        state.previous_scene();
        assert_eq!(state.scene, AnimeSceneKind::CrimsonMoonTorii);
    }

    #[test]
    fn anime_player_renders_halfblock_and_modal_cleanly() {
        let mut state = AnimePlayerState::new();
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);

        AnimeVisualModal::new(&mut state, None).render(area, &mut buf);

        let rendered: String = (0..area.height)
            .map(|y| (0..area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains('╭'), "modal should use rounded borders");
        assert!(rendered.contains('▀'), "halfblock fallback should render truecolor pixel blocks");
        assert!(rendered.contains("Frieren"), "modal title should display Frieren");
    }
}
