use crate::tilemap::{Terrain, TileMap};
use std::collections::VecDeque;

/// Per-tile chokepoint score grid. Higher scores indicate narrower corridors.
/// Scores range from 0.0 (open terrain / barrier) to 1.0 (single-tile gap).
pub struct ChokepointMap {
    pub width: usize,
    pub height: usize,
    /// Per-tile chokepoint score, indexed by `y * width + x`.
    pub scores: Vec<f64>,
    /// Discrete chokepoint clusters detected by flood-fill.
    pub locations: Vec<ChokepointLocation>,
}

/// A discrete chokepoint: a cluster of high-scoring tiles forming a pass, ford, or narrow.
#[derive(Debug, Clone)]
pub struct ChokepointLocation {
    /// Center tile (highest-scoring tile in the cluster).
    pub x: usize,
    pub y: usize,
    /// Corridor width at the narrowest point (tiles).
    pub width: u16,
    /// Primary axis of the corridor (direction traffic flows through).
    pub axis: (i32, i32),
    /// What kind of chokepoint.
    pub kind: ChokepointKind,
    /// The chokepoint score at the center tile.
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChokepointKind {
    /// Narrow pass between mountains/cliffs.
    MountainPass,
    /// Ford or narrow crossing over a river.
    RiverCrossing,
    /// Strip of land between water body and impassable terrain.
    CoastalNarrow,
}

/// Returns true if the terrain type counts as a barrier for chokepoint detection.
/// Mountain is walkable but treated as a barrier because mountain passes ARE chokepoints.
pub fn is_barrier(terrain: Terrain) -> bool {
    matches!(
        terrain,
        Terrain::Water | Terrain::Cliff | Terrain::BuildingWall | Terrain::Mountain
    )
}

/// 8 ray directions: 4 axis-aligned + 4 diagonal.
const DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Cast a ray from (x, y) in direction (dx, dy) until hitting a barrier or map edge.
/// Returns the distance (number of tiles walked before hitting barrier).
/// Early-terminates at distance 9 (optimization: can't produce min_width <= 8).
fn ray_distance(map: &TileMap, x: usize, y: usize, dx: i32, dy: i32) -> u16 {
    let w = map.width;
    let h = map.height;
    let mut dist: u16 = 0;
    loop {
        dist += 1;
        let nx = x as i32 + dx * dist as i32;
        let ny = y as i32 + dy * dist as i32;
        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
            return dist; // map edge counts as barrier
        }
        if let Some(t) = map.get(nx as usize, ny as usize) {
            if is_barrier(*t) {
                return dist;
            }
        } else {
            return dist;
        }
        if dist >= 9 {
            return dist; // early termination: can't produce min_width <= 8
        }
    }
}

/// Compute the chokepoint score for a single tile given its minimum corridor width.
/// Width 1 -> 1.0, width 8 -> 0.125, width > 8 -> 0.0.
pub fn chokepoint_score(min_width: u16) -> f64 {
    if min_width > 8 {
        0.0
    } else {
        1.0 / min_width as f64
    }
}

/// Compute the minimum corridor width at tile (x, y) by casting rays in 4 perpendicular
/// pairs (axis-aligned + diagonal) and taking the minimum width across all pairs.
fn compute_min_width(map: &TileMap, x: usize, y: usize) -> u16 {
    // 4 perpendicular pairs: E+W, N+S, NE+SW, NW+SE
    let _pairs: [(usize, usize); 4] = [
        (0, 1), // E + W
        (2, 3), // S + N
        (4, 7), // NE + NW... no, let's pair opposites:
        (5, 6), // SE(1,-1) + NW(-1,1)... wait
    ];
    // Actually pair opposite directions:
    // E(1,0) + W(-1,0) -> width_EW
    // S(0,1) + N(0,-1) -> width_NS
    // NE(1,1) + SW(-1,-1) -> width_NESW  -- that's indices 4 and 7... let me just be explicit

    let d_e = ray_distance(map, x, y, 1, 0);
    let d_w = ray_distance(map, x, y, -1, 0);
    let d_s = ray_distance(map, x, y, 0, 1);
    let d_n = ray_distance(map, x, y, 0, -1);
    let d_ne = ray_distance(map, x, y, 1, -1);
    let d_sw = ray_distance(map, x, y, -1, 1);
    let d_nw = ray_distance(map, x, y, -1, -1);
    let d_se = ray_distance(map, x, y, 1, 1);

    // Corridor width = sum of opposite rays + 1 (the tile itself)
    let width_ew = d_e + d_w - 1; // -1 because each ray starts at dist=1
    let width_ns = d_n + d_s - 1;
    let width_nesw = d_ne + d_sw - 1;
    let width_nwse = d_nw + d_se - 1;

    width_ew.min(width_ns).min(width_nesw).min(width_nwse)
}

impl ChokepointMap {
    /// Create an empty chokepoint map.
    pub fn empty(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            scores: vec![0.0; width * height],
            locations: Vec::new(),
        }
    }

    /// Compute chokepoint scores for the entire map.
    pub fn compute(map: &TileMap, river_mask: &[bool]) -> Self {
        let width = map.width;
        let height = map.height;
        let n = width * height;
        let mut scores = vec![0.0f64; n];

        // Pass 1: perpendicular ray-cast for every walkable, non-barrier tile
        for y in 0..height {
            for x in 0..width {
                if let Some(t) = map.get(x, y) {
                    if is_barrier(*t) {
                        continue; // barrier tiles score 0
                    }
                }
                let min_w = compute_min_width(map, x, y);
                scores[y * width + x] = chokepoint_score(min_w);
            }
        }

        // Pass 2: river crossing detection
        // For narrow river segments (width <= 3), mark adjacent walkable bank tiles.
        if river_mask.len() == n {
            let river_crossings = detect_river_crossings(map, river_mask, width, height);
            for (idx, score) in river_crossings {
                if idx < n && score > scores[idx] {
                    scores[idx] = score;
                }
            }
        }

        // Pass 3: clustering
        let locations = cluster_chokepoints(&scores, map, river_mask, width, height);

        Self {
            width,
            height,
            scores,
            locations,
        }
    }

    /// Get chokepoint score at (x, y). Returns 0.0 for out-of-bounds.
    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.scores[y * self.width + x]
        } else {
            0.0
        }
    }
}

/// Detect river crossings: find narrow river segments and mark adjacent walkable tiles.
fn detect_river_crossings(
    map: &TileMap,
    river_mask: &[bool],
    width: usize,
    height: usize,
) -> Vec<(usize, f64)> {
    let mut crossings = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if !river_mask[idx] {
                continue;
            }

            // Measure river width in E-W and N-S directions
            let rw_ew = measure_river_width(map, river_mask, x, y, 1, 0, width, height);
            let rw_ns = measure_river_width(map, river_mask, x, y, 0, 1, width, height);
            let min_rw = rw_ew.min(rw_ns);

            if min_rw <= 3 {
                let score = 1.0 / (min_rw as f64 + 1.0);
                // Mark adjacent walkable (non-barrier) bank tiles
                for &(dx, dy) in &[(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
                    let bx = x as i32 + dx;
                    let by = y as i32 + dy;
                    if bx >= 0 && by >= 0 && (bx as usize) < width && (by as usize) < height {
                        let bidx = by as usize * width + bx as usize;
                        if let Some(t) = map.get(bx as usize, by as usize) {
                            if !is_barrier(*t) {
                                crossings.push((bidx, score));
                            }
                        }
                    }
                }
            }
        }
    }
    crossings
}

/// Measure the width of a river at (x, y) along direction (dx, dy).
/// Counts consecutive river_mask=true tiles in both directions from (x, y).
fn measure_river_width(
    map: &TileMap,
    river_mask: &[bool],
    x: usize,
    y: usize,
    dx: i32,
    dy: i32,
    width: usize,
    height: usize,
) -> u16 {
    let _ = map; // river_mask is the authority here
    let mut count: u16 = 1; // the tile itself
    // Forward direction
    for step in 1..=10i32 {
        let nx = x as i32 + dx * step;
        let ny = y as i32 + dy * step;
        if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
            break;
        }
        let nidx = ny as usize * width + nx as usize;
        if river_mask[nidx] {
            count += 1;
        } else {
            break;
        }
    }
    // Backward direction
    for step in 1..=10i32 {
        let nx = x as i32 - dx * step;
        let ny = y as i32 - dy * step;
        if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
            break;
        }
        let nidx = ny as usize * width + nx as usize;
        if river_mask[nidx] {
            count += 1;
        } else {
            break;
        }
    }
    count
}

/// Flood-fill connected high-score tiles into discrete ChokepointLocation objects.
fn cluster_chokepoints(
    scores: &[f64],
    map: &TileMap,
    river_mask: &[bool],
    width: usize,
    height: usize,
) -> Vec<ChokepointLocation> {
    let n = width * height;
    let threshold = 0.1; // min_width <= 8
    let mut visited = vec![false; n];
    let mut locations = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if visited[idx] || scores[idx] < threshold {
                continue;
            }

            // Flood-fill this connected component (4-connected)
            let mut component: Vec<(usize, usize)> = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back((x, y));
            visited[idx] = true;

            while let Some((cx, cy)) = queue.pop_front() {
                component.push((cx, cy));
                for &(dx, dy) in &[(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                        let nidx = ny as usize * width + nx as usize;
                        if !visited[nidx] && scores[nidx] >= threshold {
                            visited[nidx] = true;
                            queue.push_back((nx as usize, ny as usize));
                        }
                    }
                }
            }

            // Discard components smaller than 2 tiles (noise) or larger than 30 tiles (valley)
            if component.len() < 2 || component.len() > 30 {
                continue;
            }

            // Find the tile with the highest score (center)
            let (best_x, best_y) = component
                .iter()
                .copied()
                .max_by(|&(ax, ay), &(bx, by)| {
                    let sa = scores[ay * width + ax];
                    let sb = scores[by * width + bx];
                    sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();

            let best_score = scores[best_y * width + best_x];
            let best_width = if best_score > 0.0 {
                (1.0 / best_score).round() as u16
            } else {
                8
            };

            // Determine the corridor axis (direction traffic flows through).
            // The axis is perpendicular to the barrier walls. If width_EW < width_NS,
            // barriers are to east/west, so traffic flows north-south.
            let d_e = ray_distance(map, best_x, best_y, 1, 0);
            let d_w = ray_distance(map, best_x, best_y, -1, 0);
            let d_n = ray_distance(map, best_x, best_y, 0, -1);
            let d_s = ray_distance(map, best_x, best_y, 0, 1);
            let width_ew = d_e + d_w - 1;
            let width_ns = d_n + d_s - 1;
            let axis = if width_ew <= width_ns {
                (0, 1) // narrow E-W -> traffic flows N-S
            } else {
                (1, 0) // narrow N-S -> traffic flows E-W
            };

            // Classify kind
            let kind =
                classify_chokepoint(&component, map, river_mask, width, height, best_x, best_y);

            locations.push(ChokepointLocation {
                x: best_x,
                y: best_y,
                width: best_width,
                axis,
                kind,
                score: best_score,
            });
        }
    }

    // Sort by score descending (best chokepoints first)
    locations.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    locations
}

/// Classify a chokepoint cluster by examining the barrier types around it.
fn classify_chokepoint(
    component: &[(usize, usize)],
    map: &TileMap,
    river_mask: &[bool],
    width: usize,
    height: usize,
    _center_x: usize,
    _center_y: usize,
) -> ChokepointKind {
    let n = width * height;
    let mut has_adjacent_river = false;
    let mut has_water_barrier = false;
    let mut has_mountain_barrier = false;

    for &(cx, cy) in component {
        for &(dx, dy) in &[(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                let nidx = ny as usize * width + nx as usize;
                if nidx < n && river_mask.len() == n && river_mask[nidx] {
                    has_adjacent_river = true;
                }
                if let Some(t) = map.get(nx as usize, ny as usize) {
                    match t {
                        Terrain::Water => has_water_barrier = true,
                        Terrain::Mountain | Terrain::Cliff => has_mountain_barrier = true,
                        _ => {}
                    }
                }
            }
        }
    }

    if has_adjacent_river {
        ChokepointKind::RiverCrossing
    } else if has_water_barrier && has_mountain_barrier {
        ChokepointKind::CoastalNarrow
    } else {
        ChokepointKind::MountainPass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a map filled with a single terrain, then apply overrides.
    fn make_map(
        width: usize,
        height: usize,
        fill: Terrain,
        overrides: &[(usize, usize, Terrain)],
    ) -> TileMap {
        let mut map = TileMap::new(width, height, fill);
        for &(x, y, t) in overrides {
            map.set(x, y, t);
        }
        map
    }

    #[test]
    fn mountain_pass_detection() {
        // 30x30 map: Grass everywhere, two Mountain walls leaving a 3-tile gap.
        // Mountains at x=12 and x=16, y=10..20 — gap is x=13,14,15.
        let mut overrides = Vec::new();
        for y in 10..20 {
            overrides.push((12, y, Terrain::Mountain));
            overrides.push((16, y, Terrain::Mountain));
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        // Tiles in the gap should have high scores
        let score_13 = cm.get(13, 15);
        let score_14 = cm.get(14, 15);
        let score_15 = cm.get(15, 15);
        assert!(
            score_13 >= 0.25,
            "x=13 y=15 should score >= 0.25, got {}",
            score_13
        );
        assert!(
            score_14 >= 0.25,
            "x=14 y=15 should score >= 0.25, got {}",
            score_14
        );
        assert!(
            score_15 >= 0.25,
            "x=15 y=15 should score >= 0.25, got {}",
            score_15
        );

        // Tile in open area should score 0.0
        let score_open = cm.get(5, 15);
        assert!(
            score_open < 0.01,
            "x=5 y=15 (open) should score ~0.0, got {}",
            score_open
        );
    }

    #[test]
    fn river_crossing_detection() {
        // 30x30 map with horizontal river at y=14,15 (Water terrain + river_mask).
        let mut overrides = Vec::new();
        for x in 0..30 {
            overrides.push((x, 14, Terrain::Water));
            overrides.push((x, 15, Terrain::Water));
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let mut river_mask = vec![false; 30 * 30];
        for x in 0..30 {
            river_mask[14 * 30 + x] = true;
            river_mask[15 * 30 + x] = true;
        }
        let cm = ChokepointMap::compute(&map, &river_mask);

        // Bank tiles at y=13 and y=16 should have scores > 0
        let bank_top = cm.get(15, 13);
        let bank_bot = cm.get(15, 16);
        assert!(
            bank_top > 0.0,
            "bank tile y=13 should score > 0, got {}",
            bank_top
        );
        assert!(
            bank_bot > 0.0,
            "bank tile y=16 should score > 0, got {}",
            bank_bot
        );
    }

    #[test]
    fn coastal_narrow_detection() {
        // Water on left half (x<10), Mountain on right half (x>15), Grass in 5-tile strip.
        let mut overrides = Vec::new();
        for y in 0..30 {
            for x in 0..10 {
                overrides.push((x, y, Terrain::Water));
            }
            for x in 16..30 {
                overrides.push((x, y, Terrain::Mountain));
            }
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        // Strip tiles (x=10..16) at y=15 should have a score around 0.14-0.17 (width ~6)
        let score_strip = cm.get(13, 15);
        assert!(
            score_strip > 0.1,
            "strip tile should score > 0.1, got {}",
            score_strip
        );
    }

    #[test]
    fn open_terrain_scores_zero() {
        // All-Grass 30x30 map — no barriers except map edges.
        let map = make_map(30, 30, Terrain::Grass, &[]);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        // Interior tiles far from edges should score 0 (min_width > 8)
        for y in 10..20 {
            for x in 10..20 {
                assert!(
                    cm.get(x, y) < 0.01,
                    "open tile ({},{}) should score 0, got {}",
                    x,
                    y,
                    cm.get(x, y)
                );
            }
        }
    }

    #[test]
    fn barrier_terrain_scores_zero() {
        // Mountain and Water tiles themselves should have score 0.
        let mut overrides = Vec::new();
        overrides.push((15, 15, Terrain::Mountain));
        overrides.push((15, 16, Terrain::Water));
        overrides.push((15, 17, Terrain::Cliff));
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        assert_eq!(cm.get(15, 15), 0.0, "Mountain tile should score 0");
        assert_eq!(cm.get(15, 16), 0.0, "Water tile should score 0");
        assert_eq!(cm.get(15, 17), 0.0, "Cliff tile should score 0");
    }

    #[test]
    fn clustering_produces_one_location_for_pass() {
        // Same mountain pass as test 1: should produce exactly 1 ChokepointLocation.
        let mut overrides = Vec::new();
        for y in 10..20 {
            overrides.push((12, y, Terrain::Mountain));
            overrides.push((16, y, Terrain::Mountain));
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        // Filter to MountainPass locations near the gap
        let pass_locations: Vec<_> = cm
            .locations
            .iter()
            .filter(|loc| {
                loc.kind == ChokepointKind::MountainPass
                    && loc.x >= 12
                    && loc.x <= 16
                    && loc.y >= 10
                    && loc.y < 20
            })
            .collect();

        assert!(
            !pass_locations.is_empty(),
            "should detect at least one mountain pass location"
        );

        // The detected width should be around 3
        let loc = &pass_locations[0];
        assert!(loc.width <= 4, "pass width should be ~3, got {}", loc.width);
    }

    #[test]
    fn score_normalization() {
        // Width-1 gap scores 1.0
        assert!((chokepoint_score(1) - 1.0).abs() < f64::EPSILON);
        // Width-4 gap scores 0.25
        assert!((chokepoint_score(4) - 0.25).abs() < f64::EPSILON);
        // Width-9 gap scores 0.0 (above threshold)
        assert!((chokepoint_score(9) - 0.0).abs() < f64::EPSILON);
        // Width-8 is the boundary: 0.125
        assert!((chokepoint_score(8) - 0.125).abs() < f64::EPSILON);
    }

    #[test]
    fn single_tile_gap_scores_one() {
        // Create a 1-tile gap between two mountain walls.
        let mut overrides = Vec::new();
        for y in 5..25 {
            // Wall at x=14 and x=16, gap at x=15
            overrides.push((14, y, Terrain::Mountain));
            overrides.push((16, y, Terrain::Mountain));
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        let score = cm.get(15, 15);
        assert!(
            (score - 1.0).abs() < f64::EPSILON,
            "single-tile gap should score 1.0, got {}",
            score
        );
    }

    #[test]
    fn wide_gap_scores_zero() {
        // A 10-tile gap (width > 8) should score 0.
        let mut overrides = Vec::new();
        for y in 5..25 {
            overrides.push((5, y, Terrain::Mountain));
            overrides.push((16, y, Terrain::Mountain));
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        // Center of the 10-tile gap at x=10
        let score = cm.get(10, 15);
        assert!(
            score < 0.01,
            "10-tile gap center should score ~0, got {}",
            score
        );
    }

    #[test]
    fn building_wall_is_barrier() {
        // BuildingWall should act as barrier for chokepoint detection.
        let mut overrides = Vec::new();
        for y in 5..25 {
            overrides.push((14, y, Terrain::BuildingWall));
            overrides.push((16, y, Terrain::BuildingWall));
        }
        let map = make_map(30, 30, Terrain::Grass, &overrides);
        let river_mask = vec![false; 30 * 30];
        let cm = ChokepointMap::compute(&map, &river_mask);

        let score = cm.get(15, 15);
        assert!(
            (score - 1.0).abs() < f64::EPSILON,
            "gap between BuildingWalls should score 1.0, got {}",
            score
        );
    }
}
