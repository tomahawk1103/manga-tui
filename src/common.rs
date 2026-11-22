use std::collections::HashMap;
use std::fmt::{Display, Write as _};

use image::DynamicImage;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use crate::config::MangaTuiConfig;

#[derive(Default)]
pub struct ImageState {
    /// save the image loaded for a manga, it will be retrieved by it's id
    image_state: HashMap<String, Box<dyn StatefulProtocol>>,
    raw_images: HashMap<String, DynamicImage>,
    img_area: Rect,
}

impl ImageState {
    pub fn insert_manga(&mut self, protocol: Box<dyn StatefulProtocol>, id_manga: String) {
        self.image_state.insert(id_manga, protocol);
    }

    pub fn insert_manga_with_raw(&mut self, protocol: Box<dyn StatefulProtocol>, raw: DynamicImage, id_manga: String) {
        self.raw_images.insert(id_manga.clone(), raw);
        self.image_state.insert(id_manga, protocol);
    }

    pub fn rebuild_protocols(&mut self, picker: Option<&mut Picker>) {
        self.image_state.clear();
        if let Some(picker) = picker {
            for (id, raw) in &self.raw_images {
                self.image_state.insert(id.clone(), picker.new_resize_protocol(raw.clone()));
            }
        }
    }

    pub fn get_img_area(&self) -> Rect {
        self.img_area
    }

    /// After a manga is rendered it will be know what area the covers fits into
    pub fn set_area(&mut self, area: Rect) {
        self.img_area = area;
    }

    /// get the image cover state given the manga id
    pub fn get_image_state(&mut self, id: &str) -> Option<&mut Box<dyn StatefulProtocol>> {
        self.image_state.get_mut(id)
    }

    pub fn has_image(&self, id: &str) -> bool {
        self.image_state.contains_key(id)
    }

    pub fn is_empty(&self) -> bool {
        self.image_state.is_empty()
    }
}

/// Resets every cell inside `canvas` to a visible space with `margin_fg` (`skip = false`)
/// so ratatui overwrites any previous Sixel/graphical cells cleanly.
pub fn clear_image_canvas(canvas: Rect, buf: &mut Buffer, margin_fg: Color) {
    for y in canvas.top()..canvas.bottom() {
        for x in canvas.left()..canvas.right() {
            let cell = &mut buf[(x, y)];
            cell.reset();
            cell.set_symbol(" ").set_fg(margin_fg);
        }
    }
}

/// Renders a `StatefulImage` into `draw_area` and atomically clears the full `canvas`:
/// - For escape-based protocols (Sixel/Kitty/iTerm2), prepends ANSI row-clearing blanks before the escape sequence so the image is
///   cleared and redrawn in one frame without full-screen flicker or ghost pixels.
/// - For character-cell protocols (Halfblocks), clears cells in `canvas` outside `draw_area` using `margin_fg`.
pub fn render_stateful_image_cleared(
    protocol: &mut Box<dyn StatefulProtocol>,
    canvas: Rect,
    draw_area: Rect,
    buf: &mut Buffer,
    margin_fg: Color,
) {
    if canvas.width == 0 || canvas.height == 0 || draw_area.width == 0 || draw_area.height == 0 {
        return;
    }

    let filter = MangaTuiConfig::get().image_resize_filter.to_filter_type();
    let image = StatefulImage::new(None).resize(Resize::Fit(Some(filter)));
    StatefulWidget::render(image, draw_area, buf, protocol);

    let is_graphical_escape =
        canvas.contains(Position::new(draw_area.x, draw_area.y)) && buf[(draw_area.x, draw_area.y)].symbol().starts_with('\x1b');

    if is_graphical_escape {
        let reaches_right_edge = canvas.right() >= buf.area.right();
        let mut prefix = String::with_capacity(
            (canvas.height as usize) * (if reaches_right_edge { 16 } else { canvas.width as usize + 12 }) + 24,
        );
        prefix.push_str("\x1b[0m");
        if reaches_right_edge {
            for row in canvas.top()..canvas.bottom() {
                let _ = write!(prefix, "\x1b[{};{}H\x1b[K", row + 1, canvas.x + 1);
            }
        } else {
            let blanks = " ".repeat(canvas.width as usize);
            for row in canvas.top()..canvas.bottom() {
                let _ = write!(prefix, "\x1b[{};{}H{blanks}", row + 1, canvas.x + 1);
            }
        }
        let _ = write!(prefix, "\x1b[{};{}H", draw_area.y + 1, draw_area.x + 1);
        let combined = format!("{prefix}{}", buf[(draw_area.x, draw_area.y)].symbol());
        let origin = &mut buf[(draw_area.x, draw_area.y)];
        origin.set_style(Style::reset());
        origin.set_symbol(&combined);
        origin.set_skip(false);

        for y in canvas.top()..canvas.bottom() {
            for x in canvas.left()..canvas.right() {
                if x != draw_area.x || y != draw_area.y {
                    let cell = &mut buf[(x, y)];
                    cell.reset();
                    cell.set_skip(true);
                }
            }
        }
    } else {
        for y in canvas.top()..canvas.bottom() {
            for x in canvas.left()..canvas.right() {
                if !draw_area.contains(Position::new(x, y)) {
                    buf[(x, y)].set_symbol(" ").set_fg(margin_fg);
                }
            }
        }
    }
}

pub fn format_error_message_tracking_reading_history<A: Display, B: Display, C: Display>(
    chapter: A,
    manga_title: B,
    error: C,
) -> String {
    format!(
        "Could not track reading progress of chapter : {chapter} \n of manga : {manga_title}, more details about the error : \n ERROR | {error}"
    )
}

#[cfg(test)]
mod tests {
    use ratatui_image::picker::ProtocolType;

    use super::*;

    #[test]
    fn image_state_rebuild_protocols_switches_and_clears() {
        let mut state = ImageState::default();
        let mut picker_hb = Picker::new((8, 16));
        picker_hb.protocol_type = ProtocolType::Halfblocks;

        let raw = DynamicImage::new_rgb8(64, 96);
        let proto = picker_hb.new_resize_protocol(raw.clone());
        state.insert_manga_with_raw(proto, raw, "manga-1".to_string());
        assert!(state.has_image("manga-1"));

        // Rebuilding with None clears active protocols while preserving raw_images
        state.rebuild_protocols(None);
        assert!(!state.has_image("manga-1"));
        assert!(state.is_empty());

        // Rebuilding with Sixel picker restores protocols without refetching
        let mut picker_sixel = Picker::new((10, 22));
        picker_sixel.protocol_type = ProtocolType::Sixel;
        state.rebuild_protocols(Some(&mut picker_sixel));
        assert!(state.has_image("manga-1"));
    }

    #[test]
    fn clear_image_canvas_resets_skip_cells() {
        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(area);
        buf[(3, 2)].set_skip(true);
        buf[(3, 2)].set_symbol("X");

        clear_image_canvas(Rect::new(2, 1, 4, 3), &mut buf, Color::Rgb(1, 1, 1));
        assert!(!buf[(3, 2)].skip);
        assert_eq!(buf[(3, 2)].symbol(), " ");
        assert_eq!(buf[(3, 2)].fg, Color::Rgb(1, 1, 1));
        // Outside canvas remains untouched
        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(0, 0)].fg, Color::Reset);
    }

    #[test]
    fn render_stateful_image_cleared_prepends_canvas_clear_for_sixel() {
        let mut picker = Picker::new((10, 22));
        picker.protocol_type = ProtocolType::Sixel;
        let mut proto = picker.new_resize_protocol(DynamicImage::new_rgb8(200, 300));

        let canvas = Rect::new(5, 2, 20, 10);
        let draw_area = Rect::new(8, 3, 12, 8);
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 15));

        render_stateful_image_cleared(&mut proto, canvas, draw_area, &mut buf, Color::Rgb(1, 1, 1));

        let sym = buf[(draw_area.x, draw_area.y)].symbol();
        assert!(sym.starts_with("\x1b[0m\x1b[3;6H"), "Expected ANSI canvas clear prefix, got: {sym:?}");
        assert!(sym.contains("\x1b[4;9H"), "Expected cursor reposition to draw_area origin");
        assert!(!buf[(draw_area.x, draw_area.y)].skip);
        assert!(buf[(canvas.x, canvas.y)].skip);
        assert!(buf[(canvas.right() - 1, canvas.bottom() - 1)].skip);
    }

    #[test]
    fn render_stateful_image_cleared_uses_el_on_right_edge_canvas_to_avoid_bottom_right_wrap() {
        let mut picker = Picker::new((9, 18));
        picker.protocol_type = ProtocolType::Sixel;
        let mut proto = picker.new_resize_protocol(DynamicImage::new_rgb8(400, 600));

        let full_area = Rect::new(0, 0, 40, 20);
        let draw_area = Rect::new(8, 0, 24, 19);
        let mut buf = Buffer::empty(full_area);

        render_stateful_image_cleared(&mut proto, full_area, draw_area, &mut buf, Color::Rgb(1, 1, 1));

        let sym = buf[(draw_area.x, draw_area.y)].symbol();
        assert!(sym.starts_with("\x1b[0m\x1b[1;1H\x1b[K"), "Expected CSI EL row clear prefix, got: {sym:?}");
        assert!(sym.contains("\x1b[20;1H\x1b[K"), "Final row should use CSI EL instead of trailing spaces");
        assert!(sym.contains("\x1b[1;9H"), "Expected cursor reposition to draw_area origin");
        assert!(!sym.contains(&" ".repeat(40)), "Full-width canvas must not emit trailing spaces into the right edge");
        assert!(!buf[(draw_area.x, draw_area.y)].skip);
        assert!(buf[(full_area.right() - 1, full_area.bottom() - 1)].skip);
    }
}
