# Systems Chain Through Simulation

**Status:** Proposed
**Pillar:** 2 — Emergent Complexity from Simple Agents (Section B)
**Last updated:** 2026-04-01

## Problem

Events bypass the simulation and directly manipulate gameplay values. A drought calls `farm_yield *= 0.7` through `EventSystem::farm_yield_multiplier()`, which `system_farms()` multiplies into the flat `growth_rate`. The drought has no physical presence in the world — it does not touch water levels, soil, or any intermediate system. The same pattern applies to every environmental effect: the code IS the logic, instead of the simulation being the logic.

This creates two concrete problems:

1. **New systems can't interact with old ones.** When we add irrigation, we would need to write `if drought && irrigated` rather than having irrigation simply raise water levels, which the drought lowered. Every cross-system interaction requires a new special case.

2. **Players can't read causation.** A drought looks identical to a debuff icon. There is no visible chain of consequences to observe, diagnose, or mitigate through world-shaping. The simulation tells no story.

### Current code paths

| Event | Current effect | Code location |
|-------|---------------|---------------|
| Drought | `farm_yield_multiplier() -> 0.7` | `game/mod.rs:196-199` |
| Bountiful Harvest | `farm_yield_multiplier() -> 2.0` | `game/mod.rs:200` |
| Farm growth | `base_rate * skill_mult` (flat per-season) | `ecs/systems.rs:803-810` |
| Moisture | Drives vegetation, ignores farms entirely | `simulation.rs:270-340` |
| Water | Flows downhill, erodes, evaporates — no link to crops | `simulation.rs:104-230` |

The `FarmPlot` component (`ecs/components.rs:454`) has `growth: f64` and `worker_present: bool` but no concept of local water, soil quality, or moisture. It grows at a flat rate determined solely by season and a global multiplier.

## Design

Every environmental effect flows through physical simulation state. No event directly modifies a gameplay output. Instead, events modify world conditions, and gameplay systems read those conditions.

**Core rule:** If you can't point to the intermediate tile state that changed, the chain is wrong.

### Intermediate systems required

These are the simulation layers that chains flow through. Some exist, some need additions.

| Layer | Exists today | What it tracks | Changes needed |
|-------|-------------|----------------|----------------|
| **WaterMap** | Yes | Per-tile water level, flow, evaporation | Add drought modifier to `rain()` and `evaporation` |
| **MoistureMap** | Yes | Per-tile moisture, derived from water | None — already reads WaterMap |
| **SoilFertility** | No | Per-tile fertility (0.0-1.0) | New grid in simulation.rs |
| **VegetationMap** | Yes | Per-tile vegetation density | Add erosion vulnerability when vegetation is low |
| **Heights** | Yes (Vec<f64>) | Terrain elevation | Already modified by erosion |

### New component: SoilFertility

A per-tile `f64` grid (like MoistureMap) tracking how fertile each tile is. Initialized from terrain type at world-gen (alluvial soil near rivers = high, sand = low, mountain = zero). Degrades from repeated farming. Recovers slowly when left fallow. Farms read this value instead of using a flat growth rate.

```
pub struct SoilFertilityMap {
    width: usize,
    height: usize,
    fertility: Vec<f64>,  // 0.0 (barren) to 1.0 (rich)
}
```

### New FarmPlot fields

`FarmPlot` gains awareness of its tile position so `system_farms` can sample local conditions:

```
pub struct FarmPlot {
    pub growth: f64,
    pub harvest_ready: bool,
    pub worker_present: bool,
    pub pending_food: u32,
    pub tile_x: usize,      // NEW: farm's map position
    pub tile_y: usize,      // NEW: farm's map position
}
```

### New system_farms signature

```
pub fn system_farms(
    world: &mut World,
    season: Season,
    skill_mult: f64,           // civ-wide farming skill (keep)
    moisture: &MoistureMap,    // NEW: local moisture lookup
    fertility: &SoilFertilityMap,  // NEW: local soil lookup
)
```

Growth rate becomes: `base_rate * skill_mult * moisture_factor * fertility_factor`

Where:
- `moisture_factor` = `moisture.get(tile_x, tile_y).clamp(0.0, 1.0)` — zero moisture means zero growth
- `fertility_factor` = `fertility.get(tile_x, tile_y).clamp(0.1, 1.0)` — degraded soil grows slowly, never fully zero

The `EventSystem::farm_yield_multiplier()` method is **deleted**. Drought no longer needs to exist as a yield multiplier — it exists as reduced water.

## Simulation Chains

### Chain 1: Drought

**Current:** `GameEvent::Drought` -> `farm_yield_multiplier() -> 0.7` -> flat growth reduction.

**Proposed:**

```
Drought event fires
  -> SimConfig.rain_rate *= 0.2 (80% less rain)
  -> SimConfig.evaporation *= 3.0 (faster drying)
    -> WaterMap: less water accumulates, existing water evaporates faster
      -> MoistureMap: moisture decays toward zero (existing decay logic handles this)
        -> system_farms: moisture_factor drops, crop growth slows or stops
          -> Less food at harvest
          -> Longer time to next harvest
            -> Food pressure on settlement
```

**Observable:** Player sees water levels drop on the water overlay. Farms near rivers hold out longer because river-fed moisture persists. Farms on dry ground fail first. The crisis has spatial texture — it is not uniform.

**Irrigation interaction (future):** An irrigation channel moves water from a river to farm tiles. During drought, the river still has water (reduced, but present). Irrigation raises local water levels -> moisture stays up -> farms keep growing. No special-case code. The simulation handles it.

**Bountiful Harvest replacement:** Instead of `farm_yield_multiplier() -> 2.0`, a bountiful season increases `rain_rate *= 1.5` and soil fertility recovery rate. More water -> more moisture -> faster growth. The effect is the same but flows through the world.

### Chain 2: Flooding

**Current:** No flooding system exists. Water flows and pools but has no gameplay effect beyond terrain erosion.

**Proposed:**

```
Heavy spring rain (seasonal rain_rate spike)
  -> WaterMap: water accumulates in low-lying areas
    -> Tiles with water_level > flood_threshold:
      -> Farm destruction: FarmPlot on flooded tile has growth reset to 0
      -> Building damage: buildings on flooded tiles take durability hits
      -> Movement: flooded tiles become unwalkable (temporary water terrain)
    -> After flood recedes:
      -> MoistureMap: high residual moisture in flood plain
      -> SoilFertility: alluvial deposit bonus (+0.1 fertility on previously flooded tiles)
        -> Farms rebuilt on flood plain grow faster than before
          -> Incentive to farm flood plains despite risk
```

**Observable:** Player sees water rise in spring. Low farms flood — visible as water tiles replacing farm tiles. After the flood, those tiles are extra fertile. The tension: flood plains are the best farmland AND the most dangerous. Geography creates a real decision.

**Levee interaction (future):** A levee building (wall variant placed along riverbank) blocks water flow into farm areas. Water pools behind the levee instead. No special-case code — the levee is just terrain that water cannot flow through.

### Chain 3: Deforestation -> Erosion

**Current:** Trees are cut by villagers (resource gathering). VegetationMap tracks density. Erosion exists in WaterMap but is disabled by default (`erosion_enabled: false`). These systems do not interact.

**Proposed:**

```
Villagers cut trees for wood
  -> VegetationMap: density drops in harvested area
    -> Erosion vulnerability: tiles with vegetation < 0.2 have erosion_strength *= 3.0
      -> Rain on exposed soil:
        -> WaterMap erosion carves terrain (heights decrease)
        -> Soil loss: SoilFertility decreases on eroded tiles
        -> Sediment deposit: SoilFertility increases in downstream basins
          -> Upland farms degrade
          -> Lowland/river farms improve (alluvial deposit)
    -> MoistureMap: less vegetation means less moisture retention
      -> Moisture decays faster in deforested areas
        -> Remaining vegetation struggles to regrow (positive feedback loop)
          -> Desertification if unchecked
```

**Observable:** Player clears a forest for wood. Over many ticks, the exposed hillside visibly erodes — terrain height drops, soil color changes to indicate low fertility. Meanwhile, the river delta downstream gets richer. The player can read the history: "I cleared that ridge and now it's barren, but look at the valley farms."

**Reforestation interaction (future):** A tree-planting building or policy halts cutting in an area. Vegetation regrows (existing VegetationMap.grow logic). Once vegetation > 0.2, erosion vulnerability drops. Soil stabilizes. The feedback loop reverses — slowly.

**Implementation note:** This chain requires enabling `erosion_enabled` at runtime (currently compile-time config). The erosion strength per tile should be modulated by local vegetation density rather than being a global constant.

### Chain 4: Over-farming -> Soil Degradation

**Current:** Farms produce food at a flat rate forever. There is no concept of soil depletion. A farm placed at tick 100 produces identically at tick 50,000.

**Proposed:**

```
Farm is harvested repeatedly
  -> SoilFertility: each harvest decreases fertility by 0.02
    -> fertility_factor in growth rate drops
      -> Crops grow slower each cycle
        -> Less food per unit time
          -> More farms needed to sustain same population
            -> Settlement expands into new land
              -> Old farmland goes fallow
                -> SoilFertility slowly recovers (+0.005/tick when no farm active)
                  -> Player can return to restored land later
```

**Observable:** Farms that have been running for many cycles visibly slow down — longer time between harvests. The player notices production declining and must decide: build more farms elsewhere, or let old fields rest. Expansion is not optional; it is driven by the land itself.

**Crop rotation interaction (future):** Different crop types deplete different soil nutrients. Alternating crops (if we add crop types) slows degradation. A field planted with the same crop continuously degrades fastest. No special-case code — each crop type has a `soil_depletion_rate` and the system does the math.

**Fertilizer interaction (future):** A composting building converts food waste to fertilizer. Applying fertilizer to a tile increases SoilFertility. The chain becomes: over-farm -> degrade -> compost -> restore. Each link is a visible system.

## Migration Path

The refactor touches three hot paths (farm growth, event processing, simulation update). This is the order that minimizes breakage.

### Step 1: Add SoilFertilityMap (no behavior change)

- Add `SoilFertilityMap` to `simulation.rs`, initialized to 1.0 everywhere
- Add `tile_x`, `tile_y` to `FarmPlot` (set at spawn time)
- Pass new maps to `system_farms` but ignore them (multiply by 1.0)
- All existing tests pass unchanged

### Step 2: Wire moisture into farm growth

- `system_farms` reads `MoistureMap` at each farm's tile position
- `moisture_factor = moisture.get(x, y).clamp(0.1, 1.0)` (floor of 0.1 so farms near water aren't strictly required yet)
- Farms near rivers grow faster; farms on dry ground grow slower
- Tune moisture floor until existing game balance feels similar to current

### Step 3: Replace drought multiplier with water reduction

- `GameEvent::Drought` modifies `SimConfig` (lower rain_rate, higher evaporation) instead of returning a yield multiplier
- Delete `EventSystem::farm_yield_multiplier()` (or reduce it to always return 1.0 as a no-op, then remove)
- Drought now has spatial variation: river-adjacent farms survive, dry farms suffer
- Update test `drought_reduces_farm_yield` to test water levels instead of multiplier

### Step 4: Add soil degradation

- Each harvest decreases `SoilFertilityMap` at the farm's tile
- `fertility_factor` in growth rate uses the real value instead of 1.0
- Fallow recovery: tiles without an active farm slowly regenerate fertility
- Initialize fertility from terrain type at world-gen (river-adjacent > grass > sand)

### Step 5: Enable erosion-vegetation coupling

- Enable `erosion_enabled` by default (or per-seed based on terrain type)
- Modulate `erosion_strength` per tile based on `VegetationMap` density
- Low vegetation -> high erosion vulnerability
- Erosion modifies `SoilFertilityMap` (eroded tiles lose fertility, deposit tiles gain it)

### Step 6: Add flooding threshold

- Tiles where `WaterMap.get(x, y) > flood_threshold` trigger gameplay effects
- Farm growth resets on flooded tiles
- Post-flood fertility bonus
- Tune threshold so floods are seasonal events, not constant

## Testing Strategy

Each step has targeted tests. The chain structure means we can test links independently.

| Test | Validates |
|------|-----------|
| `drought_lowers_water_levels` | Drought SimConfig changes reduce WaterMap water |
| `lower_water_reduces_moisture` | Already exists: `moisture_decays_without_water` |
| `low_moisture_slows_farm_growth` | Farm with moisture=0.1 grows slower than moisture=1.0 |
| `river_adjacent_farm_survives_drought` | Farm near water tile keeps growing during drought |
| `harvest_degrades_soil` | Repeated harvests lower SoilFertilityMap at farm tile |
| `fallow_restores_soil` | Unharvested tile's fertility increases over time |
| `deforestation_increases_erosion` | Low vegetation tiles erode faster under rain |
| `erosion_reduces_soil_fertility` | Eroded tiles have lower SoilFertilityMap values |
| `flood_resets_farm_growth` | Farm on tile with water > threshold has growth zeroed |
| `flood_deposits_fertility` | Post-flood tiles gain fertility bonus |

## Performance Notes

- `SoilFertilityMap` is a flat `Vec<f64>`, same as MoistureMap. O(1) per lookup, ~512KB for a 256x256 map. Negligible.
- `system_farms` gains two map lookups per farm entity per tick. Farms are typically <20 entities. Negligible.
- Erosion-vegetation coupling adds one VegetationMap lookup per tile in the erosion inner loop. This loop already iterates all tiles. One extra read per tile is ~0.1ms on 256x256. Acceptable.
- No new per-tick allocations. No new entity queries.

## Open Questions

- **Moisture floor for farms:** Should farms with zero moisture produce anything at all? A floor of 0.1 prevents instant death but removes pressure. A floor of 0.0 makes water access mandatory, which is a stronger design signal but harder to balance for early game (pre-irrigation).
- **Fertility initialization:** Should fertility come from the terrain pipeline (biome-based) or be computed at runtime from moisture/vegetation at tick 0? Pipeline-based is simpler; runtime-based is more emergent.
- **Flood threshold tuning:** What water level constitutes a "flood"? Too low and every rain floods. Too high and floods never happen. Needs playtesting with the spring rain spike.
- **Erosion performance at scale:** Runtime erosion on 256x256 every tick may be too expensive. The existing viewport culling helps, but we may need to run erosion every N ticks instead of every tick.
- **Event log messaging:** When drought reduces water instead of directly reducing yield, the event log message "Farm yields reduced" is no longer accurate. It should say "Water levels dropping" — the player infers the farm impact from observation.
