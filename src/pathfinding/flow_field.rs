use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::tilemap::TileMap;

/// Minimum entities requesting the same destination to trigger flow field generation.
pub const FLOW_FIELD_THRESHOLD: u32 = 5;

/// Maximum number of active flow fields at once.
pub const MAX_ACTIVE_FIELDS: usize = 8;

/// Maximum flow field computations per tick (budget control).
pub const MAX_COMPUTES_PER_TICK: usize = 2;

/// Default max age before a flow field is considered stale (ticks).
pub const DEFAULT_MAX_AGE: u64 = 200;

/// Number of consecutive zero-demand ticks before eviction.
const ZERO_DEMAND_EVICT_TICKS: u32 = 3;

/// Default radius for flow field computation.
pub const DEFAULT_RADIUS: usize = 80;

/// A precomputed flow field for a single destination.
/// Stores per-tile direction vectors and costs computed via reverse Dijkstra.
#[derive(Clone)]
pub struct FlowField {
    /// For each tile, the direction to step toward the destination.
    /// (0, 0) means destination tile or unreachable.
    directions: Vec<(i8, i8)>,

    /// Cost-to-destination for each tile. f32::MAX means unreachable.
    costs: Vec<f32>,

    width: usize,
    height: usize,

    /// Destination tile.
    pub dest_x: usize,
    pub dest_y: usize,

    /// Tick when this field was last computed.
    pub computed_tick: u64,

    /// Maximum radius from destination that was computed.
    pub radius: usize,
}

impl FlowField {
    /// Get the direction to walk from tile (x, y) toward this field's destination.
    /// Returns (0, 0) if the tile is the destination, unreachable, or out of bounds.
    pub fn direction_at(&self, x: usize, y: usize) -> (i8, i8) {
        if x < self.width && y < self.height {
            self.directions[y * self.width + x]
        } else {
            (0, 0)
        }
    }

    /// Get the cost from tile (x, y) to this field's destination.
    /// Returns f32::MAX if unreachable.
    pub fn cost_at(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.costs[y * self.width + x]
        } else {
            f32::MAX
        }
    }

    /// True if this field covers the given tile (within radius and reachable).
    pub fn covers(&self, x: usize, y: usize) -> bool {
        self.cost_at(x, y) < f32::MAX
    }

    /// True if this field is stale (older than max_age ticks or older than dirty tick).
    pub fn is_stale(&self, current_tick: u64, max_age: u64) -> bool {
        current_tick.saturating_sub(self.computed_tick) >= max_age
    }

    /// True if this field was computed before the given dirty tick.
    pub fn is_dirty(&self, terrain_dirty_tick: u64) -> bool {
        self.computed_tick < terrain_dirty_tick
    }
}

impl TileMap {
    /// Compute a flow field via reverse Dijkstra from the destination.
    /// Expands outward from (dest_x, dest_y) up to `radius` tiles, recording
    /// the optimal direction to walk toward the destination for each reachable tile.
    pub fn compute_flow_field(
        &self,
        dest_x: usize,
        dest_y: usize,
        radius: usize,
        tick: u64,
    ) -> FlowField {
        let w = self.width;
        let h = self.height;
        let size = w * h;

        let mut costs = vec![f32::MAX; size];
        let mut directions = vec![(0i8, 0i8); size];

        // Min-heap entry: (cost, x, y). Uses Reverse ordering via custom Ord.
        #[derive(PartialEq)]
        struct Entry {
            cost: f32,
            x: usize,
            y: usize,
        }
        impl Eq for Entry {}
        impl PartialOrd for Entry {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for Entry {
            fn cmp(&self, other: &Self) -> Ordering {
                // Reverse for min-heap
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(Ordering::Equal)
            }
        }

        let mut heap = BinaryHeap::new();
        let dest_idx = dest_y * w + dest_x;
        costs[dest_idx] = 0.0;
        heap.push(Entry {
            cost: 0.0,
            x: dest_x,
            y: dest_y,
        });

        let neighbors: [(i32, i32); 8] = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ];

        while let Some(Entry { cost, x: cx, y: cy }) = heap.pop() {
            let idx = cy * w + cx;
            if cost > costs[idx] {
                continue; // stale entry
            }

            // Radius bound: don't expand beyond radius from destination
            let dx = (cx as i32 - dest_x as i32).unsigned_abs() as usize;
            let dy = (cy as i32 - dest_y as i32).unsigned_abs() as usize;
            if dx > radius || dy > radius {
                continue;
            }

            for &(nx_off, ny_off) in &neighbors {
                let nx = cx as i32 + nx_off;
                let ny = cy as i32 + ny_off;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let (nxu, nyu) = (nx as usize, ny as usize);
                let terrain = match self.get(nxu, nyu) {
                    Some(t) => t,
                    None => continue,
                };
                if !terrain.is_walkable() {
                    continue;
                }

                // Diagonal movement costs sqrt(2) * terrain cost
                let step_cost = terrain.move_cost() as f32
                    * if nx_off != 0 && ny_off != 0 {
                        1.414
                    } else {
                        1.0
                    };
                let new_cost = cost + step_cost;
                let n_idx = nyu * w + nxu;

                if new_cost < costs[n_idx] {
                    costs[n_idx] = new_cost;
                    // Direction points FROM neighbor TOWARD current tile
                    // (i.e., the direction this neighbor should walk)
                    directions[n_idx] = (-nx_off as i8, -ny_off as i8);
                    heap.push(Entry {
                        cost: new_cost,
                        x: nxu,
                        y: nyu,
                    });
                }
            }
        }

        FlowField {
            directions,
            costs,
            width: w,
            height: h,
            dest_x,
            dest_y,
            computed_tick: tick,
            radius,
        }
    }
}

/// Registry managing active flow fields, demand tracking, and lifecycle.
/// Stored on `Game`, queried by entities each tick.
pub struct FlowFieldRegistry {
    /// Active flow fields keyed by destination tile.
    fields: HashMap<(usize, usize), FlowField>,

    /// Demand counter: how many entities requested each destination this tick.
    demand: HashMap<(usize, usize), u32>,

    /// Consecutive zero-demand tick counter per destination (for eviction).
    zero_demand_ticks: HashMap<(usize, usize), u32>,

    /// Tick when terrain last changed. Fields computed before this are stale.
    terrain_dirty_tick: u64,
}

impl Default for FlowFieldRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowFieldRegistry {
    pub fn new() -> Self {
        FlowFieldRegistry {
            fields: HashMap::new(),
            demand: HashMap::new(),
            zero_demand_ticks: HashMap::new(),
            terrain_dirty_tick: 0,
        }
    }

    /// Look up a flow field for the given destination.
    pub fn get(&self, dest_x: usize, dest_y: usize) -> Option<&FlowField> {
        self.fields.get(&(dest_x, dest_y))
    }

    /// Signal that an entity wants to move toward this destination.
    /// Call once per entity per tick, before `move_toward_cached`.
    pub fn request(&mut self, dest_x: usize, dest_y: usize) {
        *self.demand.entry((dest_x, dest_y)).or_insert(0) += 1;
    }

    /// Notify the registry that terrain has changed.
    /// All fields computed before this tick are considered stale.
    pub fn mark_terrain_dirty(&mut self, tick: u64) {
        self.terrain_dirty_tick = tick;
    }

    /// Returns the current terrain dirty tick.
    pub fn terrain_dirty_tick(&self) -> u64 {
        self.terrain_dirty_tick
    }

    /// Number of currently active flow fields.
    pub fn active_count(&self) -> usize {
        self.fields.len()
    }

    /// Demand for a specific destination this tick.
    pub fn demand_for(&self, dest_x: usize, dest_y: usize) -> u32 {
        self.demand.get(&(dest_x, dest_y)).copied().unwrap_or(0)
    }

    /// Per-tick maintenance: create new fields for high-demand destinations,
    /// recompute stale fields, evict unused fields.
    /// Returns the number of fields computed this tick.
    pub fn maintain(&mut self, map: &TileMap, tick: u64) -> usize {
        let mut computes = 0usize;

        // Phase 1: Update zero-demand counters and collect eviction candidates
        let mut to_evict: Vec<(usize, usize)> = Vec::new();
        for key in self.fields.keys() {
            let demand = self.demand.get(key).copied().unwrap_or(0);
            if demand == 0 {
                let counter = self.zero_demand_ticks.entry(*key).or_insert(0);
                *counter += 1;
                if *counter >= ZERO_DEMAND_EVICT_TICKS {
                    to_evict.push(*key);
                }
            } else {
                self.zero_demand_ticks.insert(*key, 0);
            }
        }

        // Evict zero-demand fields
        for key in &to_evict {
            self.fields.remove(key);
            self.zero_demand_ticks.remove(key);
        }

        // Phase 2: Recompute stale fields that still have demand
        let stale_keys: Vec<(usize, usize)> = self
            .fields
            .iter()
            .filter(|(key, ff)| {
                let demand = self.demand.get(key).copied().unwrap_or(0);
                demand > 0
                    && (ff.is_stale(tick, DEFAULT_MAX_AGE) || ff.is_dirty(self.terrain_dirty_tick))
            })
            .map(|(key, _)| *key)
            .collect();

        for key in stale_keys {
            if computes >= MAX_COMPUTES_PER_TICK {
                break;
            }
            let old_radius = self
                .fields
                .get(&key)
                .map(|f| f.radius)
                .unwrap_or(DEFAULT_RADIUS);
            let ff = map.compute_flow_field(key.0, key.1, old_radius, tick);
            self.fields.insert(key, ff);
            computes += 1;
        }

        // Phase 3: Create new fields for high-demand destinations without an existing field
        let mut new_candidates: Vec<((usize, usize), u32)> = self
            .demand
            .iter()
            .filter(|(key, count)| {
                **count >= FLOW_FIELD_THRESHOLD && !self.fields.contains_key(key)
            })
            .map(|(key, count)| (*key, *count))
            .collect();
        // Sort by demand descending so highest-demand destinations are prioritized
        new_candidates.sort_by(|a, b| b.1.cmp(&a.1));

        for (key, _) in new_candidates {
            if computes >= MAX_COMPUTES_PER_TICK {
                break;
            }
            if self.fields.len() >= MAX_ACTIVE_FIELDS {
                // Evict the lowest-demand existing field to make room
                if let Some((&evict_key, _)) = self
                    .fields
                    .iter()
                    .min_by_key(|(k, _)| self.demand.get(k).copied().unwrap_or(0))
                {
                    self.fields.remove(&evict_key);
                    self.zero_demand_ticks.remove(&evict_key);
                } else {
                    break;
                }
            }
            let ff = map.compute_flow_field(key.0, key.1, DEFAULT_RADIUS, tick);
            self.fields.insert(key, ff);
            computes += 1;
        }

        // Clear demand for next tick
        self.demand.clear();

        computes
    }

    /// Force-insert a flow field (used for always-on stockpile fields).
    pub fn insert(&mut self, ff: FlowField) {
        let key = (ff.dest_x, ff.dest_y);
        self.fields.insert(key, ff);
        // Ensure it won't be evicted immediately
        self.zero_demand_ticks.insert(key, 0);
    }

    /// Remove all fields (e.g. before save).
    pub fn clear(&mut self) {
        self.fields.clear();
        self.demand.clear();
        self.zero_demand_ticks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilemap::{Terrain, TileMap};

    fn grass_map(w: usize, h: usize) -> TileMap {
        TileMap::new(w, h, Terrain::Grass)
    }

    // ── FlowField computation tests ──

    #[test]
    fn flow_field_destination_has_zero_cost() {
        let map = grass_map(16, 16);
        let ff = map.compute_flow_field(8, 8, 10, 0);
        assert_eq!(ff.cost_at(8, 8), 0.0);
        assert_eq!(ff.direction_at(8, 8), (0, 0));
    }

    #[test]
    fn flow_field_open_terrain_directions_point_toward_dest() {
        let map = grass_map(16, 16);
        let ff = map.compute_flow_field(8, 8, 15, 0);

        // Tile to the left of destination should point right (+1, 0)
        assert_eq!(ff.direction_at(5, 8), (1, 0));
        // Tile to the right should point left (-1, 0)
        assert_eq!(ff.direction_at(11, 8), (-1, 0));
        // Tile above should point down (0, +1)
        assert_eq!(ff.direction_at(8, 5), (0, 1));
        // Tile below should point up (0, -1)
        assert_eq!(ff.direction_at(8, 11), (0, -1));
    }

    #[test]
    fn flow_field_diagonal_directions() {
        let map = grass_map(16, 16);
        let ff = map.compute_flow_field(8, 8, 15, 0);

        // Tile at (5, 5) should point toward (8, 8) => (+1, +1)
        assert_eq!(ff.direction_at(5, 5), (1, 1));
        // Tile at (11, 11) should point toward (8, 8) => (-1, -1)
        assert_eq!(ff.direction_at(11, 11), (-1, -1));
    }

    #[test]
    fn flow_field_around_water_barrier() {
        let mut map = grass_map(16, 16);
        // Place a vertical water wall at x=7, y=4..12
        for y in 4..12 {
            map.set(7, y, Terrain::Water);
        }
        let ff = map.compute_flow_field(10, 8, 15, 0);

        // Tile at (5, 8) is blocked from going directly right through water.
        // It should still be reachable (route around the wall).
        assert!(ff.covers(5, 8));
        // Water tiles should be unreachable
        assert!(!ff.covers(7, 8));
        assert_eq!(ff.cost_at(7, 8), f32::MAX);
    }

    #[test]
    fn flow_field_prefers_road_over_forest() {
        let mut map = grass_map(20, 5);
        // Create a forest band at y=2, except a road at x=10
        for x in 0..20 {
            map.set(x, 2, Terrain::Forest);
        }
        map.set(10, 2, Terrain::Road);

        let ff = map.compute_flow_field(10, 0, 20, 0);

        // Cost through road tile should be lower than through forest
        let road_cost = ff.cost_at(10, 2);
        let forest_cost = ff.cost_at(9, 2);
        assert!(
            road_cost < forest_cost,
            "road cost {} should be less than forest cost {}",
            road_cost,
            forest_cost
        );
    }

    #[test]
    fn flow_field_unreachable_tile() {
        let mut map = grass_map(16, 16);
        // Surround tile (2, 2) with water on all sides
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                map.set((2i32 + dx) as usize, (2i32 + dy) as usize, Terrain::Water);
            }
        }
        let ff = map.compute_flow_field(8, 8, 15, 0);

        // (2, 2) is surrounded by water — unreachable
        assert!(!ff.covers(2, 2));
        assert_eq!(ff.direction_at(2, 2), (0, 0));
    }

    #[test]
    fn flow_field_radius_bound() {
        let map = grass_map(64, 64);
        let ff = map.compute_flow_field(32, 32, 5, 0);

        // Tile within radius should be reachable
        assert!(ff.covers(30, 32));
        // Tile way outside radius should be unreachable
        assert!(!ff.covers(10, 10));
        assert_eq!(ff.cost_at(10, 10), f32::MAX);
    }

    #[test]
    fn flow_field_staleness() {
        let map = grass_map(16, 16);
        let ff = map.compute_flow_field(8, 8, 10, 100);

        assert!(!ff.is_stale(150, 200)); // 50 ticks old, max 200 => not stale
        assert!(ff.is_stale(300, 200)); // 200 ticks old => stale
        assert!(ff.is_stale(301, 200)); // 201 ticks old => stale

        assert!(!ff.is_dirty(50)); // computed at 100, dirty at 50 => not dirty
        assert!(ff.is_dirty(101)); // computed at 100, dirty at 101 => dirty
    }

    // ── FlowFieldRegistry tests ──

    #[test]
    fn registry_below_threshold_no_field() {
        let map = grass_map(16, 16);
        let mut reg = FlowFieldRegistry::new();

        // Only 3 requests — below threshold of 5
        for _ in 0..3 {
            reg.request(8, 8);
        }
        reg.maintain(&map, 1);

        assert!(reg.get(8, 8).is_none());
    }

    #[test]
    fn registry_at_threshold_creates_field() {
        let map = grass_map(16, 16);
        let mut reg = FlowFieldRegistry::new();

        for _ in 0..FLOW_FIELD_THRESHOLD {
            reg.request(8, 8);
        }
        reg.maintain(&map, 1);

        assert!(reg.get(8, 8).is_some());
        assert_eq!(reg.active_count(), 1);
    }

    #[test]
    fn registry_stale_field_recomputed_with_demand() {
        let map = grass_map(16, 16);
        let mut reg = FlowFieldRegistry::new();

        // Create field at tick 0
        let ff = map.compute_flow_field(8, 8, DEFAULT_RADIUS, 0);
        reg.insert(ff);

        // Tick 300: field is stale (>200 ticks old), demand exists
        for _ in 0..5 {
            reg.request(8, 8);
        }
        reg.maintain(&map, 300);

        let ff = reg.get(8, 8).unwrap();
        assert_eq!(ff.computed_tick, 300); // recomputed
    }

    #[test]
    fn registry_zero_demand_eviction() {
        let map = grass_map(16, 16);
        let mut reg = FlowFieldRegistry::new();

        // Insert a field
        let ff = map.compute_flow_field(8, 8, DEFAULT_RADIUS, 0);
        reg.insert(ff);
        assert_eq!(reg.active_count(), 1);

        // 3 ticks with zero demand => evicted
        for tick in 1..=3 {
            reg.maintain(&map, tick);
        }
        assert_eq!(reg.active_count(), 0);
    }

    #[test]
    fn registry_max_active_cap() {
        let map = grass_map(64, 64);
        let mut reg = FlowFieldRegistry::new();

        // Request MAX_ACTIVE_FIELDS + 1 different destinations
        for i in 0..=(MAX_ACTIVE_FIELDS as usize) {
            let x = 5 + i * 5;
            for _ in 0..FLOW_FIELD_THRESHOLD {
                reg.request(x, 10);
            }
        }
        // Need multiple maintain calls since budget is 2 per tick
        for tick in 0..10 {
            // Re-request to keep demand alive
            for i in 0..=(MAX_ACTIVE_FIELDS as usize) {
                let x = 5 + i * 5;
                for _ in 0..FLOW_FIELD_THRESHOLD {
                    reg.request(x, 10);
                }
            }
            reg.maintain(&map, tick);
        }

        assert!(reg.active_count() <= MAX_ACTIVE_FIELDS);
    }

    #[test]
    fn registry_budget_cap_two_per_tick() {
        let map = grass_map(64, 64);
        let mut reg = FlowFieldRegistry::new();

        // Request 4 different destinations, all above threshold
        for i in 0..4 {
            for _ in 0..FLOW_FIELD_THRESHOLD {
                reg.request(10 + i * 10, 10);
            }
        }

        let computes = reg.maintain(&map, 1);
        assert!(
            computes <= MAX_COMPUTES_PER_TICK,
            "Expected at most {} computes, got {}",
            MAX_COMPUTES_PER_TICK,
            computes
        );
    }

    #[test]
    fn registry_terrain_dirty_invalidates() {
        let map = grass_map(16, 16);
        let mut reg = FlowFieldRegistry::new();

        // Create field at tick 10
        let ff = map.compute_flow_field(8, 8, DEFAULT_RADIUS, 10);
        reg.insert(ff);

        // Mark terrain dirty at tick 15
        reg.mark_terrain_dirty(15);

        // Field computed at tick 10 < dirty tick 15 => dirty
        assert!(reg.get(8, 8).unwrap().is_dirty(reg.terrain_dirty_tick()));

        // Maintain with demand => should recompute
        for _ in 0..5 {
            reg.request(8, 8);
        }
        reg.maintain(&map, 20);

        let ff = reg.get(8, 8).unwrap();
        assert_eq!(ff.computed_tick, 20);
    }

    #[test]
    fn registry_demand_cleared_each_tick() {
        let mut reg = FlowFieldRegistry::new();
        reg.request(5, 5);
        assert_eq!(reg.demand_for(5, 5), 1);

        let map = grass_map(16, 16);
        reg.maintain(&map, 1);

        // After maintain, demand should be cleared
        assert_eq!(reg.demand_for(5, 5), 0);
    }

    #[test]
    fn flow_field_covers_walkable_within_radius() {
        let map = grass_map(32, 32);
        let ff = map.compute_flow_field(16, 16, 10, 0);

        // All walkable tiles within radius should be covered
        assert!(ff.covers(16, 16)); // destination itself
        assert!(ff.covers(10, 16)); // 6 tiles away
        assert!(ff.covers(16, 10)); // 6 tiles away
    }

    #[test]
    fn flow_field_out_of_bounds() {
        let map = grass_map(16, 16);
        let ff = map.compute_flow_field(8, 8, 10, 0);

        assert_eq!(ff.direction_at(100, 100), (0, 0));
        assert_eq!(ff.cost_at(100, 100), f32::MAX);
        assert!(!ff.covers(100, 100));
    }
}
