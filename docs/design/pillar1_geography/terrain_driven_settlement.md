# Settlement Shape Driven by Terrain

## What

Replace the radial-blob building placement algorithm (`find_building_spot`) with a terrain-aware scoring system. Each candidate tile receives a per-building-type score derived from elevation, slope, water proximity, soil type, river adjacency, chokepoint geometry, and existing infrastructure. Auto-build picks the highest-scoring reachable tile instead of the first valid tile found in an outward ring scan.

## Why

Design Pillar 1 ("Geography Shapes Everything") demands that two seeds produce visibly different settlement footprints. Today every settlement is a radial cluster around the stockpile because `find_building_spot` walks outward in concentric rings and returns the *first* valid tile. Terrain type is only a pass/fail gate (Grass/Sand/Forest = ok, everything else = reject). A river to the east, a mountain range to the north, a fertile floodplain to the south -- none of it changes where the farm goes. The settlement is round on every seed.

The game design doc explicitly lists "Settlement shape driven by resource/terrain layout, not just radial blob" as a Core feature for Pillar 1. It also calls out specific placement intelligence: "farms near water, stockpiles at crossroads, garrisons at chokepoints." This design makes all of that concrete.

## Current State

### `find_building_spot` (build.rs:1265)

Searches outward from the villager centroid `(cx, cy)` in two passes:

1. **Coarse grid** (r=2..8, stepping by building width/height): Scans ring shells, returns the first tile where `can_place_building_impl` passes AND the tile is reachable via BFS from the centroid.
2. **Fine grid fallback** (r=4..64, stepping by 1): Same logic, single-tile steps.

No scoring. No terrain preference. First-valid-wins.

### `can_place_building_impl` (build.rs:17)

Pass/fail check: every tile in the building footprint must be Grass, Sand, or Forest. Checks for overlap with existing build sites. Optionally checks influence radius. No awareness of elevation, moisture, soil, rivers, or neighboring terrain features.

### `auto_build_tick` (build.rs:629)

Priority-ordered building queue (Workshop > Garrison > Farm > Hut > ...). Each priority calls `find_building_spot(cx, cy, building_type)`. The centroid is the mean position of all villagers. Every building type uses the same placement logic.

### Data available on `Game` at runtime

| Field | Type | Source | Notes |
|-------|------|--------|-------|
| `map` | `TileMap` | Pipeline | Terrain enum per tile |
| `heights` | `Vec<f64>` | Pipeline | Elevation 0.0-1.0 |
| `soil` | `Vec<SoilType>` | Pipeline | Sand/Loam/Alluvial/Clay/Rocky/Peat |
| `river_mask` | `Vec<bool>` | Pipeline | True for river cells |
| `water` | `WaterMap` | Simulation | Runtime water depth per tile |
| `moisture` | `MoistureMap` | Simulation | Runtime moisture (separate from pipeline moisture) |
| `vegetation` | `VegetationMap` | Simulation | Vegetation density |
| `influence` | `InfluenceMap` | Simulation | Settlement influence radius |
| `traffic` | `TrafficMap` | Simulation | Foot traffic density |

**Not currently stored on Game but available from pipeline:** `slope: Vec<f64>`, pipeline `moisture: Vec<f64>`. These need to be persisted.

---

## Design

### Overview

Replace the first-valid ring scan with a **score-all-candidates, pick-best** approach. For each building type, define a scoring function that maps a candidate tile to a `f64` score. Scan all reachable tiles within a search radius, score each valid one, return the highest.

### Data Structures

#### `PlacementScorer` -- per-tile scoring context

```rust
/// Precomputed terrain analysis cached on Game, recomputed when terrain changes.
pub struct TerrainAnalysis {
    pub width: usize,
    pub height: usize,
    /// Pipeline slope, persisted from PipelineResult.
    pub slope: Vec<f64>,
    /// Pipeline moisture (distinct from runtime MoistureMap).
    pub pipeline_moisture: Vec<f64>,
    /// BFS distance to nearest river tile (u16::MAX if unreachable).
    pub dist_to_river: Vec<u16>,
    /// BFS distance to nearest water tile (river or lake/ocean).
    pub dist_to_water: Vec<u16>,
    /// Chokepoint score: how narrow the walkable corridor is at this tile.
    /// Higher = more constrained (good for garrisons/walls).
    pub chokepoint: Vec<f64>,
    /// Crossroads score: sum of traffic from multiple directions.
    /// Higher = more convergence (good for stockpiles, town halls).
    pub crossroads: Vec<f64>,
}
```

This struct is computed once at game start from pipeline data, and incrementally updated when terrain changes (building placed, tree cut, road formed). The BFS passes are O(w*h) each -- acceptable at 256x256 (65K tiles) and only runs on terrain mutation, not every tick.

#### `PlacementScore` -- per-building-type weight table

```rust
/// Weights for the placement scoring function. Each building type has its own.
pub struct PlacementWeights {
    /// Prefer tiles closer to water (negative = prefer far from water).
    pub water_proximity: f64,
    /// Prefer tiles with specific soil types (multiplied by soil yield_multiplier).
    pub soil_fertility: f64,
    /// Prefer flat terrain (low slope).
    pub flatness: f64,
    /// Prefer low elevation (valley floors).
    pub low_elevation: f64,
    /// Prefer high elevation (defensive vantage).
    pub high_elevation: f64,
    /// Prefer high chokepoint score (narrow passes).
    pub chokepoint: f64,
    /// Prefer high crossroads score (traffic convergence).
    pub crossroads: f64,
    /// Prefer proximity to existing buildings (cluster bonus).
    pub cluster: f64,
    /// Prefer tiles along river axis (not just near water, but following the river direction).
    pub river_parallel: f64,
    /// Penalty for distance from settlement centroid (still prefer closer, but softly).
    pub distance_penalty: f64,
}
```

### Algorithm

#### 1. Candidate generation

Replace the ring-scan with a bounded area scan. Scan all tiles within `search_radius` (default 40, configurable per building type) of the centroid. Filter to tiles that pass `can_place_building_impl` AND `is_reachable`. Collect into a `Vec<(i32, i32)>` of candidates.

The reachable BFS is already computed once per `find_building_spot` call (unchanged from current code).

#### 2. Scoring function

For each candidate `(bx, by)`, compute:

```
score = 0.0

// Water proximity: exponential falloff from river/water
let wd = terrain_analysis.dist_to_water[idx] as f64;
score += weights.water_proximity * (1.0 / (1.0 + wd * 0.15));

// Soil fertility (for farms)
score += weights.soil_fertility * soil[idx].yield_multiplier();

// Flatness: prefer slope < 0.05, penalize steep
score += weights.flatness * (1.0 - (slope[idx] / 0.1).min(1.0));

// Elevation preference
score += weights.low_elevation * (1.0 - heights[idx]);
score += weights.high_elevation * heights[idx];

// Chokepoint (for garrisons/walls)
score += weights.chokepoint * terrain_analysis.chokepoint[idx];

// Crossroads (for stockpiles, town halls)
score += weights.crossroads * terrain_analysis.crossroads[idx];

// Cluster bonus: count buildings within 8 tiles, mild preference
score += weights.cluster * (nearby_building_count as f64 * 0.1).min(0.5);

// Distance penalty: soft falloff from centroid
let dist = ((bx as f64 - cx).powi(2) + (by as f64 - cy).powi(2)).sqrt();
score += weights.distance_penalty * (-dist / 20.0);
```

Return `argmax(candidates, score)`.

#### 3. Per-building-type weight profiles

| Building | water_prox | soil_fert | flatness | low_elev | high_elev | chokepoint | crossroads | cluster | distance_pen |
|----------|-----------|-----------|----------|----------|-----------|------------|------------|---------|-------------|
| **Farm** | 2.0 | 3.0 | 1.5 | 0.5 | 0.0 | 0.0 | 0.0 | 0.3 | -0.5 |
| **Hut** | 0.5 | 0.0 | 1.0 | 0.3 | 0.0 | 0.0 | 0.0 | 1.5 | -1.0 |
| **Stockpile** | 0.3 | 0.0 | 1.0 | 0.2 | 0.0 | 0.0 | 2.5 | 1.0 | -0.3 |
| **Garrison** | 0.0 | 0.0 | 0.5 | 0.0 | 1.5 | 3.0 | 0.0 | 0.0 | -0.2 |
| **Wall** | 0.0 | 0.0 | 0.3 | 0.0 | 1.0 | 2.5 | 0.0 | 0.0 | -0.1 |
| **Workshop** | 0.0 | 0.0 | 1.0 | 0.2 | 0.0 | 0.0 | 1.0 | 2.0 | -1.0 |
| **Smithy** | 0.0 | 0.0 | 1.0 | 0.0 | 0.5 | 0.0 | 1.0 | 2.0 | -1.0 |
| **Granary** | 0.0 | 0.0 | 1.0 | 0.2 | 0.0 | 0.0 | 1.5 | 1.5 | -0.8 |
| **Bakery** | 0.0 | 0.0 | 1.0 | 0.2 | 0.0 | 0.0 | 1.0 | 2.0 | -1.0 |
| **TownHall** | 0.5 | 0.0 | 1.5 | 0.3 | 0.0 | 0.0 | 3.0 | 2.0 | -0.5 |

**Reading the table:** Farms strongly prefer water proximity and fertile soil. Garrisons strongly prefer chokepoints and high ground. Huts cluster near existing buildings (residential districts). Workshops/Smithies cluster (industrial district). Stockpiles and Town Halls prefer crossroads where traffic converges.

The `distance_penalty` is always negative (closer to centroid is better), but the magnitude varies: garrisons have a weak penalty (willing to be placed far out at a chokepoint), while huts have a strong penalty (stay near the core).

#### 4. Chokepoint detection

Computed as part of `TerrainAnalysis`. For each walkable tile, measure the minimum "corridor width" -- the shortest distance to an impassable boundary (water, cliff, mountain) in any direction.

```
For each walkable tile (x, y):
    min_clear = MAX
    For each of 8 compass directions:
        Walk outward until hitting impassable terrain or map edge
        Record distance
    chokepoint_score = 1.0 / (min_clear as f64 + 1.0)
```

A tile in a 3-wide mountain pass scores `1.0 / (2.0) = 0.5`. A tile in open grassland scores `1.0 / 41.0 = 0.02`. Garrisons with `chokepoint = 3.0` weight will strongly prefer the pass.

Optimization: only compute for tiles within plausible settlement range (within 60 tiles of spawn). Can be extended lazily as the settlement expands.

#### 5. Crossroads detection

Derived from `TrafficMap`. A crossroads is a tile where traffic arrives from multiple distinct directions.

```
For each tile with traffic > threshold:
    Sample traffic in 4 quadrants (NE, SE, SW, NW) at distance 3-5
    Count quadrants with traffic > half of center traffic
    crossroads_score = quadrant_count / 4.0
```

A tile on a single path scores 0.25-0.5. A tile where two paths cross scores 0.75-1.0. This naturally identifies trail intersections.

Note: crossroads scoring requires some traffic to have accumulated. For early game (tick < 500), fall back to a geometric heuristic: tiles equidistant from multiple resource clusters score higher.

### Integration Points

#### 1. `Game` struct changes

```rust
// Add to Game struct:
pub terrain_analysis: TerrainAnalysis,
// Add pipeline slope + moisture to persisted fields:
pub slope: Vec<f64>,
pub pipeline_moisture: Vec<f64>,
```

Initialized in `Game::new_with_size` from `PipelineResult` fields. `TerrainAnalysis` computed immediately after pipeline runs.

#### 2. `find_building_spot` replacement

The current signature stays the same:

```rust
pub(super) fn find_building_spot(&self, cx: f64, cy: f64, bt: BuildingType) -> Option<(i32, i32)>
```

Internally, it changes from ring-scan to:
1. Compute reachable tiles (existing BFS, unchanged).
2. Collect all valid candidates within `search_radius`.
3. Score each candidate using `bt.placement_weights()` and `self.terrain_analysis`.
4. Return the highest-scoring candidate, or `None` if no candidates.

#### 3. `auto_build_tick` -- no changes needed

The priority queue logic in `auto_build_tick` is orthogonal to placement. It decides *what* to build; the scoring decides *where*. The only change is that `find_building_spot` now returns better locations.

#### 4. `TerrainAnalysis` cache invalidation

Recompute `dist_to_water` and `chokepoint` when:
- A building is placed (terrain changes to BuildingFloor/BuildingWall).
- A road forms (traffic threshold crossed).
- Water simulation changes water tiles.

Recompute `crossroads` when:
- `TrafficMap` is updated (every ~50 ticks, not every tick).

Use a dirty flag (`terrain_analysis_dirty: bool`) set by `place_build_site`, road formation, and water sim. Check once per `auto_build_tick` invocation (every 50 ticks), recompute if dirty. Cost is ~3 BFS passes at O(w*h) = ~200K ops for 256x256, well under 1ms.

#### 5. Manual build mode

When the player manually places a building in build mode, show the placement score as a color overlay on the cursor tile. Green = high score for this building type, red = low score. This gives the player feedback about *why* the auto-builder would or wouldn't pick this spot, without removing manual control.

#### 6. Save/load

`TerrainAnalysis` is derived data and does NOT need to be serialized. Recompute it on load from the persisted `slope`, `pipeline_moisture`, `river_mask`, `heights`, and current `TrafficMap`.

`slope` and `pipeline_moisture` need to be added to the save format (two `Vec<f64>` fields on `Game`).

---

## Edge Cases

**No valid candidates within search radius.** Fall back to the current ring-scan algorithm (first-valid) at expanded radius. This should be rare but handles extreme terrain (tiny island, enclosed valley).

**All candidates score equally.** Break ties by distance to centroid (prefer closer). If still tied, use the first candidate found (deterministic scan order).

**River cuts settlement in half.** The existing reachability BFS already handles this -- candidates must be reachable from the centroid. Settlement grows along one bank until a bridge is built. Future bridge-building auto-build logic can extend this.

**Mountain-locked spawn.** If the spawn point is in a narrow mountain valley, the search radius may contain very few valid tiles. The `distance_penalty` weight should be weak enough that the scorer is willing to look far. The fallback fine-grid scan (r up to 64) catches this.

**Early game with no traffic data.** Crossroads score is 0 everywhere. Stockpiles and Town Halls fall back to their other weights (flatness, low elevation, cluster). This is fine -- crossroads become relevant in mid-game when traffic patterns exist.

**Terrain analysis recomputation cost.** At 256x256, three BFS passes take ~0.5ms. At 512x512, ~2ms. Cap recomputation frequency at once per 50 ticks (already the auto-build cadence). If maps grow to 512+, consider chunked/lazy recomputation around the settlement centroid only.

**Scoring function produces degenerate placements.** If weights are poorly tuned, farms might all cluster on the same river tile. Mitigate by adding a **spacing penalty**: score decreases if another building of the same type is within 5 tiles. This encourages spatial distribution without hard constraints.

---

## Test Criteria

### Unit tests (in `build.rs`)

1. **Farm prefers water.** Generate a 30x30 map with a river down the middle. Call `find_building_spot` for a Farm. Assert the returned position has `dist_to_water <= 3`.

2. **Garrison prefers chokepoint.** Generate a map with a 4-tile-wide mountain pass. Call `find_building_spot` for a Garrison. Assert the returned position is within the pass.

3. **Hut clusters near existing buildings.** Place 3 huts, then call `find_building_spot` for a 4th. Assert the 4th hut is within 8 tiles of at least one existing hut.

4. **Different seeds produce different farm layouts.** Run auto-build for 2000 ticks on seed 42 and seed 137. Collect farm positions. Assert the bounding-box centroids differ by at least 10 tiles.

5. **Fallback to ring-scan when no scored candidates.** Create a 10x10 map that is almost entirely Mountain with 2 Grass tiles. Assert `find_building_spot` still finds a valid placement.

6. **Spacing penalty prevents stacking.** Place 3 farms near a river. Call `find_building_spot` for a 4th farm. Assert it is at least 4 tiles from the nearest existing farm.

### Integration tests

7. **Settlement shape follows river.** Run seed with known river layout for 5000 ticks with auto-build. Compute the principal axis of all building positions (PCA or bounding box aspect ratio). Assert aspect ratio > 1.5 (elongated along river, not circular).

8. **Settlement shape hugs valley.** Run a valley seed. Assert >80% of buildings are below median elevation.

9. **No regressions.** Run the existing `auto_build` integration tests. All 237+ existing tests must still pass.

### Visual validation (manual, not automated)

10. Run seed 42 and seed 137 side-by-side at tick 5000. Verify visually that settlement footprints are shaped differently by terrain. Screenshot for comparison.

---

## Dependencies

| Dependency | Status | Blocking? |
|-----------|--------|-----------|
| `slope: Vec<f64>` persisted on `Game` | Not stored yet (available from pipeline) | Yes -- add field, pass through from `PipelineResult` |
| `pipeline_moisture: Vec<f64>` persisted on `Game` | Not stored yet (available from pipeline) | Yes -- add field, pass through from `PipelineResult` |
| `TerrainAnalysis` struct + computation | New code | Yes -- core of this design |
| BFS `dist_to_water` / `dist_to_river` | Similar code exists in `terrain_pipeline::compute_moisture` | No -- can reuse pattern, but needs runtime version on `Game` |
| Chokepoint detection | New algorithm | Yes |
| Crossroads detection from `TrafficMap` | New algorithm, depends on existing `TrafficMap` | Yes |
| `PlacementWeights` per `BuildingType` | New data | Yes |
| Save/load for new `Game` fields | Extend existing serialization | Low effort |

## Estimated Scope

| Task | Effort | Notes |
|------|--------|-------|
| Persist `slope` + `pipeline_moisture` on `Game` | 1 hour | Add fields, wire through from pipeline, update save/load |
| `TerrainAnalysis` struct + BFS computation | 3 hours | dist_to_water, dist_to_river, chokepoint, crossroads |
| `PlacementWeights` per building type | 1 hour | Data table, impl on `BuildingType` |
| Scoring function in `find_building_spot` | 3 hours | Replace ring-scan, candidate collection, scoring, fallback |
| Cache invalidation (dirty flag) | 1 hour | Set flag on terrain mutation, check in auto_build_tick |
| Spacing penalty for same-type buildings | 1 hour | Nearby-building scan in scorer |
| Unit tests (items 1-6) | 2 hours | Test maps with known terrain features |
| Integration tests (items 7-8) | 2 hours | Multi-seed runs with shape assertions |
| Weight tuning via playtesting | 2-4 hours | Run seeds, adjust weights, iterate |
| **Total** | **16-18 hours** | Spread across ~3 sessions |

### Implementation order

1. Persist `slope` + `pipeline_moisture` on `Game` (unblocks everything).
2. `TerrainAnalysis` struct with BFS passes (core infrastructure).
3. Scoring function + `PlacementWeights` (the actual behavior change).
4. Replace `find_building_spot` internals (swap in scorer, keep signature).
5. Unit tests for scoring behavior.
6. Integration tests for settlement shape.
7. Tune weights through multi-seed playtesting.
8. Cache invalidation and spacing penalty (polish).
