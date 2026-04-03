# Soil Degradation System (Unified SoilFertilityMap)

**Status:** Proposed
**Pillars:** Geography Shapes Everything (#1), Emergent Complexity (#2), Observable Simulation (#4)
**Phase:** 2 (Economy Depth)
**Last updated:** 2026-04-01

---

## What

A single per-tile `SoilFertilityMap` that every terrain-modifying system writes to and every terrain-dependent system reads from. Fertility is not a farm-only concept -- it is a world property that farming, deforestation, mining, flooding, foot traffic, and vegetation all modify. One map, many writers, many readers.

This replaces the per-FarmPlot `fertility: f64` proposed in `farming_changes_terrain.md` with a world-level grid. Farms still degrade the soil beneath them, but so does everything else. And farms read from the same grid that vegetation growth, regrowth probability, and visual color all read from.

## Why

Six different design docs propose modifications to soil quality:

| Doc | What it does to soil |
|-----|---------------------|
| `farming_changes_terrain.md` | Harvest depletes fertility; fallow recovers it |
| `deforestation_regrowth.md` | Vegetation removal exposes soil to erosion -> fertility loss |
| `mining_changes_terrain.md` | Quarry/ScarredGround tiles have degraded soil |
| `simulation_chains.md` | Flooding deposits alluvial fertility; erosion removes it |
| `seasonal_terrain_effects.md` | Fire leaves ash (+fertility); floods deposit sediment (+fertility) |
| `water_proximity_farming.md` | River proximity accelerates fallow recovery |

Without a shared layer, each doc invents its own fertility tracking. `farming_changes_terrain.md` puts `fertility: f64` on `FarmPlot`. `simulation_chains.md` proposes a `SoilFertilityMap` but only describes farm interaction. `deforestation_regrowth.md` mentions erosion vulnerability but has no fertility grid to write to. The result: six features that should interact cannot, because they each store soil state differently or not at all.

The unified map also enables a critical emergent behavior: a tile that was deforested, then farmed to exhaustion, then abandoned, recovers differently than a tile that was only farmed. The accumulated damage from multiple systems compounds. History is written into the soil.

## Current State

| Layer | Status | Where |
|-------|--------|-------|
| `SoilType` enum | Exists. 6 variants (Sand, Loam, Alluvial, Clay, Rocky, Peat) with yield multipliers. | `terrain_pipeline.rs:63-80` |
| `Game::soil` | Stored. `Vec<SoilType>` from pipeline. Never read at runtime. | `game/mod.rs:329` |
| `MoistureMap` | Working. Per-tile moisture, updated from WaterMap. | `simulation.rs:241-300` |
| `VegetationMap` | Working. Per-tile vegetation density. | `simulation.rs:841-845` |
| `TrafficMap` | Working. Per-tile foot traffic from villager movement. | `simulation.rs:982-1050` |
| `WaterMap` | Working. Per-tile water level, flow, evaporation. | `simulation.rs:9-14` |
| `FarmPlot.fertility` | Does not exist. Proposed in `farming_changes_terrain.md` but not implemented. | -- |
| `SoilFertilityMap` | Does not exist. Mentioned in `simulation_chains.md` but not specified in detail. | -- |
| `mine_counts` | Does not exist. Proposed in `mining_changes_terrain.md`. | -- |
| `fire_risk` | Does not exist. Proposed in `seasonal_terrain_effects.md`. | -- |

---

## Design

### Data Structure

```rust
/// Per-tile soil fertility. Written by many systems, read by many systems.
/// Follows the same pattern as MoistureMap, VegetationMap, TrafficMap.
#[derive(Clone, Serialize, Deserialize)]
pub struct SoilFertilityMap {
    pub width: usize,
    pub height: usize,
    fertility: Vec<f64>,  // 0.0 (barren rock) to 1.0 (pristine alluvial)
}

impl SoilFertilityMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            fertility: vec![0.5; width * height],  // overwritten at init
        }
    }

    /// Read fertility at a tile. Returns 0.0 for out-of-bounds.
    pub fn get(&self, x: usize, y: usize) -> f64 {
        if x < self.width && y < self.height {
            self.fertility[y * self.width + x]
        } else {
            0.0
        }
    }

    /// Absolute set. Used during initialization.
    pub fn set(&mut self, x: usize, y: usize, val: f64) {
        if x < self.width && y < self.height {
            self.fertility[y * self.width + x] = val.clamp(0.0, 1.0);
        }
    }

    /// Add to fertility (clamped to 1.0). Used by recovery/deposit systems.
    pub fn add(&mut self, x: usize, y: usize, delta: f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.fertility[idx] = (self.fertility[idx] + delta).clamp(0.0, 1.0);
        }
    }

    /// Subtract from fertility (clamped to 0.0). Used by degradation systems.
    pub fn degrade(&mut self, x: usize, y: usize, delta: f64) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.fertility[idx] = (self.fertility[idx] - delta).clamp(0.0, 1.0);
        }
    }
}
```

Stored on `Game`:

```rust
pub struct Game {
    // ... existing fields ...
    pub soil_fertility: SoilFertilityMap,  // NEW
}
```

Memory: one `f64` per tile. 256x256 = 512KB. Same footprint as `MoistureMap`.

### Initialization from Terrain Pipeline

At world-gen, `SoilFertilityMap` is initialized from `SoilType` and terrain features. This is the **baseline** -- the world before any civilization touches it.

```rust
fn init_soil_fertility(
    soil: &[SoilType],
    terrain: &TileMap,
    vegetation: &VegetationMap,
    width: usize,
    height: usize,
) -> SoilFertilityMap {
    let mut fert = SoilFertilityMap::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let base = match soil[idx] {
                SoilType::Alluvial => 1.0,
                SoilType::Loam     => 0.85,
                SoilType::Peat     => 0.75,
                SoilType::Clay     => 0.70,
                SoilType::Sand     => 0.40,
                SoilType::Rocky    => 0.15,
            };
            // Terrain override: some tiles have inherently zero fertility
            let terrain_mult = match terrain.get(x, y) {
                Some(t) if !t.is_walkable() => 0.0,  // Water, BuildingWall
                Some(Terrain::Mountain) => 0.05,       // Bare rock
                Some(Terrain::Snow) => 0.1,            // Frozen
                Some(Terrain::Desert) => 0.15,         // Arid
                _ => 1.0,
            };
            // Vegetation provides a small boost (root systems hold soil)
            let veg_bonus = vegetation.get(x, y) * 0.1;
            fert.set(x, y, (base * terrain_mult + veg_bonus).clamp(0.0, 1.0));
        }
    }
    fert
}
```

**Relationship to SoilType:** `SoilType` is the geological classification -- it never changes. `SoilFertilityMap` is the current state of the topsoil -- it changes constantly. A tile can be `SoilType::Alluvial` (great potential) with fertility 0.1 (exhausted by farming). Think of SoilType as the ceiling and current fertility as where you actually are.

### Connection to Existing SoilType

`SoilType` remains the immutable pipeline output. It controls:

- **Initial fertility** (see table above)
- **Degradation resistance** -- how much fertility each harvest costs
- **Recovery rate** -- how fast fertility regenerates when the stressor is removed
- **Yield multiplier** -- still used independently in the food formula (it encodes soil texture suitability, not just fertility)

The yield formula from `farming_changes_terrain.md` becomes:

```
food = base_yield * soil_type.yield_multiplier() * soil_fertility.get(x, y) * skill_mult * moisture_factor
```

Both `SoilType` (immutable geology) and `SoilFertilityMap` (mutable topsoil) contribute. Alluvial soil with degraded fertility still produces more than rocky soil with degraded fertility, because the geological suitability (drainage, mineral content) persists even when the topsoil is depleted.

---

### Writers: Systems That Modify Fertility

Every system that changes soil quality writes to `SoilFertilityMap` through the `add()` and `degrade()` methods. No system reads AND writes in the same tick to avoid order-dependence -- writes are accumulated and applied at the end of the simulation step (or interleaved safely because `add`/`degrade` are commutative operations on a clamped float).

#### Writer 1: Farming (harvest depletion)

**Source:** `system_farms` in `ecs/systems.rs`
**When:** Each completed harvest cycle
**Effect:** `soil_fertility.degrade(tile_x, tile_y, degradation_rate)`

| SoilType | Degradation per Harvest | Harvests to Zero |
|----------|------------------------|------------------|
| Alluvial | 0.02 | ~50 |
| Loam | 0.04 | ~21 |
| Clay | 0.04 | ~17 |
| Peat | 0.06 | ~12 |
| Sand | 0.08 | ~5 |
| Rocky | 0.10 | ~1-2 |

This replaces the `FarmPlot.fertility` field proposed in `farming_changes_terrain.md`. The farm reads from the world grid instead of tracking its own copy. Multiple farms on adjacent tiles degrade a visible patch, not isolated per-entity values.

#### Writer 2: Deforestation Erosion

**Source:** Erosion step in `simulation.rs` (or new erosion-vegetation coupling from `simulation_chains.md`)
**When:** Per-tick, on tiles where `vegetation < 0.2` AND tile receives rain/water flow
**Effect:** `soil_fertility.degrade(x, y, erosion_loss)`

```
erosion_loss = base_erosion_rate * (1.0 - vegetation.get(x, y)).max(0.0) * water_flow_strength
```

Where `base_erosion_rate` = 0.0001 per tick. A fully deforested tile under heavy rain loses fertility roughly 10x faster than a vegetated tile. Over a season (~2500 ticks), sustained deforestation on a slope can drop fertility by 0.25.

**Sediment deposit (alluvial gain):** Eroded material flows downhill. Downstream tiles (lower elevation, near rivers) gain fertility:

```
soil_fertility.add(downstream_x, downstream_y, erosion_loss * 0.5)
```

Half the lost fertility is deposited downstream. The other half is "lost to the river" (carried away). This creates the realistic pattern: upland erosion, lowland enrichment.

#### Writer 3: Mining Scarring

**Source:** Mining terrain transitions in `system_ai` result processing
**When:** Mountain tile transitions to Quarry or QuarryDeep; StoneDeposit depletes to ScarredGround
**Effect:** Immediate degradation on the mined tile AND adjacent tiles

| Transition | Mined Tile | Each Adjacent Tile (4-neighbors) |
|-----------|-----------|--------------------------------|
| Mountain -> Quarry | Set to 0.05 | degrade by 0.1 |
| Quarry -> QuarryDeep | Set to 0.0 | degrade by 0.15 |
| StoneDeposit -> ScarredGround | Set to 0.1 | degrade by 0.05 |

Mining scars spread fertility damage to neighboring tiles. A quarry surrounded by farmland is bad news -- the farm soil gets degraded from mining activity, not just from harvesting. This creates a spatial trade-off: mine near your farms for short haul distances, but accept soil damage.

#### Writer 4: Flooding (Alluvial Deposit)

**Source:** Flood system in `simulation.rs` (from `seasonal_terrain_effects.md`)
**When:** FloodWater recedes (water_level drops below 0.2 on a previously flooded tile)
**Effect:** `soil_fertility.add(x, y, 0.15)`

Spring floods destroy crops but enrich the soil. A tile that floods every spring steadily gains fertility -- up to the 1.0 cap. This is the Nile delta mechanic: the best farmland is the most dangerous.

**Erosive flooding (edge case):** If water_level exceeds a high threshold (> 0.8, severe flood), the current is erosive rather than depositional:

```
if water_level > 0.8 {
    soil_fertility.degrade(x, y, 0.1);  // violent flood strips topsoil
} else {
    soil_fertility.add(x, y, 0.15);     // gentle flood deposits sediment
}
```

#### Writer 5: Foot Traffic Compaction

**Source:** `TrafficMap` decay step in `simulation.rs`
**When:** Per traffic decay tick (same cadence as `TrafficMap::decay()`)
**Effect:** Tiles with `traffic.get(x, y) > COMPACTION_THRESHOLD` lose fertility slowly

```
if traffic.get(x, y) > 50.0 {
    let compaction = (traffic.get(x, y) - 50.0) * 0.00001;
    soil_fertility.degrade(x, y, compaction);
}
```

Heavy foot traffic (paths between stockpile and resources, areas around buildings) compacts soil and destroys root structure. This creates a realistic fertility dead-zone around the settlement center and along well-worn paths. The effect is slow -- hundreds of ticks of sustained traffic to matter -- but cumulative.

Roads (auto-built from traffic) do NOT compact further. The road surface protects the soil. Only walkable non-road terrain (Grass, Sand, etc.) suffers compaction. This creates an emergent incentive for roads: they lock in the path and stop the damage from spreading.

```
// Only compact natural terrain, not roads or building floors
if terrain.get(x, y) != Some(Terrain::Road)
    && terrain.get(x, y).map_or(false, |t| t.is_walkable())
{
    // apply compaction
}
```

#### Writer 6: Fallow Recovery (Passive)

**Source:** Per-tick update in `SoilFertilityMap::update()` (new method)
**When:** Every tick, for tiles that have NO active farm AND are not Mountain/Water/Building
**Effect:** `soil_fertility.add(x, y, recovery_rate)`

Recovery rate depends on three factors:

```rust
fn recovery_rate(
    soil_type: SoilType,
    vegetation: f64,
    river_dist: u32,
    season: Season,
) -> f64 {
    if matches!(season, Season::Winter | Season::Autumn) {
        return 0.0;  // dormant seasons: no recovery
    }

    let base = match soil_type {
        SoilType::Alluvial => 0.0004,
        SoilType::Loam     => 0.0003,
        SoilType::Clay     => 0.0002,
        SoilType::Peat     => 0.0003,
        SoilType::Sand     => 0.0001,
        SoilType::Rocky    => 0.00005,
    };

    // Vegetation accelerates recovery (root systems rebuild soil)
    let veg_mult = 1.0 + vegetation * 0.5;  // 0 veg = 1.0x, full veg = 1.5x

    // Water proximity accelerates recovery (from water_proximity_farming.md)
    let water_mult = match river_dist {
        0..=1 => 2.0,
        2..=3 => 1.75,
        4..=6 => 1.5,
        _     => 1.0,
    };

    base * veg_mult * water_mult
}
```

Full recovery times (from 0.0 to 1.0, best case -- spring/summer, full vegetation, river-adjacent):

| SoilType | Base Rate | With Veg + River | Ticks to Full | Approx Seasons |
|----------|-----------|-----------------|---------------|----------------|
| Alluvial | 0.0004 | 0.0012 | ~830 | ~0.3 (one month) |
| Loam | 0.0003 | 0.0009 | ~1110 | ~0.4 |
| Clay | 0.0002 | 0.0006 | ~1670 | ~0.7 |
| Peat | 0.0003 | 0.0009 | ~1110 | ~0.4 |
| Sand | 0.0001 | 0.0003 | ~3330 | ~1.3 |
| Rocky | 0.00005 | 0.00015 | ~6670 | ~2.7 |

Worst case (no vegetation, far from water): multiply those times by ~3x.

This replaces the `FarmPlot.fallow_ticks` and per-farm recovery logic from `farming_changes_terrain.md`. Recovery is a world-level process, not a farm-level one. A tile recovers whether or not a farm is on it.

#### Writer 7: Vegetation Cover (Stabilization)

**Source:** `VegetationMap::update()` in `simulation.rs`
**When:** When vegetation grows on a tile (existing growth step)
**Effect:** Implicit through recovery_rate multiplier (Writer 6)

Vegetation does not directly write to `SoilFertilityMap`. Instead, vegetation density modulates recovery rate (Writer 6) and erosion loss (Writer 2). This keeps the interaction chain clean: vegetation protects soil from loss and accelerates its return, but the actual fertility changes flow through erosion and recovery.

#### Writer 8: Fire (Ash Fertilization)

**Source:** Fire burnout in `simulation.rs` (from `seasonal_terrain_effects.md`)
**When:** `Terrain::Fire` transitions to `Terrain::Scorched`
**Effect:** `soil_fertility.add(x, y, 0.05)`

A small fertility boost from ash nutrients. Fire is destructive (destroys vegetation, resets regrowth) but leaves behind marginally richer soil. Over time, a cycle of fire and regrowth on the same land gradually increases baseline fertility -- the ecological role of wildfire.

---

### Readers: Systems That Use Fertility

#### Reader 1: Farm Growth Rate

**System:** `system_farms` in `ecs/systems.rs`
**How:** `fertility_factor = soil_fertility.get(farm.tile_x, farm.tile_y)`
**Effect:** Fertility directly scales crop growth speed

```
growth_rate = base_rate * skill_mult * moisture_factor * fertility_factor
```

At fertility 0.1, farms grow at 10% speed. At fertility below 0.05, farms produce 0 food (effective death threshold). This replaces the `FarmPlot.fertility` field -- the farm reads the world state, not its own copy.

#### Reader 2: Farm Yield

**System:** `system_farms` harvest completion
**How:** `yield_mult = soil_fertility.get(farm.tile_x, farm.tile_y)`
**Effect:** Food produced scales with fertility

```
food = max(0, round(base_yield * soil_type.yield_multiplier() * yield_mult * skill_mult))
```

A farm on exhausted soil produces less food per harvest AND takes longer to grow. Double pressure.

#### Reader 3: Auto-Build Farm Placement

**System:** `auto_build_farms` in `game/build.rs`
**How:** Score candidate tiles by `soil_fertility.get(x, y)`
**Effect:** Auto-build prefers high-fertility tiles for new farms

```
placement_score = soil_fertility.get(x, y) * 0.5
                + moisture_factor * 0.3
                + proximity_to_stockpile * 0.2
```

Farms naturally gravitate toward the best soil. Exhausted tiles near the settlement are skipped in favor of fresh land further out. This drives the expansion arc.

#### Reader 4: Auto-Fallow Decision

**System:** `system_farms` in `ecs/systems.rs`
**How:** Check `soil_fertility.get(x, y) < FALLOW_THRESHOLD`
**Effect:** Farms on depleted soil auto-enter fallow state

```
const FALLOW_ENTER: f64 = 0.3;
const FALLOW_EXIT: f64 = 0.6;

if soil_fertility.get(x, y) < FALLOW_ENTER {
    farm.fallow = true;
}
if farm.fallow && soil_fertility.get(x, y) > FALLOW_EXIT {
    farm.fallow = false;
}
```

Hysteresis prevents rapid flip-flopping. A fallow farm stops accepting workers, letting the world-level recovery (Writer 6) do its work.

#### Reader 5: Vegetation Regrowth Probability

**System:** `system_regrowth` in `ecs/systems.rs`
**How:** Gate regrowth on `soil_fertility.get(x, y) > 0.2`
**Effect:** Saplings cannot sprout on barren soil

From `deforestation_regrowth.md`: Bare -> Sapling requires adjacent forest AND `vegetation > 0.2`. Add a fertility gate: `soil_fertility.get(x, y) > 0.2`. Severely degraded land (mined, over-farmed, compacted) cannot support regrowth until the soil recovers. This creates persistent deforestation scars in abused areas -- nature cannot heal what the soil cannot support.

#### Reader 6: Visual Terrain Color

**System:** `Terrain::fg()` / `Terrain::bg()` in rendering (or overlay)
**How:** Modulate terrain color saturation by fertility
**Effect:** Low-fertility tiles appear washed out, grey-brown

```rust
fn fertility_color(base: Color, fertility: f64) -> Color {
    let desat = 1.0 - fertility;
    Color(
        lerp(base.r, 140, desat),
        lerp(base.g, 130, desat),
        lerp(base.b, 120, desat),
    )
}
```

This applies to ALL terrain types, not just farms. A deforested, eroded hillside looks pale and washed out. A river valley with high fertility is deep green. The fertility gradient is readable from the zoomed-out view.

#### Reader 7: Fertility Overlay

**System:** Overlay rendering in `game/render.rs`
**How:** New `OverlayMode::SoilFertility` variant
**Effect:** Heat map showing fertility across the map

Green (1.0) -> Yellow (0.5) -> Red (0.2) -> Grey (0.0). One of the overlay modes cycled with `o`. Shows the cumulative effect of all writers. The player can diagnose: "my farmland is pale yellow because I over-farmed it; the river valley is green because floods keep enriching it; the quarry zone is grey because mining destroyed the soil."

---

### Update Frequency

Not all writers run every tick. Frequency is tuned for performance and gameplay pacing.

| Writer | Frequency | Rationale |
|--------|-----------|-----------|
| Farming depletion | On harvest (event-driven) | Discrete event, not per-tick |
| Deforestation erosion | Every tick (within erosion loop) | Already iterates tiles for water flow |
| Mining scarring | On mine completion (event-driven) | Discrete event |
| Flood deposit | On flood recede (event-driven) | Discrete event |
| Traffic compaction | Every traffic decay tick (~every tick) | Piggybacks on existing TrafficMap decay |
| Fallow recovery | Every 10 ticks | Slow process, no need for per-tick granularity |
| Fire ash | On fire burnout (event-driven) | Discrete event |

Recovery (Writer 6) runs every 10 ticks for performance. At the rates specified, this means the recovery delta per update is `rate * 10`, applied every 10 ticks. Mathematically equivalent to per-tick at 1/10 the CPU cost. With 65K tiles, even a full scan every 10 ticks is ~0.1ms.

**Optimization:** Recovery only needs to run on tiles that are below their maximum. Track a `dirty_count` or use the same random-sampling approach as `system_regrowth` (sample N random tiles per update). For a 256x256 map, scanning all tiles every 10 ticks is cheap enough that random sampling is not needed yet.

---

### The FarmPlot Simplification

With the unified `SoilFertilityMap`, `FarmPlot` no longer needs its own fertility tracking. The component simplifies:

```rust
pub struct FarmPlot {
    pub growth: f64,           // 0.0 to 1.0 (existing)
    pub harvest_ready: bool,   // (existing)
    pub worker_present: bool,  // (existing)
    pub pending_food: u32,     // (existing)
    pub tile_x: usize,        // NEW: map position (from simulation_chains.md)
    pub tile_y: usize,        // NEW: map position (from simulation_chains.md)
    pub soil_type: SoilType,  // NEW: underlying soil type (from farming_changes_terrain.md)
    pub fallow: bool,          // NEW: resting state (from farming_changes_terrain.md)
    pub harvests: u32,         // NEW: lifetime harvest count (diagnostic/visual only)
}
```

Fields **removed** compared to `farming_changes_terrain.md` proposal:
- `fertility: f64` -- replaced by `soil_fertility.get(tile_x, tile_y)`
- `fallow_ticks: u32` -- not needed; recovery is world-level, not farm-level

The farm reads the world. The farm writes to the world on harvest (via `soil_fertility.degrade()`). The farm does not maintain its own copy of soil state.

---

## Edge Cases

| Case | Behavior | Rationale |
|------|----------|-----------|
| Multiple writers hit same tile in one tick | All deltas applied via `add()`/`degrade()`, clamped to [0.0, 1.0] | Operations are commutative and clamped; order does not matter |
| Farm placed on zero-fertility tile | Growth rate = 0, effectively dead farm | Player should not farm here; auto-build avoids these tiles |
| Fertility exceeds initial SoilType baseline (e.g., flood enrichment on Sandy soil) | Allowed. Fertility can reach 1.0 regardless of SoilType. | SoilType still limits yield_multiplier(); fertile sand is better but still sand |
| Tile transitions terrain type (Forest -> Stump, Mountain -> Quarry) | Fertility is modified by the transition event but the grid value persists across terrain changes | Fertility is a separate layer from terrain type |
| Mining + farming on adjacent tiles | Mining scars degrade neighboring farm fertility | Intentional spatial trade-off; players learn to separate industrial and agricultural zones |
| River dries up (drought) | Recovery rate drops (water_mult goes to 1.0), erosion increases (less vegetation) | Double pressure through existing simulation chains |
| Save/load with old save (no SoilFertilityMap) | `#[serde(default)]` initializes from SoilType baseline, as if the world was never modified | Safe migration; existing saves play slightly differently but not broken |
| Map edge tiles | `get()` returns 0.0 for out-of-bounds; `add()`/`degrade()` are no-ops | Consistent with MoistureMap/VegetationMap bounds handling |
| Traffic on Road tiles | No compaction applied; roads protect the soil | Emergent incentive for road infrastructure |
| Fallow farm with active traffic nearby | Recovery slowed by adjacent compaction | Realistic: a field next to a busy path recovers slower |

---

## Test Criteria

### Unit Tests

| Test | Setup | Assertion |
|------|-------|-----------|
| `fertility_init_from_soil_type` | Create SoilFertilityMap, init from SoilType vec with known types | Alluvial tile = 1.0, Rocky tile = 0.15, Mountain tile ~= 0.0 |
| `fertility_degrade_clamps_to_zero` | Set fertility to 0.05, degrade by 0.1 | Value is 0.0, not negative |
| `fertility_add_clamps_to_one` | Set fertility to 0.95, add 0.15 | Value is 1.0, not > 1.0 |
| `harvest_degrades_world_fertility` | Place farm, complete harvest cycle, read soil_fertility at tile | Fertility decreased by soil-specific degradation rate |
| `farm_growth_scales_with_world_fertility` | Two farms: one on 1.0 fertility tile, one on 0.2 tile. Run 100 summer ticks. | High-fertility farm has ~5x more growth |
| `farm_yield_uses_world_fertility` | Complete harvest on 1.0 vs 0.5 fertility tiles | Food produced differs proportionally |
| `auto_fallow_reads_world_fertility` | Set world fertility at farm tile to 0.25 | Farm enters fallow state |
| `fallow_exit_reads_world_fertility` | Fallow farm; set world fertility to 0.65 | Farm exits fallow state |
| `deforestation_erosion_degrades_fertility` | Set vegetation to 0.0 on a tile, run erosion with water flow | Fertility at tile decreases |
| `erosion_deposits_downstream` | Erode uphill tile, check downhill tile | Downhill fertility increases (by ~half the uphill loss) |
| `mining_scars_adjacent_fertility` | Transition Mountain -> Quarry | Adjacent tiles lose 0.1 fertility |
| `flood_deposit_increases_fertility` | Simulate flood recede on a tile | Fertility increases by 0.15 |
| `severe_flood_erodes_fertility` | Set water_level > 0.8, then recede | Fertility decreases by 0.1 (not increases) |
| `traffic_compaction_degrades` | Accumulate 100+ traffic on a grass tile | Fertility decreases over time |
| `road_prevents_compaction` | Accumulate traffic on a Road tile | Fertility unchanged |
| `recovery_faster_near_river` | Two tiles at same fertility, one river_dist=1, one river_dist=20 | Near-river tile recovers faster |
| `recovery_zero_in_winter` | Run recovery step in winter | No fertility change |
| `recovery_requires_no_farm` | Tile with active farm vs tile without | Only farmless tile recovers |
| `fire_ash_adds_fertility` | Transition Fire -> Scorched on a tile | Fertility increases by 0.05 |
| `regrowth_gated_on_fertility` | Bare tile adjacent to Forest, fertility = 0.1 | Sapling does NOT sprout |
| `regrowth_allowed_above_threshold` | Bare tile adjacent to Forest, fertility = 0.3 | Sapling CAN sprout (probabilistic, run multiple trials) |
| `fertility_overlay_renders` | Set known fertility values, render overlay | Correct color mapping (green/yellow/red/grey) |

### Integration Tests

| Test | Setup | Assertion |
|------|-------|-----------|
| `over_farming_drives_expansion` | Run 5000 ticks with auto-build, limited starting area | New farms placed further from settlement as near-soil degrades |
| `river_valley_sustains_farming` | Farms placed near river vs far from river, run 10000 ticks | River farms still productive; distant farms exhausted |
| `deforestation_erosion_chain` | Clear forest on hillside, run 5000 ticks with rain | Uphill fertility decreases; downhill fertility increases |
| `mining_farming_conflict` | Place farms adjacent to mountain, mine the mountain | Farm fertility drops from mining scars |
| `full_cycle_recovery` | Farm to exhaustion, abandon, run 10000 ticks | Fertility recovers toward SoilType baseline |
| `serialize_round_trip` | Save game with modified fertility, load, compare | All fertility values preserved |

---

## Dependencies

| Dependency | Status | Required For |
|-----------|--------|-------------|
| `SoilType` enum + `Game::soil` | Exists | Initialization, degradation rates |
| `MoistureMap` with `.get()` | Exists | Recovery rate factor (indirect) |
| `VegetationMap` with `.get()` | Exists | Erosion vulnerability, recovery rate |
| `TrafficMap` with `.get()` | Exists | Compaction writer |
| `WaterMap` | Exists | Erosion water flow, flood detection |
| `FarmPlot.tile_x`, `tile_y` | Proposed (simulation_chains.md) | Farm reads fertility at position |
| `river_distance` stored in Game | Proposed (water_proximity_farming.md) | Recovery rate water bonus |
| Terrain transitions (Stump, Quarry, etc.) | Proposed (deforestation, mining docs) | Triggers for fertility writes |
| Flood system (FloodWater) | Proposed (seasonal_terrain_effects.md) | Alluvial deposit trigger |
| Fire system (Fire -> Scorched) | Proposed (seasonal_terrain_effects.md) | Ash fertilization trigger |
| Erosion enabled at runtime | Proposed (simulation_chains.md) | Erosion-fertility coupling |

### Implementation Order

The `SoilFertilityMap` itself has no hard dependencies -- it can be created and initialized from existing `SoilType` data immediately. Writers are added incrementally as their parent features land:

1. **SoilFertilityMap struct + initialization** -- standalone, no dependencies
2. **Farming reads fertility** -- requires `FarmPlot.tile_x/tile_y` (simulation_chains or water_proximity)
3. **Farming writes fertility** -- same dependency as #2
4. **Fallow recovery** -- requires `river_distance` for water bonus (water_proximity_farming)
5. **Traffic compaction** -- standalone, TrafficMap exists
6. **Mining scars** -- requires mining terrain transitions (mining_changes_terrain)
7. **Deforestation erosion** -- requires runtime erosion enabled (simulation_chains)
8. **Flood deposit** -- requires flood system (seasonal_terrain_effects)
9. **Fire ash** -- requires fire system (seasonal_terrain_effects)
10. **Visual: fertility overlay + color modulation** -- standalone after #1

---

## Estimated Scope

### Tier 1: Core Map + Farm Integration (~4 hours)

| Task | Files | Estimate |
|------|-------|----------|
| Define `SoilFertilityMap` struct with `get`/`set`/`add`/`degrade` | `simulation.rs` | 30 min |
| `init_soil_fertility` from SoilType + terrain | `simulation.rs` | 30 min |
| Store on `Game`, initialize in `Game::new()` | `game/mod.rs` | 15 min |
| `system_farms` reads `soil_fertility.get()` for growth + yield | `ecs/systems.rs` | 30 min |
| `system_farms` calls `soil_fertility.degrade()` on harvest | `ecs/systems.rs` | 15 min |
| Auto-fallow reads world fertility (enter < 0.3, exit > 0.6) | `ecs/systems.rs` | 20 min |
| Serialization with `#[serde(default)]` for backward compat | `simulation.rs`, `game/save.rs` | 20 min |
| Unit tests for map operations + farm integration | `simulation.rs`, `ecs/mod.rs` | 60 min |

### Tier 2: Passive Recovery + Traffic Compaction (~2 hours)

| Task | Files | Estimate |
|------|-------|----------|
| `SoilFertilityMap::update_recovery()` method (every 10 ticks) | `simulation.rs` | 30 min |
| Wire recovery into `Game::step()` | `game/mod.rs` | 10 min |
| Traffic compaction in `TrafficMap::decay()` or parallel step | `simulation.rs` | 30 min |
| Recovery tests + compaction tests | `simulation.rs` | 30 min |
| Road-prevents-compaction test | `simulation.rs` | 15 min |

### Tier 3: Cross-System Writers (~3 hours, incremental as features land)

| Task | Files | Estimate |
|------|-------|----------|
| Mining scar writes (on terrain transition) | `ecs/systems.rs` | 30 min |
| Erosion-fertility coupling | `simulation.rs` | 45 min |
| Flood deposit write (on FloodWater recede) | `simulation.rs` | 30 min |
| Fire ash write (on Fire -> Scorched) | `simulation.rs` | 15 min |
| Tests for each writer | various | 45 min |

### Tier 4: Visuals (~1.5 hours)

| Task | Files | Estimate |
|------|-------|----------|
| Fertility overlay mode (`OverlayMode::SoilFertility`) | `game/render.rs` | 30 min |
| Fertility color modulation on terrain rendering | `game/render.rs` | 30 min |
| Visual tests / screenshot verification | manual | 30 min |

### Total: ~10.5 hours across all tiers

Tier 1 is the critical path. Tiers 2-4 are additive and can land alongside or after their respective parent features.
