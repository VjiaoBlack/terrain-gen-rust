# Deforestation and Regrowth

**Status:** Proposed
**Pillars:** Geography Shapes Everything (#1), Emergent Complexity (#2), Explore/Expand/Exploit/Endure (#3), Observable Simulation (#4)
**Phase:** 2 (Economy Depth)
**Last updated:** 2026-04-01

## Problem

Wood gathering currently has no visible impact on the world. A villager stands on a Forest tile, ticks down a 90-frame timer, and walks back with wood. The Forest tile remains unchanged. You can strip-mine the same 3x3 forest patch for the entire game without consequence. This violates Pillar 1 (terrain should change from activity), Pillar 3 (no depletion pressure to expand), and Pillar 4 (can't see what's happening).

### What's broken today

1. **No terrain change on harvest.** `ai_villager` finds the nearest `Terrain::Forest` tile, enters `BehaviorState::Gathering { timer: 90, resource_type: Wood }`, and hauls to stockpile. The tile never changes. (`src/ecs/ai.rs` ~line 1300-1360)

2. **No depletion.** Forest is infinite. A single forest tile yields unlimited wood forever. Stone deposits have `ResourceYield { remaining }` and deplete; wood has no equivalent.

3. **Regrowth is disconnected.** `system_regrowth` in `src/ecs/systems.rs` only spawns berry bushes near forest. The `VegetationMap` in `src/simulation.rs` grows/decays based on moisture but never converts Grass back to Forest or Forest to Grass. There is no terrain-level forest recovery.

4. **No expansion pressure.** Because wood is infinite, there's no reason to explore for new forests. The Exploit->Endure transition never triggers for wood.

## Design

### Core Idea

When a villager finishes gathering wood from a Forest tile, that tile degrades through visible stages: **Forest -> Stump -> Bare Ground**. Recovery follows the reverse path at forest edges: **Bare Ground -> Sapling -> Forest**. Regrowth is slow (game-years, not game-days) and only happens adjacent to existing forest, creating a visible frontier that expands outward from surviving trees.

### Terrain Stages

Add two new `Terrain` variants:

| Stage | Terrain Variant | Glyph | FG Color | BG Color | Speed Mult | A* Cost | Description |
|-------|----------------|-------|----------|----------|------------|---------|-------------|
| Forest | `Forest` (existing) | `:` | (15,80,20) | (10,60,15) | 0.6 | 1.7 | Dense trees, harvestable |
| Stump | `Stump` (new) | `%` | (100,80,40) | (40,90,30) | 0.8 | 1.2 | Recently cut, dead wood remnants |
| Bare | `Bare` (new) | `.` | (90,80,50) | (55,90,40) | 0.9 | 1.1 | Cleared ground, no vegetation |
| Sapling | `Sapling` (new) | `!` | (30,120,30) | (35,95,30) | 0.7 | 1.4 | Young tree, not yet harvestable |

All four stages are walkable. Saplings slow movement slightly (young brush). Stumps and bare ground are faster than forest -- deforestation makes the area easier to traverse, which is a realistic side effect.

### Harvest: Forest -> Stump

**When:** A villager completes `BehaviorState::Gathering { resource_type: Wood }` (timer reaches 0).

**What happens:**
1. The tile at the villager's position changes from `Terrain::Forest` to `Terrain::Stump`.
2. This is a one-time terrain mutation per gather action.
3. The stump is NOT harvestable for more wood. Villagers seeking wood will skip `Stump` tiles (they only target `Forest`).

**Where to implement:** In `system_ai` (`src/ecs/systems.rs`), when processing AI results. After a `Gathering { resource_type: Wood }` completes and transitions to `Hauling`, mutate the map tile at the villager's position from Forest to Stump. This requires passing `&mut TileMap` into the AI result processing, which already has access to `&self.map` in `Game::step()`.

### Decay: Stump -> Bare Ground

**When:** Stumps naturally decay into bare ground over time.

**Timing:** Each stump has a ~600 tick lifetime (roughly 2 in-game days). Implemented as a tick counter on the stump or via the periodic regrowth system check.

**Implementation option A (simple, preferred):** In `system_regrowth`, scan for Stump tiles and convert to Bare with a probability. Every 400 ticks (existing regrowth cadence), each Stump has a 30% chance of becoming Bare. Expected conversion time: ~1200 ticks (~4 days). No new components needed.

**Implementation option B (precise):** Add a `StumpAge` component or per-tile metadata. More complex, probably not worth it for v1.

### Regrowth: Bare -> Sapling -> Forest

This is the slow, visible recovery that creates the deforestation scar.

**Bare -> Sapling:**
- Checked every 400 ticks (same cadence as `system_regrowth`).
- A Bare tile becomes Sapling ONLY if it has at least one orthogonal neighbor that is `Forest` or `Sapling`. Regrowth spreads from edges, not from nothing.
- Probability per check: **5%** when adjacent to Forest, **2%** when adjacent to Sapling only.
- Expected time: ~8000 ticks to start sprouting (~27 in-game days, roughly one season).
- `VegetationMap` value at the tile should be > 0.2 (needs moisture). Bare ground in a desert stays bare.

**Sapling -> Forest:**
- Checked every 400 ticks.
- Probability per check: **3%**.
- Expected time: ~13,000 ticks from sapling to forest (~43 days, roughly one more season).
- Saplings are NOT harvestable. Villagers cannot gather wood from saplings. This prevents villagers from killing regrowth.

**Total recovery time (Stump -> Bare -> Sapling -> Forest):** Approximately 1,200 + 8,000 + 13,000 = **~22,000 ticks** (~73 in-game days, roughly **2 game-years** assuming 4 seasons of ~10 days each). This is slow enough that clear-cutting a forest creates a visible scar lasting multiple seasons, but fast enough that the player can watch recovery happen during a full playthrough.

### Regrowth Interaction with VegetationMap

The existing `VegetationMap` in `src/simulation.rs` tracks moisture-driven vegetation density. Tie regrowth into it:

- Bare/Stump tiles should have their vegetation value set to 0.0 when created.
- Sapling tiles grow vegetation naturally (they are plants).
- Regrowth probability should be gated on `vegetation.get(x, y) > 0.2`. In dry areas without moisture, forests don't regrow. This creates permanent deforestation in arid regions -- a meaningful geographic consequence.
- When a Sapling becomes Forest, vegetation value should be set to at least 0.5.

### Harvest Yield

Currently gathering wood gives 1 wood per gather action (90-tick timer). This stays the same. The change is that the tile is consumed, so the *effective yield per tile* is 1 wood. To balance this:

- **Option A (recommended for v1):** Keep 1 wood per Forest tile. Forests are large enough that this creates natural pacing. If wood feels too scarce, tune by reducing gather timer or increasing yield to 2.
- **Option B (future):** Add a `TreeDensity` per-tile value (e.g., 3 harvests before the tile becomes a stump). More realistic but adds complexity. Defer to v2.

### AI Behavior Changes

Minimal changes to `ai_villager` in `src/ecs/ai.rs`:

1. `find_nearest_terrain(pos, map, Terrain::Forest, sight_range)` already only targets `Forest` tiles. No change needed -- Stump, Bare, and Sapling are not `Forest`, so villagers naturally skip them.

2. **Wood scarcity triggers exploration.** The existing logic at ~line 1395 already sends villagers to explore frontier tiles when `stockpile_wood < 10`. As nearby forests thin, villagers will walk further for wood, creating longer haul trips. This is the emergent behavior we want: deforestation makes wood gathering visibly slower, pushing expansion.

3. **Future: Lumber Mill building.** A building placed adjacent to Forest that provides sustainable wood income (1 wood per N ticks) without consuming the forest tile. This becomes the late-game answer to deforestation. Out of scope for this design doc.

### Visual Storytelling

This system should create a readable history on the map (Pillar 4):

- **Early game:** Dense forest surrounds the settlement. Villagers chop nearby trees.
- **Mid game:** A ring of stumps and bare ground around the settlement, with forest pushed back. Saplings visible at the edge of cleared areas. Villagers walk further for wood.
- **Late game:** Large deforested zone around the settlement. Distant forests being harvested. Old cleared areas show saplings recovering into new forest. The map tells the story of where the settlement consumed and where the land is healing.

In Map mode (Mode A): `:`(forest) -> `%`(stump) -> `.`(bare) -> `!`(sapling) -> `:`(forest). Each glyph is distinct.

In Landscape mode (Mode B): Color gradient from dark green (forest) through brown (stump/bare) back to light green (sapling). The deforestation scar is visible as a brown patch that slowly greens from the edges inward.

## Implementation Plan

### Step 1: New Terrain Variants
- Add `Terrain::Stump`, `Terrain::Bare`, `Terrain::Sapling` to `src/tilemap.rs`.
- Define `ch()`, `fg()`, `bg()`, `speed_multiplier()`, `move_cost()`, `is_walkable()` for each (all walkable).
- Add serialization support (they're enum variants, serde derives handle this).

### Step 2: Harvest Mutation
- In `src/ecs/systems.rs`, `system_ai` result processing: when a villager deposits `ResourceType::Wood`, check the tile they gathered from and convert `Forest` -> `Stump`.
- This requires tracking *where* the villager was gathering. Option: store gather position in `BehaviorState::Gathering` or infer from position at haul start. Storing it explicitly is cleaner -- add `gather_x: f64, gather_y: f64` fields to `Gathering` variant.
- Alternatively, convert the tile at the villager's current position when `Gathering` timer hits 0 (before they start hauling). Simpler, avoids new fields. **Prefer this approach.**

### Step 3: Regrowth System
- Extend `system_regrowth` in `src/ecs/systems.rs`:
  - Stump -> Bare: 30% chance per 400-tick check.
  - Bare -> Sapling: 5% chance per check if adjacent to Forest, 2% if adjacent to Sapling, 0% otherwise. Gate on vegetation > 0.2.
  - Sapling -> Forest: 3% chance per check.
- Regrowth scan: sample N random tiles per check (like existing berry bush logic), don't iterate full map. N=20 is probably enough for 256x256 maps. Scale with map size if needed.

### Step 4: Vegetation Integration
- In `MoistureMap::update` (`src/simulation.rs`), treat Stump/Bare like Grass for vegetation growth rules. Sapling should grow vegetation faster than bare ground.
- Gate Bare->Sapling on `VegetationMap` value.

### Step 5: Tests
- Unit test: gathering wood converts Forest to Stump.
- Unit test: Stump decays to Bare over time.
- Unit test: Bare adjacent to Forest can become Sapling.
- Unit test: Sapling with no forest neighbor does NOT regrow (isolated stumps stay bare).
- Unit test: Sapling converts to Forest over time.
- Unit test: Bare tile in low-moisture area does not sprout.
- Integration test: run 30K ticks, verify deforested area near settlement, regrowth at edges.

## Tuning Knobs

| Parameter | Default | Effect |
|-----------|---------|--------|
| Stump->Bare probability (per 400 ticks) | 30% | How fast stumps disappear. Higher = faster visual cleanup. |
| Bare->Sapling probability (adj. to Forest) | 5% | How fast regrowth starts. Lower = longer deforestation scars. |
| Bare->Sapling probability (adj. to Sapling) | 2% | How fast regrowth cascades. Controls spread speed. |
| Sapling->Forest probability | 3% | How fast saplings mature. Lower = longer sapling phase. |
| Regrowth moisture threshold | 0.2 | Minimum vegetation density for regrowth. Higher = more geographic restriction. |
| Regrowth check interval | 400 ticks | How often regrowth runs. Shared with berry bush system. |
| Wood yield per Forest tile | 1 | Economic balance. Increase if wood is too scarce. |
| Random tiles sampled per check | 20 | Performance vs regrowth smoothness. |

## Risks and Mitigations

**Risk: Wood becomes too scarce early game.**
Mitigation: Initial forest patches are large (terrain gen produces substantial forest belts). At 1 wood per tile, a 10x10 forest patch = 100 wood, enough for many buildings. Monitor in playtesting. Tune yield or gather timer if needed.

**Risk: Regrowth scan is expensive on large maps.**
Mitigation: Sample random tiles, don't iterate full map. 20 random checks per 400 ticks is negligible. If needed, limit scan to tiles within settlement influence radius.

**Risk: Saplings block regrowth chain.**
Mitigation: Saplings count as regrowth sources (Bare->Sapling triggers on adjacent Sapling too), so the frontier propagates through sapling zones. The 2% vs 5% rate difference means forest-adjacent regrowth is faster, creating a natural gradient.

**Risk: Villagers get stuck with no wood.**
Mitigation: Existing exploration logic already sends villagers to frontier tiles when wood is low. Forests exist across the map from terrain generation. The pressure to expand IS the feature. If truly stuck, the future Lumber Mill building provides sustainable wood.

## Out of Scope (Future)

- **Lumber Mill building** -- sustainable wood from adjacent forest without consuming tiles.
- **Tree density per tile** -- multiple harvests before stump. Adds realism but complexity.
- **Fire clearing** -- forest fires from dry seasons that create large bare patches quickly.
- **Replanting** -- player-directed reforestation via a building or designation.
- **Old growth vs new growth** -- different wood yield from ancient forest vs regrown forest.
- **Animal habitat** -- deforestation drives prey/predators to relocate (but sets up Pillar 2 emergence nicely).

## References

- `src/tilemap.rs` -- Terrain enum, movement costs, rendering
- `src/ecs/ai.rs` -- `ai_villager`, `find_nearest_terrain`, wood gathering logic (~line 1294-1360)
- `src/ecs/systems.rs` -- `system_regrowth` (line 461), `system_ai` result processing
- `src/simulation.rs` -- `VegetationMap`, `MoistureMap::update` vegetation step
- `src/game/mod.rs` -- `Game::step()` calls `system_regrowth` at line 1420
- `docs/game_design.md` -- Pillar 1 (terrain changes from activity), Pillar 3 (depletion drives expansion)
- `docs/economy_design.md` -- Wood rework section (line 54-57) outlines this feature
