use crossterm::event::{KeyCode, KeyEvent};
use image::{DynamicImage, Rgba, RgbaImage};
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
pub const ANIME_FRAME_COUNT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimeSceneKind {
    /// Neo-Tokyo Cyberpunk Rooftop with shooting stars, neon skyline, and anime silhouette
    #[default]
    CyberpunkSakura = 0,
    /// Crimson Blood Moon Torii Gate over reflective ocean ripples and floating lanterns
    CrimsonMoonTorii = 1,
    /// Emerald Spirit Forest with bioluminescent fireflies, glowing shrine, and aurora sky
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
            Self::CyberpunkSakura => tr("Neo-Tokyo · Lluvia de Sakura y Neón", "Neo-Tokyo · Sakura & Neon Rain"),
            Self::CrimsonMoonTorii => tr("Torii Carmesí · Luna de Sangre", "Crimson Torii · Blood Moon"),
            Self::SpiritForestAurora => tr("Bosque Espiritual · Aurora y Luciérnagas", "Spirit Forest · Aurora & Fireflies"),
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::CyberpunkSakura => "🌸 NEO-TOKYO",
            Self::CrimsonMoonTorii => "⛩  CRIMSON MOON",
            Self::SpiritForestAurora => "✨ SPIRIT FOREST",
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

fn blend_rgba(base: [u8; 4], over: [u8; 4], alpha: f32) -> [u8; 4] {
    let a = alpha.clamp(0.0, 1.0);
    [lerp_u8(base[0], over[0], a), lerp_u8(base[1], over[1], a), lerp_u8(base[2], over[2], a), 255]
}

/// Generates a full-color procedural anime pixel-art frame (`DynamicImage`) for the given scene and frame index.
pub fn generate_anime_frame(scene: AnimeSceneKind, frame_idx: usize, width: u32, height: u32) -> DynamicImage {
    let w = width.max(16);
    let h = height.max(16);
    let mut img = RgbaImage::new(w, h);
    let phase = (frame_idx % ANIME_FRAME_COUNT) as f32 / ANIME_FRAME_COUNT as f32;
    let tau = std::f32::consts::TAU;
    let angle = phase * tau;

    for y in 0..h {
        let ny = y as f32 / (h.saturating_sub(1).max(1)) as f32;
        for x in 0..w {
            let nx = x as f32 / (w.saturating_sub(1).max(1)) as f32;
            let px = match scene {
                AnimeSceneKind::CyberpunkSakura => render_cyberpunk_pixel(nx, ny, phase, angle),
                AnimeSceneKind::CrimsonMoonTorii => render_crimson_torii_pixel(nx, ny, phase, angle),
                AnimeSceneKind::SpiritForestAurora => render_spirit_forest_pixel(nx, ny, phase, angle),
            };
            img.put_pixel(x, y, Rgba(px));
        }
    }

    DynamicImage::ImageRgba8(img)
}

fn render_cyberpunk_pixel(nx: f32, ny: f32, phase: f32, angle: f32) -> [u8; 4] {
    // 1. Deep night-to-magenta cyberpunk sky gradient
    let mut col = if ny < 0.55 {
        let t = ny / 0.55;
        [lerp_u8(12, 68, t), lerp_u8(10, 24, t), lerp_u8(38, 96, t), 255]
    } else {
        let t = (ny - 0.55) / 0.45;
        [lerp_u8(68, 195, t), lerp_u8(24, 52, t), lerp_u8(96, 132, t), 255]
    };

    // 2. Glowing crescent anime moon in upper-right
    let mx = (nx - 0.76) * 1.45;
    let my = ny - 0.24;
    let dist_moon = (mx * mx + my * my).sqrt();
    let shadow_dist = ((mx + 0.055) * (mx + 0.055) + (my - 0.02) * (my - 0.02)).sqrt();
    if dist_moon < 0.14 && shadow_dist > 0.12 {
        let pulse = 0.92 + 0.08 * angle.sin();
        col = blend_rgba(col, [255, 240, 200, 255], pulse);
    } else if dist_moon < 0.24 {
        let glow = ((0.24 - dist_moon) / 0.24).powi(2) * 0.45;
        col = blend_rgba(col, [255, 120, 210, 255], glow);
    }

    // 3. Shooting star / comet across the sky
    let comet_head_x = 0.92 - phase * 0.85;
    let comet_head_y = 0.10 + phase * 0.28;
    let dx = nx - comet_head_x;
    let dy = ny - comet_head_y;
    let along = dx * 0.88 - dy * 0.48;
    let perp = (dx * 0.48 + dy * 0.88).abs();
    if (0.0..0.22).contains(&along) && perp < 0.016 {
        let intensity = (1.0 - along / 0.22) * (1.0 - perp / 0.016);
        col = blend_rgba(col, [140, 245, 255, 255], intensity);
    }

    // 4. Twinkling stars in upper sky
    if ny < 0.52 {
        let cell_x = (nx * 28.0).floor();
        let cell_y = (ny * 18.0).floor();
        let hash = ((cell_x * 12.9898 + cell_y * 78.233).sin() * 43758.547).fract().abs();
        if hash > 0.87 {
            let cx = (cell_x + 0.5) / 28.0;
            let cy = (cell_y + 0.5) / 18.0;
            if (nx - cx).abs() < 0.014 && (ny - cy).abs() < 0.022 {
                let twinkle = 0.45 + 0.55 * (angle + hash * 6.28).sin().abs();
                col = blend_rgba(col, [235, 250, 255, 255], twinkle);
            }
        }
    }

    // 5. Distant Neo-Tokyo city skyline with animated neon windows
    let tower_idx = (nx * 14.0).floor() as i32;
    let tower_hash = ((tower_idx as f32 * 41.3).sin() * 197.3).fract().abs();
    let roof_y = 0.50 + (tower_hash * 0.22);
    if ny >= roof_y && ny < 0.86 {
        col = [16, 14, 34, 255];
        // Neon windows
        let win_x = (nx * 56.0).fract();
        let win_y = (ny * 36.0).fract();
        if win_x > 0.25 && win_x < 0.75 && win_y > 0.25 && win_y < 0.75 {
            let win_id = (nx * 56.0).floor() + (ny * 36.0).floor() * 17.0;
            let blink = ((win_id * 1.7 + phase * 6.28).sin()) > 0.15;
            if blink {
                let is_cyan = (win_id as i32) % 2 == 0;
                let neon = if is_cyan { [0, 230, 255, 255] } else { [255, 75, 195, 255] };
                col = blend_rgba(col, neon, 0.82);
            }
        }
    }

    // 6. Foreground rooftop & anime character silhouette with flowing scarf
    if ny >= 0.84 {
        col = [10, 9, 22, 255];
        if (ny - 0.84).abs() < 0.015 {
            col = blend_rgba(col, [0, 225, 255, 255], 0.7);
        }
    }
    // Character head & torso on left rooftop
    let head_dist = ((nx - 0.24).powi(2) * 1.6 + (ny - 0.69).powi(2)).sqrt();
    let in_torso = (0.20..0.28).contains(&nx) && (0.72..0.85).contains(&ny) && (nx - 0.24).abs() < (ny - 0.68) * 0.35;
    // Flowing anime scarf waving in the wind
    let scarf_wave = (nx * 18.0 - angle * 2.0).sin() * 0.022;
    let in_scarf = (0.25..0.44).contains(&nx) && (ny - (0.73 + scarf_wave)).abs() < 0.022;
    if head_dist < 0.038 || in_torso {
        col = [14, 12, 28, 255];
    } else if in_scarf {
        col = [255, 62, 145, 255];
    }

    // 7. Drifting sakura petals across the scene
    for i in 0..14 {
        let seed = i as f32 * 1.37;
        let px = (seed * 0.73 + phase * 0.65 + (seed + angle).sin() * 0.04).rem_euclid(1.0);
        let py = (seed * 0.51 + phase).rem_euclid(1.0);
        if (nx - px).abs() < 0.018 && (ny - py).abs() < 0.024 {
            col = blend_rgba(col, [255, 165, 215, 255], 0.92);
        }
    }

    col
}

fn render_crimson_torii_pixel(nx: f32, ny: f32, phase: f32, angle: f32) -> [u8; 4] {
    let horizon = 0.64;

    // 1. Twilight crimson-gold sky and reflective ocean water
    let mut col = if ny < horizon {
        let t = ny / horizon;
        [lerp_u8(24, 185, t), lerp_u8(10, 38, t), lerp_u8(36, 62, t), 255]
    } else {
        let t = (ny - horizon) / (1.0 - horizon);
        let wave = ((ny * 48.0 - angle * 2.0) + (nx * 20.0).sin()).sin();
        let mut water = [lerp_u8(120, 18, t), lerp_u8(24, 10, t), lerp_u8(48, 28, t), 255];
        // Shimmering crimson sun reflection on water
        let center_dist = (nx - 0.5).abs();
        let reflection_width = 0.16 * (1.0 - t * 0.55);
        if center_dist < reflection_width && wave > 0.15 {
            water = blend_rgba(water, [255, 195, 110, 255], 0.75 * (1.0 - center_dist / reflection_width));
        }
        water
    };

    // 2. Giant Crimson Blood Moon rising behind the Torii gate
    let mx = (nx - 0.5) * 1.35;
    let my = ny - 0.42;
    let moon_r = (mx * mx + my * my).sqrt();
    let pulse = 0.21 + 0.008 * angle.sin();
    if ny < horizon && moon_r < pulse {
        let t = (my + pulse) / (2.0 * pulse);
        let moon_col = [255, lerp_u8(230, 95, t), lerp_u8(155, 70, t), 255];
        col = blend_rgba(col, moon_col, 0.95);
    } else if ny < horizon && moon_r < pulse + 0.12 {
        let halo = (1.0 - (moon_r - pulse) / 0.12).powi(2) * 0.5;
        col = blend_rgba(col, [255, 90, 75, 255], halo);
    }

    // 3. Traditional Japanese Torii Gate silhouette in center
    let top_beam = (0.26..0.74).contains(&nx) && (0.25..0.29).contains(&(ny - (nx - 0.5).powi(2) * 0.25));
    let sub_beam = (0.30..0.70).contains(&nx) && (0.34..0.37).contains(&ny);
    let left_pillar = (0.35..0.385).contains(&nx) && (0.28..0.76).contains(&ny);
    let right_pillar = (0.615..0.65).contains(&nx) && (0.28..0.76).contains(&ny);
    let center_strut = (0.488..0.512).contains(&nx) && (0.28..0.36).contains(&ny);

    if top_beam || sub_beam || left_pillar || right_pillar || center_strut {
        col = [22, 8, 14, 255];
        // Rim light from the moon
        if (nx - 0.385).abs() < 0.008 || (nx - 0.615).abs() < 0.008 {
            col = [220, 65, 55, 255];
        }
    }

    // 4. Floating spirit embers / lanterns rising into the sky
    for i in 0..10 {
        let seed = i as f32 * 1.91;
        let lx = (0.15 + seed * 0.37 + (angle + seed).sin() * 0.03).rem_euclid(0.75) + 0.12;
        let ly = (1.0 - (phase + seed * 0.29).rem_euclid(1.0)) * 0.85;
        let d = ((nx - lx).powi(2) + (ny - ly).powi(2)).sqrt();
        if d < 0.025 {
            col = blend_rgba(col, [255, 220, 120, 255], 1.0 - d / 0.025);
        }
    }

    col
}

fn render_spirit_forest_pixel(nx: f32, ny: f32, phase: f32, angle: f32) -> [u8; 4] {
    // 1. Deep emerald-midnight forest sky
    let mut col = [lerp_u8(8, 16, ny), lerp_u8(20, 52, ny), lerp_u8(36, 58, ny), 255];

    // 2. Animated Aurora Borealis ribbons waving across upper sky
    if ny < 0.62 {
        let wave1 = (nx * 7.5 + angle).sin() * 0.08 + (nx * 13.0 - angle * 2.0).cos() * 0.04;
        let ribbon_y = 0.26 + wave1;
        let dist = (ny - ribbon_y).abs();
        if dist < 0.16 {
            let intensity = (1.0 - dist / 0.16).powi(2) * 0.72;
            let aurora_col = [lerp_u8(40, 150, nx), 245, lerp_u8(165, 255, (nx + phase).fract()), 255];
            col = blend_rgba(col, aurora_col, intensity);
        }
    }

    // 3. Layered parallax pine / bamboo forest silhouettes
    for layer in 0..3 {
        let lf = layer as f32;
        let freq = 9.0 + lf * 6.0;
        let tree_wave = ((nx * freq).sin().abs() * 0.18) + ((nx * freq * 2.3).cos().abs() * 0.08);
        let base_y = 0.56 + lf * 0.12;
        if ny > base_y + tree_wave * (0.6 - lf * 0.12) {
            col = [6 + layer * 3, 18 + layer * 8, 24 + layer * 6, 255];
        }
    }

    // 4. Bioluminescent anime fireflies / spirit orbs pulsing gently
    for i in 0..16 {
        let seed = i as f32 * 2.13;
        let fx = (seed * 0.61 + (angle + seed).sin() * 0.07).rem_euclid(1.0);
        let fy = (seed * 0.43 + (angle * 0.5 + seed).cos() * 0.06).rem_euclid(0.85) + 0.10;
        let d = ((nx - fx).powi(2) * 1.4 + (ny - fy).powi(2)).sqrt();
        let glow_radius = 0.034 * (0.65 + 0.35 * (angle * 2.0 + seed).sin());
        if d < glow_radius {
            let alpha = (1.0 - d / glow_radius).powi(2);
            let orb_col = if i % 3 == 0 { [120, 255, 220, 255] } else { [200, 255, 110, 255] };
            col = blend_rgba(col, orb_col, alpha);
        }
    }

    col
}

pub struct AnimePlayerState {
    pub scene: AnimeSceneKind,
    pub current_frame: usize,
    pub is_paused: bool,
    pub is_fullscreen_open: bool,
    frames_raw: Vec<DynamicImage>,
    frames_protocols: Vec<Option<Box<dyn StatefulProtocol>>>,
}

impl Default for AnimePlayerState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimePlayerState {
    pub fn new() -> Self {
        let scene = AnimeSceneKind::default();
        let frames_raw = Self::build_raw_frames(scene);
        let frames_protocols = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
        Self {
            scene,
            current_frame: 0,
            is_paused: false,
            is_fullscreen_open: false,
            frames_raw,
            frames_protocols,
        }
    }

    fn build_raw_frames(scene: AnimeSceneKind) -> Vec<DynamicImage> {
        (0..ANIME_FRAME_COUNT).map(|idx| generate_anime_frame(scene, idx, 96, 48)).collect()
    }

    pub fn set_scene(&mut self, scene: AnimeSceneKind) {
        if self.scene != scene {
            self.scene = scene;
            self.current_frame = 0;
            self.frames_raw = Self::build_raw_frames(scene);
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
            self.current_frame = (self.current_frame + 1) % ANIME_FRAME_COUNT;
        }
    }

    pub fn invalidate_protocols(&mut self) {
        self.frames_protocols = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
    }

    /// Renders the current anime frame into `area`.
    /// Uses `Picker` (Sixel / Kitty / iTerm2 / Halfblocks) when available, or a native halfblock `▀` fallback so it works in 100%
    /// of terminals.
    pub fn render_canvas(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let frame_idx = self.current_frame % ANIME_FRAME_COUNT;

        if let Some(picker) = picker {
            if self.frames_protocols.get(frame_idx).and_then(|p| p.as_ref()).is_none() {
                if let Some(raw) = self.frames_raw.get(frame_idx) {
                    let proto = picker.new_resize_protocol(raw.clone());
                    if let Some(slot) = self.frames_protocols.get_mut(frame_idx) {
                        *slot = Some(proto);
                    }
                }
            }
            if let Some(Some(protocol)) = self.frames_protocols.get_mut(frame_idx) {
                render_stateful_image_cleared(protocol, area, area, buf, Color::Rgb(8, 10, 22));
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
        let modal_w = (area.width.saturating_mul(84) / 100).clamp(48.min(area.width), area.width.saturating_sub(2).max(1));
        let modal_h = (area.height.saturating_mul(82) / 100).clamp(16.min(area.height), area.height.saturating_sub(2).max(1));
        let x = area.x + area.width.saturating_sub(modal_w) / 2;
        let y = area.y + area.height.saturating_sub(modal_h) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        Clear.render(modal_area, buf);

        let status_badge =
            if self.state.is_paused { tr(" ⏸ PAUSADO ", " ⏸ PAUSED ") } else { tr(" ▶ EN VIVO ", " ▶ LIVE ") };

        let title_left = Line::from(vec![
            Span::styled(
                tr(" 🎬 Escena Visual Anime ", " 🎬 Anime Visual Scene "),
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
            Span::styled(tr(" Cambiar escena  ", " Switch scene  "), Style::default().fg(t.text_muted)),
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

        // Progress bar & scene indicator
        let progress_dots: Vec<Span<'static>> = (0..ANIME_FRAME_COUNT)
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
    fn anime_scene_cycles_and_generates_frames_cleanly() {
        let mut state = AnimePlayerState::new();
        assert_eq!(state.scene, AnimeSceneKind::CyberpunkSakura);
        assert_eq!(state.current_frame, 0);

        state.tick();
        assert_eq!(state.current_frame, 1);

        state.toggle_pause();
        state.tick();
        assert_eq!(state.current_frame, 1, "paused player must not advance frame");

        state.toggle_pause();
        state.next_scene();
        assert_eq!(state.scene, AnimeSceneKind::CrimsonMoonTorii);
        assert_eq!(state.current_frame, 0);

        state.next_scene();
        assert_eq!(state.scene, AnimeSceneKind::SpiritForestAurora);

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
        assert!(rendered.contains("Neo-Tokyo"), "modal title should display current scene title");
    }
}
