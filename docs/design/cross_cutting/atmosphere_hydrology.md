# Feature: Atmosphere & Hydrology (Wind + Water + Erosion)
Pillar: 1 (Geography), 2 (Emergence)
Priority: Core — the system that makes terrain alive
Phase: Next

## The Loop

```
Wind (terrain-deflected) 
  → carries moisture
    → deposits as rain (windward slopes, random)
      → surface water (pipe model, 8-directional)
        → flows downhill, pools in basins
          → fast water erodes (picks up sediment)
            → slow water deposits (drops sediment)
              → terrain height changes
                → changed terrain deflects wind
                  → (repeat)
```

Every component feeds the next. No component works well alone.

## What Exists Now
| System | Status | Problem |
|--------|--------|---------|
| Wind | None | Moisture propagation hardcoded +y |
| Rain | Manual toggle, uniform random | No spatial pattern, no wind coupling |
| WaterMap | Simple heightfield flow | No pipes, volume not conserved, unstable |
| Erosion | All disabled | Droplets stalled in priority-flood basins, river carving made cliffs |
| Sediment | None | No transport, no deposition |
| MoistureMap | Exists, propagates from water | Not wind-driven, vegetation uses avg_moisture |

## Design

### Layer 1: Wind Field

**Computed once per wind direction change (~5ms), updated with curl noise each tick.**

```rust
pub struct WindField {
    width: usize,
    height: usize,
    wind_x: Vec<f64>,      // per-tile x component
    wind_y: Vec<f64>,      // per-tile y component
    wind_speed: Vec<f64>,  // magnitude cache
    wind_shadow: Vec<f64>, // 0.0 = full shadow, 1.0 = exposed
    moisture_carried: Vec<f64>, // moisture being transported by wind
}
```

**Computation:**
1. Start with prevailing direction (seasonal)
2. For each tile, ray-march upwind 30 tiles
   - If terrain rises: this tile is in wind shadow → reduce speed
   - If terrain drops: wind accelerates
3. Compute terrain gradient, deflect wind perpendicular to slope
4. Narrow gaps (low min-corridor-width from ChokepointMap): boost speed
5. Add curl noise for turbulence: `perlin(x*0.05, y*0.05, time*0.01)`

**Wind particle viewer:** Spawn sparse particles that drift with the local wind vector. Visual debug mode. Basically free — use existing particle system.

### Layer 2: Moisture Transport

**Wind carries moisture across the map. Deposited as rain.**

Each tick:
1. Wind picks up moisture from water surfaces: `moisture_carried[i] += water_evap_rate * wind_speed[i]`
2. Transport moisture along wind vector: advect the `moisture_carried` field
3. Orographic precipitation: when wind pushes air uphill, moisture drops:
   `rain_amount = moisture_carried * orographic_lift * precip_rate`
   where `orographic_lift = max(0, dot(wind_dir, terrain_gradient))`
4. Random light rain everywhere (10% of orographic amount)
5. Deposited rain goes into WaterMap

Result: windward mountain slopes get lots of rain (lush). Leeward gets very little (dry). Rain shadow emerges from terrain + wind interaction.

### Layer 3: Pipe Model Water

**Replaces current WaterMap with 8-directional pipe flow.**

```rust
pub struct PipeWater {
    width: usize,
    height: usize,
    depth: Vec<f64>,           // water depth per tile
    flux: Vec<[f64; 8]>,       // flow rate through 8 pipes (N,NE,E,SE,S,SW,W,NW)
    velocity: Vec<(f64, f64)>, // derived: avg velocity from flux (for sediment)
}
```

**Per tick:**
1. Compute pressure at each tile: `pressure = depth + terrain_height`
2. For each pipe: `flux_delta = dt * g * (pressure_here - pressure_neighbor) / pipe_length`
3. Update flux: `flux[i] = max(0, flux[i] + flux_delta)` (no negative flow)
4. Scale flux if total outflow > available water (conservation)
5. Update depth from net flux: `depth += dt * (sum_inflow - sum_outflow)`
6. Compute velocity from flux for sediment transport

**Diagonal pipes:** NE/SE/SW/NW pipes have `pipe_length = sqrt(2)` and flow area adjusted. Same math, just different constant.

**Performance:** 8 pipes * 65K tiles = 520K flux updates per tick. ~0.3ms. Fine.

### Layer 4: Sediment Transport (IS the erosion)

**No separate erosion pass. Water naturally erodes fast-flowing areas and deposits in slow areas.**

```rust
pub struct Sediment {
    // Stored alongside PipeWater
    suspended: Vec<f64>,  // sediment in water per tile
    capacity: Vec<f64>,   // max sediment water can carry (cache)
}
```

**Per tick (after water flow):**
1. Compute carrying capacity: `capacity = K_c * depth * velocity_magnitude^2`
   (Hjulström curve simplified)
2. If suspended < capacity: **erode** terrain
   - `erode_amount = K_e * (capacity - suspended) * dt`
   - `terrain_height -= erode_amount`
   - `suspended += erode_amount`
3. If suspended > capacity: **deposit** sediment
   - `deposit_amount = K_d * (suspended - capacity) * dt`
   - `terrain_height += deposit_amount`
   - `suspended -= deposit_amount`
4. Transport suspended sediment with water velocity (advect)

**Result:** Rivers naturally deepen where they flow fast (steep gradient) and build up alluvial fans where they slow down (flat areas). Erosion and deposition are emergent from water dynamics.

**Tuning constants:**
- `K_c = 0.01` (capacity coefficient — how much sediment water can carry)
- `K_e = 0.001` (erosion rate — how fast terrain is removed)
- `K_d = 0.01` (deposition rate — how fast sediment settles, typically faster than erosion)

### Layer 5: Terrain Feedback

When terrain height changes (from erosion/deposition):
1. Reclassify biome for affected tiles (already implemented, runs every 500 ticks)
2. Mark nav graph regions dirty (hierarchical pathfinding update)
3. Mark chokepoint map dirty (if significant height change)
4. Wind field recompute if cumulative terrain change exceeds threshold

## Implementation Order

### Phase A: Wind (foundation)
1. `WindField` struct with prevailing direction
2. Terrain deflection via ray march
3. Curl noise turbulence
4. Wind particle viewer (debug visualization)
5. Wire into moisture: wind advects moisture_carried field

### Phase B: Pipe Water (replace WaterMap)
1. `PipeWater` struct with 8-directional flux
2. Pressure-driven flow
3. Conservation + stability
4. Replace WaterMap in all systems (rendering, moisture, farms)
5. Rain deposits from wind moisture transport

### Phase C: Sediment Transport (emergent erosion)
1. `Sediment` struct alongside PipeWater
2. Hjulström capacity formula
3. Erosion + deposition per tick
4. Sediment advection with water flow
5. Terrain height update + reclassification trigger

### Phase D: Integration + Polish
1. Full loop test: wind → rain → water → erosion → terrain → wind
2. Performance profiling (target: all layers < 2ms total at 256x256)
3. Visual tuning: river appearance, erosion scars, alluvial fans
4. Seasonal variation: monsoon season, dry season, winter freeze

## Quick Win (now, before full system)
From erosion research: move `droplet_erosion` BEFORE `priority_flood` in the pipeline. This fixes the "droplets stall in flat basins" bug. Can re-enable pipeline erosion without the cliff-lake artifact.

## Performance Budget

| System | Per tick | Notes |
|--------|---------|-------|
| Wind field recompute | ~5ms | Only on direction change (seasonal) |
| Curl noise update | ~0.2ms | Perlin samples at visible tiles only |
| Pipe water flow | ~0.3ms | 8 pipes * 65K tiles |
| Sediment transport | ~0.2ms | Piggybacks on water flow |
| Moisture advection | ~0.1ms | Simple field advection |
| **Total per tick** | **~0.8ms** | Well within 16ms frame budget |

## Dependencies
- Existing: heights, terrain pipeline, MoistureMap, VegetationMap, ChokepointMap
- New: WindField, PipeWater, Sediment
- Design docs: wind_system.md (superseded by this), dynamic_terrain_classification.md

## Open Questions
- Should pipe water run at half frequency (every 2 ticks) to save budget?
- Do we need a separate "river" terrain type, or is water depth > threshold enough?
- How deep should erosion be allowed to go? Cap at some fraction of original height?
- Should sediment have types (sand, clay, rock) or just be generic "dirt"?
- At what scale does this need GPU acceleration? 512x512? 1024x1024?
