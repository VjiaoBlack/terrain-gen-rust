# Farming Changes Terrain

**Status:** Proposed
**Pillar:** 1 (Geography Shapes Everything), 4 (Observable Simulation)
**Phase:** 2 (Economy Depth)
**Dependencies:** FarmPlot component, SoilType pipeline, system_farms

---

## Problem

Farms are visually static. A freshly placed farm and a farm harvested twenty times look identical between growth cycles. There is no concept of soil degradation -- a farm on alluvial floodplain and a farm on sandy scrubland differ only in yield multiplier, and even that is not connected at runtime (SoilType is computed at world-gen but never read by `system_farms`). The farming landscape cannot tell a story. You cannot look at the map at tick 50K and see which fields have been worked hard and which are resting.

### What exists today

| Layer | Current State |
|-------|--------------|
| **FarmPlot component** | `growth: f64` (0.0-1.0), `harvest_ready: bool`, `worker_present: bool`, `pending_food: u32` |
| **Visual states** | 4 states: dirt `·` (growth<0.3), growing `♠` (0.3-0.7), mature `"` (0.7-1.0), harvest-ready/pending `♣` (gold) |
| **SoilType** | 6 types (Sand, Loam, Alluvial, Clay, Rocky, Peat) with yield multipliers, computed at world-gen, stored in `Game::soil` vec |
| **Soil <-> Farm link** | None. `system_farms` does not read `Game::soil`. Yield comes from season + civ skill + event multiplier only |
| **Terrain tile under farm** | Set to `BuildingFloor` at construction. Never changes back |
| **Degradation** | None. A farm produces identically on harvest 1 and harvest 100 |
| **Fallow / recovery** | None. No concept of resting land |

---

## Design

### Core Idea

Every harvest extracts something from the soil. Fertile land can sustain many harvests before degrading. Poor land degrades fast. Degraded land produces less and looks visibly exhausted. Leaving a field fallow lets the soil recover. The player (or auto-build) must eventually rotate farming areas or watch yields collapse. The map accumulates a visible history: dark rich fields, pale exhausted ones, green recovering fallow plots.

---

### 1. Fertility System

Add a `fertility` field to `FarmPlot` that connects farms to the underlying `SoilType` and tracks cumulative use.

#### New fields on FarmPlot

```rust
pub struct FarmPlot {
    pub growth: f64,           // 0.0 to 1.0 (existing)
    pub harvest_ready: bool,   // (existing)
    pub worker_present: bool,  // (existing)
    pub pending_food: u32,     // (existing)
    pub fertility: f64,        // NEW: 0.0 (dead) to 1.0 (pristine), init from SoilType
    pub harvests: u32,         // NEW: total lifetime harvests on this plot
    pub fallow: bool,          // NEW: true = resting, no planting
    pub fallow_ticks: u32,     // NEW: how long this plot has been fallow
}
```

#### Initialization

When a farm is placed, read the `SoilType` at that tile from `Game::soil` and set initial fertility:

| SoilType  | Initial Fertility | Degradation Resistance |
|-----------|------------------|----------------------|
| Alluvial  | 1.0              | High (0.02 per harvest) |
| Loam      | 0.9              | Medium (0.04 per harvest) |
| Clay      | 0.8              | Medium (0.04 per harvest) |
| Peat      | 0.7              | Low (0.06 per harvest) |
| Sand      | 0.5              | Very low (0.08 per harvest) |
| Rocky     | 0.3              | Very low (0.10 per harvest) |

"Degradation resistance" is how much fertility drops per harvest. Alluvial soil near rivers lasts 50 harvests before hitting zero. Sandy soil lasts about 6.

#### Degradation

Each time `system_farms` completes a harvest (growth hits 1.0 and worker collects):

```
fertility -= degradation_rate_for_soil_type
fertility = fertility.clamp(0.0, 1.0)
harvests += 1
```

Fertility directly scales yield:

```
food_produced = base_yield * fertility * skill_mult * event_mult
```

Where `base_yield` is 3 (current hardcoded value). At fertility 0.3, a harvest produces 1 food instead of 3. At fertility below 0.1, the farm produces 0 food (soil is dead).

#### Recovery (Fallow)

A farm marked fallow stops accepting workers and slowly recovers fertility:

```
// Per tick, if fallow and not winter:
fertility += recovery_rate   // 0.0002 per tick
fallow_ticks += 1
```

Recovery rate by soil type (ticks to recover from 0.0 to 1.0):

| SoilType  | Recovery Rate | Full Recovery Time |
|-----------|--------------|-------------------|
| Alluvial  | 0.0004/tick  | ~2,500 ticks (~1 season) |
| Loam      | 0.0003/tick  | ~3,333 ticks (~1.3 seasons) |
| Clay      | 0.0002/tick  | ~5,000 ticks (~2 seasons) |
| Peat      | 0.0003/tick  | ~3,333 ticks (~1.3 seasons) |
| Sand      | 0.0001/tick  | ~10,000 ticks (~4 seasons) |
| Rocky     | 0.00005/tick | ~20,000 ticks (~8 seasons) |

Alluvial soil near rivers recovers in about one season of fallow. Sandy soil takes a full year. Rocky soil barely recovers at all -- you probably should not have farmed there.

Recovery only happens in spring and summer. Winter and autumn provide no recovery (dormant season).

#### Auto-Fallow Logic

When fertility drops below 0.3, the farm auto-enters fallow state. Villager AI skips fallow farms when looking for work. The farm exits fallow automatically when fertility recovers above 0.6 (hysteresis prevents rapid flip-flopping).

Auto-build should factor in soil fertility when choosing where to place new farms: prefer tiles with SoilType Alluvial or Loam, avoid Rocky.

---

### 2. Visual States

The farm's appearance should communicate its fertility and use history at a glance. This is the observable simulation pillar -- if you cannot see it, it does not count.

#### Growth Stage Visuals (updated)

The existing 4-state visual system expands to encode both growth stage AND fertility. Fertility affects the color saturation/brightness of the growth sprites.

**Map Mode (Mode A) -- glyph carries meaning:**

| State | Glyph | Color | Meaning |
|-------|-------|-------|---------|
| Freshly tilled (growth < 0.1) | `~` | Brown, brightness scales with fertility | Turned earth, ready for planting |
| Seedling (0.1 - 0.3) | `·` | Green-brown, dim | Seeds sprouting |
| Growing (0.3 - 0.7) | `,` | Green, brightness scales with fertility | Stalks rising |
| Mature (0.7 - 1.0) | `"` | Rich green | Crop nearly ready |
| Harvest ready | `♣` | Gold | Ready for collection |
| Pending food | `♣` | Bright gold (existing) | Food waiting for pickup |
| Fallow (recovering) | `·` | Pale green, slowly deepening | Resting field, weeds returning |
| Exhausted (fertility < 0.1) | `·` | Grey-brown | Dead soil, nothing will grow |

**Landscape Mode (Mode B) -- color carries meaning:**

| State | Texture | Color Field | Read As |
|-------|---------|-------------|---------|
| Freshly tilled | `~` `=` alternating | Dark brown (60, 40, 20) | Plowed furrows |
| Seedling | `.` sparse | Brown-green blend | Sparse shoots in dirt |
| Growing | `,` `'` mixed | Green, saturation from fertility | Leafy field |
| Mature | `"` `♠` dense | Deep green | Dense crop canopy |
| Harvest ready | `♣` `*` | Warm gold | Ripe grain/vegetables |
| Fallow early | `.` very sparse | Pale tan (160, 140, 100) | Bare earth, some stubble |
| Fallow recovering | `'` `,` increasing | Tan -> pale green gradient | Weeds and wild grass returning |
| Exhausted | `.` sparse | Grey (140, 130, 120) | Dry, cracked, lifeless |

#### Fertility Color Gradient

Fertility modulates the base color of every growth state. This creates a visible spectrum across the farming landscape:

```
// Pseudocode for fertility-adjusted color:
fn fertility_color(base: Color, fertility: f64) -> Color {
    // High fertility: vivid, saturated
    // Low fertility: desaturated, shifted toward grey-brown
    let desat = 1.0 - fertility;
    Color(
        lerp(base.r, 140, desat),  // pull toward grey
        lerp(base.g, 130, desat),
        lerp(base.b, 120, desat),
    )
}
```

At a glance, the player sees: bright green fields are healthy, pale/grey fields are exhausted. A farming district tells its story through color alone.

#### Terrain Tile Changes

The `BuildingFloor` terrain under a farm should change to reflect farming state. Add a new terrain variant or use existing ones:

| Farm State | Terrain Under Farm |
|------------|-------------------|
| Active farm (any growth stage) | `BuildingFloor` (existing, no change) |
| Fallow (recovering) | `Grass` (revert to grass while resting) |
| Abandoned / demolished | `Sand` or `Grass` depending on fertility |

When a farm is demolished on exhausted soil (fertility < 0.2), the terrain reverts to `Sand` instead of `Grass`. The scar remains visible. If fertility was still decent (>0.5), it reverts to `Grass` and eventually the vegetation system re-covers it. This means over-farmed land is literally visible on the map long after the farm is gone.

---

### 3. Soil-Farm Connection at Runtime

Currently `SoilType` exists only as a pipeline output. This design connects it to live gameplay.

#### Read soil on farm placement

In `spawn_farm_plot` (or the build completion handler), look up `Game::soil[tile_index]` and use it to initialize `FarmPlot::fertility` and store a degradation rate.

Add a `soil_type` field to FarmPlot (or compute degradation/recovery rates from the stored SoilType on the fly):

```rust
pub struct FarmPlot {
    // ... existing + new fertility fields ...
    pub soil_type: SoilType,  // the underlying soil, set at construction
}
```

#### Yield formula (replacing current hardcoded 3)

```
base_food = 3
soil_mult = soil_type.yield_multiplier()    // existing: 0.4 .. 1.25
fert_mult = fertility                        // 0.0 .. 1.0
skill_mult = (1.0 + civ_farming / 100.0)
event_mult = events.farm_yield_multiplier()
season_mult = season_growth_rate / base_summer_rate  // normalize

food = max(0, round(base_food * soil_mult * fert_mult * skill_mult * event_mult))
```

This means:
- Alluvial soil, full fertility, skilled farmers: 3 * 1.25 * 1.0 * 1.5 * 1.0 = 5 food
- Sandy soil, half fertility, unskilled: 3 * 0.7 * 0.5 * 1.0 * 1.0 = 1 food
- Rocky soil, exhausted: 3 * 0.4 * 0.05 = 0 food

The spread is dramatic and intentional. Geography shapes everything.

#### Water proximity bonus

Farms within 4 tiles of a river (use `Game::river_mask` BFS distance, already computed for SoilType) get a recovery rate bonus of +50%. This makes river-adjacent farmland the most sustainable, creating the classic fertile river valley pattern. No irrigation building needed yet -- the proximity IS the mechanic.

---

### 4. Gameplay Implications

#### Settlement Arc

**Early game (Explore):** First farm is placed on whatever soil is nearby. It works fine initially. Player does not think about soil.

**Mid game (Expand):** After 10-15 harvests, the original farm starts producing less food. Auto-build places new farms. If the settlement is near alluvial soil, this barely matters. If on sandy ground, food pressure builds and motivates expansion toward the river valley.

**Late game (Exploit/Endure):** A mature settlement has a visible patchwork: active fields (green), fallow fields (pale), exhausted plots (grey), and virgin farmland at the expanding frontier. The farming landscape tells the story of the settlement's growth and resource pressure.

#### Strategic Decisions

These emerge naturally from the system, no UI needed:

- **Where to expand farms:** Alluvial > Loam > Clay >> Sand > Rocky. Auto-build encodes this preference.
- **When to rest fields:** Auto-fallow at 0.3 fertility handles this, but a savvy observer notices the pattern.
- **River valley dominance:** Settlements near rivers farm sustainably. Desert settlements must constantly expand farmland or starve.
- **Over-farming death spiral:** Too many villagers + not enough farmland = all fields exhausted = famine = population crash. Visible as grey fields surrounding the settlement. Emergent, not scripted.

#### Interaction with Existing Systems

| System | Interaction |
|--------|------------|
| **Drought event** | Drought should also slow fallow recovery (no rain = no recovery). Doubles the pain. |
| **Bountiful Harvest** | Does NOT restore fertility. You get more food per harvest, but the soil still degrades. Bounty is a trap if you do not rest fields after. |
| **Seasonal growth** | Winter: no growth, no recovery. Fields sit dormant. Spring: fastest growth AND fastest recovery. Fallow in winter is wasted time. |
| **Auto-build** | Should prefer high-fertility soil when placing farms. Should place more farms when average fertility across existing farms drops below 0.5. |
| **Population growth** | More mouths = more farming pressure = faster degradation = forces expansion. The Malthusian pressure is visible on the map. |
| **Exploration** | Discovering alluvial soil along a distant river creates a reason to expand in that direction. Geography pulls the settlement. |

---

### 5. Implementation Plan

#### Tier 1: Core (must-have)

1. **Add fertility to FarmPlot** -- new fields: `fertility`, `harvests`, `fallow`, `fallow_ticks`, `soil_type`
2. **Initialize fertility from SoilType** -- read `Game::soil` at farm placement
3. **Degrade fertility on harvest** -- subtract soil-specific rate in `system_farms`
4. **Scale food yield by fertility** -- replace hardcoded `3` with formula
5. **Auto-fallow** -- farms below 0.3 fertility enter fallow, exit at 0.6
6. **Fallow recovery** -- slow fertility gain per tick when fallow and not winter
7. **Visual: fertility affects sprite color** -- desaturate toward grey as fertility drops
8. **Visual: fallow state appearance** -- pale green/tan for resting fields
9. **Visual: exhausted state** -- grey-brown, distinct from other states

#### Tier 2: Rich (makes it special)

10. **Water proximity recovery bonus** -- farms near rivers recover faster
11. **Auto-build soil preference** -- prefer alluvial/loam when placing farms
12. **Terrain scar on demolish** -- exhausted farms leave Sand, healthy leave Grass
13. **Notification on degradation** -- "Farm soil is becoming exhausted" at 0.3 fertility
14. **Overlay: soil fertility** -- heatmap overlay showing fertility across all farms (green = healthy, red = exhausted)

#### Tier 3: Dream (future)

15. **Crop rotation** -- different crop types that degrade different soil nutrients
16. **Compost/manure** -- processing building that converts food waste to fertility restoration
17. **Irrigation building** -- placed near water, extends water proximity bonus to distant farms
18. **Soil type evolution** -- extreme over-farming permanently downgrades SoilType (Loam -> Sand)
19. **Visual: plow lines** -- farms show directional furrow pattern based on orientation

---

### 6. Testing Plan

| Test | Validates |
|------|-----------|
| `farm_fertility_decreases_on_harvest` | Fertility drops by soil-specific rate after each harvest cycle |
| `farm_fertility_scales_yield` | Food produced = base * soil_mult * fertility (not hardcoded 3) |
| `farm_auto_fallow_at_threshold` | Farm enters fallow when fertility < 0.3, exits at > 0.6 |
| `farm_fallow_recovery_by_soil` | Alluvial recovers faster than Sand; Rocky barely recovers |
| `farm_no_recovery_in_winter` | Fallow farms do not gain fertility in winter |
| `farm_exhausted_produces_nothing` | Fertility < 0.1 yields 0 food |
| `farm_init_fertility_from_soil` | Farm on Alluvial starts at 1.0, on Rocky at 0.3 |
| `farm_visual_desaturates_with_fertility` | Sprite color shifts toward grey as fertility drops |
| `farm_fallow_visual_distinct` | Fallow farm shows pale green/tan, not growth-stage colors |
| `farm_river_proximity_recovery_bonus` | Farm within 4 tiles of river recovers 50% faster |
| `farm_demolish_terrain_scar` | Demolished exhausted farm leaves Sand tile; healthy leaves Grass |
| `auto_build_prefers_fertile_soil` | New farm placement favors Alluvial/Loam tiles over Rocky/Sand |

---

### 7. Migration / Serialization

New FarmPlot fields need `#[serde(default)]` for backward compatibility with existing saves:

```rust
#[serde(default = "default_fertility")]
pub fertility: f64,        // default 0.8 for existing farms

#[serde(default)]
pub harvests: u32,

#[serde(default)]
pub fallow: bool,

#[serde(default)]
pub fallow_ticks: u32,

#[serde(default)]
pub soil_type: SoilType,  // default Loam
```

Existing saves load with fertility 0.8 and Loam soil -- reasonable defaults that do not break running games.
