# Feature: Resources Distributed by Geography

Pillar: Geography Shapes Everything (#1), Explore > Expand > Exploit > Endure (#3)
Priority: Core

## What

A precomputed **resource map** generated at world-gen (Stage 8 of the terrain pipeline) that places resource deposits based on geological rules. Stone deposits cluster near mountains and cliffs. Wood abundance tracks forest density and canopy age. Fertile soil (already computed as `SoilType::Alluvial`) concentrates near rivers. Berry bushes appear in temperate forests with adequate moisture. Iron veins occur deep in mountain ranges. Each tile gets a `ResourcePotential` describing what can be extracted there and in what quantity, before a single villager spawns.

Different seeds produce fundamentally different resource landscapes: a coastal seed has abundant fish and driftwood but scarce stone; a mountain valley seed is stone-rich but farmland-poor; a river delta seed has the best soil but floods.

## Why

Today, resource placement is ad-hoc. `Game::new_with_size()` in `src/game/mod.rs` spawns 4 berry bushes at hardcoded offsets from the settlement center (lines ~707-710) and 2 stone deposits at fixed positions (lines ~714-716). The auto-build system in `src/game/build.rs` spawns additional stone deposits reactively when villagers need them (line ~411, ~660). Wood is "gathered" by walking to any `Terrain::Forest` tile — there is no wood deposit entity. None of this is geography-aware.

This means:
- Seed 42 and seed 137 have the same resource layout relative to spawn, just in different terrain.
- There is no reason to expand in a particular direction. No "the mountains to the east have stone."
- Resource scarcity is artificial (fixed spawn counts) rather than geological.
- The Explore phase from Pillar 3 has nothing to discover — resources appear near you automatically.

A precomputed resource map makes geography the driver of settlement strategy, creates genuine exploration value, and ensures every seed tells a different story.

## Current State

**What exists and can be reused:**

- **`PipelineResult`** (`src/terrain_pipeline.rs:91`): Already outputs `heights`, `moisture`, `temperature`, `soil`, `river_mask`, `slope` — all the geological data needed to derive resource placement. This is the natural home for a resource map.
- **`SoilType`** (`src/terrain_pipeline.rs:62`): Six soil types with `yield_multiplier()`. `Alluvial` (near rivers, 1.25x) and `Loam` (default, 1.0x) already encode fertility. Ready to use.
- **`classify_biome()`** (`src/terrain_pipeline.rs:673`): Biome assignment uses height, temperature, moisture, and slope. Resource rules can reference these same inputs.
- **`Terrain` enum** (`src/tilemap.rs:6`): 14 terrain types including `Mountain`, `Forest`, `Cliff`, `Marsh`. Resource affinity maps directly to these.
- **`ResourceYield`** component (`src/ecs/components.rs:490`): Tracks `remaining`/`max` for harvestable entities. Already used by `StoneDeposit` and `FoodSource` (berry bushes).
- **`StoneDeposit`**, **`FoodSource`** marker components (`src/ecs/components.rs`): Existing resource entity markers queried by AI in `src/ecs/systems.rs`.
- **`spawn_stone_deposit()`**, **`spawn_berry_bush()`** (`src/ecs/spawn.rs:102`, `76`): Existing spawn helpers. Currently called with fixed positions; will be called from the resource map instead.
- **`SettlementKnowledge`** (`src/game/mod.rs:181`): Tracks `known_wood`, `known_stone`, `known_food` as `Vec<(usize, usize)>`. This is the discovery layer — villagers learn resource locations over time. The resource map is the ground truth (Layer 4 in Pillar 2's knowledge architecture); `SettlementKnowledge` is what villagers actually know.
- **`ExplorationMap`** (`src/simulation.rs`): Tracks which tiles have been visited. Bridges resource map (ground truth) and settlement knowledge (discovered).
- **`flow_accumulation`** (`src/terrain_pipeline.rs:288`): Computed during hydrology. Higher accumulation = larger drainage basin = richer alluvial deposits. Available to resource placement logic.

**What does NOT exist:**

- No `ResourceMap` or `ResourcePotential` data structure.
- No pipeline stage that converts geology into resource density.
- No wood deposit entity — wood is implicitly "any Forest tile."
- No concept of resource richness/quality (a mountain tile is just a mountain tile).
- No iron, clay pits, or other resource types beyond food/wood/stone.

## Design

### Data Structures

```rust
// In src/terrain_pipeline.rs

/// What resources a tile can yield and how much.
/// Precomputed at world-gen, never modified by gameplay
/// (gameplay depletes entities spawned FROM this map, not the map itself).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ResourcePotential {
    pub stone: u8,    // 0-255, density score
    pub wood: u8,     // 0-255, timber quality/density
    pub fertility: u8, // 0-255, farming potential
    pub food: u8,     // 0-255, forageable food (berries, game)
    pub iron: u8,     // 0-255, ore density (future: used by advanced smithy)
    pub clay: u8,     // 0-255, pottery/brick material (future)
}

/// The full resource map: one ResourcePotential per tile.
/// Stored as a flat Vec matching the tilemap layout (row-major, w*h).
pub struct ResourceMap {
    pub width: usize,
    pub height: usize,
    pub data: Vec<ResourcePotential>,
}

impl ResourceMap {
    pub fn get(&self, x: usize, y: usize) -> &ResourcePotential {
        &self.data[y * self.width + x]
    }

    /// Returns all tiles where the given resource exceeds a threshold,
    /// sorted by density descending. Used by settlement placement and
    /// exploration scoring.
    pub fn find_deposits(&self, resource: ResourceType, min_density: u8)
        -> Vec<(usize, usize, u8)>;

    /// Sum of a resource within a radius. Used to evaluate
    /// "how much stone is near this mountain range?"
    pub fn density_in_radius(&self, x: usize, y: usize, radius: usize,
        resource: ResourceType) -> u32;
}
```

Add `resource_map: ResourceMap` to `PipelineResult`. Add `resource_map: ResourceMap` to the `Game` struct (alongside existing `soil`, `river_mask`, `heights`).

### Algorithm

**Stage 8: Resource Distribution** — runs after soil assignment (Stage 7), using all prior pipeline outputs.

```
for each tile (x, y):
    let terrain = map.get(x, y)
    let h = heights[i], m = moisture[i], t = temperature[i]
    let s = slope[i], soil = soil[i]

    // === STONE ===
    // High near mountains, cliffs, and rocky terrain.
    // Concentrated in veins using a separate Perlin noise layer (seed + 5000).
    stone_base = match terrain:
        Mountain => 180
        Cliff    => 200
        _        => 0
    // Rocky soil contributes even on non-mountain terrain (foothills)
    if soil == Rocky: stone_base = max(stone_base, 100)
    // Vein noise: creates clusters rather than uniform blankets
    stone_noise = perlin(x * 0.08, y * 0.08, seed + 5000)  // [-1, 1]
    stone_vein = clamp((stone_noise + 0.3) * 200, 0, 255)   // biased positive
    stone = min(stone_base * stone_vein / 255, 255)
    // Slope bonus: steeper = more exposed rock
    stone += clamp(slope * 400, 0, 50)

    // === WOOD ===
    // Only in Forest tiles. Denser forests (higher moisture) have more timber.
    wood = match terrain:
        Forest => clamp(100 + moisture * 150, 0, 255)
        Scrubland => 30  // sparse scrub wood
        _ => 0
    // Temperature penalty: cold forests grow slowly, less wood
    if temp < 0.3: wood *= temp / 0.3

    // === FERTILITY ===
    // Driven by soil type, moisture, and flatness.
    fert_base = soil.yield_multiplier() * 200  // Alluvial=250, Loam=200, Rocky=80
    // Flat + wet = best farmland
    slope_penalty = clamp(slope * 1000, 0, 150)
    fert = clamp(fert_base - slope_penalty + moisture * 50, 0, 255)
    // Near rivers: bonus from alluvial deposits (already in soil type,
    // but add extra for "irrigation proximity")
    if dist_to_river[i] < 6: fert += (6 - dist_to_river) * 8

    // === FOOD (forageable) ===
    // Berry bushes in temperate forests, game animals near water.
    food = 0
    if terrain == Forest && temp > 0.3 && temp < 0.8:
        food = clamp(moisture * 180, 0, 200)
    if terrain == Grass && moisture > 0.4:
        food = clamp(moisture * 80, 0, 100)  // meadow foraging
    // Noise variation so bushes cluster naturally
    food_noise = perlin(x * 0.12, y * 0.12, seed + 6000)
    food = food * clamp((food_noise + 1.0) / 2.0, 0.2, 1.0)

    // === IRON ===
    // Deep mountain only. Rare. Uses its own noise layer for veins.
    iron = 0
    if terrain == Mountain && height > 0.85:
        iron_noise = perlin(x * 0.05, y * 0.05, seed + 7000)
        if iron_noise > 0.4:  // only in positive noise peaks
            iron = clamp((iron_noise - 0.4) * 400, 0, 255)

    // === CLAY ===
    // River banks and marshes.
    clay = 0
    if soil == Clay: clay = 120
    if terrain == Marsh: clay = max(clay, 80)
    if dist_to_river[i] < 3 && slope < 0.03: clay = max(clay, 150)
```

**Entity spawning from the resource map** — replaces the current hardcoded spawns in `Game::new_with_size()`:

```
/// Spawn resource deposit entities from the precomputed resource map.
/// Called once at world-gen. Only spawns entities for high-density tiles
/// to keep entity count manageable.
fn spawn_resources_from_map(
    world: &mut World,
    resource_map: &ResourceMap,
    map: &TileMap,
) {
    for y in 0..resource_map.height {
        for x in 0..resource_map.width {
            let rp = resource_map.get(x, y);
            let terrain = map.get(x, y);

            // Stone deposits: spawn entity if density > 150
            if rp.stone > 150 {
                // Yield scales with density: 10-40 units
                let yield_amount = 10 + (rp.stone as u32 - 150) * 30 / 105;
                spawn_stone_deposit_with_yield(world, x as f64, y as f64, yield_amount);
            }

            // Berry bushes: spawn if food > 120, not on every tile (1-in-4 sampling)
            if rp.food > 120 && (x + y * 7) % 4 == 0 {
                let yield_amount = 8 + (rp.food as u32 - 120) * 20 / 135;
                spawn_berry_bush_with_yield(world, x as f64, y as f64, yield_amount);
            }

            // Wood: Forest tiles ARE the wood source (no entity needed yet).
            // The resource_map.wood value tells villagers HOW MUCH wood
            // a forest tile yields when cut, replacing the current flat rate.
        }
    }
}
```

**Spawn density control:** On a 256x256 map, naive every-tile spawning could create thousands of entities. Controls:

1. **Threshold filtering**: Only tiles above a density threshold spawn entities (stone > 150, food > 120). Most tiles have density 0.
2. **Spatial sampling**: For common resources (berries), sample 1-in-N tiles in high-density regions. This creates natural clusters without flooding the ECS.
3. **Chunk budgets**: Divide the map into 32x32 chunks. Cap entities per chunk (e.g., max 8 stone deposits per chunk). Highest-density tiles win within each chunk.

### Integration Points

**`src/terrain_pipeline.rs`** — New Stage 8:
- Add `compute_resource_map()` function after `assign_soil()`.
- Add `resource_map: ResourceMap` to `PipelineResult`.
- `run_pipeline()` calls `compute_resource_map()` and includes result.
- Needs `dist_to_river` from Stage 7 — refactor `assign_soil()` to return it alongside the soil vec, or compute it once and share.

**`src/game/mod.rs`** — `Game::new_with_size()`:
- Store `resource_map` on `Game` struct (alongside `soil`, `heights`).
- Replace hardcoded berry bush / stone deposit spawns (lines ~707-717) with `spawn_resources_from_map()`.
- Remove the reactive stone deposit spawning in `src/game/build.rs` (lines ~408-413, ~655-663). Resources exist from tick 0; villagers discover them through exploration.

**`src/ecs/spawn.rs`** — New spawn variants:
- `spawn_stone_deposit_with_yield(world, x, y, amount)` — like `spawn_stone_deposit` but with variable `ResourceYield`.
- `spawn_berry_bush_with_yield(world, x, y, amount)` — like `spawn_berry_bush` but with variable `ResourceYield`.
- Future: `spawn_iron_deposit()`, `spawn_clay_pit()`.

**`src/ecs/systems.rs`** — Gathering behavior:
- Wood gathering: instead of "walk to any Forest tile," check `resource_map.wood` at the target tile. Higher wood density = faster gathering / more yield per trip. Depleted forest tiles (after cutting) have their terrain changed to `Grass` with a regrowth timer.
- Stone gathering: villagers seek the nearest `StoneDeposit` entity (unchanged), but now deposits are geographically distributed, so "nearest" actually means something.

**`src/game/build.rs`** — Auto-build and settlement placement:
- Settlement start position scoring: weight by nearby resource diversity. A start near stone AND wood AND fertile soil scores higher than one near only wood. Use `ResourceMap::density_in_radius()`.
- Auto-build farm placement: prefer tiles with high `fertility` in the resource map. Currently farms go wherever there is space; they should go where soil is good.

**`src/ecs/ai.rs`** — Villager AI:
- Exploration priority: when deciding where to explore, prefer directions where the `ExplorationMap` has unexplored tiles AND nearby explored tiles showed high resource potential (extrapolate from gradient).
- Resource seeking: villagers check `SettlementKnowledge` for known deposits, then navigate to the best known one (nearest high-yield), not just the nearest.

**`src/game/render.rs`** — Resource overlay:
- The existing `OverlayMode::Resources` can read the `ResourceMap` to show a heat map of resource density by type, color-coded: grey for stone, green for wood, brown for fertility, red for iron. Currently it just shows entity positions.

**`src/game/save.rs`** — Serialization:
- `ResourceMap` is deterministic from seed, so it does NOT need to be saved. On load, re-run the pipeline (or just Stage 8) to reconstruct it. Add `resource_map` reconstruction to the load path.

## Edge Cases

**Seed produces no mountains (all-water or all-grass map):**
Stone density will be near-zero everywhere. The `ResourceMap::find_deposits()` method returns an empty vec. The settlement must rely on whatever stone exists in rocky foothills/cliffs. This is correct — it's a resource-scarce seed and the player must adapt. Ensure at least 2-3 stone deposits within 80 tiles of spawn (fallback: if `find_deposits(Stone, 100)` returns nothing within 80 tiles of center, lower the threshold to 50 and spawn 2 minimum deposits on the highest-slope tiles).

**Forest-free seed (desert biome dominant):**
Wood density near-zero. Scrubland provides minimal wood (density 30). The settlement must prioritize stone construction and import wood from distant forests. This is a valid hard-mode seed. No artificial wood injection — the difficulty IS the design.

**River-free seed:**
Alluvial soil won't appear (no river = no alluvial classification in Stage 7). Fertility comes from Loam + moisture. Clay is scarce. The settlement relies on rain-fed agriculture. Less productive but playable.

**Entity count explosion:**
A mountain-heavy seed could have thousands of tiles above the stone threshold. The chunk budget system (max 8 per 32x32 chunk) caps this at ~512 stone entities on a 256x256 map. If profiling shows this is too many, raise the threshold or reduce the per-chunk cap.

**Overlapping resource types on one tile:**
A mountain tile near a river could have high stone AND high fertility AND high clay. This is correct and valuable — it's a strategic location worth fighting for. The `ResourcePotential` struct stores all values independently.

**Resource depletion vs. the map:**
The `ResourceMap` is the geological potential, not current availability. When a `StoneDeposit` entity is depleted and despawned, the `ResourceMap` still shows stone potential there. A future mining building could re-open the deposit (spawn a new entity from the map data). The map is permanent; entities are transient.

## Test Criteria

**Unit tests (in `src/terrain_pipeline.rs`):**

1. `resource_map_stone_near_mountains` — Generate a 64x64 map with seed that produces mountains. Assert: tiles classified as `Terrain::Mountain` have `stone > 100`. Tiles classified as `Terrain::Grass` far from mountains have `stone < 30`.

2. `resource_map_wood_in_forests` — Assert: `Terrain::Forest` tiles have `wood > 50`. Non-forest, non-scrubland tiles have `wood == 0`.

3. `resource_map_fertility_near_rivers` — Assert: tiles within 4 of a river with `SoilType::Alluvial` have `fertility > 180`. Tiles with `SoilType::Rocky` have `fertility < 100`.

4. `resource_map_food_in_temperate_forest` — Assert: forest tiles with temperature 0.3-0.8 have `food > 0`. Desert/tundra tiles have `food == 0`.

5. `resource_map_iron_only_high_mountains` — Assert: tiles with `height < 0.85` have `iron == 0`. Some tiles with `height > 0.85` and `Terrain::Mountain` have `iron > 0`.

6. `resource_map_different_seeds_differ` — Run pipeline with seed 42 and seed 137. Compute total stone, total wood, total fertility for each. Assert the distributions differ by at least 20% in at least one resource.

7. `resource_map_dimensions_match_tilemap` — Assert: `resource_map.width == map.width` and `resource_map.height == map.height` and `resource_map.data.len() == w * h`.

**Integration tests (in `tests/integration.rs`):**

8. `stone_deposits_near_mountains_in_game` — Create `Game::new_with_size(60, seed, 64, 64)`. Query all `StoneDeposit` entities. Assert: >80% are on tiles where `terrain == Mountain || terrain == Cliff || soil == Rocky`.

9. `berry_bushes_in_forests_in_game` — Create game. Query all `FoodSource` entities. Assert: >70% are on `Forest` or `Grass` tiles with `moisture > 0.3`.

10. `no_resources_on_water` — Assert: no `StoneDeposit` or `FoodSource` entity is positioned on a `Terrain::Water` tile.

11. `seed_42_vs_137_different_resource_layout` — Create two games with different seeds. Assert: the centroid of stone deposits differs by at least 15 tiles, OR the total count of deposits differs by at least 30%.

**Property tests (optional, with proptest):**

12. For any seed 0-1000: `resource_map` has no `ResourcePotential` with all fields zero on a walkable tile that has `Terrain::Forest` (forests always have some wood).

## Dependencies

- **Requires:** No new crate dependencies. Uses existing `noise::Perlin` (already in `Cargo.toml`) for vein noise layers.
- **Blocked by:** Nothing. The terrain pipeline already outputs all needed geological data.
- **Blocks:**
  - *Exploration as discovery* — villagers need a resource map to discover. Without it, there's nothing hidden to find.
  - *Scarcity-driven expansion* — resources must be geographically distant from spawn to motivate expansion.
  - *Terrain-constrained building* — farm placement near fertile soil requires knowing where fertile soil is.
  - *Resource overlay rendering* — the `OverlayMode::Resources` heatmap needs a `ResourceMap` to read from.

## Estimated Scope

| Task | Effort |
|------|--------|
| `ResourcePotential` + `ResourceMap` structs | Small (1 hr) |
| `compute_resource_map()` — Stage 8 algorithm | Medium (3-4 hr) |
| Wire into `run_pipeline()` and `PipelineResult` | Small (30 min) |
| `spawn_resources_from_map()` + variable-yield spawn helpers | Medium (2 hr) |
| Remove hardcoded spawns from `Game::new_with_size()` | Small (30 min) |
| Remove reactive stone spawning from `build.rs` | Small (30 min) |
| Store `resource_map` on `Game`, skip in save/load | Small (30 min) |
| Wood gathering reads `resource_map.wood` instead of flat rate | Medium (1-2 hr) |
| Auto-build farm placement uses fertility | Small (1 hr) |
| Settlement start position scoring | Medium (1-2 hr) |
| Resource overlay reads `ResourceMap` | Small (1 hr) |
| Unit tests (7 tests) | Medium (2 hr) |
| Integration tests (4 tests) | Medium (2 hr) |
| **Total** | **~15-18 hours** |

Implementation order: structs and algorithm first (testable in isolation), then wire into game, then remove hardcoded spawns, then AI/rendering integration. Each step is independently committable and testable.
