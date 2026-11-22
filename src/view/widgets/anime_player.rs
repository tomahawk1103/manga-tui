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
pub const ANIME_FRAME_COUNT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimeSceneKind {
    /// Sakura Cyber-Idol Girl: Twin-tail cyan/pink anime character with big expressive sparkling eyes, breathing, blinking, hair
    /// swaying, and sakura petals
    #[default]
    CyberpunkSakura = 0,
    /// Crimson Kitsune Ronin: White-haired fox-spirit anime character with glowing crimson eyes, fox ears, katana aura, and
    /// floating embers
    CrimsonMoonTorii = 1,
    /// Astral Spirit Mage: Silver/lavender anime heroine with golden starry eyes, glowing magic halo, and emerald fireflies
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
            Self::CyberpunkSakura => tr("Aoi · Chica Anime Cyber-Sakura (HD)", "Aoi · Cyber-Sakura Anime Girl (HD)"),
            Self::CrimsonMoonTorii => tr("Kitsune · Espíritu Carmesí (HD)", "Kitsune · Crimson Fox Spirit (HD)"),
            Self::SpiritForestAurora => tr("Lumine · Maga Astral del Bosque (HD)", "Lumine · Astral Forest Mage (HD)"),
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::CyberpunkSakura => "🌸 AOI · ANIME HD",
            Self::CrimsonMoonTorii => "🦊 KITSUNE · HD",
            Self::SpiritForestAurora => "✨ LUMINE · HD",
        }
    }
}

#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

#[inline]
fn blend_rgba(base: [u8; 4], over: [u8; 4], alpha: f32) -> [u8; 4] {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.001 {
        return base;
    }
    if a >= 0.999 {
        return [over[0], over[1], over[2], 255];
    }
    [lerp_u8(base[0], over[0], a), lerp_u8(base[1], over[1], a), lerp_u8(base[2], over[2], a), 255]
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn sd_circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r
}

#[inline]
fn sd_ellipse(px: f32, py: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let dx = (px - cx) / rx.max(0.0001);
    let dy = (py - cy) / ry.max(0.0001);
    (dx * dx + dy * dy).sqrt() - 1.0
}

#[inline]
fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay).max(0.0001)).clamp(0.0, 1.0);
    ((pax - bax * h).powi(2) + (pay - bay * h).powi(2)).sqrt()
}

struct CharacterPalette {
    bg_top: [u8; 4],
    bg_bottom: [u8; 4],
    halo_color: [u8; 4],
    hair_back: [u8; 4],
    hair_main: [u8; 4],
    hair_shadow: [u8; 4],
    hair_highlight: [u8; 4],
    skin_base: [u8; 4],
    skin_shadow: [u8; 4],
    blush: [u8; 4],
    eye_top: [u8; 4],
    eye_bottom: [u8; 4],
    eye_core: [u8; 4],
    outfit_main: [u8; 4],
    outfit_accent: [u8; 4],
    particle_color: [u8; 4],
    has_fox_ears: bool,
    has_twin_tails: bool,
}

impl CharacterPalette {
    fn for_scene(scene: AnimeSceneKind) -> Self {
        match scene {
            AnimeSceneKind::CyberpunkSakura => Self {
                bg_top: [14, 12, 36, 255],
                bg_bottom: [62, 22, 78, 255],
                halo_color: [0, 230, 255, 255],
                hair_back: [26, 55, 115, 255],
                hair_main: [48, 130, 225, 255],
                hair_shadow: [28, 72, 148, 255],
                hair_highlight: [165, 240, 255, 255],
                skin_base: [255, 232, 220, 255],
                skin_shadow: [232, 188, 175, 255],
                blush: [255, 115, 155, 255],
                eye_top: [18, 85, 195, 255],
                eye_bottom: [75, 240, 255, 255],
                eye_core: [15, 24, 68, 255],
                outfit_main: [22, 24, 48, 255],
                outfit_accent: [255, 65, 165, 255],
                particle_color: [255, 175, 220, 255],
                has_fox_ears: false,
                has_twin_tails: true,
            },
            AnimeSceneKind::CrimsonMoonTorii => Self {
                bg_top: [26, 8, 18, 255],
                bg_bottom: [115, 18, 38, 255],
                halo_color: [255, 85, 65, 255],
                hair_back: [185, 175, 198, 255],
                hair_main: [242, 236, 248, 255],
                hair_shadow: [195, 182, 212, 255],
                hair_highlight: [255, 255, 255, 255],
                skin_base: [255, 236, 226, 255],
                skin_shadow: [228, 186, 176, 255],
                blush: [255, 105, 125, 255],
                eye_top: [140, 12, 32, 255],
                eye_bottom: [255, 95, 75, 255],
                eye_core: [45, 6, 14, 255],
                outfit_main: [32, 12, 20, 255],
                outfit_accent: [235, 45, 65, 255],
                particle_color: [255, 200, 110, 255],
                has_fox_ears: true,
                has_twin_tails: false,
            },
            AnimeSceneKind::SpiritForestAurora => Self {
                bg_top: [8, 24, 38, 255],
                bg_bottom: [16, 72, 68, 255],
                halo_color: [110, 255, 200, 255],
                hair_back: [88, 68, 145, 255],
                hair_main: [165, 135, 235, 255],
                hair_shadow: [112, 84, 180, 255],
                hair_highlight: [235, 225, 255, 255],
                skin_base: [255, 235, 224, 255],
                skin_shadow: [228, 190, 180, 255],
                blush: [255, 130, 165, 255],
                eye_top: [16, 125, 110, 255],
                eye_bottom: [140, 255, 195, 255],
                eye_core: [10, 48, 42, 255],
                outfit_main: [18, 38, 48, 255],
                outfit_accent: [255, 215, 105, 255],
                particle_color: [175, 255, 150, 255],
                has_fox_ears: false,
                has_twin_tails: false,
            },
        }
    }
}

/// Generates a high-resolution, anti-aliased anime character portrait frame (`DynamicImage`).
/// Rendered with smooth vector SDF curves so it looks like a crisp anime illustration in Sixel / Kitty / iTerm2.
pub fn generate_anime_frame(scene: AnimeSceneKind, frame_idx: usize, width: u32, height: u32) -> DynamicImage {
    let w = width.max(64);
    let h = height.max(64);
    let mut img = RgbaImage::new(w, h);

    let phase = (frame_idx % ANIME_FRAME_COUNT) as f32 / ANIME_FRAME_COUNT as f32;
    let angle = phase * std::f32::consts::TAU;
    let pal = CharacterPalette::for_scene(scene);

    // Subtle breathing, head tilt, hair wind sway, and natural eye blink curve
    let breath_y = angle.sin() * 0.014;
    let wind_x = (angle).cos() * 0.024;
    let wind_wave = (angle * 2.0).sin() * 0.015;
    // Blink briefly around frame 11..13
    let blink_open = match frame_idx % ANIME_FRAME_COUNT {
        11 | 13 => 0.45,
        12 => 0.08,
        _ => 1.0,
    };

    let aspect = w as f32 / h as f32;
    let aa = 1.8 / (h as f32);

    for y in 0..h {
        let ny = y as f32 / (h - 1) as f32;
        for x in 0..w {
            let nx = x as f32 / (w - 1) as f32;
            // Aspect-corrected coordinate centered at (0.0, 0.0)
            let px = (nx - 0.5) * aspect;
            let py = ny - 0.5;

            let color =
                render_anime_character_sample(px, py, nx, ny, phase, angle, breath_y, wind_x, wind_wave, blink_open, aa, &pal);
            img.put_pixel(x, y, Rgba(color));
        }
    }

    DynamicImage::ImageRgba8(img)
}

#[allow(clippy::too_many_arguments)]
fn render_anime_character_sample(
    px: f32,
    py: f32,
    nx: f32,
    ny: f32,
    phase: f32,
    angle: f32,
    breath_y: f32,
    wind_x: f32,
    wind_wave: f32,
    blink_open: f32,
    aa: f32,
    pal: &CharacterPalette,
) -> [u8; 4] {
    // 1. Smooth atmospheric background gradient + glowing anime aura ring behind character
    let mut col = [
        lerp_u8(pal.bg_top[0], pal.bg_bottom[0], ny),
        lerp_u8(pal.bg_top[1], pal.bg_bottom[1], ny),
        lerp_u8(pal.bg_top[2], pal.bg_bottom[2], ny),
        255,
    ];

    let aura_dist = (px * px + (py + 0.04) * (py + 0.04)).sqrt();
    let aura_glow = (1.0 - (aura_dist / 0.58).clamp(0.0, 1.0)).powi(2) * (0.35 + 0.08 * angle.sin());
    col = blend_rgba(col, pal.halo_color, aura_glow);

    // Subtle magic circle / neon halo ring behind the head
    let ring_r = 0.36 + 0.01 * (angle * 2.0).cos();
    let ring_d = (aura_dist - ring_r).abs();
    if ring_d < 0.018 {
        let ring_alpha = (1.0 - ring_d / 0.018).powi(2) * 0.55;
        col = blend_rgba(col, pal.halo_color, ring_alpha);
    }

    // Coordinate in character space (moves gently with breathing)
    let cx = px;
    let cy = py - breath_y;

    // 2. Back hair volume & flowing twin-tails / long locks swaying in the wind
    let sway = wind_x * (cy + 0.35).max(0.0) * 1.8 + (cy * 9.0 - angle * 2.0).sin() * 0.018;
    let back_hair_d = sd_ellipse(cx - sway * 0.7, cy - 0.05, 0.0, 0.0, 0.31, 0.44);
    let back_hair_alpha = smoothstep(aa * 1.5, -aa * 1.5, back_hair_d);
    if back_hair_alpha > 0.0 {
        let shade = ((cy + 0.3) * 1.2).clamp(0.0, 1.0);
        let bh_col = blend_rgba(pal.hair_main, pal.hair_back, 0.45 + 0.45 * shade);
        col = blend_rgba(col, bh_col, back_hair_alpha);
    }

    if pal.has_twin_tails {
        // Left and right flowing anime twin-tails
        let tail_l = sd_ellipse(cx + 0.29 - sway * 1.1, cy - 0.06, 0.0, 0.0, 0.11, 0.38);
        let tail_r = sd_ellipse(cx - 0.29 - sway * 1.1, cy - 0.06, 0.0, 0.0, 0.11, 0.38);
        let tail_d = tail_l.min(tail_r);
        let tail_a = smoothstep(aa * 1.5, -aa * 1.5, tail_d);
        if tail_a > 0.0 {
            let t_col = blend_rgba(pal.hair_main, pal.hair_shadow, ((cy + 0.25) * 1.4).clamp(0.0, 1.0));
            col = blend_rgba(col, t_col, tail_a);
        }
    }

    // 3. Fox Spirit Ears (for Kitsune scene)
    if pal.has_fox_ears {
        for side in [-1.0_f32, 1.0_f32] {
            let ex = cx * side;
            let ear_outer = sd_segment(ex, cy, 0.12, -0.24, 0.26 + wind_wave * side, -0.46)
                - 0.075 * (1.0 - ((-cy - 0.24) * 2.8).clamp(0.0, 0.85));
            let ear_a = smoothstep(aa, -aa, ear_outer);
            if ear_a > 0.0 {
                col = blend_rgba(col, pal.hair_main, ear_a);
                let ear_inner = sd_segment(ex, cy, 0.13, -0.25, 0.23 + wind_wave * side, -0.42) - 0.035;
                let inner_a = smoothstep(aa, -aa, ear_inner);
                col = blend_rgba(col, pal.blush, inner_a * 0.75);
            }
        }
    }

    // 4. Neck, Shoulders & Anime Collar / Scarf
    let torso_d = sd_ellipse(cx, cy - 0.48, 0.0, 0.0, 0.34, 0.24);
    let torso_a = smoothstep(aa, -aa, torso_d);
    if torso_a > 0.0 {
        col = blend_rgba(col, pal.outfit_main, torso_a);
        // Glowing cyber/magic collar trim
        let collar_v = (cx.abs() * 1.35 - (cy - 0.27)).abs() - 0.024;
        let collar_a = smoothstep(aa, -aa, collar_v);
        col = blend_rgba(col, pal.outfit_accent, collar_a);
    }

    let neck_d = sd_ellipse(cx, cy - 0.22, 0.0, 0.0, 0.072, 0.11);
    let neck_a = smoothstep(aa, -aa, neck_d);
    if neck_a > 0.0 {
        // Cel-shaded shadow under the chin
        let chin_shadow = smoothstep(0.24, 0.16, cy);
        let neck_col = blend_rgba(pal.skin_base, pal.skin_shadow, 0.35 + 0.55 * chin_shadow);
        col = blend_rgba(col, neck_col, neck_a);
    }

    // 5. Anime Face (smooth V-jawline + rounded upper head)
    let jaw_t = ((cy + 0.12) / 0.32).clamp(0.0, 1.0);
    let head_half_w = 0.225 * (1.0 - 0.38 * jaw_t.powf(1.65));
    let face_d = sd_ellipse(cx, cy + 0.03, 0.0, 0.0, head_half_w.max(0.04), 0.24);
    let face_a = smoothstep(aa, -aa, face_d);
    if face_a > 0.0 {
        // Cel-shaded anime skin with soft edge lighting
        let rim = smoothstep(0.14, 0.22, cx.abs() + cy * 0.15);
        let mut skin = blend_rgba(pal.skin_base, pal.skin_shadow, rim * 0.35);

        // Soft anime cheek blush + cute blush tally lines
        for side in [-1.0_f32, 1.0_f32] {
            let bx = cx * side - 0.115;
            let by = cy - 0.045;
            let blush_d = (bx * bx / (0.055 * 0.055) + by * by / (0.028 * 0.028)).sqrt();
            if blush_d < 1.0 {
                let ba = (1.0 - blush_d).powi(2) * 0.48;
                skin = blend_rgba(skin, pal.blush, ba);
            }
        }

        col = blend_rgba(col, skin, face_a);
        // Crisp anime jawline outline
        let outline_a = smoothstep(0.009, 0.0, face_d.abs()) * smoothstep(-0.08, 0.15, cy);
        col = blend_rgba(col, [42, 24, 32, 255], outline_a * 0.72);
    }

    // 6. Big Expressive Sparkling Anime Eyes (Left & Right)
    for side in [-1.0_f32, 1.0_f32] {
        let ex = cx * side - 0.098;
        let ey = cy + 0.015;
        let eye_rx = 0.054;
        let eye_ry = (0.062 * blink_open).max(0.005);

        if blink_open <= 0.15 {
            // Closed happy/blinking eyelash arc
            let lash_curve = (ey - (ex * ex * 2.5)).abs() - 0.008;
            if ex.abs() < eye_rx {
                let la = smoothstep(aa, -aa, lash_curve);
                col = blend_rgba(col, [24, 14, 28, 255], la);
            }
        } else {
            // Sclera (white of the eye)
            let sclera_d = sd_ellipse(ex, ey, 0.0, 0.0, eye_rx, eye_ry);
            let sclera_a = smoothstep(aa, -aa, sclera_d);
            if sclera_a > 0.0 {
                let sclera_col = blend_rgba([252, 250, 255, 255], [210, 215, 235, 255], smoothstep(-eye_ry, 0.0, -ey) * 0.35);
                col = blend_rgba(col, sclera_col, sclera_a);

                // Large detailed anime iris with rich vertical gradient
                let iris_x = ex + wind_x * 0.18 * side;
                let iris_d = sd_ellipse(iris_x, ey, 0.0, 0.0, 0.042, eye_ry * 0.92);
                let iris_a = smoothstep(aa, -aa, iris_d);
                if iris_a > 0.0 {
                    let it = ((ey / eye_ry) * 0.5 + 0.5).clamp(0.0, 1.0);
                    let mut iris_col = [
                        lerp_u8(pal.eye_top[0], pal.eye_bottom[0], it),
                        lerp_u8(pal.eye_top[1], pal.eye_bottom[1], it),
                        lerp_u8(pal.eye_top[2], pal.eye_bottom[2], it),
                        255,
                    ];

                    // Dark pupil core
                    let pupil_d = sd_ellipse(iris_x, ey, 0.0, 0.0, 0.019, eye_ry * 0.45);
                    let pupil_a = smoothstep(aa, -aa, pupil_d);
                    iris_col = blend_rgba(iris_col, pal.eye_core, pupil_a * 0.92);

                    // Lower glowing crescent inside iris
                    let crescent_d = (iris_d + 0.012).abs();
                    if ey > 0.005 && crescent_d < 0.012 {
                        iris_col = blend_rgba(iris_col, [255, 255, 255, 255], (1.0 - crescent_d / 0.012) * 0.55);
                    }

                    // Dark outer iris limbal ring
                    let limbal = smoothstep(-0.008, 0.0, iris_d);
                    iris_col = blend_rgba(iris_col, pal.eye_core, limbal * 0.65);

                    col = blend_rgba(col, iris_col, iris_a);

                    // Crisp white anime catchlights (main top-left sparkle + secondary bottom-right dot)
                    let hl1_d = sd_circle(ex * side, ey, -0.018, -0.022 * blink_open, 0.014);
                    let hl2_d = sd_circle(ex * side, ey, 0.016, 0.018 * blink_open, 0.007);
                    let hl_a = smoothstep(aa, -aa, hl1_d.min(hl2_d));
                    col = blend_rgba(col, [255, 255, 255, 255], hl_a);
                }
            }

            // Thick upper anime eyelash wing
            let lash_y = -eye_ry * (1.0 - (ex / eye_rx).powi(2)).max(0.0).sqrt();
            if ex.abs() <= eye_rx * 1.15 && ey <= 0.01 {
                let lash_d = (ey - lash_y).abs() - 0.011;
                let lash_a = smoothstep(aa, -aa, lash_d);
                col = blend_rgba(col, [18, 12, 26, 255], lash_a);
            }
        }

        // Delicate anime eyebrow
        let brow_y = -0.072 + (ex * 0.25);
        if ex.abs() < 0.058 {
            let brow_d = (ey - brow_y).abs() - 0.0045;
            let brow_a = smoothstep(aa, -aa, brow_d);
            col = blend_rgba(col, pal.hair_shadow, brow_a * 0.85);
        }
    }

    // 7. Delicate anime nose & gentle smile
    let nose_d = sd_circle(cx, cy, 0.0, 0.055, 0.005);
    col = blend_rgba(col, pal.skin_shadow, smoothstep(aa, -aa, nose_d) * 0.85);

    let smile_curve = cy - (0.115 + cx * cx * 1.8);
    if cx.abs() < 0.036 {
        let mouth_d = smile_curve.abs() - 0.0045;
        let mouth_a = smoothstep(aa, -aa, mouth_d);
        col = blend_rgba(col, [185, 82, 98, 255], mouth_a * 0.9);
    }

    // 8. Front Hair Bangs & Side Locks (with swaying strands and glossy angel-ring highlight)
    // Crown hair dome
    let crown_d = sd_ellipse(cx, cy + 0.16, 0.0, 0.0, 0.25, 0.16);
    let mut front_hair_d = if cy < -0.09 { crown_d } else { 1.0 };

    // Layered anime bangs across forehead
    let bang_centers = [-0.17_f32, -0.085, 0.0, 0.085, 0.17];
    let bang_lengths = [0.22_f32, 0.17, 0.185, 0.165, 0.22];
    for (&bx_center, &b_len) in bang_centers.iter().zip(bang_lengths.iter()) {
        let strand_sway = wind_x * 0.45 * ((cy + 0.22).max(0.0));
        let bx = cx - (bx_center + strand_sway);
        let by = cy + 0.18;
        if by >= 0.0 && by <= b_len + 0.05 {
            let t = (by / b_len).clamp(0.0, 1.0);
            let half_w = 0.054 * (1.0 - t.powf(1.7));
            let d = bx.abs() - half_w;
            front_hair_d = front_hair_d.min(d);
        }
    }

    // Long framing side locks past the cheeks
    for side in [-1.0_f32, 1.0_f32] {
        let lock_sway = wind_x * 0.85 * (cy + 0.15).max(0.0);
        let lx = cx - (side * 0.215 + lock_sway);
        let ly = cy + 0.10;
        if (-0.12..0.38).contains(&ly) {
            let t = ((ly + 0.12) / 0.50).clamp(0.0, 1.0);
            let half_w = 0.048 * (1.0 - 0.75 * t.powf(1.5));
            front_hair_d = front_hair_d.min(lx.abs() - half_w);
        }
    }

    let front_hair_a = smoothstep(aa, -aa, front_hair_d);
    if front_hair_a > 0.0 {
        let grad = ((cy + 0.28) * 1.9).clamp(0.0, 1.0);
        let mut h_col = blend_rgba(pal.hair_main, pal.hair_shadow, grad * 0.55);

        // Signature anime "angel ring" glossy hair highlight band across the bangs
        let ring_y = -0.145 + cx * cx * 0.9 + (cx * 28.0).sin() * 0.008;
        let hl_dist = (cy - ring_y).abs();
        if hl_dist < 0.018 {
            let hl_alpha = (1.0 - hl_dist / 0.018).powi(2) * 0.75;
            h_col = blend_rgba(h_col, pal.hair_highlight, hl_alpha);
        }

        // Crisp dark hair tip outline
        let edge_a = smoothstep(0.007, 0.0, front_hair_d.abs());
        h_col = blend_rgba(h_col, pal.hair_shadow, edge_a * 0.7);

        col = blend_rgba(col, h_col, front_hair_a);
    }

    // 9. Foreground floating anime particles (Sakura petals / Spirit embers / Starlight sparkles)
    for i in 0..10 {
        let seed = i as f32 * 1.73;
        let p_nx = (seed * 0.67 + phase * 0.45 + (angle + seed).sin() * 0.05).rem_euclid(1.0);
        let p_ny = (seed * 0.41 + phase).rem_euclid(1.0);
        let dx = (nx - p_nx) * 1.4;
        let dy = ny - p_ny;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.022 {
            let pa = (1.0 - dist / 0.022).powi(2) * 0.88;
            col = blend_rgba(col, pal.particle_color, pa);
        }
    }

    col
}

pub struct AnimePlayerState {
    pub scene: AnimeSceneKind,
    pub current_frame: usize,
    pub is_paused: bool,
    pub is_fullscreen_open: bool,
    frames_raw: Vec<Option<DynamicImage>>,
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
        let frames_raw = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
        let hero_protocols = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
        let modal_protocols = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
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

    fn ensure_raw_frame(&mut self, frame_idx: usize) -> &DynamicImage {
        let idx = frame_idx % ANIME_FRAME_COUNT;
        if self.frames_raw[idx].is_none() {
            // High-definition 320x220 anti-aliased vector anime character portrait
            self.frames_raw[idx] = Some(generate_anime_frame(self.scene, idx, 320, 220));
        }
        self.frames_raw[idx].as_ref().unwrap()
    }

    pub fn set_scene(&mut self, scene: AnimeSceneKind) {
        if self.scene != scene {
            self.scene = scene;
            self.current_frame = 0;
            self.frames_raw = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
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
        self.hero_protocols = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
        self.modal_protocols = (0..ANIME_FRAME_COUNT).map(|_| None).collect();
    }

    /// Renders the current anime frame into the Home Hero banner using high-res `Picker` (Sixel / Kitty / iTerm2) when available.
    pub fn render_hero_canvas(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>) {
        self.render_with_slot(area, buf, picker, false);
    }

    /// Renders the current anime frame into the fullscreen modal using high-res `Picker` (Sixel / Kitty / iTerm2) when available.
    pub fn render_canvas(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>) {
        self.render_with_slot(area, buf, picker, true);
    }

    fn render_with_slot(&mut self, area: Rect, buf: &mut Buffer, picker: Option<&mut Picker>, is_modal: bool) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let frame_idx = self.current_frame % ANIME_FRAME_COUNT;
        let _ = self.ensure_raw_frame(frame_idx);

        if let Some(picker) = picker {
            let need_init = if is_modal {
                self.modal_protocols.get(frame_idx).and_then(|p| p.as_ref()).is_none()
            } else {
                self.hero_protocols.get(frame_idx).and_then(|p| p.as_ref()).is_none()
            };
            if need_init && let Some(Some(raw)) = self.frames_raw.get(frame_idx) {
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
        let Some(Some(DynamicImage::ImageRgba8(img))) = self.frames_raw.get(frame_idx) else {
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
        let modal_h = (area.height.saturating_mul(84) / 100).clamp(16.min(area.height), area.height.saturating_sub(2).max(1));
        let x = area.x + area.width.saturating_sub(modal_w) / 2;
        let y = area.y + area.height.saturating_sub(modal_h) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        Clear.render(modal_area, buf);

        let status_badge =
            if self.state.is_paused { tr(" ⏸ PAUSADO ", " ⏸ PAUSED ") } else { tr(" ▶ EN VIVO HD ", " ▶ LIVE HD ") };

        let title_left = Line::from(vec![
            Span::styled(
                tr(" 🎬 Personaje Anime en Vivo ", " 🎬 Live Anime Character "),
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
            Span::styled(tr(" Cambiar personaje  ", " Switch character  "), Style::default().fg(t.text_muted)),
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
        assert!(rendered.contains("Aoi"), "modal title should display current character title");
    }
}
