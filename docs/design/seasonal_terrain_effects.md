# Seasonal Terrain Effects

**Status:** Proposed
**Pillars:** Geography Shapes Everything (#1), Emergent Complexity (#2), Explore/Expand/Exploit/Endure (#3), Observable Simulation (#4)
**Phase:** 2 (Economy Depth)
**Last updated:** 2026-04-01

## Problem

Seasons exist mechanically (`SeasonModifiers` in `simulation.rs`) but are invisible on the map. Spring doubles vegetation growth rate, summer halves rain, winter kills vegetation growth entirely -- but you cannot *see* any of this. The map at tick 500 (spring) looks identical to tick 2000 (winter). A river in January is the same blue `~` as in July. A dry grassland in August has the same fire risk as a wet meadow in April: zero, because fire does not exist.

This violates multiple pillars:

- **Pillar 1:** Seasons should change what the terrain *offers*. A frozen river is a crossing. A flooded plain is a barrier. A dry forest is a hazard. Geography should shift with the calendar.
- **Pillar 2:** Seasonal effects should chain through simulation state (simulation_chains.md), not apply flat multipliers. Spring floods should flow through WaterMap, not `farm_growth *= 0`.
- **Pillar 3:** Seasons create rhythm within the 4X arc. Spring = expand, summer = exploit, autumn = prepare, winter = endure. Without visible/mechanical seasons, the arc has no pulse.
- **Pillar 4:** If seasons are invisible, they don't count. The player should glance at the map and know what season it is from terrain alone.

### What exists today

| Aspect | Current state | Code location |
|--------|--------------|---------------|
| Season enum | `Spring / Summer / Autumn / Winter`, 10 days each | `simulation.rs:366-371`, `DayNightCycle::step` |
| Season modifiers | `rain_mult`, `evap_mult`, `veg_growth_mult`, `hunger_mult`, `wolf_aggression` | `simulation.rs:385-477` |
| Visual change | None. Terrain colors/glyphs are season-independent. | `tilemap.rs:Terrain::ch()`, `Terrain::fg()` |
| Water behavior | WaterMap flows, evaporates, accumulates. No freeze, no flood threshold. | `simulation.rs` WaterMap |
| Fire | Does not exist. | -- |
| River crossing | Water is walkable at 0.15x (pending rivers_as_barriers.md making it impassable) | `tilemap.rs:move_cost` |

## Design

Four seasonal effects, each tied to a specific season, each chaining through existing simulation layers. Every effect changes terrain tiles visibly and has a concrete gameplay consequence.

**Core rule (from simulation_chains.md):** No effect directly modifies a gameplay output. Effects modify world conditions; gameplay systems read those conditions.

### Effect 1: Spring Floods

**When:** Spring (days 0-10 of each year). Peak at days 3-7.

**Mechanism:**
Spring already has `rain_mult: 1.5`. Floods emerge naturally when we add a flood threshold to WaterMap (as described in simulation_chains.md Chain 2). The sequence:

```
Spring rain_mult: 1.5 (existing)
  -> WaterMap: water accumulates in low-elevation tiles near rivers
    -> Tiles with water_level > FLOOD_THRESHOLD (proposed: 0.4):
      -> Terrain temporarily becomes Terrain::FloodWater
        -> Farms on flooded tiles: growth resets to 0, crop destroyed
        -> Buildings on flooded tiles: take durability damage (future)
        -> Movement: FloodWater is non-walkable (like Water)
    -> After water recedes (water_level drops below 0.2):
      -> Tile reverts to base terrain
      -> SoilFertility gets alluvial deposit bonus (+0.15)
        -> Post-flood farmland is the richest in the game
```

**Which tiles flood:**
- Tiles within 3-5 tiles of a river AND below river elevation + 0.02
- Tiles with `SoilType::Alluvial` are the primary flood zone (these are already placed by the pipeline at `dist_to_river < 4 && slope < 0.05`)
- Marsh tiles adjacent to rivers flood at a lower threshold (0.25 instead of 0.4)
- Mountain, Cliff, and elevated tiles never flood

**New terrain variant:**

| Variant | Glyph | FG Color | BG Color | Walkable | Speed | A* Cost |
|---------|-------|----------|----------|----------|-------|---------|
| `FloodWater` | `~` | (100, 150, 200) | (40, 70, 110) | No | -- | Infinity |

FloodWater is visually distinct from permanent Water: lighter blue with a brownish tint (sediment-laden). It signals "temporary, dangerous, but fertile aftermath."

**Gameplay impact:**
- Flood plains are high-risk, high-reward farmland. Best soil in the game, but crops get destroyed every spring.
- Settlements must choose: farm the flood plain and accept spring losses, or farm safer ground with worse yields.
- Granary stockpiles become critical -- you need food reserves to survive the spring gap.
- Building placement near rivers matters. Huts on the flood plain get damaged; huts on higher ground are safe.
- This creates the "Nile delta" decision: the richest land is the most dangerous.

**Interaction with rivers_as_barriers.md:** If rivers are already impassable (as proposed), spring floods *widen* the impassable zone. Bridges on flood-plain tiles may become temporarily impassable during peak flood, cutting off far-bank settlements for a few days. This is a dramatic seasonal event, not a bug.

### Effect 2: Winter Ice

**When:** Winter (days 30-39 of each year). Ice forms at day 31, thaws at day 39 (Spring day 0).

**Mechanism:**
```
Winter arrives (day 30)
  -> Temperature proxy: season == Winter (future: per-tile temperature from pipeline)
    -> Water tiles freeze:
      -> Terrain::Water becomes Terrain::Ice (visual swap, not permanent terrain change)
      -> Terrain::Ice is WALKABLE at 0.5x speed (A* cost 2.0)
        -> Rivers become crossable without bridges!
        -> Lakes become traversable shortcuts
    -> FloodWater (if any remains) also freezes to Ice
    -> Spring arrives (day 0):
      -> Ice reverts to Water
      -> Any entities standing on Ice when it thaws: forced to nearest walkable tile (pushed to bank)
```

**Which tiles freeze:**
- All `Terrain::Water` tiles freeze. Rivers, lakes, ocean shallows -- everything.
- In a future temperature system, only tiles below a freeze threshold would ice over (allowing tropical rivers to stay liquid). For now, winter = frozen everywhere. This is acceptable because the biome system already places snow/tundra in cold regions; freezing is consistent with the existing seasonal model.

**New terrain variant:**

| Variant | Glyph | FG Color | BG Color | Walkable | Speed | A* Cost |
|---------|-------|----------|----------|----------|-------|---------|
| `Ice` | `=` | (180, 210, 240) | (120, 150, 180) | Yes | 0.5x | 2.0 |

The `=` glyph reads as a flat, solid surface -- distinct from `~` water. The pale blue palette is immediately recognizable as ice.

**Gameplay impact:**
- Winter opens new routes. A river that blocked expansion all year becomes a highway in winter. Villagers can cross to scout, gather, or raid.
- Wolf packs can also cross frozen rivers. A settlement that relied on a river as a natural wall is suddenly vulnerable in winter. This creates seasonal tension: winter is the dangerous season not just for hunger but for defense.
- Ice crossings enable winter-only exploration. Resources across a river can be scouted in winter, motivating bridge construction for year-round access.
- If rivers_as_barriers.md is implemented (rivers impassable), winter ice is the natural counterbalance: rivers block you 3 seasons, open for 1. Early-game settlements without bridges get a winter window.
- Frozen lakes become shortcuts -- a lake that forces a long detour in summer is a straight path in winter.

**Interaction with fords (rivers_as_barriers.md):** Fords remain usable year-round. Ice makes the entire river walkable, but fords are still the fastest crossing (ford speed 0.3x > ice speed 0.5x... wait, fords are slower. Actually fords at 0.3x are slower than ice at 0.5x). This is intentional -- ice is a wide, smooth surface; fords are rocky and shallow. Winter makes fords less special, which is realistic.

### Effect 3: Summer Fire Risk

**When:** Summer (days 10-19). Peak risk at days 14-18 (mid-to-late summer).

**Mechanism:**
```
Summer evap_mult: 2.0 (existing) + rain_mult: 0.5 (existing)
  -> MoistureMap: moisture drops across the map
    -> VegetationMap: dry vegetation becomes flammable
      -> Tiles with moisture < 0.15 AND vegetation > 0.5:
        -> fire_risk accumulates (0.01/tick while conditions hold)
        -> When fire_risk > IGNITION_THRESHOLD (0.8):
          -> Lightning strike chance OR spontaneous ignition
            -> Terrain::Forest / Terrain::Scrubland -> Terrain::Fire
              -> Fire spreads to adjacent flammable tiles (moisture < 0.2, vegetation > 0.3)
              -> Spread rate: 1 adjacent tile every 5 ticks
              -> Fire burns for 15-25 ticks per tile, then:
                -> Terrain::Fire -> Terrain::Scorched (new)
                  -> VegetationMap: density set to 0.0
                  -> SoilFertility: +0.05 (ash fertilization)
                  -> Scorched reverts to Grass/Sand after 40+ ticks (slow recovery)
```

**Which tiles are flammable:**
- `Forest`: high vegetation, primary fuel. Burns readily.
- `Scrubland`: medium vegetation, burns fast but lower intensity.
- `Grass`: only if vegetation density > 0.6 (tall dry grass).
- `Marsh`: effectively fireproof (high moisture even in summer).
- `Desert`, `Sand`, `Tundra`, `Mountain`, `Snow`, `Water`: non-flammable.

**Fire risk map:** A new per-tile `f64` grid (like MoistureMap). Accumulates when conditions are met, decays when moisture rises. Rain (even light summer rain) resets fire_risk to 0 on affected tiles. This means fire is not random -- it builds visibly in dry areas.

**New terrain variants:**

| Variant | Glyph | FG Color | BG Color | Walkable | Speed | A* Cost |
|---------|-------|----------|----------|----------|-------|---------|
| `Fire` | `^` | (255, 160, 30) | (180, 50, 10) | No | -- | Infinity |
| `Scorched` | `.` | (60, 50, 40) | (30, 25, 20) | Yes | 0.9x | 1.1 |

Fire is bright orange/red -- unmissable. Scorched is dark and ashen -- visibly dead ground.

**Gameplay impact:**
- Deforestation becomes a double-edged sword. Cutting trees removes fuel (no fire risk on cleared land) but also removes a resource. Unmanaged forests far from the settlement become fire hazards in summer.
- Fire clears land. A forest fire that burns through a dense woodland leaves open, slightly fertile scorched ground. This is free land clearing -- if you can tolerate the loss. Savvy play: let fires burn away from the settlement, then farm the cleared land.
- Fire threatens buildings. A `Fire` tile adjacent to a wooden building (Hut, Workshop, Farm) should damage or destroy it. This creates defensive pressure: clear firebreaks (cut trees in a line) around the settlement perimeter.
- Interaction with wind (future): fire spreads preferentially downwind. The wind direction already affects rain shadow in the terrain pipeline; reusing it for fire spread is natural.
- Interaction with deforestation_regrowth.md: fire is an alternative path to deforestation. Forest -> Fire -> Scorched -> (slow regrowth) -> Sapling -> Forest. The cycle takes longer than villager-driven clearing but happens automatically.

**Fire suppression:** Villagers do not fight fires (anti-goal: no micromanagement). Instead, the player's tool is prevention: deforest firebreaks, build near water (high moisture zones), avoid building deep in dry forests. Fire is a geographic hazard the player works around, not a crisis they manage in real-time.

### Effect 4: Autumn Leaf Fall

**When:** Autumn (days 20-29). Gradual transition across the season.

**Mechanism:**
```
Autumn arrives (day 20)
  -> VegetationMap growth multiplier: 0.3 (existing, already slowing growth)
    -> Visual change: Forest tiles shift color palette
      -> Day 20-23: green -> yellow-green (early autumn)
      -> Day 24-27: yellow-green -> orange/gold (peak autumn)
      -> Day 28-29: orange -> brown/bare (late autumn, approaching winter)
    -> Gameplay change: Forest tiles yield +50% wood during autumn
      -> Rationale: fallen branches, dry wood, easier harvesting
      -> Implemented as: gathering timer reduced from 90 to 60 ticks on Forest tiles when season == Autumn
    -> Scrubland color shifts to dry brown/tan
    -> Grass color shifts to pale yellow
```

**Which tiles change visually:**

| Terrain | Normal FG | Autumn FG (peak) | Description |
|---------|-----------|-----------------|-------------|
| `Forest` | (15, 80, 20) | (180, 120, 20) | Green -> golden orange |
| `Grass` | (50, 130, 45) | (160, 150, 60) | Green -> pale yellow |
| `Scrubland` | (110, 120, 50) | (130, 100, 40) | Olive -> dry brown |
| `Marsh` | (40, 100, 60) | (70, 100, 50) | Dark green -> muted olive |

Other terrain types (Sand, Desert, Mountain, Snow, Tundra, Water) do not change -- they have no deciduous vegetation.

**Implementation:** `Terrain::fg()` and `Terrain::bg()` gain a `season: Season` parameter. During Autumn, affected terrain types interpolate between their normal color and their autumn color based on `day_within_season / 10.0`. This is purely a rendering change -- no new terrain variants needed.

**Gameplay impact:**
- Autumn is preparation season. Faster wood gathering (the +50% bonus) creates a natural "stock up before winter" rhythm. Players see forests turn gold and know: gather now.
- The color shift is the primary "seasons are visible" signal. Even if the player ignores all other seasonal mechanics, autumn foliage makes the passage of time *feel* real.
- Autumn vegetation slowdown (existing `veg_growth_mult: 0.3`) means forests cut in autumn regrow very slowly. Autumn deforestation has consequences that last through winter.
- Late autumn's bare brown trees visually preview winter's harshness. The map gradually becomes less lush, building tension.

**Spring counterpart:** Spring should reverse the effect -- bare brown/grey of winter gradually greens up (day 0-3: brown -> light green, day 4-10: light green -> full green). This uses the same interpolation system. Spring green-up is subtle but creates the "relief after winter" feeling.

**Winter visuals:** In winter, `Forest` FG shifts to (80, 80, 70) -- grey-brown bare branches. `Grass` shifts to (100, 100, 80) -- dormant straw. These shifts use the same seasonal color system. Combined with Snow terrain at high elevations and Ice on water, winter should be visually stark.

## Seasonal Color Table (Full Year)

All colors for terrain types that shift with seasons. Terrain types not listed are season-invariant.

| Terrain | Spring FG | Summer FG | Autumn FG | Winter FG |
|---------|-----------|-----------|-----------|-----------|
| Forest | (30, 130, 30) bright green | (15, 80, 20) normal | (180, 120, 20) gold | (80, 80, 70) bare grey |
| Grass | (60, 160, 50) fresh green | (50, 130, 45) normal | (160, 150, 60) pale yellow | (100, 100, 80) straw |
| Scrubland | (90, 130, 50) olive-green | (110, 120, 50) normal | (130, 100, 40) dry brown | (90, 80, 60) dull tan |
| Marsh | (50, 120, 60) spring green | (40, 100, 60) normal | (70, 100, 50) muted | (50, 70, 50) dark muted |

## Seasonal Terrain State Table

Summary of which terrain variants are active in each season.

| Season | New/changed terrain | Source tiles | Trigger condition |
|--------|-------------------|--------------|-------------------|
| Spring | `FloodWater` | Low tiles near rivers | `water_level > 0.4` on alluvial/low tiles |
| Summer | `Fire`, `Scorched` | `Forest`, `Scrubland`, dry `Grass` | `moisture < 0.15 AND vegetation > 0.5`, then ignition |
| Autumn | (color shift only) | `Forest`, `Grass`, `Scrubland`, `Marsh` | `season == Autumn`, interpolated by day |
| Winter | `Ice` | `Water` (all) | `season == Winter`, day >= 1 |

## New Terrain Variants Summary

Four new `Terrain` enum variants needed:

```rust
pub enum Terrain {
    // ... existing variants ...
    FloodWater,  // temporary spring flood, impassable, sediment-colored
    Ice,         // frozen water, walkable, winter only
    Fire,        // actively burning, impassable, spreads
    Scorched,    // post-fire, walkable, low vegetation, slow recovery
}
```

## New Simulation State

| Grid | Type | Size | Purpose |
|------|------|------|---------|
| `fire_risk` | `Vec<f64>` | width * height | Per-tile fire risk accumulation (0.0-1.0) |
| `base_terrain` | `Vec<Terrain>` | width * height | Original terrain under seasonal overlays (so Ice can revert to Water, FloodWater can revert to Grass, etc.) |

The `base_terrain` grid stores what each tile "really is" when no seasonal effect is active. Seasonal systems write temporary terrain to the active tilemap and revert to `base_terrain` when the effect ends. This avoids permanent terrain corruption from seasonal cycling.

## Interaction Matrix

How seasonal effects interact with each other and with proposed systems from other design docs.

| System A | System B | Interaction |
|----------|----------|-------------|
| Spring floods | Rivers as barriers | Floods widen the impassable river zone; bridges on flood tiles temporarily blocked |
| Spring floods | Simulation chains (soil fertility) | Post-flood alluvial deposit raises SoilFertility |
| Spring floods | Farms | Crops on flooded tiles destroyed; motivates flood-plain risk/reward farming |
| Winter ice | Rivers as barriers | Ice makes rivers walkable, bypassing bridge requirement for 1 season |
| Winter ice | Wolves / threats | Predators can cross frozen rivers; settlements lose river-wall defense in winter |
| Summer fire | Deforestation/regrowth | Fire is natural deforestation; scorched land enters the regrowth cycle |
| Summer fire | Simulation chains (moisture) | Moisture drives fire risk; drought (low rain) + summer = extreme fire danger |
| Summer fire | Buildings | Adjacent fire damages/destroys wooden buildings |
| Autumn wood bonus | Deforestation/regrowth | Faster autumn harvesting accelerates deforestation if not managed |
| Autumn colors | Observable simulation | Primary visual indicator that seasons are passing |

## Implementation Plan

### Phase A: Seasonal color system (rendering only, no gameplay change)

1. **tilemap.rs**: Add `season` parameter to `Terrain::fg()` and `Terrain::bg()`. Affected types (`Forest`, `Grass`, `Scrubland`, `Marsh`) interpolate between seasonal palettes based on `(season, day_in_season)`.
2. **game/render.rs**: Pass current season from `DayNightCycle` into terrain rendering calls.
3. **Tests**: Verify `Forest::fg(Season::Autumn, 5)` returns gold, `Forest::fg(Season::Spring, 5)` returns bright green, etc.

### Phase B: Winter ice (high impact, low complexity)

4. **tilemap.rs**: Add `Terrain::Ice` variant with glyph `=`, walkable, speed 0.5, cost 2.0.
5. **simulation.rs**: Add `base_terrain: Vec<Terrain>` to store pre-seasonal terrain. On winter day 1, scan all `Water` tiles and swap to `Ice`. On spring day 0, revert `Ice` to `Water`. Entities on melting ice get displaced to nearest walkable tile.
6. **Tests**: Verify water becomes ice in winter, reverts in spring. Verify A* can path across ice. Verify entities displaced on thaw.

### Phase C: Spring floods (depends on simulation_chains.md step 6)

7. **tilemap.rs**: Add `Terrain::FloodWater` variant, non-walkable, sediment-colored.
8. **simulation.rs**: Add flood threshold check in WaterMap update. Tiles near rivers (use `river_mask` from pipeline data) with `water_level > FLOOD_THRESHOLD` become `FloodWater`. When water recedes below 0.2, revert to base terrain and apply fertility bonus to `SoilFertilityMap`.
9. **game/build.rs**: Auto-build should avoid placing farms on known flood-plain tiles (tiles that flooded in the previous year). Or: warn via overlay, let the player decide.
10. **Tests**: Verify flood triggers in spring on river-adjacent low tiles. Verify farms destroyed on flooded tiles. Verify post-flood fertility bonus. Verify flood recedes and terrain reverts.

### Phase D: Summer fire (most complex, standalone system)

11. **simulation.rs**: Add `fire_risk: Vec<f64>` grid. Each tick in summer: for tiles with `moisture < 0.15 AND vegetation > 0.5`, increment `fire_risk` by 0.01. Decay `fire_risk` by 0.02 when moisture > 0.3.
12. **simulation.rs**: Add ignition check: tiles with `fire_risk > 0.8` have a per-tick ignition chance (0.005). On ignition, set tile to `Terrain::Fire`.
13. **simulation.rs**: Add fire spread: `Fire` tiles check adjacent tiles every 5 ticks. Adjacent tiles with `moisture < 0.2 AND vegetation > 0.3` catch fire.
14. **simulation.rs**: Add fire burnout: `Fire` tiles burn for 15-25 ticks (randomized at ignition), then become `Terrain::Scorched`. Scorched tiles recover to base terrain after 40+ ticks.
15. **tilemap.rs**: Add `Terrain::Fire` (non-walkable, bright orange) and `Terrain::Scorched` (walkable, dark ash).
16. **ecs/systems.rs**: Buildings adjacent to `Fire` tiles take damage (if building durability exists) or are destroyed.
17. **Tests**: Verify fire_risk accumulates in dry forest. Verify fire spreads to adjacent dry tiles. Verify fire does not spread to wet tiles. Verify scorched terrain recovers. Verify fire does not occur in non-summer seasons (moisture too high).

### Phase E: Autumn wood bonus (simple, pairs with Phase A)

18. **ecs/ai.rs**: In wood gathering logic, check season. If `Autumn`, reduce gathering timer from 90 to 60 ticks (or multiply by 0.67).
19. **Tests**: Verify wood gathering is faster in autumn. Verify gathering speed is normal in other seasons.

## Testing Strategy

| Test | Validates | Phase |
|------|-----------|-------|
| `forest_fg_changes_in_autumn` | Terrain color interpolation works for autumn | A |
| `grass_fg_greens_in_spring` | Spring color shift is distinct from summer | A |
| `water_freezes_in_winter` | Water -> Ice transition on winter day 1 | B |
| `ice_is_walkable` | A* can path across ice tiles | B |
| `ice_thaws_in_spring` | Ice -> Water reversion on spring day 0 | B |
| `entity_displaced_on_thaw` | Entity on melting ice moved to nearest bank | B |
| `spring_rain_causes_flood` | River-adjacent low tiles become FloodWater | C |
| `flood_destroys_farm_growth` | Farm on flooded tile has growth reset | C |
| `flood_recede_restores_terrain` | FloodWater reverts when water drops | C |
| `post_flood_fertility_bonus` | SoilFertility increases after flood recedes | C |
| `fire_risk_accumulates_in_dry_forest` | fire_risk grid increases during summer drought | D |
| `fire_spreads_to_adjacent_dry` | Fire propagates to nearby flammable tiles | D |
| `fire_blocked_by_water` | Fire does not cross water/marsh tiles | D |
| `fire_burns_out_to_scorched` | Fire -> Scorched after burn duration | D |
| `scorched_recovers_over_time` | Scorched -> base terrain after recovery period | D |
| `autumn_wood_gathering_faster` | Gathering timer reduced in autumn | E |

## Performance Notes

- **Seasonal color interpolation (Phase A):** One branch + lerp per rendered tile per frame. Negligible -- fewer operations than the existing lighting system.
- **Ice/flood terrain swap (Phase B, C):** Full tilemap scan once per season transition (every 10 game-days). O(width * height) = 65K operations on 256x256. Sub-millisecond. Not per-tick.
- **Fire simulation (Phase D):** Per-tick update of `fire_risk` grid only during summer, only for flammable tiles. Could spatial-partition to only check tiles with vegetation > 0.3, but even a full scan is O(65K) simple comparisons -- well under 1ms. Fire spread is local (8-neighbor check per burning tile) and fires are rare. No concern.
- **base_terrain grid:** One additional `Vec<Terrain>` the size of the tilemap. ~65KB for 256x256. Negligible memory.

## Open Questions

- **Flood threshold tuning:** 0.4 is a starting guess. Too low = constant floods. Too high = floods never happen. Needs playtesting with the spring `rain_mult: 1.5`. Related: simulation_chains.md lists this same question.
- **Ice on ocean tiles:** Should ocean freeze? Realistically, open ocean does not freeze (except polar). But the game has no ocean/river distinction in the `Terrain` enum -- it is all `Water`. Options: (a) freeze everything (simple, dramatic), (b) only freeze tiles marked by `river_mask` (rivers + small lakes), (c) freeze based on water depth / distance from shore. Option (b) is probably the right balance.
- **Fire intensity:** Should all fires burn the same? Or should forest fires (high vegetation) burn longer/hotter than scrubland fires? Variable burn duration based on initial vegetation density is low-cost and adds realism.
- **Building fire damage:** The building system does not currently have durability/HP. If buildings are binary (exists / destroyed), fire adjacent to a building should have a per-tick destruction probability rather than guaranteed damage. This keeps fire dangerous but not deterministic.
- **Should the player see fire_risk?** An overlay showing fire danger (yellow -> orange -> red on dry forests) would let the player act preventively (cut firebreaks). This fits Pillar 4 (observable). But it also reduces surprise. Propose: show fire_risk overlay only when the fire overlay mode is active (added to the `o` overlay cycle).
- **Frozen ford interaction:** If fords exist (rivers_as_barriers.md) and the river freezes, does the ford tile also become Ice? Yes -- consistency. The ford is just another water-adjacent tile that freezes. When ice thaws, the ford returns.
- **Multi-biome temperature:** The current system freezes all water in winter regardless of biome. A tropical river should not freeze. Proper fix requires per-tile temperature (the pipeline has `temperature: Vec<f64>`). Future work: only freeze tiles where `temperature[idx] < FREEZE_THRESHOLD`. For now, global freeze is acceptable as a simplification.

## Validation

The feature is working when:

1. A new player can identify the current season by looking at the map without checking the date display -- autumn is golden, winter is grey with ice, spring has visible floods near rivers, summer shows brown/dry terrain.
2. A settlement near a river visibly deals with spring floods: farms on the flood plain are destroyed, rebuilt after the flood, and grow faster than inland farms.
3. Winter opens a frozen river crossing that wolves use to attack, creating a seasonal defense crisis.
4. A dry forest catches fire in summer, burns a visible scar across the map, and the scorched area slowly recovers over subsequent years.
5. Villagers stock up on wood faster in autumn (visible as increased gathering trips), creating a natural "prepare for winter" rhythm.
6. Playing the same seed twice, the player can narrate the seasonal cycle from terrain changes alone: "spring floods, summer drought and fire, autumn gold, winter ice."
