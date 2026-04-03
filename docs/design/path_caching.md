# Path Caching

## Problem

Every villager in a movement state (Seek, Exploring, Hauling, Farming, Working, Building, FleeHome) calls `move_toward_astar` every tick. That function runs a full A* search from the entity's current position to its destination, explores up to `d * 4` nodes (capped at 600), then returns only the next single tile. The entire computed path is discarded.

At 100 villagers each moving every tick, that is 100 A* searches per tick. At 500 villagers (Phase 3 target), it becomes 500 searches per tick, each allocating a `Vec<Node>`, a `BinaryHeap`, and a `visited` bitmap of `width * height` booleans. This is the single largest CPU cost in the simulation and the primary blocker for the "500 villagers at 60fps" target in Pillar 5.

### Specific call sites (src/ecs/ai.rs)

| State | Line(s) | Destination | Frequency |
|-------|---------|-------------|-----------|
| FleeHome | 546 | nearest stockpile | every tick while fleeing |
| Hauling | 625 | stockpile (target_x/y) | every tick while hauling |
| Farming | 765 | farm (target_x/y) | every tick while walking to farm |
| Working | 833 | workshop (target_x/y) | every tick while walking to workshop |
| Exploring | 897, 915, 943 | frontier / resource | every tick while exploring |
| Seek(Food) | 1083 | berry bush / stockpile | every tick while seeking food |
| Seek(Stockpile) | 1118 | stockpile | every tick while seeking food from stockpile |
| Seek(Stone) | 1173 | stone deposit | every tick while seeking stone |
| Seek(BuildSite) | 1238 | build site | every tick while seeking build |
| Seek(Wood) | 1277 | forest tile | every tick while seeking wood |
| Seek(Hut) | 1023 | hut | every tick at night |
| Seek(ExitBuilding) | 984 | nearest outdoor tile | every tick while exiting building |
| Building (timer>0) | 1378, 1404 | build site | every tick while walking to build |

All 18+ call sites share the same pattern: compute full A*, use one tile, throw the rest away.

## Design

Three tiers, implemented incrementally. Each tier is independently shippable.

### Tier 1: Per-Entity Path Cache (waypoint list)

#### Data structure

New ECS component:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathCache {
    /// Waypoints from current position to destination, in order.
    /// Entity pops from the front as it reaches each waypoint.
    pub waypoints: Vec<(f64, f64)>,
    /// The destination these waypoints lead to. Used for invalidation.
    pub dest_x: f64,
    pub dest_y: f64,
    /// Tick when this path was computed. Used for staleness check.
    pub computed_tick: u64,
    /// Index of next waypoint to follow (avoids Vec::remove(0) cost).
    pub cursor: usize,
}
```

Using `cursor` index instead of `VecDeque` or `Vec::remove(0)` because:
- `Vec::remove(0)` is O(n) shift on every waypoint reached
- `VecDeque` adds overhead for a typical path of 10-40 waypoints
- An index into a `Vec` is zero-cost advancement

#### New function: `astar_full_path`

Add to `TileMap`:

```rust
pub fn astar_full_path(
    &self,
    sx: f64, sy: f64,
    gx: f64, gy: f64,
    max_steps: usize,
) -> Option<Vec<(f64, f64)>>
```

This is the existing `astar_next` logic but instead of tracing back to find only the first step, it traces the full parent chain and returns the complete waypoint list (start excluded, destination included). The search itself is identical -- same heuristic, same move costs, same budget. We keep `astar_next` for short-range movement (d < 5) where caching is not worth it.

#### Modified `move_toward_astar`

```rust
pub(super) fn move_toward_cached(
    pos: &Position,
    tx: f64, ty: f64,
    speed: f64,
    vel: &mut Velocity,
    map: &TileMap,
    cache: &mut PathCache,
    current_tick: u64,
) -> f64 {
    let d = dist(pos.x, pos.y, tx, ty);
    if d < 0.5 {
        return d;
    }

    // Short distance: direct movement, no caching overhead
    if d <= 3.0 {
        return move_toward(pos, tx, ty, speed, vel);
    }

    // Check if cache is valid
    let cache_valid = cache.cursor < cache.waypoints.len()
        && (cache.dest_x - tx).abs() < 0.5
        && (cache.dest_y - ty).abs() < 0.5
        && (current_tick - cache.computed_tick) < 120;  // stale after ~2 game-seconds

    if !cache_valid {
        // Recompute
        let budget = (d as usize * 4).min(600);
        if let Some(path) = map.astar_full_path(pos.x, pos.y, tx, ty, budget) {
            cache.waypoints = path;
            cache.dest_x = tx;
            cache.dest_y = ty;
            cache.computed_tick = current_tick;
            cache.cursor = 0;
        } else {
            // No path found -- fall back to direct movement
            return move_toward(pos, tx, ty, speed, vel);
        }
    }

    // Follow next waypoint
    let (wx, wy) = cache.waypoints[cache.cursor];
    let wd = dist(pos.x, pos.y, wx, wy);

    if wd < 0.8 {
        // Reached this waypoint, advance
        cache.cursor += 1;
        if cache.cursor >= cache.waypoints.len() {
            // Reached destination
            return d;
        }
        let (wx2, wy2) = cache.waypoints[cache.cursor];
        move_toward(pos, wx2, wy2, speed, vel);
    } else {
        move_toward(pos, wx, wy, speed, vel);
    }
    d
}
```

#### Invalidation conditions

A cached path is invalidated (forcing recompute) when:

1. **Destination changed.** `(dest_x, dest_y)` differs from the target passed to `move_toward_cached` by more than 0.5 tiles. This catches state transitions (villager switches from Seek(Wood) to Hauling) automatically because the target coordinates change.

2. **Staleness timeout.** Path is older than 120 ticks (~2 in-game seconds at normal speed). This catches gradual terrain changes (building construction, road placement, forest regrowth) without needing an explicit notification system.

3. **Blocked waypoint.** When following the next waypoint, if that tile is no longer walkable (building placed, wall constructed), the path is invalidated immediately. Check: `!map.is_walkable(wx, wy)`.

4. **State transition.** When the AI function returns a new `BehaviorState` with different target coordinates, the old `PathCache` is naturally invalidated on the next tick by condition (1). No explicit clearing needed.

5. **Entity drift.** If the entity is more than 3 tiles away from its next waypoint (pushed by collision, teleported by building exit logic), invalidate and recompute. This prevents following a stale path after displacement.

#### Integration with AI states

Every call site in `ai.rs` that currently calls `move_toward_astar(pos, tx, ty, speed, &mut vel, map)` changes to `move_toward_cached(pos, tx, ty, speed, &mut vel, map, &mut path_cache, tick)`. The `PathCache` component is added to all villager entities at spawn. The current tick counter is passed down from `Game::step()` through the AI system.

States that do NOT need caching (keep using direct movement):
- **Wander** -- random direction, no destination
- **Idle** -- stationary
- **Eating** -- stationary
- **Sleeping** -- stationary
- **Gathering** -- stationary (timer countdown at resource)
- **Captured** -- frozen

#### Memory cost

Per entity: `Vec<(f64, f64)>` with typical length 10-40 waypoints = 160-640 bytes. At 500 entities: 80-320 KB total. Negligible.

#### Expected speedup

For a villager walking 30 tiles to a stockpile at 1 tile/tick: current approach runs A* 30 times. With caching, A* runs once (plus maybe 1 recompute if terrain changes). That is a 15-30x reduction in A* calls for moving entities. Total per-tick A* budget drops from O(entities) to O(entities_that_just_started_moving + entities_whose_path_expired).

### Tier 2: Blocked-Waypoint Early Detection

Instead of checking only the next waypoint, scan ahead 3 waypoints each tick. Cost: 3 `is_walkable` lookups per entity per tick (trivial). Benefit: detect newly placed buildings or walls before the entity walks into them, allowing smoother rerouting.

```rust
// In move_toward_cached, after cache_valid check:
for i in cache.cursor..cache.waypoints.len().min(cache.cursor + 3) {
    let (wx, wy) = cache.waypoints[i];
    if !map.is_walkable(wx, wy) {
        // Path blocked ahead -- recompute from current position
        let budget = (d as usize * 4).min(600);
        if let Some(path) = map.astar_full_path(pos.x, pos.y, tx, ty, budget) {
            cache.waypoints = path;
            cache.cursor = 0;
            cache.computed_tick = current_tick;
        }
        break;
    }
}
```

### Tier 3: Shared Flow Fields for Common Destinations

#### Motivation

Many villagers walk to the same destinations: stockpile, active build site, huts at night. With 200 villagers hauling resources, 200 individual A* paths to the same stockpile are redundant. A flow field computes the optimal direction from every tile to a single destination, and all entities heading there just look up their tile.

#### Data structure

```rust
pub struct FlowField {
    /// For each tile, the (dx, dy) direction to move toward the destination.
    /// (0, 0) means "you are at the destination" or "unreachable."
    directions: Vec<(i8, i8)>,
    /// Cost from each tile to the destination (used for tie-breaking).
    costs: Vec<f32>,
    width: usize,
    height: usize,
    /// World position of the destination (for cache key).
    dest_x: usize,
    dest_y: usize,
    /// Tick when computed.
    computed_tick: u64,
}

impl FlowField {
    pub fn direction_at(&self, x: usize, y: usize) -> (i8, i8) {
        if x < self.width && y < self.height {
            self.directions[y * self.width + x]
        } else {
            (0, 0)
        }
    }
}
```

#### Computation

Reverse Dijkstra from the destination: start at the destination tile with cost 0, expand outward using the same `Terrain::move_cost()` weights. For each tile, record the direction back toward the neighbor with lowest cost. Budget: explore up to `radius * radius` tiles (e.g., 60x60 = 3600 tiles for a 60-tile radius around the stockpile). One BFS pass, no per-entity cost.

```rust
impl TileMap {
    pub fn compute_flow_field(
        &self,
        dest_x: usize,
        dest_y: usize,
        radius: usize,
    ) -> FlowField { ... }
}
```

#### Which destinations get flow fields

Not every destination. Only destinations where multiple entities converge simultaneously:

| Destination | Trigger | Radius | Recompute interval |
|-------------|---------|--------|--------------------|
| Stockpile | always (central hub) | 60 tiles | every 200 ticks or on terrain change near stockpile |
| Active build site | when >= 3 villagers assigned | 30 tiles | every 100 ticks or when site completes |
| Hut cluster center | nighttime only | 40 tiles | once per night cycle |

Flow fields are stored in a `HashMap<(usize, usize), FlowField>` on the `Game` struct, keyed by destination tile. Recomputed lazily: if a villager requests direction from a flow field older than its recompute interval, regenerate it before returning.

#### Integration with per-entity path cache

Flow fields and per-entity caches coexist. The lookup order:

1. If distance to destination < 3 tiles: direct movement (no cache, no flow field)
2. If a flow field exists for this destination and is fresh: use it (zero per-entity cost)
3. Otherwise: use per-entity `PathCache` (Tier 1)

In `move_toward_cached`, add a `flow_fields` parameter:

```rust
pub(super) fn move_toward_cached(
    pos: &Position,
    tx: f64, ty: f64,
    speed: f64,
    vel: &mut Velocity,
    map: &TileMap,
    cache: &mut PathCache,
    current_tick: u64,
    flow_fields: &HashMap<(usize, usize), FlowField>,  // Tier 3
) -> f64 {
    let d = dist(pos.x, pos.y, tx, ty);
    if d < 0.5 { return d; }
    if d <= 3.0 { return move_toward(pos, tx, ty, speed, vel); }

    // Try flow field first
    let dest_key = (tx.round() as usize, ty.round() as usize);
    if let Some(ff) = flow_fields.get(&dest_key) {
        if (current_tick - ff.computed_tick) < 200 {
            let (dx, dy) = ff.direction_at(pos.x.round() as usize, pos.y.round() as usize);
            if dx != 0 || dy != 0 {
                let mag = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();
                vel.dx = (dx as f64 / mag) * speed;
                vel.dy = (dy as f64 / mag) * speed;
                return d;
            }
        }
    }

    // Fall back to per-entity cache (Tier 1)
    // ... existing PathCache logic ...
}
```

#### Memory cost

Per flow field: `Vec<(i8, i8)>` + `Vec<f32>` for the full map = `width * height * 6` bytes. On a 200x200 map: 240 KB per flow field. With 3-5 active flow fields: ~1 MB total. Acceptable.

#### Expected speedup

At 200 villagers hauling to stockpile: instead of 200 A* searches (Tier 1 reduces to ~10 recomputes/tick), flow fields reduce to 0 A* searches and 1 flow field recompute every 200 ticks. Per-entity cost becomes a single array lookup. This is the key unlock for 500+ villagers.

## Implementation Plan

### Tier 1 (path caching) -- do first, largest bang for buck

1. Add `PathCache` component to `src/ecs/components.rs`
2. Add `astar_full_path` to `src/tilemap.rs` (extract from existing `astar_next`, return full chain)
3. Add `move_toward_cached` to `src/ecs/ai.rs`
4. Attach `PathCache` to villager entities in `src/ecs/spawn.rs`
5. Thread `current_tick` through to AI system (from `Game::step`)
6. Replace all 18 `move_toward_astar` call sites in `ai.rs` with `move_toward_cached`
7. Add serialization support for `PathCache` (save/load)
8. Tests:
   - Path cache reuse: entity walks 20 tiles, A* called once
   - Cache invalidation on destination change
   - Cache invalidation on blocked waypoint
   - Cache invalidation on staleness (120 tick timeout)
   - Drift detection: entity teleported, cache recomputed
   - Short distance bypasses cache (d < 3)
   - Unreachable destination falls back to direct movement

### Tier 2 (look-ahead) -- small addition on top of Tier 1

8. Add 3-waypoint look-ahead in `move_toward_cached`
9. Test: place wall on cached path, entity reroutes before hitting it

### Tier 3 (flow fields) -- Phase 3 feature, when population exceeds ~100

10. Add `FlowField` struct to `src/tilemap.rs`
11. Add `compute_flow_field` to `TileMap` (reverse Dijkstra)
12. Add `flow_fields: HashMap<(usize, usize), FlowField>` to `Game`
13. Recompute stockpile flow field every 200 ticks in `Game::step`
14. Add flow field lookup to `move_toward_cached`
15. Add flow field overlay to debug view (`render.rs`, overlay cycle)
16. Tests:
    - Flow field produces correct directions on simple map
    - Flow field respects terrain costs (prefers roads)
    - Flow field handles unreachable tiles (water, walls)
    - Flow field + path cache fallback works together
    - Flow field recompute on terrain change

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Stale paths cause villagers to walk into new buildings | Visible glitch, villager stuck | Tier 2 look-ahead + blocked-waypoint invalidation + 120-tick staleness cap |
| `astar_full_path` is slower than `astar_next` (more parent-chain tracing) | Slightly slower single computation | Amortized over 15-30 ticks of reuse; net speedup is massive |
| Flow field memory at large map sizes (400x400) | 960 KB per field | Limit to 3-5 concurrent flow fields; use radius-bounded computation instead of full-map |
| Serialization of `PathCache` bloats save files | Larger saves | Clear all caches before save (waypoints are recomputable); serialize as empty |
| Path cache component adds ECS query overhead | Slower entity iteration | `PathCache` only on villagers (not prey/predators); hecs query overhead is negligible for one extra component |
| Flow field direction is coarse (8 directions) | Slightly jagged movement | Acceptable for ant-colony aesthetic; interpolation possible later if needed |

## Pillar Alignment

- **Pillar 5 (Scale Over Fidelity):** This is the core motivation. Path caching is the single highest-leverage optimization for reaching 500 villagers at 60fps. Flow fields are O(1) per entity per tick.
- **Pillar 4 (Observable Simulation):** Cached paths could be drawn as debug overlay (dotted line from entity to destination), making pathfinding visible and debuggable.
- **Pillar 2 (Emergent Complexity):** Flow fields naturally create "ant trail" movement patterns where many villagers follow the same optimal route, which looks emergent and organic. Traffic heat maps become more meaningful when villagers actually follow consistent paths.
- **Pillar 1 (Geography Shapes Everything):** Flow fields encode terrain costs, so villagers visibly prefer roads and avoid mountains. The settlement's path network becomes a readable feature of the landscape.

## Non-Goals

- **Navigation mesh.** Overkill for a tile-based grid. A* on tiles is correct and sufficient; we just need to stop recomputing it.
- **Hierarchical pathfinding (HPA*).** Worth investigating at 1000+ villagers or 500x500+ maps, but flow fields solve the common-destination case more directly.
- **Steering behaviors / local avoidance.** Villagers currently overlap and that is fine for the ant-colony aesthetic. Collision avoidance is a separate concern.
- **Dynamic obstacle prediction.** We do not predict where other entities will be. Invalidation handles obstacles that actually appear.
