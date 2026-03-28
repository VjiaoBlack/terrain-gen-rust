use serde::{Serialize, Deserialize};

use crate::renderer::{Color, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Terrain {
    Water,
    Sand,
    Grass,
    Forest,
    Mountain,
    Snow,
    BuildingFloor,
    BuildingWall,
}

impl Terrain {
    pub fn is_walkable(&self) -> bool {
        match self {
            Terrain::Water | Terrain::Mountain | Terrain::Snow | Terrain::BuildingWall => false,
            Terrain::Sand | Terrain::Grass | Terrain::Forest | Terrain::BuildingFloor => true,
        }
    }

    /// Characters chosen for similar visual density — subtle texture, not brightness.
    pub fn ch(&self) -> char {
        match self {
            Terrain::Water => '~',
            Terrain::Sand => '·',      // middle dot: lighter than '.' but visible
            Terrain::Grass => '\'',
            Terrain::Forest => ':',
            Terrain::Mountain => '^',
            Terrain::Snow => '·',
            Terrain::BuildingFloor => '░',
            Terrain::BuildingWall => '█',
        }
    }

    /// Foreground: subtle texture color, close to bg so character density doesn't dominate.
    pub fn fg(&self) -> Color {
        match self {
            Terrain::Water => Color(60, 110, 220),
            Terrain::Sand => Color(190, 165, 90),
            Terrain::Grass => Color(45, 140, 45),
            Terrain::Forest => Color(15, 80, 20),
            Terrain::Mountain => Color(120, 110, 100),
            Terrain::Snow => Color(220, 220, 240),
            Terrain::BuildingFloor => Color(140, 120, 90),
            Terrain::BuildingWall => Color(160, 140, 110),
        }
    }

    /// Background: every terrain gets a bg color so lighting controls perceived brightness.
    pub fn bg(&self) -> Option<Color> {
        match self {
            Terrain::Water => Some(Color(20, 40, 100)),
            Terrain::Sand => Some(Color(170, 145, 80)),
            Terrain::Grass => Some(Color(30, 100, 30)),
            Terrain::Forest => Some(Color(10, 60, 15)),
            Terrain::Mountain => Some(Color(95, 85, 75)),
            Terrain::Snow => Some(Color(200, 200, 215)),
            Terrain::BuildingFloor => Some(Color(100, 80, 60)),
            Terrain::BuildingWall => Some(Color(120, 100, 80)),
        }
    }
}

#[derive(Serialize, Deserialize)]
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

    /// Check if a world position is walkable (in-bounds and walkable terrain).
    pub fn is_walkable(&self, x: f64, y: f64) -> bool {
        let ix = x.round() as i64;
        let iy = y.round() as i64;
        if ix < 0 || iy < 0 {
            return false;
        }
        match self.get(ix as usize, iy as usize) {
            Some(t) => t.is_walkable(),
            None => false, // out of bounds = blocked
        }
    }
}

#[derive(Serialize, Deserialize)]
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
                assert_eq!(ch, '\'', "expected grass char, got '{}'", ch);
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

        assert_eq!(r.get_cell(2, 1).unwrap().ch, '^');
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
        assert_eq!(r.get_cell(1, 0).unwrap().ch, '·');
        assert_eq!(r.get_cell(2, 0).unwrap().ch, ':');
    }
}
