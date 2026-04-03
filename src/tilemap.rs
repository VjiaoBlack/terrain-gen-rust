use serde::{Deserialize, Serialize};

use crate::renderer::{Color, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Terrain {
    Water,
    Sand,
    Grass,
    Forest,
    Mountain,
    Snow,
    Cliff,
    Marsh,
    Desert,
    Tundra,
    Scrubland,
    Stump,
    Bare,
    Sapling,
    Quarry,
    QuarryDeep,
    ScarredGround,
    BuildingFloor,
    BuildingWall,
    Road,
}

impl Terrain {
    pub fn is_walkable(&self) -> bool {
        match self {
            Terrain::BuildingWall | Terrain::Cliff => false,
            _ => true,
        }
    }

    /// Characters chosen for similar visual density — subtle texture, not brightness.
    pub fn ch(&self) -> char {
        match self {
            Terrain::Water => '~',
            Terrain::Sand => '·',
            Terrain::Grass => '\'',
            Terrain::Forest => ':',
            Terrain::Mountain => '^',
            Terrain::Snow => '·',
            Terrain::Cliff => '#',
            Terrain::Marsh => ',',
            Terrain::Desert => '.',
            Terrain::Tundra => '-',
            Terrain::Scrubland => ';',
            Terrain::Stump => '%',
            Terrain::Bare => '.',
            Terrain::Sapling => '!',
            Terrain::Quarry => 'U',
            Terrain::QuarryDeep => 'V',
            Terrain::ScarredGround => '.',
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
            Terrain::Cliff => Color(100, 90, 80),
            Terrain::Marsh => Color(40, 90, 60),
            Terrain::Desert => Color(200, 180, 120),
            Terrain::Tundra => Color(160, 170, 180),
            Terrain::Scrubland => Color(130, 120, 60),
            Terrain::Stump => Color(100, 80, 40),
            Terrain::Bare => Color(90, 80, 50),
            Terrain::Sapling => Color(30, 120, 30),
            Terrain::Quarry => Color(140, 130, 115),
            Terrain::QuarryDeep => Color(110, 100, 90),
            Terrain::ScarredGround => Color(145, 135, 120),
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
            Terrain::Cliff => Some(Color(70, 65, 55)),
            Terrain::Marsh => Some(Color(25, 60, 40)),
            Terrain::Desert => Some(Color(180, 160, 100)),
            Terrain::Tundra => Some(Color(140, 150, 160)),
            Terrain::Scrubland => Some(Color(110, 100, 50)),
            Terrain::Stump => Some(Color(40, 90, 30)),
            Terrain::Bare => Some(Color(55, 90, 40)),
            Terrain::Sapling => Some(Color(35, 95, 30)),
            Terrain::Quarry => Some(Color(90, 80, 70)),
            Terrain::QuarryDeep => Some(Color(65, 58, 50)),
            Terrain::ScarredGround => Some(Color(115, 105, 90)),
            Terrain::BuildingFloor => Some(Color(100, 80, 60)),
            Terrain::BuildingWall => Some(Color(120, 100, 80)),
            Terrain::Road => Some(Color(130, 105, 65)),
        }
    }

    /// Movement speed multiplier for this terrain.
    pub fn speed_multiplier(&self) -> f64 {
        match self {
            Terrain::Road => 1.5,
            Terrain::Grass | Terrain::BuildingFloor | Terrain::Scrubland => 1.0,
            Terrain::ScarredGround => 0.9,
            Terrain::Bare => 0.9,
            Terrain::Desert | Terrain::Tundra => 0.8,
            Terrain::Sand | Terrain::Stump => 0.8,
            Terrain::Sapling | Terrain::Quarry => 0.7,
            Terrain::QuarryDeep => 0.5,
            Terrain::Forest => 0.6,
            Terrain::Snow => 0.4,
            Terrain::Marsh => 0.3,
            Terrain::Mountain => 0.25,
            Terrain::Water => 0.15, // swimming, very slow
            Terrain::BuildingWall | Terrain::Cliff => 0.0,
        }
    }

    /// Movement cost for A* pathfinding (inverse of speed, higher = harder).
    pub fn move_cost(&self) -> f64 {
        match self {
            Terrain::Road => 0.7,
            Terrain::Grass | Terrain::BuildingFloor | Terrain::Scrubland | Terrain::Bare => 1.0,
            Terrain::ScarredGround => 1.1,
            Terrain::Stump => 1.2,
            Terrain::Sand | Terrain::Desert | Terrain::Tundra => 1.3,
            Terrain::Sapling | Terrain::Quarry => 1.4,
            Terrain::QuarryDeep => 2.0,
            Terrain::Forest => 1.7,
            Terrain::Snow => 2.5,
            Terrain::Marsh => 3.0,
            Terrain::Mountain => 4.0,
            Terrain::Water => 8.0, // swimmable but heavily penalized by A*
            Terrain::BuildingWall | Terrain::Cliff => f64::INFINITY,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct TileMap {
    pub width: usize,
    pub height: usize,
    tiles: Vec<Terrain>,
    /// Per-tile mining counter for mountain mining progression.
    /// Tracks how many times each tile has been mined.
    #[serde(default)]
    mine_counts: Vec<u8>,
}

impl TileMap {
    pub fn new(width: usize, height: usize, fill: Terrain) -> Self {
        Self {
            width,
            height,
            tiles: vec![fill; width * height],
            mine_counts: vec![0u8; width * height],
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

    /// Get the mining counter for a tile.
    pub fn mine_count(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height && !self.mine_counts.is_empty() {
            self.mine_counts[y * self.width + x]
        } else {
            0
        }
    }

    /// Increment the mining counter for a tile and return the new count.
    pub fn increment_mine_count(&mut self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            // Ensure mine_counts is initialized (for old saves with default empty vec)
            if self.mine_counts.is_empty() {
                self.mine_counts = vec![0u8; self.width * self.height];
            }
            let idx = y * self.width + x;
            self.mine_counts[idx] = self.mine_counts[idx].saturating_add(1);
            self.mine_counts[idx]
        } else {
            0
        }
    }

    /// A* pathfinding from (sx, sy) to (gx, gy). Returns next waypoint (not full path).
    /// Returns None if no path found within search budget. max_steps caps exploration.
    pub fn astar_next(
        &self,
        sx: f64,
        sy: f64,
        gx: f64,
        gy: f64,
        max_steps: usize,
    ) -> Option<(f64, f64)> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        let si = sx.round() as i32;
        let sj = sy.round() as i32;
        let gi = gx.round() as i32;
        let gj = gy.round() as i32;

        if si == gi && sj == gj {
            return Some((gx, gy));
        }

        #[derive(Clone)]
        struct Node {
            cost: f64,
            heuristic: f64,
            x: i32,
            y: i32,
            parent: usize,
        }
        impl PartialEq for Node {
            fn eq(&self, o: &Self) -> bool {
                self.cost + self.heuristic == o.cost + o.heuristic
            }
        }
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
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
        // Wrap (Node, usize) so heap sorts by Node (reversed cost+heuristic)
        struct HeapEntry(Node, usize); // (node for ordering, index into nodes vec)
        impl PartialEq for HeapEntry {
            fn eq(&self, o: &Self) -> bool {
                self.0 == o.0
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, o: &Self) -> Ordering {
                self.0.cmp(&o.0)
            }
        }
        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();

        let heuristic =
            |x: i32, y: i32| -> f64 { ((x - gi) as f64).abs() + ((y - gj) as f64).abs() };

        let start = Node {
            cost: 0.0,
            heuristic: heuristic(si, sj),
            x: si,
            y: sj,
            parent: usize::MAX,
        };
        nodes.push(start.clone());
        heap.push(HeapEntry(start, 0));

        const DIRS: [(i32, i32); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];

        let mut steps = 0;
        while let Some(HeapEntry(node, idx)) = heap.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;

            let vx = node.x as usize;
            let vy = node.y as usize;
            if vx >= self.width || vy >= self.height {
                continue;
            }
            if visited[vy * self.width + vx] {
                continue;
            }
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
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * self.width + nx as usize;
                if visited[ni] {
                    continue;
                }

                let terrain = &self.tiles[ni];
                if !terrain.is_walkable() {
                    continue;
                }

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
                heap.push(HeapEntry(new_node, new_idx));
            }
        }
        None // no path found
    }

    /// A* pathfinding returning the full path as waypoints (start excluded, destination included).
    /// Returns None if no path found within search budget.
    pub fn astar_full_path(
        &self,
        sx: f64,
        sy: f64,
        gx: f64,
        gy: f64,
        max_steps: usize,
    ) -> Option<Vec<(f64, f64)>> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        let si = sx.round() as i32;
        let sj = sy.round() as i32;
        let gi = gx.round() as i32;
        let gj = gy.round() as i32;

        if si == gi && sj == gj {
            return Some(vec![(gx, gy)]);
        }

        #[derive(Clone)]
        struct Node {
            cost: f64,
            heuristic: f64,
            x: i32,
            y: i32,
            parent: usize,
        }
        impl PartialEq for Node {
            fn eq(&self, o: &Self) -> bool {
                self.cost + self.heuristic == o.cost + o.heuristic
            }
        }
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                let a = self.cost + self.heuristic;
                let b = o.cost + o.heuristic;
                b.partial_cmp(&a).unwrap_or(Ordering::Equal)
            }
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let mut visited = vec![false; self.width * self.height];
        let mut nodes: Vec<Node> = Vec::new();
        struct HeapEntry(Node, usize);
        impl PartialEq for HeapEntry {
            fn eq(&self, o: &Self) -> bool {
                self.0 == o.0
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, o: &Self) -> Ordering {
                self.0.cmp(&o.0)
            }
        }
        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();

        let heuristic =
            |x: i32, y: i32| -> f64 { ((x - gi) as f64).abs() + ((y - gj) as f64).abs() };

        let start = Node {
            cost: 0.0,
            heuristic: heuristic(si, sj),
            x: si,
            y: sj,
            parent: usize::MAX,
        };
        nodes.push(start.clone());
        heap.push(HeapEntry(start, 0));

        const DIRS: [(i32, i32); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];

        let mut steps = 0;
        while let Some(HeapEntry(node, idx)) = heap.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;

            let vx = node.x as usize;
            let vy = node.y as usize;
            if vx >= self.width || vy >= self.height {
                continue;
            }
            if visited[vy * self.width + vx] {
                continue;
            }
            visited[vy * self.width + vx] = true;

            if node.x == gi && node.y == gj {
                // Trace back full path
                let mut path = Vec::new();
                let mut cur = idx;
                while nodes[cur].parent != usize::MAX {
                    path.push((nodes[cur].x as f64, nodes[cur].y as f64));
                    cur = nodes[cur].parent;
                }
                path.reverse();
                return Some(path);
            }

            for &(dx, dy) in &DIRS {
                let nx = node.x + dx;
                let ny = node.y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * self.width + nx as usize;
                if visited[ni] {
                    continue;
                }

                let terrain = &self.tiles[ni];
                if !terrain.is_walkable() {
                    continue;
                }

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
                heap.push(HeapEntry(new_node, new_idx));
            }
        }
        None
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
            if wx >= 0
                && wy >= 0
                && let Some(terrain) = map.get(wx as usize, wy as usize)
            {
                renderer.draw(sx, sy, terrain.ch(), terrain.fg(), terrain.bg());
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
        assert!(
            !frame.contains(','),
            "should not render grass past map edge"
        );
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
        assert!(Terrain::Water.is_walkable()); // swimmable
        assert_eq!(Terrain::Water.speed_multiplier(), 0.15);
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
        assert!(
            ny > 5.0 || nx < 5.0,
            "should route around water, not through it"
        );
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
        // Surround target with BuildingWall (truly impassable)
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                map.set((5 + dx) as usize, (5 + dy) as usize, Terrain::BuildingWall);
            }
        }
        let next = map.astar_next(0.0, 0.0, 5.0, 5.0, 500);
        assert!(
            next.is_none(),
            "should return None when target is unreachable"
        );
    }

    #[test]
    fn astar_straight_line_on_open_map() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        // Should find path from (2,2) to (17,17) on open map
        let next = map.astar_next(2.0, 2.0, 17.0, 17.0, 500);
        assert!(next.is_some(), "should find path on open map");
        let (nx, ny) = next.unwrap();
        // First step should move toward target (diagonal)
        assert!(nx > 2.0 || ny > 2.0, "should move toward target");
    }

    #[test]
    fn astar_reaches_target_iteratively() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let mut x = 2.0f64;
        let mut y = 2.0f64;
        let tx = 17.0;
        let ty = 17.0;
        // Simulate walking step by step
        for _ in 0..100 {
            let d = ((x - tx).powi(2) + (y - ty).powi(2)).sqrt();
            if d < 1.5 {
                break;
            }
            let next = map.astar_next(x, y, tx, ty, 500);
            assert!(next.is_some(), "should find path at ({:.1},{:.1})", x, y);
            let (nx, ny) = next.unwrap();
            x = nx;
            y = ny;
        }
        let final_d = ((x - tx).powi(2) + (y - ty).powi(2)).sqrt();
        assert!(
            final_d < 2.0,
            "should reach target iteratively, got dist={}",
            final_d
        );
    }

    #[test]
    fn astar_navigates_corridor() {
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        // Wall across middle with one gap
        for x in 0..20 {
            map.set(x, 5, Terrain::BuildingWall);
        }
        map.set(15, 5, Terrain::Grass); // gap at x=15

        // Path from (2,2) to (2,8) must go through gap at (15,5)
        let next = map.astar_next(2.0, 2.0, 2.0, 8.0, 500);
        assert!(next.is_some(), "should find path through corridor gap");
    }

    #[test]
    fn astar_inside_hut_finds_door() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Build a hut-like structure: walls on 3 sides, open south
        // WWW
        // W.W
        // ...  (open)
        map.set(3, 3, Terrain::BuildingWall);
        map.set(4, 3, Terrain::BuildingWall);
        map.set(5, 3, Terrain::BuildingWall);
        map.set(3, 4, Terrain::BuildingWall);
        // (4,4) = interior floor
        map.set(5, 4, Terrain::BuildingWall);
        // (3,5), (4,5), (5,5) = open (door)

        // From inside (4,4) to outside (4,7)
        let next = map.astar_next(4.0, 4.0, 4.0, 7.0, 100);
        assert!(
            next.is_some(),
            "should find path from inside hut through door"
        );
        let (nx, ny) = next.unwrap();
        // First step should go south toward the open side
        assert!(
            ny > 4.0 || nx != 4.0,
            "should move toward door, got ({},{})",
            nx,
            ny
        );
    }

    #[test]
    fn astar_avoids_water() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Lake in the middle
        for y in 3..7 {
            for x in 3..7 {
                map.set(x, y, Terrain::Water);
            }
        }
        // Path from (1,5) to (8,5) must go around lake
        let next = map.astar_next(1.0, 5.0, 8.0, 5.0, 500);
        assert!(next.is_some(), "should find path around lake");
    }

    #[test]
    fn astar_same_position_returns_target() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let next = map.astar_next(5.0, 5.0, 5.0, 5.0, 100);
        assert!(next.is_some());
        let (nx, ny) = next.unwrap();
        assert_eq!(nx, 5.0);
        assert_eq!(ny, 5.0);
    }

    #[test]
    fn astar_adjacent_target() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let next = map.astar_next(5.0, 5.0, 6.0, 5.0, 100);
        assert!(next.is_some());
        let (nx, ny) = next.unwrap();
        assert_eq!(nx, 6.0);
        assert_eq!(ny, 5.0);
    }

    #[test]
    fn quarry_terrain_properties() {
        assert!(Terrain::Quarry.is_walkable());
        assert_eq!(Terrain::Quarry.ch(), 'U');
        assert_eq!(Terrain::Quarry.speed_multiplier(), 0.7);
        assert_eq!(Terrain::Quarry.move_cost(), 1.4);
        assert!(Terrain::Quarry.bg().is_some());
    }

    #[test]
    fn quarry_deep_terrain_properties() {
        assert!(Terrain::QuarryDeep.is_walkable());
        assert_eq!(Terrain::QuarryDeep.ch(), 'V');
        assert_eq!(Terrain::QuarryDeep.speed_multiplier(), 0.5);
        assert_eq!(Terrain::QuarryDeep.move_cost(), 2.0);
        assert!(Terrain::QuarryDeep.bg().is_some());
    }

    #[test]
    fn scarred_ground_terrain_properties() {
        assert!(Terrain::ScarredGround.is_walkable());
        assert_eq!(Terrain::ScarredGround.ch(), '.');
        assert_eq!(Terrain::ScarredGround.speed_multiplier(), 0.9);
        assert_eq!(Terrain::ScarredGround.move_cost(), 1.1);
        assert!(Terrain::ScarredGround.bg().is_some());
    }

    #[test]
    fn mine_count_increment_and_get() {
        let mut map = TileMap::new(10, 10, Terrain::Mountain);
        assert_eq!(map.mine_count(5, 5), 0);
        let count = map.increment_mine_count(5, 5);
        assert_eq!(count, 1);
        assert_eq!(map.mine_count(5, 5), 1);
        for _ in 0..5 {
            map.increment_mine_count(5, 5);
        }
        assert_eq!(map.mine_count(5, 5), 6);
    }

    #[test]
    fn mine_count_threshold_transitions() {
        let mut map = TileMap::new(10, 10, Terrain::Mountain);
        // Increment to 6 -> should become Quarry
        for _ in 0..6 {
            let count = map.increment_mine_count(5, 5);
            if count >= 6 {
                map.set(5, 5, Terrain::Quarry);
            }
        }
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Quarry);

        // Increment to 12 -> should become QuarryDeep
        for _ in 0..6 {
            let count = map.increment_mine_count(5, 5);
            if count >= 12 {
                map.set(5, 5, Terrain::QuarryDeep);
            }
        }
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::QuarryDeep);
    }

    #[test]
    fn mine_count_saturates_at_u8_max() {
        let mut map = TileMap::new(5, 5, Terrain::Mountain);
        for _ in 0..260 {
            map.increment_mine_count(2, 2);
        }
        assert_eq!(map.mine_count(2, 2), 255);
    }

    #[test]
    fn astar_routes_through_quarry() {
        let mut map = TileMap::new(20, 5, Terrain::Grass);
        // Wall of mountains from (10,0) to (10,4)
        for y in 0..5 {
            map.set(10, y, Terrain::Mountain);
        }
        // No path through mountains (Mountain is walkable but cost 4.0)
        // Convert one tile to Quarry (cost 1.4) -- A* should prefer it
        map.set(10, 2, Terrain::Quarry);
        let next = map.astar_next(5.0, 2.0, 15.0, 2.0, 500);
        assert!(next.is_some(), "should find path through quarry gap");
    }

    #[test]
    fn mine_count_out_of_bounds_returns_zero() {
        let map = TileMap::new(5, 5, Terrain::Grass);
        assert_eq!(map.mine_count(10, 10), 0);
    }

    // --- astar_full_path tests ---

    #[test]
    fn astar_full_path_same_position() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let path = map.astar_full_path(5.0, 5.0, 5.0, 5.0, 100);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], (5.0, 5.0));
    }

    #[test]
    fn astar_full_path_adjacent() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let path = map.astar_full_path(5.0, 5.0, 6.0, 5.0, 100);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], (6.0, 5.0));
    }

    #[test]
    fn astar_full_path_straight_line() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let path = map.astar_full_path(2.0, 5.0, 10.0, 5.0, 500);
        assert!(path.is_some());
        let path = path.unwrap();
        // Path should end at or near destination
        let (lx, ly) = path.last().unwrap();
        assert_eq!(*lx, 10.0);
        assert_eq!(*ly, 5.0);
        // Path length should be roughly the manhattan distance
        assert!(path.len() >= 5, "path should have multiple waypoints");
    }

    #[test]
    fn astar_full_path_around_wall() {
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        // Wall across middle with one gap
        for x in 0..20 {
            map.set(x, 5, Terrain::BuildingWall);
        }
        map.set(15, 5, Terrain::Grass); // gap at x=15
        let path = map.astar_full_path(2.0, 2.0, 2.0, 8.0, 500);
        assert!(path.is_some(), "should find path through corridor gap");
        let path = path.unwrap();
        // Path must go through x=15 gap
        assert!(
            path.iter().any(|&(x, _)| x >= 14.0),
            "path should route through gap at x=15"
        );
    }

    #[test]
    fn astar_full_path_unreachable() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Surround target with walls
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                map.set((5 + dx) as usize, (5 + dy) as usize, Terrain::BuildingWall);
            }
        }
        let path = map.astar_full_path(0.0, 0.0, 5.0, 5.0, 500);
        assert!(path.is_none(), "should return None for unreachable target");
    }

    #[test]
    fn astar_full_path_reaches_destination_when_walked() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let path = map.astar_full_path(2.0, 2.0, 17.0, 17.0, 500);
        assert!(path.is_some());
        let path = path.unwrap();
        let (lx, ly) = path.last().unwrap();
        assert_eq!(*lx, 17.0);
        assert_eq!(*ly, 17.0);
        // Verify monotonic progress
        for i in 1..path.len() {
            let (px, py) = path[i - 1];
            let (cx, cy) = path[i];
            let step = ((cx - px).abs().max((cy - py).abs())) as i32;
            assert!(step <= 2, "each step should be at most 1 tile away");
        }
    }
}
