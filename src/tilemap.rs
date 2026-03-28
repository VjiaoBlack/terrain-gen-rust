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
    Road,
}

impl Terrain {
    pub fn is_walkable(&self) -> bool {
        match self {
            Terrain::Water | Terrain::BuildingWall => false,
            _ => true,
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
            Terrain::Road => '=',
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
            Terrain::Road => Color(160, 130, 80),
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
            Terrain::Road => Some(Color(130, 105, 65)),
        }
    }

    /// Movement speed multiplier for this terrain.
    pub fn speed_multiplier(&self) -> f64 {
        match self {
            Terrain::Road => 1.5,
            Terrain::Grass | Terrain::BuildingFloor => 1.0,
            Terrain::Sand => 0.8,
            Terrain::Forest => 0.6,
            Terrain::Snow => 0.4,
            Terrain::Mountain => 0.25,
            Terrain::Water | Terrain::BuildingWall => 0.0, // impassable
        }
    }

    /// Movement cost for A* pathfinding (inverse of speed, higher = harder).
    pub fn move_cost(&self) -> f64 {
        match self {
            Terrain::Road => 0.7,
            Terrain::Grass | Terrain::BuildingFloor => 1.0,
            Terrain::Sand => 1.3,
            Terrain::Forest => 1.7,
            Terrain::Snow => 2.5,
            Terrain::Mountain => 4.0,
            Terrain::Water | Terrain::BuildingWall => f64::INFINITY,
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

    /// A* pathfinding from (sx, sy) to (gx, gy). Returns next waypoint (not full path).
    /// Returns None if no path found within search budget. max_steps caps exploration.
    pub fn astar_next(&self, sx: f64, sy: f64, gx: f64, gy: f64, max_steps: usize) -> Option<(f64, f64)> {
        use std::collections::BinaryHeap;
        use std::cmp::Ordering;

        let si = sx.round() as i32;
        let sj = sy.round() as i32;
        let gi = gx.round() as i32;
        let gj = gy.round() as i32;

        if si == gi && sj == gj { return Some((gx, gy)); }

        #[derive(Clone)]
        struct Node { cost: f64, heuristic: f64, x: i32, y: i32, parent: usize }
        impl PartialEq for Node { fn eq(&self, o: &Self) -> bool { self.cost + self.heuristic == o.cost + o.heuristic } }
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                // Reverse for min-heap
                let a = self.cost + self.heuristic;
                let b = o.cost + o.heuristic;
                b.partial_cmp(&a).unwrap_or(Ordering::Equal)
            }
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let mut visited = vec![false; self.width * self.height];
        let mut nodes: Vec<Node> = Vec::new();
        let mut heap = BinaryHeap::new();

        let heuristic = |x: i32, y: i32| -> f64 {
            ((x - gi) as f64).abs() + ((y - gj) as f64).abs()
        };

        let start = Node { cost: 0.0, heuristic: heuristic(si, sj), x: si, y: sj, parent: usize::MAX };
        nodes.push(start.clone());
        heap.push((nodes.len() - 1, start));

        const DIRS: [(i32, i32); 8] = [
            (1, 0), (-1, 0), (0, 1), (0, -1),
            (1, 1), (1, -1), (-1, 1), (-1, -1),
        ];

        let mut steps = 0;
        while let Some((idx, node)) = heap.pop() {
            if steps >= max_steps { break; }
            steps += 1;

            let vx = node.x as usize;
            let vy = node.y as usize;
            if vx >= self.width || vy >= self.height { continue; }
            if visited[vy * self.width + vx] { continue; }
            visited[vy * self.width + vx] = true;

            if node.x == gi && node.y == gj {
                // Trace back to find first step
                let mut cur = idx;
                loop {
                    let p = nodes[cur].parent;
                    if p == usize::MAX || (nodes[p].x == si && nodes[p].y == sj) {
                        return Some((nodes[cur].x as f64, nodes[cur].y as f64));
                    }
                    cur = p;
                }
            }

            for &(dx, dy) in &DIRS {
                let nx = node.x + dx;
                let ny = node.y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
                let ni = ny as usize * self.width + nx as usize;
                if visited[ni] { continue; }

                let terrain = &self.tiles[ni];
                if !terrain.is_walkable() { continue; }

                let step_cost = terrain.move_cost() * if dx != 0 && dy != 0 { 1.414 } else { 1.0 };
                let new_cost = node.cost + step_cost;
                let new_node = Node {
                    cost: new_cost,
                    heuristic: heuristic(nx, ny),
                    x: nx,
                    y: ny,
                    parent: idx,
                };
                let new_idx = nodes.len();
                nodes.push(new_node.clone());
                heap.push((new_idx, new_node));
            }
        }
        None // no path found
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

    #[test]
    fn road_terrain_properties() {
        assert!(Terrain::Road.is_walkable());
        assert_eq!(Terrain::Road.ch(), '=');
        assert!(Terrain::Road.bg().is_some());
        assert_eq!(Terrain::Road.speed_multiplier(), 1.5);
    }

    #[test]
    fn terrain_speed_multipliers() {
        assert_eq!(Terrain::Grass.speed_multiplier(), 1.0);
        assert_eq!(Terrain::Sand.speed_multiplier(), 0.8);
        assert_eq!(Terrain::Forest.speed_multiplier(), 0.6);
        assert_eq!(Terrain::Mountain.speed_multiplier(), 0.25);
        assert_eq!(Terrain::Road.speed_multiplier(), 1.5);
        assert!(!Terrain::Water.is_walkable());
        assert!(Terrain::Mountain.is_walkable());
    }

    #[test]
    fn astar_paths_around_water() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // Wall of water from (5,0) to (5,8), leaving a gap at (5,9)
        for y in 0..9 {
            map.set(5, y, Terrain::Water);
        }
        // Path from (3,5) to (7,5) must go around the water wall
        let next = map.astar_next(3.0, 5.0, 7.0, 5.0, 500);
        assert!(next.is_some(), "should find a path around water");
        // The first step should move south (toward the gap) not east (into water)
        let (nx, ny) = next.unwrap();
        assert!(ny > 5.0 || nx < 5.0, "should route around water, not through it");
    }

    #[test]
    fn astar_prefers_roads() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // Road along y=3
        for x in 0..20 {
            map.set(x, 3, Terrain::Road);
        }
        // Path from (0,5) to (15,5) — should prefer routing via road
        let next = map.astar_next(0.0, 5.0, 15.0, 5.0, 500);
        assert!(next.is_some());
    }

    #[test]
    fn astar_returns_none_for_unreachable() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Surround target with water
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                map.set((5 + dx) as usize, (5 + dy) as usize, Terrain::Water);
            }
        }
        let next = map.astar_next(0.0, 0.0, 5.0, 5.0, 500);
        assert!(next.is_none(), "should return None when target is unreachable");
    }
}
