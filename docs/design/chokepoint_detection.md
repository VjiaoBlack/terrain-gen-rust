# Design: Chokepoint Detection

**Status:** Proposed
**Pillars:** 1 (Geography Shapes Everything), 3 (Explore/Expand/Exploit/Endure), 4 (Observable Simulation)
**Phase:** Phase 2 (Economy Depth) / Phase 4 (Threats & Mastery)
**Depends on:** Terrain pipeline (heights, river_mask, slope), TerrainAnalysis struct (terrain_driven_settlement.md)
**Unlocks:** Garrison auto-placement at passes, raider pathing through chokepoints (threat_scaling.md), wall auto-placement, threat overlay chokepoint markers

## Problem

The game has no concept of "narrow pass," "river crossing," or "defensible position." Garrisons are placed near the settlement centroid because `find_building_spot` returns the first valid tile in a ring scan. A 3-tile-wide gap between two mountain ranges is strategically identical to open grassland -- the auto-build system cannot see it, the threat system does not route through it, and the player gets no visual signal that terrain has created a natural bottleneck.

This matters for three systems:

1. **Auto-build (build.rs).** The terrain_driven_settlement design gives garrisons a `chokepoint: 3.0` weight, but that weight reads from `TerrainAnalysis.chokepoint: Vec<f64>` which does not exist yet. Without a detection algorithm, the weight is dead code.

2. **Threat scaling (threat_scaling.md).** Raiders should prefer approach corridors through chokepoints. The `scan_approach_corridors()` function described in that doc needs to know where chokepoints are. Without detection, raiders spawn at random angles.

3. **Player readability (Pillar 4).** The Threats overlay should highlight natural defensive positions so the player can anticipate where threats will arrive and where garrisons matter. Without detection, the overlay has nothing to show.

## What Qualifies as a Chokepoint

A chokepoint is a tile where the walkable corridor is locally narrow -- bounded on both sides by terrain that units strongly avoid or cannot cross. Three categories:

### Mountain passes
Walkable tiles flanked by Mountain, Cliff, or high-elevation impassable terrain on both sides. The classic "narrow gap between two ridges." Width 1-8 tiles.

### River crossings
The small number of tiles where a river can actually be crossed without swimming through deep water. If the river_mask marks a continuous band of water, any walkable tile adjacent to river on both its left and right is a crossing candidate. In practice this means shallow fords, bridges (once buildable), or the narrow gap between two river bends.

### Coastal narrows
A strip of walkable land between ocean/lake and mountains. Common on coastal maps where the settlement has a single land approach.

### What is NOT a chokepoint
- Open terrain that happens to have one mountain tile nearby (no pinch on both sides).
- A single tree in grassland (Forest is walkable, just slow).
- Tiles deep inside an impassable region (no walkable corridor at all).

## Detection Algorithm

### Core: Walkable-Width Measurement via Perpendicular Ray Casting

For each walkable tile, measure how wide the walkable corridor is at that point. A tile in a 3-wide mountain pass gets width=3. A tile in open grassland gets width=80+. The chokepoint score is the inverse of this width.

#### Step 1: Define barrier terrain

A tile is a **barrier** for chokepoint purposes if it is one of:
- `Terrain::Water` (A* cost 8.0, effectively impassable for practical movement)
- `Terrain::Cliff` (not walkable)
- `Terrain::BuildingWall` (not walkable)
- Out of bounds (map edge)

`Terrain::Mountain` is walkable (speed 0.25x, A* cost 4.0) but is a **soft barrier** -- units can cross mountains but strongly avoid them. Mountain tiles count as barriers for chokepoint detection because a mountain pass IS a chokepoint even though the mountains themselves are technically traversable. If we only used hard-impassable tiles, most mountain passes would not register.

```rust
fn is_barrier(terrain: Terrain) -> bool {
    matches!(terrain, Terrain::Water | Terrain::Cliff | Terrain::BuildingWall | Terrain::Mountain)
}
```

#### Step 2: Per-tile minimum corridor width

For each walkable, non-barrier tile `(x, y)`:

1. Cast rays in 4 axis-aligned directions: N, E, S, W.
2. For each ray, walk outward tile-by-tile until hitting a barrier or map edge. Record distance `d`.
3. Compute corridor width along each axis:
   - `width_NS = d_north + d_south + 1` (north-south corridor, measuring east-west width with E/W rays would be wrong -- we want the perpendicular width)

Correction -- the corridor width perpendicular to the corridor's axis is what matters. A north-south corridor (mountain walls to east and west) has its width measured east-west. But we do not know the corridor axis a priori. So we measure width in both perpendicular pairs and take the **minimum**:

```
width_EW = d_east + d_west + 1     // distance to barrier going east + west + the tile itself
width_NS = d_north + d_south + 1   // distance to barrier going north + south + the tile itself
min_width = min(width_EW, width_NS)
```

This captures corridors aligned to either axis. For diagonal corridors (NE-SW, NW-SE), add diagonal ray pairs:

```
width_NESW = d_northeast + d_southwest + 1
width_NWSE = d_northwest + d_southeast + 1
min_width = min(width_EW, width_NS, width_NESW, width_NWSE)
```

Diagonal distances are measured in Chebyshev distance (king-moves), not Euclidean, to keep the grid-based scan simple.

**Chokepoint raw score:**

```
raw_score = 1.0 / (min_width as f64 + 1.0)
```

| min_width | raw_score | Example |
|-----------|-----------|---------|
| 1 | 0.50 | Single-tile gap between mountains |
| 2 | 0.33 | Two-tile pass |
| 3 | 0.25 | Narrow pass |
| 5 | 0.17 | Moderate pass |
| 8 | 0.11 | Wide pass (borderline useful) |
| 20 | 0.05 | Open terrain (not a chokepoint) |
| 40+ | <0.03 | Completely open |

#### Step 3: Require barriers on BOTH sides

A tile near a single mountain wall is not a chokepoint -- it needs pinching from two sides. The min_width measurement inherently captures this: if barriers exist only to the east (d_east=2) but not to the west (d_west=60), then `width_EW = 63`, which is not narrow. The tile only scores high if barriers close in from both directions on at least one axis.

No additional filtering needed -- the geometry handles it.

#### Step 4: Threshold and normalize

Only tiles with `min_width <= 8` are meaningful chokepoints. Normalize the score to [0.0, 1.0]:

```rust
fn chokepoint_score(min_width: u16) -> f64 {
    if min_width > 8 {
        0.0
    } else {
        // Linear: width 1 -> 1.0, width 8 -> 0.125
        1.0 / min_width as f64
    }
}
```

Store as `chokepoint: Vec<f64>` on `TerrainAnalysis`, indexed by `y * width + x`.

### Complexity

For a 256x256 map (65,536 tiles), each tile casts 8 rays (4 axis-aligned + 4 diagonal). Average ray length before hitting barrier or edge: ~20 tiles on a typical map with mountains and water. Total work: `65,536 * 8 * 20 = ~10.5M` tile lookups. Each lookup is an array index + enum match -- roughly 5ns. Total: ~50ms.

This is too expensive for every-tick computation but fine as a one-time pass at game start, recomputed only when terrain changes (building placement, deforestation). The dirty-flag invalidation from terrain_driven_settlement.md applies here.

**Optimization: early termination.** If a ray reaches distance 9 without hitting a barrier, stop -- that axis cannot produce a width <= 8. This cuts average ray length significantly on open maps, bringing typical cost to ~15ms.

**Optimization: lazy computation.** Only compute for tiles within radius 80 of settlement center. Expand the computed region as the settlement grows. On a 256x256 map, radius 80 covers at most ~20,000 tiles, cutting cost to ~5ms.

### River Crossing Detection

River crossings are a special case. The perpendicular-width algorithm does not directly capture them because rivers are narrow (often 1-3 tiles) and the "corridor" across a river is the river itself, which is barrier terrain.

Separate pass for river crossings:

1. For each tile in `river_mask` that is `true` (river water):
   - Check if both perpendicular banks (N+S or E+W) have walkable non-barrier terrain within 2 tiles.
   - If yes, this is a potential crossing point.

2. Among crossing candidates, find the **narrowest** crossings -- tiles where the river is 1-2 tiles wide. These are fords.

3. Mark the walkable tiles immediately adjacent to the ford on both banks as river-crossing chokepoints with a bonus score.

```rust
fn detect_river_crossings(map: &TileMap, river_mask: &[bool], width: usize, height: usize) -> Vec<(usize, f64)> {
    let mut crossings = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if !river_mask[idx] { continue; }

            // Check east-west crossing (river runs N-S, crossing E-W)
            let river_width_ew = measure_river_width(map, x, y, (1, 0), width, height);
            // Check north-south crossing (river runs E-W, crossing N-S)
            let river_width_ns = measure_river_width(map, x, y, (0, 1), width, height);

            let min_river_width = river_width_ew.min(river_width_ns);
            if min_river_width <= 3 {
                // Score: narrower crossing = higher chokepoint value
                let score = 1.0 / (min_river_width as f64 + 1.0);
                // Mark the walkable bank tiles, not the river tile itself
                for (dx, dy) in &[(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                    let bx = x as i32 + dx;
                    let by = y as i32 + dy;
                    if in_bounds(bx, by, width, height) {
                        let bidx = by as usize * width + bx as usize;
                        if !is_barrier(map.get_idx(bidx)) {
                            crossings.push((bidx, score));
                        }
                    }
                }
            }
        }
    }
    crossings
}
```

River crossing scores are merged into the main `chokepoint: Vec<f64>` by taking the max of the perpendicular-width score and the river-crossing score at each tile.

### Coastal Narrow Detection

Coastal narrows (land pinched between ocean and mountains) are automatically captured by the perpendicular-width algorithm. Ocean tiles are Water (barrier), mountain tiles are Mountain (barrier). A 5-tile-wide strip of Grass between ocean and mountains scores `min_width=5, score=0.2`. No special case needed.

## Data Structures

### On `TerrainAnalysis` (defined in terrain_driven_settlement.md)

```rust
pub struct TerrainAnalysis {
    // ... existing fields from terrain_driven_settlement.md ...

    /// Per-tile chokepoint score [0.0, 1.0]. Higher = narrower corridor.
    /// 0.0 for open terrain and barrier tiles. 1.0 for single-tile gaps.
    pub chokepoint: Vec<f64>,

    /// Detected named chokepoints for threat routing and overlay display.
    /// Computed by clustering high-scoring tiles.
    pub chokepoint_locations: Vec<ChokepointLocation>,
}

/// A discrete chokepoint: a cluster of high-scoring tiles forming a pass, ford, or narrow.
pub struct ChokepointLocation {
    /// Center tile of the chokepoint (highest-scoring tile in the cluster).
    pub x: usize,
    pub y: usize,
    /// Corridor width at the narrowest point (tiles).
    pub width: u16,
    /// Primary axis of the corridor: the direction traffic flows THROUGH the chokepoint.
    /// Perpendicular to the barrier walls.
    pub axis: (i32, i32),
    /// What kind of chokepoint.
    pub kind: ChokepointKind,
    /// Distance from settlement center.
    pub distance_to_settlement: f64,
    /// The perpendicular-width chokepoint score at the center tile.
    pub score: f64,
}

pub enum ChokepointKind {
    /// Narrow pass between mountains/cliffs.
    MountainPass,
    /// Ford or narrow crossing over a river.
    RiverCrossing,
    /// Strip of land between water body and impassable terrain.
    CoastalNarrow,
}
```

### Clustering: From Per-Tile Scores to Discrete Chokepoints

The per-tile `chokepoint: Vec<f64>` is continuous -- a 5-tile-wide pass has 5 tiles all scoring ~0.2. For threat routing and garrison placement, we need discrete chokepoint objects. Clustering algorithm:

1. Collect all tiles with `chokepoint[idx] >= 0.1` (min_width <= 8) into a candidate set.
2. Flood-fill connected components among candidates (4-connected adjacency).
3. For each connected component:
   - Find the tile with the highest score (narrowest point) -- this is the center.
   - Record the width at center, the component's bounding box, and classify the kind.
4. Classify kind:
   - If any tile in the component is adjacent to `river_mask == true`: `RiverCrossing`.
   - If flanking barriers are Mountain on both sides: `MountainPass`.
   - If one barrier is Water and the other is Mountain/Cliff: `CoastalNarrow`.
   - Default to `MountainPass` if ambiguous.

Discard components smaller than 2 tiles (noise) or larger than 30 tiles (too broad to be a chokepoint -- that is just a valley).

## Integration Points

### 1. Auto-Build Garrison Placement (build.rs)

The `PlacementWeights` for Garrison already has `chokepoint: 3.0`. With detection in place, a garrison at a 3-tile mountain pass (score 0.33) gets `3.0 * 0.33 = 1.0` bonus, while one in open terrain (score 0.0) gets nothing. Combined with the weak `distance_penalty: -0.2`, the scorer will send garrisons 30+ tiles from centroid to reach a good chokepoint.

Additional integration: when `auto_build_tick` decides to build a garrison, check if any `ChokepointLocation` with `distance_to_settlement < 50` lacks a garrison within 5 tiles. If so, override the normal scoring and place directly at that chokepoint. This ensures the first garrison always goes to the best nearby pass rather than relying on the continuous score alone.

### 2. Wall Auto-Placement (build.rs)

Walls at chokepoints are force multipliers. When the auto-build system places walls, prefer tiles adjacent to chokepoint centers. A wall across a 3-tile pass blocks the entire corridor. Weight profile for Wall already has `chokepoint: 2.5` in the terrain_driven_settlement design.

Walls should be placed **perpendicular to the chokepoint axis** to block the corridor. The `ChokepointLocation.axis` field tells us which direction traffic flows through -- walls go across that axis. Implementation: when scoring wall placement at a chokepoint, give bonus to tiles where the wall's orientation (walls are 1-tile buildings, but adjacent walls form a line) aligns perpendicular to `axis`.

### 3. Raider Approach Corridors (threat_scaling.md)

The threat_scaling design describes `scan_approach_corridors()` which casts 24 rays outward from settlement center and measures corridor width along each. Chokepoint detection replaces this with a more robust approach:

1. For each `ChokepointLocation`, compute the direction from settlement center to the chokepoint.
2. Chokepoints that lie between settlement center and the map edge are natural approach funnels.
3. Raiders spawn beyond the chokepoint (on the far side from settlement) and must path through it.
4. Corridor scoring from threat_scaling.md uses `c.min_width <= 8` -- this maps directly to `ChokepointLocation.width`.

```rust
fn find_raider_approach(
    chokepoints: &[ChokepointLocation],
    settlement: (f64, f64),
    map: &TileMap,
) -> Option<(f64, f64, &ChokepointLocation)> {
    // Filter to chokepoints between settlement and map edge
    let approaches: Vec<_> = chokepoints.iter()
        .filter(|cp| cp.distance_to_settlement > 10.0 && cp.distance_to_settlement < 60.0)
        .collect();

    if approaches.is_empty() { return None; }

    // Score: narrower = more likely raider route (ambush terrain)
    let chosen = weighted_random(&approaches, |cp| cp.score * 2.0 + 1.0);

    // Spawn point: 15-25 tiles beyond the chokepoint, away from settlement
    let dx = chosen.x as f64 - settlement.0;
    let dy = chosen.y as f64 - settlement.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let spawn_x = chosen.x as f64 + (dx / dist) * 20.0;
    let spawn_y = chosen.y as f64 + (dy / dist) * 20.0;
    let (sx, sy) = snap_to_walkable(map, spawn_x, spawn_y);

    Some((sx, sy, chosen))
}
```

### 4. Defense Rating Bonus (threat_scaling.md)

From the threat_scaling design: `chokepoint_bonus = garrison_at_chokepoint * 5.0`. Implementation: for each garrison entity, check if its position is within 5 tiles of any `ChokepointLocation`. If so, add 5.0 to defense rating per covered chokepoint.

```rust
fn chokepoint_defense_bonus(
    garrison_positions: &[(f64, f64)],
    chokepoints: &[ChokepointLocation],
) -> f64 {
    let mut bonus = 0.0;
    for cp in chokepoints {
        let covered = garrison_positions.iter().any(|(gx, gy)| {
            let dx = *gx - cp.x as f64;
            let dy = *gy - cp.y as f64;
            (dx * dx + dy * dy).sqrt() <= 5.0
        });
        if covered {
            bonus += 5.0;
        }
    }
    bonus
}
```

### 5. Threats Overlay (render.rs)

The `OverlayMode::Threats` overlay should highlight chokepoints:

- Chokepoint tiles: bright yellow or orange background tint, proportional to score.
- `ChokepointLocation` centers: a `X` marker or distinct glyph.
- If a garrison covers a chokepoint: green tint (defended). If uncovered: red tint (vulnerable).
- Raider approach arrows: draw arrows from map edge through chokepoints toward settlement.

This gives the player an at-a-glance read of "where are my natural defenses, and which ones have I fortified?"

### 6. Event Log Messages

When a `ChokepointLocation` is first detected (game start or terrain change reveals a new one):
- "Mountain pass detected to the northwest (3 tiles wide)"
- "River ford discovered to the east"

When raiders spawn through a chokepoint:
- "Raiders approaching through the northern mountain pass!"

When a garrison is placed at a chokepoint:
- "Garrison fortifying the eastern river crossing"

## Edge Cases

**No chokepoints on map.** Flat grassland maps with no mountains or rivers produce zero chokepoints. The system gracefully degrades: garrison placement falls back to other scoring weights (high_elevation, cluster). Raider spawning falls back to the lowest-influence direction (existing threat_scaling fallback). The overlay shows nothing, which is itself information -- "you have no natural defenses."

**Too many chokepoints.** Mountain-heavy maps might have 20+ detected chokepoints. The auto-build cannot garrison all of them. Prioritize by distance to settlement (closer = more urgent) and width (narrower = more valuable). The `chokepoint_locations` vec should be sorted by `score / distance_to_settlement` so the auto-builder picks the best ones first.

**Chokepoint blocked by own buildings.** If the player (or auto-build) places non-garrison buildings inside a chokepoint, the corridor width changes. The pass might become even narrower (more chokepoint-like) or fully blocked (no longer a chokepoint -- it is a wall). Recomputation on terrain change handles this. A fully blocked pass drops from the chokepoint list, and the threat system will route raiders elsewhere.

**Chokepoint behind the settlement.** A mountain pass on the far side of the settlement from the map edge is not a useful defensive position -- threats would have to pass through the settlement to reach it. Filter: only chokepoints where the settlement lies between the chokepoint and the interior of the map are relevant for defense. Chokepoints beyond the settlement (closer to map edge) are the approach funnels.

**Moving settlement center.** As the settlement expands, its centroid shifts. A chokepoint that was 40 tiles away might become 20 tiles away, changing its priority. `distance_to_settlement` should be recomputed when the centroid moves significantly (>5 tiles), not every tick.

**River dries up or is bridged.** If water simulation removes river tiles (drought) or a bridge is built, river-crossing chokepoints may disappear or change. The dirty-flag recomputation handles this. A bridged crossing is no longer a chokepoint -- it is just a road.

## Test Criteria

### Unit tests

1. **Mountain pass detection.** Create a 30x30 map: fill with Grass, place two Mountain walls (y=10..20 at x=12 and x=16) leaving a 3-tile gap. Compute chokepoint scores. Assert tiles at x=13,14,15 y=15 have score >= 0.25. Assert tiles at x=5, y=15 (open area) have score 0.0.

2. **River crossing detection.** Create a 30x30 map with a horizontal river (Water at y=14,15). Place Grass everywhere else. Set river_mask for those tiles. Run river crossing detection. Assert bank tiles at y=13 and y=16 near the crossing score > 0.

3. **Coastal narrow.** Create a map with Water on the left half (x<10), Mountain on the right half (x>15), Grass in the 5-tile strip between. Assert the strip tiles have chokepoint score ~0.2.

4. **Open terrain scores zero.** Create an all-Grass 30x30 map. Assert all chokepoint scores are 0.0 (min_width > 8 everywhere).

5. **Barrier terrain scores zero.** Assert Mountain, Water, Cliff tiles themselves have chokepoint score 0.0 (barriers are not chokepoints).

6. **Clustering produces one ChokepointLocation per pass.** Mountain pass test from #1: assert `chokepoint_locations` has exactly 1 entry with `kind == MountainPass` and `width == 3`.

7. **Score normalization.** Width-1 gap scores 1.0. Width-4 gap scores 0.25. Width-9 gap scores 0.0 (above threshold).

### Integration tests

8. **Garrison placed at chokepoint.** Generate a seed with a known mountain pass. Run auto-build for 2000 ticks until a garrison is built. Assert garrison position is within 5 tiles of a detected `ChokepointLocation`.

9. **Defense rating bonus.** Place a garrison at a chokepoint and one in open field. Assert the chokepoint garrison contributes higher defense_rating.

10. **Raiders path through chokepoint.** Spawn raiders on the far side of a mountain pass. Assert their A* path passes within 3 tiles of the chokepoint center (they have no other route).

11. **Existing tests pass.** All 237+ existing tests must not regress.

### Visual validation (manual)

12. Run 3 seeds with different terrain. Open the Threats overlay. Verify chokepoints are highlighted at visually obvious narrow passes and river crossings. Screenshot for comparison.

## Dependencies

| Dependency | Status | Blocking? |
|-----------|--------|-----------|
| `TerrainAnalysis` struct on `Game` | Defined in terrain_driven_settlement.md, not implemented | Yes |
| `river_mask: Vec<bool>` on `Game` | Exists | No |
| `heights: Vec<f64>` on `Game` | Exists | No |
| `TileMap` with `Terrain` enum | Exists | No |
| Dirty-flag recomputation system | Defined in terrain_driven_settlement.md, not implemented | No (can recompute unconditionally at start) |
| `PlacementWeights` per `BuildingType` | Defined in terrain_driven_settlement.md, not implemented | Yes (for auto-build integration) |
| Threat overlay (`OverlayMode::Threats`) | Exists in render.rs | No (overlay enhancement is additive) |

## Estimated Scope

| Task | Effort | Notes |
|------|--------|-------|
| `is_barrier()` + perpendicular ray-cast per tile | 2 hours | Core algorithm, straightforward grid iteration |
| Diagonal ray support + early termination | 1 hour | Optimization pass on core algorithm |
| River crossing detection pass | 1.5 hours | Separate logic, merge into chokepoint vec |
| `ChokepointLocation` clustering (flood-fill + classify) | 2 hours | Flood fill on thresholded scores |
| Wire into `TerrainAnalysis` + dirty-flag recomputation | 1 hour | Depends on TerrainAnalysis existing |
| Auto-build garrison override (place at best chokepoint) | 1 hour | Small addition to `auto_build_tick` |
| Defense rating `chokepoint_bonus` integration | 0.5 hours | Extend `compute_defense_rating()` |
| Raider approach corridor using `chokepoint_locations` | 1.5 hours | Replace ray-cast corridor scan in events.rs |
| Threats overlay chokepoint visualization | 1.5 hours | Tint tiles, draw markers |
| Unit tests (items 1-7) | 2 hours | Small test maps with known geometry |
| Integration tests (items 8-10) | 2 hours | Multi-tick simulation assertions |
| **Total** | **~16 hours** | Spread across ~3 sessions |

### Implementation order

1. `is_barrier()` + perpendicular ray-cast + diagonal rays -> `chokepoint: Vec<f64>` (core algorithm, testable in isolation).
2. River crossing detection -> merge into chokepoint scores.
3. Clustering -> `chokepoint_locations: Vec<ChokepointLocation>`.
4. Unit tests for all three passes.
5. Wire into `TerrainAnalysis` (once terrain_driven_settlement.md is implemented, or as part of it).
6. Auto-build garrison override + defense rating bonus.
7. Raider approach corridor integration.
8. Threats overlay visualization.
9. Integration tests.

## Open Questions

1. **Should Mountain be a barrier?** Mountain is walkable (0.25x speed). Treating it as a barrier means a "pass" between two mountain peaks counts as a chokepoint, which is correct -- units CAN cross the peaks but strongly prefer the pass. If we exclude Mountain from barriers, most mountain passes vanish from detection. Current design: Mountain IS a barrier for chokepoint detection. Revisit if this produces false positives.

2. **Diagonal corridor accuracy.** Chebyshev-distance diagonal rays approximate corridor width but are not geometrically precise for diagonal passes. A 45-degree corridor measured in Chebyshev appears wider than its true Euclidean width by a factor of ~1.41. This means diagonal passes score slightly lower than axis-aligned passes of the same true width. Acceptable for gameplay purposes? Or should diagonal scores be multiplied by sqrt(2) to compensate?

3. **Dynamic chokepoints from buildings.** If the player builds walls to narrow a gap further, should the newly narrowed corridor register as a stronger chokepoint? Current design says yes (recomputation on terrain change). But this means the "natural" chokepoints and "constructed" chokepoints are indistinguishable. Should `ChokepointKind` include a `Constructed` variant?

4. **Chokepoint value decay with distance.** A chokepoint 5 tiles from settlement center is immediately useful. One 80 tiles away is irrelevant until the settlement expands. Should `chokepoint_locations` be filtered by distance, or should the auto-build scorer handle distance naturally via `distance_penalty`? Current design: the scorer handles it, but the garrison override (Section "Auto-Build Garrison Placement") filters to `distance_to_settlement < 50`.

5. **Multiple chokepoints on same approach axis.** If raiders approach from the north and there are two mountain passes in sequence (an outer pass and an inner pass), should the auto-builder garrison both? Or just the outer one? Intuitively, defense in depth is good -- garrison the outer pass, and if it falls, the inner pass is the fallback. But this might spread garrisons too thin for small settlements.

## References

- `src/tilemap.rs` -- `Terrain` enum, `is_walkable()`, `TileMap`
- `src/game/build.rs` -- `find_building_spot()`, `auto_build_tick()`, `compute_defense_rating()`
- `src/game/events.rs` -- wolf surge, bandit raid spawn logic
- `src/game/render.rs` -- overlay rendering, `OverlayMode::Threats`
- `docs/design/terrain_driven_settlement.md` -- `TerrainAnalysis`, `PlacementWeights`, chokepoint field
- `docs/design/threat_scaling.md` -- raider corridors, geographic spawns, defense rating formula
- `docs/game_design.md` -- Pillar 1 (chokepoint detection listed as "Rich" tier feature)
