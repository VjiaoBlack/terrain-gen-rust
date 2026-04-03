# Water Proximity Bonus for Farms (Irrigation)

**Status:** Proposed
**Pillar:** 1 (Geography Shapes Everything)
**Priority:** Rich
**Phase:** 2 (Economy Depth)
**Dependencies:** simulation_chains (moisture wired into farm growth), farming_changes_terrain (fertility system)

---

## What

Farms near rivers and standing water grow faster and recover fertility faster. The bonus flows through the existing simulation -- water raises moisture, moisture accelerates crop growth -- not through a hardcoded proximity multiplier. This creates the classic fertile river valley pattern: settlements cluster along waterways because the land there is genuinely better for farming, and the player can see why.

## Why

Three problems this solves:

1. **River valleys have no gameplay pull.** `SoilType::Alluvial` exists within 4 tiles of rivers (computed via BFS in `compute_soil_type`, `terrain_pipeline.rs:754-782`), and it has a 1.25x yield multiplier. But `system_farms` (`ecs/systems.rs:803`) never reads `Game::soil`. Rivers are decorative. There is no reason to farm near water rather than anywhere else.

2. **The simulation chain is broken at the last link.** `MoistureMap::update()` (`simulation.rs:272`) already generates higher moisture near water tiles. `WaterMap` already models rivers with persistent water levels. But `system_farms` ignores moisture entirely -- it uses a flat seasonal rate times a global skill multiplier. The simulation produces the right data; nothing reads it.

3. **Drought has no spatial texture.** When the simulation_chains design replaces the drought yield multiplier with reduced rainfall, river-adjacent farms should survive longer because river-fed moisture persists. Without moisture wired into farm growth, this spatial variation is lost.

## Current State

| Layer | Status | Code |
|-------|--------|------|
| `WaterMap` | Working. Per-tile water level, flow, evaporation. Rivers maintain persistent water. | `simulation.rs:9-14` |
| `MoistureMap` | Working. Updates from `WaterMap`, propagates, decays. Higher near water. | `simulation.rs:241-300` |
| `MoistureMap.get(x,y)` | Exists. Returns per-tile moisture as f64. | `simulation.rs:256-261` |
| `Game::river_mask` | Stored. `Vec<bool>` from pipeline, marks river tiles. | `game/mod.rs:330` |
| `Game::soil` | Stored. `Vec<SoilType>` from pipeline. Never read at runtime. | `game/mod.rs:329` |
| `SoilType::Alluvial` | Assigned to tiles within 4 BFS steps of `river_mask`. Yield multiplier 1.25. | `terrain_pipeline.rs:795-796` |
| `FarmPlot` | Has `growth`, `harvest_ready`, `worker_present`, `pending_food`. No tile position, no moisture awareness. | `ecs/components.rs:454-461` |
| `system_farms` | Flat `base_rate * skill_mult`. Does not read moisture, soil, or position. | `ecs/systems.rs:803-830` |
| `dist_to_river` BFS | Computed inside `compute_soil_type()` but discarded. Not stored in `PipelineResult` or `Game`. | `terrain_pipeline.rs:754-782` |

The pipeline computes everything needed (river proximity, soil type, moisture). The runtime ignores all of it.

---

## Design

### Core Principle

No new "irrigation bonus" field. The bonus emerges from existing simulation layers:

```
River (persistent water in WaterMap)
  -> MoistureMap: tiles near rivers have higher moisture (already happens)
    -> system_farms: moisture_factor scales growth rate (new wiring)
      -> Farms near rivers grow faster (emergent result)
```

The only new code is the last link: `system_farms` reads `MoistureMap` at each farm's tile position.

### Data Structures

#### FarmPlot additions

`FarmPlot` needs to know where it is on the map so it can sample `MoistureMap`:

```rust
pub struct FarmPlot {
    pub growth: f64,           // existing
    pub harvest_ready: bool,   // existing
    pub worker_present: bool,  // existing
    pub pending_food: u32,     // existing
    pub tile_x: usize,        // NEW: map x coordinate
    pub tile_y: usize,        // NEW: map y coordinate
}
```

These are set once at spawn time from the entity's `Position` component. No per-tick update needed -- farms do not move.

Note: `tile_x`/`tile_y` are also proposed in `simulation_chains.md`. This design shares that requirement. Whichever lands first adds the fields.

#### River distance map (stored at world-gen)

The BFS `dist_to_river` computed inside `compute_soil_type()` is currently discarded after soil assignment. Store it in `PipelineResult` and `Game` for runtime use:

```rust
// terrain_pipeline.rs
pub struct PipelineResult {
    pub map: TileMap,
    pub heights: Vec<f64>,
    pub moisture: Vec<f64>,
    pub temperature: Vec<f64>,
    pub soil: Vec<SoilType>,
    pub river_mask: Vec<bool>,
    pub slope: Vec<f64>,
    pub river_distance: Vec<u32>,  // NEW: BFS distance to nearest river tile
}
```

```rust
// game/mod.rs — Game struct
pub river_distance: Vec<u32>,
```

This costs 256KB on a 256x256 map (one `u32` per tile). The BFS already runs; we just stop throwing away the result.

#### No new simulation map

Critically, this design does **not** add an "irrigation map" or "water bonus map." The moisture map IS the irrigation map. River water raises moisture. Moisture raises growth. That is the entire mechanism.

### Algorithm

#### Growth rate formula (updated system_farms)

```rust
pub fn system_farms(
    world: &mut World,
    season: Season,
    skill_mult: f64,
    moisture: &MoistureMap,      // NEW parameter
) {
    let base_rate = match season {
        Season::Spring => 0.002,
        Season::Summer => 0.003,
        Season::Autumn => 0.001,
        Season::Winter => 0.0,
    };

    for farm in world.query_mut::<&mut FarmPlot>() {
        if farm.harvest_ready {
            if farm.worker_present {
                farm.growth = 0.0;
                farm.harvest_ready = false;
                farm.pending_food += 3;  // yield amount; later scaled by fertility
            }
        } else if farm.worker_present {
            let moisture_val = moisture.get(farm.tile_x, farm.tile_y);
            let moisture_factor = moisture_ramp(moisture_val);
            let growth_rate = base_rate * skill_mult * moisture_factor;

            farm.growth += growth_rate;
            if farm.growth >= 1.0 {
                farm.growth = 1.0;
                farm.harvest_ready = true;
            }
        }
        farm.worker_present = false;
    }
}
```

#### Moisture-to-growth ramp

Not a raw multiplication. A shaped curve that:
- Prevents instant death at zero moisture (minimum floor for early game)
- Gives diminishing returns above "well-watered" (prevents moisture stacking exploits)
- Has a clear sweet spot near rivers

```rust
fn moisture_ramp(moisture: f64) -> f64 {
    // Floor: even dry land grows at 40% rate (prevents hard dependency on water
    // before irrigation exists as a mechanic)
    // Ceiling: moisture above 0.6 gives diminishing returns
    // Sweet spot: 0.3-0.6 moisture = 70%-100% growth rate
    //
    // Ramp: 0.4 + 0.6 * (moisture / 0.6).clamp(0.0, 1.0)
    //
    // moisture=0.0 -> 0.4  (dry grassland)
    // moisture=0.1 -> 0.5
    // moisture=0.3 -> 0.7  (moderate, away from water)
    // moisture=0.6 -> 1.0  (near river, well-watered)
    // moisture=1.0 -> 1.0  (capped, no bonus for flooding)

    let t = (moisture / 0.6).clamp(0.0, 1.0);
    0.4 + 0.6 * t
}
```

The 0.4 floor is deliberate. Farms away from water still work -- they are just 40% slower. This preserves early-game viability (first farm is placed wherever) while creating strong incentive to expand toward rivers. The floor can be lowered in Phase 3 once irrigation buildings exist as a mitigation path.

#### Fertility recovery bonus from water proximity

When the farming_changes_terrain fertility system is implemented, water proximity accelerates fallow recovery. This uses the stored `river_distance` map:

```rust
fn fallow_recovery_rate(soil: SoilType, river_dist: u32) -> f64 {
    let base = match soil {
        SoilType::Alluvial => 0.0004,
        SoilType::Loam     => 0.0003,
        SoilType::Clay     => 0.0002,
        SoilType::Peat     => 0.0003,
        SoilType::Sand     => 0.0001,
        SoilType::Rocky    => 0.00005,
    };
    // Farms within 4 tiles of a river recover 50% faster
    // Farms within 2 tiles recover 75% faster
    // Farms on river-adjacent tiles recover 100% faster (2x)
    let water_bonus = match river_dist {
        0..=1 => 2.0,
        2..=3 => 1.75,
        4..=6 => 1.5,
        _     => 1.0,
    };
    base * water_bonus
}
```

This makes alluvial river soil recover in roughly half a season when fallow -- the best farmland in the game, and it is geographically locked to river valleys.

### Integration Points

#### 1. system_farms call site (game/mod.rs:1341)

Currently:
```rust
ecs::system_farms(&mut self.world, self.day_night.season, farm_mult);
```

Becomes:
```rust
ecs::system_farms(&mut self.world, self.day_night.season, farm_mult, &self.moisture);
```

One new parameter. No structural change.

#### 2. Farm spawn (ecs/spawn.rs or game/build.rs)

Wherever `FarmPlot` is constructed, set `tile_x` and `tile_y` from the entity's `Position`:

```rust
FarmPlot {
    growth: 0.0,
    harvest_ready: false,
    worker_present: false,
    pending_food: 0,
    tile_x: pos.x as usize,
    tile_y: pos.y as usize,
}
```

#### 3. Pipeline result storage (terrain_pipeline.rs + game/mod.rs)

Extract `dist_to_river` from the local scope of `compute_soil_type()` and return it in `PipelineResult`. Store in `Game` during `Game::new()`.

#### 4. MoistureMap (no changes needed)

`MoistureMap::update()` already produces higher moisture near water tiles. The river-proximity gradient is an emergent property of the existing water-to-moisture propagation. No tuning needed for the basic case.

#### 5. Interaction with simulation_chains.md

The simulation_chains design proposes the same `system_farms` signature change (adding `&MoistureMap`). This design is compatible: both want `moisture_factor` in the growth formula. If simulation_chains lands first, this design reduces to "already done for growth; add fertility recovery bonus." If this design lands first, simulation_chains gets the wiring for free.

#### 6. Interaction with farming_changes_terrain.md

That design proposes a "water proximity recovery bonus of +50%" in a single line (line 229). This design is the full specification of that bonus: the `fallow_recovery_rate` function with graduated distance tiers. The two designs are complementary, not conflicting.

---

## Edge Cases

| Case | Behavior | Rationale |
|------|----------|-----------|
| Farm placed on zero-moisture tile (desert) | Grows at 40% rate (moisture_ramp floor) | Prevents hard failure; player learns water matters through speed, not death |
| Farm placed directly on river-adjacent tile | Gets high moisture from `MoistureMap::update()`, grows near 100% rate | Correct: this IS the irrigation bonus |
| River dries up (drought) | `WaterMap` water level drops -> `MoistureMap` moisture decays -> farms slow down naturally | No special case needed; the chain handles it |
| River dries up then returns | Moisture rebuilds from water -> farms speed back up | Simulation handles recovery automatically |
| Farm placed, then river is dammed upstream (future) | Water stops flowing -> moisture drops locally -> farm slows | Future-compatible: works through water simulation |
| Tile has high moisture from rain (not river) | Same growth bonus as river moisture | Correct: moisture is moisture regardless of source |
| Two rivers nearby (confluence) | Higher water levels -> higher moisture -> slightly faster growth | Emergent and reasonable |
| Winter: rivers still have water | Moisture persists but `base_rate = 0.0` so no growth anyway | Correct: winter blocks farming regardless of moisture |
| Farm at map edge (moisture propagation wraps) | `MoistureMap::get()` handles bounds via `wrapping_idx` | Already safe |
| `tile_x`/`tile_y` desync from entity position | Cannot happen: farms do not move, coordinates set once at spawn | No mitigation needed |

---

## Test Criteria

| Test | Setup | Assertion |
|------|-------|-----------|
| `farm_near_river_grows_faster` | Two farms: one at moisture 0.6 (near river), one at moisture 0.1 (dry). Run 100 ticks in summer with worker present. | River farm has higher growth value. |
| `moisture_ramp_floor` | Farm at moisture 0.0, worker present, summer. Run 100 ticks. | Growth > 0.0 (floor prevents zero growth). |
| `moisture_ramp_ceiling` | Farm at moisture 1.0 vs farm at moisture 0.6. Run 100 ticks. | Growth values are equal (cap at 0.6 moisture). |
| `drought_reduces_river_farm_slower` | Set up river-adjacent farm. Run drought (reduce rain_rate, increase evaporation). Compare growth decline to dry-land farm. | River farm retains growth longer because residual water maintains moisture. |
| `system_farms_reads_moisture_map` | Construct `MoistureMap`, set specific tile to 0.0. Place farm on that tile. Run `system_farms`. | Growth equals `base_rate * skill_mult * 0.4` (the floor). |
| `farm_tile_position_set_at_spawn` | Spawn a farm entity at position (10, 20). | `FarmPlot.tile_x == 10`, `FarmPlot.tile_y == 20`. |
| `river_distance_stored_in_game` | Run pipeline, check `Game::river_distance`. | Non-empty vec, river tiles have distance 0, adjacent tiles have distance 1. |
| `fallow_recovery_faster_near_river` | Two fallow farms: one with `river_distance=1`, one with `river_distance=10`. Same soil type. Run recovery ticks. | Near-river farm has higher fertility after same tick count. |
| `alluvial_river_farm_is_best` | Farm on Alluvial soil, river_distance=1, moisture=0.6. Compare total food output over 20 harvest cycles to Sand soil, river_distance=20, moisture=0.1. | Alluvial-river farm produces significantly more food and degrades slower. |
| `moisture_ramp_values` | Unit test `moisture_ramp` with inputs [0.0, 0.1, 0.3, 0.6, 1.0]. | Returns [0.4, 0.5, 0.7, 1.0, 1.0] respectively. |

---

## Dependencies

| Dependency | Status | Required for |
|------------|--------|-------------|
| `MoistureMap` with `.get(x, y)` | Exists | Reading moisture at farm tile |
| `WaterMap` driving `MoistureMap` | Exists | River water -> moisture gradient |
| `Game::river_mask` | Exists | Identifying river tiles (used by BFS) |
| `FarmPlot.tile_x`, `tile_y` | Proposed (simulation_chains.md) | Sampling moisture at farm position |
| `river_distance` in `PipelineResult` | New (extract from `compute_soil_type`) | Graduated recovery bonus |
| Fertility system (farming_changes_terrain.md) | Proposed | Fallow recovery bonus near water |
| `system_farms` signature change | Proposed (simulation_chains.md) | Passing `&MoistureMap` to farm system |

The growth-rate portion (moisture_factor in system_farms) can be implemented independently of the fertility system. The fallow recovery bonus requires farming_changes_terrain to land first.

---

## Estimated Scope

### Tier 1: Moisture wired into farm growth (standalone, no other design doc needed)

| Task | Files | Estimate |
|------|-------|----------|
| Add `tile_x`, `tile_y` to `FarmPlot` | `ecs/components.rs` | 10 min |
| Set tile coords at farm spawn | `ecs/spawn.rs` or `game/build.rs` | 15 min |
| Add `&MoistureMap` param to `system_farms` | `ecs/systems.rs` | 10 min |
| Implement `moisture_ramp()` | `ecs/systems.rs` | 10 min |
| Update call site in `Game::step()` | `game/mod.rs:1341` | 5 min |
| Add `#[serde(default)]` for backward compat | `ecs/components.rs` | 5 min |
| Tests: moisture_ramp values, farm growth with moisture | `ecs/mod.rs` | 30 min |
| **Subtotal** | | **~1.5 hours** |

### Tier 2: River distance stored + fallow recovery bonus (requires farming_changes_terrain)

| Task | Files | Estimate |
|------|-------|----------|
| Extract `dist_to_river` from `compute_soil_type`, return in `PipelineResult` | `terrain_pipeline.rs` | 20 min |
| Store `river_distance` in `Game` | `game/mod.rs` | 10 min |
| Implement `fallow_recovery_rate()` with distance tiers | `ecs/systems.rs` | 15 min |
| Wire into fallow recovery logic | `ecs/systems.rs` | 15 min |
| Tests: recovery rate varies by distance | `ecs/mod.rs` | 20 min |
| **Subtotal** | | **~1.5 hours** |

### Total: ~3 hours

Tier 1 is the high-value change. It makes rivers matter for farming with minimal code. Tier 2 deepens the effect but depends on the fertility system existing first.
