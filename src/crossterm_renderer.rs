use anyhow::Result;
use crossterm::{
    cursor,
    event::DisableMouseCapture,
    event::EnableMouseCapture,
    execute, queue,
    style::{Color as CColor, Colors, ResetColor, SetColors},
    terminal,
};
use std::io::{self, Stdout, Write};

use crate::renderer::{Cell, Color, Renderer};

pub struct CrosstermRenderer {
    stdout: Stdout,
    width: u16,
    height: u16,
    back: Vec<Cell>,  // what we drew last frame
    front: Vec<Cell>, // what we want to draw this frame
}

impl CrosstermRenderer {
    pub fn new() -> Result<Self> {
        let (width, height) = terminal::size()?;
        let blank = Cell::blank();
        let buf = vec![blank; (width * height) as usize];
        let mut stdout = io::stdout();

        terminal::enable_raw_mode()?;
        execute!(
            stdout,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            EnableMouseCapture
        )?;
        // clear the alternate screen with the default background
        execute!(
            stdout,
            ResetColor,
            terminal::Clear(terminal::ClearType::All)
        )?;

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
        let _ = execute!(
            self.stdout,
            DisableMouseCapture,
            ResetColor,
            terminal::LeaveAlternateScreen,
            cursor::Show
        );
        let _ = terminal::disable_raw_mode();
    }
}

fn to_ccolor(c: Color) -> CColor {
    CColor::Rgb {
        r: c.0,
        g: c.1,
        b: c.2,
    }
}

impl CrosstermRenderer {
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let blank = Cell::blank();
        let buf = vec![blank; (width * height) as usize];
        self.back = buf.clone();
        self.front = buf;
        // force full redraw
        let _ = execute!(
            self.stdout,
            ResetColor,
            terminal::Clear(terminal::ClearType::All)
        );
    }
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

    fn get_cell(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.front[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    fn clear(&mut self) {
        let blank = Cell::blank();
        self.front.fill(blank);
    }

    fn flush(&mut self) -> Result<()> {
        // Batch consecutive changed cells into single writes.
        // Track current cursor position and colors to skip redundant commands.
        let mut cur_x: i32 = -1;
        let mut cur_y: i32 = -1;
        let mut cur_fg = CColor::Reset;
        let mut cur_bg = CColor::Reset;

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = (y * self.width + x) as usize;
                let cell = self.front[idx];
                if cell == self.back[idx] {
                    continue;
                }

                // Move cursor if not already at the right position
                if x as i32 != cur_x || y as i32 != cur_y {
                    queue!(self.stdout, cursor::MoveTo(x, y))?;
                }

                // Set colors only if changed
                let fg = to_ccolor(cell.fg);
                let bg = match cell.bg {
                    Some(c) => to_ccolor(c),
                    None => CColor::Reset,
                };
                if fg != cur_fg || bg != cur_bg {
                    queue!(self.stdout, SetColors(Colors::new(fg, bg)))?;
                    cur_fg = fg;
                    cur_bg = bg;
                }

                write!(self.stdout, "{}", cell.ch)?;
                self.back[idx] = cell;

                // After writing one char, cursor advances by 1
                cur_x = x as i32 + 1;
                cur_y = y as i32;
            }
        }
        self.stdout.flush()?;
        Ok(())
    }
}
