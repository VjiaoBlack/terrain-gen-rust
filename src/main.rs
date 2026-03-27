mod renderer;
mod crossterm_renderer;
mod headless_renderer;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use std::time::{Duration, Instant};

use renderer::{Color, Renderer};
use crossterm_renderer::CrosstermRenderer;

struct GameLoop {
    target_fps: u32,
    tick: u64,
}

impl GameLoop {
    fn new(target_fps: u32) -> Self {
        Self { target_fps, tick: 0 }
    }

    fn frame_duration(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.target_fps as f64)
    }

    fn run(&mut self, renderer: &mut dyn Renderer) -> Result<()> {
        loop {
            let frame_start = Instant::now();

            // input
            if event::poll(Duration::ZERO)? {
                if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                    match code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Up => self.target_fps = (self.target_fps + 5).min(120),
                        KeyCode::Down => self.target_fps = self.target_fps.saturating_sub(5).max(1),
                        _ => {}
                    }
                }
            }

            // update
            self.tick += 1;

            // render
            renderer.clear();
            self.draw(renderer);
            renderer.flush()?;

            // sleep to hit target fps
            let elapsed = frame_start.elapsed();
            if let Some(remaining) = self.frame_duration().checked_sub(elapsed) {
                std::thread::sleep(remaining);
            }
        }
    }

    fn draw(&self, renderer: &mut dyn Renderer) {
        let (w, h) = renderer.size();

        // bouncing block demo
        let block_w = 8u16;
        let block_h = 4u16;
        let range_x = w.saturating_sub(block_w).max(1);
        let range_y = h.saturating_sub(block_h + 1).max(1); // leave room for status line
        let bx = (self.tick as u16) % (range_x * 2);
        let by = (self.tick as u16) % (range_y * 2);
        // triangle wave: bounce back
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

#[cfg(test)]
mod tests {
    use super::*;
    use headless_renderer::HeadlessRenderer;

    #[test]
    fn game_draws_block_on_first_tick() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = GameLoop::new(30);
        game.tick = 1;
        game.draw(&mut r);

        // the block should be somewhere — at least one '█' in the frame
        let frame = r.frame_as_string();
        assert!(frame.contains('█'), "expected block char in frame:\n{}", frame);
    }

    #[test]
    fn game_draws_status_line() {
        let mut r = HeadlessRenderer::new(60, 20);
        let mut game = GameLoop::new(30);
        game.tick = 42;
        game.draw(&mut r);

        let frame = r.frame_as_string();
        assert!(frame.contains("tick: 42"), "expected tick in status line:\n{}", frame);
        assert!(frame.contains("fps: 30"), "expected fps in status line:\n{}", frame);
    }

    #[test]
    fn block_moves_between_ticks() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = GameLoop::new(30);

        game.tick = 1;
        game.draw(&mut r);
        let frame1 = r.frame_as_string();

        r.clear();
        game.tick = 10;
        game.draw(&mut r);
        let frame2 = r.frame_as_string();

        assert_ne!(frame1, frame2, "frames should differ between ticks");
    }

    #[test]
    fn clear_between_frames_removes_old_content() {
        let mut r = HeadlessRenderer::new(40, 20);
        let mut game = GameLoop::new(30);

        game.tick = 1;
        game.draw(&mut r);
        r.clear();

        // after clear, no block chars should remain
        let frame = r.frame_as_string();
        assert!(!frame.contains('█'), "frame should be blank after clear:\n{}", frame);
    }

    #[test]
    fn headless_runs_many_ticks_without_panic() {
        let mut r = HeadlessRenderer::new(80, 24);
        let mut game = GameLoop::new(60);

        for _ in 0..1000 {
            r.clear();
            game.tick += 1;
            game.draw(&mut r);
            let _ = r.flush();
        }

        let frame = r.frame_as_string();
        assert!(frame.contains("tick: 1000"));
    }
}

fn main() -> Result<()> {
    let mut renderer = CrosstermRenderer::new()?;
    let mut game = GameLoop::new(30);
    game.run(&mut renderer)?;
    Ok(())
}
