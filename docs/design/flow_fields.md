# Flow Fields for Common Destinations

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Pillar: 5 (Scale Over Fidelity), 1 (Geography Shapes Everything), 2 (Emergent Complexity)*
*Phase: 3 (Scale)*
*Depends on: Path Caching (Tier 1), Spatial Hash Grid*
*Unlocks: 500+ villagers sharing amortized pathfinding, visible ant-trail movement*

## Problem

Path Caching (Tier 1) reduces A* calls from once-per-tick to once-per-journey, but at 200+ villagers many of those journeys share a destination. When 80 haulers walk to the stockpile, 80 independent A* searches compute nearly identical paths. When 40 villagers head home at nightfall, 40 searches fan out to the same hut cluster. When 15 builders converge on a build site, 15 searches overlap.

At 500 population the path budget is 3ms per tick. Tier 1 caching keeps recomputes to ~10-30 per tick under normal conditions, but burst events blow the budget:

| Event | Simultaneous pathfinds | With Tier 1 caching | Cost at 500 pop |
|-------|----------------------|---------------------|-----------------|
| Haul wave after big harvest | 80-120 villagers seek stockpile | 80-120 A* (all new journeys) | ~4ms spike |
| Nightfall | 60-100 villagers seek huts | 60-100 A* (all change destination at once) | ~3ms spike |
| Build site placed | 10-20 builders assigned | 10-20 A* | ~1ms spike |
| Threat flee | 30-50 villagers flee toward stockpile | 30-50 A* | ~2ms spike |

These spikes are correlated -- they happen on the same tick because the trigger (nightfall, harvest, build order) hits everyone simultaneously. Tier 1 caching amortizes steady-state cost but does not help with synchronized destination changes.

A flow field inverts the problem: instead of N entities each searching toward one destination, compute one reverse-Dijkstra from the destination and let all N entities read from it. Cost becomes O(map_area) once, amortized across all users, instead of O(N * path_length * search_factor).

## Design

### Data Structure

```rust
pub struct FlowField {
    /// For each tile, the direction to step toward the destination.
    /// (0, 0) means destination tile or unreachable.
    /// Values are one of the 8 cardinal/diagonal directions.
    directions: Vec<(i8, i8)>,

    /// Cost-to-destination for each tile. f32::MAX means unreachable.
    /// Used for: (a) computing directions, (b) tie-breaking when
    /// an entity is equidistant from two waypoints, (c) debug overlay.
    costs: Vec<f32>,

    width: usize,
    height: usize,

    /// Destination tile (for cache keying and invalidation).
    dest_x: usize,
    dest_y: usize,

    /// Tick when this field was last computed.
    computed_tick: u64,

    /// Maximum radius from destination that was computed.
    /// Tiles beyond this radius have cost = f32::MAX.
    radius: usize,
}
```

**Why `Vec<(i8, i8)>` for directions?** Each direction is one of 9 values (8 compass directions + zero). Two bytes per tile. On a 256x256 map a full-map field is 128 KB for directions alone. With the bounded radius (typically 60-80 tiles), the Vec is still full-map-sized but only tiles within the radius have meaningful values -- the rest default to `(0, 0)`. This avoids offset math and lets `direction_at` be a direct index.

**Why `Vec<f32>` for costs?** Terrain move costs are fractional (road = 0.7, forest = 1.7, mountain = 4.0). Integer costs would lose the road preference. `f32` matches the existing `Terrain::move_cost()` return type. Per-field cost: 256 * 256 * 4 = 256 KB. With 3-5 active fields that is 1-1.5 MB total. Acceptable.

**Alternative considered: `HashMap<(usize, usize), (i8, i8)>` storing only reachable tiles.** Rejected because the lookup hot path (every entity, every tick) benefits from direct indexing over hash probing. The memory savings do not justify the per-lookup overhead.

### Computation: Reverse Dijkstra

Start at the destination with cost 0. Expand outward using a min-heap, respecting `Terrain::move_cost()` for each neighbor. For each tile, record the direction pointing toward the neighbor that provided its lowest cost (the direction to walk to get closer to the destination). Stop when the heap is exhausted or all tiles within the radius have been visited.

```rust
impl TileMap {
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

        // Min-heap: (cost, x, y)
        let mut heap = BinaryHeap::new();
        let dest_idx = dest_y * w + dest_x;
        costs[dest_idx] = 0.0;
        heap.push(Reverse((OrderedFloat(0.0f32), dest_x, dest_y)));

        let neighbors: [(i32, i32); 8] = [
            (-1, -1), (0, -1), (1, -1),
            (-1,  0),          (1,  0),
            (-1,  1), (0,  1), (1,  1),
        ];

        while let Some(Reverse((OrderedFloat(cost), cx, cy))) = heap.pop() {
            let idx = cy * w + cx;
            if cost > costs[idx] { continue; } // stale entry

            // Radius bound: don't expand beyond radius from destination
            let dx = (cx as i32 - dest_x as i32).unsigned_abs() as usize;
            let dy = (cy as i32 - dest_y as i32).unsigned_abs() as usize;
            if dx > radius || dy > radius { continue; }

            for &(nx_off, ny_off) in &neighbors {
                let nx = cx as i32 + nx_off;
                let ny = cy as i32 + ny_off;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let (nxu, nyu) = (nx as usize, ny as usize);
                let terrain = self.terrain_at(nxu, nyu);
                if !terrain.is_walkable() { continue; }

                // Diagonal movement costs sqrt(2) * terrain cost
                let step_cost = terrain.move_cost()
                    * if nx_off != 0 && ny_off != 0 { 1.414 } else { 1.0 };
                let new_cost = cost + step_cost;
                let n_idx = nyu * w + nxu;

                if new_cost < costs[n_idx] {
                    costs[n_idx] = new_cost;
                    // Direction points FROM neighbor TOWARD current tile
                    // (i.e., the direction this neighbor should walk)
                    directions[n_idx] = (-nx_off as i8, -ny_off as i8);
                    heap.push(Reverse((OrderedFloat(new_cost), nxu, nyu)));
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
```

**Cost for a radius-60 field:** The bounded Dijkstra visits at most `(2*60+1)^2 = 14,641` tiles. Each tile expands 8 neighbors. With the heap and early-exit on stale entries, real work is ~20-40K operations. Measured estimate: 0.5-1.0ms on a single core. This is a one-time cost shared by all entities using the field.

### Lookup

```rust
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

    /// True if this field is stale and should be recomputed.
    pub fn is_stale(&self, current_tick: u64, max_age: u64) -> bool {
        current_tick.saturating_sub(self.computed_tick) >= max_age
    }
}
```

Per-entity cost per tick: one array index + one branch. Effectively zero.

### Flow Field Registry

Flow fields are managed by a registry on the `Game` struct, not by individual entities.

```rust
pub struct FlowFieldRegistry {
    /// Active flow fields keyed by destination tile.
    fields: HashMap<(usize, usize), FlowField>,

    /// Demand counter: how many entities requested each destination this tick.
    /// Reset every tick. Used to decide which fields to keep alive.
    demand: HashMap<(usize, usize), u32>,
}
```

The registry provides:
- `get(dest_x, dest_y) -> Option<&FlowField>` -- lookup for entities.
- `request(dest_x, dest_y)` -- entity signals it wants to use a flow field for this destination. Increments demand counter.
- `maintain(map, tick)` -- called once per tick in `Game::step()`. Recomputes stale fields, creates new ones for high-demand destinations, evicts low-demand ones.

### When to Generate Flow Fields

Not every destination gets a flow field. The cost of computing one (0.5-1ms) is only worth it when enough entities share the destination to recoup the investment versus individual A* paths.

**Threshold rule:** A flow field is generated when `demand >= FLOW_FIELD_THRESHOLD` for a destination in a single tick. Recommended threshold: **5 entities**.

At 5 entities, the flow field cost (~0.8ms) replaces 5 A* searches (~0.4ms each = 2ms). Net savings: ~1.2ms. At 20 entities the savings are ~7.2ms. The threshold is conservative -- even at 5 it pays for itself.

**Which destinations qualify in practice:**

| Destination | When demand spikes | Typical demand | Radius | Max age (ticks) |
|-------------|-------------------|----------------|--------|-----------------|
| Stockpile | Always (hauling hub) | 20-80 | 80 | 200 |
| Hut cluster centroid | Nightfall | 30-60 | 50 | 100 (one night) |
| Active build site | Build order placed | 5-20 | 40 | 100 |
| Flee destination (stockpile) | Threat event | 10-50 | 80 | 50 (short-lived) |
| Popular farm cluster | Harvest season | 5-15 | 40 | 150 |

**Stockpile is special:** It always has a flow field, regardless of demand, because it is the single highest-traffic destination in the game. Its field is precomputed at game start and refreshed every 200 ticks or on terrain change within the radius.

**Hut cluster fields are time-bounded:** Generated at nightfall (when `is_night` transitions to true), evicted at dawn. The centroid of the hut cluster is computed from hut positions via the spatial grid.

**Build site fields are event-driven:** Generated when a build site is placed and demand exceeds threshold. Evicted when the building completes.

### Demand Tracking

Each tick, during the AI phase, before an entity calls `move_toward_cached`, it calls `flow_field_registry.request(dest_x, dest_y)`. This is a HashMap increment -- negligible cost. At the end of the tick, `maintain()` reads the demand map:

1. For each destination with `demand >= FLOW_FIELD_THRESHOLD` and no existing field (or stale field): compute a new flow field.
2. For each existing field with `demand == 0` for 3 consecutive ticks: evict it (no one is using it).
3. For each existing field past its max age: recompute if demand > 0, otherwise evict.
4. Cap total active fields at `MAX_ACTIVE_FIELDS` (default: 8). If the cap is hit, evict the lowest-demand field.

**Budget control:** At most 2 flow fields are computed per tick (2ms budget). If more than 2 need recomputing, the rest are deferred to subsequent ticks. Entities whose flow field is pending fall back to Tier 1 path caching -- they are never stuck.

### Integration: Entity Decision Tree

When a moving entity needs to step toward its destination, the lookup order is:

```
1. Distance < 3 tiles?
   -> Direct movement (move_toward). No cache, no flow field.

2. Flow field exists for this destination AND covers my tile AND is fresh?
   -> Read direction from flow field. Step that way.

3. Per-entity PathCache is valid?
   -> Follow cached waypoints (Tier 1).

4. Nothing cached?
   -> Compute A* full path, store in PathCache (Tier 1).
```

In code, `move_toward_cached` gains a `flow_fields: &FlowFieldRegistry` parameter:

```rust
pub(super) fn move_toward_cached(
    pos: &Position,
    tx: f64, ty: f64,
    speed: f64,
    vel: &mut Velocity,
    map: &TileMap,
    cache: &mut PathCache,
    current_tick: u64,
    flow_fields: &FlowFieldRegistry,
) -> f64 {
    let d = dist(pos.x, pos.y, tx, ty);
    if d < 0.5 { return d; }
    if d <= 3.0 { return move_toward(pos, tx, ty, speed, vel); }

    // Try flow field
    let dest_key = (tx.round() as usize, ty.round() as usize);
    if let Some(ff) = flow_fields.get(dest_key.0, dest_key.1) {
        let px = pos.x.round() as usize;
        let py = pos.y.round() as usize;
        if ff.covers(px, py) && !ff.is_stale(current_tick, 200) {
            let (dx, dy) = ff.direction_at(px, py);
            if dx != 0 || dy != 0 {
                let mag = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();
                vel.dx = (dx as f64 / mag) * speed;
                vel.dy = (dy as f64 / mag) * speed;
                return d;
            }
        }
    }

    // Fall back to per-entity PathCache (Tier 1)
    // ... existing path cache logic ...
}
```

**The entity does not know or care whether it is using a flow field or A*.** The interface is the same: call `move_toward_cached`, get velocity set. The flow field is a transparent optimization layer.

### Invalidation

Flow fields encode terrain costs. When terrain changes, affected fields must be recomputed.

| Terrain change | Invalidation scope | Response |
|---------------|-------------------|----------|
| Building placed/demolished | Fields whose radius covers the building tile | Mark stale, recompute on next `maintain()` |
| Road formed (traffic threshold) | Fields whose radius covers the road tile | Mark stale (road lowers cost, field is suboptimal but not wrong) |
| Tree cut / regrowth | Fields whose radius covers the tile | Mark stale (low priority, forest cost change is minor) |
| Stockpile moved/destroyed | Stockpile flow field | Evict immediately, recompute for new stockpile |
| Build site completed | Build site flow field | Evict immediately |

**Coarse invalidation strategy:** Rather than tracking which specific tiles changed and which fields they affect, use a simple dirty flag. `Game` tracks a `terrain_dirty_tick: u64` that is bumped whenever terrain changes (building placed, road formed, tree cut). During `maintain()`, any flow field with `computed_tick < terrain_dirty_tick` is marked stale. This is conservative (recomputes fields even if the change was outside their radius) but simple and correct.

**Fine-grained invalidation (future optimization):** Track dirty tiles in a `Vec<(usize, usize)>` per tick. During `maintain()`, check if any dirty tile falls within a field's radius before marking it stale. Only worth adding if profiling shows excessive recomputes.

### Handling Multiple Stockpiles

When the settlement has multiple stockpiles, each gets its own flow field. An entity hauling resources picks the nearest stockpile (via spatial grid query) and uses that stockpile's flow field. If a villager is closer to stockpile A but stockpile B's flow field covers them too, they use A's field -- the spatial grid nearest-query already resolved the destination before the flow field is consulted.

### Edge Cases

**Entity between two flow field tiles.** Entity positions are continuous `(f64, f64)` but flow field directions are per-tile. The entity rounds its position to the nearest tile to look up the direction. At sub-tile scale (< 1 tile from destination), it falls through to direct movement (step 1 in the decision tree).

**Entity at the edge of a flow field's radius.** `covers()` returns false, entity falls through to Tier 1 path cache. The path cache navigates the entity closer to the destination until the flow field covers it. There is no discontinuity -- the path cache and flow field agree on the optimal route because both use the same terrain costs.

**Unreachable destination (water, walls blocking all paths).** The Dijkstra never reaches tiles on the other side of the barrier. `covers()` returns false for those tiles. Entities on the far side use Tier 1 path cache, which will also fail A*, triggering the existing fallback behavior (direct movement toward destination, which gets stuck and triggers a behavior state change in the AI).

**Flow field direction causes entity to walk into newly placed building.** The entity's movement system already checks walkability before applying velocity. If the next tile is unwalkable, the entity stops, its path cache (if any) is invalidated, and next tick it falls back to A* which routes around the obstacle. The stale flow field is also marked dirty by the building placement event and will be recomputed within 1-2 ticks.

## Memory Budget

| Component | Size per field (256x256 map) | Typical count | Total |
|-----------|------------------------------|---------------|-------|
| `directions: Vec<(i8, i8)>` | 128 KB | 3-5 | 384-640 KB |
| `costs: Vec<f32>` | 256 KB | 3-5 | 768-1280 KB |
| `FlowFieldRegistry` overhead | ~1 KB | 1 | 1 KB |
| **Total** | | | **1.2-1.9 MB** |

At max 8 concurrent fields: ~3 MB. Well within budget for a simulation targeting 500+ entities.

On a future 512x512 map, per-field cost quadruples to 1.5 MB. Cap fields at 5-6 or use radius-bounded allocation (allocate only the bounding rect, not the full map). The radius-bounded approach trades index simplicity for memory -- worth it only at 512x512+.

## Performance Budget

| Operation | Cost | Frequency | Per-tick budget |
|-----------|------|-----------|-----------------|
| Compute one flow field (r=60) | 0.5-1.0ms | 1-2 per tick (amortized, most ticks 0) | 0-2ms |
| Entity flow field lookup | ~5ns (array index) | 200-400 per tick | ~0.002ms |
| Registry maintain | ~0.01ms | 1 per tick | 0.01ms |
| Demand tracking (HashMap increment) | ~10ns | 500 per tick | 0.005ms |
| **Typical per-tick total** | | | **~0.02ms** (steady state) |
| **Worst-case per-tick total** | | | **~2ms** (2 recomputes) |

Compare to the replaced cost: 80 simultaneous A* searches at ~0.1ms each = 8ms spike. Flow fields reduce this to ~0.02ms steady state with occasional 1-2ms recomputes spread over time.

## Implementation Plan

### Step 1: FlowField struct and computation

Add `FlowField` to `src/tilemap.rs` (alongside the existing A* code). Implement `compute_flow_field` on `TileMap`. Unit tests:

- Flow field on open terrain: all directions point toward destination
- Flow field around water barrier: directions route around it
- Flow field respects terrain costs: prefers road over forest
- Unreachable tile has `(0, 0)` direction and `f32::MAX` cost
- Destination tile has `(0, 0)` direction and `0.0` cost
- Radius bound: tiles beyond radius are `f32::MAX`
- Diagonal directions are correct (not just cardinal)

### Step 2: FlowFieldRegistry

Add `src/ecs/flow_fields.rs` (or extend `tilemap.rs`). Implement the registry with demand tracking, creation, staleness, and eviction. Unit tests:

- Request below threshold does not create field
- Request at threshold creates field
- Stale field is recomputed when demand exists
- Zero-demand field is evicted after 3 ticks
- Max active fields cap is respected
- Budget cap: at most 2 computes per tick

### Step 3: Integrate with move_toward_cached

Add `flow_fields: &FlowFieldRegistry` parameter to `move_toward_cached`. Add the flow field lookup before the path cache fallback. Thread the registry through from `Game::step()` to the AI system. Tests:

- Entity uses flow field when available (no A* called)
- Entity falls back to path cache when flow field does not cover tile
- Entity falls back to path cache when flow field is stale
- Short-distance movement bypasses flow field (d < 3)

### Step 4: Demand wiring in AI states

In `ai_villager`, before each `move_toward_cached` call, call `registry.request(dest)` for the destination. This wires up automatic demand tracking. States that benefit:

- Hauling -> stockpile (highest traffic)
- Seek(Hut) at night -> hut position
- Seek(BuildSite) -> build site position
- FleeHome -> stockpile
- Seek(Stockpile) -> stockpile (food retrieval)

### Step 5: Stockpile always-on field

In `Game::step()`, ensure the stockpile flow field is always present. Compute it at game start and refresh it on terrain dirty or every 200 ticks. No demand threshold needed.

### Step 6: Invalidation wiring

In `game/build.rs` (building placement, road formation) and terrain mutation code, bump `terrain_dirty_tick`. In `FlowFieldRegistry::maintain()`, mark fields stale when `computed_tick < terrain_dirty_tick`.

### Step 7: Debug overlay

Add a flow field overlay to the debug view cycle (`render.rs`). Draw arrows or directional characters showing the active flow field directions. Color-code by cost (green = low, red = high). Toggle with the existing overlay key `o`.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Flow field directions are coarse (8 directions) causing jagged movement | Visible zigzag patterns, especially on diagonals | Acceptable for ant-colony aesthetic (Pillar 2). If problematic, interpolate between flow field direction and direct-to-destination vector for the last 5 tiles. |
| Burst recomputes when terrain changes (building placed invalidates 3+ fields) | Frame spike up to 3ms | Budget cap of 2 recomputes per tick. Remaining fields are deferred. Entities fall back to Tier 1 path cache -- they never stall. |
| Multiple stockpiles create redundant overlapping fields | Wasted memory and compute | Fields are keyed by exact destination tile. Two stockpiles at different tiles get separate fields. Overlap is harmless -- entities only query one field (for their chosen stockpile). |
| Demand tracking HashMap grows unbounded with unique destinations | Memory leak over long games | Clear demand map every tick (it is rebuilt each tick from scratch). Evict fields with zero demand. |
| Flow field points entity into local minimum (terrain pocket) | Entity gets stuck | `covers()` returns false for unreachable tiles, so the entity falls through to A* which handles pockets correctly. If the tile IS reachable but the path is suboptimal (wraps around a long barrier), the flow field is still correct -- it found the true shortest path via Dijkstra. |
| Serialization: flow fields are large and recomputable | Save file bloat | Do not serialize flow fields. Clear the registry before save. Recompute on load (stockpile field immediately, others on demand). |

## Pillar Alignment

- **Pillar 5 (Scale Over Fidelity):** This is the primary motivation. Flow fields convert O(N) per-entity pathfinding into O(1) lookups for the most common destinations. Combined with Tier 1 path caching, the pathfinding budget becomes nearly independent of population count. This is the key unlock for the 500-villager target.
- **Pillar 2 (Emergent Complexity):** Flow fields naturally produce ant-trail movement patterns. Many villagers following the same optimal route to a stockpile creates visible highways that emerge from the terrain, not from player placement. This reinforces the ant-colony fantasy.
- **Pillar 1 (Geography Shapes Everything):** Flow fields encode terrain costs. Villagers visibly prefer roads, avoid mountains, and route around water -- all without per-entity decision-making. The terrain literally shapes the flow of villagers. A new road changes the flow field and all villagers immediately adjust, creating an organic response to infrastructure.
- **Pillar 4 (Observable Simulation):** The debug overlay showing flow field arrows makes pathfinding visible. Players can see WHY villagers take a certain route. The flow field overlay is also a diagnostic tool for terrain design -- it reveals connectivity, bottlenecks, and dead zones.

## Non-Goals

- **Full-map flow fields.** Computing Dijkstra over 65K tiles for every destination is wasteful. Radius-bounded fields (60-80 tiles) cover the useful range. Entities beyond the radius use Tier 1 path caching to reach the field's coverage area.
- **Per-entity flow fields.** Flow fields are shared infrastructure, not per-entity state. An entity exploring a unique destination gets a Tier 1 path cache, not a flow field.
- **Hierarchical pathfinding (HPA*).** Flow fields solve the common-destination case. HPA* solves the any-to-any case on very large maps. These are complementary, not competing. HPA* is a future optimization if 512x512+ maps are needed.
- **Steering / local avoidance.** Flow fields tell entities WHICH direction to walk, not HOW to avoid each other. Collision avoidance is a separate system. Villagers currently overlap and that fits the ant-colony aesthetic.
- **Dynamic obstacle avoidance in the field.** Moving entities (other villagers, predators) are not encoded in the flow field. The field represents static terrain costs only. Dynamic avoidance, if ever needed, is a per-entity steering layer on top.
