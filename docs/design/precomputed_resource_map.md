# Feature: Precomputed Resource Map
Pillar: Geography Shapes Everything (#1), Explore > Expand > Exploit > Endure (#3)
Priority: Core

## What

A per-tile grid computed at world-gen time that records the ground-truth location, type, and richness of every harvestable resource on the map. Villagers never read this grid directly -- they discover its contents through exploration and share knowledge at the stockpile. The resource map replaces the current ad-hoc stone deposit spawning and implicit "Forest = wood" logic with a unified system where all resources have explicit locations, quantities, and depletion state.

## Why

Right now, wood is gathered from any Forest tile (infinite, no depletion), stone spawns from 2 hardcoded deposits near the stockpile plus periodic emergency respawns (`build.rs:365`), and food comes from pre-placed berry bushes. This creates three problems:

1. **Geography doesn't shape strategy.** Every map plays the same because resources are either everywhere (wood) or spawned on demand (stone). There is no reason to expand in a specific direction.
2. **No discovery arc.** Villagers know everything from tick 0 because Forest tiles are visible and stone deposits appear when needed. The Explore phase of the 4X arc is empty.
3. **No depletion pressure.** Infinite Forest wood means the Exploit-to-Endure transition never triggers from resource scarcity.

A precomputed resource map fixes all three: resources are placed by geology at world-gen, distributed unevenly across the map, and depletable. A settlement near alluvial soil has great farms but may lack stone. A settlement near mountains has stone for days but must trek to the river valley for fertile land. Two seeds produce two fundamentally different games.

## Current State

**Terrain pipeline** (`terrain_pipeline.rs`) already computes per-tile: elevation (`heights`), moisture, temperature, slope, soil type (`SoilType`), river mask, and biome classification (`classify_biome`). All data needed to place resources geologically already exists in `PipelineResult`.

**Resources today:**
- **Wood**: implicit -- villagers call `find_nearest_terrain(Terrain::Forest)` in `ai.rs:887,1305`. No entity, no depletion, no quantity. Forest tiles are the resource.
- **Stone**: 2 `StoneDeposit` entities spawned at fixed offsets from stockpile (`game/mod.rs:714-717`), each with `ResourceYield { remaining: 20, max: 20 }`. Emergency respawn every 2000 ticks when `stone < 20` (`build.rs:644-662`).
- **Food**: 2 `FoodSource` berry bushes spawned near stockpile (`game/mod.rs:704-711`), each with `ResourceYield { remaining: 20, max: 20 }`.
- **Soil fertility**: `SoilType` stored in `Game.soil` and used for `yield_multiplier()` on farms, but not connected to resource placement.

**Settlement knowledge** (`SettlementKnowledge` in `game/mod.rs:180-186`) already has `known_wood`, `known_stone`, `known_food`, `frontier` fields -- but these are currently empty/unused vectors. The structure exists but is not populated.

**Exploration map** (`ExplorationMap` in `simulation.rs:1053-1068`) tracks revealed tiles per `reveal(x, y, radius)` calls. This is the fog-of-war layer that will gate resource discovery.

## Design

### Data Structures

#### New: `ResourceDeposit` (in `terrain_pipeline.rs`)

```rust
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum DepositType {
    Timber,         // harvestable wood, placed on Forest tiles
    StoneVein,      // mineable stone, placed on/near Mountain tiles
    BerryGrove,     // food source, placed in temperate Forest/Grass
    ClayPit,        // clay for future pottery/bricks, placed in Marsh/river-adjacent
    IronOre,        // future: metal, placed in Mountain with high slope
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResourceDeposit {
    pub deposit_type: DepositType,
    pub richness: u16,      // total harvestable units (e.g. 10-200)
    pub remaining: u16,     // current units left (starts == richness)
    pub quality: f32,       // 0.0-1.0, affects yield per harvest (e.g. alluvial stone > desert stone)
}
```

#### New: `ResourceMap` (in `terrain_pipeline.rs`)

```rust
pub struct ResourceMap {
    pub width: usize,
    pub height: usize,
    pub deposits: Vec<Option<ResourceDeposit>>,  // indexed [y * width + x], None = no resource
}
```

`Option<ResourceDeposit>` per tile keeps the structure flat and cache-friendly. Most tiles are `None`. At 256x256 with ~5% coverage, that is roughly 3,200 deposits and 65,536 `Option` slots (about 650KB at 10 bytes per slot -- trivial).

#### Extended: `PipelineResult`

```rust
pub struct PipelineResult {
    pub map: TileMap,
    pub heights: Vec<f64>,
    pub moisture: Vec<f64>,
    pub temperature: Vec<f64>,
    pub soil: Vec<SoilType>,
    pub river_mask: Vec<bool>,
    pub slope: Vec<f64>,
    pub resources: ResourceMap,  // NEW
}
```

#### Extended: `Game` struct (in `game/mod.rs`)

```rust
pub struct Game {
    // ... existing fields ...
    pub resource_map: ResourceMap,  // NEW: ground truth, mutated on depletion
}
```

### Algorithm

Resource placement happens as a new **Stage 8** at the end of `run_pipeline`, after soil assignment (Stage 7). It reads the already-computed terrain, biome, elevation, moisture, soil, slope, and river mask to place deposits geologically.

#### Stage 8: Resource Placement

```
for each tile (x, y):
    terrain = map.get(x, y)
    h = heights[i], m = moisture[i], t = temperature[i]
    s = slope[i], soil = soil[i], river_adj = river_mask within 3 tiles

    // Timber: Forest tiles with enough moisture
    if terrain == Forest:
        richness = base_timber * moisture_bonus(m) * temperature_bonus(t)
        quality = soil.yield_multiplier()
        deposit = Timber { richness, remaining: richness, quality }

    // Stone veins: Mountain tiles, high-slope areas, Rocky soil
    if terrain == Mountain || (slope > 0.10 && soil == Rocky):
        richness = base_stone * elevation_bonus(h) * slope_bonus(s)
        quality = if h > 0.85 { 1.0 } else { 0.6 }  // deep mountain = richer
        deposit = StoneVein { richness, remaining: richness, quality }

    // Berry groves: temperate grass/forest, moderate moisture, not too hot/cold
    if (terrain == Grass || terrain == Forest) && t > 0.3 && t < 0.8 && m > 0.4:
        // Sparse placement: use noise to cluster berries naturally
        if berry_noise(x, y, seed) > 0.7:
            richness = base_berry * moisture_bonus(m)
            deposit = BerryGrove { richness, remaining: richness, quality: 1.0 }

    // Clay pits: Marsh, or river-adjacent with Clay/Alluvial soil
    if terrain == Marsh || (river_adj && matches!(soil, Clay | Alluvial)):
        if clay_noise(x, y, seed) > 0.75:
            deposit = ClayPit { richness: base_clay, remaining: base_clay, quality }

    // Iron ore: deep mountain, high elevation, steep
    if terrain == Mountain && h > 0.85 && slope > 0.08:
        if iron_noise(x, y, seed) > 0.8:
            deposit = IronOre { richness: base_iron, remaining: base_iron, quality }
```

**Noise-based clustering**: Berry, clay, and iron deposits use a secondary Perlin noise layer (seeded from `seed + offset`) with a high threshold to create natural clusters rather than uniform scatter. This makes certain regions resource-rich and others barren -- the geographic asymmetry that Pillar 1 demands.

**Richness tuning constants** (starting values, tuned by playtesting):

| Deposit   | base_richness | typical per tile | tiles per 256x256 map |
|-----------|--------------|------------------|-----------------------|
| Timber    | 30           | 15-45            | ~8,000 (all Forest)   |
| StoneVein | 40           | 20-60            | ~2,000 (Mountain)     |
| BerryGrove| 15           | 10-20            | ~500 (clustered)      |
| ClayPit   | 25           | 15-35            | ~300 (river/marsh)    |
| IronOre   | 50           | 30-70            | ~200 (deep mountain)  |

#### Discovery Flow

Resources exist in `ResourceMap` from tick 0 but are invisible to villagers until explored:

1. Villager moves to tile (x, y).
2. `ExplorationMap::reveal(x, y, sight_range)` marks tiles as revealed (already implemented).
3. **New**: for each newly revealed tile, if `resource_map.deposits[i].is_some()`, add the location to `SettlementKnowledge` (the appropriate `known_wood`/`known_stone`/`known_food` vec).
4. Villager AI reads from `SettlementKnowledge` instead of scanning terrain or entity lists.
5. When a villager harvests a deposit, `resource_map.deposits[i].remaining -= 1`. When `remaining == 0`, the deposit is exhausted.

#### Depletion Effects

When a deposit reaches `remaining == 0`:
- **Timber**: Forest tile converts to Grass (deforestation visible on map). Slow regrowth possible via `VegetationMap` (existing system).
- **StoneVein**: Mountain tile remains but resource marker disappears. Leaves a visual "quarry pit" (could convert to a Scrubland tile or add a quarry marker later).
- **BerryGrove**: `FoodSource` entity removed. Bush gone.
- **ClayPit**: Marsh remains, resource gone.
- **IronOre**: Same as stone -- mountain stays, ore gone.

### Integration Points

| System | Change | File |
|--------|--------|------|
| `run_pipeline()` | Add Stage 8: resource placement after soil | `terrain_pipeline.rs` |
| `PipelineResult` | Add `resources: ResourceMap` field | `terrain_pipeline.rs` |
| `Game::new()` | Store `resource_map` from pipeline result. Remove hardcoded stone/berry spawning (`mod.rs:704-717`). Seed initial `SettlementKnowledge` from tiles revealed at spawn. | `game/mod.rs` |
| `Game.resource_map` | New field on `Game` struct | `game/mod.rs` |
| `SettlementKnowledge` | Populated from exploration events, read by AI | `game/mod.rs` |
| `ai_villager()` | Replace `find_nearest_terrain(Forest)` with lookup in `SettlementKnowledge.known_wood`. Replace stone deposit entity scan with `known_stone`. | `ecs/ai.rs` |
| `system_ai()` | Pass `&SettlementKnowledge` into villager AI (currently passes `stone_deposit_positions` vec already at `systems.rs:149`) | `ecs/systems.rs` |
| `ExplorationMap::reveal()` | After revealing tiles, scan revealed area for deposits and update `SettlementKnowledge` | `simulation.rs` or `game/mod.rs` step |
| `build.rs` auto-build | Remove `discover_stone_deposits()` emergency spawner (`build.rs:365-413`). Stone now comes from the map. | `game/build.rs` |
| `build.rs` auto-build | Remove `discover_timber_grove()` emergency spawner (`build.rs:423-498`). Wood now comes from the map. | `game/build.rs` |
| Overlay rendering | Resources overlay (`OverlayMode::Resources`) reads `resource_map` for revealed deposits, color-coded by type | `game/render.rs` |
| Save/Load | Serialize `ResourceMap` in `SaveState` | `game/save.rs` |
| Serialization | `ResourceDeposit` and `ResourceMap` need Serialize/Deserialize | `terrain_pipeline.rs` |

## Edge Cases

**Spawn location has no nearby resources.** The start-position search (`game/mod.rs:398-470`) currently looks for grass near forest. It should additionally verify that the `ResourceMap` has at least N timber deposits and M stone deposits within a radius (e.g. 30 tiles). Otherwise the settlement is doomed from tick 0.

**All nearby timber depleted.** Without emergency spawning, a deforested settlement stalls. Mitigation: timber deposits on Forest tiles should have enough richness that full depletion takes thousands of ticks. Additionally, `VegetationMap` regrowth (existing but slow) can eventually restore Forest tiles, which Stage 8 logic could re-seed with smaller timber deposits. For v1, accept that deforestation is a real constraint -- this IS the game (Endure phase).

**Stone deposits too far from spawn.** Mountains might be 80+ tiles away on flat maps. The pipeline should guarantee at least 2-3 small stone deposits on Scrubland or Rocky-soil tiles near the map center, even if no mountain is nearby. Use a "guaranteed minimum" pass after the noise-based placement.

**Map seeds with very few mountains.** Some seeds produce mostly grassland/forest. Reduce stone density but ensure minimum viable stone within reasonable range of any spawn candidate.

**Exploration reveals thousands of deposits at once.** If a villager with sight_range 22 reveals a forest, that could add hundreds of timber entries to `SettlementKnowledge`. Optimization: `known_wood` etc. should store only the closest N deposits, or use a spatial structure. For v1, a simple Vec with periodic cleanup (remove depleted entries) is fine -- 500 entries is trivial.

**Existing `StoneDeposit` ECS entity pattern.** The current system spawns stone as ECS entities with `ResourceYield`. The resource map replaces this as ground truth, but we still need entities for rendering and villager interaction. Option A: spawn entities lazily when a deposit is discovered. Option B: keep resource map as pure data and have villagers interact with it directly (no entity). **Recommendation: Option B for v1.** Eliminates entity bloat from thousands of deposits. Villagers gather from map coordinates, not entities. Rendering reads the resource map directly for the overlay.

**Berry bushes vs BerryGrove deposits.** Currently berry bushes are `FoodSource` entities that prey also eat from. The resource map's `BerryGrove` deposits should spawn `FoodSource` entities when discovered (or at world-gen for prey access). Prey need food regardless of villager exploration. **Decision: BerryGrove deposits that are near dens (within 15 tiles) should spawn FoodSource entities at world-gen. Others spawn on discovery.** This preserves the prey ecosystem while gating villager knowledge.

## Test Criteria

1. **Resource placement correlates with terrain.** Generate 10 seeds. For each: 100% of Timber deposits are on Forest tiles. 100% of StoneVein deposits are on Mountain or Rocky-soil tiles. BerryGrove deposits are only on temperate Grass/Forest.

2. **Geographic asymmetry.** Generate seeds 42 and 137. Count deposits by type within 30 tiles of map center. The distributions should differ meaningfully (different dominant resource type, or >30% difference in total richness).

3. **Discovery gating works.** Create a game, check `SettlementKnowledge` is initially populated only from tiles within sight range of spawn. Advance 500 ticks with exploration enabled. `SettlementKnowledge` should grow as villagers explore.

4. **Depletion works.** Harvest a Timber deposit to 0. Verify `remaining == 0`, tile converts to Grass, and `SettlementKnowledge` removes or marks the entry as depleted.

5. **No emergency spawning.** Run seed 42 for 10,000 ticks. Verify `discover_stone_deposits()` and `discover_timber_grove()` are not called (or are removed). Stone and wood come exclusively from the resource map.

6. **Minimum viable resources near spawn.** For 20 random seeds: within 30 tiles of spawn, there are at least 3 timber deposits (>= 15 richness each) and 2 stone deposits (>= 20 richness each).

7. **Save/load round-trip.** Save a game at tick 1000 (some deposits partially depleted). Load it. Verify `resource_map` matches: same deposit types, same remaining values, same tile positions.

8. **Performance.** `ResourceMap` construction adds < 5ms to pipeline runtime (currently ~200ms for 256x256). Memory overhead < 1MB.

## Dependencies

- `run_pipeline()` and `PipelineResult` (exists)
- `ExplorationMap` with `reveal()` (exists)
- `SettlementKnowledge` struct (exists, fields defined but unpopulated)
- `SoilType` and `classify_biome()` (exists)
- Perlin noise crate for clustering noise (exists, already used by terrain_gen)

No new crate dependencies. All prerequisite systems exist.

## Estimated Scope

**Medium** (3-5 focused sessions).

- Stage 8 placement algorithm + `ResourceMap` struct: 1 session
- Wire into `Game::new()`, remove hardcoded spawns, seed knowledge from spawn reveal: 1 session
- Update `ai_villager()` to read from knowledge instead of terrain scan / entity scan: 1 session
- Depletion logic, overlay rendering, save/load: 1 session
- Tests + tuning richness constants across seeds: 1 session

The core data structures are simple. The bulk of the work is replacing the existing resource-finding code paths in `ai.rs` and removing the emergency spawners in `build.rs` without breaking the simulation.
