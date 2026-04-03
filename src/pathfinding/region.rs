use crate::tilemap::TileMap;

/// Size of one navigation region in tiles.
pub const REGION_SIZE: usize = 16;

/// A 16x16 tile region with flood-fill connectivity zones.
/// A river or wall splitting a region creates multiple zones.
#[derive(Debug, Clone)]
pub struct NavRegion {
    /// Grid coordinates of this region (tile_x / 16, tile_y / 16).
    pub rx: usize,
    pub ry: usize,
    /// Number of distinct walkable zones within this region.
    pub zone_count: u8,
    /// For each tile in the 16x16 block, which zone it belongs to.
    /// 0 = unwalkable, 1..N = zone ID.
    /// Indexed by `local_y * REGION_SIZE + local_x`.
    pub zone_map: [u8; REGION_SIZE * REGION_SIZE],
}

impl NavRegion {
    /// Compute zones for the region at grid position (rx, ry) by flood-filling walkable tiles.
    pub fn compute(rx: usize, ry: usize, map: &TileMap) -> Self {
        let mut zone_map = [0u8; REGION_SIZE * REGION_SIZE];
        let mut zone_count: u8 = 0;

        let base_x = rx * REGION_SIZE;
        let base_y = ry * REGION_SIZE;

        for ly in 0..REGION_SIZE {
            for lx in 0..REGION_SIZE {
                let idx = ly * REGION_SIZE + lx;
                if zone_map[idx] != 0 {
                    continue;
                }
                let tx = base_x + lx;
                let ty = base_y + ly;
                if tx >= map.width || ty >= map.height {
                    continue;
                }
                if let Some(t) = map.get(tx, ty) {
                    if !t.is_walkable() {
                        continue;
                    }
                } else {
                    continue;
                }

                // New zone found -- flood fill
                zone_count = zone_count.saturating_add(1);
                let zone_id = zone_count;
                let mut stack = vec![(lx, ly)];
                zone_map[ly * REGION_SIZE + lx] = zone_id;

                while let Some((cx, cy)) = stack.pop() {
                    for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= REGION_SIZE as i32 || ny >= REGION_SIZE as i32
                        {
                            continue;
                        }
                        let nxu = nx as usize;
                        let nyu = ny as usize;
                        let nidx = nyu * REGION_SIZE + nxu;
                        if zone_map[nidx] != 0 {
                            continue;
                        }
                        let wx = base_x + nxu;
                        let wy = base_y + nyu;
                        if wx >= map.width || wy >= map.height {
                            continue;
                        }
                        if let Some(t) = map.get(wx, wy) {
                            if t.is_walkable() {
                                zone_map[nidx] = zone_id;
                                stack.push((nxu, nyu));
                            }
                        }
                    }
                }
            }
        }

        NavRegion {
            rx,
            ry,
            zone_count,
            zone_map,
        }
    }

    /// Get the zone ID for a tile at world coordinates (tx, ty).
    /// Returns 0 if the tile is outside this region or unwalkable.
    pub fn zone_at(&self, tx: usize, ty: usize) -> u8 {
        let lx = tx.wrapping_sub(self.rx * REGION_SIZE);
        let ly = ty.wrapping_sub(self.ry * REGION_SIZE);
        if lx >= REGION_SIZE || ly >= REGION_SIZE {
            return 0;
        }
        self.zone_map[ly * REGION_SIZE + lx]
    }
}

/// A border transition between two adjacent regions: a contiguous run of walkable
/// tiles on the shared edge, represented by its midpoint.
#[derive(Debug, Clone)]
pub struct Transition {
    /// Tile position on the "from" side of the border.
    pub from_tile: (usize, usize),
    /// Tile position on the "to" side of the border.
    pub to_tile: (usize, usize),
    /// Region coordinates on the "from" side.
    pub from_region: (usize, usize),
    /// Zone ID on the "from" side.
    pub from_zone: u8,
    /// Region coordinates on the "to" side.
    pub to_region: (usize, usize),
    /// Zone ID on the "to" side.
    pub to_zone: u8,
    /// Movement cost to cross this transition (move_cost of the "to" tile).
    pub cross_cost: f64,
}

/// Find transitions between two horizontally adjacent regions (left at (rx, ry), right at (rx+1, ry)).
pub fn find_horizontal_transitions(
    left: &NavRegion,
    right: &NavRegion,
    map: &TileMap,
) -> Vec<Transition> {
    debug_assert_eq!(left.rx + 1, right.rx);
    debug_assert_eq!(left.ry, right.ry);

    let base_y = left.ry * REGION_SIZE;
    let left_edge_x = left.rx * REGION_SIZE + REGION_SIZE - 1; // rightmost column of left region
    let right_edge_x = right.rx * REGION_SIZE; // leftmost column of right region

    collect_transitions_on_edge(
        left,
        right,
        map,
        base_y,
        REGION_SIZE.min(map.height.saturating_sub(base_y)),
        |i| (left_edge_x, base_y + i),
        |i| (right_edge_x, base_y + i),
    )
}

/// Find transitions between two vertically adjacent regions (top at (rx, ry), bottom at (rx, ry+1)).
pub fn find_vertical_transitions(
    top: &NavRegion,
    bottom: &NavRegion,
    map: &TileMap,
) -> Vec<Transition> {
    debug_assert_eq!(top.ry + 1, bottom.ry);
    debug_assert_eq!(top.rx, bottom.rx);

    let base_x = top.rx * REGION_SIZE;
    let top_edge_y = top.ry * REGION_SIZE + REGION_SIZE - 1;
    let bottom_edge_y = bottom.ry * REGION_SIZE;

    collect_transitions_on_edge(
        top,
        bottom,
        map,
        base_x,
        REGION_SIZE.min(map.width.saturating_sub(base_x)),
        |i| (base_x + i, top_edge_y),
        |i| (base_x + i, bottom_edge_y),
    )
}

/// Generic transition collection along a shared edge of `length` tiles.
/// `from_pos(i)` and `to_pos(i)` return world coordinates for tile i along the edge.
fn collect_transitions_on_edge(
    from_region: &NavRegion,
    to_region: &NavRegion,
    map: &TileMap,
    _base: usize,
    length: usize,
    from_pos: impl Fn(usize) -> (usize, usize),
    to_pos: impl Fn(usize) -> (usize, usize),
) -> Vec<Transition> {
    let mut transitions = Vec::new();

    // Scan the edge, collecting contiguous runs of walkable pairs
    let mut run_start: Option<usize> = None;
    // Track (from_zone, to_zone) of the current run for consistency
    let mut run_from_zone: u8 = 0;
    let mut run_to_zone: u8 = 0;

    for i in 0..=length {
        let walkable = if i < length {
            let (fx, fy) = from_pos(i);
            let (tx, ty) = to_pos(i);
            let fz = from_region.zone_at(fx, fy);
            let tz = to_region.zone_at(tx, ty);
            if fz > 0 && tz > 0 {
                if run_start.is_none() {
                    run_from_zone = fz;
                    run_to_zone = tz;
                    true
                } else if fz == run_from_zone && tz == run_to_zone {
                    true
                } else {
                    // Zone changed -- end current run and start new one
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if walkable {
            if run_start.is_none() {
                run_start = Some(i);
            }
        } else {
            if let Some(start) = run_start.take() {
                let end = i; // exclusive
                let mid = (start + end - 1) / 2;
                let (fx, fy) = from_pos(mid);
                let (tx, ty) = to_pos(mid);
                let cross_cost = map
                    .get(tx, ty)
                    .map(|t| t.move_cost())
                    .unwrap_or(f64::INFINITY);
                transitions.push(Transition {
                    from_tile: (fx, fy),
                    to_tile: (tx, ty),
                    from_region: (from_region.rx, from_region.ry),
                    from_zone: run_from_zone,
                    to_region: (to_region.rx, to_region.ry),
                    to_zone: run_to_zone,
                    cross_cost,
                });
            }
            // If this tile is walkable but zone changed, start a new run
            if i < length {
                let (fx, fy) = from_pos(i);
                let (tx, ty) = to_pos(i);
                let fz = from_region.zone_at(fx, fy);
                let tz = to_region.zone_at(tx, ty);
                if fz > 0 && tz > 0 {
                    run_start = Some(i);
                    run_from_zone = fz;
                    run_to_zone = tz;
                }
            }
        }
    }

    transitions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilemap::Terrain;

    #[test]
    fn single_zone_all_grass() {
        let map = TileMap::new(16, 16, Terrain::Grass);
        let region = NavRegion::compute(0, 0, &map);
        assert_eq!(region.zone_count, 1);
        for ly in 0..REGION_SIZE {
            for lx in 0..REGION_SIZE {
                assert_eq!(region.zone_map[ly * REGION_SIZE + lx], 1);
            }
        }
    }

    #[test]
    fn two_zones_river_split() {
        let mut map = TileMap::new(16, 16, Terrain::Grass);
        // Vertical river down the middle (column 8)
        for y in 0..16 {
            map.set(8, y, Terrain::Water);
        }
        let region = NavRegion::compute(0, 0, &map);
        assert_eq!(region.zone_count, 2);
        // Left side and right side should have different zones
        let left_zone = region.zone_map[0]; // (0,0)
        let right_zone = region.zone_map[9]; // (9,0)
        assert_ne!(left_zone, right_zone);
        assert!(left_zone > 0);
        assert!(right_zone > 0);
        // Water tiles should be zone 0
        assert_eq!(region.zone_map[8], 0); // (8,0)
    }

    #[test]
    fn fully_unwalkable() {
        let map = TileMap::new(16, 16, Terrain::Water);
        let region = NavRegion::compute(0, 0, &map);
        assert_eq!(region.zone_count, 0);
        for &z in &region.zone_map {
            assert_eq!(z, 0);
        }
    }

    #[test]
    fn mixed_terrain_walkability() {
        let mut map = TileMap::new(16, 16, Terrain::Grass);
        // Place some walls and cliffs
        map.set(5, 5, Terrain::BuildingWall);
        map.set(6, 5, Terrain::Cliff);
        // These should not block connectivity (they are isolated obstacles)
        let region = NavRegion::compute(0, 0, &map);
        assert_eq!(region.zone_count, 1);
        assert_eq!(region.zone_map[5 * REGION_SIZE + 5], 0); // wall is unwalkable
        assert_eq!(region.zone_map[5 * REGION_SIZE + 6], 0); // cliff is unwalkable
        assert_eq!(region.zone_map[0], 1); // grass is walkable
    }

    #[test]
    fn zone_at_world_coords() {
        let map = TileMap::new(32, 32, Terrain::Grass);
        let region = NavRegion::compute(1, 1, &map);
        // (16, 16) is the top-left of region (1,1)
        assert_eq!(region.zone_at(16, 16), 1);
        // (31, 31) is the bottom-right
        assert_eq!(region.zone_at(31, 31), 1);
        // (0, 0) is outside this region
        assert_eq!(region.zone_at(0, 0), 0);
    }

    #[test]
    fn horizontal_transitions_full_edge() {
        let map = TileMap::new(32, 16, Terrain::Grass);
        let left = NavRegion::compute(0, 0, &map);
        let right = NavRegion::compute(1, 0, &map);
        let transitions = find_horizontal_transitions(&left, &right, &map);
        // Full walkable edge -> 1 transition at midpoint
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].from_tile.0, 15); // rightmost col of left
        assert_eq!(transitions[0].to_tile.0, 16); // leftmost col of right
        // Midpoint of 0..16 run = 7
        assert_eq!(transitions[0].from_tile.1, 7);
    }

    #[test]
    fn horizontal_transitions_split_by_water() {
        let mut map = TileMap::new(32, 16, Terrain::Grass);
        // Place water at the border for rows 6-9 on both sides
        for y in 6..10 {
            map.set(15, y, Terrain::Water);
            map.set(16, y, Terrain::Water);
        }
        let left = NavRegion::compute(0, 0, &map);
        let right = NavRegion::compute(1, 0, &map);
        let transitions = find_horizontal_transitions(&left, &right, &map);
        // Should have 2 transitions: one above the water, one below
        assert_eq!(transitions.len(), 2);
    }

    #[test]
    fn fully_blocked_border() {
        let mut map = TileMap::new(32, 16, Terrain::Grass);
        // Block entire right edge of left region
        for y in 0..16 {
            map.set(15, y, Terrain::Water);
        }
        let left = NavRegion::compute(0, 0, &map);
        let right = NavRegion::compute(1, 0, &map);
        let transitions = find_horizontal_transitions(&left, &right, &map);
        assert_eq!(transitions.len(), 0);
    }

    #[test]
    fn vertical_transitions() {
        let map = TileMap::new(16, 32, Terrain::Grass);
        let top = NavRegion::compute(0, 0, &map);
        let bottom = NavRegion::compute(0, 1, &map);
        let transitions = find_vertical_transitions(&top, &bottom, &map);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].from_tile.1, 15); // bottom row of top region
        assert_eq!(transitions[0].to_tile.1, 16); // top row of bottom region
    }

    #[test]
    fn partial_region_at_map_edge() {
        // Map not a multiple of 16 -- region at edge should handle gracefully
        let map = TileMap::new(20, 20, Terrain::Grass);
        let region = NavRegion::compute(1, 1, &map);
        // Region (1,1) covers tiles (16..32, 16..32), but map only goes to (20,20)
        // Only 4x4 tiles are in-bounds
        assert_eq!(region.zone_count, 1);
        // Tile (16,16) should be zone 1
        assert_eq!(region.zone_at(16, 16), 1);
        // Tile (19,19) should be zone 1
        assert_eq!(region.zone_at(19, 19), 1);
    }
}
