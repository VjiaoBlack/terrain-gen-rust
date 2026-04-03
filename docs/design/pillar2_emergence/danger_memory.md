# Danger Memory

**Status:** Proposed
**Pillar:** 2 (Emergent Complexity from Simple Agents)
**Phase:** Foundation (Step 4 of per-villager memory migration)
**Depends on:** Per-villager memory (`VillagerMemory`, `MemoryEntry`, `MemoryKind::DangerZone`)
**Enables:** Emergent safe/danger zones, geography-aware avoidance, wolf territory as lived knowledge

---

## Problem

Villagers treat every tile equally when choosing gather targets and exploration directions. A villager who just watched three companions get killed by wolves at the eastern forest edge will walk right back there next tick if that forest scores highest for wood. There is no learning from trauma, no spatial fear, no collective avoidance behavior.

The `MemoryKind::DangerZone` variant exists in the per-villager memory design but is only sketched as a soft penalty (`danger_penalty = 0.3`) on resource scoring. That is not enough to produce visible "danger zones" on the map or meaningful routing changes. Wolves kill villagers in the same spot repeatedly because nobody remembers the spot is deadly.

This also misses a key emergent behavior from the game design doc: "wolf territory becomes known and villagers route around it." For that to happen, danger memory needs to affect pathfinding, not just target selection.

## Goals

1. Villagers who witness predator encounters remember the location with high urgency.
2. Danger memories create an avoidance radius that affects both target selection and pathfinding.
3. Danger memories decay over time so zones reopen naturally (wolves move on, seasons change).
4. Villagers who never personally saw the danger are unaffected (Layer 2 only -- no global "danger map").
5. Observable behavior: after a wolf attack, survivors visibly detour around the attack site for hundreds of ticks.

## Non-Goals

- Shared danger knowledge via bulletin board (Layer 3 -- separate design, builds on this).
- Predator territorial AI (wolves don't "claim" zones; danger zones are purely villager perception).
- Permanent no-go zones. All danger fades eventually.
- Combat changes. This is about avoidance, not fighting.

## Design

### Danger Entry Structure

Danger memories use the existing `MemoryEntry` with `MemoryKind::DangerZone`, but with different confidence dynamics than resource memories.

```rust
// When a villager observes a predator within sight range:
memory.upsert(MemoryKind::DangerZone, predator_x, predator_y, tick);
```

The key difference from resource observations: **danger entries start at confidence 1.0 and use a slower decay rate.** Fear lasts longer than knowledge of where trees are.

### Danger-Specific Constants

```rust
const DANGER_DECAY_RATE: f32 = 0.001;        // half the normal 0.002 — fear fades slowly
const DANGER_AVOIDANCE_RADIUS: f64 = 12.0;   // tiles around remembered danger to avoid
const DANGER_PATHFINDING_RADIUS: f64 = 8.0;  // smaller radius for hard pathfinding penalty
const DANGER_CONFIDENCE_BOOST: f32 = 0.5;    // bonus confidence on re-observation (stacks to 1.0)
const DANGER_FORGET_THRESHOLD: f32 = 0.05;   // same as general memory
const DANGER_TARGET_PENALTY: f32 = 0.5;      // score penalty when choosing gather targets
const DANGER_PATHFINDING_COST: f32 = 6.0;    // A* cost multiplier for tiles in danger zone
```

At `DANGER_DECAY_RATE = 0.001`, a danger memory at confidence 1.0 takes ~1000 ticks to reach 0.0. With the forget threshold at 0.05, it persists for ~950 ticks. At 5x speed that is roughly 3 in-game minutes of avoidance -- long enough to be visible, short enough to not permanently block map areas.

### Observation: When Danger Entries Are Created

A danger entry is created or refreshed when any of these occur during the per-tick observation pass in `system_update_memories`:

1. **Predator sighting.** A villager sees a predator (wolf, future: bear, bandit) within sight range. Entry placed at the predator's position.

2. **Witnessing a kill.** A villager within sight range of a villager death caused by a predator. The dying villager's position is recorded as a danger zone. This is the strongest signal -- confidence 1.0 and the `DANGER_CONFIDENCE_BOOST` is applied, effectively making it a "reinforced" memory.

3. **Fleeing.** A villager who transitions to `VillagerBehavior::Fleeing` from a predator records a danger entry at their current position (where the flight started). This captures "I was chased here."

```rust
// In the flee trigger (ai.rs, when predator detected and villager starts fleeing):
if transitioning_to_flee {
    memory.upsert(MemoryKind::DangerZone, self_x, self_y, tick);
    // Also record where the predator was
    memory.upsert(MemoryKind::DangerZone, predator_x, predator_y, tick);
}
```

Multiple sightings in the same area reinforce the memory. The upsert logic (existing: merge entries within 5 tiles) refreshes confidence to 1.0 and updates the tick. A wolf that patrols the same forest edge for 200 ticks will produce a very persistent danger memory in any villager who keeps seeing it.

### Avoidance: Target Selection

When a villager evaluates gather targets using `memory.best_resource()`, danger zones penalize nearby targets. This extends the existing sketch from the per-villager memory doc with a stronger, distance-scaled penalty:

```rust
fn score_resource(&self, entry: &MemoryEntry, from_x: f64, from_y: f64) -> f32 {
    let base_score = entry.confidence - (dist(from_x, from_y, entry.x, entry.y) / 100.0);

    // Check all DangerZone memories against this target location
    let danger_penalty: f32 = self.entries.iter()
        .filter(|e| e.kind == MemoryKind::DangerZone)
        .map(|danger| {
            let d = dist(entry.x, entry.y, danger.x, danger.y);
            if d < DANGER_AVOIDANCE_RADIUS {
                // Penalty scales with danger confidence and proximity
                // Close to a fresh danger: full penalty
                // Far from a fading danger: tiny penalty
                danger.confidence * DANGER_TARGET_PENALTY * (1.0 - d / DANGER_AVOIDANCE_RADIUS)
            } else {
                0.0
            }
        })
        .sum::<f32>()
        .min(0.8); // cap so danger never makes score go impossibly negative

    base_score - danger_penalty
}
```

The effect: a forest 10 tiles from a remembered wolf attack (confidence 0.8) gets a penalty of roughly `0.8 * 0.5 * (1.0 - 10/12) = 0.067`. A forest 3 tiles from a fresh attack (confidence 1.0) gets `1.0 * 0.5 * (1.0 - 3/12) = 0.375`. That 0.375 penalty is enough to make a villager choose a forest that is 37 tiles further away, which produces visible rerouting.

### Avoidance: Pathfinding Cost

Target selection alone is not enough. A villager heading to a "safe" target might path directly through a danger zone. The A* pathfinding cost must also account for danger.

This requires passing danger zone information into the pathfinding function. Since danger zones are per-villager (not global), the cost function becomes villager-specific:

```rust
/// Compute A* tile cost, incorporating this villager's danger memories.
fn tile_cost_with_danger(
    terrain: Terrain,
    tile_x: f64,
    tile_y: f64,
    danger_zones: &[(f64, f64, f32)],  // (x, y, confidence) from villager's DangerZone entries
) -> f32 {
    let base_cost = terrain.movement_cost();  // existing: 1.0 for grass, 1.7 for forest, etc.

    let danger_multiplier: f32 = danger_zones.iter()
        .map(|&(dx, dy, conf)| {
            let d = dist(tile_x, tile_y, dx, dy);
            if d < DANGER_PATHFINDING_RADIUS {
                // Closer to danger center = higher cost
                // At distance 0: full multiplier. At radius edge: 1.0 (no effect).
                let proximity = 1.0 - (d / DANGER_PATHFINDING_RADIUS);
                conf * (DANGER_PATHFINDING_COST - 1.0) * proximity + 1.0
            } else {
                1.0
            }
        })
        .fold(1.0_f32, |acc, m| acc.max(m)); // take the worst (highest) multiplier

    base_cost * danger_multiplier
}
```

At the center of a fresh danger zone (confidence 1.0, distance 0): cost multiplier is `1.0 * (6.0 - 1.0) * 1.0 + 1.0 = 6.0`. A grass tile that normally costs 1.0 now costs 6.0 -- equivalent to traversing 6 grass tiles. A* will route around unless the detour is longer than 6 tiles.

At the edge of the radius (distance ~8 tiles): multiplier approaches 1.0. Smooth falloff, no hard boundary.

At confidence 0.3 (fading memory): center multiplier is `0.3 * 5.0 * 1.0 + 1.0 = 2.5`. Much weaker -- villagers start cutting through fading danger zones when the detour would be long.

### Performance: Extracting Danger Zones for Pathfinding

Running per-villager A* with per-villager danger zones sounds expensive. The key optimization: extract danger zones once per villager per pathfind call, not per tile evaluation.

```rust
// Before calling A*, extract this villager's danger zones
let danger_zones: Vec<(f64, f64, f32)> = memory.entries.iter()
    .filter(|e| e.kind == MemoryKind::DangerZone && e.confidence > DANGER_FORGET_THRESHOLD)
    .map(|e| (e.x, e.y, e.confidence))
    .collect();

// Pass to pathfinder
let path = astar_with_danger(start, goal, map, &danger_zones);
```

With `MEMORY_CAPACITY = 32`, a villager has at most 32 entries, and typically 0-4 danger entries. The inner loop in `tile_cost_with_danger` iterates over 0-4 entries per tile -- negligible compared to the A* expansion itself.

For path caching (see `path_caching.md`): cached paths must be invalidated when a new danger zone is added or an existing one's confidence changes significantly. A simple approach: store a `danger_hash` (sum of danger entry confidence values, quantized to 0.1) with the cached path. If it changes, recompute.

### Danger Zone Overlap and Reinforcement

Multiple danger entries near each other create a stronger zone. If a wolf pack attacks at (50, 30) and a survivor also records their flee-start at (48, 32), the overlapping radii create a larger avoidance area. This is natural -- the `fold(max)` in pathfinding takes the worst multiplier from any single entry, and the `sum` in target scoring accumulates penalties.

A wolf that patrols back and forth between (50, 30) and (55, 35) over 100 ticks creates two overlapping danger zones (or one that keeps getting upserted) covering roughly a 24-tile-wide swath. Any villager who witnessed this will route well around the whole area.

### Decay and Zone Reopening

Danger zones reopen naturally as confidence decays:

| Ticks since sighting | Confidence | Pathfinding multiplier (center) | Avoidance radius (effective) |
|---------------------|------------|--------------------------------|------------------------------|
| 0 | 1.0 | 6.0x | 12 tiles |
| 250 | 0.75 | 4.75x | ~9 tiles (penalty too small beyond) |
| 500 | 0.50 | 3.5x | ~6 tiles |
| 750 | 0.25 | 2.25x | ~3 tiles |
| 950 | 0.05 | ~1.0x (forgotten) | 0 tiles |

The zone shrinks as it fades. First the outer edges become traversable (villagers cut corners), then the core opens up. This creates a visible "thawing" effect -- villagers gradually reclaim territory as fear fades.

If wolves return before the memory fades, confidence resets to 1.0. Persistent wolf presence means persistent avoidance. Wolves that leave allow recovery. This is exactly the "wolf territory becomes known" behavior from the game design doc.

### Emergent Behaviors

**Safe corridors.** If wolves inhabit forests to the north and east, villagers with danger memories will naturally converge on southern and western routes. Without any "safe zone" system, the settlement develops preferred paths that avoid known wolf territory. Traffic maps will show this -- worn paths curve around danger zones.

**Danger asymmetry between villagers.** A villager who was attacked in the eastern forest avoids it. A newly spawned villager who has never been there walks right in. This is correct -- the new villager has no fear because they have no experience. If they survive, they too will learn. If they don't, the settlement loses a villager who might have discovered something useful.

**Seasonal danger shifts.** In winter, wolves are more aggressive (threat_scaling.md: 1.5x wolf chance) and push closer to the settlement. Villagers accumulate more danger memories in winter. In spring, wolves recede, danger memories from winter fade, and villagers reclaim territory. This creates a visible seasonal "breathing" of the settlement's safe zone.

**Deforestation as defense.** If villagers cut down the forest where wolves spawn, two things happen: (1) the forest cluster shrinks, pushing wolf spawns further away (threat_scaling.md), and (2) the open ground has no cover, making wolves visible earlier. Danger memories in deforested areas fade because no new sightings occur. The player sees formerly dangerous land become safe through exploitation.

**Knowledge death.** If the only villager who knew about the eastern wolves dies, that danger zone knowledge is lost. The next villager to wander east will be surprised. This creates value in keeping experienced villagers alive and motivates garrison placement.

### Interaction with Layer 3 (Future)

When the stockpile bulletin board system is implemented (see `stockpile_bulletin_board.md`), danger memories become shareable:

- A villager deposits resources at the stockpile and also "reports" their highest-confidence DangerZone entries.
- Other villagers visiting the stockpile can pick up these reports as lower-confidence copies (e.g., confidence * 0.6 -- secondhand fear is weaker than personal experience).
- This creates settlement-wide danger awareness that spreads through the stockpile hub, not through global state.

This is not part of the current design but the `MemoryEntry` structure supports it without modification.

## Implementation Plan

### Step 1: Danger observation triggers

- In `system_update_memories`: predator-in-sight-range already creates DangerZone entries. Verify this works.
- Add flee-trigger recording: when a villager transitions to `Fleeing`, record DangerZone at own position and predator position.
- Add kill-witness recording: when a villager death is processed and another villager is within sight range, record DangerZone at the death location with confidence boost.
- Use `DANGER_DECAY_RATE` (0.001) for DangerZone entries instead of the general `MEMORY_DECAY_RATE` (0.002). This requires a small change to `decay_tick()` to check entry kind.

**Files:** `src/ecs/systems.rs` (memory update), `src/ecs/ai.rs` (flee trigger), `src/ecs/components.rs` (if adding decay rate per kind)

### Step 2: Danger-aware target selection

- Modify `score_resource()` (or `best_resource()`) in `VillagerMemory` to apply the danger proximity penalty described above.
- Replace the flat `0.3` penalty from the per-villager memory doc with the distance-scaled formula.
- Test: after a wolf attack, villagers prefer resource targets away from the attack site.

**Files:** `src/ecs/components.rs` (VillagerMemory methods), `src/ecs/ai.rs` (gather target selection)

### Step 3: Danger-aware pathfinding

- Add `tile_cost_with_danger()` function that wraps the existing terrain cost with danger multiplier.
- Modify the A* caller to extract danger zones from the requesting villager's memory and pass them in.
- If path caching is implemented: add danger hash to cache key for invalidation.
- Test: villagers visibly route around danger zones instead of walking through them.

**Files:** `src/ecs/ai.rs` (pathfinding calls), `src/ecs/systems.rs` (A* cost function or wrapper)

### Step 4: Debug overlay

- Add danger zone visualization to the Threats overlay (`OverlayMode::Threats`). For a selected villager (via `k` inspect), show their personal danger zones as colored circles on the map.
- Alternatively, aggregate all villagers' danger zones into a heat map: tiles that many villagers remember as dangerous glow red. Tiles only one villager fears glow dim.

**Files:** `src/game/render.rs`

## Testing Strategy

**Unit tests:**
- `DangerZone` entry decays at `DANGER_DECAY_RATE`, not `MEMORY_DECAY_RATE`.
- `score_resource()` penalizes targets within `DANGER_AVOIDANCE_RADIUS` of a danger entry.
- Penalty scales with confidence and distance (closer + fresher = bigger penalty).
- `tile_cost_with_danger()` returns base cost when no danger zones exist.
- `tile_cost_with_danger()` returns `base_cost * 6.0` at center of a fresh danger zone.
- `tile_cost_with_danger()` returns `base_cost * 1.0` outside `DANGER_PATHFINDING_RADIUS`.
- Danger entries are evictable (not pinned like HomeLocation).

**Integration tests:**
- Run 500-tick simulation with wolves attacking from the east. After attack, villagers with danger memories choose western resources over equally-scored eastern ones.
- Verify danger memories decay: after 1000 ticks without new sightings, danger entries are forgotten and villagers resume eastern gathering.
- Verify new villagers (spawned after the attack) have no danger memories and path through the danger zone freely.
- Run simulation where the only path to a resource goes through a danger zone. Villager should still reach the resource (penalty makes it expensive, not impassable).
- Verify danger memory does not prevent fleeing villagers from moving (flee behavior ignores pathfinding cost modifications).

**Regression tests:**
- Villagers still gather resources on maps with no predators (no danger entries = no penalty).
- A* performance with 4 danger entries per villager stays within tick budget.
- Memory capacity of 32 is not exhausted by danger entries alone (upsert merges nearby entries).

## Performance Budget

| Component | Per villager | 30 villagers | 500 villagers |
|-----------|-------------|-------------|---------------|
| Danger observation (in existing memory scan) | ~0 extra | ~0 extra | ~0 extra |
| Flee/kill-witness recording | ~0.1 us (rare event) | ~0 | ~0 |
| Danger-aware target scoring (4 danger entries * ~5 candidates) | ~0.2 us | ~6 us | ~100 us |
| Danger-aware A* (4 danger entries checked per tile expanded, ~200 tiles) | ~2 us | ~60 us | ~1 ms |
| **Total per tick** | | **~66 us** | **~1.1 ms** |

The A* danger check is the bottleneck, but it only runs when a villager needs a new path (not every tick due to path caching). In practice, maybe 10-20% of villagers pathfind per tick, so the real cost is 5-10x lower than the worst case above. Well within Pillar 5 budgets.

## Open Questions

1. **Should danger affect exploration direction?** Currently, explorers pick a random direction. Should they avoid exploring toward remembered danger? Leaning yes -- explorers should prefer unexplored directions that are also safe. But this might trap villagers in "known safe" areas and prevent rediscovery.

2. **Danger from near-miss vs. actual attack?** A villager who sees a wolf at max sight range (22 tiles away, never chased) gets the same DangerZone entry as one who was actively fleeing. Should near-miss sightings create weaker entries (confidence 0.5 instead of 1.0)? This would make active encounters more impactful.

3. **Multiple predator types.** When raiders are added (threat_scaling.md), should raider sightings also create DangerZone entries? Or a separate `MemoryKind::RaiderDanger`? Raiders use roads; wolves use forests. Different avoidance patterns might warrant different memory kinds.

4. **Villager courage / personality.** Should some villagers be braver (lower `DANGER_TARGET_PENALTY`, faster `DANGER_DECAY_RATE`)? This adds per-villager variation and could create emergent "scouts" who go where others won't. But it conflicts with Pillar 5 (scale over fidelity) -- personality per villager is complexity we might not want.

5. **Danger as motivation for garrison placement.** The auto-build system could read aggregate danger zones to decide where to place garrisons. "Villagers keep getting attacked from the northeast" -> build garrison to the northeast. This is compelling but requires Layer 3 aggregation (settlement-level knowledge of individual danger memories).

## References

- `docs/design/per_villager_memory.md` -- VillagerMemory, MemoryEntry, MemoryKind::DangerZone, decay system
- `docs/design/threat_scaling.md` -- wolf spawn from forests, threat tiers, seasonal modifiers
- `docs/game_design.md` -- Pillar 2 (Layer 2 danger memory), Pillar 4 (observable zones)
- `src/ecs/ai.rs` -- `ai_predator`, `ai_villager` flee behavior
- `src/ecs/systems.rs` -- death processing, A* pathfinding
- `src/ecs/components.rs` -- VillagerMemory, MemoryEntry, VillagerBehavior::Fleeing
- `docs/design/path_caching.md` -- cache invalidation considerations
- `docs/design/stockpile_bulletin_board.md` -- future Layer 3 sharing of danger info
