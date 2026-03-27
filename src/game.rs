use anyhow::Result;
use serde::Serialize;

use crate::renderer::{Cell, Color, Renderer};
use crate::headless_renderer::HeadlessRenderer;

#[derive(Clone, Debug, Serialize)]
pub struct FrameSnapshot {
    pub tick: u64,
    pub width: u16,
    pub height: u16,
    pub text: String,
    pub cells: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameInput {
    Quit,
    FpsUp,
    FpsDown,
    None,
}

pub struct Game {
    pub target_fps: u32,
    pub tick: u64,
}

impl Game {
    pub fn new(target_fps: u32) -> Self {
        Self { target_fps, tick: 0 }
    }

    /// Process input and advance one tick. Renders to the given renderer.
    pub fn step(&mut self, input: GameInput, renderer: &mut dyn Renderer) -> Result<()> {
        match input {
            GameInput::FpsUp => self.target_fps = (self.target_fps + 5).min(120),
            GameInput::FpsDown => self.target_fps = self.target_fps.saturating_sub(5).max(1),
            GameInput::Quit | GameInput::None => {}
        }

        self.tick += 1;

        renderer.clear();
        self.draw(renderer);
        renderer.flush()?;
        Ok(())
    }

    /// Step and return a serializable snapshot. Only works with HeadlessRenderer.
    pub fn step_headless(&mut self, input: GameInput, renderer: &mut HeadlessRenderer) -> Result<FrameSnapshot> {
        self.step(input, renderer)?;
        Ok(self.snapshot(renderer))
    }

    pub fn draw(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();

        // bouncing block demo
        let block_w = 8u16;
        let block_h = 4u16;
        let range_x = w.saturating_sub(block_w).max(1);
        let range_y = h.saturating_sub(block_h + 1).max(1);
        let bx = (self.tick as u16) % (range_x * 2);
        let by = (self.tick as u16) % (range_y * 2);
        let bx = if bx >= range_x { range_x * 2 - bx } else { bx };
        let by = if by >= range_y { range_y * 2 - by } else { by };

        let hue = (self.tick % 360) as f64;
        let (r, g, b) = hue_to_rgb(hue);

        for dy in 0..block_h {
            for dx in 0..block_w {
                renderer.draw(bx + dx, by + dy, '█', Color(r, g, b), None);
            }
        }

        // status line at bottom
        let status = format!(
            " tick: {}  fps: {} (up/down to change)  q to quit ",
            self.tick, self.target_fps
        );
        for (i, ch) in status.chars().enumerate() {
            if (i as u16) < w {
                renderer.draw(i as u16, h - 1, ch, Color(0, 0, 0), Some(Color(200, 200, 200)));
            }
        }
    }

    fn snapshot(&self, renderer: &HeadlessRenderer) -> FrameSnapshot {
        let (w, h) = renderer.size();
        let mut cells = Vec::with_capacity(h as usize);
        for y in 0..h {
            let mut row = Vec::with_capacity(w as usize);
            for x in 0..w {
                row.push(*renderer.get_cell(x, y).unwrap());
            }
            cells.push(row);
        }
        FrameSnapshot {
            tick: self.tick,
            width: w,
            height: h,
            text: renderer.frame_as_string(),
            cells,
        }
    }
}

fn hue_to_rgb(hue: f64) -> (u8, u8, u8) {
    let h = (hue % 360.0) / 60.0;
    let c = 255.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r as u8, g as u8, b as u8)
}
