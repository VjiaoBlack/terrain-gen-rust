use serde::Serialize;

use crate::renderer::{Color, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Terrain {
    Water,
    Sand,
    Grass,
    Forest,
    Mountain,
    Snow,
}

impl Terrain {
    pub fn ch(&self) -> char {
        match self {
            Terrain::Water => '~',
            Terrain::Sand => '.',
            Terrain::Grass => ',',
            Terrain::Forest => '♣',
            Terrain::Mountain => '▲',
            Terrain::Snow => '▓',
        }
    }

    pub fn fg(&self) -> Color {
        match self {
            Terrain::Water => Color(50, 100, 200),
            Terrain::Sand => Color(210, 180, 100),
            Terrain::Grass => Color(60, 180, 60),
            Terrain::Forest => Color(20, 120, 30),
            Terrain::Mountain => Color(140, 130, 120),
            Terrain::Snow => Color(240, 240, 255),
        }
    }

    pub fn bg(&self) -> Option<Color> {
        match self {
            Terrain::Water => Some(Color(20, 40, 100)),
            _ => None,
        }
    }
}

pub struct TileMap {
    pub width: usize,
    pub height: usize,
    tiles: Vec<Terrain>,
}

impl TileMap {
    pub fn new(width: usize, height: usize, fill: Terrain) -> Self {
        Self {
            width,
            height,
            tiles: vec![fill; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Terrain> {
        if x < self.width && y < self.height {
            Some(&self.tiles[y * self.width + x])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: usize, y: usize, terrain: Terrain) {
        if x < self.width && y < self.height {
            self.tiles[y * self.width + x] = terrain;
        }
    }
}

pub struct Camera {
    pub x: i32,
    pub y: i32,
}

impl Camera {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Clamp camera so viewport stays within map bounds.
    pub fn clamp(&mut self, map_w: usize, map_h: usize, view_w: u16, view_h: u16) {
        let max_x = (map_w as i32) - (view_w as i32);
        let max_y = (map_h as i32) - (view_h as i32);
        self.x = self.x.clamp(0, max_x.max(0));
        self.y = self.y.clamp(0, max_y.max(0));
    }
}

pub fn render_map(map: &TileMap, camera: &Camera, renderer: &mut dyn Renderer) {
    let (vw, vh) = renderer.size();
    for sy in 0..vh {
        for sx in 0..vw {
            let wx = camera.x + sx as i32;
            let wy = camera.y + sy as i32;
            if wx >= 0 && wy >= 0 {
                if let Some(terrain) = map.get(wx as usize, wy as usize) {
                    renderer.draw(sx, sy, terrain.ch(), terrain.fg(), terrain.bg());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_renderer::HeadlessRenderer;

    #[test]
    fn tilemap_new_and_get() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        assert_eq!(*map.get(0, 0).unwrap(), Terrain::Grass);
        assert_eq!(*map.get(9, 9).unwrap(), Terrain::Grass);
        assert!(map.get(10, 10).is_none());
    }

    #[test]
    fn tilemap_set_and_get() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(5, 3, Terrain::Water);
        assert_eq!(*map.get(5, 3).unwrap(), Terrain::Water);
        assert_eq!(*map.get(5, 4).unwrap(), Terrain::Grass);
    }

    #[test]
    fn render_map_draws_terrain() {
        let map = TileMap::new(20, 10, Terrain::Grass);
        let camera = Camera::new(0, 0);
        let mut r = HeadlessRenderer::new(10, 5);
        render_map(&map, &camera, &mut r);

        let frame = r.frame_as_string();
        // every cell should be grass
        for ch in frame.chars() {
            if ch != '\n' {
                assert_eq!(ch, ',', "expected grass char, got '{}'", ch);
            }
        }
    }

    #[test]
    fn render_map_with_camera_offset() {
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        map.set(5, 3, Terrain::Mountain);

        // camera at (3, 2) means world (5,3) maps to screen (2,1)
        let camera = Camera::new(3, 2);
        let mut r = HeadlessRenderer::new(10, 5);
        render_map(&map, &camera, &mut r);

        assert_eq!(r.get_cell(2, 1).unwrap().ch, '▲');
    }

    #[test]
    fn render_map_camera_past_edge_shows_nothing() {
        let map = TileMap::new(5, 5, Terrain::Grass);
        let camera = Camera::new(10, 10);
        let mut r = HeadlessRenderer::new(10, 5);
        render_map(&map, &camera, &mut r);

        // everything should be blank
        let frame = r.frame_as_string();
        assert!(!frame.contains(','), "should not render grass past map edge");
    }

    #[test]
    fn camera_clamp_keeps_in_bounds() {
        let mut camera = Camera::new(-5, -5);
        camera.clamp(100, 100, 20, 10);
        assert_eq!(camera.x, 0);
        assert_eq!(camera.y, 0);

        camera.x = 200;
        camera.y = 200;
        camera.clamp(100, 100, 20, 10);
        assert_eq!(camera.x, 80);
        assert_eq!(camera.y, 90);
    }

    #[test]
    fn mixed_terrain_renders_correctly() {
        let mut map = TileMap::new(5, 3, Terrain::Water);
        map.set(1, 0, Terrain::Sand);
        map.set(2, 0, Terrain::Forest);

        let camera = Camera::new(0, 0);
        let mut r = HeadlessRenderer::new(5, 3);
        render_map(&map, &camera, &mut r);

        assert_eq!(r.get_cell(0, 0).unwrap().ch, '~');
        assert_eq!(r.get_cell(1, 0).unwrap().ch, '.');
        assert_eq!(r.get_cell(2, 0).unwrap().ch, '♣');
    }
}
