# Mining Changes Terrain

## Problem

When villagers mine stone from mountains or stone deposits, the terrain is unchanged. A mountain that has been mined for 30K ticks looks identical to one that was never touched. Stone deposits simply vanish when depleted (despawned via `to_deplete_despawn` in `systems.rs`), leaving behind whatever terrain was there before -- usually grass or scrubland. There is no visual history of industrial activity. The map at tick 50K should tell you where the settlement mined, how aggressively, and how long ago.

This directly undermines Pillar 1 ("Geography Shapes Everything", section D: "Activity changes terrain over time") and Pillar 3 ("Explore -> Expand -> Exploit -> Endure", where depletion should be visible and not just frustrating).

## Current Behavior

1. **Stone deposits** (`StoneDeposit` + `ResourceYield { remaining: 20, max: 20 }`) are entities spawned on the map. Villagers seek them, enter `Gathering { timer: 90, resource_type: Stone }`, then haul stone to the stockpile. Each successful haul decrements `remaining` by 1. When `remaining` hits 0, the entity is despawned. The tile underneath is untouched.

2. **Mountain mining** works differently. When no stone deposits are visible, villagers seek `Terrain::Mountain` tiles directly via `find_nearest_terrain()`. They stand adjacent to the mountain (the pathfinder routes to a walkable neighbor) and gather. This also produces stone, but the mountain tile itself never changes. A mountain mined 100 times looks the same as tick 0.

3. **Wood gathering** (for contrast) targets `Terrain::Forest` tiles. There is currently no terrain change when wood is gathered either -- but forest regrowth is mentioned in `economy_design.md` as a future feature. Mining terrain change should ship alongside or before forest terrain change since stone is the scarcer resource and mining is the more dramatic visual transformation.

## Design

### New Terrain Types

Add three new variants to the `Terrain` enum in `tilemap.rs`:

| Terrain | Char | FG Color | BG Color | Speed | A* Cost | Walkable |
|---------|------|----------|----------|-------|---------|----------|
| `Quarry` | `U` | `(140, 130, 115)` | `(90, 80, 70)` | 0.7x | 1.4 | yes |
| `QuarryDeep` | `V` | `(110, 100, 90)` | `(65, 58, 50)` | 0.5x | 2.0 | yes |
| `ScarredGround` | `.` | `(145, 135, 120)` | `(115, 105, 90)` | 0.9x | 1.1 | yes |

**Visual rationale:**
- `Quarry` uses `U` -- a bowl shape suggesting a shallow pit. Muted warm grey, lighter than mountain, darker than sand. Walkable but slow.
- `QuarryDeep` uses `V` -- deeper pit. Darker, slower. The progression `^` -> `U` -> `V` visually reads as "mountain carved down."
- `ScarredGround` uses `.` -- minimal, barren. The lightest of the three. Where a stone deposit was fully mined out. Distinct from grass (`'`), sand (`*`), and desert (`.` but different color). Slightly warm/grey tint vs desert's yellow.

**Map mode (Pillar 4):** The glyphs are distinct and readable. A cluster of `U` and `V` tiles surrounded by `^` mountains instantly reads as "quarry." Scattered `.` tiles where `*` deposits used to be read as "mined out."

**Landscape mode (Pillar 4):** The color palette progresses from mountain grey-brown through quarry warm-grey to scarred pale-brown. Under the lighting system, quarry pits will naturally appear darker (lower elevation feel) while scarred ground will be brighter (flat, exposed).

### Terrain Transitions

#### Mountain Mining: `Mountain` -> `Quarry` -> `QuarryDeep`

Each mountain tile tracks how many times it has been mined via a **per-tile mining counter** stored in a parallel grid (see Implementation section). The transitions:

| Mine Count | Terrain | What Happened |
|------------|---------|---------------|
| 0 | `Mountain` | Untouched |
| 1-5 | `Mountain` | Surface scratches, no visible change yet |
| 6 | `Quarry` | Enough stone extracted that the mountainside is carved open |
| 12 | `QuarryDeep` | Major excavation, the mountain is gutted |

**Why thresholds and not immediate?** A single mining trip should not visibly alter a mountain. The change should be gradual -- the player notices "wait, that mountain looks different" after sustained mining. This matches realistic geology (quarries take years) and creates a satisfying slow reveal of industrial impact.

**Adjacency rule:** When a `Mountain` tile transitions to `Quarry`, check its 4-neighbors. If a neighbor is `Mountain` and has mine_count >= 3, it also transitions to `Quarry`. This creates organic quarry shapes that spread along the mountain face rather than isolated single-tile pits. The spread is capped -- only immediate neighbors, only if they have been partially mined.

**Edge case -- last mountain tile:** If mining would convert the last `Mountain` tile in a contiguous mountain region to `Quarry`, do it anyway. Mountains can be fully consumed. That IS the story.

#### Stone Deposit Depletion: Entity Despawn -> `ScarredGround`

When a `StoneDeposit` entity's `ResourceYield.remaining` hits 0:

1. Record the entity's position before despawning.
2. Set the terrain at that position to `ScarredGround`.
3. Despawn the entity (existing behavior).

This is the simplest change -- a one-line terrain set in the existing depletion code path in `system_ai()` (`systems.rs:443`).

**Partial depletion visual:** When `remaining` drops below `max / 2` (i.e., 10 out of 20), change the deposit's sprite from `*` (full) to `*` with a dimmer color -- `Color(120, 110, 100)` instead of `Color(150, 140, 130)`. This gives an intermediate "this deposit is getting thin" signal before it vanishes entirely.

#### Quarry -> ScarredGround (Optional, Long-Term)

After a `QuarryDeep` tile has not been mined for 2000+ ticks, it could transition to `ScarredGround` -- the quarry has been abandoned and the pit floor is just bare rock. This is a "dream" tier feature; the quarry pits themselves are already good visual history.

### Mining Counter Storage

**Option A: Parallel grid (recommended).** Add a `mine_counts: Vec<u8>` field to `TileMap`, same dimensions as the tile grid. Initialized to 0. Incremented when a villager completes a stone-gathering action at a mountain tile. Cheap (1 byte per tile = 64KB for 256x256), fast lookup, serializable.

**Option B: Component on terrain.** Not viable -- terrain tiles are not entities in this architecture.

**Option C: HashMap<(usize, usize), u8>.** Sparse, only stores mined tiles. Better memory for huge maps with little mining. Worse cache performance. Not worth the complexity at 256x256.

Go with Option A. The `mine_counts` grid is zero-initialized and only written to during mining actions, so it has zero cost when mining is not happening.

### Integration with Existing Systems

#### `ai.rs` -- Gathering Completion

When a villager finishes `BehaviorState::Gathering { timer: 0, resource_type: Stone }` and their gather target was a `Terrain::Mountain` tile (not a `StoneDeposit` entity):

1. Identify the mountain tile they were mining (the non-walkable tile adjacent to their position).
2. Increment `mine_counts[tile_index]`.
3. If the count crosses a threshold (6 or 12), call `tilemap.set(x, y, new_terrain)`.

This requires passing a `&mut TileMap` into the gathering completion path, which `system_ai()` already receives.

#### `systems.rs` -- StoneDeposit Depletion

In the existing depletion block (~line 443):

```rust
for e in to_deplete_despawn {
    // NEW: set terrain to ScarredGround at deposit position
    if let Ok(pos) = world.get::<&Position>(e) {
        let tx = pos.x.round() as usize;
        let ty = pos.y.round() as usize;
        map.set(tx, ty, Terrain::ScarredGround);
    }
    let _ = world.despawn(e);
}
```

#### `tilemap.rs` -- New Terrain Variants

Add `Quarry`, `QuarryDeep`, `ScarredGround` to the `Terrain` enum and implement all trait methods (`ch`, `fg`, `bg`, `is_walkable`, `speed_multiplier`, `move_cost`).

Add `mine_counts: Vec<u8>` to `TileMap` with accessor methods:
- `pub fn mine_count(&self, x: usize, y: usize) -> u8`
- `pub fn increment_mine_count(&mut self, x: usize, y: usize) -> u8` (returns new count)

#### `serialize.rs` -- Save/Load

The `mine_counts` grid must be serialized. Add it as an optional field on the `TileMap` serialization (default to zeros for old saves). The three new `Terrain` variants need serde support -- they derive `Serialize, Deserialize` already via the enum, so adding variants is sufficient. Old saves with unknown variants will fail to load, which is acceptable at this stage of development.

#### `terrain_gen.rs` -- World Generation

No changes. Quarries and scarred ground do not exist at world gen. They are created exclusively by villager activity. This is the whole point -- these terrain types are the settlement's industrial fingerprint.

### Pathfinding Implications

- `Quarry` is walkable with cost 1.4 (between grass and forest). Villagers can walk through quarries but prefer roads/grass. This means old quarries become traversable shortcuts through mountain ranges -- an emergent benefit of mining.
- `QuarryDeep` is walkable with cost 2.0 (same as snow). Passable but villagers avoid it. Deep quarry pits are not convenient paths.
- `ScarredGround` is walkable with cost 1.1 (nearly grass). Depleted deposit sites become easy to traverse.

**Emergent behavior:** Mining a path through a mountain range (converting `Mountain` tiles at cost 4.0 into `Quarry` tiles at cost 1.4) creates a pass. The A* pathfinder will naturally route villagers through the quarried gap. This is the kind of terrain-activity feedback loop that Pillar 1 demands.

### Visual Storytelling Examples

**Early game (tick 5K):** A few `ScarredGround` tiles near the settlement where initial stone deposits were mined out. Mountains still pristine. The player sees "we used up the easy stone."

**Mid game (tick 20K):** A cluster of `Quarry` tiles along the eastern mountain face where villagers have been mining for thousands of ticks. A line of `ScarredGround` dots trails between the quarry and the stockpile (where stone deposits were discovered and consumed along the route). The player sees "we're industrializing that mountain."

**Late game (tick 50K):** A deep gouge of `QuarryDeep` and `Quarry` tiles where the mountain used to be solid. The quarry has eaten into the range far enough that a navigable pass exists. `ScarredGround` scatters the landscape around old deposit sites. The player sees "this civilization consumed the mountain."

### Overlay Integration

The existing overlay system (`game/render.rs`) should include mining activity in the resource overlay:
- `Quarry` tiles highlighted with a pick-axe indicator or distinct color in the resource overlay.
- `ScarredGround` shown as "depleted" markers.
- `mine_counts > 0` on mountain tiles shown as a heat gradient (low mining = cool, heavy mining = warm) in a debug/resource overlay.

## Feature Tiers

### Core (this design doc)
- Three new terrain types: `Quarry`, `QuarryDeep`, `ScarredGround`
- Mountain tiles transition based on cumulative mine count
- Stone deposits leave `ScarredGround` on depletion
- `mine_counts` parallel grid on TileMap
- Serialization support

### Rich (future extensions)
- Partial depletion sprite dimming on stone deposits
- Quarry adjacency spread (mined neighbors also transition)
- Mining overlay showing extraction history as heat map
- Quarry tiles produce ambient dust particles in landscape mode
- `ScarredGround` slowly regrows to `Scrubland` after 5000+ ticks without mining (nature reclaims)

### Dream
- Quarry depth as a z-level visual (parallax rendering in Mode B+)
- Quarry flooding: deep quarries adjacent to water sources fill with water over time, creating artificial ponds
- Abandoned quarry repurposing: villagers build workshops or stockpiles inside old quarry pits (sheltered, defensive)
- Geological layers: mining through mountain reveals different stone types (granite surface, marble deeper, ore veins)

## Test Plan

1. **Unit: new terrain properties.** Verify `Quarry`, `QuarryDeep`, `ScarredGround` return correct `ch()`, `fg()`, `bg()`, `is_walkable()`, `speed_multiplier()`, `move_cost()`.

2. **Unit: mine count increment and threshold.** Create a TileMap with a Mountain tile. Increment mine count 6 times, verify terrain transitions to `Quarry`. Increment to 12, verify `QuarryDeep`.

3. **Unit: stone deposit depletion leaves ScarredGround.** Spawn a stone deposit, deplete its `ResourceYield` to 0, run the depletion system, verify the tile at the deposit's position is `ScarredGround`.

4. **Unit: pathfinding through quarry.** Create a map with a mountain wall. Convert one tile to `Quarry`. Verify A* routes through the quarry tile (cost 1.4) rather than the long way around.

5. **Integration: 500-tick mining session.** Run 500 ticks with villagers mining a mountain. Verify at least one mountain tile has transitioned to `Quarry`. Verify mine_counts are non-zero.

6. **Integration: serialization round-trip.** Save a game with quarry tiles and mine_counts. Load it. Verify terrain types and mine_counts are preserved.

7. **Visual: screenshot comparison.** Take a screenshot at tick 0 and tick 30K of the same seed. The mountain region should look visibly different. (Manual verification, not automated.)

## Open Questions

- **Should quarry tiles affect the influence/territory map?** Quarries could extend settlement influence into mountain areas, reflecting industrial claim on the land.
- **Should villagers prefer quarry-adjacent mountain tiles?** Mining next to an existing quarry (where infrastructure/paths already exist) is more realistic than mining a random mountain tile. This would create natural quarry expansion patterns.
- **Mine count cap?** Should `mine_counts` saturate at 255 (u8 max) or should we use u16? At one increment per gather cycle (~90 ticks), 255 harvests from a single tile would take ~23K ticks of continuous mining. u8 seems sufficient.
- **Should QuarryDeep tiles yield bonus stone?** Mining a quarry tile could be faster than mining raw mountain (the rock is already exposed). This would create a natural "quarry site" that villagers return to repeatedly, concentrating the visual impact.
