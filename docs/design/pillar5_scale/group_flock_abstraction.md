# Design: Group/Flock Abstraction

**Status:** Proposed
**Pillars:** 5 (Scale Over Fidelity), 2 (Emergent Complexity), 4 (Observable Simulation)
**Phase:** Phase 3 (Scale)
**Depends on:** Spatial hash grid, tick budgeting
**Unlocks:** 500+ villager performance, group-level pathfinding, group-level knowledge sharing, army-style coordination

## Problem

At 500+ population, even with tick budgeting and spatial hash grids, there are scenarios where many villagers independently compute redundant work. Ten farmers in the same field each run their own AI evaluation, each pathfind to the same stockpile, each do their own predator-nearby check -- arriving at identical conclusions. Five builders at a construction site each independently decide "keep building," each scan for the nearest stone deposit, each check for threats from the same direction.

This is wasted computation that scales linearly with cluster size. Worse, it produces visually incoherent behavior: 10 farmers pathfinding independently to the same stockpile don't walk together -- they fan out along slightly different A* paths, jitter around each other, and look like a broken simulation rather than a coordinated workforce.

### The numbers

Assume 500 villagers. A mature settlement concentrates activity:

| Activity cluster | Typical count | AI evals/tick (individual) | Redundant work |
|-----------------|---------------|---------------------------|----------------|
| Farmers in a field | 8-15 | 8-15 (Active priority) | Same threat check, same stockpile path, same harvest timing |
| Builders at a site | 3-8 | 3-8 (Active priority) | Same threat check, same resource seek, same site evaluation |
| Gatherers at a forest | 5-10 | 5-10 (Normal priority) | Same pathfind to trees, same return route |
| Sleepers in huts | 20-40 | 3-6 (Dormant priority) | Already cheap via tick budgeting, but group wakeup is still individual |
| Haulers on a road | 5-15 | 5-15 (Active priority) | Same route, same destination, same threat scan |

Groups of 5-15 entities doing identical work in the same area account for 60-70% of the active population. Simulating them as a single unit for decision-making (while keeping individual positions for rendering) is a 3-5x reduction on top of tick budgeting.

## Solution: Lightweight Group Abstraction

A **group** is a temporary, runtime-only structure that clusters entities sharing the same activity in the same spatial area. The group gets one AI evaluation per tick interval; members inherit the group's decision. Individual entities keep their own position, rendering, and can leave the group at any time.

This is similar to They Are Billions' army grouping: selected units move as a formation with one pathfind call, but each unit is rendered individually and can be split off. The difference is that our groups form and dissolve automatically based on simulation state -- no player input required.

### What a group is NOT

- Not a permanent social structure (no "clans" or "teams").
- Not a player-facing concept (no group selection UI).
- Not serialized to save files (groups are recomputed from entity state on load).
- Not an entity in the ECS (it's a side structure, like the spatial hash grid).

## Group Formation Criteria

A group forms when **all four conditions** are met:

### 1. Same activity type

Entities must share the same `BehaviorState` variant (ignoring timer/position fields):

| Groupable states | Group type |
|-----------------|------------|
| `Farming { .. }` | Farm crew |
| `Building { .. }` | Build crew |
| `Gathering { resource_type, .. }` | Gather crew (per resource type) |
| `Hauling { resource_type, .. }` with same target stockpile | Haul column |
| `Working { .. }` at same building | Workshop shift |
| `Sleeping { .. }` in same hut cluster | Dormitory (low priority, mostly for knowledge sharing) |
| `Exploring { .. }` in same direction | Scout party |

States that are NOT groupable: `Wander`, `Idle`, `Seek` (too heterogeneous in destination), `FleeHome` (individual panic behavior -- grouping would look wrong), `Hunting`, `Captured`, `Eating`.

### 2. Spatial proximity

All members must be within a **group radius** of the group's centroid. The radius depends on activity type:

| Group type | Radius (tiles) | Rationale |
|-----------|----------------|-----------|
| Farm crew | 8 | Farms are ~4x4; a field of farms spans ~8 tiles |
| Build crew | 4 | Build sites are single tiles; builders cluster tightly |
| Gather crew | 10 | Forest/stone areas are larger, gatherers spread out more |
| Haul column | 6 | Haulers on the same road are roughly in a line |
| Workshop shift | 3 | Workshops are single buildings |
| Dormitory | 8 | Hut clusters span a small area |
| Scout party | 12 | Explorers heading the same direction can be loosely grouped |

### 3. Minimum size

A group requires at least **3 members**. Below that, the overhead of group management exceeds the savings from shared computation. Two villagers farming near each other just run individually.

### 4. Shared context

Members must share the same operational context. Specifically:

- **Same target zone**: Farmers targeting the same field (within radius). Builders targeting the same build site. Haulers heading to the same stockpile.
- **Same resource type**: Gatherers collecting wood are a different group from gatherers collecting stone, even if spatially overlapping.
- **Same threat exposure**: If one member is fleeing and another is farming, they cannot be in the same group. (The fleeing member leaves the group -- see Dissolution below.)

## Group Data Structure

```rust
/// A temporary group of entities performing the same activity in the same area.
/// Not an ECS entity. Lives in a side structure rebuilt periodically.
pub struct Group {
    /// Unique ID for this tick's group set. Not stable across ticks.
    pub id: u32,
    /// The shared activity type (discriminant only, no per-entity fields).
    pub activity: GroupActivity,
    /// Centroid position, updated when membership changes.
    pub centroid_x: f64,
    pub centroid_y: f64,
    /// Member entity IDs.
    pub members: Vec<Entity>,
    /// Shared pathfinding result: waypoints from centroid to target.
    /// Members offset from these waypoints by their individual positions.
    pub shared_path: Option<Vec<(f64, f64)>>,
    /// Shared threat assessment: is there a predator near the group?
    pub threat_nearby: bool,
    /// Shared knowledge: resource locations discovered by any member.
    /// Propagated to all members on group dissolution.
    pub shared_knowledge: Vec<KnowledgeEntry>,
    /// The tick this group was formed. Used for staleness checks.
    pub formed_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupActivity {
    Farming,
    Building,
    GatheringWood,
    GatheringStone,
    GatheringFood,
    Hauling,
    Working,
    Sleeping,
    Exploring,
}
```

On each entity, a lightweight marker indicates group membership:

```rust
/// Added to entities currently in a group. Not serialized.
pub struct GroupMembership {
    pub group_id: u32,
    /// Offset from group centroid (preserves individual position spread).
    pub offset_x: f64,
    pub offset_y: f64,
}
```

## Group Detection Algorithm

Groups are recomputed periodically, not every tick. Frequency: **every 8-16 ticks** (tunable). Between recomputations, existing groups persist unless an interrupt dissolves them.

The detection runs after the spatial hash grid is populated:

```
fn detect_groups(grid: &SpatialHashGrid, world: &World, current_tick: u64) -> Vec<Group> {
    // 1. For each spatial cell, bucket entities by GroupActivity.
    //    An entity maps to a GroupActivity based on its BehaviorState variant.
    //    Entities in non-groupable states are skipped.
    
    // 2. For each (cell, activity) bucket with >= 3 entities:
    //    - Compute centroid of the bucket.
    //    - Filter out entities farther than the activity's group radius from centroid.
    //    - If >= 3 remain, form a Group.
    
    // 3. For adjacent cells with the same activity, merge groups whose
    //    centroids are within 1.5x the group radius. This handles clusters
    //    that straddle cell boundaries.
    
    // 4. Assign GroupMembership to each member entity.
    //    offset = entity position - group centroid.
}
```

The spatial hash grid already buckets entities by cell (16x16 tiles). Since group radii are 3-12 tiles, most groups fit within one or two cells. The detection pass iterates occupied cells, not all entities -- cost is proportional to active cells, not population.

### Complexity

- Step 1: O(entities in occupied cells) -- same entities already in the grid.
- Step 2: O(bucket_size) per bucket, with a centroid + distance filter.
- Step 3: O(adjacent_cell_pairs * groups_per_cell) -- small, most cells have 0-1 groups.
- Total: O(active_entities) with a small constant. At 500 pop, ~300 are in groupable states, spread across ~20-40 cells. Detection cost is negligible.

## Per-Group vs Per-Individual Simulation

This is the core design decision. Getting the split wrong either wastes performance (simulating too much per-individual) or creates visible artifacts (simulating too little).

### Simulated per-GROUP (one evaluation shared by all members)

| System | What the group computes | Why group-level is sufficient |
|--------|------------------------|------------------------------|
| **Threat detection** | `grid.any_within(centroid, threat_range, PREDATOR)` | A predator threatening one farmer threatens them all. One check for 10 farmers. |
| **Pathfinding to target** | A* from centroid to shared destination (stockpile, resource site) | Members walk the same general route. Individual offsets preserve visual spread. |
| **Resource scanning** | `grid.nearest(centroid, sight_range, FOOD_SOURCE)` | Farmers in the same field see the same food sources. One scan for all. |
| **Decision-making** | "Should we keep farming or haul to stockpile?" | The answer is the same for all members in the same situation. |
| **Knowledge aggregation** | Union of all members' known resource locations | Any member's discovery benefits all. This is the ant-colony info-sharing from Pillar 2. |
| **Tick scheduling** | One `TickPriority` for the group, applied to all members | All members are in the same state and area; they share a priority category. |

### Simulated per-INDIVIDUAL (every member, every tick)

| System | What each entity computes | Why individual is necessary |
|--------|--------------------------|---------------------------|
| **Movement/position** | Position update from velocity via `system_movement` | Entities must have distinct positions or they stack on one tile. |
| **Rendering** | Glyph, color, visibility | Pillar 4: individual rendering is non-negotiable. The player sees 10 farmers, not one "farm group" icon. |
| **Hunger** | `system_hunger` decrements per entity | Hunger is individual state. One villager might be starving while others are fine (arrived later, ate less). |
| **Collision/terrain** | Terrain speed multiplier, walkability check | Each entity is on a different tile with potentially different terrain. |
| **Death** | `system_death` checks `hunger >= 1.0` | Individual. |
| **State timers** | Gathering timer, farming lease, sleep timer | Timers started at different ticks. A villager who joined the farm 20 ticks late has a different lease. |

### Simulated per-individual BUT using group results

| System | Individual action | Group input used |
|--------|------------------|-----------------|
| **Flee response** | Each member sets `FleeHome` individually (with own home position) | Group threat detection triggers the flee. Individual flee direction varies. |
| **Path following** | Each member walks along shared_path with individual offset | Group pathfinds once. Members offset by `(offset_x, offset_y)` so they walk in a loose formation, not single-file. |
| **Haul destination** | Each member walks to the same stockpile | Group identified the stockpile. Individual hauling uses their own position as origin. |
| **Knowledge update** | On group dissolution, each member receives the group's `shared_knowledge` | Knowledge is aggregated at group level, distributed on split. |

### Performance savings breakdown

For a group of N members at Active priority (interval 2):

| Operation | Without groups | With groups | Savings |
|-----------|---------------|-------------|---------|
| Threat scan | N queries/interval | 1 query/interval | N:1 |
| Pathfinding | N A* calls when destination changes | 1 A* call | N:1 |
| Resource scan | N nearest queries/interval | 1 nearest query/interval | N:1 |
| Decision logic | N evaluations/interval | 1 evaluation/interval | N:1 |
| Movement | N position updates/tick | N position updates/tick | 1:1 (no savings) |
| Hunger | N hunger updates/tick | N hunger updates/tick | 1:1 (no savings) |

For 10 farmers in a group: threat + pathfinding + resource + decision = 4 operations instead of 40. Movement and hunger remain 10. Net: 14 operations instead of 50, a **3.6x reduction** for that cluster.

## Transition Between Grouped and Individual Modes

### Group formation (individual -> grouped)

Happens during the periodic group detection pass (every 8-16 ticks).

1. Detection algorithm identifies a cluster meeting all four criteria.
2. A `Group` struct is created with the members list.
3. Each member entity gets a `GroupMembership` component (or marker).
4. The group's first AI evaluation runs immediately: threat check, path computation, decision.
5. Members' `TickSchedule.next_ai_tick` is set to follow the group's schedule. Individual AI is skipped while grouped.

**Visual effect:** None. Entities don't teleport or snap into formation. They continue at their current positions. The `offset` in `GroupMembership` captures their existing spread relative to the centroid. Formation is invisible to the player.

### Group dissolution (grouped -> individual)

A group dissolves when any dissolution trigger fires. Dissolution is immediate -- it does not wait for the next detection pass.

#### Dissolution triggers

| Trigger | Detection method | Response |
|---------|-----------------|----------|
| **Threat enters group radius** | Group's per-tick threat check finds a predator | All members revert to individual AI. Each evaluates flee independently (different home locations = different flee directions). Group struct is dropped. |
| **Member leaves radius** | Checked during group AI: any member > 1.5x group radius from centroid | That member is removed from the group. If group drops below 3, group dissolves entirely. |
| **Activity change** | A member's state changes (e.g., gathering timer expires, transitions to hauling) | That member leaves the group. Remaining members continue if >= 3. |
| **Member dies** | `system_death` removes entity | Member removed from group. Dissolve if < 3. |
| **Night/day transition** | Global event | Farm crews and gather crews dissolve (workers go home). Sleep groups may form. |
| **Detection pass rebuild** | Every 8-16 ticks, groups are re-detected from scratch | Stale groups that no longer meet criteria are not reformed. Effectively dissolved. |

#### Dissolution process

1. Each member's `GroupMembership` is removed.
2. Each member's `TickSchedule.next_ai_tick` is set to `current_tick + 1` (run AI next tick to make an individual decision).
3. The group's `shared_knowledge` is copied into each member's personal memory (Pillar 2 integration -- once per-villager memory exists).
4. The `Group` struct is dropped.

**Visual effect:** Minimal. On threat dissolution, members scatter naturally because they flee toward different homes. On activity-change dissolution, members are already transitioning to different states. The player sees a group of farmers finish their shift and walk off in different directions -- which looks correct and intentional.

### The seamless principle

The player should never perceive the group/individual transition. Specific measures:

- **No snapping**: Members keep their positions. No teleporting to formation positions.
- **No synchronized animation**: Members don't suddenly start walking in lockstep. The offset system preserves natural spread.
- **No visual indicator**: No "group icon" or selection ring. Groups are a performance optimization, not a gameplay feature.
- **Staggered state transitions**: When a group dissolves due to activity change (e.g., gather timers expire), timers were started at different ticks, so members transition individually over several ticks. This looks like natural staggering, not a synchronized batch.

## Group Pathfinding

The biggest per-group win. Instead of N independent A* calls to the same destination, one call from the group centroid.

### How it works

1. Group AI determines destination (e.g., stockpile at (50, 30)).
2. A* runs from centroid to destination, producing waypoints: `[(40,25), (45,27), (50,30)]`.
3. Each member follows the same waypoints but offset by their `(offset_x, offset_y)`:
   - Member at offset (+2, +1) walks toward (42,26), then (47,28), then (52,31).
   - But clamped to walkable terrain -- if the offset position is water, the member nudges toward the nearest walkable tile on the path.
4. The result: members walk in a **loose formation** that follows the same route. Not single-file, not a rigid grid. A natural-looking cluster moving together.

### Path invalidation

The shared path is recomputed when:
- The destination changes (new stockpile, resource depleted).
- A terrain change blocks the path (building placed, tree cut -- detected by checking path waypoints against current terrain).
- The group centroid drifts more than 5 tiles from the expected path position (members wandered off course).

Path is NOT recomputed every group AI tick. It persists until invalidated. This is the same caching strategy as the path_caching design doc, but at group level.

### Formation spread

The offset system naturally produces spread that looks good:

- Farmers in a field: spread across the field area (offsets of 2-6 tiles). Looks like people working different parts of the field.
- Haulers on a road: spread along the road direction (offsets of 1-3 tiles). Looks like a line of carriers.
- Builders at a site: tight cluster (offsets of 1-2 tiles). Looks like a construction crew.

The offset is computed once at group formation and stays fixed unless the member's position changes significantly (e.g., pushed by collision).

## Group Knowledge Sharing

This is the Pillar 2 integration point. Groups are natural knowledge-sharing units.

### During group lifetime

- Any member's sight-range observations are added to `group.shared_knowledge`.
- If one farmer spots a wolf approaching from the east, the group threat check picks it up and all members flee. Information propagated instantly within the group.
- If an explorer in a scout party discovers a stone deposit, all scout party members "learn" it.

### On group dissolution

- `shared_knowledge` is distributed to all members' personal memory.
- This is the mechanism for "two villagers met and exchanged knowledge" from Pillar 2, but scaled: 10 farmers who worked together all day share a full day's worth of observations when they go home at night.

### At buildings (information hubs)

- When a group arrives at a stockpile, the group's shared knowledge is posted to the bulletin board once (not N times).
- The group picks up the bulletin board's knowledge once and distributes to all members.
- This is N:1 at the stockpile instead of N:N.

## Integration with Existing Systems

### Spatial hash grid

Groups are detected using the spatial grid's cell structure. The grid already buckets entities spatially -- group detection reads those buckets. No additional spatial index needed.

Future: the grid could store group IDs per cell, enabling `grid.groups_in_radius()` queries for inter-group interactions (e.g., two haul columns merging on the same road).

### Tick budgeting

Groups interact with tick budgeting naturally:

- A group has a single `TickPriority` (computed from its activity and threat status).
- All members share this priority and schedule.
- When the group's scheduled tick arrives, one AI evaluation runs for the group, not N.
- The combined savings: tick budgeting reduces how often AI runs (3-5x), grouping reduces how many evaluations per scheduled tick (3-5x per cluster). Multiplicative: **9-25x reduction** for clustered entities.

### system_ai modification

The AI loop in `system_ai` needs a new branch:

```
for entity in entities_with_behavior {
    if has GroupMembership {
        // This entity is in a group.
        // Skip individual AI. Movement continues from group path.
        // Only the group leader (first member) triggers group AI evaluation.
        continue;
    }
    // ... existing individual AI ...
}

// Separate pass: evaluate each group's AI
for group in &mut groups {
    if group.next_ai_tick > current_tick { continue; }
    group_ai_evaluate(group, grid, map, ...);
    // Results applied to all members
}
```

### Rendering

No changes to rendering. `system_render` iterates entities by position as always. Each member has an individual position and is drawn individually. The group abstraction is invisible to the renderer.

The one rendering opportunity: when a group of 8+ entities occupies a small area, the per-tile renderer (from Pillar 5) already handles overlap by showing the "most important" entity per tile. Groups naturally create the "crowded area shows density" behavior without special-casing.

## Data Lifecycle

```
Game struct
  +-- groups: Vec<Group>          // rebuilt every 8-16 ticks
  +-- group_id_counter: u32       // monotonic, wraps at u32::MAX

Entity components (non-serialized):
  +-- GroupMembership { group_id, offset_x, offset_y }

Tick lifecycle:
  1. system_hunger          -- per-individual (unchanged)
  2. system_ai
     a. populate spatial grid
     b. IF detection_tick: detect_groups() -> rebuild groups Vec
     c. evaluate group AI for scheduled groups
     d. evaluate individual AI for ungrouped scheduled entities
     e. dissolution checks (threat, membership changes)
  3. system_movement        -- per-individual (unchanged)
  4. system_death           -- per-individual, removes dead from groups
  5. render                 -- per-individual (unchanged)
```

Groups are NOT serialized. On load, `groups` is empty and `GroupMembership` components don't exist. The next detection pass (within 8-16 ticks) reforms any natural groups. This avoids save/load complexity and guarantees groups are always consistent with entity state.

## Implementation Plan

### Step 1: Group data structure and detection

Add `src/ecs/group.rs` with `Group`, `GroupActivity`, `GroupMembership`, and `detect_groups()`.

Unit tests:
- 3 farmers within radius form a group.
- 2 farmers within radius do NOT form a group (minimum 3).
- Farmers and builders in the same cell form separate groups.
- Entities outside group radius are excluded.
- Groups spanning cell boundaries are merged.
- Non-groupable states (Wander, Idle, FleeHome) produce no groups.
- Detection with empty grid returns no groups.

### Step 2: Group AI evaluation

Add `group_ai_evaluate()` that performs threat check, resource scan, and pathfinding for the group. Apply results to member entities.

Tests:
- Group threat check detects predator near centroid.
- Group pathfind produces waypoints from centroid to destination.
- Members receive offset-adjusted waypoints.
- Group decision (e.g., "return to stockpile") updates all members' BehaviorState.

### Step 3: Integrate into system_ai

Modify the entity loop to skip grouped entities. Add group evaluation pass. Wire up detection frequency (every N ticks).

Tests:
- Grouped entities skip individual AI.
- Ungrouped entities run individual AI as before.
- Mixed population (some grouped, some not) produces correct behavior.
- 30K tick simulation with groups matches baseline settlement metrics (population, resource curves, building count) within statistical tolerance.

### Step 4: Dissolution triggers

Implement all dissolution triggers: threat, radius departure, activity change, death, day/night.

Tests:
- Predator entering group radius dissolves group and members flee.
- Member walking away removes them from group.
- Group dropping below 3 members dissolves.
- Timer-based state change (gathering -> hauling) removes member from group.
- Dead entity is cleaned from group.

### Step 5: Group pathfinding

Implement shared path with member offsets. Path caching and invalidation.

Tests:
- Group path produces valid A* route.
- Member offsets stay on walkable terrain.
- Path recomputes when blocked.
- Members arrive at destination with natural spread (not stacked).

### Step 6: Knowledge sharing integration

Wire group knowledge to per-villager memory system (depends on per_villager_memory design).

### Step 7: Performance benchmarking

Profile at 200, 500, and 1000 entities with and without grouping. Measure:
- AI evaluations per tick (expect 3-5x reduction for clustered populations).
- Frame time at 500 pop (target: under 5ms AI budget).
- Path computations per tick (expect significant reduction).

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Groups make behavior look robotic/synchronized | Offset system + staggered timers prevent lockstep. Members naturally spread over time due to terrain speed differences. If still too uniform, add small random jitter to offsets each detection pass. |
| Group AI makes a decision that's wrong for one member (e.g., "keep farming" but one member is starving) | Hunger interrupt (from tick budgeting) overrides group membership. Member with critical hunger gets `next_ai_tick = now`, runs individual AI, leaves group. |
| Detection pass is expensive at high entity counts | Detection reads the spatial grid's existing buckets -- no new spatial queries. Cost is O(entities in groupable states). At 500 pop, ~300 entities, ~30 cells to check. Under 0.1ms. |
| Group path is suboptimal for outlier members | Offset clamping keeps members on walkable terrain. Worst case: an outlier member's offset path is slightly longer than their optimal individual path. This is invisible at the scale we're operating. |
| Dissolution creates frame-time spikes (20 entities suddenly need individual AI) | Dissolved members get `next_ai_tick = current_tick + random(1..3)`, staggering their first individual evaluation over 3 ticks. |
| Groups interact badly with building auto-build (new farm placed splits a farm crew) | Detection pass naturally handles this: next recomputation sees two spatial clusters and forms two groups, or one expanded group if they overlap. No special case needed. |

## Open Questions

- Should groups have a **leader** (first member, or member closest to centroid) who is the one rendered with a "walking" animation while others follow? This could improve visual coherence for haul columns but might look weird for farm crews.
- Should group maximum size be capped? A 30-villager mega-group saves computation but might mask important individual behavior (e.g., one member stuck on terrain). Consider a cap of 15-20.
- How should groups interact with each other? Two haul columns heading to the same stockpile on the same road could merge. Two farm crews in adjacent fields could share threat detection. Worth exploring but adds complexity.
- Should the detection frequency be adaptive? More frequent when entities are actively moving (early game exploration), less frequent when the settlement is stable (late game farming). Could tie to the tick budgeting system's frame time measurement.

## References

- `src/ecs/systems.rs` -- `system_ai` (entity loop that would gain group branch)
- `src/ecs/ai.rs` -- `ai_villager` (individual AI that groups replace for clustered entities)
- `src/ecs/components.rs` -- `BehaviorState` (determines groupability), `Behavior`
- `docs/design/spatial_hash_grid.md` -- spatial partitioning used for group detection
- `docs/design/tick_budgeting.md` -- tick scheduling that groups compose with
- `docs/design/per_villager_memory.md` -- knowledge system that groups feed into
- `docs/design/path_caching.md` -- individual path caching; group pathfinding extends this pattern
- `docs/game_design.md` -- Pillar 5 Section D (group/flock abstraction mentioned as a Rich-tier feature)
