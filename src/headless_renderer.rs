use anyhow::Result;

use crate::renderer::{Cell, Color, Renderer};

pub struct HeadlessRenderer {
    width: u16,
    height: u16,
    front: Vec<Cell>,
}

impl HeadlessRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        let blank = Cell::blank();
        Self {
            width,
            height,
            front: vec![blank; (width * height) as usize],
        }
    }

    pub fn get_cell(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.front[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    pub fn frame_as_string(&self) -> String {
        let mut out = String::with_capacity((self.width as usize + 1) * self.height as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = &self.front[(y * self.width + x) as usize];
                out.push(cell.ch);
            }
            if y < self.height - 1 {
                out.push('\n');
            }
        }
        out
    }

    /// Render frame with ANSI true-color escape codes for terminal viewing.
    pub fn frame_as_ansi(&self) -> String {
        let mut out = String::with_capacity((self.width as usize * 20 + 1) * self.height as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = &self.front[(y * self.width + x) as usize];
                let Color(fr, fg, fb) = cell.fg;
                out.push_str(&format!("\x1b[38;2;{};{};{}m", fr, fg, fb));
                if let Some(Color(br, bg, bb)) = cell.bg {
                    out.push_str(&format!("\x1b[48;2;{};{};{}m", br, bg, bb));
                }
                out.push(cell.ch);
            }
            out.push_str("\x1b[0m\n");
        }
        out.push_str("\x1b[0m");
        out
    }

    /// Render the frame buffer to a PNG file with bitmap font characters.
    #[cfg(feature = "png")]
    pub fn save_png(&self, path: &str, cell_w: u32, cell_h: u32) -> anyhow::Result<()> {
        use font8x8::UnicodeFonts;

        let img_w = self.width as u32 * cell_w;
        let img_h = self.height as u32 * cell_h;
        let mut img = image::RgbImage::new(img_w, img_h);

        for cy in 0..self.height as u32 {
            for cx in 0..self.width as u32 {
                let cell = &self.front[(cy * self.width as u32 + cx) as usize];
                let Color(br, bg_c, bb) = cell.bg.unwrap_or(Color(0, 0, 0));
                let Color(fr, fg_c, fb) = cell.fg;
                let bg_px = image::Rgb([br, bg_c, bb]);
                let fg_px = image::Rgb([fr, fg_c, fb]);

                // Fill background
                for py in 0..cell_h {
                    for px in 0..cell_w {
                        img.put_pixel(cx * cell_w + px, cy * cell_h + py, bg_px);
                    }
                }

                if cell.ch == ' ' {
                    continue;
                }

                // Try to get glyph from font8x8
                let glyph = font8x8::BASIC_FONTS
                    .get(cell.ch)
                    .or_else(|| font8x8::BLOCK_FONTS.get(cell.ch))
                    .or_else(|| font8x8::BOX_FONTS.get(cell.ch))
                    .or_else(|| font8x8::MISC_FONTS.get(cell.ch));

                if let Some(bitmap) = glyph {
                    // Render 8x8 bitmap scaled to cell size
                    for (row_idx, row) in bitmap.iter().enumerate() {
                        for bit in 0..8u32 {
                            if row & (1 << bit) != 0 {
                                // Scale pixel to cell size
                                let sx = bit * cell_w / 8;
                                let sy = row_idx as u32 * cell_h / 8;
                                let ex = (bit + 1) * cell_w / 8;
                                let ey = (row_idx as u32 + 1) * cell_h / 8;
                                for py in sy..ey {
                                    for px in sx..ex {
                                        img.put_pixel(cx * cell_w + px, cy * cell_h + py, fg_px);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Fallback for Unicode chars without glyphs: draw a centered dot/block
                    let m = cell_w / 3;
                    let my = cell_h / 3;
                    for py in my..cell_h - my {
                        for px in m..cell_w - m {
                            img.put_pixel(cx * cell_w + px, cy * cell_h + py, fg_px);
                        }
                    }
                }
            }
        }

        img.save(path)?;
        Ok(())
    }
}

impl Renderer for HeadlessRenderer {
    fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn draw(&mut self, x: u16, y: u16, ch: char, fg: Color, bg: Option<Color>) {
        if x < self.width && y < self.height {
            self.front[(y * self.width + x) as usize] = Cell { ch, fg, bg };
        }
    }

    fn get_cell(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.front[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    fn clear(&mut self) {
        self.front.fill(Cell::blank());
    }

    fn flush(&mut self) -> Result<()> {
        // no-op — nothing to flush to
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_frame_is_spaces() {
        let r = HeadlessRenderer::new(4, 3);
        assert_eq!(r.frame_as_string(), "    \n    \n    ");
    }

    #[test]
    fn draw_and_read_back() {
        let mut r = HeadlessRenderer::new(10, 5);
        r.draw(2, 1, '#', Color(255, 0, 0), None);
        let cell = r.get_cell(2, 1).unwrap();
        assert_eq!(cell.ch, '#');
        assert_eq!(cell.fg, Color(255, 0, 0));
        assert_eq!(cell.bg, None);
    }

    #[test]
    fn frame_string_reflects_draws() {
        let mut r = HeadlessRenderer::new(5, 3);
        r.draw(0, 0, 'H', Color(255, 255, 255), None);
        r.draw(1, 0, 'i', Color(255, 255, 255), None);
        let frame = r.frame_as_string();
        assert!(frame.starts_with("Hi"));
    }

    #[test]
    fn clear_resets_frame() {
        let mut r = HeadlessRenderer::new(5, 3);
        r.draw(0, 0, 'X', Color(255, 0, 0), None);
        r.clear();
        assert_eq!(r.get_cell(0, 0).unwrap().ch, ' ');
    }

    #[test]
    fn out_of_bounds_draw_is_safe() {
        let mut r = HeadlessRenderer::new(5, 3);
        r.draw(99, 99, 'X', Color(255, 0, 0), None);
        assert!(r.get_cell(99, 99).is_none());
    }
}
