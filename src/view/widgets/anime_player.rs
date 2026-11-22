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
    /// Frieren · Blue Moonweed Field: Frieren with silver twin-tails, elf ears, red mirror-lotus earrings, white/gold capelet, and
    /// glowing blue petals
    #[default]
    CyberpunkSakura = 0,
    /// Frieren · Zoltraak Magic Circle: Frieren casting her signature cyan/golden mana spell with her crimson jewel crescent staff
    CrimsonMoonTorii = 1,
    /// Frieren · Cozy Grimoire & Winter Scarf: Frieren wrapped in her striped winter scarf with warm golden magic light
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
            Self::CyberpunkSakura => tr("Frieren · Campo de Flores de Luna Azul (HD)", "Frieren · Blue Moonweed Field (HD)"),
            Self::CrimsonMoonTorii => tr("Frieren · Magia Zoltraak y Báculo (HD)", "Frieren · Zoltraak Magic & Staff (HD)"),
            Self::SpiritForestAurora => {
                tr("Frieren · Grimorio y Bufanda de Invierno (HD)", "Frieren · Grimoire & Winter Scarf (HD)")
            },
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::CyberpunkSakura => "🧝‍♀️ FRIEREN · MOONWEED",
            Self::CrimsonMoonTorii => "🪄 FRIEREN · ZOLTRAAK",
            Self::SpiritForestAurora => "📖 FRIEREN · GRIMOIRE",
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

struct FrierenSceneTheme {
    bg_top: [u8; 4],
    bg_bottom: [u8; 4],
    halo_color: [u8; 4],
    particle_primary: [u8; 4],
    particle_secondary: [u8; 4],
    show_staff: bool,
    show_winter_scarf: bool,
    smug_smile: bool,
}

impl FrierenSceneTheme {
    fn for_scene(scene: AnimeSceneKind) -> Self {
        match scene {
            AnimeSceneKind::CyberpunkSakura => Self {
                bg_top: [10, 18, 44, 255],
                bg_bottom: [24, 62, 118, 255],
                halo_color: [90, 210, 255, 255],
                particle_primary: [110, 220, 255, 255],
                particle_secondary: [200, 245, 255, 255],
                show_staff: true,
                show_winter_scarf: false,
                smug_smile: false,
            },
            AnimeSceneKind::CrimsonMoonTorii => Self {
                bg_top: [16, 12, 38, 255],
                bg_bottom: [48, 28, 86, 255],
                halo_color: [120, 245, 255, 255],
                particle_primary: [140, 250, 255, 255],
                particle_secondary: [255, 220, 120, 255],
                show_staff: true,
                show_winter_scarf: false,
                smug_smile: false,
            },
            AnimeSceneKind::SpiritForestAurora => Self {
                bg_top: [22, 16, 34, 255],
                bg_bottom: [68, 42, 64, 255],
                halo_color: [255, 215, 130, 255],
                particle_primary: [255, 225, 145, 255],
                particle_secondary: [140, 245, 215, 255],
                show_staff: false,
                show_winter_scarf: true,
                smug_smile: true,
            },
        }
    }
}

/// Generates a high-resolution, anti-aliased portrait of Frieren (`Sousou no Frieren`).
/// Rendered with smooth vector SDF curves so it looks like a crisp anime illustration in Sixel / Kitty / iTerm2.
pub fn generate_anime_frame(scene: AnimeSceneKind, frame_idx: usize, width: u32, height: u32) -> DynamicImage {
    let w = width.max(64);
    let h = height.max(64);
    let mut img = RgbaImage::new(w, h);

    let phase = (frame_idx % ANIME_FRAME_COUNT) as f32 / ANIME_FRAME_COUNT as f32;
    let angle = phase * std::f32::consts::TAU;
    let st = FrierenSceneTheme::for_scene(scene);

    // Subtle breathing, hair wind sway, and natural eye blink curve
    let breath_y = angle.sin() * 0.012;
    let wind_x = angle.cos() * 0.022;
    let wind_wave = (angle * 2.0).sin() * 0.012;
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
            let px = (nx - 0.5) * aspect;
            let py = ny - 0.5;

            let color = render_frieren_sample(px, py, nx, ny, phase, angle, breath_y, wind_x, wind_wave, blink_open, aa, &st);
            img.put_pixel(x, y, Rgba(color));
        }
    }

    DynamicImage::ImageRgba8(img)
}

#[allow(clippy::too_many_arguments)]
fn render_frieren_sample(
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
    st: &FrierenSceneTheme,
) -> [u8; 4] {
    // Canonical Frieren color palette
    let hair_main = [244, 245, 252, 255];
    let hair_shadow = [194, 200, 224, 255];
    let hair_deep = [155, 164, 196, 255];
    let hair_highlight = [255, 255, 255, 255];
    let ribbon_dark = [36, 32, 48, 255];
    let gold_trim = [242, 198, 92, 255];
    let gold_shadow = [188, 142, 48, 255];
    let cape_white = [246, 246, 250, 255];
    let cape_shadow = [212, 216, 230, 255];
    let earring_red = [228, 38, 62, 255];
    let earring_glow = [255, 130, 145, 255];
    let skin_base = [255, 238, 228, 255];
    let skin_shadow = [230, 194, 184, 255];
    let blush_pink = [255, 135, 155, 255];
    // Frieren's signature emerald-teal eyes with golden-green lower gradient
    let eye_top = [16, 108, 98, 255];
    let eye_bottom = [125, 235, 178, 255];
    let eye_core = [10, 44, 42, 255];

    // 1. Atmospheric background & Zoltraak magic circle / mana aura behind Frieren
    let mut col = [
        lerp_u8(st.bg_top[0], st.bg_bottom[0], ny),
        lerp_u8(st.bg_top[1], st.bg_bottom[1], ny),
        lerp_u8(st.bg_top[2], st.bg_bottom[2], ny),
        255,
    ];

    let aura_dist = (px * px + (py + 0.03) * (py + 0.03)).sqrt();
    let aura_glow = (1.0 - (aura_dist / 0.62).clamp(0.0, 1.0)).powi(2) * (0.36 + 0.08 * angle.sin());
    col = blend_rgba(col, st.halo_color, aura_glow);

    // Concentric Zoltraak arcane magic rings behind Frieren
    for &(ring_r, thickness) in &[(0.37_f32, 0.014_f32), (0.44_f32, 0.009_f32)] {
        let r_d = (aura_dist - (ring_r + 0.006 * (angle * 2.0).cos())).abs();
        if r_d < thickness {
            let ra = (1.0 - r_d / thickness).powi(2) * 0.52;
            col = blend_rgba(col, st.halo_color, ra);
        }
    }

    let cx = px;
    let cy = py - breath_y;

    // 2. Frieren's Signature Twin-Tails (High & Wide Flowing Silver-White Pigtails) + Dark Hair Ties
    let sway = wind_x * (cy + 0.32).max(0.0) * 1.7 + (cy * 8.5 - angle * 2.0).sin() * 0.016;

    for side in [-1.0_f32, 1.0_f32] {
        // Long sweeping twin-tail curving outward from the upper sides of her head
        let tx = cx * side - (0.28 + (cy + 0.12).max(0.0) * 0.34) - sway * side * 0.9;
        let ty = cy - 0.04;
        let tail_rx = 0.115 * (1.0 - ((ty + 0.22) * 0.65).clamp(0.0, 0.55));
        let tail_d = sd_ellipse(tx, ty, 0.0, 0.0, tail_rx.max(0.04), 0.42);
        let tail_a = smoothstep(aa * 1.4, -aa * 1.4, tail_d);
        if tail_a > 0.0 {
            let shade = ((ty + 0.28) * 1.3).clamp(0.0, 1.0);
            let mut t_col = blend_rgba(hair_main, hair_shadow, 0.25 + 0.55 * shade);
            // Twin-tail inner strand contour line
            let strand_line = (tx * 18.0).sin().abs();
            if strand_line < 0.18 {
                t_col = blend_rgba(t_col, hair_deep, 0.25);
            }
            let edge = smoothstep(0.008, 0.0, tail_d.abs());
            t_col = blend_rgba(t_col, hair_deep, edge * 0.65);
            col = blend_rgba(col, t_col, tail_a);
        }

        // Frieren's black/dark-violet hair ribbon ties at the base of her twin-tails
        let tie_x = cx * side - 0.245;
        let tie_y = cy + 0.165;
        let tie_d = sd_ellipse(tie_x, tie_y, 0.0, 0.0, 0.045, 0.032);
        let tie_a = smoothstep(aa, -aa, tie_d);
        if tie_a > 0.0 {
            col = blend_rgba(col, ribbon_dark, tie_a);
        }
    }

    // Back crown hair volume
    let back_hair_d = sd_ellipse(cx - sway * 0.4, cy + 0.02, 0.0, 0.0, 0.26, 0.30);
    let back_hair_a = smoothstep(aa, -aa, back_hair_d);
    if back_hair_a > 0.0 {
        col = blend_rgba(col, hair_shadow, back_hair_a);
    }

    // 3. Frieren's Iconic Long Horizontal Pointed Elf Ears
    for side in [-1.0_f32, 1.0_f32] {
        let ex = cx * side;
        let ey = cy - 0.015;
        // Elf ear extends horizontally outward from (0.18, 0.015) to (0.41, -0.025)
        let t_ear = ((ex - 0.17) / 0.24).clamp(0.0, 1.0);
        let ear_half_h = 0.044 * (1.0 - t_ear.powf(1.45)) * (0.75 + 0.4 * (t_ear * 3.14).sin());
        let ear_center_y = -0.025 * t_ear + wind_wave * 0.15 * t_ear;
        let ear_d = if (0.16..=0.415).contains(&ex) { (ey - ear_center_y).abs() - ear_half_h } else { 1.0 };
        let ear_a = smoothstep(aa, -aa, ear_d);
        if ear_a > 0.0 {
            let mut ear_col = blend_rgba(skin_base, skin_shadow, t_ear * 0.35);
            // Inner ear blush fold line
            let inner_fold = (ey - (ear_center_y + 0.006)).abs() - ear_half_h * 0.38;
            if inner_fold < 0.0 {
                ear_col = blend_rgba(ear_col, blush_pink, 0.38);
            }
            let outline = smoothstep(0.006, 0.0, ear_d.abs());
            ear_col = blend_rgba(ear_col, [65, 38, 45, 255], outline * 0.72);
            col = blend_rgba(col, ear_col, ear_a);
        }

        // 4. Frieren's Iconic Red & Gold Hanging Earrings (dangling below her elf ears)
        let ear_attach_x = 0.235;
        let ear_attach_y = 0.045;
        let drop_x = ear_attach_x + sway * 0.35 * side;
        let drop_y = 0.125;

        // Thin gold chain
        let chain_d = sd_segment(ex, cy, ear_attach_x, ear_attach_y, drop_x, drop_y) - 0.005;
        col = blend_rgba(col, gold_trim, smoothstep(aa, -aa, chain_d));

        // Gold upper cap + Crimson red teardrop gem
        let gem_d = sd_ellipse(ex, cy, drop_x, drop_y + 0.015, 0.022, 0.032);
        let gem_a = smoothstep(aa, -aa, gem_d);
        if gem_a > 0.0 {
            let mut g_col = earring_red;
            // Glossy white/pink highlight on the red gem
            let g_hl = sd_circle(ex, cy, drop_x - 0.007, drop_y + 0.006, 0.008);
            g_col = blend_rgba(g_col, earring_glow, smoothstep(aa, -aa, g_hl));
            col = blend_rgba(col, g_col, gem_a);
        }
        let cap_d = sd_circle(ex, cy, drop_x, drop_y - 0.012, 0.011);
        col = blend_rgba(col, gold_trim, smoothstep(aa, -aa, cap_d));
    }

    // 5. Frieren's White & Gold Mage Capelet (or Cozy Winter Scarf in Scene 3)
    let torso_d = sd_ellipse(cx, cy - 0.46, 0.0, 0.0, 0.36, 0.25);
    let torso_a = smoothstep(aa, -aa, torso_d);
    if torso_a > 0.0 {
        let mut cape_col = blend_rgba(cape_white, cape_shadow, ((cy - 0.24) * 1.8).clamp(0.0, 1.0) * 0.45);

        // Dark navy inner shirt V-neck + red ribbon tie
        if cx.abs() < 0.085 && cy < 0.36 {
            cape_col = ribbon_dark;
            let brooch_d = sd_circle(cx, cy, 0.0, 0.32, 0.024);
            if brooch_d < 0.0 {
                cape_col = earring_red;
            }
        }

        // Gold border stripes & shoulder epualettes on Frieren's white cape
        let collar_ring = ((cx * cx * 1.4 + (cy - 0.23).powi(2)).sqrt() - 0.165).abs() - 0.016;
        let lower_gold_trim = ((cx * cx * 0.9 + (cy - 0.20).powi(2)).sqrt() - 0.255).abs() - 0.014;
        let gold_a = smoothstep(aa, -aa, collar_ring.min(lower_gold_trim));
        cape_col = blend_rgba(cape_col, gold_trim, gold_a);

        // Red gem clasp on the white cape collar
        for side in [-1.0_f32, 1.0_f32] {
            let clasp_d = sd_circle(cx * side, cy, 0.125, 0.31, 0.016);
            cape_col = blend_rgba(cape_col, earring_red, smoothstep(aa, -aa, clasp_d));
        }

        col = blend_rgba(col, cape_col, torso_a);
    }

    // Neck with soft cel-shaded chin shadow
    let neck_d = sd_ellipse(cx, cy - 0.21, 0.0, 0.0, 0.068, 0.10);
    let neck_a = smoothstep(aa, -aa, neck_d);
    if neck_a > 0.0 {
        let chin_shadow = smoothstep(0.23, 0.15, cy);
        let neck_col = blend_rgba(skin_base, skin_shadow, 0.32 + 0.55 * chin_shadow);
        col = blend_rgba(col, neck_col, neck_a);
    }

    // Cozy winter scarf option (Scene 3)
    if st.show_winter_scarf {
        let scarf_d = sd_ellipse(cx, cy - 0.25, 0.0, 0.0, 0.22, 0.075);
        let scarf_a = smoothstep(aa, -aa, scarf_d);
        if scarf_a > 0.0 {
            let stripe = ((cx * 24.0).sin() > 0.35) as i32;
            let s_col = if stripe == 1 { [185, 52, 68, 255] } else { [238, 228, 212, 255] };
            col = blend_rgba(col, s_col, scarf_a);
        }
    }

    // 6. Frieren's Face (calm, delicate elf jawline)
    let jaw_t = ((cy + 0.12) / 0.32).clamp(0.0, 1.0);
    let head_half_w = 0.222 * (1.0 - 0.39 * jaw_t.powf(1.65));
    let face_d = sd_ellipse(cx, cy + 0.03, 0.0, 0.0, head_half_w.max(0.04), 0.238);
    let face_a = smoothstep(aa, -aa, face_d);
    if face_a > 0.0 {
        let rim = smoothstep(0.14, 0.22, cx.abs() + cy * 0.15);
        let mut skin = blend_rgba(skin_base, skin_shadow, rim * 0.32);

        // Subtle anime cheek blush
        for side in [-1.0_f32, 1.0_f32] {
            let bx = cx * side - 0.112;
            let by = cy - 0.045;
            let blush_d = (bx * bx / (0.052 * 0.052) + by * by / (0.026 * 0.026)).sqrt();
            if blush_d < 1.0 {
                let ba = (1.0 - blush_d).powi(2) * 0.42;
                skin = blend_rgba(skin, blush_pink, ba);
            }
        }

        col = blend_rgba(col, skin, face_a);
        let outline_a = smoothstep(0.008, 0.0, face_d.abs()) * smoothstep(-0.08, 0.15, cy);
        col = blend_rgba(col, [52, 34, 42, 255], outline_a * 0.72);
    }

    // 7. Frieren's Expressive Emerald-Green Eyes (calm, slightly sleepy upper lids with rich teal-gold gradient)
    for side in [-1.0_f32, 1.0_f32] {
        let ex = cx * side - 0.096;
        let ey = cy + 0.012;
        let eye_rx = 0.055;
        let eye_ry = (0.054 * blink_open).max(0.005);

        if blink_open <= 0.15 {
            let lash_curve = (ey - (ex * ex * 2.2)).abs() - 0.008;
            if ex.abs() < eye_rx {
                col = blend_rgba(col, [28, 22, 36, 255], smoothstep(aa, -aa, lash_curve));
            }
        } else {
            let sclera_d = sd_ellipse(ex, ey, 0.0, 0.0, eye_rx, eye_ry);
            let sclera_a = smoothstep(aa, -aa, sclera_d);
            if sclera_a > 0.0 {
                let sclera_col = blend_rgba([252, 252, 255, 255], [212, 220, 236, 255], smoothstep(-eye_ry, 0.0, -ey) * 0.35);
                col = blend_rgba(col, sclera_col, sclera_a);

                // Emerald-teal iris
                let iris_x = ex + wind_x * 0.16 * side;
                let iris_d = sd_ellipse(iris_x, ey, 0.0, 0.0, 0.042, eye_ry * 0.94);
                let iris_a = smoothstep(aa, -aa, iris_d);
                if iris_a > 0.0 {
                    let it = ((ey / eye_ry) * 0.5 + 0.5).clamp(0.0, 1.0);
                    let mut iris_col = [
                        lerp_u8(eye_top[0], eye_bottom[0], it),
                        lerp_u8(eye_top[1], eye_bottom[1], it),
                        lerp_u8(eye_top[2], eye_bottom[2], it),
                        255,
                    ];

                    // Golden-green lower iris ring highlight (characteristic of Frieren's eyes)
                    let pupil_d = sd_ellipse(iris_x, ey, 0.0, 0.0, 0.018, eye_ry * 0.44);
                    iris_col = blend_rgba(iris_col, eye_core, smoothstep(aa, -aa, pupil_d) * 0.92);

                    let crescent_d = (iris_d + 0.012).abs();
                    if ey > 0.004 && crescent_d < 0.012 {
                        iris_col = blend_rgba(iris_col, [225, 255, 195, 255], (1.0 - crescent_d / 0.012) * 0.65);
                    }

                    let limbal = smoothstep(-0.008, 0.0, iris_d);
                    iris_col = blend_rgba(iris_col, eye_core, limbal * 0.72);
                    col = blend_rgba(col, iris_col, iris_a);

                    // Sparkling white catchlights
                    let hl1_d = sd_circle(ex * side, ey, -0.018, -0.020 * blink_open, 0.014);
                    let hl2_d = sd_circle(ex * side, ey, 0.016, 0.016 * blink_open, 0.007);
                    col = blend_rgba(col, [255, 255, 255, 255], smoothstep(aa, -aa, hl1_d.min(hl2_d)));
                }
            }

            // Frieren's signature flat/calm upper tsurime-tareme eyelash line
            let lash_y = -eye_ry * (1.0 - (ex / eye_rx).powi(2)).max(0.0).sqrt() * 0.88;
            if ex.abs() <= eye_rx * 1.16 && ey <= 0.01 {
                let lash_d = (ey - lash_y).abs() - 0.011;
                col = blend_rgba(col, [26, 20, 34, 255], smoothstep(aa, -aa, lash_d));
            }
        }

        // Silver-grey delicate eyebrows
        let brow_y = -0.068 + (ex * 0.14);
        if ex.abs() < 0.056 {
            let brow_d = (ey - brow_y).abs() - 0.0045;
            col = blend_rgba(col, hair_deep, smoothstep(aa, -aa, brow_d) * 0.9);
        }
    }

    // 8. Delicate nose & Frieren's serene / smug `=w=` cat-smile
    let nose_d = sd_circle(cx, cy, 0.0, 0.054, 0.0048);
    col = blend_rgba(col, skin_shadow, smoothstep(aa, -aa, nose_d) * 0.85);

    if st.smug_smile {
        // Frieren's famous smug `<(=w=)>` cat mouth in Scene 3
        let w_curve = cy - (0.112 + ((cx.abs() - 0.018).powi(2) * 8.5));
        if cx.abs() < 0.042 {
            let mouth_d = w_curve.abs() - 0.0045;
            col = blend_rgba(col, [175, 78, 92, 255], smoothstep(aa, -aa, mouth_d) * 0.92);
        }
    } else {
        let smile_curve = cy - (0.114 + cx * cx * 1.3);
        if cx.abs() < 0.032 {
            let mouth_d = smile_curve.abs() - 0.0042;
            col = blend_rgba(col, [175, 78, 92, 255], smoothstep(aa, -aa, mouth_d) * 0.9);
        }
    }

    // 9. Frieren's Center-Parted Silver-White Bangs & Long Side Locks
    let crown_d = sd_ellipse(cx, cy + 0.165, 0.0, 0.0, 0.245, 0.155);
    let mut front_hair_d = if cy < -0.095 { crown_d } else { 1.0 };

    // Frieren's bangs part near the center and sweep gently left and right over her eyebrows
    let bang_specs: [(f32, f32, f32); 6] = [
        (-0.18, 0.23, 0.050),
        (-0.105, 0.19, 0.048),
        (-0.042, 0.165, 0.038),
        (0.042, 0.165, 0.038),
        (0.105, 0.19, 0.048),
        (0.18, 0.23, 0.050),
    ];
    for &(bx_center, b_len, b_width) in &bang_specs {
        let strand_sway = wind_x * 0.42 * (cy + 0.22).max(0.0);
        let arch = (cy + 0.18).max(0.0) * bx_center.signum() * 0.12;
        let bx = cx - (bx_center + arch + strand_sway);
        let by = cy + 0.185;
        if by >= 0.0 && by <= b_len + 0.05 {
            let t = (by / b_len).clamp(0.0, 1.0);
            let half_w = b_width * (1.0 - t.powf(1.65));
            front_hair_d = front_hair_d.min(bx.abs() - half_w);
        }
    }

    // Long silver side locks framing her cheeks
    for side in [-1.0_f32, 1.0_f32] {
        let lock_sway = wind_x * 0.80 * (cy + 0.15).max(0.0);
        let lx = cx - (side * 0.212 + lock_sway);
        let ly = cy + 0.10;
        if (-0.12..0.40).contains(&ly) {
            let t = ((ly + 0.12) / 0.52).clamp(0.0, 1.0);
            let half_w = 0.046 * (1.0 - 0.72 * t.powf(1.5));
            front_hair_d = front_hair_d.min(lx.abs() - half_w);
        }
    }

    let front_hair_a = smoothstep(aa, -aa, front_hair_d);
    if front_hair_a > 0.0 {
        let grad = ((cy + 0.28) * 1.8).clamp(0.0, 1.0);
        let mut h_col = blend_rgba(hair_main, hair_shadow, grad * 0.48);

        // Soft glossy white angel-ring highlight across her silver bangs
        let ring_y = -0.145 + cx * cx * 0.85 + (cx * 26.0).sin() * 0.007;
        let hl_dist = (cy - ring_y).abs();
        if hl_dist < 0.020 {
            let hl_alpha = (1.0 - hl_dist / 0.020).powi(2) * 0.85;
            h_col = blend_rgba(h_col, hair_highlight, hl_alpha);
        }

        let edge_a = smoothstep(0.007, 0.0, front_hair_d.abs());
        h_col = blend_rgba(h_col, hair_deep, edge_a * 0.72);
        col = blend_rgba(col, h_col, front_hair_a);
    }

    // 10. Frieren's Golden Crescent Staff with Crimson Gem (on the right side)
    if st.show_staff {
        let staff_x = 0.46 + wind_x * 0.25;
        let staff_y = -0.06 + breath_y * 0.5;
        // Wooden staff shaft
        let shaft_d = sd_segment(px, py, staff_x, staff_y + 0.08, staff_x - 0.05, 0.55) - 0.016;
        col = blend_rgba(col, [110, 62, 38, 255], smoothstep(aa, -aa, shaft_d));

        // Golden crescent head of Frieren's staff
        let outer_c = sd_circle(px, py, staff_x, staff_y, 0.082);
        let inner_c = sd_circle(px, py, staff_x - 0.024, staff_y - 0.012, 0.062);
        let crescent_d = outer_c.max(-inner_c);
        let cresc_a = smoothstep(aa, -aa, crescent_d);
        if cresc_a > 0.0 {
            let g_col = blend_rgba(gold_trim, gold_shadow, ((py - staff_y) * 5.0).clamp(0.0, 1.0));
            col = blend_rgba(col, g_col, cresc_a);
        }

        // Glowing crimson orb nestled inside the golden crescent
        let gem_d = sd_circle(px, py, staff_x - 0.012, staff_y - 0.005, 0.030);
        let gem_a = smoothstep(aa, -aa, gem_d);
        if gem_a > 0.0 {
            let pulse = 0.5 + 0.5 * (angle * 2.0).sin();
            let orb_col = blend_rgba(earring_red, [255, 160, 175, 255], pulse * 0.45);
            col = blend_rgba(col, orb_col, gem_a);
        }
        // Magic mana glow around the staff gem
        let staff_glow_d = sd_circle(px, py, staff_x - 0.012, staff_y - 0.005, 0.12);
        if staff_glow_d < 0.0 {
            let ga = (-staff_glow_d / 0.12).powi(2) * 0.38;
            col = blend_rgba(col, st.halo_color, ga);
        }
    }

    // 11. Floating Blue Moonweed Petals & Zoltraak Mana Butterflies
    for i in 0..11 {
        let seed = i as f32 * 1.73;
        let p_nx = (seed * 0.67 + phase * 0.42 + (angle + seed).sin() * 0.05).rem_euclid(1.0);
        let p_ny = (seed * 0.41 + phase).rem_euclid(1.0);
        let dx = (nx - p_nx) * 1.4;
        let dy = ny - p_ny;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.024 {
            let pa = (1.0 - dist / 0.024).powi(2) * 0.90;
            let p_col = if i % 2 == 0 { st.particle_primary } else { st.particle_secondary };
            col = blend_rgba(col, p_col, pa);
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
            // High-definition 320x220 anti-aliased vector portrait of Frieren
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
            Span::styled(tr(" Cambiar escena de Frieren  ", " Switch Frieren scene  "), Style::default().fg(t.text_muted)),
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
        assert!(rendered.contains("Frieren"), "modal title should display Frieren");
    }
}
