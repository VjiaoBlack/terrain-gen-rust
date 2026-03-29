use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Option<Color>,
}

impl Cell {
    pub fn blank() -> Self {
        Cell {
            ch: ' ',
            fg: Color(255, 255, 255),
            bg: None,
        }
    }
}

pub trait Renderer {
    fn size(&self) -> (u16, u16);
    fn draw(&mut self, x: u16, y: u16, ch: char, fg: Color, bg: Option<Color>);
    fn get_cell(&self, x: u16, y: u16) -> Option<&Cell>;
    fn clear(&mut self);
    fn flush(&mut self) -> Result<()>;
}
