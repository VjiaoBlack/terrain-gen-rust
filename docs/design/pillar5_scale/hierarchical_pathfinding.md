# Hierarchical Pathfinding

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 5 (Scale Over Fidelity)*
*Depends on: Path Caching (Tier 1), Spatial Hash Grid*
*Unlocks: 1000+ villagers, 512x512+ maps, multi-settlement pathfinding*

## Problem

Path caching (Tier 1) eliminates redundant per-tick A* calls by reusing waypoint lists. But each path still runs a full A* search over the raw tile grid when first computed. On a 256x256 map, A* explores up to 600 nodes per call with a Manhattan heuristic. For long-distance paths (60+ tiles -- a villager walking from a mountain mining outpost back to the stockpile), the search budget is often exhausted before finding the goal, producing no path at all.

The problem gets worse on larger maps and with more terrain obstacles:

| Map size | Tiles | A* budget (d*4, cap 600) | Typical long path | Budget sufficient? |
|----------|-------|--------------------------|-------------------|--------------------|
| 256x256 | 65K | 600 | 40-80 tiles | Marginal -- fails for cross-map paths |
| 512x512 | 262K | 600 | 80-200 tiles | Frequently fails |
| 1024x1024 | 1M | 600 | 200-500 tiles | Almost never succeeds |

Raising the budget is not a solution. A* with 2000-node budget on a 512x512 map allocates a 262K-element visited array and explores tiles quadratically around the start. At 500 villagers each needing one path computation per ~120 ticks, that is ~4 long-distance A* searches per tick with large allocations.

Flow fields (path_caching.md Tier 3) solve the case where many entities share a destination. Hierarchical pathfinding solves the complementary case: **any entity, any destination, any distance, cheap.**

## Solution: Two-Level Hierarchical Pathfinding (HPA*)

Divide the map into fixed-size **regions**. Precompute which regions connect and the cost to cross between them. Long-distance pathfinding becomes:

1. **High-level A***: find a sequence of regions from the start region to the goal region. This searches a graph of ~64-256 nodes instead of 65K-262K tiles. Microseconds, not milliseconds.
2. **Local A***: within each region, pathfind from the entry point to the exit point (or to the final destination in the last region). Each local search covers at most `region_size * region_size` tiles.

The high-level graph is precomputed once at world-gen and updated incrementally when terrain changes. Most ticks, no graph recomputation happens at all.

### Region size: 16x16 tiles

The region size must balance two concerns:
- **Too small** (8x8): too many regions (256x256 map = 1024 regions, 512x512 = 4096), large inter-region graph, many region transitions per path.
- **Too large** (64x64): local A* within a region is expensive (up to 4096 tiles), and a terrain change invalidates a large area.

16x16 is the sweet spot:
- 256x256 map: 16x16 = **256 regions**. Inter-region graph has ~256 nodes. High-level A* explores <50 nodes for any path.
- 512x512 map: 32x32 = **1024 regions**. Still small for A*.
- Matches the spatial hash grid cell size (16x16), so the same cell boundaries serve both systems.
- Local A* within a 16x16 region explores at most 256 tiles -- trivially fast, no budget cap needed.
- A building placement or tree being cut invalidates exactly 1 region (recompute its internal connectivity and border transitions). Localized cost.

### Region computation

At world-gen (or map load), partition the map into non-overlapping 16x16 regions. Each region is identified by `(region_x, region_y)` where `region_x = tile_x / 16`, `region_y = tile_y / 16`.

#### Intra-region connectivity

Within each region, identify connected components of walkable tiles using flood fill. A region with a river running through it may have two or more disconnected walkable areas. Each connected component is a **zone**. Most regions have a single zone (all walkable tiles are connected internally).

```rust
#[derive(Debug, Clone)]
pub struct Region {
    /// Grid coordinates of this region.
    pub rx: usize,
    pub ry: usize,
    /// Number of distinct walkable zones within this region.
    /// Computed by flood fill over the 16x16 tile block.
    pub zone_count: usize,
    /// For each tile in the 16x16 block, which zone it belongs to (0 = unwalkable, 1..N = zone ID).
    pub zone_map: [u8; 256],  // 16*16, indexed by local_y * 16 + local_x
}
```

Zone computation is a flood fill over the 16x16 block: iterate tiles, for each unvisited walkable tile, BFS/DFS and assign a zone ID. Cost: 256 tiles per region, negligible.

#### Border transitions

For each pair of adjacent regions (sharing a 16-tile edge), identify **transition points** -- pairs of tiles on opposite sides of the border that are both walkable and in connected zones.

Rather than creating a transition for every walkable border tile pair (up to 16 per edge), we merge contiguous runs of walkable border tiles into single transitions at their midpoints. This keeps the graph small.

```rust
#[derive(Debug, Clone)]
pub struct Transition {
    /// Tile position on the "from" side of the border.
    pub from_tile: (usize, usize),
    /// Tile position on the "to" side of the border.
    pub to_tile: (usize, usize),
    /// Region and zone on the "from" side.
    pub from_region: (usize, usize),
    pub from_zone: u8,
    /// Region and zone on the "to" side.
    pub to_region: (usize, usize),
    pub to_zone: u8,
    /// Cost to cross this transition (move_cost of the destination tile).
    pub cross_cost: f64,
}
```

Example: regions (2,3) and (3,3) share a vertical border. Tiles (47,48)-(47,55) on the right edge of (2,3) are all walkable grass, and tiles (48,48)-(48,55) on the left edge of (3,3) are also walkable. This produces one transition at the midpoint: `from_tile=(47,51), to_tile=(48,51)`.

For a 256x256 map with 256 regions and 4 edges each, the total transition count is typically 400-800 (most edges have 1-3 transitions; edges blocked by water or mountains have 0).

### Inter-region graph

The high-level navigation graph connects transition points. Two types of edges:

1. **Cross-border edges**: directly from each `Transition` -- connects `from_tile` in one region to `to_tile` in the adjacent region. Cost: `cross_cost` (single tile move cost). These are bidirectional.

2. **Intra-region edges**: within a region, connects every pair of transition points that share the same zone. Cost: the A* distance between them through the 16x16 tile block (precomputed once).

```rust
pub struct NavGraph {
    /// All transition points, indexed by a compact ID.
    pub transitions: Vec<Transition>,
    /// Adjacency list. For each transition ID, a list of (neighbor_transition_id, cost).
    pub edges: Vec<Vec<(usize, f64)>>,
    /// Lookup: given a region (rx, ry) and zone, which transition IDs touch it?
    /// Used to find the start/end nodes for a high-level search.
    pub region_transitions: HashMap<(usize, usize, u8), Vec<usize>>,
}
```

#### Intra-region edge cost computation

For each region, collect all transition points that touch it. For each pair in the same zone, run local A* over the 16x16 block to get the exact traversal cost. This is precomputed at build time.

Worst case per region: 4 edges x 3 transitions each = 12 transition points, C(12,2) = 66 pairs, each running A* on 256 tiles. In practice most regions have 2-6 transition points, so 1-15 pairs. Total across 256 regions: roughly 500-2000 local A* calls on 16x16 grids. Takes <50ms at world-gen. Runs once.

### High-level pathfinding

Given start tile `(sx, sy)` and goal tile `(gx, gy)`:

1. **Identify start region** `(sx/16, sy/16)` and **goal region** `(gx/16, gy/16)`.

2. **Same region?** If start and goal are in the same 16x16 region, skip the high-level search entirely. Run local A* on the region block. This handles the common case of short-distance movement with zero overhead.

3. **Find entry/exit nodes.** In the start region, find which zone `(sx, sy)` belongs to (lookup `zone_map`). Get all transition IDs for that zone via `region_transitions`. For each, compute the local A* cost from `(sx, sy)` to that transition's `from_tile` within the 16x16 block. These become "virtual start edges" in the graph. Same process for the goal region: compute cost from each transition's `to_tile` to `(gx, gy)`.

4. **Run A* on the NavGraph.** The search space is transition points (typically 400-800 nodes total). With good heuristics (Euclidean distance between tile positions), this explores 20-60 nodes for a cross-map path. Microseconds.

5. **Result: region waypoint sequence.** The high-level path is a list of transition points: `[(tx0,ty0), (tx1,ty1), ..., (txN,tyN)]`. Each consecutive pair of transitions is either a cross-border step or an intra-region traversal.

### Local pathfinding within regions

The entity follows the region waypoint sequence. Within each region, it needs a detailed tile-by-tile path between the entry point and the exit point (or final destination).

This is handled by the existing path cache (Tier 1 from path_caching.md). The `PathCache` stores waypoints. When the entity enters a new region, it runs local A* from its current position to the next region transition point. The search is bounded to the 16x16 region block -- at most 256 tiles, completing in microseconds with no budget cap needed.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalPath {
    /// High-level path: sequence of transition tile positions to pass through.
    pub region_waypoints: Vec<(f64, f64)>,
    /// Index of the next region waypoint to reach.
    pub region_cursor: usize,
    /// Tick when the high-level path was computed.
    pub computed_tick: u64,
    /// Destination this path leads to.
    pub dest_x: f64,
    pub dest_y: f64,
}
```

Integration with `PathCache`: the existing `PathCache` handles the local tile-by-tile movement. `HierarchicalPath` sits above it and feeds intermediate destinations (transition points) to the path cache. When the entity reaches a transition waypoint, `region_cursor` advances and the `PathCache` is invalidated so it recomputes a local path to the next transition point.

### Movement function integration

```rust
pub(super) fn move_toward_hierarchical(
    pos: &Position,
    tx: f64, ty: f64,
    speed: f64,
    vel: &mut Velocity,
    map: &TileMap,
    local_cache: &mut PathCache,
    hier_path: &mut Option<HierarchicalPath>,
    nav_graph: &NavGraph,
    current_tick: u64,
) -> f64 {
    let d = dist(pos.x, pos.y, tx, ty);
    if d < 0.5 { return d; }

    // Short distance or same region: use local A* directly (no hierarchy overhead)
    let same_region = (pos.x as usize / 16) == (tx as usize / 16)
                   && (pos.y as usize / 16) == (ty as usize / 16);
    if d <= 5.0 || same_region {
        return move_toward_cached(pos, tx, ty, speed, vel, map, local_cache, current_tick);
    }

    // Check if hierarchical path is valid
    let hier_valid = hier_path.as_ref().map_or(false, |hp| {
        (hp.dest_x - tx).abs() < 0.5
        && (hp.dest_y - ty).abs() < 0.5
        && (current_tick - hp.computed_tick) < 600  // stale after ~10 game-seconds
        && hp.region_cursor < hp.region_waypoints.len()
    });

    if !hier_valid {
        // Compute high-level path through NavGraph
        if let Some(waypoints) = nav_graph.find_path(pos.x, pos.y, tx, ty, map) {
            *hier_path = Some(HierarchicalPath {
                region_waypoints: waypoints,
                region_cursor: 0,
                computed_tick: current_tick,
                dest_x: tx,
                dest_y: ty,
            });
            local_cache.waypoints.clear();
            local_cache.cursor = 0;
        } else {
            // No high-level path exists -- fall back to direct local A*
            return move_toward_cached(pos, tx, ty, speed, vel, map, local_cache, current_tick);
        }
    }

    // Follow region waypoints via local path cache
    let hp = hier_path.as_mut().unwrap();
    let (wx, wy) = hp.region_waypoints[hp.region_cursor];
    let wd = dist(pos.x, pos.y, wx, wy);

    if wd < 1.5 {
        // Reached this region waypoint, advance to next
        hp.region_cursor += 1;
        local_cache.waypoints.clear();
        local_cache.cursor = 0;
        if hp.region_cursor >= hp.region_waypoints.len() {
            // All region waypoints traversed, walk to final destination
            return move_toward_cached(pos, tx, ty, speed, vel, map, local_cache, current_tick);
        }
        let (wx2, wy2) = hp.region_waypoints[hp.region_cursor];
        return move_toward_cached(pos, wx2, wy2, speed, vel, map, local_cache, current_tick);
    }

    move_toward_cached(pos, wx, wy, speed, vel, map, local_cache, current_tick)
}
```

### NavGraph incremental update

The nav graph is precomputed at world-gen, but terrain changes at runtime: buildings are placed, trees are cut, roads are built, mining changes mountain tiles. The graph must stay current.

**Key insight:** a terrain change at tile `(tx, ty)` only affects the region containing that tile: `(tx/16, ty/16)`. At most, it also affects transitions on the region's borders (if the changed tile is on an edge).

Update procedure when tile `(tx, ty)` changes:

1. **Mark region dirty.** Add `(tx/16, ty/16)` to a dirty set.
2. **Batch updates.** Do not recompute immediately. At the end of each game tick (after all building/terrain changes), process the dirty set.
3. **Per dirty region:**
   a. Re-flood-fill the 16x16 block to recompute `zone_map` and `zone_count`. Cost: 256 tiles.
   b. Recompute border transitions for all 4 edges of this region. Cost: 4 edges x 16 tiles = 64 checks.
   c. Recompute intra-region edge costs between the new transition points. Cost: ~15 local A* calls on 16x16 grids.
   d. Update the `NavGraph` adjacency list: remove old transitions for this region, insert new ones.
4. **Invalidate affected hierarchical paths.** Any entity whose `HierarchicalPath` passes through a dirty region has its path invalidated (set `computed_tick = 0`). On its next movement tick, it will recompute a high-level path. This is conservative but simple.

**Cost of incremental update:** ~1ms per dirty region. In a typical tick, 0-2 regions are dirty (a building placement touches 1 region, a deforestation event touches 1 region). Worst case: a large construction project dirties 4 adjacent regions = ~4ms, acceptable within the path budget.

**Amortization:** Multiple terrain changes in the same region within the same tick are batched -- the region is recomputed once, not once per changed tile.

### Memory cost

| Component | Per unit | Count (256x256 map) | Total |
|-----------|---------|---------------------|-------|
| Region zone_map | 256 bytes | 256 regions | 64 KB |
| Transitions | ~48 bytes each | ~600 | 29 KB |
| NavGraph edges | ~16 bytes per edge | ~2400 | 38 KB |
| region_transitions map | ~32 bytes per entry | ~1200 | 38 KB |
| **Total nav mesh** | | | **~170 KB** |
| HierarchicalPath per entity | ~200 bytes (10 region waypoints avg) | 500 entities | 100 KB |
| **Total** | | | **~270 KB** |

Negligible. The visited bitmap for a single full-map A* call (256x256 = 64 KB) costs more than the entire nav mesh.

### Performance comparison

| Scenario | Full A* (current) | Path cache only (Tier 1) | Hierarchical |
|----------|-------------------|--------------------------|--------------|
| 40-tile path, 256x256 | 160 nodes explored, ~0.3ms | Same, but once per 120 ticks | Same (local A* within region) |
| 100-tile path, 256x256 | 400+ nodes, often hits 600 cap = **failure** | Same problem on first compute | ~30 high-level nodes + 2-3 local A* on 16x16 = **0.1ms** |
| 200-tile path, 512x512 | 600 cap = **failure** | **failure** | ~50 high-level nodes + 4-5 local A* on 16x16 = **0.15ms** |
| Cross-map (400 tiles), 512x512 | **impossible** | **impossible** | ~80 high-level nodes + 8 local A* = **0.3ms** |

The hierarchy makes long-distance paths both possible and cheap. Short-distance paths (same region) skip the hierarchy entirely and use local A* with zero overhead.

## Integration with existing systems

### Path caching (Tier 1)

HierarchicalPath and PathCache are separate components. PathCache handles tile-by-tile movement within a region. HierarchicalPath feeds it intermediate goals (transition points). When the hierarchy is not needed (same-region movement), PathCache works exactly as designed in path_caching.md.

### Flow fields (Tier 3)

Flow fields and hierarchical pathfinding complement each other:
- **Flow fields**: many entities, one destination (stockpile). O(1) per entity per tick.
- **Hierarchical pathfinding**: one entity, one unique long-distance destination (distant resource, exploration target). O(small) per path computation.

The lookup order in `move_toward`:
1. Distance < 3 tiles: direct movement.
2. Flow field exists for destination: use it.
3. Same region as destination: local A* via PathCache.
4. Cross-region: hierarchical path via NavGraph + local PathCache.

### Spatial hash grid

The spatial hash grid uses the same 16x16 cell size. This is intentional. A region boundary is always a cell boundary. When invalidating hierarchical paths for entities in a dirty region, we can use `grid.query_radius()` centered on the region to find affected entities efficiently, rather than scanning all entities.

### Terrain changes

`TileMap::set()` is the single point where terrain changes. Currently called from:
- `game/build.rs` -- building placement and demolition
- `src/ecs/systems.rs` -- deforestation, mining, road auto-build
- `src/simulation.rs` -- vegetation regrowth

Each call site that mutates terrain should notify the nav graph by adding the region to a dirty set. The simplest approach: add a `dirty_regions: HashSet<(usize, usize)>` field to `Game`. In `TileMap::set()`, push the region coordinates. At the end of `Game::step()`, process dirty regions and clear the set.

Alternatively, wrap `TileMap::set()` to automatically mark regions dirty:

```rust
impl TileMap {
    pub fn set_and_mark(&mut self, x: usize, y: usize, terrain: Terrain,
                         dirty: &mut HashSet<(usize, usize)>) {
        self.set(x, y, terrain);
        dirty.insert((x / 16, y / 16));
        // If tile is on a region border, also mark the adjacent region
        if x % 16 == 0 && x > 0 { dirty.insert(((x - 1) / 16, y / 16)); }
        if x % 16 == 15 { dirty.insert(((x + 1) / 16, y / 16)); }
        if y % 16 == 0 && y > 0 { dirty.insert((x / 16, (y - 1) / 16)); }
        if y % 16 == 15 { dirty.insert((x / 16, (y + 1) / 16)); }
    }
}
```

## Implementation plan

### Step 1: Region and zone computation

Add `src/pathfinding/region.rs` with `Region` struct and flood-fill zone computation.

Unit tests:
- Single-zone region (all grass): zone_count = 1, all tiles zone 1
- Two-zone region (river splits it): zone_count = 2, tiles on each side have different zone IDs
- Fully unwalkable region (all water): zone_count = 0
- Mixed terrain respects walkability (BuildingWall and Cliff block, everything else connects)

### Step 2: Border transitions

Add transition detection between adjacent regions. Merge contiguous walkable border runs into single transition points at midpoints.

Unit tests:
- Two grass regions sharing a full edge: 1 transition at midpoint
- Border partially blocked by water: 2 transitions (one on each side of the water)
- Fully blocked border: 0 transitions
- Diagonal regions share no transitions (only cardinal adjacency)

### Step 3: NavGraph construction

Build the inter-region graph. Compute intra-region edge costs between transition points via local A*.

Unit tests:
- Simple 3x3 region map with known connectivity: verify graph structure
- Region with no outgoing transitions is an island: entities in it get no high-level path (correctly falls back to local A*)
- Intra-region costs reflect terrain (road is cheaper than forest)

### Step 4: High-level A* on NavGraph

Implement `NavGraph::find_path(sx, sy, gx, gy, map)` that returns a sequence of region waypoints.

Unit tests:
- Straight-line path across 3 regions: returns 2-3 transition waypoints
- Path around a water body: correctly routes through available transitions
- Same-region query: returns empty waypoint list (caller uses local A* directly)
- Unreachable destination: returns None

### Step 5: HierarchicalPath component and movement integration

Add `HierarchicalPath` to villager entities. Implement `move_toward_hierarchical`. Wire into AI call sites alongside existing `move_toward_cached`.

Migration: replace `move_toward_cached` calls with `move_toward_hierarchical` at call sites where the destination may be far (Seek, Hauling, Exploring, FleeHome). Keep `move_toward_cached` for short-range states (Wander, Seek(ExitBuilding)).

### Step 6: Incremental NavGraph update

Add dirty-region tracking. Process dirty set at end of `Game::step()`. Invalidate affected entity paths.

Tests:
- Place a building that blocks a transition: nav graph updates, paths reroute
- Cut a forest tile inside a region: zone_map recomputed, transitions unchanged
- Build a road: move costs update, intra-region edge costs decrease

### Step 7: Debug overlay

Add a "nav mesh" overlay to the debug view cycle (press `o`). Draw region boundaries as grid lines, transition points as highlighted tiles, and the high-level path of the selected entity as a dotted line between transition points.

## Risks and mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Region boundaries create suboptimal paths (entity zigzags to transition point instead of cutting straight) | Slightly longer paths than true A* | Transition points at midpoints of walkable border runs minimize detour. For adjacent-region destinations, skip hierarchy entirely (same as same-region check but extended to 1-region-away). |
| Incremental update misses a terrain change, nav graph goes stale | Entity follows impossible path, gets stuck | Blocked-waypoint detection from PathCache Tier 2 catches this at the local level. Entity reroutes within 1-2 ticks. Belt and suspenders. |
| Zone computation incorrect after complex terrain change (e.g., bridge connects two previously separate zones) | High-level path says unreachable when it is reachable | Full region re-flood-fill on any terrain change in the region. Cannot miss connectivity changes within the region. |
| Many simultaneous terrain changes (large construction, seasonal flood) dirty many regions at once | Spike in recomputation cost | Cap dirty region processing to 8 regions per tick. Remaining regions carry over to next tick. Staleness is bounded by the 600-tick hierarchical path timeout. |
| Memory fragmentation from NavGraph edge Vecs | Allocator pressure over long games | Edge lists are small (4-12 entries) and rarely reallocated. If measured as a problem, switch to a flat edge array with offset table. |
| Hierarchical path is longer than optimal tile-level A* for medium distances (20-40 tiles across 2-3 regions) | Villagers take slightly suboptimal routes | Acceptable for ant-colony aesthetic. The path is valid and close to optimal. Pure A* with sufficient budget would be better but is too expensive at scale. |

## Pillar alignment

- **Pillar 5 (Scale Over Fidelity):** Primary motivation. Enables long-distance pathfinding at O(small) cost regardless of map size. Combined with path caching and flow fields, this closes the pathfinding performance gap for the 500-1000 villager target.
- **Pillar 1 (Geography Shapes Everything):** The nav graph makes terrain barriers *structurally* meaningful. A river that splits a region into two zones creates a real connectivity constraint. Bridges become graph edges. Mountain ranges that eliminate transitions between regions create natural borders. The settlement's reachable area is defined by the nav graph's connected component.
- **Pillar 3 (Explore -> Expand -> Exploit -> Endure):** Long-distance pathfinding enables outpost mechanics. Villagers can reliably path from the main settlement to a distant mining camp 150 tiles away. Without this, cross-map expansion is impossible.
- **Pillar 4 (Observable Simulation):** The nav mesh debug overlay makes the "shape of the world" visible. Players can see why villagers take a certain route -- the transition points and region connectivity tell the story of how geography channels movement.
- **Pillar 2 (Emergent Complexity):** When a bridge is built (creating a new transition between previously disconnected zones), the nav graph updates and villagers suddenly start using the new route. The player sees behavior change as a direct result of terrain modification. Emergent, not scripted.

## Non-goals

- **Dynamic obstacle avoidance between entities.** Hierarchical pathfinding routes around terrain, not other villagers. Entity-entity collision is a separate concern (and currently not a problem given the ant-colony aesthetic).
- **Optimal paths.** Hierarchical paths are near-optimal, not provably shortest. The small detour to transition points is acceptable and usually unnoticeable.
- **Three or more hierarchy levels.** Two levels (regions + tiles) are sufficient for maps up to 1024x1024. A third level (super-regions of 4x4 regions) would help for 2048+ maps but is not needed for the foreseeable roadmap.
- **Precomputed local paths within regions.** We do not cache tile-level paths between transition points. The per-entity PathCache handles this dynamically, which is simpler and handles entity-specific start/end positions naturally.

## File placement

```
src/pathfinding/
    mod.rs              # pub mod region, graph; re-exports
    region.rs           # Region, zone computation, border transitions
    graph.rs            # NavGraph, high-level A*, incremental update
```

The pathfinding module is separate from `ecs/` because it operates on the TileMap, not on entities. It is consumed by `ecs/ai.rs` (movement functions) and `game/mod.rs` (dirty region processing, graph storage).
