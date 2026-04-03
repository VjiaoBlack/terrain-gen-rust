# Design: Spatial Hash Grid

**Status:** Proposed
**Pillars:** 5 (Scale Over Fidelity), 2 (Emergent Complexity), 4 (Observable Simulation), 1 (Geography Shapes Everything)
**Phase:** Foundation / Phase 3 prep
**Depends on:** Nothing (foundational infrastructure)
**Unlocks:** Tick budgeting, knowledge sharing, efficient rendering, geographic queries

## Problem

Every AI tick, `system_ai` in `src/ecs/systems.rs` collects **all** entities of each type into flat Vecs:

```
food_positions:         Vec<(f64, f64)>          — every FoodSource in the world
prey_positions:         Vec<(Entity, f64, f64)>  — every Prey
villager_positions:     Vec<(Entity, f64, f64)>  — every Villager
predator_positions:     Vec<(f64, f64)>          — every Predator
stockpile_positions:    Vec<(f64, f64)>          — every Stockpile
build_site_positions:   Vec<(Entity, f64, f64)>  — every BuildSite
stone_deposit_positions:Vec<(f64, f64)>          — every StoneDeposit
hut_positions:          Vec<(f64, f64)>          — every Hut
```

Then each entity scans these lists linearly to find the nearest target. `ai_villager` alone does 6+ linear scans per tick (nearest food, nearest stockpile, nearest stone deposit, nearest build site, predator-within-range check, forest terrain scan). `ai_predator` scans prey + villagers. `ai_prey` scans food + predators.

`find_nearest_terrain` in `src/ecs/ai.rs` iterates a square of `(2r+1)^2` tiles for each call. With `sight_range = 22`, that is **2025 tiles per call**.

### Current complexity per tick

| Operation | Current cost | At 30 pop | At 500 pop |
|-----------|-------------|-----------|------------|
| Collect all positions | O(all_entities) x8 queries | ~200 entities | ~2000 entities |
| Each villager finds nearest X | O(villagers * entities_of_type) | 900 comparisons | 250,000 comparisons |
| Predator-nearby check | O(villagers * predators) | 300 | 50,000 |
| find_nearest_terrain | O(sight_range^2) per call | 2025 tiles x 30 = 60K | 2025 x 500 = 1M |
| **Total distance checks** | | **~62K** | **~1.3M** |

This is already the dominant cost at 30 villagers. At 500 it becomes unplayable.

## Solution: Spatial Hash Grid

A fixed-grid spatial partitioning structure that maps `(cell_x, cell_y) -> Vec<EntityEntry>` for O(nearby) lookups.

### Cell size: 16x16 tiles

The cell size should approximate the radius of the most common query. Villager sight range is 22 tiles, predator sight range is 25, prey sight range is 18.

- **16x16 cells** means a sight-range query (r=22) checks at most a 3x3 neighborhood of cells (the entity's cell plus all adjacent cells, covering 48x48 tiles).
- On a 256x256 map: 16x16 = **256 cells** total. Fits in cache. Each cell holds ~8 entities on average at 500 pop.
- On a future 512x512 map: 32x32 = **1024 cells**. Still tiny.

Why not 32x32? Too coarse. A 32x32 cell covers 1024 tiles. A sight-range query would check 1 cell (the entity's own), but that cell contains far too many entities in dense areas. With 16x16, the 3x3 neighborhood is 9 cells x ~8 entities = ~72 candidates instead of scanning all 500.

Why not 8x8? Too fine. A sight-range query would check 5x5 = 25 cells. More iteration overhead, more cell boundary crossings. The sweet spot is where the cell size is slightly below the typical query radius.

### Data structure

```rust
/// An entry in the spatial grid. Stores entity ID plus position to avoid
/// hecs lookups during spatial queries.
#[derive(Clone, Copy)]
pub struct SpatialEntry {
    pub entity: Entity,
    pub x: f64,
    pub y: f64,
    /// Bitflags for fast category filtering during queries.
    /// Avoids needing to look up components just to check species/type.
    pub categories: u16,
}

/// Category bitflags — an entity can belong to multiple categories.
pub mod category {
    pub const VILLAGER:       u16 = 1 << 0;
    pub const PREDATOR:       u16 = 1 << 1;
    pub const PREY:           u16 = 1 << 2;
    pub const FOOD_SOURCE:    u16 = 1 << 3;
    pub const STOCKPILE:      u16 = 1 << 4;
    pub const BUILD_SITE:     u16 = 1 << 5;
    pub const STONE_DEPOSIT:  u16 = 1 << 6;
    pub const HUT:            u16 = 1 << 7;
    pub const BUILDING:       u16 = 1 << 8;  // any completed building
    pub const WORKSHOP:       u16 = 1 << 9;
}

pub struct SpatialHashGrid {
    cell_size: usize,               // 16
    cols: usize,                    // map_width / cell_size
    rows: usize,                    // map_height / cell_size
    cells: Vec<Vec<SpatialEntry>>,  // flat array, indexed by row * cols + col
}
```

**Why flat `Vec<Vec<SpatialEntry>>` instead of `HashMap`?** The grid is dense and small (256 cells on a 256x256 map). A flat array with direct indexing is faster than hashing. The inner `Vec` grows/shrinks dynamically but starts with a reasonable pre-allocation (16 per cell).

**Why store `x, y` in `SpatialEntry` instead of just `Entity`?** Distance calculations are the hot path. Storing position inline avoids an hecs lookup per candidate. This is the same pattern `system_ai` already uses (it snapshots positions into Vecs), just organized spatially.

**Why category bitflags?** The most common query pattern is "find nearest X within range." Without categories, we'd iterate all entities in nearby cells and filter by component. With bitflags, we do a single `entry.categories & FOOD_SOURCE != 0` check -- no hecs lookup, no branching on species. An entity spawned with both `FoodSource` and `Building` components gets `FOOD_SOURCE | BUILDING`.

### Construction and maintenance

The grid is rebuilt from scratch at the start of each `system_ai` call, replacing the current 8 separate `world.query` collection passes.

```rust
impl SpatialHashGrid {
    pub fn new(map_width: usize, map_height: usize, cell_size: usize) -> Self {
        let cols = (map_width + cell_size - 1) / cell_size;
        let rows = (map_height + cell_size - 1) / cell_size;
        SpatialHashGrid {
            cell_size,
            cols,
            rows,
            cells: (0..cols * rows)
                .map(|_| Vec::with_capacity(16))
                .collect(),
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();  // retains allocated capacity
        }
    }

    pub fn insert(&mut self, entry: SpatialEntry) {
        let cx = (entry.x as usize) / self.cell_size;
        let cy = (entry.y as usize) / self.cell_size;
        if cx < self.cols && cy < self.rows {
            self.cells[cy * self.cols + cx].push(entry);
        }
    }
}
```

**Rebuild vs. incremental update:** Rebuilding each tick is simpler and surprisingly fast. 500 entities = 500 inserts = ~2us. Incremental update (track moves, handle cell transitions) adds complexity for negligible gain. Entities move every tick anyway, so most would need updating regardless. Rebuild also guarantees no stale data.

**Lifecycle:** One `SpatialHashGrid` instance lives on `Game`, allocated once in `Game::new()`. Each tick: `grid.clear()` then `grid.insert()` for every entity with a `Position`. This replaces the 8 `world.query` passes at the top of `system_ai`.

### Population pass (replaces current Phase 1 snapshot)

A single pass over the world builds the grid:

```rust
fn populate_grid(world: &World, grid: &mut SpatialHashGrid) {
    // Single query: everything with a Position
    for (entity, pos) in world.query::<(Entity, &Position)>().iter() {
        let mut cats: u16 = 0;

        if let Ok(creature) = world.get::<&Creature>(entity) {
            match creature.species {
                Species::Villager => cats |= category::VILLAGER,
                Species::Predator => cats |= category::PREDATOR,
                Species::Prey => cats |= category::PREY,
            }
        }
        if world.get::<&FoodSource>(entity).is_ok() { cats |= category::FOOD_SOURCE; }
        if world.get::<&Stockpile>(entity).is_ok()   { cats |= category::STOCKPILE; }
        if world.get::<&BuildSite>(entity).is_ok()    { cats |= category::BUILD_SITE; }
        if world.get::<&StoneDeposit>(entity).is_ok() { cats |= category::STONE_DEPOSIT; }
        if world.get::<&HutBuilding>(entity).is_ok()  { cats |= category::HUT; }

        grid.insert(SpatialEntry {
            entity,
            x: pos.x,
            y: pos.y,
            categories: cats,
        });
    }
}
```

This replaces the 8 separate `world.query::<(&Position, &FoodSource)>`, `world.query::<(&Position, &Stockpile)>`, etc. One pass instead of eight.

### Query API

```rust
impl SpatialHashGrid {
    /// Iterate all entries within `radius` of `(cx, cy)` matching `category_mask`.
    /// Returns entries in arbitrary order. Caller finds min/nearest as needed.
    pub fn query_radius(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        category_mask: u16,
    ) -> impl Iterator<Item = &SpatialEntry> {
        let r = radius;
        let min_col = ((cx - r).max(0.0) as usize) / self.cell_size;
        let max_col = ((cx + r) as usize / self.cell_size).min(self.cols - 1);
        let min_row = ((cy - r).max(0.0) as usize) / self.cell_size;
        let max_row = ((cy + r) as usize / self.cell_size).min(self.rows - 1);
        let r_sq = radius * radius;

        (min_row..=max_row).flat_map(move |row| {
            (min_col..=max_col).flat_map(move |col| {
                self.cells[row * self.cols + col].iter()
            })
        })
        .filter(move |e| e.categories & category_mask != 0)
        .filter(move |e| {
            let dx = e.x - cx;
            let dy = e.y - cy;
            dx * dx + dy * dy <= r_sq
        })
    }

    /// Find the single nearest entry matching `category_mask` within `radius`.
    /// The most common query pattern in the codebase.
    pub fn nearest(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        category_mask: u16,
    ) -> Option<(SpatialEntry, f64)> {
        let mut best: Option<(SpatialEntry, f64)> = None;
        for entry in self.query_radius(cx, cy, radius, category_mask) {
            let dx = entry.x - cx;
            let dy = entry.y - cy;
            let d_sq = dx * dx + dy * dy;
            if best.is_none() || d_sq < best.unwrap().1 {
                best = Some((*entry, d_sq));
            }
        }
        best.map(|(e, d_sq)| (e, d_sq.sqrt()))
    }

    /// Check if ANY entry matching `category_mask` exists within `radius`.
    /// Used for predator_nearby checks. Short-circuits on first match.
    pub fn any_within(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        category_mask: u16,
    ) -> bool {
        self.query_radius(cx, cy, radius, category_mask).next().is_some()
    }

    /// Count entries matching `category_mask` within `radius`.
    pub fn count_within(
        &self,
        cx: f64,
        cy: f64,
        radius: f64,
        category_mask: u16,
    ) -> usize {
        self.query_radius(cx, cy, radius, category_mask).count()
    }

    /// Get all entries in a specific cell (for rendering: "what's on this tile?").
    pub fn entries_in_cell(&self, cell_x: usize, cell_y: usize) -> &[SpatialEntry] {
        if cell_x < self.cols && cell_y < self.rows {
            &self.cells[cell_y * self.cols + cell_x]
        } else {
            &[]
        }
    }
}
```

### How existing code migrates

#### system_ai (src/ecs/systems.rs, lines 75-400)

**Before:** 8 query passes to build flat Vecs, then linear scans inside each entity's AI.

**After:** 1 pass to populate the grid. The flat Vecs are deleted. Each AI function receives `&SpatialHashGrid` instead of 6+ slice parameters.

Concrete changes to `ai_villager` signature:

```rust
// BEFORE (17 parameters):
fn ai_villager(
    pos, creature, state, speed, predator_nearby,
    food: &[(f64, f64)],
    stockpile: &[(f64, f64)],
    build_sites: &[(Entity, f64, f64, bool)],
    stone_deposits: &[(f64, f64)],
    has_stockpile_food, stockpile_food, stockpile_wood, stockpile_stone,
    map, skill_mults, rng, hut_positions: &[(f64, f64)], is_night, frontier,
) -> ...

// AFTER (11 parameters):
fn ai_villager(
    pos, creature, state, speed,
    grid: &SpatialHashGrid,
    has_stockpile_food, stockpile_food, stockpile_wood, stockpile_stone,
    map, skill_mults, rng, is_night, frontier,
) -> ...
```

The `predator_nearby` bool becomes a local query:
```rust
let predator_nearby = grid.any_within(pos.x, pos.y, threat_range, category::PREDATOR);
```

The "find nearest food source within sight range" pattern:
```rust
// BEFORE (ai.rs line 1064):
let nearest_food = food
    .iter()
    .map(|&(fx, fy)| (fx, fy, dist(pos.x, pos.y, fx, fy)))
    .filter(|(_, _, d)| *d < creature.sight_range)
    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

// AFTER:
let nearest_food = grid.nearest(pos.x, pos.y, creature.sight_range, category::FOOD_SOURCE);
```

The "find nearest stone deposit" pattern:
```rust
// BEFORE (ai.rs line 1308):
let nearest_deposit = stone_deposits
    .iter()
    .map(|&(dx, dy)| (dx, dy, dist(pos.x, pos.y, dx, dy)))
    .filter(|(_, _, d)| *d < creature.sight_range)
    .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

// AFTER:
let nearest_deposit = grid.nearest(pos.x, pos.y, creature.sight_range, category::STONE_DEPOSIT);
```

#### find_nearest_terrain (src/ecs/ai.rs, line 171)

This function scans `(2r+1)^2` tiles on the TileMap and does **not** use entities. It stays as-is initially. It queries terrain tiles (Forest, Mountain), not entities. A separate terrain-layer spatial index is a future optimization but out of scope here.

However, entity-based resource lookups (StoneDeposit entities near mountains) migrate to the grid immediately.

#### Predator/prey AI (ai.rs)

Same pattern. `ai_predator` currently receives `prey: &[(Entity, f64, f64, bool)]` and `villagers: &[(Entity, f64, f64, bool)]`. Replace with grid queries:

```rust
// Find nearest huntable target within sight range
let nearest_prey = grid.nearest(pos.x, pos.y, creature.sight_range, category::PREY);
// When desperate, also consider villagers
let nearest_target = if hunger > wolf_aggression {
    grid.nearest(pos.x, pos.y, creature.sight_range, category::PREY | category::VILLAGER)
} else {
    nearest_prey
};
```

#### Build site matching (systems.rs, line 387)

Currently iterates all build sites to find one near the work position:
```rust
for (pos, site) in world.query_mut::<(&Position, &mut BuildSite)>() {
    if (pos.x - bx).abs() < 1.5 && (pos.y - by).abs() < 1.5 { ... }
}
```

After: `grid.nearest(bx, by, 1.5, category::BUILD_SITE)` returns the entity, then do a single `world.get::<&mut BuildSite>(entity)`.

#### Rendering (game/render.rs)

Per-tile rendering (Pillar 5 Section C) can use `grid.entries_in_cell(cell_x, cell_y)` to get all entities in a screen region without scanning every entity in the world. For a typical 80x24 terminal viewport (~1920 tiles, ~8 cells), this checks only entities in those cells instead of all 500+.

#### Future: knowledge sharing (Pillar 2)

Encounter-based knowledge sharing ("two villagers meet, exchange info") needs to find villagers near each other. Without the grid: O(villagers^2). With the grid: iterate each cell, check pairs within that cell. Cost is O(sum of cell_population^2), which for evenly distributed 500 villagers across 256 cells is about 500 * (2^2) = 2000 pair-checks instead of 250,000.

## Performance estimates

### Grid rebuild cost per tick

| Population | Inserts | Estimated time |
|------------|---------|----------------|
| 30 | 30 | <1us |
| 100 | 100 | ~2us |
| 500 | 500 | ~5us |
| 1000 | 1000 | ~10us |

Grid rebuild is negligible. `clear()` on 256 cells (just resetting lengths, no dealloc) is ~0.5us.

### Query cost: nearest entity within sight range

With 16x16 cells and radius=22, each query checks a 3x3 = 9 cells.

| Population | Entities per cell (avg) | Candidates checked per query | Current linear scan |
|------------|------------------------|------------------------------|---------------------|
| 30 | ~0.1 | ~1 | 30 |
| 100 | ~0.4 | ~4 | 100 |
| 500 | ~2.0 | ~18 | 500 |
| 1000 | ~3.9 | ~35 | 1000 |

**Speedup at 500 pop:** Each query checks ~18 candidates instead of 500. A villager doing 6 nearest-queries per tick: 108 distance checks instead of 3000. That is a **~28x reduction** in distance calculations per villager.

### Total AI distance calculations per tick

| Population | Current | With grid | Speedup |
|------------|---------|-----------|---------|
| 30 | ~5,400 | ~180 | 30x |
| 100 | ~60,000 | ~2,400 | 25x |
| 500 | ~1,500,000 | ~54,000 | 28x |
| 1000 | ~6,000,000 | ~210,000 | 29x |

These numbers assume 6 nearest-queries per villager per tick (food, stockpile, stone, build site, predator check, hut). Actual savings are higher because `any_within` (predator check) short-circuits.

### Worst case: dense clusters

If 200 villagers cluster in one cell (e.g., around the stockpile), that cell has 200 entries. A query centered on that cell checks 200 candidates -- same as linear scan of 200. This is the pathological case but it is also **local**: only queries near the cluster pay the cost. Villagers in sparse areas still get the full speedup. In practice, settlements spread across 4-8 cells even when clustered.

If clustering becomes a measured problem, the mitigation is to halve cell size to 8x8 for a 4x improvement in cluster density at the cost of checking 5x5=25 cells per query.

## Implementation plan

### Step 1: Data structure and tests

Add `src/ecs/spatial.rs` with `SpatialHashGrid`, `SpatialEntry`, and category constants.

Unit tests:
- Empty grid returns no results
- Insert one entity, find it by category and radius
- Entity outside radius is not returned
- Entity with wrong category is not returned
- `nearest` returns closest of multiple candidates
- `any_within` short-circuits correctly
- `count_within` is accurate
- Entities on cell boundaries are found from adjacent queries
- Grid handles entities at map edges (x=0, y=255)
- Rebuild (clear + reinsert) produces correct results

### Step 2: Populate grid in system_ai

Add a `SpatialHashGrid` field to `Game`. In `system_ai`, replace the 8 query-to-Vec passes with a single `populate_grid` pass. Keep the existing flat Vecs alive temporarily -- both the grid and the Vecs coexist during migration so tests keep passing.

### Step 3: Migrate ai_villager queries

Replace each linear scan in `ai_villager` with the equivalent `grid.nearest()` or `grid.any_within()` call. Remove the corresponding Vec parameter. Run the full test suite after each parameter removal to catch regressions.

Migration order (lowest risk first):
1. `predator_nearby` check -> `grid.any_within(..., PREDATOR)`
2. `nearest stockpile` -> `grid.nearest(..., STOCKPILE)`
3. `nearest food source` -> `grid.nearest(..., FOOD_SOURCE)`
4. `nearest stone deposit` -> `grid.nearest(..., STONE_DEPOSIT)`
5. `nearest build site` -> `grid.nearest(..., BUILD_SITE)`
6. `nearest hut` -> `grid.nearest(..., HUT)`

### Step 4: Migrate ai_predator and ai_prey

Same pattern. Replace slice parameters with grid queries.

### Step 5: Remove dead code

Delete the flat Vec collection at the top of `system_ai`. Delete the slice parameters from AI function signatures. The grid is now the single source of spatial queries.

### Step 6: Add rendering support

Expose `entries_in_cell` for the renderer. When drawing the viewport, query only the cells that overlap the visible area.

## Design decisions and trade-offs

**Rebuild every tick vs. persistent grid with move tracking.**
Rebuild is simpler, correct by construction, and fast enough (10us at 1000 pop). Move tracking adds complexity (entity despawns, cell transitions, stale entries) for marginal gain. Revisit only if profiling shows rebuild as a bottleneck, which is unlikely.

**Category bitflags vs. separate grids per entity type.**
Separate grids (one for villagers, one for food, etc.) would avoid the bitflag check but require 8+ grid instances and 8+ insert passes. Bitflags keep one grid, one pass, one cache-hot data structure. The bitflag AND is a single cycle -- cheaper than a cache miss from jumping between grids.

**`Vec<Vec<SpatialEntry>>` vs. single flat `Vec<SpatialEntry>` with offset table.**
A single flat Vec with a prefix-sum offset table would be more cache-friendly for iteration but requires a two-pass build (count per cell, then place). The `Vec<Vec<>>` approach is simpler, still fast at our scale, and allows incremental insert if we ever need it. If profiling shows cache pressure, the flat layout is a straightforward follow-up.

**Grid does NOT replace `find_nearest_terrain`.**
`find_nearest_terrain` queries the TileMap (terrain tiles like Forest, Mountain), not entities. A terrain spatial index is a separate concern. The grid handles entities only. Terrain queries could benefit from a precomputed "nearest forest" distance field, but that is a different optimization.

**Grid stores copies of positions, not references.**
Positions are `(f64, f64)` = 16 bytes. Copying is cheaper than indirection. The grid is a snapshot -- it is read-only after population. This matches the existing pattern where `system_ai` already snapshots positions into Vecs.

## File placement

```
src/ecs/
  spatial.rs        # SpatialHashGrid, SpatialEntry, category constants, tests
  mod.rs            # add `pub mod spatial;` and re-export
```

The grid struct lives in `ecs` because it is fundamentally about entity queries. It is used by `systems.rs` (AI), `game/render.rs` (drawing), and eventually `game/build.rs` (building placement queries).

## Future extensions

These are NOT part of this design but are enabled by it:

- **Tick budgeting:** Use `grid.count_within(camera_x, camera_y, viewport_radius, VILLAGER)` to identify offscreen villagers. Run their AI at reduced frequency.
- **Knowledge sharing:** Iterate each cell's villagers. If two are in the same cell, they can exchange knowledge. O(cell_pop^2) per cell instead of O(total_pop^2).
- **Group/flock detection:** Cells with >N villagers in the same BehaviorState are candidates for group abstraction.
- **Threat heat map:** For each predator, increment a threat counter on nearby cells. Villagers check their cell's threat level instead of scanning for predators.
- **Efficient rendering:** Only query cells overlapping the viewport. At 80x24 terminal, that is typically 2x2 = 4 cells instead of all 256.
- **Building placement:** `game/build.rs` currently scans entities to check placement validity. Grid queries replace those scans.
