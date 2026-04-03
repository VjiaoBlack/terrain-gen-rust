# Elevation Advantage

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Last updated: 2026-04-01*

## Problem

Height data exists in the terrain pipeline (`PipelineResult::heights` in `src/terrain_pipeline.rs`) and persists at runtime on the `Game` struct as `self.heights: Vec<f64>`, but elevation has zero gameplay effect after world generation. A garrison on a hilltop and a garrison in a valley floor have identical sight range and identical defense rating. Villagers standing on a ridge see exactly as far as villagers in a riverbed.

This wastes the richest spatial signal the terrain pipeline produces and violates Pillar 1 (geography shapes everything). The player has no reason to care whether a building sits at elevation 0.7 or 0.3 -- the number is cosmetic.

## Goals

1. Higher elevation grants increased sight range for creatures and garrison detection radius.
2. Higher elevation grants a defense bonus for garrison buildings (and eventually all combat).
3. Attacking uphill incurs a combat penalty for threats (wolves, raiders).
4. The player can "read" the heightmap to identify strategically valuable hilltop sites versus fertile valley farmland.
5. Settlement placement becomes a meaningful trade-off: hilltops for defense, valleys/river plains for farming.

## Non-Goals

- Elevation-based movement speed penalties (future work, separate from this doc).
- Line-of-sight occlusion / shadow casting visibility (complex, separate feature).
- Z-level rendering or multi-layer maps.
- Elevation affecting projectile range (no ranged combat system yet).

## Design

### Height Lookup

The terrain pipeline stores heights as a flat `Vec<f64>` of length `width * height`, values normalized roughly 0.0 to 1.0. Lookup for a tile at `(x, y)`:

```rust
let elevation = self.heights[y * self.map.width + x];
```

For entity positions (floating-point), snap to the nearest tile:

```rust
fn tile_elevation(&self, px: f64, py: f64) -> f64 {
    let x = (px as usize).min(self.map.width - 1);
    let y = (py as usize).min(self.map.height - 1);
    self.heights[y * self.map.width + x]
}
```

### Sight Range Bonus

The `Creature` component in `src/ecs/components.rs` has a `sight_range: f64` field (base value, typically ~22.0 for villagers). Elevation modifies the effective sight range at query time rather than mutating the stored value.

```
effective_sight_range = base_sight_range + elevation_sight_bonus(elevation)
```

The bonus function:

```
elevation_sight_bonus(h) = max(0, (h - baseline) * sight_scale)

baseline    = 0.3    // below this elevation, no bonus (valleys, plains)
sight_scale = 15.0   // tuning knob
```

| Elevation | Bonus | Effective Range (base 22) |
|-----------|-------|---------------------------|
| 0.2 (valley) | 0.0 | 22.0 |
| 0.3 (plains) | 0.0 | 22.0 |
| 0.5 (hills) | +3.0 | 25.0 |
| 0.7 (high hills) | +6.0 | 28.0 |
| 0.9 (mountain) | +9.0 | 31.0 |

A hilltop garrison sees threats ~6 tiles further out than a valley garrison. That is 30-60 extra ticks of warning depending on threat movement speed -- enough time to matter, not enough to trivialize defense.

**Where this applies:**
- Villager AI: threat detection range in `ai_villager()` / `ai.rs`.
- Garrison detection: the radius at which garrisons "spot" incoming threats for event log warnings.
- Explorer vision: tiles revealed by exploring villagers use effective sight range.

### Defense Bonus

Elevation provides a defense bonus to garrison buildings, extending `compute_defense_rating()` in `src/game/build.rs`. This integrates with the geographic defense terms proposed in `docs/design/threat_scaling.md`.

#### Per-Garrison Elevation Defense

Each garrison building gets an individual elevation bonus based on its tile height:

```
garrison_elevation_bonus(h) = max(0, (h - 0.3) * 5.0)
```

Capped at 4.0 per garrison. A garrison at elevation 0.7 gets +2.0 defense. A garrison at elevation 0.9 gets +3.0.

#### Settlement-Wide Elevation Defense

The settlement as a whole benefits from holding high ground, computed as the height advantage of the settlement center over the average surrounding terrain:

```
settlement_elevation = height at settlement_center()
surrounding_elevation = average height of tiles at radius 25-35 from center
elevation_defense = max(0, (settlement_elevation - surrounding_elevation) * 8.0)
```

Capped at 5.0. This is the same term referenced in `threat_scaling.md` (section "Elevation Advantage"), now fully specified.

#### Updated Defense Rating

```
defense_rating = garrison_defense              // existing: sum of garrison.defense_bonus
               + wall_defense                  // existing: wall_tiles * 0.5
               + military_skill_bonus          // existing: skills.military * 0.2
               + river_defense                 // from threat_scaling.md
               + mountain_defense              // from threat_scaling.md
               + settlement_elevation_defense  // NEW: high-ground settlement bonus
               + garrison_elevation_bonus_sum  // NEW: sum of per-garrison height bonuses
               + chokepoint_bonus              // from threat_scaling.md
```

### Uphill Attack Penalty

When threats attack a settlement on higher ground, their effective strength is reduced. This is the attacker-side mirror of the defender elevation bonus.

```
height_difference = settlement_elevation - threat_spawn_elevation
uphill_penalty = max(0, height_difference * 0.15)
effective_raid_size = ceil(raid_size * (1.0 - uphill_penalty))
```

A raid spawning at elevation 0.2 attacking a settlement at elevation 0.6 loses `0.4 * 0.15 = 6%` of its effective strength. Combined with the garrison elevation defense bonus, hilltop settlements get a meaningful but not overwhelming advantage.

This stacks with the river crossing attrition from `threat_scaling.md`. A settlement on a hill behind a river is genuinely hard to crack.

### The Trade-Off: Hills vs. Valleys

The system creates a real strategic tension because the best defensive terrain is the worst farming terrain:

| Factor | Hilltop (h > 0.6) | Valley/River Plain (h < 0.35) |
|--------|-------------------|-------------------------------|
| Sight range | +4 to +9 bonus | No bonus |
| Garrison defense | +1.5 to +3.0 per garrison | No bonus |
| Settlement defense | Up to +5.0 | No bonus |
| Soil fertility | Low (thin soil, less moisture) | High (alluvial deposits, water proximity) |
| Water access | Far from rivers | Adjacent to rivers |
| Farm yield | Poor | Excellent |
| Forest access | Sparse at altitude | Dense in lowlands |
| Threat exposure | Fewer approach vectors | Open from multiple directions |

**The meaningful choice:** A hilltop settlement is safe but hungry. A valley settlement is fed but vulnerable. The best strategy uses both -- farms in the valley, garrison on the overlooking hill -- which creates the spatial spread that makes settlements interesting and different per seed.

### Elevation Overlay

Add elevation advantage visualization to the existing overlay system (`OverlayMode` in `src/game/render.rs`). This could be a layer on the existing Terrain or Threats overlay rather than a standalone mode:

- Tiles with sight range bonus: highlighted with intensity proportional to bonus.
- Tiles with defense bonus: marked on Threats overlay alongside chokepoints.
- Suggested garrison positions: hilltop tiles near chokepoints glow on the Threats overlay.

The player should be able to glance at the overlay and think "that hill overlooking the northern pass is where my garrison goes."

## Implementation Plan

### Phase 1: Elevation Query Helper

Add a `tile_elevation(x, y) -> f64` helper method to `Game`. This wraps the raw heights array lookup with bounds checking and is the single access point for all elevation-based gameplay.

**Files:** `src/game/mod.rs`

### Phase 2: Sight Range Bonus

- Add `fn effective_sight_range(&self, creature: &Creature, px: f64, py: f64) -> f64` to `Game` that returns base `sight_range` plus elevation bonus.
- Replace direct `creature.sight_range` reads in AI systems with calls to the effective function.
- Update `ai_villager` threat detection to use effective sight range.

**Files:** `src/game/mod.rs`, `src/ecs/ai.rs`, `src/ecs/systems.rs`

### Phase 3: Defense Bonus

- Extend `compute_defense_rating()` in `src/game/build.rs` to query garrison entity positions, look up their tile elevation, and add per-garrison elevation bonus.
- Add settlement-wide elevation defense term (settlement center height vs. surrounding average).
- Cap both terms per the design above.

**Files:** `src/game/build.rs`

### Phase 4: Uphill Attack Penalty

- In raid resolution (currently `src/game/events.rs`), compute height difference between threat spawn point and settlement center.
- Apply uphill penalty to effective raid size before comparing against defense rating.

**Files:** `src/game/events.rs`

### Phase 5: Overlay Integration

- Add elevation advantage data to Threats overlay rendering.
- Highlight high-defense-value tiles (hilltops near chokepoints).

**Files:** `src/game/render.rs`

## Tuning Levers

All constants are candidates for extraction into a config struct:

| Parameter | Default | Effect |
|-----------|---------|--------|
| `sight_baseline` | 0.3 | Elevation below which no sight bonus applies |
| `sight_scale` | 15.0 | Sight bonus per unit elevation above baseline |
| `defense_per_garrison_scale` | 5.0 | Per-garrison defense bonus per unit elevation |
| `defense_per_garrison_cap` | 4.0 | Max defense bonus per single garrison |
| `settlement_defense_scale` | 8.0 | Settlement-wide defense per unit height advantage |
| `settlement_defense_cap` | 5.0 | Max settlement-wide elevation defense |
| `uphill_penalty_scale` | 0.15 | Raid size reduction per unit height disadvantage |
| `surrounding_sample_radius` | 25-35 | Ring radius for computing average surrounding elevation |

Start with these defaults. Tune by running seeds with hilltop vs. valley settlements and comparing survival rates over 10K ticks. The target: hilltop settlements survive ~30% longer against equivalent threats, but produce ~25% less food.

## Testing Strategy

**Unit tests:**
- `tile_elevation()` returns correct values for known positions and handles out-of-bounds.
- `elevation_sight_bonus()` returns 0 below baseline, scales linearly above.
- `effective_sight_range()` matches base + bonus for various elevations.
- Per-garrison elevation defense bonus computes correctly for garrisons at known heights.
- Settlement elevation defense computes correctly when center is above/below/equal to surroundings.
- Uphill penalty reduces effective raid size proportionally.
- All bonuses respect their caps.

**Integration tests:**
- Spawn a garrison at a high-elevation tile and one at a low-elevation tile; verify the high garrison contributes more defense.
- Run 1000-tick simulation on a seed with hilltop settlement center; verify defense rating includes elevation terms.
- Verify that villagers on high ground detect threats further away (wolf enters effective sight range sooner).
- Verify that a raid attacking uphill has reduced effective strength compared to the same raid on flat terrain.

**Regression tests:**
- Flat maps (all heights ~0.5) should produce negligible elevation bonuses (everything is at the same height).
- Elevation bonuses should never be negative.
- Base sight range must never decrease due to elevation (bonus is floored at 0).

## Open Questions

1. **Should elevation affect villager movement speed?** Uphill slower, downhill faster. Intuitive and realistic, but adds complexity to pathfinding costs. Probably a separate design doc -- it interacts with A* cost tables.

2. **Should elevation affect farm yield directly, or is the indirect effect (distance from water, soil type) sufficient?** The terrain pipeline already produces lower moisture and thinner soil at high elevations. If that naturally suppresses farm yield enough, we do not need an explicit elevation penalty on farming. Test first.

3. **Should the sight range bonus apply to wolves and raiders too?** Wolves spawning on a ridge above the settlement could see further and path more aggressively. This cuts both ways -- it makes high-ground threat spawns more dangerous but also more detectable. Leaning toward yes for consistency, but start with player-side only.

4. **Garrison placement AI.** The auto-build system should prefer high-elevation tiles near chokepoints for garrison placement. This interacts with `threat_scaling.md` Phase 4 (chokepoint detection). Should this doc specify the preference heuristic, or defer to the auto-build design? Leaning toward: add an `elevation_score` term to the garrison site-scoring function in `build.rs`.

## References

- `src/terrain_pipeline.rs` -- `PipelineResult::heights`, terrain generation pipeline
- `src/ecs/components.rs` -- `Creature::sight_range`, `GarrisonBuilding::defense_bonus`
- `src/game/build.rs` -- `compute_defense_rating()`, `settlement_center()`
- `src/game/events.rs` -- raid resolution, wolf surge spawning
- `src/ecs/ai.rs` -- `ai_villager()`, threat detection logic
- `docs/game_design.md` -- Pillar 1 (geography shapes everything), Pillar 3 (Endure phase)
- `docs/design/threat_scaling.md` -- geographic defense terms, elevation_defense placeholder
