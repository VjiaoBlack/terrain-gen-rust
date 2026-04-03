# Forest Fire Spread

**Status:** Proposed
**Pillars:** Geography Shapes Everything (#1), Emergent Complexity (#2), Explore/Expand/Exploit/Endure (#3), Observable Simulation (#4)
**Phase:** 2 (Economy Depth) — depends on deforestation_regrowth.md terrain stages
**Last updated:** 2026-04-01

## Problem

Forests are static obstacles that only change through villager harvesting (one tile at a time, per deforestation_regrowth.md). There is no large-scale landscape disruption that the player didn't cause. Every map change is gradual, predictable, and villager-initiated. This makes the world feel tame — a resource warehouse, not a living environment.

The game design doc lists "forest fire spread in dry seasons" as a Rich-tier feature under Pillar 1. Fire is the first system that is simultaneously opportunity AND threat from the same event: it destroys valuable timber and endangers villagers, but it also clears land cheaply and leaves fertile ash soil behind.

### What's missing today

1. **No environmental disasters.** Drought reduces farm yield (or water levels per simulation_chains.md), but nothing physically reshapes the map without villager action.

2. **No reason to fear dry seasons.** Summer has low rain and high evaporation (SeasonModifiers: rain_mult 0.5, evap_mult 2.0) but these numbers only affect water/moisture. There is no gameplay consequence to a dry forest.

3. **No large-scale land clearing.** Villagers chop one tree at a time. A 20x20 forest block takes hundreds of gather actions to clear. Fire could clear that same area in tens of ticks, creating a dramatic map-scale event.

4. **No spatial danger from terrain.** Wolves are mobile threats. Fire would be the first terrain-based threat — a hazard tied to specific tiles that spreads predictably based on wind and moisture. Villagers must route around it, not fight it.

## Design

### Core Idea

Fires ignite in dry forest during late summer, spread tile-by-tile through a cellular automata rule set driven by moisture, wind direction, and vegetation density, burn for a fixed duration per tile, and leave behind ash-enriched ground that is more fertile than normal cleared land. Fire is a force of nature that creates both crisis (timber loss, villager danger, building destruction) and opportunity (free land clearing, fertile ash soil, firebreak planning).

### Fire Lifecycle

Each tile progresses through a deterministic sequence:

```
Forest (or Sapling/Scrubland)
  -> Burning (active fire, N ticks)
    -> Scorched (smoldering, M ticks)
      -> AshGround (fertile cleared land)
        -> Sapling (normal regrowth from deforestation system)
          -> Forest
```

### New Terrain Variants

Two new `Terrain` variants (in addition to Stump/Bare/Sapling from deforestation_regrowth.md):

| Stage | Terrain Variant | Glyph | FG Color | BG Color | Speed Mult | A* Cost | Walkable | Description |
|-------|----------------|-------|----------|----------|------------|---------|----------|-------------|
| Burning | `Burning` | `*` | (255,120,20) | (180,40,10) | 0.3 | 10.0 | yes (but damages) | Active fire, bright orange/red |
| Scorched | `Scorched` | `~` | (80,60,40) | (40,30,20) | 0.9 | 1.1 | yes | Smoldering embers, dark brown |
| AshGround | `AshGround` | `.` | (120,115,100) | (70,65,55) | 1.0 | 1.0 | yes | Ash-covered ground, grey-brown |

`Burning` is technically walkable but should be avoided by AI (very high A* cost of 10.0). Any entity on a `Burning` tile takes damage each tick (see Danger section). `Scorched` is safe but impassable to fire spread (already burned). `AshGround` is essentially premium `Bare` — same movement properties but higher soil fertility.

### Ignition

Fires do not start randomly. They start from concrete conditions that the player can learn to anticipate.

**Ignition conditions (ALL must be true):**

1. **Season:** Summer or late Autumn (day >= 7 of Autumn). Fire cannot start in Spring or Winter.
2. **Moisture:** The tile's `MoistureMap` value is below **0.15**. Wet forests do not ignite.
3. **Terrain:** The tile is `Forest`, `Sapling`, or `Scrubland`. Other terrain types do not ignite.
4. **No recent rain:** At least 3 consecutive days since the last rain event on this tile (tracked per-tile or approximated from `WaterMap` level being < 0.05).

**Ignition sources (at least ONE must be true):**

- **Lightning strike:** Random chance per eligible tile per day during summer thunderstorms. Probability: **0.0001** per eligible tile per day-tick (roughly 1 fire start per 10,000 forest tiles per day — on a map with 5,000 forest tiles, expect a fire every ~2 days in peak dry season). Lightning only strikes during hours 14:00-20:00 (afternoon heat).
- **Villager activity:** A smithy or bakery (buildings with fire/heat) adjacent to a dry forest tile has a **0.001** chance per day of sparking ignition. Incentivizes clearing trees near industrial buildings.
- **Spread from adjacent Burning tile:** (see Spread section below — this is not ignition, it is propagation.)

**Where to check:** In `Game::step()`, once per in-game day (when `day_night.day` increments), run an ignition scan. Sample N random forest/sapling/scrubland tiles (N=50 for 256x256 maps). For each, check moisture and season conditions, then roll for lightning. Also check tiles adjacent to smithy/bakery buildings.

### Spread Algorithm

Fire spreads through a cellular automata evaluated once per tick on active fire tiles. This is NOT a full-map scan — only tiles currently `Burning` attempt to spread.

**Per-tick spread rule:**

For each `Burning` tile, check its 8 neighbors (Moore neighborhood). For each neighbor that is flammable (`Forest`, `Sapling`, `Scrubland`):

```
spread_probability = base_rate * moisture_factor * wind_factor * vegetation_factor
```

Where:

| Factor | Formula | Notes |
|--------|---------|-------|
| `base_rate` | **0.03** per tick | ~3% chance per neighbor per tick. At 30fps, a fire that burns 150 ticks will attempt spread ~150 times per neighbor. |
| `moisture_factor` | `(1.0 - moisture).powi(2)` | Quadratic falloff. Moisture 0.0 -> factor 1.0. Moisture 0.3 -> factor 0.49. Moisture 0.5 -> factor 0.25. Wet forests resist fire strongly. |
| `wind_factor` | See wind table below | Wind pushes fire directionally. |
| `vegetation_factor` | `vegetation_density.clamp(0.3, 1.0)` | Denser vegetation burns more readily. Sparse scrubland spreads slower than dense forest. |

**Wind factor table:**

Wind has a direction (one of 8 compass directions) and strength (0.0-1.0). The wind factor for spread depends on whether the neighbor is downwind, crosswind, or upwind of the burning tile:

| Neighbor relative to wind | Wind factor |
|---------------------------|-------------|
| Downwind (within 45 degrees) | `1.0 + strength * 2.0` (up to 3.0x) |
| Crosswind (45-135 degrees) | `1.0` (no modifier) |
| Upwind (within 45 degrees of opposite) | `1.0 - strength * 0.7` (down to 0.3x) |

**Wind model:** Wind direction and strength are set per-season and shift slowly. Summer default: prevailing wind from the west (matches moisture propagation direction in `MoistureMap::update`, which pushes moisture in +y). Wind strength varies by season:

| Season | Wind strength | Notes |
|--------|--------------|-------|
| Spring | 0.3 | Mild, fires unlikely anyway (wet) |
| Summer | 0.6 | Strong dry wind, peak fire season |
| Autumn | 0.5 | Moderate, fires possible late season |
| Winter | 0.7 | Strong but too wet/cold for fire |

Wind direction shifts by +/- 45 degrees randomly every 5 days. This means a fire that burns for multiple days may change shape as wind shifts — creating realistic irregular burn patterns rather than uniform circles.

**Implementation:** Maintain a `Vec<(usize, usize)>` of currently burning tiles (the "fire front"). Each tick, iterate the fire front. For each burning tile, check neighbors and roll for spread. New ignitions are added to the fire front for the next tick. Tiles that finish burning are removed. This is O(fire_front_size * 8) per tick, not O(map_size).

**Spread barriers (natural firebreaks):**

Fire cannot spread to or through:
- `Water` tiles (rivers are natural firebreaks)
- `Sand`, `Desert` tiles (nothing to burn)
- `Mountain`, `Snow`, `Tundra` tiles (too sparse/wet)
- `Road` tiles (compacted earth, no fuel)
- `Scorched` or `AshGround` tiles (already burned)
- `BuildingWall` tiles (stone/wood structure — but see Building Damage below)
- Any tile with `moisture > 0.6` (too wet to catch)

Rivers as firebreaks is a critical geographic interaction. A settlement behind a river is naturally protected from fire approaching from the forest side. This makes river-adjacent settlement placement even more valuable (Pillar 1).

### Burn Duration and Extinguishing

**Burn duration:** Each `Burning` tile burns for **120-180 ticks** (randomly assigned at ignition). This is roughly 4-6 in-game hours. After the burn timer expires, the tile transitions to `Scorched`.

**Scorched duration:** Each `Scorched` tile smolders for **300 ticks** (~1 in-game day), then transitions to `AshGround`. Scorched tiles do not spread fire but emit visual smoke particles (Landscape mode).

**Natural extinguishing:**
- **Rain:** When `rain_rate * season_mult > 0.5` (significant rain), all `Burning` tiles have a **10%** chance per tick of extinguishing directly to `Scorched`. Heavy spring rain extinguishes fires almost immediately. This is why spring fires are nearly impossible — even if ignited, rain kills them.
- **High moisture neighbor:** If 3+ orthogonal neighbors have `moisture > 0.5`, the burning tile has a **2%** bonus extinguish chance per tick (damp surroundings slow fire).
- **Season change to Winter:** All active fires extinguish within 50 ticks of Winter starting (increased rain, snow).

**No manual extinguishing (v1).** Villagers do not fight fires. They flee from them (see Danger section). Future versions may add a well building or bucket brigade behavior, but for v1, fire is a force of nature you plan around, not fight directly.

### Danger to Entities

Fire is a terrain-based hazard. Any entity standing on a `Burning` tile takes damage.

**Damage model:**
- Each tick on a `Burning` tile: entity takes **2 hunger damage** (or health damage if we add HP). At the current hunger-death threshold, a villager standing in fire dies in ~25 ticks (~1 in-game hour). They should flee long before this.
- Animals (prey, wolves) also take fire damage and flee.

**AI response — flee behavior:**
- Villagers within sight range (22 tiles) of a `Burning` tile enter a **fire flee** state. They move away from the nearest burning tile using the existing flee logic (same as predator flee, but triggered by terrain instead of entity).
- Flee direction: away from the fire centroid (average position of all visible burning tiles), not just the nearest one. This prevents villagers from fleeing one fire into another.
- Flee priority: fire flee overrides all behaviors except predator flee. A villager gathering wood who sees fire drops what they're doing and runs.
- Villagers resume normal behavior when no `Burning` tiles are within sight range.

**Pathfinding interaction:**
- `Burning` tiles have A* cost 10.0, so pathfinding routes around active fires automatically.
- If a villager's cached path passes through a newly `Burning` tile, path invalidation triggers a repath (this requires the path caching system from path_caching.md to check for terrain changes along the cached route).

### Building Damage

Buildings adjacent to `Burning` tiles can catch fire and be destroyed.

**Building ignition:** Each tick, a building entity on a tile orthogonally adjacent to a `Burning` tile has a **1%** chance of being destroyed. The building is removed and its tile becomes `Burning` (if the tile was `BuildingFloor`) or `Scorched` (if non-flammable base). This means fire can jump through a settlement if buildings are packed tightly with no gaps.

**Building exceptions:**
- `Wall` buildings are fire-resistant (stone construction). They act as firebreaks. **0.1%** ignition chance instead of 1%.
- `Garrison` buildings are fire-resistant (same as Wall).
- `Stockpile` destruction scatters stored resources (resources are lost). This is the most devastating fire consequence for a settlement.

**Emergent defensive strategy:** Players learn to leave gaps between forest and settlement, build walls as firebreaks on the forest-facing side, and keep forests near settlements thinned (deforestation as fire prevention). This creates a genuine tension: wood is valuable, but a forest next to your stockpile is a fire hazard.

### Aftermath: Ash Terrain

`AshGround` is the payoff that makes fire an opportunity, not just a disaster.

**Fertility bonus:** `AshGround` tiles have a soil fertility value of **0.8** (compared to normal `Bare` ground at 0.3-0.5 from deforestation, and river alluvial at 0.7-0.9). Ash is rich in potassium and phosphorus — this is geologically accurate.

**Interaction with SoilFertilityMap (from simulation_chains.md):**
- When a tile transitions to `AshGround`, set `SoilFertilityMap` at that position to **0.8**.
- This fertility decays over time (rain washes ash away) at **-0.005 per 400-tick regrowth check**, settling at the tile's base fertility after ~24,000 ticks (~2 game-years).
- Farms placed on fresh `AshGround` get a significant growth bonus. This creates a post-fire land rush: the best farmland is where the fire just was.

**Interaction with deforestation regrowth:**
- `AshGround` follows the same regrowth path as `Bare` ground: `AshGround -> Sapling -> Forest` using the same adjacency rules from deforestation_regrowth.md.
- Regrowth on `AshGround` is **faster** than on `Bare` ground because of the higher fertility. `AshGround -> Sapling` probability: **8%** per 400-tick check (vs 5% for `Bare`). Fire-cleared land recovers faster than axe-cleared land. This is ecologically accurate — fire-adapted forests regenerate aggressively.

**Interaction with VegetationMap:**
- `Burning` tiles: vegetation set to 0.0.
- `Scorched` tiles: vegetation stays 0.0.
- `AshGround` tiles: vegetation begins recovering immediately (high fertility drives faster VegetationMap growth).

### Visual Storytelling

Fire should be the most dramatic visual event in the game.

**Map mode (Mode A):**
- `Burning`: `*` in bright orange/red, flickers between orange and yellow FG each tick (alternating `Color(255,120,20)` and `Color(255,200,40)`)
- `Scorched`: `~` in dark brown, static
- `AshGround`: `.` in grey, transitions to light green as saplings appear

**Landscape mode (Mode B):**
- `Burning`: bright orange/red BG glow, ember particles (`·` in yellow/orange) rising from the tile
- Smoke particles above scorched tiles (grey `.` drifting upward/downwind)
- Fire front is visible from zoomed out as a bright orange line advancing through dark green forest
- Night fires are especially dramatic — bright against the dark landscape, visible from far away

**Observable chain (Pillar 4):**
The player should be able to watch the entire lifecycle:
1. Summer arrives, rain stops, moisture drops (visible on moisture overlay)
2. Lightning strikes a dry forest tile — fire starts (bright flash)
3. Fire spreads downwind, faster through dense forest, blocked by rivers
4. Villagers flee (visible panic, entities moving away from fire)
5. Fire reaches settlement edge — buildings catch if too close
6. Rain comes or fire runs out of fuel — burning stops
7. Scorched wasteland visible as dark patch
8. Ash ground appears — grey, open
9. Player places farms on ash — fast growth
10. Saplings appear at edges — forest slowly reclaims the burn scar

## Implementation Plan

### Step 0: Prerequisites
- `Terrain::Stump`, `Terrain::Bare`, `Terrain::Sapling` from deforestation_regrowth.md must exist.
- `SoilFertilityMap` from simulation_chains.md should exist (or `AshGround` fertility can be tracked as a simpler per-tile flag until then).

### Step 1: New Terrain Variants
- Add `Terrain::Burning`, `Terrain::Scorched`, `Terrain::AshGround` to `src/tilemap.rs`.
- Define `ch()`, `fg()`, `bg()`, `speed_multiplier()`, `move_cost()`, `is_walkable()` for each.
- `Burning`: walkable=true, speed=0.3, cost=10.0.
- `Scorched`: walkable=true, speed=0.9, cost=1.1.
- `AshGround`: walkable=true, speed=1.0, cost=1.0.

### Step 2: Wind System
- Add `WindState` to `simulation.rs`:
  ```
  pub struct WindState {
      pub direction: f64,  // radians, 0 = east, PI/2 = north
      pub strength: f64,   // 0.0 - 1.0
  }
  ```
- Initialize from season. Update direction by +/- 45 degrees every 5 days.
- Store in `Game` struct alongside `DayNightCycle`.

### Step 3: Fire State Tracking
- Add `FireMap` to `simulation.rs`:
  ```
  pub struct FireMap {
      width: usize,
      height: usize,
      burn_timer: Vec<u16>,      // ticks remaining for Burning tiles, 0 = not burning
      scorch_timer: Vec<u16>,    // ticks remaining for Scorched tiles
      fire_front: Vec<(usize, usize)>,  // active burning tile positions
  }
  ```
- `FireMap::tick()` is the main update function called each tick from `Game::step()`.

### Step 4: Ignition System
- In `FireMap::check_ignition()`, called once per day:
  - Sample 50 random forest/sapling/scrubland tiles.
  - Check moisture < 0.15, season is Summer or late Autumn.
  - Roll lightning probability (0.0001 per tile).
  - Check smithy/bakery adjacency (0.001 per tile per day).
  - Ignite: set tile to `Burning`, set burn_timer to rand(120..180), add to fire_front.

### Step 5: Spread System
- In `FireMap::tick_spread()`, called every tick:
  - For each tile in `fire_front`, check 8 neighbors.
  - Calculate `spread_probability` from moisture, wind, vegetation.
  - Roll for each neighbor. On success: ignite neighbor.
  - Decrement `burn_timer` for each burning tile. When 0: transition to `Scorched`, set scorch_timer.
  - Decrement `scorch_timer` for each scorched tile. When 0: transition to `AshGround`, set fertility.

### Step 6: Entity Danger
- In `system_ai` (`src/ecs/systems.rs`): scan visible tiles for `Burning`. If found, enter flee state.
- Flee direction: away from centroid of visible burning tiles.
- In `system_hunger` or a new `system_fire_damage`: entities on `Burning` tiles take 2 hunger damage per tick.

### Step 7: Building Damage
- In `FireMap::tick_spread()` or a separate `system_fire_buildings`:
  - For each `Burning` tile, check orthogonal neighbors for buildings.
  - Roll 1% (or 0.1% for walls/garrisons) per tick.
  - On success: despawn building entity, convert tile to `Burning`.

### Step 8: Ash Fertility
- When a tile transitions to `AshGround`, set `SoilFertilityMap` value to 0.8.
- `AshGround` uses the deforestation regrowth system for `Bare -> Sapling -> Forest` with boosted probability (8% vs 5%).

### Step 9: Tests
- Unit test: fire only ignites when moisture < 0.15 and season is Summer.
- Unit test: fire does not ignite in Spring or Winter.
- Unit test: fire spreads to adjacent forest tile (deterministic test with spread_probability = 1.0).
- Unit test: fire does not spread across water tiles.
- Unit test: fire does not spread across road tiles.
- Unit test: wind factor increases spread probability downwind.
- Unit test: wind factor decreases spread probability upwind.
- Unit test: high moisture prevents spread (moisture > 0.6 blocks ignition of neighbor).
- Unit test: burn timer expiration converts Burning -> Scorched.
- Unit test: scorch timer expiration converts Scorched -> AshGround.
- Unit test: AshGround has fertility 0.8.
- Unit test: AshGround regrowth probability is higher than Bare (8% vs 5%).
- Unit test: rain extinguishes burning tiles.
- Unit test: entity on Burning tile takes damage.
- Unit test: villager AI flees from visible fire.
- Integration test: start fire in forest, run 500 ticks, verify burned area, ash terrain, and no fire spreading past a river.

## Tuning Knobs

| Parameter | Default | Effect |
|-----------|---------|--------|
| Lightning probability (per tile per day) | 0.0001 | Fire frequency. Higher = more fires. |
| Smithy/bakery ignition chance (per day) | 0.001 | Settlement-caused fire risk. |
| Moisture ignition threshold | 0.15 | Below this, forests can ignite. Higher = more fires. |
| Base spread rate (per tick per neighbor) | 0.03 | Overall fire speed. |
| Wind strength by season | 0.3/0.6/0.5/0.7 | Directional spread intensity. |
| Burn duration (ticks) | 120-180 | How long each tile burns. |
| Scorch duration (ticks) | 300 | How long smoldering lasts. |
| Rain extinguish chance (per tick) | 0.10 | How fast rain kills fire. |
| Building ignition chance (per tick) | 0.01 | How vulnerable buildings are. |
| Wall/garrison ignition chance (per tick) | 0.001 | Stone building resistance. |
| AshGround fertility | 0.8 | Post-fire soil quality. |
| Ash fertility decay rate | -0.005 per 400 ticks | How fast ash benefit fades. |
| AshGround->Sapling regrowth chance | 0.08 | Fire-cleared land recovery speed. |
| Entity fire damage (per tick) | 2 hunger | Danger level for entities in fire. |

## Emergent Behaviors

These are outcomes we expect from the system interactions but do NOT hardcode:

- **Slash-and-burn farming.** The player notices ash ground makes great farmland. They deliberately let fires burn toward desired farm areas instead of building firebreaks. Risk-reward: the fire might reach the settlement.

- **River settlement advantage.** Settlements behind rivers are naturally fire-safe. River placement becomes even more strategically important. A player on a plains map with no nearby river faces higher fire risk.

- **Deforestation as fire prevention.** Clearing a ring of forest around the settlement (creating Bare/Stump tiles from deforestation_regrowth.md) creates a firebreak. Deforestation goes from purely extractive to strategically defensive.

- **Fire-driven exploration.** A massive burn in one direction pushes villagers to explore other directions for wood. The forest they relied on is gone — now they must find new sources.

- **Seasonal anxiety.** As summer approaches and the moisture overlay shows drying forests, the player feels tension: "will there be a fire this year?" The season change becomes meaningful.

- **Animal displacement.** Prey and predators flee fire too. A large forest fire pushes wolves toward the settlement — fire indirectly causes a wolf raid. Two simple systems (fire + wolf AI) create a compound event we never scripted.

- **Post-fire land rush.** Multiple farms get placed on fresh ash ground, exploiting the fertility bonus. But the fertility decays — the player must time the planting.

## Risks and Mitigations

**Risk: Fire destroys everything, game feels unfair.**
Mitigation: Fire only starts in specific conditions (dry + summer + forest). The player has multiple seasons of warning as moisture drops. Rivers block spread. Building walls as firebreaks is an explicit defensive option. Ignition probability is low enough that most summers only produce 0-2 fires, and many burn harmlessly in distant forest.

**Risk: Fire performance — spreading fire scans all neighbors every tick.**
Mitigation: Fire front tracking means we only process active burning tiles, not the whole map. A large fire of 200 burning tiles = 200 * 8 = 1600 neighbor checks per tick. At a simple comparison + random roll per check, this is sub-millisecond. The fire front shrinks as tiles finish burning, so cost is self-limiting.

**Risk: Fire trivializes deforestation (why chop when fire clears for free?).**
Mitigation: Fire is uncontrollable and unpredictable. You can't direct it to clear exactly where you want. It may burn your settlement. Villager wood-chopping is precise and safe; fire is cheap but dangerous. Both have a role.

**Risk: Players don't understand fire mechanics.**
Mitigation: Observable simulation (Pillar 4). The player watches moisture drop on the overlay, sees lightning strike, watches fire spread directionally with wind, sees it stop at the river. Each mechanic is visible. The moisture overlay becomes a "fire risk" indicator without us labeling it as such.

**Risk: Wind system adds complexity to moisture propagation.**
Mitigation: Wind for fire spread is independent of the existing moisture wind direction. The `WindState` struct is used only by `FireMap` in v1. If we later want wind to affect moisture propagation, rain shadow, or windmill placement, the system is ready, but we don't couple them prematurely.

## Out of Scope (Future)

- **Firefighting behavior** -- villagers with water buckets, well building, bucket brigade.
- **Controlled burns** -- player designates an area to burn intentionally.
- **Fire-adapted biomes** -- Scrubland/Savanna that depends on periodic fire for ecological health.
- **Smoke effects on villagers** -- reduced visibility, coughing/slowing near fire.
- **Ember spotting** -- burning embers carried by wind ignite tiles 3-5 tiles ahead of the fire front (realistic but complex).
- **Underground fires** -- peat/marsh fires that burn slowly underground and resurface.
- **Fire seasons as climate feature** -- some seeds have fire-prone geography (dry + forested + windy) while others are fire-safe.
- **Charcoal resource** -- scorched wood produces charcoal, a fuel resource for smithies.

## References

- `src/tilemap.rs` -- Terrain enum, movement costs, rendering
- `src/simulation.rs` -- `MoistureMap`, `VegetationMap`, `DayNightCycle`, `SeasonModifiers`, `Season`
- `src/ecs/ai.rs` -- villager flee behavior, sight range logic
- `src/ecs/systems.rs` -- `system_regrowth`, `system_hunger`, `system_ai`
- `src/game/mod.rs` -- `Game::step()`, season/day tick
- `docs/game_design.md` -- Pillar 1 Rich tier: "Forest fire spread in dry seasons"
- `docs/design/deforestation_regrowth.md` -- Stump/Bare/Sapling terrain stages, regrowth system
- `docs/design/simulation_chains.md` -- SoilFertilityMap, drought-through-water chain, erosion-vegetation coupling
