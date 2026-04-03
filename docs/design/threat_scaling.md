# Threat Scaling with Settlement Size

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Last updated: 2026-04-01*

## Problem

Threats currently scale with timers, not settlement state. Wolf surges fire on a fixed 25% winter roll with a `villager_count / 5 + 1` pack size (capped at 4). Bandit raids gate on `year >= 3` with a flat 8% chance per 100-tick check. Neither system cares about settlement wealth, territorial footprint, or geography. A 30-villager fortified town and a 5-villager camp in year 3 face the same bandit raid probability. Wolves spawn at a random angle 20-35 tiles from settlement center with no regard for terrain -- they can appear in open grassland or materialize out of water.

This violates Pillar 1 (geography shapes everything) and Pillar 3 (Endure phase emerges from settlement growth, not timers).

## Goals

1. Threats scale with settlement size and wealth, not calendar year or fixed timers.
2. Threats arrive from geographically appropriate directions (wolves from forests, raiders from mountain passes and roads).
3. Large, wealthy settlements attract more frequent and larger threats.
4. Geography creates natural defensive advantages -- rivers as moats, mountains as walls, chokepoints as garrison sites.
5. The player can "read" incoming threat direction from the terrain and plan defenses accordingly.

## Non-Goals

- Real-time combat system (combat stays auto-resolved via garrison defense rating).
- Manual patrol routes or guard assignments.
- Threat faction AI with memory, diplomacy, or persistent camps (future work).
- Siege equipment or wall-breaching mechanics.

## Design

### Wealth Signal: Settlement Threat Score

Replace the current `DifficultyState::threat_level` (milestone-based additive float) with a continuously computed **threat score** derived from actual settlement state. This score drives all threat scaling.

```
threat_score = population_score + wealth_score + territory_score

population_score = villager_count * 1.0
wealth_score     = (food + wood + stone + grain * 2 + bread * 3 + planks * 2 + masonry * 3) / 20.0
territory_score  = (buildings_count * 2.0) + (explored_tiles / 100.0)
```

The divisors are tuned so that a starting settlement (5 villagers, 20 food, 15 wood, 10 stone, 1 stockpile) has a threat score around 10, and a mature settlement (25 villagers, 200+ total resources, 15+ buildings) scores around 60-80.

**Key property:** threat score drops when villagers die, resources are stolen, or buildings are destroyed. Losing a raid makes you less attractive to the next one. This creates a natural rubber-banding effect -- struggling settlements get breathing room.

### Threat Tiers

Threat score maps to discrete tiers that unlock threat types and control intensity:

| Tier | Threat Score | Threats Available | Pack/Party Size |
|------|-------------|-------------------|-----------------|
| 0 - Quiet | 0-14 | Lone wolves (ambient) | 1 |
| 1 - Noticed | 15-29 | Wolf packs | 2-3 |
| 2 - Tempting | 30-49 | Wolf packs, scout raiders | 2-4 wolves, 1-2 scouts |
| 3 - Wealthy | 50-74 | Large wolf packs, raiding parties | 3-5 wolves, 3-5 raiders |
| 4 - Empire | 75+ | Coordinated raids, wolf sieges | 4-8 wolves, 5-8 raiders |

Tier 0 means a tiny settlement is never swarmed. A settlement must grow before it attracts serious attention. This replaces the year-gated bandit raids entirely.

### Threat Check Frequency

Replace the fixed 100-tick event check with a threat-score-scaled interval:

```
base_interval    = 200 ticks
threat_interval  = max(50, base_interval - threat_score * 1.5)
```

At score 10 (early game): check every ~185 ticks. At score 75 (late game): check every ~88 ticks. This means wealthy settlements face more frequent rolls, not just bigger threats.

Each check rolls against per-threat-type probabilities that also scale:

```
wolf_chance   = min(40, 5 + threat_score * 0.4)   // 5% at score 0, 37% at score 80
raider_chance = min(30, max(0, (threat_score - 25) * 0.5))  // 0% below 25, 27.5% at score 80
```

Season still modifies these: wolves get a 1.5x multiplier in winter (hunger-driven), raiders get a 1.3x multiplier in autumn (pre-winter desperation).

### Geographic Spawn Points

This is the core of the design. Threats don't spawn at a random angle from settlement center. They spawn at geographically appropriate locations on the map edge or terrain boundary.

#### Wolf Spawn: Forest Edges

Wolves come from forests. The spawn algorithm:

1. **Scan for forest zones.** At world-gen (or lazily on first threat check), identify contiguous forest regions using flood-fill on Forest tiles. Store as a list of forest "clusters" with bounding box and centroid.

2. **Filter by distance.** Only consider forest clusters whose nearest edge is 15-60 tiles from settlement center. Too close means they'd already be visible. Too far means they'd take forever to arrive and the player can't see them approach.

3. **Weight by cluster size.** Larger forests produce larger packs. A forest cluster of 200+ tiles can spawn the tier's full pack size. A cluster under 50 tiles spawns at most 2 wolves.

4. **Pick spawn point.** Choose the forest-edge tile closest to the settlement along a walkable path. Wolves spawn 2-3 tiles inside the forest edge so they emerge from tree cover, not materialize in the open.

5. **Fallback.** If no qualifying forest exists (desert/tundra maps), wolves spawn from the nearest non-water map edge in the direction of the least settlement infrastructure (fewest buildings in that quadrant). Lone wolves only -- no pack bonus without forest cover.

```rust
// Pseudocode for wolf spawn location
fn find_wolf_spawn(map: &TileMap, settlement: (i32, i32)) -> Option<(f64, f64)> {
    let forests = find_forest_clusters(map);
    let candidates: Vec<_> = forests.iter()
        .filter(|f| f.distance_to(settlement) >= 15.0
                  && f.distance_to(settlement) <= 60.0)
        .collect();

    if candidates.is_empty() {
        return find_edge_spawn_avoiding_settlement(map, settlement);
    }

    // Weight by size: bigger forest = more likely spawn source
    let chosen = weighted_random(&candidates, |f| f.tile_count as f64);
    let edge_tile = chosen.nearest_edge_tile_toward(settlement);
    // Spawn just inside the forest, not on the edge
    Some(offset_into_forest(edge_tile, chosen, 2.0))
}
```

**Visual payoff:** The player sees wolves emerging from the dark forest to the north. Next game, the forest is to the east, so the threat axis is different. Geography shapes the threat. (Pillar 1.)

#### Raider Spawn: Mountain Passes and Roads

Raiders are humans. They travel on passable terrain and prefer existing infrastructure. The spawn algorithm:

1. **Identify approach corridors.** Scan outward from settlement center along 8 cardinal/diagonal directions. For each direction, measure the "corridor width" -- the narrowest band of walkable terrain between impassable features (water, mountains, cliffs). Narrow corridors (width 3-8 tiles) are mountain passes. Wide open areas (width 20+) are plains approaches.

2. **Score corridors.** Raiders prefer:
   - Mountain passes (narrow corridors): 3x weight. Ambush terrain. Realistic.
   - Road tiles in the corridor: 2x weight. Raiders follow roads -- they're coming from elsewhere.
   - Direction away from water bodies: 1.5x weight. Raiders don't swim.
   - Direction with lowest settlement influence: 1.2x weight. They approach from the undefended side.

3. **Pick spawn point.** Spawn raiders 40-60 tiles from settlement center along the chosen corridor, on a walkable tile. If a road exists in that direction, spawn on the road.

4. **Raider approach behavior.** Unlike wolves (who beeline toward prey), raiders should path toward the stockpile using A*. They follow roads when available (faster movement). This means the player can see them coming along the road and garrison the approach.

```rust
// Pseudocode for raider spawn location
fn find_raider_spawn(map: &TileMap, settlement: (i32, i32), influence: &InfluenceMap) -> Option<(f64, f64)> {
    let corridors = scan_approach_corridors(map, settlement, radius: 60);

    let scored: Vec<_> = corridors.iter().map(|c| {
        let mut score = 1.0;
        if c.min_width <= 8 { score *= 3.0; }  // mountain pass
        if c.has_road { score *= 2.0; }          // road access
        if !c.crosses_water { score *= 1.5; }    // no water crossing
        let inf = influence.average_along(c);
        score *= 1.0 + (1.0 - inf).max(0.0);    // low-influence bonus
        (c, score)
    }).collect();

    let chosen = weighted_random(&scored, |(_, s)| *s);
    let spawn_tile = chosen.0.tile_at_distance(50);
    Some(snap_to_walkable(map, spawn_tile))
}
```

**Visual payoff:** Raiders come through the mountain pass from the northwest. The player builds a garrison at that chokepoint. Next seed, the pass is in a different place, so the garrison goes elsewhere. (Pillar 1 again.)

### Natural Defensive Geography

Terrain features provide passive defense bonuses that modify threat outcomes. This extends `compute_defense_rating()`.

#### River Defense

Water tiles between the threat spawn direction and the settlement act as a natural moat.

```
river_defense = river_crossings_on_approach * 3.0
```

Where `river_crossings_on_approach` counts how many water tiles a straight line from spawn point to settlement center crosses. Each crossing costs the attackers time (they must path around) and reduces effective raid strength:

```
effective_raid_size = ceil(raid_size * (1.0 - 0.15 * river_crossings))
```

A settlement behind two river bends loses 30% of incoming raiders to "attrition" (they give up pathfinding, or arrive spread out over many ticks so garrison can handle them piecemeal).

#### Mountain/Cliff Walls

Impassable terrain (Mountain for wolves, Cliff always) on a flank means threats cannot approach from that direction. The system accounts for this during corridor scanning -- if a direction is fully blocked by mountains, it is simply not a valid approach corridor. Settlements in valleys with few entrances are naturally defensible.

```
// Defensive coverage: what fraction of the perimeter (at radius 30) is blocked?
perimeter_blocked = count_impassable_on_circle(settlement, radius: 30) / total_circle_tiles
mountain_defense  = perimeter_blocked * 15.0
```

A settlement backed against a mountain range (50% perimeter blocked) gets +7.5 defense. A settlement in open plains gets +0.

#### Chokepoint Detection

A chokepoint is a narrow walkable corridor between impassable features. The system should detect these at world-gen and mark them as strategic locations for the auto-build system.

**Detection algorithm:**
1. From settlement center, cast rays outward every 15 degrees (24 rays).
2. Along each ray, find the narrowest walkable band perpendicular to the ray direction.
3. If the narrowest point is 1-6 tiles wide and flanked by impassable terrain on both sides, mark it as a chokepoint.
4. Store chokepoints as `Vec<Chokepoint>` with position, width, and direction.

```rust
struct Chokepoint {
    x: f64,
    y: f64,
    width: u32,           // tiles wide at narrowest
    direction: f64,       // angle from settlement center (radians)
    distance: f64,        // distance from settlement center
    flanked_by: (Terrain, Terrain), // what blocks each side
}
```

**Auto-build integration:** When the auto-build system decides to place a garrison, it should prefer chokepoint locations over random placement near settlement center. A garrison at a 3-tile-wide mountain pass is worth far more than one in the middle of town.

**Overlay integration:** The Threats overlay (`OverlayMode::Threats`) should highlight detected chokepoints with a distinct color, showing the player where natural defenses exist.

#### Elevation Advantage

Higher ground provides sight range and defense bonuses. If the settlement center sits at higher elevation than the surrounding terrain:

```
elevation_defense = max(0, (settlement_elevation - avg_surrounding_elevation) * 2.0)
```

Capped at 5.0. A hilltop settlement sees threats earlier (increased effective sight range for garrison detection) and fights better.

### Updated Defense Rating

The full defense formula, replacing the current garrison + walls calculation:

```
defense_rating = garrison_defense           // existing: sum of garrison.defense_bonus
               + wall_defense               // existing: wall_tiles * 0.3
               + river_defense              // NEW: river_crossings * 3.0
               + mountain_defense           // NEW: perimeter_blocked * 15.0
               + elevation_defense          // NEW: height advantage * 2.0
               + chokepoint_bonus           // NEW: garrison_at_chokepoint * 5.0
```

The `chokepoint_bonus` rewards placing garrisons at detected chokepoints. If a garrison entity is within 5 tiles of a detected chokepoint, it gets a 5.0 bonus per chokepoint covered.

### Raid Resolution

Currently, bandit raids instantly steal 25% of resources. With geographic spawning, raids should be a process, not an instant event:

1. **Spawn.** Raiders appear at chosen corridor point. Event log: "Raiders spotted approaching from the northwest!"
2. **Approach.** Raiders path toward settlement stockpile. Movement speed 0.12 (slower than villagers). Takes 30-60 ticks to arrive depending on distance and terrain.
3. **Arrival.** When raiders reach within 10 tiles of stockpile, compare raid strength vs defense rating:
   - `raid_strength = raider_count * 3.0`
   - If `defense_rating >= raid_strength`: raiders are repelled. They flee. Event log: "Raiders repelled by garrison!" Some raiders may be killed (despawned).
   - If `defense_rating < raid_strength`: raiders steal resources proportional to the strength gap. `steal_fraction = min(0.5, (raid_strength - defense_rating) / (raid_strength * 2))`. They then flee.
4. **Retreat.** Surviving raiders path back the way they came and despawn at map edge.

**Visual payoff:** The player watches raiders approach along the road, sees them hit the garrison chokepoint, and watches them flee or break through. Observable simulation. (Pillar 4.)

### Wolf Behavior Refinement

Wolves already have predator AI (`ai_predator` in `ai.rs`). The changes are about *how they arrive*, not how they fight:

1. **Pack cohesion.** Wolves from the same surge share a "pack target" -- the settlement center. They path toward it as a loose group (spawn within 5 tiles of each other, similar speed). This makes them visually read as a pack, not scattered individuals.

2. **Forest-edge lurking.** Before committing to attack, wolves from a forest cluster should linger at the forest edge for 30-50 ticks. This gives the player a window to notice them on the threat overlay. After the linger period, they advance toward the nearest villager or prey animal.

3. **Retreat on satiation.** Wolves that eat (hunger drops below 0.2) should path back toward their spawn forest rather than continuing to wander near the settlement. Removes the problem of post-hunt wolves loitering and killing more villagers.

4. **Scaling with forest depletion.** If the player has deforested areas near the settlement, wolves spawn from more distant forests. This is a natural consequence of the forest-cluster algorithm -- fewer nearby forest tiles means the nearest qualifying cluster is further away. Deforestation is both expansion and defense.

### Seasonal Modifiers (Retained, Refined)

Seasons still matter but modify the wealth-based system rather than driving it:

| Season | Wolf Modifier | Raider Modifier | Notes |
|--------|--------------|-----------------|-------|
| Spring | 0.5x | 0.8x | Prey abundant, raiders recovering from winter |
| Summer | 0.7x | 1.0x | Normal |
| Autumn | 1.0x | 1.3x | Raiders stock up before winter |
| Winter | 1.5x | 0.5x | Wolves desperate, raiders hunker down |

These multiply the per-check probability, not the pack size. Winter means wolves come more often, not in bigger packs (pack size is tier-driven by threat score).

## Implementation Plan

### Phase 1: Wealth-Based Threat Score (replace timer gates)

- Add `compute_threat_score()` to `Game` using the formula above.
- Replace `year >= 3` bandit gate with `threat_score >= 30`.
- Replace fixed wolf count cap of 4 with tier-based pack size.
- Replace 100-tick fixed interval with score-scaled interval.
- Remove milestone-based `threat_level` increments from `check_milestones()` (milestones remain as notifications, just don't drive threat scaling).

**Files:** `src/game/events.rs`, `src/game/mod.rs` (DifficultyState)

### Phase 2: Geographic Wolf Spawns

- Implement `find_forest_clusters()` using flood-fill on Forest tiles.
- Cache forest clusters in `Game` state (recompute when deforestation occurs or every 500 ticks).
- Replace random-angle wolf spawn in `update_events()` with forest-edge spawn.
- Add pack cohesion (spawn wolves within 5 tiles of each other).

**Files:** `src/game/events.rs`, `src/tilemap.rs` (add cluster detection helper)

### Phase 3: Geographic Raider Spawns

- Implement `scan_approach_corridors()` for corridor detection.
- Spawn raider entities (new Species::Raider or reuse existing bandit event).
- Raiders path toward stockpile using A*.
- Replace instant resource theft with approach-and-resolve model.

**Files:** `src/game/events.rs`, `src/ecs/components.rs` (Raider species), `src/ecs/ai.rs` (raider AI), `src/ecs/spawn.rs` (spawn_raider)

### Phase 4: Natural Defense Integration

- Implement `count_river_crossings()` for a given approach direction.
- Implement `compute_perimeter_coverage()` for mountain defense.
- Implement `detect_chokepoints()` at world-gen time.
- Extend `compute_defense_rating()` with new geographic terms.
- Auto-build prefers garrison at chokepoints.

**Files:** `src/game/build.rs`, `src/tilemap.rs`, `src/game/mod.rs`

### Phase 5: Visual Integration

- Threats overlay shows: detected chokepoints, forest threat sources, corridor approach arrows.
- Wolf packs render as a cluster of `W` glyphs emerging from forest edge.
- Raiders render as `B` (bandit) glyphs moving along roads.
- Event log includes directional info: "Wolf pack approaching from the northern forest!"

**Files:** `src/game/render.rs`

## Testing Strategy

**Unit tests:**
- `compute_threat_score()` returns expected values for known resource/population states.
- `find_forest_clusters()` correctly identifies contiguous forest regions on a test map.
- `scan_approach_corridors()` identifies narrow passes on a map with mountains.
- `detect_chokepoints()` finds the 3-tile gap between two mountain ranges.
- Wolf pack size scales correctly with threat tier.
- Raider spawn probability is 0 below threat score 25.
- River crossings reduce effective raid size.
- Garrison at chokepoint gets bonus defense.

**Integration tests:**
- Run 1000-tick simulation, verify wolves spawn near forest tiles (not in water/open desert).
- Run simulation to threat score 50, verify raider events begin occurring.
- Place garrison at detected chokepoint, verify defense rating is higher than garrison in open field.
- Deforest area near settlement, verify wolf spawn distance increases.
- Verify threat score drops after a successful raid (resources stolen -> lower wealth_score).

**Regression tests:**
- Early game (score < 15) should never see more than 1 wolf at a time.
- A settlement on a map with no forests should still get occasional lone wolf threats (edge fallback).
- Bandit raids should never occur before the settlement has enough wealth to make them interesting (score < 30 = no raiders).

## Open Questions

1. **Should threat score be visible to the player?** As a number in the diagnostics panel, or just felt through raid frequency? Leaning toward showing it -- transparency supports observable simulation (Pillar 4).

2. **Raider entities vs. instant event?** The phased approach (spawn, approach, resolve, retreat) is much better for Pillar 4 but requires raider entity AI. Minimum viable version could spawn raiders as entities with simple seek-stockpile AI. Full version would have them fight garrison entities.

3. **Forest cluster caching.** Recomputing flood-fill every threat check is expensive on 256x256 maps. Cache at world-gen and invalidate on terrain change (deforestation)? Or compute lazily every N ticks?

4. **Multiple simultaneous threats.** Can a wolf pack and raider party arrive in the same season? Current design says yes -- independent rolls. But should there be a cooldown between threats to avoid overwhelming small settlements? Proposed: minimum 100 ticks between any two threat spawns.

5. **Raider retreat path.** If the garrison blocks the approach corridor, do raiders try an alternate route? For now, no -- they path back the way they came. Alt-route raiders are a Phase 5+ feature.

## References

- `src/game/events.rs` -- current event system (wolf surge, bandit raid, drought)
- `src/ecs/ai.rs` -- predator AI (`ai_predator`), wolf aggression scaling
- `src/ecs/spawn.rs` -- `spawn_predator()`, entity creation patterns
- `src/game/build.rs` -- `compute_defense_rating()`, `settlement_center()`
- `src/game/mod.rs` -- `DifficultyState`, milestone system
- `docs/game_design.md` -- Pillar 1 (geography), Pillar 3 (Endure phase), Pillar 4 (observable)
