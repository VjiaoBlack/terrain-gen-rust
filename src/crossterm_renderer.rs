use anyhow::Result;
use crossterm::{
    cursor, execute, queue,
    style::{Color as CColor, Colors, ResetColor, SetColors},
    terminal,
};
use std::io::{self, Stdout, Write};

use crate::renderer::{Cell, Color, Renderer};

pub struct CrosstermRenderer {
    stdout: Stdout,
    width: u16,
    height: u16,
    back: Vec<Cell>,   // what we drew last frame
    front: Vec<Cell>,  // what we want to draw this frame
}

impl CrosstermRenderer {
    pub fn new() -> Result<Self> {
        let (width, height) = terminal::size()?;
        let blank = Cell::blank();
        let buf = vec![blank; (width * height) as usize];
        let mut stdout = io::stdout();

        terminal::enable_raw_mode()?;
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
        // clear the alternate screen with the default background
        execute!(stdout, ResetColor, terminal::Clear(terminal::ClearType::All))?;

        Ok(Self {
            stdout,
            width,
            height,
            back: buf.clone(),
            front: buf,
        })
    }
}

impl Drop for CrosstermRenderer {
    fn drop(&mut self) {
        let _ = execute!(self.stdout, ResetColor, terminal::LeaveAlternateScreen, cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

fn to_ccolor(c: Color) -> CColor {
    CColor::Rgb { r: c.0, g: c.1, b: c.2 }
}

impl Renderer for CrosstermRenderer {
    fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn draw(&mut self, x: u16, y: u16, ch: char, fg: Color, bg: Option<Color>) {
        if x < self.width && y < self.height {
            self.front[(y * self.width + x) as usize] = Cell { ch, fg, bg };
        }
    }

    fn clear(&mut self) {
        let blank = Cell::blank();
        self.front.fill(blank);
    }

    fn flush(&mut self) -> Result<()> {
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = (y * self.width + x) as usize;
                let cell = self.front[idx];
                if cell != self.back[idx] {
                    let fg = to_ccolor(cell.fg);
                    let bg = match cell.bg {
                        Some(c) => to_ccolor(c),
                        None => CColor::Reset,
                    };
                    queue!(
                        self.stdout,
                        cursor::MoveTo(x, y),
                        SetColors(Colors::new(fg, bg)),
                    )?;
                    write!(self.stdout, "{}", cell.ch)?;
                    self.back[idx] = cell;
                }
            }
        }
        self.stdout.flush()?;
        Ok(())
    }
}
